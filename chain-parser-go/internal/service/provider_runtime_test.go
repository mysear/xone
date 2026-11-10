package service

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

func runtimeHash(s string) string { return "0x" + strings.Repeat(s, 64/len(s)) }
func probeConfig() config.ProviderProbeConfig {
	return config.ProviderProbeConfig{HistoricalHeight: 7, HistoricalHash: runtimeHash("b"), Account: "0x" + strings.Repeat("1", 40), TimeoutMS: 500, IntervalMS: 1000, MaxBackoffMS: 4000, MaxAgeMS: 5000, MaxProviders: 4}
}
func providerFixture(t *testing.T, ignore *atomic.Bool, calls *atomic.Int32, hooks ...func(*http.Request, string, []json.RawMessage)) *rpc.Client {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&q)
		for _, hook := range hooks {
			hook(r, q.Method, q.Params)
		}
		if q.Method == "eth_getCode" || q.Method == "eth_call" || q.Method == "eth_getStorageAt" {
			var selector map[string]any
			selectorIndex := 1
			if q.Method == "eth_getStorageAt" {
				selectorIndex = 2
			}
			if json.Unmarshal(q.Params[selectorIndex], &selector) == nil && selector["blockHash"] != runtimeHash("b") && selector["blockHash"] != runtimeHash("c") && !ignore.Load() {
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32001, "message": "unknown block"}})
				return
			}
		}
		var result any
		switch q.Method {
		case "eth_getStorageAt":
			result = "0x" + strings.Repeat("0", 64)
		case "eth_getCode", "eth_call":
			result = "0x"
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
		case "eth_getBalance":
			var selector map[string]any
			if json.Unmarshal(q.Params[1], &selector) == nil && selector["blockHash"] != runtimeHash("b") && selector["blockHash"] != runtimeHash("c") && !ignore.Load() {
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32001, "message": "unknown block"}})
				return
			}
			result = "0x1"
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	return rpc.New([]string{server.URL}, time.Second, 1<<20)
}
func TestProviderRuntimeActualControlsHeadGateAndRefresh(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	client := providerFixture(t, &ignored, &calls)
	runtime, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err != nil {
		t.Fatal(err)
	}
	anchor := ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}
	if runtime.Gate(anchor) == nil {
		t.Fatal("unprobed provider ready")
	}
	if err = runtime.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	state := runtime.Snapshot()
	if len(state.Providers) != 1 || state.Independence != rpc.Unknown || state.Providers[0].Report.HashPinned.Status != rpc.Supported {
		t.Fatalf("%+v", state)
	}
	if err = runtime.Gate(anchor); err != nil {
		t.Fatal(err)
	}
	ignored.Store(true)
	if runtime.Refresh(context.Background()) == nil {
		t.Fatal("refresh ignored probe budget")
	}
	runtime.mu.Lock()
	runtime.next = time.Time{}
	runtime.mu.Unlock()
	if err = runtime.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if runtime.Gate(anchor) == nil {
		t.Fatal("ignored hash selector kept gate open")
	}
}
func TestProviderRuntimeCancellationAndDetachedSnapshot(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	runtime, _ := NewProviderRuntime(providerFixture(t, &ignored, &calls), "evm:1", runtimeHash("a"), probeConfig())
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if runtime.Refresh(ctx) == nil || calls.Load() != 0 {
		t.Fatal("canceled probe performed I/O")
	}
	if err := runtime.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := runtime.Snapshot()
	s.Providers[0].Report.HashPinned.Status = rpc.NotSupported
	if runtime.Snapshot().Providers[0].Report.HashPinned.Status != rpc.Supported {
		t.Fatal("snapshot alias")
	}
}

func TestProviderRuntimeHistoryFailureDoesNotGateCurrentHead(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	client := providerFixture(t, &ignored, &calls)
	cfg := probeConfig()
	cfg.HistoricalHash = runtimeHash("f")
	runtime, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), cfg)
	if err != nil {
		t.Fatal(err)
	}
	if err = runtime.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if runtime.Snapshot().Providers[0].Report.Historical.Status == rpc.Supported {
		t.Fatal("bad historical reference accepted")
	}
	if err = runtime.Gate(ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}); err != nil {
		t.Fatal("history lag blocked current head", err)
	}
}

func TestProviderRuntimeHistoryTimeoutKeepsLiveRefreshSchedule(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	client := providerFixture(t, &ignored, &calls, func(r *http.Request, method string, params []json.RawMessage) {
		if method == "eth_getBlockByNumber" && string(params[0]) == `"0x7"` {
			<-r.Context().Done()
		}
	})
	runtime, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err != nil {
		t.Fatal(err)
	}
	if runtime.Refresh(context.Background()) == nil {
		t.Fatal("history timeout not reported")
	}
	if err = runtime.Gate(ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}); err != nil {
		t.Fatal("history budget exhausted live probe", err)
	}
	if runtime.failures != 0 {
		t.Fatal("archive timeout slowed live polling")
	}
}

func TestProviderRuntimeOptionalObserverOwnsProbeAndAnchorContexts(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	client := providerFixture(t, &ignored, &calls)
	p, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err != nil {
		t.Fatal(err)
	}
	var purposes []ProviderReadPurpose
	type observerKey struct{}
	p.SetObserver(func(ctx context.Context, purpose ProviderReadPurpose, operation func(context.Context) error) error {
		purposes = append(purposes, purpose)
		return operation(context.WithValue(ctx, observerKey{}, true))
	})
	if err = p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err = p.VerifyAnchor(context.Background(), ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}); err != nil {
		t.Fatal(err)
	}
	if len(purposes) != 2 || purposes[0] != ProviderReadProbe || purposes[1] != ProviderReadAnchor {
		t.Fatal("reads escaped observer", purposes)
	}
}
