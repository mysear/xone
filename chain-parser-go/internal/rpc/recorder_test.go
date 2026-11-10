package rpc

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestRecorderBeforeStatusEnvelopeAndResultDecode(t *testing.T) {
	for _, mode := range []string{"ok", "http-error", "malformed", "type-error"} {
		t.Run(mode, func(t *testing.T) {
			body := `{"jsonrpc":"2.0","id":1,"result":42}`
			if mode == "malformed" {
				body = "{broken"
			}
			if mode == "type-error" {
				body = `{"jsonrpc":"2.0","id":1,"result":"wrong-type"}`
			}
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if mode == "http-error" {
					w.WriteHeader(503)
				}
				fmt.Fprint(w, body)
			}))
			defer server.Close()
			client := New([]string{server.URL + "/secret?token=private"}, time.Second, 4096)
			out := 7
			called := 0
			client.SetRecorder(func(ctx context.Context, id, method string, params json.RawMessage, response []byte) error {
				called++
				if out != 7 || id != client.Providers()[0].ID || strings.Contains(id, "secret") || method != "example" || string(params) != `["0x1"]` || string(response) != body {
					t.Errorf("capture order/identity: %d %s %s %s %s", out, id, method, params, response)
				}
				return nil
			})
			err := client.Call(context.Background(), "example", []any{"0x1"}, &out)
			if called != 1 || (mode == "ok" && (err != nil || out != 42)) || (mode != "ok" && err == nil) {
				t.Fatal(called, out, err)
			}
		})
	}
}
func TestRecorderFailureCriticalNoFailoverAndChildrenShareLiveRecorder(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x1"}`)
	}))
	defer server.Close()
	client := New([]string{server.URL, server.URL + "/backup"}, time.Second, 4096)
	_, children := client.ProviderClients()
	failed := errors.New("durable archive full")
	client.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error { return failed })
	var out string
	if err := client.Call(context.Background(), "test", nil, &out); !errors.Is(err, ErrRecordingFailed) || !errors.Is(err, failed) || calls.Load() != 1 || out != "" {
		t.Fatal(err, calls.Load(), out)
	}
	if err := children[0].Call(context.Background(), "test", nil, &out); !errors.Is(err, ErrRecordingFailed) || calls.Load() != 2 {
		t.Fatal(err, calls.Load())
	}
	if err := client.VerifyEVM(context.Background(), "evm:1", "g"); !errors.Is(err, ErrRecordingFailed) || calls.Load() != 3 {
		t.Fatal("identity bypassed critical recorder", err, calls.Load())
	}
	children[0].SetRecorder(nil)
	if err := children[0].Call(context.Background(), "test", nil, &out); err != nil || out != "0x1" {
		t.Fatal(err, out)
	}
}
func TestRecorderCannotModifyDecodeBuffer(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":42}`) }))
	defer server.Close()
	client := New([]string{server.URL}, time.Second, 4096)
	client.SetRecorder(func(ctx context.Context, id, method string, params json.RawMessage, response []byte) error {
		for i := range response {
			response[i] = 'x'
		}
		return nil
	})
	var out int
	if err := client.Call(context.Background(), "test", nil, &out); err != nil || out != 42 {
		t.Fatal(out, err)
	}
}

func TestCapabilityProbeNeverSwallowsCriticalRecorderFailure(t *testing.T) {
	for _, failAt := range []int32{1, 2, 3, 5, 7, 9, 10, 11, 12} {
		t.Run(fmt.Sprint(failAt), func(t *testing.T) {
			var calls atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls.Add(1)
				var request struct {
					Method string
					Params []json.RawMessage
				}
				json.NewDecoder(r.Body).Decode(&request)
				result := `"0x1"`
				switch request.Method {
				case "eth_getBlockByNumber":
					var tag string
					json.Unmarshal(request.Params[0], &tag)
					if tag == "0x0" {
						result = `{"number":"0x0","hash":"g","parentHash":"p"}`
					} else {
						result = `{"number":"0x7","hash":"h","parentHash":"p"}`
					}
				case "eth_getBalance":
					var pin map[string]any
					if json.Unmarshal(request.Params[1], &pin) == nil && pin["blockHash"] != "h" {
						fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"error":{"code":-32001}}`)
						return
					}
				}
				fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":%s}`, result)
			}))
			defer server.Close()
			client := New([]string{server.URL, server.URL + "/backup"}, time.Second, 4096)
			var recorded atomic.Int32
			client.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error {
				if recorded.Add(1) == failAt {
					return errors.New("disk full")
				}
				return nil
			})
			_, err := client.ProbeEVM(context.Background(), ProbeRequest{Chain: "evm:1", Genesis: "g", HistoricalHeight: 7, HistoricalHash: "h", Account: "a"})
			if !errors.Is(err, ErrRecordingFailed) || calls.Load() != failAt {
				t.Fatal(err, calls.Load(), failAt)
			}
		})
	}
}
func TestRecordingFailureDoesNotBlameProviderHealth(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":1}`) }))
	defer server.Close()
	client := New([]string{server.URL}, time.Second, 4096)
	client.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error {
		return errors.New("local disk full")
	})
	var out int
	client.Call(context.Background(), "test", nil, &out)
	if observation := client.Providers()[0]; observation.Failures != 0 || observation.Quarantined || observation.LastClass != RecordingFailed {
		t.Fatal(observation)
	}
}
