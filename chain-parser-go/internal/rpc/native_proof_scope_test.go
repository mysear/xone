package rpc

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

type nativeScopeContextKey struct{}
type nativeScopeCountingTransport struct {
	base  http.RoundTripper
	calls atomic.Int32
}

func (t *nativeScopeCountingTransport) RoundTrip(r *http.Request) (*http.Response, error) {
	t.calls.Add(1)
	return t.base.RoundTrip(r)
}
func TestNativeProofScopedHTTPRetainsFailoverStatePolicyAndLiveRecorder(t *testing.T) {
	var failed, passed atomic.Int32
	first := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		failed.Add(1)
		w.WriteHeader(503)
		w.Write([]byte(`{"jsonrpc":"2.0","id":1,"error":{"code":-32000}}`))
	}))
	defer first.Close()
	second := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		passed.Add(1)
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": "0x1"})
	}))
	defer second.Close()
	c := New([]string{first.URL, second.URL}, time.Second, 4096)
	if err := c.SetPolicy(Policy{MaxAttempts: 2}); err != nil {
		t.Fatal(err)
	}
	originalHTTP := c.HTTP
	transport := &nativeScopeCountingTransport{base: c.HTTP.Transport}
	scopeHTTP := &http.Client{Transport: transport, Timeout: c.HTTP.Timeout, CheckRedirect: c.HTTP.CheckRedirect, Jar: c.HTTP.Jar}
	child := c.CopyWithHTTP(scopeHTTP)
	if child == c || child.HTTP != scopeHTTP || c.HTTP != originalHTTP || len(child.URLs) != 2 || child.getPolicy() != c.getPolicy() {
		t.Fatal("scoped HTTP replaced shared source configuration")
	}
	child.URLs[0] = "https://invalid.test"
	if c.URLs[0] != first.URL {
		t.Fatal("URL slice was shared")
	}
	child.URLs[0] = first.URL
	child.quarantined["local-copy-only"] = true
	if c.quarantined["local-copy-only"] {
		t.Fatal("quarantine map was shared under different locks")
	}
	var recorded atomic.Int32
	c.SetRecorder(func(ctx context.Context, _ string, _ string, _ json.RawMessage, _ []byte) error {
		if ctx.Value(nativeScopeContextKey{}) != "scope" {
			t.Fatal("responsibility context lost")
		}
		recorded.Add(1)
		return nil
	})
	ctx := context.WithValue(context.Background(), nativeScopeContextKey{}, "scope")
	var result string
	if err := child.Call(ctx, "eth_call", []any{}, &result); err != nil || result != "0x1" {
		t.Fatal("scoped source lost failover", result, err)
	}
	if failed.Load() != 1 || passed.Load() != 1 || transport.calls.Load() != 2 || recorded.Load() != 2 {
		t.Fatal("failed attempts/recorder bypassed", failed.Load(), passed.Load(), transport.calls.Load(), recorded.Load())
	}
	if c.state(first.URL) != child.state(first.URL) || c.state(second.URL) != child.state(second.URL) {
		t.Fatal("provider observation state detached")
	}
	// Revocation after cloning remains visible to the scoped source.
	state := c.state(first.URL)
	state.mu.Lock()
	state.obs.Quarantined = true
	state.mu.Unlock()
	if err := child.Call(ctx, "eth_call", []any{}, &result); err != nil {
		t.Fatal(err)
	}
	if failed.Load() != 1 || passed.Load() != 2 || recorded.Load() != 3 {
		t.Fatal("scoped source revived quarantined endpoint")
	}
}
