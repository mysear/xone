package observability

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"xone/chain-parser-go/internal/config"
)

type Logger struct {
	queue                        chan []byte
	done                         chan struct{}
	writer                       io.Writer
	service, role, release, hash string
	cfg                          config.Logging
	mu                           sync.RWMutex
	closed                       bool
	Dropped                      atomic.Uint64
	WriteErrors                  atomic.Uint64
}

func New(w io.Writer, service, role, release, hash string, cfg config.Logging) *Logger {
	l := &Logger{queue: make(chan []byte, cfg.Queue), done: make(chan struct{}), writer: w, service: service, role: role, release: release, hash: hash, cfg: cfg}
	go func() {
		defer close(l.done)
		for b := range l.queue {
			if _, e := l.writer.Write(b); e != nil {
				l.WriteErrors.Add(1)
			}
		}
	}()
	return l
}
func Redact(v any) any {
	switch value := v.(type) {
	case map[string]any:
		out := map[string]any{}
		for k, v := range value {
			key := strings.ToLower(k)
			sensitive := false
			for _, part := range []string{"authorization", "password", "secret", "token", "private_key", "dsn", "rpc_url"} {
				if strings.Contains(key, part) {
					sensitive = true
					break
				}
			}
			if sensitive {
				out[k] = "[REDACTED]"
			} else {
				out[k] = Redact(v)
			}
		}
		return out
	case []any:
		out := make([]any, len(value))
		for i, v := range value {
			out[i] = Redact(v)
		}
		return out
	default:
		return value
	}
}
func rank(s string) int {
	switch s {
	case "debug":
		return 0
	case "info":
		return 1
	case "warn":
		return 2
	default:
		return 3
	}
}
func (l *Logger) Event(level, event string, fields map[string]any) {
	if rank(level) < rank(l.cfg.Level) {
		return
	}
	record := map[string]any{"timestamp_ms": time.Now().UnixMilli(), "service": l.service, "role": l.role, "level": level, "event": event, "config_release": l.release, "config_public_hash": l.hash, "fields": Redact(fields)}
	b, e := json.Marshal(record)
	if e != nil || len(b) > l.cfg.MaxBytes {
		l.Dropped.Add(1)
		return
	}
	b = append(b, '\n')
	l.mu.RLock()
	defer l.mu.RUnlock()
	if l.closed {
		l.Dropped.Add(1)
		return
	}
	select {
	case l.queue <- b:
	default:
		l.Dropped.Add(1)
	}
}
func (l *Logger) Close() {
	l.mu.Lock()
	if l.closed {
		l.mu.Unlock()
		return
	}
	l.closed = true
	close(l.queue)
	l.mu.Unlock()
	<-l.done
}

// PebbleLogger adapts storage diagnostics to the same configured event stream.
type PebbleLogger struct{ Log *Logger }

func (p PebbleLogger) Infof(format string, args ...interface{}) {
	p.Log.Event("debug", "state_store_event", map[string]any{"message": fmt.Sprintf(format, args...)})
}
func (p PebbleLogger) Fatalf(_ string, _ ...interface{}) {
	event := map[string]any{"timestamp_ms": time.Now().UnixMilli(), "service": p.Log.service, "role": p.Log.role, "level": "error", "event": "state_store_fatal", "config_release": p.Log.release, "config_public_hash": p.Log.hash, "fields": map[string]any{"error_code": "STATE_STORE_FATAL"}}
	raw, _ := json.Marshal(event)
	_, _ = os.Stderr.Write(append(raw, '\n'))
	os.Exit(1)
}
