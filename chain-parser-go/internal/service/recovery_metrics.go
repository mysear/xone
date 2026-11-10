package service

import (
	"fmt"
	"io"
	"strings"
	"sync"
	"time"
	"xone/chain-parser-go/internal/domain"
)

type scopeRestoreStats struct {
	mu                                    sync.Mutex
	attempts, success, failures, rejected uint64
	firstQualified                        bool
	firstElapsed, lastElapsed             time.Duration
	targetPools, lastPools                int
	generation                            uint64
}

func (s *Service) startScopeMetrics(chain string) func(error) {
	stats := s.scopeTimings[chain]
	if stats == nil {
		return func(error) {}
	}
	start := time.Now()
	stats.mu.Lock()
	stats.attempts++
	stats.mu.Unlock()
	return func(err error) {
		stats.mu.Lock()
		defer stats.mu.Unlock()
		stats.lastElapsed = time.Since(start)
		if err == nil {
			stats.success++
		} else {
			stats.failures++
			message := err.Error()
			if strings.Contains(message, "FENCED") || strings.Contains(message, "FENCE_CHANGED") || strings.Contains(message, "SCOPE_OWNER_LEDGER_CHANGED") {
				stats.rejected++
			}
		}
	}
}
func (s *Service) setScopeTarget(chain string, count int) {
	if stats := s.scopeTimings[chain]; stats != nil {
		stats.mu.Lock()
		stats.targetPools = count
		stats.mu.Unlock()
	}
}
func (s *Service) observeQualifiedScope(snapshot domain.Snapshot) {
	if len(snapshot.Pools) == 0 {
		return
	}
	stats := s.scopeTimings[snapshot.Reference.Block.ChainKey]
	if stats == nil {
		return
	}
	stats.mu.Lock()
	defer stats.mu.Unlock()
	if !stats.firstQualified {
		stats.firstQualified = true
		stats.firstElapsed = time.Since(s.constructedAt)
	}
	stats.lastPools = len(snapshot.Pools)
	if stats.targetPools == 0 {
		stats.targetPools = len(snapshot.Pools)
	}
	stats.generation = snapshot.Reference.OnlineGeneration
}
func (s *Service) scopeRecoveryStatus(chain string) map[string]any {
	stats := s.scopeTimings[chain]
	if stats == nil {
		return map[string]any{"observed": false}
	}
	stats.mu.Lock()
	defer stats.mu.Unlock()
	status := map[string]any{"observed": stats.firstQualified, "restore_attempts": stats.attempts, "restore_successes": stats.success, "restore_failures": stats.failures, "old_write_rejections": stats.rejected, "last_restore_ms": stats.lastElapsed.Milliseconds(), "target_scope_pool_count": stats.targetPools, "last_qualified_pool_count": stats.lastPools, "online_generation": stats.generation, "history_blocks_readiness": false}
	if stats.firstQualified {
		status["first_qualified_snapshot_ms"] = stats.firstElapsed.Milliseconds()
	}
	return status
}
func (s *Service) writeRecoveryMetrics(w io.Writer) {
	for chain, stats := range s.scopeTimings {
		stats.mu.Lock()
		fmt.Fprintf(w, "xone_scope_restore_attempts_total{chain=%q} %d\nxone_scope_restore_success_total{chain=%q} %d\nxone_scope_restore_failures_total{chain=%q} %d\nxone_old_scope_write_rejected_total{chain=%q} %d\nxone_target_scope_pool_count{chain=%q} %d\nxone_last_scope_restore_seconds{chain=%q} %.9f\n", chain, stats.attempts, chain, stats.success, chain, stats.failures, chain, stats.rejected, chain, stats.targetPools, chain, stats.lastElapsed.Seconds())
		if stats.firstQualified {
			fmt.Fprintf(w, "xone_first_qualified_snapshot_seconds{chain=%q} %.9f\n", chain, stats.firstElapsed.Seconds())
		}
		stats.mu.Unlock()
	}
}
