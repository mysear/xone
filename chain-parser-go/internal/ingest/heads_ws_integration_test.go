package ingest

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	"xone/chain-parser-go/internal/rpc"
)

func TestRealWSFakeHeadReconnectUsesRecordedHTTPAndPipeline(t *testing.T) {
	p, a := fixture(t)
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	var tip atomic.Uint64
	tip.Store(2)
	var httpCalls, records, commits, connections atomic.Int32
	hash := func(h uint64) string { return fmt.Sprintf("0x%064x", h+1) }
	type wireHead struct {
		Number     string `json:"number"`
		Hash       string `json:"hash"`
		ParentHash string `json:"parentHash"`
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		httpCalls.Add(1)
		var request struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&request)
		if request.Method == "eth_chainId" {
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x1"}`)
			return
		}
		var tag string
		json.Unmarshal(request.Params[0], &tag)
		h := tip.Load()
		if tag != "latest" {
			var err error
			h, err = strconv.ParseUint(strings.TrimPrefix(tag, "0x"), 16, 64)
			if err != nil || h > 3 {
				t.Error("WS hint reached HTTP fact selection", tag)
				w.WriteHeader(400)
				return
			}
		}
		parent := hash(0)
		if h > 0 {
			parent = hash(h - 1)
		}
		response, _ := json.Marshal(wireHead{Number: fmt.Sprintf("0x%x", h), Hash: hash(h), ParentHash: parent})
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":%s}`, response)
	}))
	defer server.Close()
	client := rpc.New([]string{server.URL}, time.Second, 4096)
	client.SetRecorder(func(ctx context.Context, id, method string, params json.RawMessage, response []byte) error {
		n := records.Add(1)
		_, err := a.Put(Owner{Chain: "rpc:evm:1", Height: uint64(n), Hash: method, ProviderID: id}, response)
		return err
	})
	if err := client.VerifyEVM(ctx, "evm:1", hash(0)); err != nil {
		t.Fatal(err)
	}
	read := func(ctx context.Context, tag string) (HTTPHead, []byte, error) {
		var h wireHead
		err := client.Call(ctx, "eth_getBlockByNumber", []any{tag, false}, &h)
		if err != nil {
			return HTTPHead{}, nil, err
		}
		height, err := strconv.ParseUint(strings.TrimPrefix(h.Number, "0x"), 16, 64)
		raw, _ := json.Marshal(h)
		return HTTPHead{Chain: "evm:1", Height: height, Hash: h.Hash, ParentHash: h.ParentHash}, raw, err
	}
	p.Fetch = func(ctx context.Context, h uint64, max int64) (Fetched, error) {
		head, raw, err := read(ctx, fmt.Sprintf("0x%x", h))
		return Fetched{Height: h, Hash: head.Hash, ParentHash: head.ParentHash, ProviderID: client.Providers()[0].ID, Raw: raw}, err
	}
	p.Validate = func(ctx context.Context, b Fetched, decoded []byte) error {
		if b.Hash != hash(b.Height) || b.ParentHash != hash(b.Height-1) {
			return errors.New("HTTP block validation failed")
		}
		return nil
	}
	committedTwo := make(chan struct{})
	reconnected := make(chan struct{})
	p.Commit = func(ctx context.Context, expected Cursor, b Block) error {
		if expected.Hash != b.ParentHash {
			return errors.New("parent mismatch")
		}
		commits.Add(1)
		if b.Height == 2 {
			close(committedTwo)
		}
		return nil
	}
	up := websocket.Upgrader{CheckOrigin: func(*http.Request) bool { return true }}
	wsServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := up.Upgrade(w, r, nil)
		if err != nil {
			return
		}
		defer conn.Close()
		n := connections.Add(1)
		var request any
		if conn.ReadJSON(&request) != nil {
			return
		}
		conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "id": 1, "result": "sub"})
		conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "method": "eth_subscription", "params": map[string]any{"subscription": "sub", "result": map[string]any{"number": "0xffff", "hash": headHash("ff"), "parentHash": headHash("ee")}}})
		if n == 1 {
			select {
			case <-committedTwo:
				tip.Store(3)
				return
			case <-ctx.Done():
				return
			}
		}
		close(reconnected)
		<-ctx.Done()
	}))
	defer wsServer.Close()
	stream, err := rpc.WatchHeads(ctx, "ws"+strings.TrimPrefix(wsServer.URL, "http"), rpc.WSLimits{DialTimeout: time.Second, IdleTimeout: time.Second, SessionTimeout: 2 * time.Second, MaxMessageBytes: 1024, Queue: 8, MaxMessages: 8, MaxConnections: 2, ReconnectBackoff: 20 * time.Millisecond, MaxBackoff: 30 * time.Millisecond})
	if err != nil {
		t.Fatal(err)
	}
	tracker := HeadTracker{Chain: "evm:1", Pipeline: p, Limits: HeadLimits{TailWindow: 3, Timeout: 2 * time.Second, PollInterval: 25 * time.Millisecond, MinCheckInterval: 10 * time.Millisecond, MaxChecks: 20},
		ReadHead: func(ctx context.Context) (HTTPHead, error) { h, _, e := read(ctx, "latest"); return h, e }, ReadHeader: func(ctx context.Context, n uint64) (HTTPHead, error) {
			h, _, e := read(ctx, fmt.Sprintf("0x%x", n))
			return h, e
		},
		VerifyHead: func(ctx context.Context, h HTTPHead) error {
			if h.Hash != hash(h.Height) {
				return errors.New("untrusted HTTP header")
			}
			return nil
		},
	}
	result, err := tracker.Follow(ctx, Cursor{Height: 1, Hash: hash(1)}, stream.Hints, func(ctx context.Context, result HeadRecovery) error {
		if result.Cursor.Height == 3 {
			select {
			case <-reconnected:
				cancel()
			default:
			}
		}
		return nil
	})
	if !errors.Is(err, context.Canceled) || result.Cursor.Height != 3 || commits.Load() != 2 || connections.Load() != 2 || records.Load() != httpCalls.Load() {
		t.Fatal(result, err, commits.Load(), connections.Load(), records.Load(), httpCalls.Load())
	}
	select {
	case <-stream.Done:
	case <-time.After(time.Second):
		t.Fatal("WS cleanup leaked")
	}
	t.Logf("real WS connections=%d, HTTP responses durably recorded=%d, ordered block commits=%d", connections.Load(), records.Load(), commits.Load())
}
