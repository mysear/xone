package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/core/types"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// Gates live in the HTTP provider, so every blocked operation still traverses
// the normal RPC adapter, raw ownership and cancellation paths.
type isolatedRPCGate struct {
	armed        atomic.Bool
	active       atomic.Int64
	maximum      atomic.Int64
	entered      atomic.Int64
	arrivals     chan struct{}
	permits      chan struct{}
	release      chan struct{}
	canceled     chan struct{}
	lateResponse chan struct{}
	releaseOnce  sync.Once
	cancelOnce   sync.Once
	lateOnce     sync.Once
	lateOnCancel bool
}

func newIsolatedRPCGate(armed bool) *isolatedRPCGate {
	g := &isolatedRPCGate{arrivals: make(chan struct{}, 128), permits: make(chan struct{}),
		release: make(chan struct{}), canceled: make(chan struct{}), lateResponse: make(chan struct{})}
	g.armed.Store(armed)
	return g
}

func (g *isolatedRPCGate) unblock() { g.releaseOnce.Do(func() { close(g.release) }) }

func (g *isolatedRPCGate) wait(ctx context.Context) (respond, late bool) {
	if !g.armed.Load() {
		return true, false
	}
	active := g.active.Add(1)
	defer g.active.Add(-1)
	for prior := g.maximum.Load(); active > prior; prior = g.maximum.Load() {
		if g.maximum.CompareAndSwap(prior, active) {
			break
		}
	}
	g.entered.Add(1)
	select {
	case g.arrivals <- struct{}{}:
	default:
	}
	select {
	case <-g.permits:
		return true, false
	case <-g.release:
		return true, false
	case <-ctx.Done():
		g.cancelOnce.Do(func() { close(g.canceled) })
		if g.lateOnCancel {
			// The provider can still attempt a valid response after the client
			// canceled; Close must finish without waiting for that response.
			<-g.release
			return true, true
		}
		return false, false
	}
}

type isolatedRPCNode struct {
	head     atomic.Uint64
	requests atomic.Int64
	gate     *isolatedRPCGate
}

