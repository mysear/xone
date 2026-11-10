package rpc

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gorilla/websocket"
)

func wsLimits() WSLimits {
	return WSLimits{DialTimeout: time.Second, IdleTimeout: 100 * time.Millisecond, SessionTimeout: 3 * time.Second, MaxMessageBytes: 1024, Queue: 8, MaxMessages: 32, MaxConnections: 2, ReconnectBackoff: 20 * time.Millisecond, MaxBackoff: 50 * time.Millisecond}
}
func headWSServer(t *testing.T, serve func(*websocket.Conn, int32)) (*httptest.Server, *atomic.Int32) {
	t.Helper()
	var connections atomic.Int32
	up := websocket.Upgrader{CheckOrigin: func(*http.Request) bool { return true }}
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := up.Upgrade(w, r, nil)
		if err != nil {
			return
		}
		defer conn.Close()
		serve(conn, connections.Add(1))
	}))
	t.Cleanup(s.Close)
	return s, &connections
}
func acceptHeads(conn *websocket.Conn) bool {
	var request struct {
		JSONRPC string
		ID      int
		Method  string
		Params  []string
	}
	if conn.ReadJSON(&request) != nil || request.Method != "eth_subscribe" || len(request.Params) != 1 || request.Params[0] != "newHeads" {
		return false
	}
	return conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "id": request.ID, "result": "sub-1"}) == nil
}
func notifyHead(conn *websocket.Conn, number string) {
	conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "method": "eth_subscription", "params": map[string]any{"subscription": "sub-1", "result": map[string]any{"number": number, "hash": "0x" + strings.Repeat("ab", 32), "parentHash": "0x" + strings.Repeat("cd", 32)}}})
}
func awaitWSDone(t *testing.T, s *HeadStream) error {
	t.Helper()
	select {
	case err := <-s.Done:
		return err
	case <-time.After(4 * time.Second):
		t.Fatal("WS did not stop")
		return nil
	}
}

func TestWSRealHandshakeUntrustedHeadsReconnectAndCancel(t *testing.T) {
	server, count := headWSServer(t, func(conn *websocket.Conn, n int32) {
		if !acceptHeads(conn) {
			return
		}
		notifyHead(conn, "0xffff")
		if n == 1 {
			return
		}
		for {
			if _, _, err := conn.ReadMessage(); err != nil {
				return
			}
		}
	})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stream, err := WatchHeads(ctx, "ws"+strings.TrimPrefix(server.URL, "http")+"/api-key?token=secret", wsLimits())
	if err != nil {
		t.Fatal(err)
	}
	var heads, connected int
	deadline := time.After(2 * time.Second)
	for connected < 2 || heads < 2 {
		select {
		case signal := <-stream.Hints:
			if signal.Kind == WSConnected {
				connected++
			}
			if signal.Kind == WSHeadAvailable {
				heads++
				if signal.Height != 65535 || signal.ProviderID != stream.ProviderID {
					t.Fatal(signal)
				}
			}
		case <-deadline:
			t.Fatal("no reconnect", connected, heads)
		}
	}
	cancel()
	if err := awaitWSDone(t, stream); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	b, _ := json.Marshal(stream.Snapshot())
	if count.Load() != 2 || strings.Contains(string(b), "secret") || strings.Contains(string(b), "api-key") {
		t.Fatal(count.Load(), string(b))
	}
}

func TestWSFiniteReconnectBackoffAndIdle(t *testing.T) {
	server, count := headWSServer(t, func(conn *websocket.Conn, _ int32) {
		if !acceptHeads(conn) {
			return
		}
		for {
			if _, _, err := conn.ReadMessage(); err != nil {
				return
			}
		}
	})
	limits := wsLimits()
	limits.IdleTimeout = 30 * time.Millisecond
	start := time.Now()
	stream, err := WatchHeads(context.Background(), "ws"+strings.TrimPrefix(server.URL, "http"), limits)
	if err != nil {
		t.Fatal(err)
	}
	if err := awaitWSDone(t, stream); !errors.Is(err, ErrWSReconnectBudget) {
		t.Fatal(err)
	}
	if count.Load() != 2 || time.Since(start) < limits.ReconnectBackoff+2*limits.IdleTimeout {
		t.Fatal(count.Load(), time.Since(start))
	}
	if stream.Snapshot().LastCause != "idle" {
		t.Fatal(stream.Snapshot())
	}
}

func TestWSMessageQueueAndCountBudgets(t *testing.T) {
	for _, mode := range []string{"bytes", "queue", "count", "wrong-subscription", "invalid-head"} {
		t.Run(mode, func(t *testing.T) {
			server, count := headWSServer(t, func(conn *websocket.Conn, _ int32) {
				if !acceptHeads(conn) {
					return
				}
				switch mode {
				case "bytes":
					conn.WriteMessage(websocket.TextMessage, []byte(strings.Repeat("x", 2048)))
				case "wrong-subscription":
					conn.WriteMessage(websocket.TextMessage, []byte(`{"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"foreign","result":{}}}`))
				case "invalid-head":
					notifyHead(conn, "0x01")
				default:
					for i := 0; i < 16; i++ {
						notifyHead(conn, "0x1")
					}
				}
				for {
					if _, _, err := conn.ReadMessage(); err != nil {
						return
					}
				}
			})
			limits := wsLimits()
			if mode == "queue" {
				limits.Queue = 1
			}
			if mode == "count" {
				limits.MaxMessages = 1
				limits.Queue = 32
			}
			stream, err := WatchHeads(context.Background(), "ws"+strings.TrimPrefix(server.URL, "http"), limits)
			if err != nil {
				t.Fatal(err)
			}
			err = awaitWSDone(t, stream)
			want := ErrWSProtocol
			switch mode {
			case "bytes":
				want = ErrWSMessageBudget
			case "queue":
				want = ErrWSQueueBudget
			case "count":
				want = ErrWSMessageBudget
			}
			if !errors.Is(err, want) || count.Load() != 1 {
				t.Fatal(err, count.Load())
			}
		})
	}
}

func TestWSCancelDialAndInvalidLimits(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	stream, err := WatchHeads(ctx, "ws://127.0.0.1:1", wsLimits())
	if err == nil {
		err = awaitWSDone(t, stream)
	}
	if !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	limits := wsLimits()
	limits.Queue = 0
	if _, err := WatchHeads(context.Background(), "ws://127.0.0.1:1", limits); err == nil {
		t.Fatal("unbounded queue")
	}
}

func TestWSCancelDuringUpgradeHandshake(t *testing.T) {
	entered := make(chan struct{})
	release := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { close(entered); <-release }))
	defer server.Close()
	defer close(release)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stream, err := WatchHeads(ctx, "ws"+strings.TrimPrefix(server.URL, "http"), wsLimits())
	if err != nil {
		t.Fatal(err)
	}
	<-entered
	start := time.Now()
	cancel()
	err = awaitWSDone(t, stream)
	if !errors.Is(err, context.Canceled) || time.Since(start) > 200*time.Millisecond {
		t.Fatal("cancel delayed by handshake timeout", err, time.Since(start))
	}
}
