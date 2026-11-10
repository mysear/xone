package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

type budgetRawReader func(context.Context, string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error)

func (f budgetRawReader) ReadComplete(ctx context.Context, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	return f(ctx, tag)
}

func reopenBudgetMaintenance(t *testing.T, s *Service, m *RawMaintenance, key string) *RawMaintenance {
	t.Helper()
	cfg := m.cfg
	a := s.rawSources[key].archive
	path, limits := a.Directory(), a.Limits()
	if e := m.Close(); e != nil {
		t.Fatal(e)
	}
	if e := a.Close(); e != nil {
		t.Fatal(e)
	}
	reopened, e := ingest.OpenArchive(path, limits)
	if e != nil {
		t.Fatal(e)
	}
	s.rawSources[key].archive = reopened
	restored, e := NewRawMaintenance(s, &cfg)
	if e != nil {
		t.Fatal("admission left unreadable durable evidence", e)
	}
	s.rawMaintenance = restored
	return restored
}

func TestRawMaintenanceReviewAcquisitionAdmissionBudget(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t, func(c *config.RawMaintenanceConfig) {
		c.MaxRawBytes = 64 << 10
		c.MaxPinsPerCapture = 256
		c.MaxScan = 10000
	})
	current, _ := s.stores[key].Current()
	reader := m.ProtectReader(key, current.Reference, budgetRawReader(func(ctx context.Context, _ string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
		for i := 0; i < 180; i++ {
			if e := s.rawSources[key].capture(ctx, "budget-provider", "eth_chainId", json.RawMessage(`[]`), []byte(`{"jsonrpc":"2.0","id":1,"result":"0x1"}`)); e != nil {
				return domain.BlockRef{}, nil, nil, e
			}
		}
		return current.Reference.Block, current.Pools, nil, nil
	}))
	_, _, _, e := reader.ReadComplete(context.Background(), "0x64")
	if !errors.Is(e, ingest.ErrBudget) {
		t.Error("oversized acquisition must be rejected at admission", e)
	}
	rows, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	held := 0
	for _, r := range rows {
		if r.Raw.Bytes > m.cfg.MaxRawBytes {
			t.Error("persisted evidence exceeds its subsequent read budget", r.Raw.Bytes)
		}
		if r.Raw.Owner.ProviderID == "budget-provider" {
			held++
			if r.ActiveReferences == 0 {
				t.Fatal("failed acquisition released unknown input")
			}
		}
	}
	if held != 180 {
		t.Fatal("lost bounded source input", held)
	}
	if e = m.Tick(context.Background()); e != nil {
		t.Error("rejected metadata poisoned retention", e)
	}
	restored := reopenBudgetMaintenance(t, s, m, key)
	if e = restored.Tick(context.Background()); e != nil {
		t.Fatal("reopen poisoned by rejected acquisition", e)
	}
}

func TestRawMaintenanceReviewHTTPAdmissionBudget(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t, func(c *config.RawMaintenanceConfig) { c.MaxRawBytes = 64 << 10; c.MaxScan = 10000 })
	body := `{"jsonrpc":"2.0","id":1,"result":"0x` + strings.Repeat("ab", 32<<10) + `"}`
	s.adapters[key].Client.HTTP.Transport = rawMaintenanceRoundTrip(func(r *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body))}, nil
	})
	var code string
	e := m.Observe(context.Background(), key, RawObserveProbe, func(ctx context.Context) error {
		return s.adapters[key].Client.Call(ctx, "eth_getCode", []any{"0x" + strings.Repeat("3", 40), "latest"}, &code)
	})
	if e == nil {
		t.Error("oversized encoded RPC evidence was admitted")
	}
	rows, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	for _, r := range rows {
		if r.Raw.Bytes > m.cfg.MaxRawBytes {
			t.Error("unreadable raw persisted", r.Raw.Bytes)
		}
	}
	if e = m.Tick(context.Background()); e != nil {
		t.Error("oversized RPC poisoned maintenance", e)
	}
	reopenBudgetMaintenance(t, s, m, key)
}