func isolatedRPCChain(t *testing.T, base config.Chain, id uint64, phase string, gate *isolatedRPCGate) (config.Chain, *isolatedRPCNode) {
	t.Helper()
	chain := base
	chain.Key, chain.Genesis = fmt.Sprintf("evm:%d", id), fmt.Sprintf("0x%064x", id)
	chain.Websocket, chain.ProviderProbe, chain.DynamicCatalog, chain.RPCRetry = nil, nil, nil, nil
	chain.Confirmation = "finalized"
	token0, token1 := "0x"+strings.Repeat("1", 40), "0x"+strings.Repeat("2", 40)
	chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2",
		Token0: token0, Token1: token1, FeeNumerator: 3, FeeDenominator: 1000}}
	node := &isolatedRPCNode{gate: gate}
	node.head.Store(100)
	blockHash := func(height uint64) string {
		if height == 0 {
			return fmt.Sprintf("0x%064x", id)
		}
		return fmt.Sprintf("0x%064x", id<<32|height)
	}
	if phase == "provider" {
		chain.ProviderProbe = &config.ProviderProbeConfig{HistoricalHeight: 7, HistoricalHash: blockHash(7),
			Account: chain.Pools[0].ID, TimeoutMS: 60000, IntervalMS: 60000,
			MaxBackoffMS: 60000, MaxAgeMS: 120000, MaxProviders: 1}
	}
	timestamp := time.Now().Unix() - 1000
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		node.requests.Add(1)
		var req struct {
			Method string
			Params []json.RawMessage
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		var tag string
		if req.Method == "eth_getBlockByNumber" && len(req.Params) > 0 {
			_ = json.Unmarshal(req.Params[0], &tag)
		}
		var call struct{ Data string }
		if req.Method == "eth_call" && len(req.Params) > 0 {
			_ = json.Unmarshal(req.Params[0], &call)
		}
		blocked := phase == "identity" && req.Method == "eth_chainId" ||
			phase == "pool" && req.Method == "eth_call" && call.Data == "0x0902f1ac" ||
			phase == "provider" && req.Method == "eth_getBlockByNumber" && tag == "0x7" ||
			phase == "head" && req.Method == "eth_getBlockByNumber" && tag == "finalized"
		late := false
		if gate != nil && blocked {
			respond, canceledLate := gate.wait(r.Context())
			if !respond {
				return
			}
			late = canceledLate
		}
		if late {
			defer gate.lateOnce.Do(func() { close(gate.lateResponse) })
		}
		reply := map[string]any{"jsonrpc": "2.0", "id": 1}
		var result any
		switch req.Method {
		case "eth_chainId":
			result = fmt.Sprintf("0x%x", id)
		case "eth_blockNumber":
			result = fmt.Sprintf("0x%x", node.head.Load())
		case "eth_getBlockByNumber":
			height := node.head.Load()
			if strings.HasPrefix(tag, "0x") {
				var err error
				height, err = strconv.ParseUint(tag[2:], 16, 64)
				if err != nil {
					t.Errorf("invalid block tag %q", tag)
					return
				}
			}
			parent := "0x" + strings.Repeat("0", 64)
			if height > 0 {
				parent = blockHash(height - 1)
			}
			result = map[string]any{"number": fmt.Sprintf("0x%x", height), "hash": blockHash(height),
				"parentHash": parent, "timestamp": fmt.Sprintf("0x%x", timestamp+int64(height)),
				"transactions": []string{}, "transactionsRoot": types.EmptyTxsHash.Hex(),
				"receiptsRoot": types.EmptyReceiptsHash.Hex(), "gasUsed": "0x0"}
		case "eth_call":
			switch call.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + token0[2:]
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + token1[2:]
			case "0x0902f1ac":
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			case "0x":
				result = "0x"
			default:
				t.Errorf("unexpected eth_call selector %q", call.Data)
			}
		case "eth_getBalance":
			result = "0x1"
		case "eth_getCode":
			result = "0x6000"
		case "eth_getStorageAt":
			result = "0x" + strings.Repeat("0", 64)
		case "eth_getBlockReceipts", "eth_getLogs":
			result = []any{}
		default:
			t.Errorf("unexpected normal RPC method %q", req.Method)
		}
		// Provider controls use intentionally unknown block hashes. Reject them
		// instead of letting this fixture falsely certify EIP-1898 capability.
		if len(req.Params) > 0 {
			var selector struct{ BlockHash string }
			if json.Unmarshal(req.Params[len(req.Params)-1], &selector) == nil && selector.BlockHash != "" {
				known := false
				for _, height := range []uint64{0, 7, 100, 101, 102} {
					known = known || selector.BlockHash == blockHash(height)
				}
				if !known {
					reply["error"] = map[string]any{"code": -32000, "message": "unknown control block"}
				}
			}
		}
		if _, failed := reply["error"]; !failed {
			reply["result"] = result
		}
		_ = json.NewEncoder(w).Encode(reply)
	}))
	t.Cleanup(server.Close)
	chain.RPCURLs = []string{server.URL}
	return chain, node
}

type isolatedRPCOperation struct {
	done chan struct{}
	err  error
}

type isolatedRPCRun struct {
	s       *Service
	ctx     context.Context
	cancel  context.CancelFunc
	gates   []*isolatedRPCGate
	started []*isolatedRPCOperation
}

func newIsolatedRPCRun(t *testing.T, cfg *config.Snapshot, gates ...*isolatedRPCGate) *isolatedRPCRun {
	t.Helper()
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	run := &isolatedRPCRun{s: s, ctx: ctx, cancel: cancel, gates: gates}
	t.Cleanup(func() {
		cancel()
		for _, gate := range gates {
			gate.unblock()
		}
		// Release every fixture wait before cleanup, even when an assertion
		// failed against an implementation that still serializes all chains.
		s.Close()
		for _, operation := range run.started {
			select {
			case <-operation.done:
			case <-time.After(3 * time.Second):
				t.Error("canceled service operation failed to settle during cleanup")
			}
		}
	})
	return run
}

func (r *isolatedRPCRun) begin(f func() error) *isolatedRPCOperation {
	op := &isolatedRPCOperation{done: make(chan struct{})}
	r.started = append(r.started, op)
	go func() { defer close(op.done); op.err = f() }()
	return op
}

func isolatedRPCWait(t *testing.T, done <-chan struct{}, reason string) {
	t.Helper()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal(reason)
	}
}

func isolatedRPCArrival(t *testing.T, gate *isolatedRPCGate) {
	t.Helper()
	isolatedRPCWait(t, gate.arrivals, "normal RPC never reached the provider gate")
}

func isolatedRPCBaseline(t *testing.T, s *Service, key string) (int, domain.Snapshot) {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, "/internal/chains/"+key+"/baseline", nil)
	if s.cfg.Token != "" {
		req.Header.Set("Authorization", "Bearer "+s.cfg.Token)
	}
	recorder := httptest.NewRecorder()
	s.Handler().ServeHTTP(recorder, req)
	var snapshot domain.Snapshot
	if recorder.Code == http.StatusOK {
		if err := json.Unmarshal(recorder.Body.Bytes(), &snapshot); err != nil {
			t.Fatal(err)
		}
	}
	return recorder.Code, snapshot
}

