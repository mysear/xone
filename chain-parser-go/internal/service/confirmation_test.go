package service

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"
	"xone/chain-parser-go/internal/config"
)

func TestNormalSourceEvidenceDisclosesConfiguredHeadTagWithoutConsensusClaim(t *testing.T) {
	var rows []map[string]any
	for _, tag := range []string{"latest", "safe", "finalized"} {
		t.Run(tag, func(t *testing.T) {
			cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
			if err != nil {
				t.Fatal(err)
			}
			cfg.Config.State.Path = t.TempDir()
			cfg.Config.Chains[0].Confirmation = tag
			s, err := NewWithSource(cfg, testSnapshotSource(t))
			if err != nil {
				t.Fatal(err)
			}
			defer s.Close()
			if err = s.Start(context.Background()); err != nil {
				t.Fatal(err)
			}
			rec := httptest.NewRecorder()
			s.Handler().ServeHTTP(rec, httptest.NewRequest("GET", "/internal/chains/evm:31337/source-evidence", nil))
			var value map[string]any
			if rec.Code != 200 || json.Unmarshal(rec.Body.Bytes(), &value) != nil || value["confirmation_tag"] != tag || value["confirmation_policy"] != "CONFIGURED_PROVIDER_TAG" || value["consensus_verified"] != false {
				t.Fatal(rec.Code, rec.Body.String())
			}
			rows = append(rows, value)
		})
	}
	if out := os.Getenv("XONE_CONFIRMATION_HTTP_OUTPUT"); out != "" {
		if err := os.MkdirAll(out, 0700); err != nil {
			t.Fatal(err)
		}
		raw, _ := json.MarshalIndent(map[string]any{"responses": rows, "scope": "NORMAL_HTTP_HANDLER_CONFIGURED_POLICY_NOT_CONSENSUS_PROOF"}, "", "  ")
		if err := os.WriteFile(filepath.Join(out, "source-confirmation-policy.json"), raw, 0600); err != nil {
			t.Fatal(err)
		}
	}
}

func TestNormalParserRejectsHistoricalConfirmationBeforeCreatingState(t *testing.T) {
	for _, tag := range []string{"0x1", "earliest", "pending", "", "LATEST"} {
		t.Run(tag, func(t *testing.T) {
			cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
			if err != nil {
				t.Fatal(err)
			}
			cfg.Config.State.Path = filepath.Join(t.TempDir(), "must-not-exist")
			cfg.Config.Chains[0].Confirmation = tag
			s, err := New(cfg)
			if s != nil {
				s.Close()
			}
			if err == nil || err.Error() != "CONFIG_CHAIN_CONFIRMATION" || s != nil {
				t.Fatal("invalid head policy constructed service", tag, err)
			}
			if _, err = os.Stat(cfg.Config.State.Path); !os.IsNotExist(err) {
				t.Fatal("bad policy initialized state", err)
			}
		})
	}
}
func TestNormalParserUnsupportedSafeTagDoesNotFallBackOrPublishReady(t *testing.T) {
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Chains[0].Confirmation = "safe"
	var safe, wrong atomic.Int64
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			Method string
			Params []json.RawMessage
		}
		if err := json.NewDecoder(r.Body).Decode(&q); err != nil {
			t.Error(err)
			return
		}
		reply := map[string]any{"jsonrpc": "2.0", "id": 1}
		switch q.Method {
		case "eth_chainId":
			reply["result"] = "0x7a69"
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(q.Params[0], &tag)
			if tag == "0x0" {
				reply["result"] = map[string]any{"hash": cfg.Config.Chains[0].Genesis}
			} else {
				if tag == "safe" {
					safe.Add(1)
				} else {
					wrong.Add(1)
				}
				reply["error"] = map[string]any{"code": -32601, "message": "safe tag unavailable"}
			}
		default:
			t.Error("unexpected source method", q.Method)
			reply["error"] = map[string]any{"code": -32601, "message": "unavailable"}
		}
		json.NewEncoder(w).Encode(reply)
	}))
	defer server.Close()
	cfg.Config.Chains[0].RPCURLs = []string{server.URL}
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err = s.Start(context.Background()); err == nil {
		t.Fatal("unsupported safe tag fabricated baseline")
	}
	if _, ready := s.stores[cfg.Config.Chains[0].Key].Current(); ready || safe.Load() == 0 || wrong.Load() != 0 {
		t.Fatal("confirmation policy silently downgraded", safe.Load(), wrong.Load(), ready)
	}
}