func TestRawMaintenanceReviewCloseCancelsBothPipelineReadersKeepsUnknownPins(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	current, _ := s.stores[key].Current()
	parent, cancel := context.WithCancel(context.Background())
	defer cancel()
	entered := make(chan struct{}, 2)
	finished := make(chan error, 1)
	reader := budgetRawReader(func(ctx context.Context, _ string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
		if e := s.rawSources[key].capture(ctx, "close-provider", "eth_chainId", json.RawMessage(`[]`), []byte(`{"jsonrpc":"2.0","id":1,"result":"0x1"}`)); e != nil {
			return domain.BlockRef{}, nil, nil, e
		}
		entered <- struct{}{}
		<-ctx.Done()
		return domain.BlockRef{}, nil, nil, ctx.Err()
	})
	go func() {
		s.syncMu.Lock()
		defer s.syncMu.Unlock()
		finished <- s.syncStaged(parent, s.cfg.Config.Chains[0], current, current.Reference.Block.Number+2, reader)
	}()
	for i := 0; i < 2; i++ {
		select {
		case <-entered:
		case <-time.After(time.Second):
			cancel()
			t.Fatal("original two-reader pipeline did not enter concurrently")
		}
	}
	a := s.rawSources[key].archive
	path, limits := a.Directory(), a.Limits()
	closed := make(chan struct{})
	go func() { s.Close(); close(closed) }()
	select {
	case <-closed:
	case <-time.After(time.Second):
		t.Error("Service.Close did not cancel registered staging readers")
		cancel()
		select {
		case <-closed:
		case <-time.After(time.Second):
			t.Fatal("Close cleanup blocked")
		}
	}
	select {
	case e := <-finished:
		if !errors.Is(e, context.Canceled) {
			t.Fatal(e)
		}
	case <-time.After(time.Second):
		t.Fatal("pipeline did not terminate")
	}
	m.mu.Lock()
	active := len(m.captureCancels)
	m.mu.Unlock()
	if active != 0 {
		t.Fatal("capture lifecycle still registered", active)
	}
	reopened, e := ingest.OpenArchive(path, limits)
	if e != nil {
		t.Fatal(e)
	}
	defer reopened.Close()
	rows, e := reopened.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	held := 0
	for _, r := range rows {
		if r.Raw.Owner.ProviderID == "close-provider" {
			held++
			if r.ActiveReferences == 0 || r.Reclaimed {
				t.Fatal("Close cleaned up unknown pins", r)
			}
		}
	}
	if held != 2 {
		t.Fatal("unknown concurrent inputs lost", held)
	}
}

func TestRawMaintenanceReviewDLQEncodedResponseBudget(t *testing.T) {
	for _, limit := range []int64{16 << 10, 256, 1} {
		t.Run(strconv.FormatInt(limit, 10), func(t *testing.T) {
			s, m, key, _ := maintenanceFixture(t, func(c *config.RawMaintenanceConfig) { c.MaxDecodedBytes = limit })
			rows, e := s.rawSources[key].archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			for i := 0; i < 20; i++ {
				if e = s.rawSources[key].archive.Quarantine(ingest.DeadLetter{Raw: rows[0].Raw, Stage: "decode", Code: "BUDGET", Scope: fmt.Sprintf("%s/scope-%d", key, i), Message: strings.Repeat("x", 1024)}); e != nil {
					t.Fatal(e)
				}
			}
			offset := 0
			for page := 0; page < 21; page++ {
				req := httptest.NewRequest("GET", "/internal/chains/"+key+"/raw/dlq?offset="+strconv.Itoa(offset), nil)
				req.Header.Set("Authorization", "Bearer "+s.cfg.Token)
				w := httptest.NewRecorder()
				m.Handler().ServeHTTP(w, req)
				if int64(w.Body.Len()) > limit {
					t.Fatalf("response exceeds byte budget: status=%d bytes=%d limit=%d", w.Code, w.Body.Len(), limit)
				}
				if limit <= 256 {
					if w.Code != 413 || (!strings.Contains(w.Body.String(), "RAW_PAGE") && !strings.Contains(w.Header().Get("X-Raw-Error"), "RAW_PAGE")) {
						t.Fatal("single over-budget record not explicit", w.Code, w.Body.String())
					}
					return
				}
				if w.Code != 200 {
					t.Fatal(w.Code, w.Body.String())
				}
				var p struct {
					Records []ingest.DeadLetter `json:"records"`
					Next    int                 `json:"next_offset"`
					Total   int                 `json:"total"`
				}
				if e = json.Unmarshal(w.Body.Bytes(), &p); e != nil {
					t.Fatal(e)
				}
				if p.Total != 20 || len(p.Records) == 0 || p.Next != offset+len(p.Records) || p.Next > 20 {
					t.Fatal("invalid byte page", p)
				}
				offset = p.Next
				if offset == 20 {
					return
				}
			}
			t.Fatal("byte pagination lost progress", offset)
		})
	}
}