func isolatedRPCHeight(t *testing.T, s *Service, key string, height uint64) domain.Snapshot {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		status, snapshot := isolatedRPCBaseline(t, s, key)
		if status == http.StatusOK && snapshot.Reference.Block.Number == height {
			return snapshot
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("%s did not publish healthy height %d while the other chain was blocked", key, height)
	return domain.Snapshot{}
}

func isolatedRPCConfig(t *testing.T, maintenance bool) *config.Snapshot {
	t.Helper()
	cfg := startupIsolationConfig(t)
	cfg.Config.History.Enabled = false
	cfg.Config.API.RPM = 100000
	cfg.Config.Journal.TimeoutMS = 60000 // Gate assertions finish long before source timeout.
	if maintenance {
		cfg.Token = "slow-chain-rpc-maintenance-auth-token"
		cfg.Config.Journal.MaxBytes = 256 << 10
		cfg.Config.Journal.RawEvidence = &config.RawEvidenceConfig{MaxBytes: 64 << 20, MaxRecords: 10000, MaxRecordBytes: 4 << 20}
		cfg.Config.Journal.RawMaintenance = &config.RawMaintenanceConfig{SinkPath: t.TempDir(),
			SinkMaxBytes: 64 << 20, SinkMaxRecords: 10000, IntervalMS: 60000, TimeoutMS: 60000,
			ReplayTimeoutMS: 5000, TargetBytes: 32 << 20, MaxScan: 32, MaxArchive: 4, MaxReclaims: 4,
			MaxRawBytes: 4 << 20, MaxDecodedBytes: 4 << 20, MaxPageRecords: 100,
			MaxRequestBytes: 8192, MaxPinsPerCapture: 64, DecoderVersion: RawStagedDecoderVersion}
	}
	return cfg
}

func TestSlowChainRPCStartupAndHealthyProgressIsolation(t *testing.T) {
	for _, phase := range []string{"identity", "pool", "provider"} {
		for _, slowFirst := range []bool{true, false} {
			t.Run(fmt.Sprintf("%s/slow-first=%v", phase, slowFirst), func(t *testing.T) {
				cfg := isolatedRPCConfig(t, false)
				gate := newIsolatedRPCGate(true)
				slow, _ := isolatedRPCChain(t, cfg.Config.Chains[0], 31401, phase, gate)
				healthy, good := isolatedRPCChain(t, cfg.Config.Chains[0], 31402, "", nil)
				cfg.Config.Chains = []config.Chain{slow, healthy}
				if !slowFirst {
					cfg.Config.Chains = []config.Chain{healthy, slow}
				}
				run := newIsolatedRPCRun(t, cfg, gate)
				start := run.begin(func() error { return run.s.Start(run.ctx) })
				isolatedRPCArrival(t, gate)
				isolatedRPCWait(t, start.done, "healthy Start waited for another chain's blocked RPC")
				if start.err != nil {
					t.Fatal(start.err)
				}
				before := isolatedRPCHeight(t, run.s, healthy.Key, 100)
				for height := uint64(101); height <= 102; height++ {
					good.head.Store(height)
					// Include an explicit overlapping Tick. A busy slow actor may
					// be skipped, while ScheduleTick remains the production wake.
					tick := run.begin(func() error { run.s.Tick(run.ctx); return nil })
					run.s.ScheduleTick(run.ctx)
					after := isolatedRPCHeight(t, run.s, healthy.Key, height)
					isolatedRPCWait(t, tick.done, "explicit Tick was captured by the blocked slow actor")
					if after.Reference.OnlineGeneration != before.Reference.OnlineGeneration ||
						after.Reference.CanonicalEpoch != before.Reference.CanonicalEpoch ||
						after.Reference.WriterEpoch != before.Reference.WriterEpoch ||
						after.Reference.JournalSeq != before.Reference.JournalSeq+height-100 {
						t.Fatal("healthy progress changed its authority or skipped journal commits", before.Reference, after.Reference)
					}
					assertStartupBaselineHTTP(t, run.s, slow.Key, http.StatusServiceUnavailable)
					if gate.active.Load() != 1 || gate.maximum.Load() != 1 {
						t.Fatal("slow chain lost its blocked operation or admitted concurrent same-chain work")
					}
				}
			})
		}
	}
}

func TestScheduleTickCoalescesOverlappingNormalRPCWakes(t *testing.T) {
	cfg := isolatedRPCConfig(t, false)
	gate := newIsolatedRPCGate(false)
	chain, node := isolatedRPCChain(t, cfg.Config.Chains[0], 31403, "head", gate)
	cfg.Config.Chains = []config.Chain{chain}
	run := newIsolatedRPCRun(t, cfg, gate)
	start := run.begin(func() error { return run.s.Start(run.ctx) })
	isolatedRPCWait(t, start.done, "single normal chain failed to start")
	if start.err != nil {
		t.Fatal(start.err)
	}
	before := isolatedRPCHeight(t, run.s, chain.Key, 100)
	gate.armed.Store(true)
	node.head.Store(101)
	run.s.ScheduleTick(run.ctx)
	isolatedRPCArrival(t, gate)
	flood := run.begin(func() error {
		for i := 0; i < 64; i++ {
			run.s.ScheduleTick(run.ctx)
		}
		return nil
	})
	isolatedRPCWait(t, flood.done, "ScheduleTick blocked behind normal RPC")
	if gate.active.Load() != 1 || gate.maximum.Load() != 1 || gate.entered.Load() != 1 {
		t.Fatal("overlapping wakes started concurrent same-chain RPCs")
	}
	select {
	case gate.permits <- struct{}{}:
	case <-time.After(2 * time.Second):
		t.Fatal("first head request did not accept release")
	}
	isolatedRPCArrival(t, gate)
	after := isolatedRPCHeight(t, run.s, chain.Key, 101)
	if after.Reference.JournalSeq != before.Reference.JournalSeq+1 || gate.maximum.Load() != 1 || gate.entered.Load() != 2 {
		t.Fatal("coalesced follow-up overlapped or duplicated a state publication")
	}
	select {
	case gate.permits <- struct{}{}:
	case <-time.After(2 * time.Second):
		t.Fatal("coalesced head request did not accept release")
	}
	select {
	case <-gate.arrivals:
		t.Fatal("64 overlapping wakes queued more than one pending normal RPC operation")
	case <-time.After(150 * time.Millisecond):
	}
	if gate.entered.Load() != 2 || gate.maximum.Load() != 1 {
		t.Fatal("same-chain wake count exceeded one inflight plus one pending")
	}
}

func TestSlowChainCloseCancelsRPCWithoutLatePublish(t *testing.T) {
	for _, maintenance := range []bool{false, true} {
		for _, phase := range []string{"identity", "pool", "provider"} {
			for _, slowFirst := range []bool{true, false} {
				t.Run(fmt.Sprintf("raw-maintenance=%v/%s/slow-first=%v", maintenance, phase, slowFirst), func(t *testing.T) {
					cfg := isolatedRPCConfig(t, maintenance)
					gate := newIsolatedRPCGate(true)
					gate.lateOnCancel = true
					slow, _ := isolatedRPCChain(t, cfg.Config.Chains[0], 31404, phase, gate)
					healthy, _ := isolatedRPCChain(t, cfg.Config.Chains[0], 31405, "", nil)
					cfg.Config.Chains = []config.Chain{slow, healthy}
					if !slowFirst {
						cfg.Config.Chains = []config.Chain{healthy, slow}
					}
					run := newIsolatedRPCRun(t, cfg, gate)
					if (run.s.rawMaintenance != nil) != maintenance {
						t.Fatal("raw maintenance configuration was not applied")
					}
					start := run.begin(func() error { return run.s.Start(run.ctx) })
					isolatedRPCArrival(t, gate)
					isolatedRPCWait(t, start.done, "healthy chain could not complete startup while another source was blocked")
					if start.err != nil {
						t.Fatal(start.err)
					}
					isolatedRPCHeight(t, run.s, healthy.Key, 100)
					before := run.s.stores[slow.Key].DurableRef()
					closed := run.begin(func() error { run.s.Close(); return nil })
					isolatedRPCWait(t, gate.canceled, "Close did not cancel the slow normal RPC request")
					isolatedRPCWait(t, closed.done, "Close waited for a canceled provider's late response")
					gate.unblock()
					isolatedRPCWait(t, gate.lateResponse, "provider did not attempt its late valid RPC response")
					run.s.ScheduleTick(context.Background())
					if _, ready := run.s.stores[slow.Key].Current(); ready ||
						run.s.stores[slow.Key].DurableRef() != before || before.OnlineGeneration != 0 {
						t.Fatal("canceled slow source published after raw-maintenance/service close")
					}
					assertStartupBaselineHTTP(t, run.s, slow.Key, http.StatusServiceUnavailable)
				})
			}
		}
	}
}
