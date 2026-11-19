package service

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"

	"xone/chain-parser-go/internal/config"
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
