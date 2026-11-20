package service

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"sync/atomic"
	"testing"

	"xone/chain-parser-go/internal/config"
)

// Both endpoints replace the tip at the same height. The replacement's pool
// state is initially unavailable, then becomes readable on the next tick.
func TestTickRevokesUnreadableForkAndReanchorsAfterConfirmation(t *testing.T) {
	var forked, readable atomic.Bool
	var witnessReads atomic.Int32
	newHash := runtimeHash("a")
	s, _, _, _ := auditNormalRPCWSConfig(t, func(cfg *config.Snapshot) {
		ch := &cfg.Config.Chains[0]
		upstream, err := url.Parse(ch.RPCURLs[0])
		if err != nil {
			t.Fatal(err)
		}
		ch.RPCURLs = nil
		for i := 0; i < 2; i++ {
			witness := i == 1
			proxy := httputil.NewSingleHostReverseProxy(upstream)
			proxy.ModifyResponse = func(res *http.Response) error {
				if !forked.Load() {
					return nil
				}
				defer res.Body.Close()
				var body map[string]any
				if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
					return err
				}
				if block, ok := body["result"].(map[string]any); ok && block["number"] == "0x64" {
					block["hash"] = newHash
					if witness {
						witnessReads.Add(1)
					}
				} else if _, scalar := body["result"].(string); scalar && !readable.Load() {
					delete(body, "result")
					body["error"] = map[string]any{"code": -32000, "message": "replacement state unavailable"}
				}
				raw, err := json.Marshal(body)
				if err != nil {
					return err
				}
				res.Body = io.NopCloser(bytes.NewReader(raw))
				res.ContentLength = int64(len(raw))
				res.Header.Set("Content-Length", fmt.Sprint(len(raw)))
				return nil
			}
			server := httptest.NewServer(proxy)
			t.Cleanup(server.Close)
			ch.RPCURLs = append(ch.RPCURLs, server.URL)
		}
	})
	s.wsHTTPBudget = false
	key := s.cfg.Config.Chains[0].Key
	store := s.stores[key]
	before, ready := store.Current()
	if !ready {
		t.Fatal("missing initial baseline")
	}
	assertHTTP := func(want int) {
		t.Helper()
		res := httptest.NewRecorder()
		s.Handler().ServeHTTP(res, httptest.NewRequest("GET", "/internal/chains/"+key+"/baseline", nil))
		if res.Code != want {
			t.Fatalf("baseline HTTP %d, want %d: %s", res.Code, want, res.Body.String())
		}
	}
	assertHTTP(http.StatusOK)
	forked.Store(true)
	s.Tick(context.Background())
	assertHTTP(http.StatusServiceUnavailable)
	if _, ready := store.Current(); ready {
		t.Fatal("old tip still readable")
	}
	if _, err := store.Bootstrap(); err == nil {
		t.Fatal("old bootstrap still readable")
	}
	records, err := store.Records(before.Reference.JournalSeq, 3)
	if err != nil || len(records) != 1 || records[0].Kind != "ONLINE_SCOPE_REVOKED" || store.DurableRef().CanonicalEpoch != before.Reference.CanonicalEpoch {
		t.Fatalf("unexpected journal after failed read: %+v, %v", records, err)
	}
	readable.Store(true)
	witnessReads.Store(0)
	s.Tick(context.Background())
	assertHTTP(http.StatusOK)
	after, ready := store.Current()
	if !ready || witnessReads.Load() == 0 || after.Reference.Block.Hash != newHash || after.Reference.OnlineGeneration != before.Reference.OnlineGeneration+1 || after.Reference.CanonicalEpoch != before.Reference.CanonicalEpoch+1 {
		t.Fatalf("replacement not confirmed: %+v", after.Reference)
	}
	records, err = store.Records(before.Reference.JournalSeq+1, 2)
	if err != nil || len(records) != 1 || records[0].Kind != "ONLINE_BASELINE_COMMIT" {
		t.Fatalf("unexpected recovery journal: %+v, %v", records, err)
	}
}
