package rpc

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestFixedBlockUnavailableRetry(t *testing.T) {
	hash := "0x" + strings.Repeat("a", 64)
	for _, tc := range []struct {
		name, method, failure string
		params                []any
		policy, succeed       bool
		wantAttempts          int
	}{
		{"numbered body", "eth_getBlockByNumber", "-32000", []any{"0x2a", true}, true, true, 2},
		{"hash receipts", "eth_getBlockReceipts", "-32000", []any{hash}, true, true, 2},
		{"persistent error", "eth_getBlockByNumber", "-32000", []any{"0x2a", true}, true, false, 3},
		{"default policy", "eth_getBlockByNumber", "-32000", []any{"0x2a", true}, false, true, 1},
		{"moving tag", "eth_getBlockByNumber", "-32000", []any{"finalized", true}, true, true, 1},
		{"other unavailable code", "eth_getBlockByNumber", "-32001", []any{"0x2a", true}, true, true, 1},
		{"transaction receipt", "eth_getTransactionReceipt", "-32000", []any{hash}, true, true, 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var calls atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				if calls.Add(1) > 1 && tc.succeed {
					fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"ok"}`)
					return
				}
				fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"error":{"code":%s}}`, tc.failure)
			}))
			defer server.Close()
			client := New([]string{server.URL}, time.Second, 1024)
			if tc.policy {
				if err := client.SetPolicy(Policy{MaxAttempts: 3}); err != nil {
					t.Fatal(err)
				}
			}
			var out string
			err := client.Call(context.Background(), tc.method, tc.params, &out)
			if int(calls.Load()) != tc.wantAttempts {
				t.Fatalf("attempts = %d, want %d", calls.Load(), tc.wantAttempts)
			}
			if tc.succeed && tc.wantAttempts > 1 {
				if err != nil || out != "ok" {
					t.Fatalf("retry result = %q, err = %v", out, err)
				}
			} else if Classify(err) != Unavailable || out != "" {
				t.Fatalf("result = %q, err = %v, want unavailable", out, err)
			}
		})
	}
}

func TestRetryClassificationAndRateBudget(t *testing.T) {
	for _, tc := range []struct {
		wire  string
		class ErrorClass
		tries int
	}{
		{`{"jsonrpc":"2.0","id":1,"error":{"code":-32602}}`, InvalidRequest, 1},
		{`{"jsonrpc":"2.0","id":1,"error":{"code":-32601}}`, Unsupported, 1},
		{`{"jsonrpc":"2.0","id":1,"error":{"code":-32005}}`, RateLimited, 3},
		{`{"jsonrpc":"2.0","id":1,"result":null}`, Unavailable, 1},
	} {
		t.Run(string(tc.class), func(t *testing.T) {
			var calls atomic.Int32
			s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { calls.Add(1); fmt.Fprint(w, tc.wire) }))
			defer s.Close()
			c := New([]string{s.URL, s.URL}, time.Second, 1024)
			if err := c.SetPolicy(Policy{MaxAttempts: 3, BaseBackoff: time.Millisecond, MaxBackoff: 2 * time.Millisecond}); err != nil {
				t.Fatal(err)
			}
			var out any
			err := c.Call(context.Background(), "test", nil, &out)
			if Classify(err) != tc.class || int(calls.Load()) != tc.tries {
				t.Fatalf("err=%v class=%v calls=%d", err, Classify(err), calls.Load())
			}
		})
	}
	var calls atomic.Int32
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":true}`)
	}))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 1024)
	if err := c.SetPolicy(Policy{MaxAttempts: 1, RequestsPerSecond: 1, Burst: 1}); err != nil {
		t.Fatal(err)
	}
	var out bool
	if err := c.Call(context.Background(), "test", nil, &out); err != nil {
		t.Fatal(err)
	}
	_, children := c.ProviderClients()
	if err := children[0].Call(context.Background(), "test", nil, &out); Classify(err) != BudgetExceeded {
		t.Fatalf("child bypassed budget: %v", err)
	}
	if calls.Load() != 1 {
		t.Fatal(calls.Load())
	}
}

