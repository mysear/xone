package service

import (
	"context"
	"encoding/json"
	"fmt"
	"github.com/gorilla/websocket"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

// Read-only audit fixture: real New -> Start -> WatchHeads -> Wake -> Tick -> HTTP.
func auditNormalRPCWS(t *testing.T) (*Service, *atomic.Int64, *atomic.Int64, chan<- struct{}) {
	return auditNormalRPCWSConfig(t, nil)
}
func auditNormalRPCWSConfig(t *testing.T, adjust func(*config.Snapshot)) (*Service, *atomic.Int64, *atomic.Int64, chan<- struct{}) {
	t.Helper()
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Journal.PollMS = 1000
	cfg.Config.Journal.RawEvidence = &config.RawEvidenceConfig{MaxBytes: 128 * 1024 * 1024, MaxRecords: 10000, MaxRecordBytes: int64(cfg.Config.Journal.MaxBytes)*4 + 16384}
	cfg.Config.Control.AllowPlaintext = true
	height := &atomic.Int64{}
	height.Store(100)
	calls := &atomic.Int64{}
	bankTimestamp := time.Now().Unix()
	token0, token1 := "0x"+strings.Repeat("1", 40), "0x"+strings.Repeat("2", 40)
	rpcServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		var req struct {
			Method string
			Params []json.RawMessage
		}
		if json.NewDecoder(r.Body).Decode(&req) != nil {
			return
		}
		var result any
		switch req.Method {
		case "eth_chainId":
			result = "0x7a69"
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(req.Params[0], &tag)
			h := height.Load()
			hash := fmt.Sprintf("0x%064x", h+1000)
			if tag == "0x0" {
				h = 0
				hash = cfg.Config.Chains[0].Genesis
			}
			result = map[string]any{"number": fmt.Sprintf("0x%x", h), "hash": hash, "parentHash": fmt.Sprintf("0x%064x", h+999), "timestamp": fmt.Sprintf("0x%x", bankTimestamp), "transactions": []string{}}
		case "eth_call":
			var p struct{ Data string }
			_ = json.Unmarshal(req.Params[0], &p)
			switch p.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + token0[2:]
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + token1[2:]
			case "0x0902f1ac":
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			default:
				t.Errorf("unexpected selector %s", p.Data)
				return
			}
		default:
			result = "0x1"
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	t.Cleanup(rpcServer.Close)
	events := make(chan struct{})
	upgrade := websocket.Upgrader{}
	wsServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, e := upgrade.Upgrade(w, r, nil)
		if e != nil {
			return
		}
		defer conn.Close()
		var request any
		if conn.ReadJSON(&request) != nil {
			return
		}
		_ = conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "id": 1, "result": "s"})
		for range events {
			e = conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "method": "eth_subscription", "params": map[string]any{"subscription": "s", "result": map[string]any{"number": "0xffffff", "hash": "0x" + strings.Repeat("a", 64), "parentHash": "0x" + strings.Repeat("b", 64)}}})
			if e != nil {
				return
			}
		}
	}))
	t.Cleanup(wsServer.Close)
	t.Cleanup(func() { close(events) })
	chain := &cfg.Config.Chains[0]
	chain.RPCURLs = []string{rpcServer.URL}
	chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2", Token0: token0, Token1: token1, FeeNumerator: 3, FeeDenominator: 1000}}
	chain.Websocket = &config.WebsocketConfig{URL: "ws" + strings.TrimPrefix(wsServer.URL, "http"), DialTimeoutMS: 1000, IdleTimeoutMS: 1000, SessionTimeoutMS: 10000, MaxMessageBytes: 4096, Queue: 4, MaxMessages: 100, MaxConnections: 2, ReconnectBackoffMS: 5, MaxBackoffMS: 10}
	if adjust != nil {
		adjust(cfg)
	}
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if err = s.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	select {
	case <-s.Wake():
	case <-time.After(time.Second):
		t.Fatal("no connect wake")
	}
	return s, height, calls, events
}
func TestAuditNormalWSHasHTTPRateBudget(t *testing.T) {
	s, _, calls, events := auditNormalRPCWS(t)
	before := calls.Load()
	started := time.Now()
	for i := 0; i < 20; i++ {
		events <- struct{}{}
		select {
		case <-s.Wake():
			s.Tick(context.Background())
		case <-time.After(time.Second):
			t.Fatal("no hint")
		}
	}
	elapsed := time.Since(started)
	got := calls.Load() - before
	current, ready := s.stores[s.cfg.Config.Chains[0].Key].Current()
	t.Logf("poll=%dms elapsed=%s HTTP=%d ready=%v head=%d", s.cfg.Config.Journal.PollMS, elapsed, got, ready, current.Reference.Block.Number)
	if elapsed < time.Second && got > 2 {
		t.Fatalf("WS bypassed HTTP polling budget: %d HTTP calls before one poll interval", got)
	}
}

func TestAuditNormalArchiveBudgetRevokesLocalReady(t *testing.T) {
	s, _, calls, _ := auditNormalRPCWS(t)
	chain := s.cfg.Config.Chains[0]
	old := s.rawSources[chain.Key]
	if err := old.archive.Close(); err != nil {
		t.Fatal(err)
	}
	archive, err := ingest.OpenArchive(t.TempDir(), ingest.ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 1, MaxRecordBytes: 10000})
	if err != nil {
		t.Fatal(err)
	}
	if _, err = archive.Put(ingest.Owner{Chain: chain.Key, Hash: "fill", ProviderID: "audit"}, []byte("full")); err != nil {
		t.Fatal(err)
	}
	tracker := newRawRecorder(chain.Key, archive)
	s.rawSources[chain.Key] = tracker
	s.adapters[chain.Key].Client.SetRecorder(tracker.capture)
	before := calls.Load()
	s.Tick(context.Background())
	if _, ready := s.stores[chain.Key].Current(); ready {
		t.Fatal("archive budget left normal service ready")
	}
	if got := calls.Load() - before; got != 1 {
		t.Fatalf("expected one critical response attempt, got %d", got)
	}
}
func TestAuditNormalCanceledTickRevokesLocalReady(t *testing.T) {
	s, _, calls, _ := auditNormalRPCWS(t)
	before := calls.Load()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	s.Tick(ctx)
	if _, ready := s.stores[s.cfg.Config.Chains[0].Key].Current(); ready {
		t.Fatal("canceled Tick left local ready")
	}
	if got := calls.Load() - before; got != 0 {
		t.Fatalf("canceled Tick made %d HTTP calls", got)
	}
}

func TestRawArchiveBudgetMustNotReuseKafkaTailRetention(t *testing.T) {
	s, _, _, _ := auditNormalRPCWSConfig(t, func(cfg *config.Snapshot) { cfg.Config.Journal.Kafka.MaxTailRecords = 3 })
	if _, ready := s.stores[s.cfg.Config.Chains[0].Key].Current(); !ready {
		t.Fatal("unrelated Kafka tail limit broke source baseline")
	}
}

func TestRawArchiveIsExplicitAndLegacyConfigurationDoesNotPretendCapture(t *testing.T) {
	s, _, _, _ := auditNormalRPCWSConfig(t, func(cfg *config.Snapshot) {
		cfg.Config.Journal.RawEvidence = nil
		cfg.Config.Journal.Kafka.MaxTailRecords = 3
	})
	if len(s.rawSources) != 0 {
		t.Fatal("unconfigured raw archive silently enabled")
	}
	if _, ready := s.stores[s.cfg.Config.Chains[0].Key].Current(); !ready {
		t.Fatal("legacy baseline regressed")
	}
}
