package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

type catalogFixtureState struct {
	httpProofEnabled, truncatedReceipts                             atomic.Bool
	proof                                                           factoryHTTPProof
	options                                                         CatalogRuntimeOptions
	codeBad, tokenBad, memberBad, ignoreMethods, proofBad, ownerBad atomic.Bool
	blockReads                                                      atomic.Bool
	entered                                                         chan struct{}
	release                                                         chan struct{}
}

func catalogAddr(n int) string { return fmt.Sprintf("0x%040x", n) }
func catalogFixture(t *testing.T) *catalogFixtureState {
	t.Helper()
	f := &catalogFixtureState{proof: catalogHTTPProofFixture(), entered: make(chan struct{}, 1), release: make(chan struct{})}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&q)
		rpcError := func() {
			_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32001, "message": "missing block"}})
		}
		if q.Method == "eth_getCode" || q.Method == "eth_call" || q.Method == "eth_getBalance" {
			var a map[string]any
			if len(q.Params) > 1 && json.Unmarshal(q.Params[1], &a) == nil {
				h := a["blockHash"]
				if h != runtimeHash("b") && h != runtimeHash("c") && !f.ignoreMethods.Load() {
					rpcError()
					return
				}
			}
		}
		var result any
		switch q.Method {
		case "eth_chainId":
			result = "0x1"
		case "eth_blockNumber":
			result = "0xa"
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(q.Params[0], &tag)
			h := runtimeHash("c")
			n := "0xa"
			if tag == "0x0" {
				h = runtimeHash("a")
				n = "0x0"
			}
			if tag == "0x7" {
				h = runtimeHash("b")
				n = "0x7"
			}
			result = map[string]any{"hash": h, "number": n, "parentHash": runtimeHash("d"), "timestamp": "0x1", "transactions": []string{}, "receiptsRoot": runtimeHash("e")}
			if tag == "0x7" && f.httpProofEnabled.Load() {
				result = f.proof.Header
			}
		case "eth_getTransactionReceipt":
			receipt := f.proof.Receipts[0]
			if f.truncatedReceipts.Load() {
				receipt.Logs = []evm.Log{}
			}
			result = receipt
		case "eth_getBalance":
			result = "0x1"
		case "eth_getCode":
			var address string
			_ = json.Unmarshal(q.Params[0], &address)
			result = "0x6000"
			if address == catalogAddr(20) {
				result = "0x6001"
				if f.codeBad.Load() {
					result = "0x6002"
				}
			}
		case "eth_call":
			var call map[string]string
			_ = json.Unmarshal(q.Params[0], &call)
			if f.blockReads.Load() && call["to"] == catalogAddr(20) {
				select {
				case f.entered <- struct{}{}:
				default:
				}
				select {
				case <-f.release:
				case <-r.Context().Done():
					return
				}
			}
			data := call["data"]
			if len(data) > 10 {
				data = data[:10]
			}
			switch data {
			case "0x574f2ba3":
				result = "0x" + fmt.Sprintf("%064x", 1)
			case "0xe6a43905":
				n := 20
				if f.memberBad.Load() {
					n = 21
				}
				result = "0x" + fmt.Sprintf("%064x", n)
			case "0xc45a0155":
				result = "0x" + fmt.Sprintf("%064x", 10)
			case "0x0dfe1681":
				n := 1
				if f.tokenBad.Load() {
					n = 3
				}
				result = "0x" + fmt.Sprintf("%064x", n)
			case "0xd21220a7":
				result = "0x" + fmt.Sprintf("%064x", 2)
			case "0x0902f1ac":
				result = "0x" + fmt.Sprintf("%064x%064x%064x", 1000, 2000, 1)
			default:
				result = "0x"
			}
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	client := rpc.New([]string{server.URL}, time.Second, 1<<20)
	archive, err := ingest.OpenArchive(filepath.Join(t.TempDir(), "raw"), ingest.ArchiveLimits{MaxBytes: 4 << 20, MaxRecords: 128, MaxRecordBytes: 1 << 20})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = archive.Close() })
	digest := func(s string) string {
		raw, _ := hex.DecodeString(s)
		sum := sha256.Sum256(raw)
		return hex.EncodeToString(sum[:])
	}
	providers, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err != nil {
		t.Fatal(err)
	}
	if err = providers.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	f.options = CatalogRuntimeOptions{Path: filepath.Join(t.TempDir(), "catalog"), Chain: config.Chain{Key: "evm:1", Genesis: runtimeHash("a")}, Config: config.DynamicCatalogConfig{TimeoutMS: 1000, MaxCandidates: 16, MaxPools: 16, MaxBytes: 1 << 20, MaxReceipts: 16, MaxLogs: 32, Sources: []config.FactoryDeployment{{Address: catalogAddr(10), Version: ingest.UniswapV2Factory, ProvenanceID: "public-v2", FactoryCodeSHA256: digest("6000"), PoolCodeSHA256: digest("6001")}}}, Client: client, Providers: providers, Archive: archive,
		CheckFence: func(ctx context.Context, _ CatalogFence) error {
			if f.ownerBad.Load() {
				return errors.New("OWNER_FENCED")
			}
			return ctx.Err()
		},
		VerifyBlock: func(ctx context.Context, b ingest.FactoryBlock) error {
			if f.proofBad.Load() {
				return errors.New("RECEIPT_PROOF_INVALID")
			}
			data, err := archive.Read(b.Raw)
			if err != nil {
				return err
			}
			var original ingest.FactoryBlock
			if err = json.Unmarshal(data, &original); err != nil {
				return err
			}
			original.Raw = b.Raw
			lhs, _ := json.Marshal(original)
			rhs, _ := json.Marshal(b)
			if string(lhs) != string(rhs) {
				return errors.New("RAW_EVENT_UNBOUND")
			}
			return ctx.Err()
		},
	}
	return f
}
func (f *catalogFixtureState) block(t *testing.T) ingest.FactoryBlock {
	t.Helper()
	log := ingest.FactoryLog{Address: catalogAddr(10), BlockHash: runtimeHash("b"), BlockNumber: 7, TransactionHash: runtimeHash("d"), Topics: []string{ingest.V2PairCreatedTopic, "0x" + fmt.Sprintf("%064x", 1), "0x" + fmt.Sprintf("%064x", 2)}, Data: "0x" + fmt.Sprintf("%064x%064x", 20, 1)}
	block := ingest.FactoryBlock{Chain: "evm:1", Anchor: ingest.Cursor{Height: 7, Hash: runtimeHash("b")}, Receipts: []ingest.FactoryReceipt{{BlockHash: log.BlockHash, BlockNumber: 7, TransactionHash: log.TransactionHash, Logs: []ingest.FactoryLog{log}}}}
	raw, _ := json.Marshal(block)
	ref, err := f.options.Archive.Put(ingest.Owner{Chain: "evm:1", Height: 7, Hash: runtimeHash("b"), ProviderID: "public-http"}, raw)
	if err != nil {
		t.Fatal(err)
	}
	block.Raw = ref
	return block
}
func openCatalogFixture(t *testing.T) (*CatalogRuntime, *catalogFixtureState) {
	t.Helper()
	f := catalogFixture(t)
	r, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = r.Close() })
	if err = r.Begin(context.Background(), ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}, CatalogFence{1, 1, 1, 1}); err != nil {
		t.Fatal(err)
	}
	return r, f
}
func TestCatalogRuntimeActualStateImmutableHistoryAndRestart(t *testing.T) {
	r, f := openCatalogFixture(t)
	ctx := context.Background()
	block := f.block(t)
	candidates, err := r.Discover(ctx, ingest.CatalogHistory, block)
	if err != nil {
		t.Fatal(err)
	}
	if err = r.Admit(ctx, ingest.CatalogOnline, candidates); err == nil {
		t.Fatal("history directly entered online")
	}
	if err = r.Admit(ctx, ingest.CatalogHistory, candidates); err != nil {
		t.Fatal(err)
	}
	if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("history changed online readiness")
	}
	candidates, err = r.Discover(ctx, ingest.CatalogOnline, block)
	if err != nil {
		t.Fatal(err)
	}
	if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("event admitted directly")
	}
	if err = r.Admit(ctx, ingest.CatalogOnline, candidates); err != nil {
		t.Fatal(err)
	}
	snapshot := r.CatalogSnapshot(ingest.CatalogOnline)
	if !snapshot.Ready || len(snapshot.Entries) != 1 || snapshot.Entries[0].Pool.Reserve0 != "1000" || snapshot.Entries[0].Admission.StateDigest == "" {
		t.Fatalf("%+v", snapshot)
	}
	snapshot.Entries[0].Config.V2.ExpectedFactory = catalogAddr(99)
	if r.CatalogSnapshot(ingest.CatalogOnline).Entries[0].Config.V2.ExpectedFactory != catalogAddr(10) {
		t.Fatal("snapshot was mutable")
	}
	configs, err := CatalogConfigs(r.CatalogSnapshot(ingest.CatalogOnline), CatalogFence{1, 1, 1, 1}, []string{catalogAddr(20)})
	if err != nil || len(configs) != 1 {
		t.Fatal(err)
	}
	if _, err = CatalogConfigs(r.CatalogSnapshot(ingest.CatalogOnline), CatalogFence{1, 2, 1, 1}, []string{catalogAddr(20)}); err == nil {
		t.Fatal("stale scope fence")
	}
	if err = r.Close(); err != nil {
		t.Fatal(err)
	}
	restored, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Close()
	if restored.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("restart trusted cached reserves")
	}
	if err = restored.Revalidate(ctx, ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 2}, CatalogFence{1, 2, 1, 2}); err != nil {
		t.Fatal(err)
	}
	if !restored.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("restart never published validated state")
	}
}
func TestCatalogRuntimeRejectsAdmissionMismatchesAndRevokesFailures(t *testing.T) {
	for _, mode := range []string{"code", "token", "membership", "owner", "proof", "selector"} {
		t.Run(mode, func(t *testing.T) {
			r, f := openCatalogFixture(t)
			ctx := context.Background()
			c, err := r.Discover(ctx, ingest.CatalogOnline, f.block(t))
			if err != nil {
				t.Fatal(err)
			}
			if err = r.Admit(ctx, ingest.CatalogOnline, c); err != nil {
				t.Fatal(err)
			}
			switch mode {
			case "code":
				f.codeBad.Store(true)
			case "token":
				f.tokenBad.Store(true)
			case "membership":
				f.memberBad.Store(true)
			case "owner":
				f.ownerBad.Store(true)
			case "proof":
				f.proofBad.Store(true)
			case "selector":
				f.ignoreMethods.Store(true)
			}
			// New generation forces a fresh read, even for a previously admitted event.
			err = r.Revalidate(ctx, ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 2}, CatalogFence{1, 2, 1, 2})
			if err == nil {
				t.Fatal("bad evidence accepted")
			}
			if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
				t.Fatal("old admission survived failure")
			}
		})
	}
}
func TestCatalogRuntimeReorgCancelsInflightAndTombstonesEvent(t *testing.T) {
	r, f := openCatalogFixture(t)
	ctx := context.Background()
	block := f.block(t)
	c, err := r.Discover(ctx, ingest.CatalogOnline, block)
	if err != nil {
		t.Fatal(err)
	}
	f.blockReads.Store(true)
	done := make(chan error, 1)
	go func() { done <- r.Admit(ctx, ingest.CatalogOnline, c) }()
	select {
	case <-f.entered:
	case <-time.After(time.Second):
		t.Fatal("read never started")
	}
	if err = r.Revoke(ctx, []string{block.Anchor.Hash}, CatalogFence{1, 2, 2, 2}); err != nil {
		t.Fatal(err)
	}
	close(f.release)
	if err = <-done; err == nil {
		t.Fatal("late read published")
	}
	if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("orphan visible")
	}
	if err = r.Begin(ctx, ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 2}, CatalogFence{1, 2, 2, 2}); err != nil {
		t.Fatal(err)
	}
	if _, err = r.Discover(ctx, ingest.CatalogOnline, block); err == nil {
		t.Fatal("orphan replay admitted")
	}
}

func TestCatalogRuntimeHistoryFailureCannotRevokeOnline(t *testing.T) {
	r, f := openCatalogFixture(t)
	ctx := context.Background()
	b := f.block(t)
	online, err := r.Discover(ctx, ingest.CatalogOnline, b)
	if err != nil {
		t.Fatal(err)
	}
	history, err := r.Discover(ctx, ingest.CatalogHistory, b)
	if err != nil {
		t.Fatal(err)
	}
	if err = r.Admit(ctx, ingest.CatalogOnline, online); err != nil {
		t.Fatal(err)
	}
	f.memberBad.Store(true)
	if r.Admit(ctx, ingest.CatalogHistory, history) == nil {
		t.Fatal("bad history accepted")
	}
	if !r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("history failure revoked online")
	}
}
