package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestOwnerRPCRetryOnBoundWitness(t *testing.T) {
	for _, tc := range []struct {
		name      string
		status    int
		succeedAt int32
		policy    bool
		calls     int32
		success   bool
	}{
		{"408-recovers", 408, 2, true, 2, true}, {"429-exhausted", 429, 4, true, 3, false}, {"invalid", 400, 2, true, 1, false}, {"default", 408, 2, false, 1, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var calls atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if calls.Add(1) < tc.succeedAt {
					w.WriteHeader(tc.status)
					return
				}
				fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x1"}`)
			}))
			defer server.Close()
			cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
			if err != nil {
				t.Fatal(err)
			}
			cfg.Config.State.Path = t.TempDir()
			ch := &cfg.Config.Chains[0]
			ch.RPCURLs = []string{server.URL}
			if tc.policy {
				ch.RPCRetry = &config.RPCRetryConfig{MaxAttempts: 3, BaseBackoffMS: 1, MaxBackoffMS: 3}
			}
			service, err := New(cfg)
			if err != nil {
				t.Fatal(err)
			}
			defer service.Close()
			_, clients := service.adapters[ch.Key].Client.ProviderClients()
			var id string
			err = clients[0].Call(context.Background(), "eth_chainId", []any{}, &id)
			if (err == nil) != tc.success || tc.success && id != "0x1" || calls.Load() != tc.calls {
				t.Fatalf("result=%q err=%v calls=%d", id, err, calls.Load())
			}
		})
	}
}

func TestOwnerRPCRateBudgetSharesEndpointQuotaAndReportsUsage(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x1"}`)
	}))
	defer server.Close()
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	ch := &cfg.Config.Chains[0]
	ch.RPCURLs = []string{server.URL}
	ch.RPCRateLimit = &config.RPCRateLimitConfig{RequestsPerSecond: 0.01, Burst: 2}
	service, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer service.Close()
	online := service.adapters[ch.Key].Client
	history := online.WithSeparateReadLimits(time.Second, 4096)
	defer history.HTTP.CloseIdleConnections()
	var recorded atomic.Int32
	history.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error {
		recorded.Add(1)
		return nil
	})
	var id string
	for _, client := range []*rpc.Client{history, online} {
		if err := client.Call(context.Background(), "eth_chainId", []any{}, &id); err != nil || id != "0x1" {
			t.Fatalf("read failed: %s (%v)", id, err)
		}
	}
	_, children := online.ProviderClients()
	err = children[0].VerifyEVM(context.Background(), ch.Key, ch.Genesis)
	if rpc.Classify(err) != rpc.BudgetExceeded || calls.Load() != 2 || recorded.Load() != 1 {
		t.Fatalf("quota or recorder leaked across consumers: %v calls=%d recorded=%d", err, calls.Load(), recorded.Load())
	}
	recorder := httptest.NewRecorder()
	service.Handler().ServeHTTP(recorder, httptest.NewRequest("GET", "/internal/chains/"+ch.Key+"/source-status", nil))
	var status struct {
		Providers []rpc.ProviderObservation `json:"provider_observations"`
	}
	if recorder.Code != http.StatusOK || json.Unmarshal(recorder.Body.Bytes(), &status) != nil || len(status.Providers) != 1 {
		t.Fatalf("source status unavailable: %d %s", recorder.Code, recorder.Body.String())
	}
	obs := status.Providers[0]
	if obs.Quarantined || obs.Attempts != 1 || obs.BudgetRejections != 1 ||
		obs.Burst != 2 || obs.RequestsPerSecond != 0.01 || obs.LatencySamples != 1 ||
		obs.LatencyP50 <= 0 || obs.LatencyP50 != obs.LatencyP95 || obs.LatencyP95 != obs.LatencyP99 {
		t.Fatalf("wrong live provider telemetry: %+v", obs)
	}
}
