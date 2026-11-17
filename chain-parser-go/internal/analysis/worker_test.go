package analysis

import (
	"context"
	"errors"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
)

func TestJournalWorkerRepeatsSeriallyAndStopsWithoutQueuedTicks(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var cycles, active, published atomic.Int64
	err := runJournalCycles(ctx, time.Millisecond, func(context.Context) error {
		if active.Add(1) != 1 {
			t.Fatal("overlapping analytics cycles")
		}
		cycles.Add(1)
		active.Add(-1)
		return nil
	}, func() {
		if published.Add(1) == 3 {
			cancel()
		}
	})
	if err != nil || cycles.Load() != 3 || published.Load() != 3 || active.Load() != 0 {
		t.Fatal(err, cycles.Load(), published.Load(), active.Load())
	}
}
func TestJournalWorkerCancellationReachesActiveReadAndIdleWait(t *testing.T) {
	for _, active := range []bool{false, true} {
		ctx, cancel := context.WithCancel(context.Background())
		started := make(chan struct{})
		done := make(chan error, 1)
		go func() {
			done <- runJournalCycles(ctx, time.Minute, func(ctx context.Context) error {
				close(started)
				if active {
					<-ctx.Done()
					return ctx.Err()
				}
				return nil
			}, nil)
		}()
		<-started
		cancel()
		select {
		case err := <-done:
			if err != nil {
				t.Fatal(err)
			}
		case <-time.After(time.Second):
			t.Fatal("worker did not stop")
		}
	}
}
func TestJournalWorkerErrorsStopWithoutPublishingOrSkipping(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	poison := errors.New("ANALYTICS_EVENT_CONTENT_CONFLICT")
	calls, published := 0, 0
	err := runJournalCycles(ctx, time.Millisecond, func(context.Context) error {
		calls++
		if calls == 2 {
			return poison
		}
		return nil
	}, func() { published++ })
	if !errors.Is(err, poison) || calls != 2 || published != 1 {
		t.Fatal(err, calls, published)
	}
}
func TestJournalWorkerCancelledBeforeStartNeverOpensReader(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	calls := 0
	if err := runJournalCycles(ctx, time.Millisecond, func(context.Context) error { calls++; return nil }, nil); err != nil || calls != 0 {
		t.Fatal(err, calls)
	}
}
func TestJournalWorkerModeAndBudgetsAreExplicit(t *testing.T) {
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err = RunJournalWorker(ctx, cfg, nil); err == nil {
		t.Fatal("online role admitted")
	}
	cfg.Config.Service.Role = "analytics"
	cfg.Config.Chains[0].AnalysisIntervalMS = 60000
	cfg.Config.Service.Mode = "journal-continuous"
	cfg.Config.Journal.Transport = "kafka"
	cfg.Config.Control.Backend = "disabled"
	if err = RunJournalWorker(ctx, cfg, nil); err != nil {
		t.Fatal(err)
	}
	for _, poll := range []uint64{0, 60001, ^uint64(0)} {
		cfg.Config.Journal.PollMS = poll
		if RunJournalWorker(ctx, cfg, nil) == nil {
			t.Fatal("invalid poll budget", poll)
		}
	}
}
