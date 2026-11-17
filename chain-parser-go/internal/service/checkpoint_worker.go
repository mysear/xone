package service

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"time"
	"xone/chain-parser-go/internal/checkpoint"
	"xone/chain-parser-go/internal/config"
)

type checkpointResult struct {
	Manifests []string                       `json:"manifests"`
	Public    []checkpoint.Publication       `json:"public_checkpoints"`
	Remote    []checkpoint.RemotePublication `json:"remote_checkpoints"`
}

// Both manual and background publications use one cancellable gate and one total
// deadline. No snapshot/actor lock is held during remote I/O.
func (s *Service) publishCheckpoints(parent context.Context) (checkpointResult, string) {
	return s.publishCheckpointChains(parent, s.cfg.Config.Chains, false)
}
func (s *Service) publishCheckpointChains(parent context.Context, chains []config.Chain, publicOnly bool) (checkpointResult, string) {
	result := checkpointResult{Manifests: []string{}, Public: []checkpoint.Publication{}, Remote: []checkpoint.RemotePublication{}}
	fail := func(code string) (checkpointResult, string) { return checkpointResult{}, code }
	ctx, cancel := context.WithTimeout(parent, time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
	defer cancel()
	if s.checkpointContext != nil {
		stop := context.AfterFunc(s.checkpointContext, cancel)
		defer stop()
	}
	select {
	case <-ctx.Done():
		return fail("CHECKPOINT_CANCELLED")
	case s.checkpointGate <- struct{}{}:
	}
	defer func() { <-s.checkpointGate }()
	if ctx.Err() != nil || s.closed.Load() {
		return fail("CHECKPOINT_CANCELLED")
	}
	for _, chain := range chains {
		store := s.stores[chain.Key]
		if store == nil {
			continue
		}
		snapshot, ok := store.Current()
		if !ok {
			continue
		}
		if ctx.Err() != nil || s.closed.Load() {
			return fail("CHECKPOINT_CANCELLED")
		}
		dir := filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), "checkpoints", strings.ReplaceAll(chain.Key, ":", "-"))
		pending, e := checkpoint.HasPendingReclaim(dir)
		if e != nil {
			return fail("CHECKPOINT_RECLAIM_STATE_INVALID")
		}
		if pending {
			return fail("CHECKPOINT_RECLAIM_PENDING")
		}
		if !publicOnly {
			path, e := checkpoint.Write(dir, snapshot)
			if e != nil {
				return fail("CHECKPOINT_FAILED")
			}
			result.Manifests = append(result.Manifests, path)
		}
		boot, e := store.Bootstrap()
		if e != nil {
			if s.cfg.Config.Journal.Transport == "kafka" {
				return fail("CHECKPOINT_NOT_READY")
			}
			continue
		}
		guard := func() error {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			if s.closed.Load() {
				return errors.New("CHECKPOINT_CANCELLED")
			}
			current, e := store.Bootstrap()
			if e != nil {
				return e
			}
			old, next := boot.Snapshot.Reference, current.Snapshot.Reference
			if old.SchemaVersion != next.SchemaVersion || old.AdapterSetHash != next.AdapterSetHash || old.OwnerService != next.OwnerService || old.WriterEpoch != next.WriterEpoch || old.CanonicalEpoch != next.CanonicalEpoch || old.OnlineGeneration != next.OnlineGeneration || old.ScopeHash != next.ScopeHash || old.OwnerAssignmentVersion != next.OwnerAssignmentVersion {
				return errors.New("CHECKPOINT_SOURCE_REVOKED")
			}
			return nil
		}
		if e = guard(); e != nil {
			return fail("CHECKPOINT_PUBLICATION_FAILED")
		}
		if c := s.cfg.Config.Journal.CheckpointStore; c != nil && c.LocalBudget != nil {
			if _, e := checkpoint.AdmitBootstrap(dir, boot, s.cfg.Config.Journal.MaxBytes, c.BaseURL, c.LocalBudget, guard); e != nil {
				return fail("CHECKPOINT_LOCAL_BUDGET_REJECTED")
			}
		}
		publication, e := checkpoint.WriteBootstrap(dir, boot, s.cfg.Config.Journal.MaxBytes, guard)
		if e != nil {
			return fail("CHECKPOINT_PUBLICATION_FAILED")
		}
		result.Public = append(result.Public, publication)
		if c := s.cfg.Config.Journal.CheckpointStore; c != nil {
			remote, e := checkpoint.NewRemote(*c, s.cfg.Config.Journal.SourceAuthTokens[c.BaseURL], s.cfg.Config.Journal.MaxBytes, time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
			if e != nil {
				return fail("CHECKPOINT_REMOTE_CONFIG")
			}
			published, e := remote.Publish(ctx, publication, guard)
			remote.Close()
			if e != nil {
				return fail("CHECKPOINT_REMOTE_PUBLICATION_FAILED")
			}
			result.Remote = append(result.Remote, published)
		}
	}
	return result, ""
}
func (s *Service) startCheckpointWorker(parent context.Context) {
	c := s.cfg.Config.Journal.CheckpointStore
	if c == nil || c.PublishIntervalMS == 0 {
		return
	}
	s.checkpointLifecycle.Lock()
	defer s.checkpointLifecycle.Unlock()
	if s.closed.Load() || s.checkpointCancel != nil {
		return
	}
	ctx, cancel := context.WithCancel(parent)
	s.checkpointCancel = cancel
	s.checkpointWG.Add(1)
	go func() {
		defer s.checkpointWG.Done()
		s.checkpointActive.Store(true)
		defer s.checkpointActive.Store(false)
		timer := time.NewTimer(0)
		defer timer.Stop()
		next := 0
		for {
			select {
			case <-ctx.Done():
				return
			case <-timer.C:
			}
			if ctx.Err() != nil {
				return
			}
			// Select before execution and advance even after failure. A bad chain
			// cannot consume every cycle while healthy chains wait forever.
			chains := s.cfg.Config.Chains
			if len(chains) == 0 {
				return
			}
			chain := chains[next%len(chains)]
			next = (next + 1) % len(chains)
			result, code := s.publishCheckpointChains(ctx, []config.Chain{chain}, true)
			if ctx.Err() != nil {
				return
			}
			s.checkpointCycles.Add(1)
			if code != "" {
				s.checkpointErrors.Add(1)
				s.log.Event("warn", "checkpoint.failed", map[string]any{"chain_key": chain.Key, "error_code": code})
			} else if len(result.Remote) > 0 {
				s.log.Event("info", "checkpoint.uploaded", map[string]any{"chain_key": chain.Key, "checkpoint_count": len(result.Remote), "periodic": true})
			}
			// A timer starts only after completion; long uploads never queue ticks.
			timer.Reset(time.Duration(c.PublishIntervalMS) * time.Millisecond)
		}
	}()
}
func (s *Service) stopCheckpointWorker() {
	if s.checkpointStop != nil {
		s.checkpointStop()
	}
	s.checkpointLifecycle.Lock()
	cancel := s.checkpointCancel
	s.checkpointLifecycle.Unlock()
	if cancel != nil {
		cancel()
	}
	s.checkpointWG.Wait()
	// Also wait for a manual publication already in flight before closing stores.
	if s.checkpointGate != nil {
		s.checkpointGate <- struct{}{}
		<-s.checkpointGate
	}
}