func TestCancellationTotalTimeoutAndSecretFreeProviderObservation(t *testing.T) {
	var calls atomic.Int32
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { calls.Add(1); w.WriteHeader(429) }))
	defer s.Close()
	c := New([]string{s.URL + "/private-api-key?token=secret"}, 40*time.Millisecond, 1024)
	c.SetPolicy(Policy{MaxAttempts: 10, BaseBackoff: time.Second, MaxBackoff: time.Second})
	var out any
	start := time.Now()
	err := c.Call(context.Background(), "test", nil, &out)
	if !errors.Is(err, context.DeadlineExceeded) || time.Since(start) > 500*time.Millisecond {
		t.Fatalf("deadline lost: %v", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := c.Call(ctx, "test", nil, &out); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	obs := c.Providers()
	b, _ := json.Marshal(obs)
	if len(obs) != 1 || obs[0].ID == "" || strings.Contains(string(b), "secret") || strings.Contains(string(b), "private-api-key") || obs[0].Attempts != 1 {
		t.Fatal(string(b))
	}
}

func TestProbeCapabilitiesUsesHistoricalHashAndDoesNotInventRange(t *testing.T) {
	var pinned atomic.Bool
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		result := `"0x7"`
		switch req.Method {
		case "eth_chainId":
			result = `"0x1"`
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(req.Params[0], &tag)
			if tag == "0x0" {
				result = `{"number":"0x0","hash":"genesis","parentHash":"zero"}`
			} else if tag == "safe" {
				fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"error":{"code":-32602}}`)
				return
			} else {
				result = `{"number":"0x7","hash":"historical","parentHash":"parent"}`
			}
		case "eth_getBalance":
			var historicalTag string
			if json.Unmarshal(req.Params[1], &historicalTag) == nil {
				fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x0"}`)
				return
			}
			var pin map[string]any
			json.Unmarshal(req.Params[1], &pin)
			if pin["blockHash"] != "historical" {
				fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"error":{"code":-32001}}`)
				return
			}
			pinned.Store(pin["blockHash"] == "historical" && pin["requireCanonical"] == true)
			result = `"0x0"`
		}
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":%s}`, result)
	}))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 4096)
	report, err := c.ProbeEVM(context.Background(), ProbeRequest{Chain: "evm:1", Genesis: "genesis", HistoricalHeight: 7, HistoricalHash: "historical", Account: "0x0000000000000000000000000000000000000000"})
	if err != nil || len(report) != 1 {
		t.Fatal(report, err)
	}
	got := report[0]
	if got.Historical.Status != Supported || got.HashPinned.Status != Supported || got.Safe.Status != NotSupported || !pinned.Load() || got.HistoricalHeight != 7 || got.HistoricalHash != "historical" || got.CheckedAt.IsZero() {
		t.Fatalf("%+v", got)
	}
	report, err = c.ProbeEVM(context.Background(), ProbeRequest{Chain: "evm:1", Genesis: "genesis", HistoricalHeight: 7, HistoricalHash: "other", Account: "a"})
	if err != nil || report[0].Historical.Status == Supported || report[0].HashPinned.Status == Supported {
		t.Fatal(report, err)
	}
}

func TestVerifyIdentityUsesOneTotalDeadlineAndQuarantine(t *testing.T) {
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { io.Copy(io.Discard, r.Body); <-r.Context().Done() }))
	defer slow.Close()
	c := New([]string{slow.URL, slow.URL + "/b", slow.URL + "/c"}, 30*time.Millisecond, 1024)
	start := time.Now()
	err := c.VerifyEVM(context.Background(), "evm:1", "g")
	if err == nil || time.Since(start) > 80*time.Millisecond {
		t.Fatalf("per-provider timeout multiplied: %v %v", err, time.Since(start))
	}
}

func TestExistingProviderChildCannotEscapeLaterWrongChainQuarantine(t *testing.T) {
	var calls atomic.Int32
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x2"}`)
	}))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 4096)
	_, children := c.ProviderClients()
	if err := c.VerifyEVM(context.Background(), "evm:1", "g"); err == nil {
		t.Fatal("identity accepted")
	}
	before := calls.Load()
	var out string
	if err := children[0].Call(context.Background(), "eth_chainId", nil, &out); err == nil || calls.Load() != before {
		t.Fatalf("child escaped quarantine: %v calls=%d", err, calls.Load())
	}
	if len(c.Providers()) != 1 || !c.Providers()[0].Quarantined {
		t.Fatal(c.Providers())
	}
}

func TestForkConfirmationPreservesCancellation(t *testing.T) {
	c := New([]string{"http://127.0.0.1:1"}, time.Second, 1024)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := c.ConfirmEVMBlock(ctx, 1, "h", "p"); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}

