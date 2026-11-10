package service

import (
	"context"
	"errors"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestNormalHistoryLifecycleRegistersGapsWithoutBlockingFirstScope(t *testing.T) {
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
	if e = service.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	key := cfg.Config.Chains[0].Key
	history := service.historyRuntime[key]
	if history == nil {
		t.Fatal("normal baseline did not initialize isolated history runtime")
	}
	history.Pause(true)
	status, e := history.SourceStatus(context.Background())
	if e != nil {
		t.Fatal(e)
	}
	if len(status.Queue.Gaps) == 0 || status.Queue.Complete {
		t.Fatal("old baseline gaps not independently registered", status.Queue)
	}
	current, ready := service.stores[key].Current()
	if !ready || len(current.Pools) == 0 {
		t.Fatal("paused/unavailable history blocked qualified online scope")
	}
	baselineQueued := false
	for _, job := range status.Queue.Jobs {
		if job.From <= current.Reference.Block.Number && job.To >= current.Reference.Block.Number {
			baselineQueued = true
		}
	}
	if !baselineQueued {
		t.Fatal("snapshot H has no fact batch and was omitted from background history", status.Queue)
	}
	service.setHistoryInitError(key, errors.New("HISTORY_CAPACITY_RETRY"))
	service.recordHistoryBaseline(context.Background(), cfg.Config.Chains[0], current)
	merged := map[string]any{}
	service.addNormalHistoryStatus(context.Background(), key, merged)
	if _, staleError := merged["history_runtime_error"]; staleError {
		t.Fatal("successful scope refresh retained old background error", merged)
	}
}

func TestNormalReorgRefreshesHistoryCanonicalScope(t *testing.T) {
	s, ch, tip, reader := forkService(t)
	history := s.historyRuntime[ch.Key]
	history.Pause(true)
	before, err := history.SourceStatus(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if err = s.recoverFork(context.Background(), ch, tip, reader); err != nil {
		t.Fatal(err)
	}
	current, ready := s.stores[ch.Key].Current()
	after, err := history.SourceStatus(context.Background())
	if err != nil || !ready || after.Queue.ScopeStale || after.Queue.CanonicalEpoch != current.Reference.CanonicalEpoch || after.Queue.Revision == before.Queue.Revision {
		t.Fatalf("committed reorg left history on old revision: before=%+v after=%+v owner=%+v ready=%v err=%v", before.Queue, after.Queue, current.Reference, ready, err)
	}
	if s.historyInitErrors[ch.Key] != "" {
		t.Fatal("history refresh failed", s.historyInitErrors[ch.Key])
	}
}
