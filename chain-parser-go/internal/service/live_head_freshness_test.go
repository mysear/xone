package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestLiveHeadFreshnessRejectsBeforePoolReadsAndRevokesScope(t *testing.T) {
	for _, mode := range []string{"future", "stale"} {
		t.Run(mode, func(t *testing.T) {
			cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
			if err != nil {
				t.Fatal(err)
			}
			cfg.Config.State.Path = t.TempDir()
			s, err := NewWithSource(cfg, testSnapshotSource(t))
			if err != nil {
				t.Fatal(err)
			}
			defer s.Close()
			if err = s.Start(context.Background()); err != nil {
				t.Fatal(err)
			}
			s.source = nil
			s.cfg.Config.Startup.MaxAge = 60_000
			ch := cfg.Config.Chains[0]
			original, ready := s.stores[ch.Key].Current()
			if !ready {
				t.Fatal("missing baseline")
			}
			timestamp := time.Now().Unix() + 60
			expected := "BASELINE_FUTURE_TIMESTAMP"
			if mode == "stale" {
				timestamp = time.Now().Unix() - 120
				expected = "BASELINE_TOO_OLD"
			}
			var poolReads atomic.Int64
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var q struct {
					ID     json.RawMessage `json:"id"`
					Method string          `json:"method"`
				}
				if err := json.NewDecoder(r.Body).Decode(&q); err != nil {
					t.Error(err)
					return
				}
				response := map[string]any{"jsonrpc": "2.0", "id": q.ID}
				if q.Method == "eth_getBlockByNumber" {
					response["result"] = map[string]any{"number": fmt.Sprintf("0x%x", original.Reference.Block.Number+1), "hash": "0x" + strings.Repeat("a", 64), "parentHash": original.Reference.Block.Hash, "timestamp": fmt.Sprintf("0x%x", timestamp), "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
				} else {
					poolReads.Add(1)
					response["error"] = map[string]any{"code": -32000, "message": "pool reads must not begin for an invalid head clock"}
				}
				json.NewEncoder(w).Encode(response)
			}))
			defer server.Close()
			s.adapters[ch.Key] = &evm.Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: ch}
			if err = s.syncChain(context.Background(), ch); err == nil || err.Error() != expected {
				t.Fatalf("bad live clock admitted or checked too late: %v, poolReads=%d", err, poolReads.Load())
			}
			if poolReads.Load() != 0 {
				t.Fatal("invalid head reached pool reads", poolReads.Load())
			}
			after, ready := s.stores[ch.Key].Current()
			if !ready || after.Reference != original.Reference {
				t.Fatal("failed clock advanced state", after.Reference)
			}
			s.Tick(context.Background())
			if _, ready = s.stores[ch.Key].Current(); ready {
				t.Fatal("invalid live head did not withdraw scope")
			}
			records, err := s.stores[ch.Key].Records(original.Reference.JournalSeq, 10)
			if err != nil || len(records) != 1 || records[0].Kind != "ONLINE_SCOPE_REVOKED" {
				t.Fatal("remote scope not revoked", records, err)
			}
		})
	}
}
