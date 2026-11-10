package config

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestWebsocketConfigHasExplicitTransportAndRateResourceBounds(t *testing.T) {
	loaded, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	valid := WebsocketConfig{URL: "wss://source.example", DialTimeoutMS: 1000, IdleTimeoutMS: 1000, SessionTimeoutMS: 10000, MaxMessageBytes: 1024, Queue: 4, MaxMessages: 100, MaxConnections: 2, ReconnectBackoffMS: 5, MaxBackoffMS: 50}
	loaded.Config.Chains[0].Websocket = &valid
	if e = loaded.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	changes := map[string]func(*WebsocketConfig){"scheme": func(c *WebsocketConfig) { c.URL = "https://source.example" }, "plaintext-remote": func(c *WebsocketConfig) { c.URL = "ws://source.example" }, "inline-credentials": func(c *WebsocketConfig) { c.URL = "wss://name:secret@source.example" }, "query-credentials": func(c *WebsocketConfig) { c.URL = "wss://source.example?token=secret" }, "dial-zero": func(c *WebsocketConfig) { c.DialTimeoutMS = 0 }, "idle-zero": func(c *WebsocketConfig) { c.IdleTimeoutMS = 0 }, "forever-session": func(c *WebsocketConfig) { c.SessionTimeoutMS = 0 }, "queue": func(c *WebsocketConfig) { c.Queue = 65537 }, "messages": func(c *WebsocketConfig) { c.MaxMessages = 0 }, "connections": func(c *WebsocketConfig) { c.MaxConnections = 65 }, "backoff": func(c *WebsocketConfig) { c.MaxBackoffMS = 1 }}
	for name, change := range changes {
		t.Run(name, func(t *testing.T) {
			bad := valid
			change(&bad)
			loaded.Config.Chains[0].Websocket = &bad
			if loaded.Config.Validate("chain-parser-go") == nil {
				t.Fatal("invalid WS admitted")
			}
		})
	}
	loaded.Config.Chains[0].Websocket = nil
	encoded, e := json.Marshal(loaded.Config.Chains)
	if e != nil || strings.Contains(string(encoded), "\"websocket\"") {
		t.Fatal("absent websocket changed existing serialized config", e)
	}
}
