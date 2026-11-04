package observability

import (
	"bytes"
	"encoding/json"
	"strings"
	"sync"
	"testing"
	"xone/chain-parser-go/internal/config"
)

type sink struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (s *sink) Write(b []byte) (int, error) { s.mu.Lock(); defer s.mu.Unlock(); return s.b.Write(b) }
func TestRedactionAndSchema(t *testing.T) {
	var output sink
	l := New(&output, "chain-parser-go", "online", "release", "hash", config.Logging{Level: "info", Format: "json", Queue: 16, MaxBytes: 2048})
	l.Event("info", "baseline_committed", map[string]any{"chain_key": "evm:1", "authorization": "bearer-secret", "nested": map[string]any{"password": "dont-show"}})
	l.Close()
	if strings.Contains(output.b.String(), "bearer-secret") || strings.Contains(output.b.String(), "dont-show") {
		t.Fatal("secret leaked")
	}
	var event map[string]any
	if e := json.Unmarshal(bytes.TrimSpace(output.b.Bytes()), &event); e != nil {
		t.Fatal(e)
	}
	for _, k := range []string{"timestamp_ms", "service", "role", "level", "event", "config_release", "config_public_hash"} {
		if _, ok := event[k]; !ok {
			t.Fatal(k)
		}
	}
}

type blockedWriter struct {
	entered chan struct{}
	release chan struct{}
	once    sync.Once
}

func (w *blockedWriter) Write(b []byte) (int, error) {
	w.once.Do(func() { close(w.entered) })
	<-w.release
	return len(b), nil
}
func TestPressureIsBoundedAndDropsVisible(t *testing.T) {
	w := &blockedWriter{entered: make(chan struct{}), release: make(chan struct{})}
	l := New(w, "chain-parser-go", "online", "release", "hash", config.Logging{Level: "info", Format: "json", Queue: 1, MaxBytes: 2048})
	l.Event("info", "first", nil)
	<-w.entered
	for range 100 {
		l.Event("info", "pressure", nil)
	}
	if l.Dropped.Load() == 0 {
		t.Fatal("no visible drops")
	}
	close(w.release)
	l.Close()
}
func TestStorageLoggerUsesConfiguredStructuredStream(t *testing.T) {
	var output sink
	l := New(&output, "chain-parser-go", "online", "r", "h", config.Logging{Level: "debug", Format: "json", Queue: 4, MaxBytes: 2048})
	PebbleLogger{Log: l}.Infof("WAL replay %s", "first\nsecond")
	l.Close()
	var event map[string]any
	if e := json.Unmarshal(bytes.TrimSpace(output.b.Bytes()), &event); e != nil {
		t.Fatal(e)
	}
	if event["event"] != "state_store_event" {
		t.Fatal(event)
	}
}