func TestProbeDoesNotCallIgnoredHashSelectorSupported(t *testing.T) {
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		result := `"0x1"`
		switch req.Method {
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(req.Params[0], &tag)
			if tag == "0x0" {
				result = `{"number":"0x0","hash":"g","parentHash":"zero"}`
			} else {
				result = `{"number":"0x1","hash":"h","parentHash":"g"}`
			}
		}
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":%s}`, result)
	}))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 4096)
	reports, err := c.ProbeEVM(context.Background(), ProbeRequest{Chain: "evm:1", Genesis: "g", HistoricalHeight: 1, HistoricalHash: "h", Account: "a"})
	if err != nil || reports[0].HashPinned.Status == Supported {
		t.Fatal(reports, err)
	}
}

func TestEVMProbeReservesTimeForTheSecondEndpoint(t *testing.T) {
	for _, stalledMethod := range []string{"eth_chainId", "historical block"} {
		t.Run(stalledMethod, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var req struct {
					Method string
					Params []json.RawMessage
				}
				if json.NewDecoder(r.Body).Decode(&req) != nil {
					t.Error("invalid request")
					return
				}
				var tag string
				if req.Method == "eth_getBlockByNumber" {
					_ = json.Unmarshal(req.Params[0], &tag)
				}
				if r.URL.Path == "/slow" && (req.Method == stalledMethod ||
					stalledMethod == "historical block" && tag == "0x7") {
					<-r.Context().Done()
					return
				}
				result := `"0x1"`
				switch req.Method {
				case "eth_getBlockByNumber":
					if tag == "0x0" {
						result = `{"number":"0x0","hash":"g","parentHash":"p"}`
					} else {
						result = `{"number":"0x7","hash":"h","parentHash":"p"}`
					}
				case "eth_getBalance":
					var pin map[string]any
					if json.Unmarshal(req.Params[1], &pin) == nil && pin["blockHash"] != "h" {
						fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"error":{"code":-32001}}`)
						return
					}
				}
				fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":%s}`, result)
			}))
			defer server.Close()
			slow, healthy := server.URL+"/slow", server.URL+"/healthy"
			client := New([]string{slow, slow, healthy}, time.Second, 4096)
			reports, err := client.ProbeEVM(context.Background(), ProbeRequest{
				Chain: "evm:1", Genesis: "g", HistoricalHeight: 7, HistoricalHash: "h", Account: "a",
			})
			if stalledMethod == "eth_chainId" {
				if err != nil || len(reports) != 1 || !client.Providers()[0].Quarantined {
					t.Fatalf("expected only the verified endpoint: %+v (%v)", reports, err)
				}
			} else {
				if !errors.Is(err, context.DeadlineExceeded) || len(reports) != 2 ||
					reports[0].Historical.Error != Deadline || reports[0].Historical.Status != Unknown {
					t.Fatalf("expected a partial deadline report: %+v (%v)", reports, err)
				}
			}
			report := reports[len(reports)-1]
			if report.ProviderID != endpointID(healthy) || report.Historical.Status != Supported || report.HashPinned.Status != Supported {
				t.Fatalf("healthy endpoint was not fully probed: %+v", report)
			}
		})
	}
}

func TestSuccessfulFailoverReturnsActualProviderID(t *testing.T) {
	bad := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(503) }))
	defer bad.Close()
	good := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"ok"}`) }))
	defer good.Close()
	c := New([]string{bad.URL, good.URL}, time.Second, 4096)
	var out string
	id, err := c.CallWithProvider(context.Background(), "test", nil, &out)
	if err != nil || out != "ok" || id != c.Providers()[1].ID {
		t.Fatal(id, out, err, c.Providers())
	}
}

func TestNetworkAndInternalErrorsClassifyAsTransient(t *testing.T) {
	if got := Classify(errors.New("RPC_ERROR_-32603")); got != Transient {
		t.Fatal(got)
	}
}

func TestProviderAccessDenialLeavesCapabilityUnknownAndAllowsFailover(t *testing.T) {
	for _, code := range []int{http.StatusUnauthorized, http.StatusForbidden} {
		t.Run(strconv.Itoa(code), func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path == "/restricted" {
					w.WriteHeader(code)
					fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"error":{"code":-32602}}`)
					return
				}
				fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x12"}`)
			}))
			defer server.Close()
			restricted := server.URL + "/restricted"
			client := New([]string{restricted, server.URL}, time.Second, 4096)
			var balance string
			_, children := client.ProviderClients()
			params := []string{"0x0123", "0x7"}
			err := children[0].Call(context.Background(), "eth_getBalance", params, &balance)
			if got := capability(err); got.Status != Unknown || got.Error != AccessDenied {
				t.Fatalf("access denial was reported as a capability: %+v (%v)", got, err)
			}
			if id, err := client.CallWithProvider(context.Background(), "eth_getBalance", params, &balance); err != nil || id != endpointID(server.URL) || balance != "0x12" {
				t.Fatalf("access denial blocked fallback: %s %s (%v)", id, balance, err)
			}
		})
	}
}

