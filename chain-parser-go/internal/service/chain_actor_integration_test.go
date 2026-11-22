//go:build integration

package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/core/types"
	"github.com/jackc/pgx/v5"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
)

// These synthetic RPC banks test actor/lease/transport isolation, not native
// mainnet provenance. An actual empty receipt root permits complete reads.
func actorIntegrationRPC(t *testing.T, base config.Chain, id uint64, gate *isolatedRPCGate) (config.Chain, *atomic.Uint64, *atomic.Uint64) {
	t.Helper()
	chain := base
	chain.Key, chain.Genesis = fmt.Sprintf("evm:%d", id), fmt.Sprintf("0x%064x", id)
	chain.Websocket, chain.ProviderProbe, chain.DynamicCatalog, chain.RPCRetry = nil, nil, nil, nil
	chain.Confirmation, chain.BlockIntegrity, chain.ReceiptsMode = "finalized", "", "bulk-auto"
	chain.NativeProof, chain.BeaconFinality, chain.OPOrigin = nil, nil, nil
	token0, token1 := "0x"+strings.Repeat("1", 40), "0x"+strings.Repeat("2", 40)
	chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2", Token0: token0, Token1: token1, FeeNumerator: 3, FeeDenominator: 1000}}
	head, requests := new(atomic.Uint64), new(atomic.Uint64)
	head.Store(100)
	hash := func(height uint64) string { return fmt.Sprintf("0x%064x", id<<32|height) }
	timestamp := time.Now().Unix() - 1000
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		var q struct {
			Method string
			Params []json.RawMessage
		}
		if err := json.NewDecoder(r.Body).Decode(&q); err != nil {
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		var tag string
		if q.Method == "eth_getBlockByNumber" && len(q.Params) > 0 {
			_ = json.Unmarshal(q.Params[0], &tag)
		}
		if gate != nil && q.Method == "eth_getBlockByNumber" && tag == "finalized" {
			respond, late := gate.wait(r.Context())
			if !respond {
				return
			}
			if late {
				defer gate.lateOnce.Do(func() { close(gate.lateResponse) })
			}
		}
		var result any
		switch q.Method {
		case "eth_chainId":
			result = fmt.Sprintf("0x%x", id)
		case "eth_getBlockByNumber":
			height := head.Load()
			if strings.HasPrefix(tag, "0x") {
				var err error
				height, err = strconv.ParseUint(tag[2:], 16, 64)
				if err != nil {
					t.Error("invalid actor integration block selector")
					return
				}
			}
			blockHash, parent := hash(height), hash(height-1)
			if height == 0 {
				blockHash, parent = chain.Genesis, "0x"+strings.Repeat("0", 64)
			}
			result = map[string]any{"number": fmt.Sprintf("0x%x", height), "hash": blockHash, "parentHash": parent, "timestamp": fmt.Sprintf("0x%x", timestamp+int64(height)), "transactions": []string{}, "receiptsRoot": types.EmptyReceiptsHash.Hex()}
		case "eth_getBlockReceipts":
			result = []any{}
		case "eth_call":
			var call struct{ Data string }
			_ = json.Unmarshal(q.Params[0], &call)
			switch call.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + token0[2:]
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + token1[2:]
			case "0x0902f1ac":
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			default:
				t.Error("unexpected actor integration state selector")
			}
		default:
			t.Error("unexpected actor integration RPC", q.Method)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	t.Cleanup(server.Close)
	chain.RPCURLs = []string{server.URL}
	return chain, head, requests
}

func actorIntegrationLease(t *testing.T, ctx context.Context, pg *pgx.Conn, chain string) control.Lease {
	t.Helper()
	l := control.Lease{Chain: chain}
	var epoch, expires, generation, canonical, sequence int64
	if err := pg.QueryRow(ctx, `SELECT holder,writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq FROM xone.chain_lease WHERE chain_key=$1`, chain).Scan(&l.Holder, &epoch, &expires, &generation, &canonical, &sequence); err != nil {
		t.Fatal("cannot inspect isolated PG lease", err)
	}
	l.WriterEpoch, l.ExpiresMS, l.Generation, l.CanonicalEpoch, l.JournalSeq = uint64(epoch), uint64(expires), uint64(generation), uint64(canonical), uint64(sequence)
	return l
}

func actorIntegrationWait(t *testing.T, done <-chan struct{}, description string) {
	t.Helper()
	select {
	case <-done:
	case <-time.After(12 * time.Second):
		t.Fatal(description)
	}
}

func actorIntegrationHeight(t *testing.T, s *Service, chain string, height uint64) domain.Snapshot {
	t.Helper()
	deadline := time.Now().Add(12 * time.Second)
	for time.Now().Before(deadline) {
		code, snapshot := isolatedRPCBaseline(t, s, chain)
		if code == http.StatusOK && snapshot.Reference.Block.Number == height {
			return snapshot
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatal("healthy Kafka writer failed to publish while peer RPC remained blocked", chain, height)
	return domain.Snapshot{}
}

func TestChainActorsKeepHealthyKafkaWriterAndLeaseWhilePeerRPCBlocked(t *testing.T) {
	if *startupDSNFile == "" && *startupBroker == "" {
		t.Skip("dedicated PG/Kafka flags not configured")
	}
	if *startupDSNFile == "" || *startupBroker == "" {
		t.Fatal("both --control-test-dsn-file and --kafka-test-broker are required")
	}
	rawDSN, err := os.ReadFile(*startupDSNFile)
	if err != nil {
		t.Fatal("test DSN unavailable")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 75*time.Second)
	defer cancel()
	pg, err := pgx.Connect(ctx, strings.TrimSpace(string(rawDSN)))
	if err != nil {
		t.Fatal("isolated PG connection unavailable")
	}
	t.Cleanup(func() { _ = pg.Close(context.Background()) })
	cfg := startupIsolationConfig(t)
	cfg.Config.History.Enabled = false
	cfg.Config.Journal.Transport, cfg.Config.Control.Backend = "kafka", "postgres"
	cfg.Config.Journal.TimeoutMS = 60000
	cfg.Config.API.RPM = 100000
	cfg.Config.Control.DSNFile, cfg.ControlDSN = *startupDSNFile, strings.TrimSpace(string(rawDSN))
	cfg.Config.Control.InstanceID = fmt.Sprintf("actor-integration-%d", time.Now().UnixNano())
	cfg.Config.Control.LeaseTTLMS, cfg.Config.Control.RenewalMS = 120000, 1000
	cfg.Config.Control.MinCommitMS = 15000
	gate := newIsolatedRPCGate(true)
	gate.lateOnCancel = true
	id := uint64(time.Now().UnixNano()%1000000000) + 10000000
	slow, _, _ := actorIntegrationRPC(t, cfg.Config.Chains[0], id, gate)
	healthy, head, requests := actorIntegrationRPC(t, cfg.Config.Chains[0], id+1, nil)
	cfg.Config.Chains = []config.Chain{slow, healthy}
	t.Cleanup(func() {
		cleanup, finish := context.WithTimeout(context.Background(), 5*time.Second)
		defer finish()
		for _, table := range []string{"control_audit", "chain_lease", "chain_registry"} {
			if _, err := pg.Exec(cleanup, "DELETE FROM xone."+table+" WHERE chain_key=$1 OR chain_key=$2", slow.Key, healthy.Key); err != nil {
				t.Error("isolated actor PG cleanup failed", err)
			}
		}
	})
	badTopic, goodTopic := startupIntegrationTopic(t, ctx), startupIntegrationTopic(t, ctx)
	cfg.Config.Journal.Kafka.Brokers = []string{*startupBroker}
	cfg.Config.Journal.Kafka.Topics = map[string]string{slow.Key: badTopic, healthy.Key: goodTopic}
	s, err := New(cfg)
	if err != nil {
		gate.unblock()
		t.Fatal(err)
	}
	t.Cleanup(func() { cancel(); gate.unblock(); s.Close() })
	started := make(chan struct{})
	var startErr error
	go func() { defer close(started); startErr = s.Start(ctx) }()
	actorIntegrationWait(t, gate.arrivals, "slow normal RPC did not block after acquiring its writer")
	slowLease := actorIntegrationLease(t, ctx, pg, slow.Key)
	actorIntegrationWait(t, started, "healthy Kafka baseline waited for slow peer RPC")
	if startErr != nil {
		t.Fatal("healthy Kafka chain failed startup", startErr)
	}
	baseline := actorIntegrationHeight(t, s, healthy.Key, 100)
	if slowLease.Holder != cfg.Config.Control.InstanceID || slowLease.ExpiresMS <= uint64(time.Now().UnixMilli()) {
		t.Fatal("blocked slow source did not hold a real live PG writer lease")
	}
	for height := uint64(101); height <= 102; height++ {
		head.Store(height)
		s.ScheduleTick(ctx)
		after := actorIntegrationHeight(t, s, healthy.Key, height)
		if after.Reference.JournalSeq != baseline.Reference.JournalSeq+height-100 || after.Reference.WriterEpoch != baseline.Reference.WriterEpoch || after.Reference.OnlineGeneration != baseline.Reference.OnlineGeneration || after.Reference.CanonicalEpoch != baseline.Reference.CanonicalEpoch || gate.active.Load() != 1 {
			t.Fatal("healthy chain lost contiguous authority while slow peer remained blocked")
		}
	}
	last := actorIntegrationHeight(t, s, healthy.Key, 102)
	beforeRenewal := actorIntegrationLease(t, ctx, pg, healthy.Key)
	beforeRequests := requests.Load()
	renewed := beforeRenewal
	deadline := time.Now().Add(4 * time.Second)
	for time.Now().Before(deadline) && renewed.ExpiresMS <= beforeRenewal.ExpiresMS {
		time.Sleep(20 * time.Millisecond)
		renewed = actorIntegrationLease(t, ctx, pg, healthy.Key)
	}
	if renewed.ExpiresMS <= beforeRenewal.ExpiresMS || renewed.WriterEpoch != beforeRenewal.WriterEpoch || requests.Load() != beforeRequests || gate.active.Load() != 1 {
		t.Fatal("idle healthy writer did not independently renew its actual PG expiry")
	}
	t.Logf("healthy PG lease ExpiresMS advanced %d -> %d without source polling", beforeRenewal.ExpiresMS, renewed.ExpiresMS)
	reader, err := journal.OpenReadOnly(ctx, []string{*startupBroker}, goodTopic, cfg.Config.Journal.MaxBytes, cfg.Config.Journal.Kafka.MaxTailRecords)
	if err != nil {
		t.Fatal(err)
	}
	defer reader.Close()
	peer, err := reducer.Open(t.TempDir(), healthy.Owner, healthy.Assignment, healthy.ReorgWindow)
	if err != nil {
		t.Fatal(err)
	}
	defer peer.Close()
	peer.SetAuthorityDeadline(time.UnixMilli(int64(renewed.ExpiresMS)))
	var units []journal.Unit
	cursor := journal.Cursor{Topic: goodTopic, Partition: 0, Offset: 0}
	apply := func(unit journal.Unit, next journal.Cursor) error {
		if err := peer.Prepare(unit); err != nil {
			return err
		}
		if err := peer.ApplyPrepared(next); err != nil {
			return err
		}
		units = append(units, unit)
		cursor = next
		return nil
	}
	if err := reader.Consume(ctx, cursor, apply); err != nil {
		t.Fatal("read_committed Kafka peer could not apply the healthy prefix", err)
	}
	if len(units) != 3 || units[0].Kind != "ONLINE_BASELINE_COMMIT" || units[1].Kind != "BLOCK_COMMIT" || units[2].Kind != "BLOCK_COMMIT" {
		t.Fatal("real Kafka prefix did not contain one baseline and two block commits")
	}
	for index, unit := range units {
		if unit.Snapshot.Reference.Block.Number != 100+uint64(index) || unit.Snapshot.Reference.JournalSeq != baseline.Reference.JournalSeq+uint64(index) {
			t.Fatal("Kafka peer prefix was not contiguous")
		}
	}
	if current, ready := peer.Current(); !ready || current.Reference != last.Reference {
		t.Fatal("independent Kafka peer did not expose the same healthy reference")
	}
	closed := make(chan struct{})
	go func() { defer close(closed); s.Close() }()
	actorIntegrationWait(t, gate.canceled, "Close failed to cancel blocked ordinary RPC")
	actorIntegrationWait(t, closed, "Close waited for the canceled provider's late response")
	gate.unblock()
	actorIntegrationWait(t, gate.lateResponse, "provider did not attempt its late valid response")
	if code, _ := isolatedRPCBaseline(t, s, slow.Key); code != http.StatusServiceUnavailable {
		t.Fatal("slow chain became readable after close")
	}
	if err := reader.Consume(ctx, cursor, apply); err != nil {
		t.Fatal("Kafka peer could not read owner shutdown revocation", err)
	}
	if _, ready := peer.Current(); ready || len(units) != 4 || units[3].Kind != "ONLINE_SCOPE_REVOKED" {
		t.Fatal("owner shutdown did not revoke the independent Kafka peer")
	}
	badReader, err := journal.OpenReadOnly(ctx, []string{*startupBroker}, badTopic, cfg.Config.Journal.MaxBytes, cfg.Config.Journal.Kafka.MaxTailRecords)
	if err != nil {
		t.Fatal(err)
	}
	defer badReader.Close()
	if err := badReader.Consume(ctx, journal.Cursor{Topic: badTopic, Partition: 0}, func(journal.Unit, journal.Cursor) error { return fmt.Errorf("late slow-source Kafka commit") }); err != nil {
		t.Fatal("canceled slow source committed a Kafka unit", err)
	}
	peerControl, err := control.Open(ctx, cfg.ControlDSN)
	if err != nil {
		t.Fatal(err)
	}
	defer peerControl.Close()
	for _, chain := range []config.Chain{slow, healthy} {
		lease, err := peerControl.Acquire(ctx, chain.Key, chain.Owner, chain.Assignment, "actor-integration-close-peer", cfg.Config.Control.LeaseTTLMS)
		if err != nil {
			t.Fatal("Close leaked an unexpired writer lease", chain.Key, err)
		}
		if err := peerControl.Release(ctx, lease); err != nil {
			t.Fatal(err)
		}
	}
}
