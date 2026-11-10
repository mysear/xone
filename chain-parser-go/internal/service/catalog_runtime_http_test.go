package service

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

func catalogHTTPProofFixture() factoryHTTPProof {
	data, _ := hex.DecodeString(fmt.Sprintf("%064x%064x", 20, 1))
	topics := []common.Hash{common.HexToHash(ingest.V2PairCreatedTopic), common.HexToHash(fmt.Sprintf("0x%064x", 1)), common.HexToHash(fmt.Sprintf("0x%064x", 2))}
	log := &types.Log{Address: common.HexToAddress(catalogAddr(10)), Topics: topics, Data: data}
	receipt := &types.Receipt{Type: 0, Status: 1, CumulativeGasUsed: 21000, Logs: []*types.Log{log}}
	receipt.Bloom = types.CreateBloom(receipt)
	root := types.DeriveSha(types.Receipts{receipt}, trie.NewStackTrie(nil)).Hex()
	return factoryHTTPProof{Header: evm.RawBlock{Number: "0x7", Hash: runtimeHash("b"), ParentHash: runtimeHash("d"), Timestamp: "0x1", Transactions: []string{runtimeHash("d")}, ReceiptsRoot: root}, Receipts: []evm.Receipt{{BlockHash: runtimeHash("b"), TxHash: runtimeHash("d"), TxIndex: "0x0", Type: "0x0", Status: "0x1", CumulativeGasUsed: "0x5208", LogsBloom: "0x" + hex.EncodeToString(receipt.Bloom[:]), Logs: []evm.Log{{BlockHash: runtimeHash("b"), TxHash: runtimeHash("d"), TxIndex: "0x0", Index: "0x0", Address: catalogAddr(10), Topics: []string{topics[0].Hex(), topics[1].Hex(), topics[2].Hex()}, Data: "0x" + hex.EncodeToString(data)}}}}}
}
func TestCatalogRuntimeHTTPReceiptProofCollectionAndRestart(t *testing.T) {
	f := catalogFixture(t)
	f.httpProofEnabled.Store(true)
	f.options.VerifyBlock = nil
	r, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	ctx := context.Background()
	a := ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}
	fence := CatalogFence{1, 1, 1, 1}
	if err = r.Begin(ctx, a, fence); err != nil {
		t.Fatal(err)
	}
	candidates, err := r.DiscoverHeight(ctx, ingest.CatalogOnline, 7)
	if err != nil || len(candidates) != 1 {
		t.Fatal(candidates, err)
	}
	if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("receipt proof bypassed current admission")
	}
	if err = r.Admit(ctx, ingest.CatalogOnline, candidates); err != nil {
		t.Fatal(err)
	}
	if err = r.Close(); err != nil {
		t.Fatal(err)
	}
	restored, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Close()
	a.Generation = 2
	fence.Generation = 2
	fence.WriterEpoch = 2
	if err = restored.Revalidate(ctx, a, fence); err != nil {
		t.Fatal("default proof restart", err)
	}
	if !restored.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("restart not admitted")
	}
}
func TestCatalogRuntimeHTTPRejectsTruncatedReceipts(t *testing.T) {
	f := catalogFixture(t)
	f.httpProofEnabled.Store(true)
	f.truncatedReceipts.Store(true)
	f.options.VerifyBlock = nil
	r, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	ctx := context.Background()
	if err = r.Begin(ctx, ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}, CatalogFence{1, 1, 1, 1}); err != nil {
		t.Fatal(err)
	}
	if _, err = r.DiscoverHeight(ctx, ingest.CatalogOnline, 7); err == nil {
		t.Fatal("HTTP 200 truncated logs accepted")
	}
	if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("incomplete block visible")
	}
}

