package evm

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/rpc"
)

type proofBudgetTestTransport func(*http.Request) (*http.Response, error)

func (f proofBudgetTestTransport) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestNativeProofTransportRequiresRequestBudgetBeforeSending(t *testing.T) {
	for _, tc := range []struct {
		name      string
		remaining int
		length    int64
		wantSend  bool
	}{
		{"short", 4, 5, false}, {"exact", 5, 5, true}, {"zero", 0, 5, false}, {"unknown-length", 5, -1, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			sent := 0
			budget := receiptProofBudget{remaining: tc.remaining}
			transport := nativeBudgetTransport{budget: &budget, base: proofBudgetTestTransport(func(r *http.Request) (*http.Response, error) {
				sent++
				return &http.Response{StatusCode: 503, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("")), Request: r}, nil
			})}
			request, err := http.NewRequest(http.MethodPost, "http://native-proof.invalid", strings.NewReader("12345"))
			if err != nil {
				t.Fatal(err)
			}
			request.ContentLength = tc.length
			response, err := transport.RoundTrip(request)
			if response != nil {
				response.Body.Close()
			}
			if tc.wantSend {
				if err != nil || sent != 1 || budget.remaining != 0 {
					t.Fatal("paid request was not charged exactly", err, sent, budget.remaining)
				}
			} else if err == nil || sent != 0 {
				t.Fatal("unpaid request reached transport", err, sent, budget.remaining)
			}
		})
	}
}

func TestNativeProofCallChargesEveryRetryWithoutDoubleChargingFirst(t *testing.T) {
	var attempts, requestBytes, responseBytes, recorded atomic.Int64
	const reply = `{"jsonrpc":"2.0","id":1,"result":"0xa4b1"}`
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		requestBytes.Add(int64(len(raw)))
		if attempts.Add(1) <= 3 {
			w.WriteHeader(503)
			return
		}
		responseBytes.Add(int64(len(reply)))
		io.WriteString(w, reply)
	}))
	defer server.Close()
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "proof-retries")
	reader := &nativeProofReader{ctx: ctx, client: rpc.New([]string{server.URL}, 10*time.Second, 4096), baseMaxBytes: 4096, budget: receiptProofBudget{remaining: 8192}}
	if err := reader.client.SetPolicy(rpc.Policy{MaxAttempts: 8}); err != nil {
		t.Fatal(err)
	}
	reader.client.HTTP.Transport = nativeBudgetTransport{base: reader.client.HTTP.Transport, budget: &reader.budget}
	reader.client.SetRecorder(func(ctx context.Context, _ string, _ string, _ json.RawMessage, _ []byte) error {
		if ctx.Value(nativeDutyKey{}) != "proof-retries" {
			t.Fatal("retry recorder lost duty context")
		}
		recorded.Add(1)
		return nil
	})
	result, err := reader.call("eth_chainId", []any{})
	if err != nil {
		t.Fatal(err)
	}
	want := 128 + int(requestBytes.Load()+responseBytes.Load()) + len(result)
	if attempts.Load() != 4 || recorded.Load() != 4 || 8192-reader.budget.remaining != want {
		t.Fatal("retry payload or first-attempt accounting mismatch", attempts.Load(), recorded.Load(), 8192-reader.budget.remaining, want)
	}
}

func TestNativeProofRetryBudgetStopsBeforeUnpaidSend(t *testing.T) {
	var sent, requestBytes, recorded atomic.Int64
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		sent.Add(1)
		requestBytes.Add(int64(len(raw)))
		w.WriteHeader(503)
	}))
	defer server.Close()
	request, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": []any{}})
	// Pay the logical overhead and two attempts, but be one byte short of a third.
	limit := 128 + 3*len(request) - 1
	reader := &nativeProofReader{ctx: context.Background(), client: rpc.New([]string{server.URL}, 10*time.Second, 4096), baseMaxBytes: 4096, budget: receiptProofBudget{remaining: limit}}
	if err := reader.client.SetPolicy(rpc.Policy{MaxAttempts: 8}); err != nil {
		t.Fatal(err)
	}
	reader.client.HTTP.Transport = nativeBudgetTransport{base: reader.client.HTTP.Transport, budget: &reader.budget}
	reader.client.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error { recorded.Add(1); return nil })
	_, err := reader.call("eth_chainId", []any{})
	if err == nil || !strings.Contains(err.Error(), "RECEIPT_PROOF_BUDGET") || sent.Load() != 2 || recorded.Load() != 2 || requestBytes.Load() != int64(2*len(request)) || reader.budget.remaining != 0 {
		t.Fatal("retry escaped budget before sending", err, sent.Load(), recorded.Load(), requestBytes.Load(), reader.budget.remaining)
	}
}

func TestNativeProofRetryRecorderFailureRemainsCritical(t *testing.T) {
	sentinel := errors.New("proof retry recorder unavailable")
	for _, failAt := range []int64{1, 2} {
		var sent, requestBytes, recorded atomic.Int64
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			raw, _ := io.ReadAll(r.Body)
			requestBytes.Add(int64(len(raw)))
			sent.Add(1)
			w.WriteHeader(503)
		}))
		reader := &nativeProofReader{ctx: context.Background(), client: rpc.New([]string{server.URL}, 10*time.Second, 4096), baseMaxBytes: 4096, budget: receiptProofBudget{remaining: 8192}}
		if err := reader.client.SetPolicy(rpc.Policy{MaxAttempts: 8}); err != nil {
			t.Fatal(err)
		}
		reader.client.HTTP.Transport = nativeBudgetTransport{base: reader.client.HTTP.Transport, budget: &reader.budget}
		reader.client.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error {
			if recorded.Add(1) == failAt {
				return sentinel
			}
			return nil
		})
		_, err := reader.call("eth_chainId", []any{})
		server.Close()
		if !errors.Is(err, rpc.ErrRecordingFailed) || !errors.Is(err, sentinel) || sent.Load() != failAt || recorded.Load() != failAt || 8192-reader.budget.remaining != 128+int(requestBytes.Load()) {
			t.Fatal("failed recorder retried or lost request cost", failAt, err, sent.Load(), recorded.Load(), 8192-reader.budget.remaining, requestBytes.Load())
		}
	}
}
