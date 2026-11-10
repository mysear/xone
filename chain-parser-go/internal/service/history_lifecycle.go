package service

import (
	"context"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func (s *Service) initHistoryLifecycle(ctx context.Context) {
	s.historyMu.Lock()
	defer s.historyMu.Unlock()
	if s.historyCancel != nil {
		return
	}
	s.historyContext, s.historyCancel = context.WithCancel(ctx)
	s.historyRuntime = map[string]*HistoryRuntime{}
	s.historyInitErrors = map[string]string{}
	// Give the first qualified scope and normal head collector priority before
	// admitting historical I/O. This does not enter the online ready condition.
	s.historyScheduler = &HistoryScheduler{next: time.Now().Add(time.Second)}
}
func (s *Service) recordHistoryBaseline(ctx context.Context, chain config.Chain, snapshot domain.Snapshot) {
	s.historyMu.Lock()
	history := s.historyRuntime[chain.Key]
	lifetime := s.historyContext
	scheduler := s.historyScheduler
	s.historyMu.Unlock()
	if lifetime == nil || scheduler == nil {
		return
	}
	scope := s.historyScopeForCommit(chain, snapshot)
	if history == nil {
		opened, e := s.NewChainHistoryRuntimeForScope(chain.Key, scheduler, scope, snapshot.Reference)
		if e != nil {
			s.setHistoryInitError(chain.Key, e)
			return
		}
		s.historyMu.Lock()
		s.historyRuntime[chain.Key] = opened
		delete(s.historyInitErrors, chain.Key)
		s.historyMu.Unlock()
		history = opened
		if e = history.Start(lifetime); e != nil {
			s.setHistoryInitError(chain.Key, e)
			return
		}
	} else if err := history.UpdateScope(ctx, scope, snapshot.Reference); err != nil {
		s.setHistoryInitError(chain.Key, err)
		return
	}
	// This is a bounded local namespace write after the owner commit. A source or
	// capacity failure belongs to history health and cannot revoke the snapshot.
	if e := history.EnqueueBaseline(ctx, snapshot); e != nil {
		s.setHistoryInitError(chain.Key, e)
		return
	}
	s.historyMu.Lock()
	delete(s.historyInitErrors, chain.Key)
	s.historyMu.Unlock()
}
func (s *Service) setHistoryInitError(key string, err error) {
	s.historyMu.Lock()
	if s.historyInitErrors != nil {
		s.historyInitErrors[key] = err.Error()
	}
	s.historyMu.Unlock()
	s.log.Event("warn", "history_background_retry", map[string]any{"chain_key": key, "error_code": err.Error(), "history_blocks_readiness": false})
}
func (s *Service) addNormalHistoryStatus(ctx context.Context, key string, status map[string]any) {
	s.historyMu.Lock()
	history := s.historyRuntime[key]
	failure := s.historyInitErrors[key]
	s.historyMu.Unlock()
	AddHistorySourceStatus(ctx, status, history)
	if failure != "" {
		status["history_runtime_error"] = failure
		status["history_configured_enabled"] = s.cfg.Config.History.Enabled
	}
}
func (s *Service) closeHistoryLifecycle() {
	s.historyMu.Lock()
	cancel := s.historyCancel
	histories := make([]*HistoryRuntime, 0, len(s.historyRuntime))
	for _, h := range s.historyRuntime {
		histories = append(histories, h)
	}
	s.historyMu.Unlock()
	if cancel != nil {
		cancel()
	}
	for _, h := range histories {
		h.Close()
	}
}

func (s *Service) historyScopeForCommit(selected config.Chain, snapshot domain.Snapshot) config.Chain {
	root := selected
	for _, chain := range s.cfg.Config.Chains {
		if chain.Key == selected.Key {
			root = chain
			break
		}
	}
	result := s.runtimeCatalogChain(root)
	known := map[string]bool{}
	for _, pool := range result.Pools {
		known[pool.ID] = true
	}
	qualified := map[string]bool{}
	for _, pool := range snapshot.Pools {
		qualified[pool.PoolID] = true
	}
	for _, pool := range selected.Pools {
		if qualified[pool.ID] && !known[pool.ID] {
			result.Pools = append(result.Pools, pool)
			known[pool.ID] = true
		}
	}
	return result
}