func TestIdentityMismatchIsObservableAndClassified(t *testing.T) {
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x2"}`) }))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 4096)
	if err := c.VerifyEVM(context.Background(), "evm:1", "g"); Classify(err) != IdentityMismatch {
		t.Fatal(err, Classify(err))
	}
	obs := c.Providers()[0]
	if obs.LastClass != IdentityMismatch || obs.Failures != 1 {
		t.Fatal(obs)
	}
}

func TestNonTransientProviderIsNotRetriedInLaterRounds(t *testing.T) {
	var invalidCalls atomic.Int32
	invalid := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		invalidCalls.Add(1)
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":null}`)
	}))
	defer invalid.Close()
	unavailable := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(503) }))
	defer unavailable.Close()
	c := New([]string{invalid.URL, unavailable.URL}, time.Second, 4096)
	c.SetPolicy(Policy{MaxAttempts: 4, BaseBackoff: time.Millisecond, MaxBackoff: time.Millisecond})
	var out any
	c.Call(context.Background(), "test", nil, &out)
	if invalidCalls.Load() != 1 {
		t.Fatal("retried permanent response", invalidCalls.Load())
	}
}

func TestHistoricalHeaderDoesNotProveHistoricalState(t *testing.T) {
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		result := `"0x1"`
		if req.Method == "eth_getBlockByNumber" {
			var tag string
			json.Unmarshal(req.Params[0], &tag)
			if tag == "0x0" {
				result = `{"number":"0x0","hash":"g","parentHash":"zero"}`
			} else {
				result = `{"number":"0x1","hash":"h","parentHash":"g"}`
			}
		}
		if req.Method == "eth_getBalance" {
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"error":{"code":-32000}}`)
			return
		}
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":%s}`, result)
	}))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 4096)
	reports, err := c.ProbeEVM(context.Background(), ProbeRequest{Chain: "evm:1", Genesis: "g", HistoricalHeight: 1, HistoricalHash: "h", Account: "a"})
	if err != nil || reports[0].Historical.Status == Supported {
		t.Fatal(reports, err)
	}
}

func TestFailoverDoesNotMergePartialDecodedResults(t *testing.T) {
	bad := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":{"Name":"tainted","Count":"invalid"}}`)
	}))
	defer bad.Close()
	good := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":{"Count":1}}`)
	}))
	defer good.Close()
	c := New([]string{bad.URL, good.URL}, time.Second, 4096)
	var out struct {
		Name  string
		Count int
	}
	if err := c.Call(context.Background(), "test", nil, &out); err != nil || out.Name != "" || out.Count != 1 {
		t.Fatal(out, err)
	}
}

func TestNonPositiveTimeoutNeverDisablesTotalBudget(t *testing.T) {
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":true}`) }))
	defer s.Close()
	for _, timeout := range []time.Duration{0, -time.Second} {
		c := New([]string{s.URL}, timeout, 4096)
		var out bool
		if err := c.Call(context.Background(), "test", nil, &out); !errors.Is(err, context.DeadlineExceeded) {
			t.Fatal("total timeout disabled", timeout, err)
		}
	}
}

func TestCallKeepsLegacyMergeSemanticsOnSuccessfulDecode(t *testing.T) {
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":{"Count":1}}`)
	}))
	defer s.Close()
	c := New([]string{s.URL}, time.Second, 4096)
	out := struct {
		Name  string
		Count int
	}{Name: "existing"}
	if err := c.Call(context.Background(), "test", nil, &out); err != nil || out.Name != "existing" || out.Count != 1 {
		t.Fatal(out, err)
	}
}
