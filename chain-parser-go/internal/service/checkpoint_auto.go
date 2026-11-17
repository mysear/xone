package service

import (
	"context"
	"errors"
	"time"
	"xone/chain-parser-go/internal/checkpoint"
	"xone/chain-parser-go/internal/journal"
)

func (s *Service) checkpointRetentionGuard(ctx context.Context, key string, boot journal.Bootstrap) func() error {
	return func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if s.closed.Load() {
			return errors.New("CHECKPOINT_OWNER_CLOSED")
		}
		store := s.stores[key]
		if store == nil {
			return errors.New("CHECKPOINT_UNKNOWN_CHAIN")
		}
		reference, cursor, e := store.BootstrapHead()
		if e != nil {
			return e
		}
		if reference != boot.Snapshot.Reference || cursor != boot.Cursor {
			return errors.New("CHECKPOINT_RETENTION_SOURCE_CHANGED")
		}
		return nil
	}
}
func (s *Service) checkpointAutomaticEnabled() bool {
	c := s.cfg.Config.Journal.CheckpointStore
	return c != nil && c.LocalRetention != nil && c.LocalRetention.ReclaimEnabled && c.LocalRetention.AutoReclaim
}

// The caller already holds the shared publication gate and its total deadline.
// There is never an HTTP self-call, another worker, or a queued cleanup loop.
func (s *Service) maintainCheckpointAutomatic(ctx context.Context, key, dir string, boot journal.Bootstrap) (checkpoint.ReclaimResult, error) {
	if !s.checkpointAutomaticEnabled() {
		return checkpoint.ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_AUTOMATIC_DISABLED")
	}
	c := s.cfg.Config.Journal.CheckpointStore
	s.checkpointAutoAttempts.Add(1)
	remote, e := checkpoint.NewRemote(*c, s.cfg.Config.Journal.SourceAuthTokens[c.BaseURL], s.cfg.Config.Journal.MaxBytes, time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
	if e != nil {
		s.checkpointAutoFailures.Add(1)
		return checkpoint.ReclaimResult{}, e
	}
	defer remote.Close()
	result, e := remote.AutoReclaim(ctx, dir, boot, *c.LocalRetention, time.Now(), s.checkpointRetentionGuard(ctx, key, boot))
	if e != nil {
		s.checkpointAutoFailures.Add(1)
		return result, e
	}
	if result.Applied && !result.Idempotent {
		s.checkpointAutoCompletions.Add(1)
		s.log.Event("info", "checkpoint.reclaimed", map[string]any{"chain_key": key, "operation_id": result.OperationID, "completed_objects": result.CompletedObjects, "completed_bytes": result.CompletedBytes, "resumed": result.Resumed, "automatic": true})
	} else if result.Status == "NO_ELIGIBLE_OBJECTS" {
		s.checkpointAutoNoop.Add(1)
	}
	return result, nil
}
