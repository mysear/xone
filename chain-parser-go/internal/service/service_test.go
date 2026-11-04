package service

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestParserAuthReadinessAndBaseline(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Token = "unit-test-explicit-api-token-12345"
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	h := s.Handler()
	req := httptest.NewRequest("GET", "/internal/chains/evm:31337/baseline", nil)
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != http.StatusUnauthorized {
		t.Fatal(w.Code)
	}
	req = httptest.NewRequest("GET", "/internal/chains/evm:31337/baseline", nil)
	req.Header.Set("Authorization", "Bearer "+cfg.Token)
	w = httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != 200 {
		t.Fatal(w.Code, w.Body.String())
	}
}

func testSnapshotSource(t *testing.T) SnapshotSource {
	t.Helper()
	raw, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	return func(context.Context, config.Chain) (domain.Snapshot, error) {
		var s domain.Snapshot
		e := json.Unmarshal(raw, &s)
		return s, e
	}
}

func TestParserWithoutConfiguredAuthNeedsNoDevelopmentCredential(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, httptest.NewRequest("GET", "/internal/chains/evm:31337/baseline", nil))
	if response.Code != http.StatusOK {
		t.Fatal(response.Code, response.Body.String())
	}
	response = httptest.NewRecorder()
	s.Handler().ServeHTTP(response, httptest.NewRequest("GET", "/health/ready", nil))
	if response.Code != http.StatusOK {
		t.Fatal("parser readiness must use its baseline freshness limit", response.Code, response.Body.String())
	}
}

func TestParserReadinessUsesParserFreshnessWithoutRoutingConfig(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Routing.MaxAge = 0
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, httptest.NewRequest("GET", "/health/ready", nil))
	if response.Code != http.StatusOK {
		t.Fatal("parser readiness depends on omitted router config", response.Code, response.Body.String())
	}
}
