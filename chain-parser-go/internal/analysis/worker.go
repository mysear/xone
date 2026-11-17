package analysis

import (
	"context"
	"errors"
	"time"
	"xone/chain-parser-go/internal/config"
)

// RunJournalWorker is an explicitly selected, read-only role of the existing Go
// service. Each cycle reopens only its own durable analytics checkpoint and
// bounded committed reader. It never opens online KV, a lease or a producer.
// Semantic/transport errors are durably recorded by RunJournalJob and stop this
// worker; no poison fact, retention gap or topic replacement is silently skipped.
func RunJournalWorker(ctx context.Context, cfg *config.Snapshot, published func()) error {
	if cfg == nil || cfg.Config.Service.Name != "chain-parser-go" || cfg.Config.Service.Role != "analytics" ||
		cfg.Config.Service.Mode != "journal-continuous" || cfg.Config.Journal.Transport != "kafka" ||
		cfg.Config.Control.Backend != "disabled" || cfg.Config.Journal.PollMS == 0 || cfg.Config.Journal.PollMS > 60000 {
		return errors.New("ANALYTICS_WORKER_CONFIG")
	}
	for _, chain := range cfg.Config.Chains {
		if chain.AnalysisIntervalMS == 0 || chain.AnalysisIntervalMS > 86400000 {
			return errors.New("ANALYTICS_JOURNAL_DEFINITION")
		}
	}
	return runJournalCycles(ctx, time.Duration(cfg.Config.Journal.PollMS)*time.Millisecond,
		func(ctx context.Context) error { return RunJournalJob(ctx, cfg) }, published)
}

// One synchronous cycle owns its readers through cleanup. The next interval
// starts after completion, so a slow read cannot create overlapping jobs or a
// backlog of queued ticks. Cancellation also wakes an idle worker immediately.
func runJournalCycles(ctx context.Context, interval time.Duration, cycle func(context.Context) error, published func()) error {
	if interval <= 0 || interval > time.Minute || cycle == nil {
		return errors.New("ANALYTICS_WORKER_CONFIG")
	}
	for {
		if ctx.Err() != nil {
			return nil
		}
		if err := cycle(ctx); err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return err
		}
		if published != nil {
			published()
		}
		timer := time.NewTimer(interval)
		select {
		case <-ctx.Done():
			if !timer.Stop() {
				select {
				case <-timer.C:
				default:
				}
			}
			return nil
		case <-timer.C:
		}
	}
}
