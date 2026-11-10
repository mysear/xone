package service

import (
	"context"
	"github.com/gorilla/websocket"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
)

func TestConfiguredWSWakesNormalHTTPActorWithoutTrustingNotification(t *testing.T) {
	upgrade := websocket.Upgrader{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		c, e := upgrade.Upgrade(w, r, nil)
		if e != nil {
			return
		}
		defer c.Close()
		var request any
		if c.ReadJSON(&request) != nil {
			return
		}
		_ = c.WriteJSON(map[string]any{"jsonrpc": "2.0", "id": 1, "result": "sub-1"})
		_ = c.WriteJSON(map[string]any{"jsonrpc": "2.0", "method": "eth_subscription", "params": map[string]any{"subscription": "sub-1", "result": map[string]any{"number": "0xffffff", "hash": "0x" + strings.Repeat("a", 64), "parentHash": "0x" + strings.Repeat("b", 64)}}})
		for {
			if _, _, e = c.ReadMessage(); e != nil {
				return
			}
		}
	}))
	defer server.Close()
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Control.AllowPlaintext = true
	cfg.Config.Chains[0].Websocket = &config.WebsocketConfig{URL: "ws" + strings.TrimPrefix(server.URL, "http"), DialTimeoutMS: 1000, IdleTimeoutMS: 1000, SessionTimeoutMS: 2000, MaxMessageBytes: 4096, Queue: 4, MaxMessages: 10, MaxConnections: 2, ReconnectBackoffMS: 5, MaxBackoffMS: 10}
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	before, _ := s.stores[cfg.Config.Chains[0].Key].Current()
	select {
	case <-s.Wake():
	case <-time.After(time.Second):
		t.Fatal("no wake")
	}
	after, _ := s.stores[cfg.Config.Chains[0].Key].Current()
	if before.Reference != after.Reference {
		t.Fatal("untrusted WS advanced state")
	}
}
