package rpc

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"slices"
	"testing"
	"time"
)

func TestEthCallBatchOrdersAndArchivesExactExchange(t *testing.T) {
	var sent []byte
	reply := []byte(`[{"jsonrpc":"2.0","id":2,"result":"0xbb"},{"jsonrpc":"2.0","id":1,"result":"0xaa"}]`)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		sent, _ = io.ReadAll(req.Body)
		w.Write(reply)
	}))
	defer server.Close()
	c := New([]string{server.URL}, time.Second, 1<<16)
	params := [][]any{{map[string]string{"data": "0x01"}, "0x99"}, {map[string]string{"data": "0x02"}, "0x99"}}
	var archivedRequest, archivedReply []byte
	c.SetRecorder(func(_ context.Context, _ string, method string, args json.RawMessage, raw []byte) error {
		if method != batchArchiveMethod {
			t.Error(method)
		}
		archivedRequest, archivedReply = args, raw
		return nil
	})
	values, err := c.BatchEthCall(context.Background(), params)
	if err != nil || !slices.Equal(values, []string{"0xaa", "0xbb"}) {
		t.Fatal(values, err)
	}
	if !bytes.Equal(archivedRequest, sent) || !bytes.Equal(archivedReply, reply) {
		t.Fatal("wire exchange differs from archive")
	}
	var reqs []struct {
		Method string
		ID     int
		Params json.RawMessage
	}
	if err = json.Unmarshal(sent, &reqs); err != nil || len(reqs) != 2 {
		t.Fatal(err, len(reqs))
	}
	for i, r := range reqs {
		want, _ := json.Marshal(params[i])
		if r.ID != i+1 || r.Method != "eth_call" || !bytes.Equal(r.Params, want) {
			t.Fatal(r)
		}
	}
	c.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error { return errors.New("disk") })
	if result, err := c.BatchEthCall(context.Background(), params); !errors.Is(err, ErrRecordingFailed) || result != nil {
		t.Fatal(result, err)
	}
	c.SetRecorder(nil)
	if err = c.SetPolicy(Policy{RequestsPerSecond: 0.01, Burst: 2}); err != nil {
		t.Fatal(err)
	}
	if _, err = c.BatchEthCall(context.Background(), params); err != nil {
		t.Fatal(err)
	}
	if _, err = c.BatchEthCall(context.Background(), params); Classify(err) != BudgetExceeded {
		t.Fatal(err)
	}
	if _, err = c.BatchEthCall(context.Background(), make([][]any, MaxEthCallBatch+1)); err == nil {
		t.Fatal("oversize batch accepted")
	}
}

func TestEthCallBatchRejectsPartialOrAmbiguousReplies(t *testing.T) {
	const a = `{"jsonrpc":"2.0","id":1,"result":"0xaa"}`
	const b = `{"jsonrpc":"2.0","id":2,"result":"0xbb"}`
	for _, raw := range []string{
		"[]", "[" + a + "]", "[" + a + "," + a + "]", "[" + a + "," + b + "," + b + "]", "[" + a + "," + b + "]{}",
		"[" + a + `,{"jsonrpc":"2.0","id":1,"id":2,"result":"0xbb"}]`,
		"[" + a + `,{"jsonrpc":"2.0","id":2,"ID":2,"result":"0xbb"}]`,
		"[" + a + `,{"jsonrpc":"2.0","id":2,"result":null}]`,
		"[" + a + `,{"jsonrpc":"2.0","id":2,"result":"0xbb","error":{"code":-32000}}]`,
		"[" + a + `,{"jsonrpc":"2.0","id":3,"result":"0xbb"}]`,
		"[" + a + `,{"jsonrpc":"2.0","id":2,"error":{"code":-32000}}]`,
	} {
		if values, err := decodeEthCallBatch([]byte(raw), 2); err == nil || values != nil {
			t.Fatalf("accepted %s: %v", raw, values)
		}
	}
}