func catalogNativeHTTPProof(t *testing.T) factoryHTTPProof {
	t.Helper()
	p := catalogHTTPProofFixture()
	p.Receipts[0].Type = "0x7e"
	l := p.Receipts[0].Logs[0]
	data, _ := hex.DecodeString(l.Data[2:])
	topics := make([]common.Hash, len(l.Topics))
	for i, v := range l.Topics {
		topics[i] = common.HexToHash(v)
	}
	log := &types.Log{Address: common.HexToAddress(l.Address), Topics: topics, Data: data}
	r := &types.Receipt{Status: 1, CumulativeGasUsed: 21000, Logs: []*types.Log{log}}
	r.Bloom = types.CreateBloom(r)
	payload, err := rlp.EncodeToBytes([]any{[]byte{1}, uint64(21000), r.Bloom, r.Logs})
	if err != nil {
		t.Fatal(err)
	}
	p.Header.ReceiptsRoot = types.DeriveSha(runtimeEncodedReceipts{append([]byte{0x7e}, payload...)}, trie.NewStackTrie(nil)).Hex()
	return p
}
func nativeCatalogHTTP(t *testing.T, mode string) (*CatalogRuntime, *atomic.Int32, *atomic.Int32, *atomic.Bool, *atomic.Bool) {
	t.Helper()
	f := catalogFixture(t)
	proof := catalogNativeHTTPProof(t)
	bulk, perTx := &atomic.Int32{}, &atomic.Int32{}
	truncated, wrong := &atomic.Bool{}, &atomic.Bool{}
	origin, _ := url.Parse(f.options.Client.URLs[0])
	proxy := httputil.NewSingleHostReverseProxy(origin)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		r.Body = io.NopCloser(bytes.NewReader(raw))
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.Unmarshal(raw, &q)
		var result any
		handled := true
		switch q.Method {
		case "eth_chainId":
			result = "0x2105"
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(q.Params[0], &tag)
			if tag == "0x7" {
				h := proof.Header
				if wrong.Load() {
					h.Hash = runtimeHash("f")
				}
				result = h
			} else {
				handled = false
			}
		case "eth_getBlockReceipts":
			bulk.Add(1)
			if truncated.Load() {
				result = []evm.Receipt{}
			} else {
				result = proof.Receipts
			}
		case "eth_getTransactionReceipt":
			perTx.Add(1)
			result = proof.Receipts[0]
		default:
			handled = false
		}
		if handled {
			_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
			return
		}
		proxy.ServeHTTP(w, r)
	}))
	t.Cleanup(server.Close)
	options := f.options
	options.Chain.Key = "evm:8453"
	options.Chain.ReceiptsMode = mode
	options.VerifyBlock = nil
	options.Client = rpc.New([]string{server.URL}, time.Second, 1<<20)
	p, err := NewProviderRuntime(options.Client, options.Chain.Key, options.Chain.Genesis, probeConfig())
	if err != nil {
		t.Fatal(err)
	}
	if err = p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	options.Providers = p
	c, err := OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = c.Close() })
	if err = c.Begin(context.Background(), ingest.CatalogAnchor{Chain: options.Chain.Key, Height: 10, Hash: runtimeHash("c"), Generation: 1}, CatalogFence{1, 1, 1, 1}); err != nil {
		t.Fatal(err)
	}
	return c, bulk, perTx, truncated, wrong
}
func TestCatalogRuntimeHTTPNativeChainAndReceiptMode(t *testing.T) {
	for _, mode := range []string{"", evm.ReceiptsModeBulkAuto} {
		t.Run("mode="+mode, func(t *testing.T) {
			c, bulk, perTx, _, wrong := nativeCatalogHTTP(t, mode)
			ctx := context.Background()
			candidates, err := c.DiscoverHeight(ctx, ingest.CatalogOnline, 7)
			if err != nil || len(candidates) != 1 {
				t.Fatal("Base native proof failed", candidates, err)
			}
			if mode == "" && (bulk.Load() != 0 || perTx.Load() != 1) {
				t.Fatal("legacy mode changed", bulk.Load(), perTx.Load())
			}
			if mode == evm.ReceiptsModeBulkAuto && (bulk.Load() != 1 || perTx.Load() != 0) {
				t.Fatal("explicit bulk mode unused", bulk.Load(), perTx.Load())
			}
			if err = c.Admit(ctx, ingest.CatalogOnline, candidates); err != nil {
				t.Fatal(err)
			}
			if !c.CatalogSnapshot(ingest.CatalogOnline).Ready {
				t.Fatal("native proof never admitted")
			}
			wrong.Store(true)
			if _, err = c.DiscoverHeight(ctx, ingest.CatalogOnline, 7); !errors.Is(err, ingest.ErrFactoryStale) {
				t.Fatal("archived proof lost canonical header binding", err)
			}
		})
	}
}
func TestCatalogRuntimeHTTPBulkRejectsPartialAndKeepsBudgets(t *testing.T) {
	c, bulk, perTx, truncated, _ := nativeCatalogHTTP(t, evm.ReceiptsModeBulkAuto)
	truncated.Store(true)
	if _, err := c.DiscoverHeight(context.Background(), ingest.CatalogOnline, 7); err == nil {
		t.Fatal("partial bulk accepted")
	}
	if bulk.Load() != 1 || perTx.Load() != 0 {
		t.Fatal("invalid bulk silently fell back", bulk.Load(), perTx.Load())
	}
	truncated.Store(false)
	c.options.Config.MaxBytes = 1024
	if _, err := c.DiscoverHeight(context.Background(), ingest.CatalogOnline, 7); err == nil {
		t.Fatal("aggregate receipt/proof byte budget ignored")
	}
	if c.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("failed proof published")
	}
}

func TestCatalogRuntimeHTTPOutsideConfiguredCreationRangeDoesNotLeakProofPins(t *testing.T) {
	f := catalogFixture(t)
	f.httpProofEnabled.Store(true)
	f.options.VerifyBlock = nil
	f.options.Config.Sources[0].Since = 8
	r, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	if err = r.Begin(context.Background(), ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}, CatalogFence{1, 1, 1, 1}); err != nil {
		t.Fatal(err)
	}
	candidates, err := r.DiscoverHeight(context.Background(), ingest.CatalogOnline, 7)
	if err != nil || len(candidates) != 0 {
		t.Fatal(candidates, err)
	}
	records, err := f.options.Archive.RawRecords()
	if err != nil || len(records) != 0 {
		t.Fatal("out-of-scope creation retained an unowned proof pin", records, err)
	}
}

func TestCatalogRuntimeHTTPProjectionPreservesValidAnonymousLogTopics(t *testing.T) {
	r, f := openCatalogFixture(t)
	proof := catalogHTTPProofFixture()
	anonymous := proof.Receipts[0].Logs[0]
	anonymous.Address = catalogAddr(99)
	anonymous.Index = "0x1"
	anonymous.Topics = []string{}
	anonymous.Data = "0x"
	proof.Receipts[0].Logs = append(proof.Receipts[0].Logs, anonymous)
	block, err := r.projectHTTPProof(proof)
	if err != nil {
		t.Fatal(err)
	}
	raw, err := json.Marshal(block)
	if err != nil {
		t.Fatal(err)
	}
	block.Raw, err = f.options.Archive.Put(ingest.Owner{Chain: block.Chain, Height: block.Anchor.Height, Hash: block.Anchor.Hash, ProviderID: "anonymous-public-http"}, raw)
	if err != nil {
		t.Fatal(err)
	}
	found, err := r.Discover(context.Background(), ingest.CatalogOnline, block)
	if err != nil || len(found) != 1 {
		t.Fatal("valid anonymous log blocked complete receipt projection", found, err)
	}
}
