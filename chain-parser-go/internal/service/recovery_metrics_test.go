package service

import (
	"context"
	"fmt"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestRecoveryMetricsRecordFirstQualifiedScopeAndDoNotWaitForHistory(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		snapshot, e := testSnapshotSource(t)(ctx, ch)
		snapshot.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		return snapshot, e
	}
	service, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer service.Close()
	scrape := func() string {
		w := httptest.NewRecorder()
		service.Handler().ServeHTTP(w, httptest.NewRequest("GET", "/metrics", nil))
		return w.Body.String()
	}
	if strings.Contains(scrape(), "xone_first_qualified_snapshot_seconds") {
		t.Fatal("first scope metric fabricated before qualification")
	}
	if e = service.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	body := scrape()
	expectedSnapshot, err := testSnapshotSource(t)(context.Background(), cfg.Config.Chains[0])
	if err != nil {
		t.Fatal(err)
	}
	for _, expected := range []string{`xone_first_qualified_snapshot_seconds{chain="evm:31337"}`, fmt.Sprintf(`xone_target_scope_pool_count{chain="evm:31337"} %d`, len(expectedSnapshot.Pools)), `xone_scope_restore_success_total{chain="evm:31337"} 1`} {
		if !strings.Contains(body, expected) {
			t.Fatal("missing actual recovery metric", expected, body)
		}
	}
}
