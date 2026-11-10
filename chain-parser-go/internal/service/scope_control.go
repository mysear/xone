package service

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"sort"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// ensurePoolScope admits only configured identities. Discovery candidates must
// pass their version-specific current-state admission before entering this list.
// A control request never publishes its own online generation or history state.
func (s *Service) ensurePoolScope(w http.ResponseWriter, r *http.Request) {
	write := func(code int, value any) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(code)
		_ = json.NewEncoder(w).Encode(value)
	}
	fail := func(code int, reason string) { write(code, map[string]any{"error": reason, "ready": false}) }
	var request struct {
		PoolIDs            []string `json:"pool_ids"`
		ExpectedGeneration uint64   `json:"expected_online_generation"`
	}
	decoder := json.NewDecoder(http.MaxBytesReader(w, r.Body, int64(s.cfg.Config.API.MaxBody)))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&request) != nil || decoder.Decode(new(any)) != io.EOF || len(request.PoolIDs) == 0 || len(request.PoolIDs) > 1024 {
		fail(400, "INVALID_POOL_SCOPE")
		return
	}
	key := r.PathValue("chain")
	var selected *config.Chain
	for i := range s.cfg.Config.Chains {
		if s.cfg.Config.Chains[i].Key == key {
			selected = &s.cfg.Config.Chains[i]
			break
		}
	}
	if selected == nil {
		fail(404, "UNKNOWN_CHAIN")
		return
	}
	merged := s.runtimeCatalogChain(*selected)
	selected = &merged
	catalog := map[string]bool{}
	for _, p := range selected.Pools {
		catalog[p.ID] = true
	}
	seen := map[string]bool{}
	for _, id := range request.PoolIDs {
		if seen[id] || id == "" {
			fail(400, "INVALID_POOL_SCOPE")
			return
		}
		seen[id] = true
		if !catalog[id] {
			fail(404, "POOL_NOT_IN_ADMITTED_CATALOG")
			return
		}
	}
	if !s.syncMu.TryLock() {
		fail(429, "CHAIN_ACTOR_BUSY")
		return
	}
	defer s.syncMu.Unlock()
	if s.closed.Load() {
		fail(503, "SERVICE_CLOSED")
		return
	}
	store := s.stores[key]
	reference := store.DurableRef()
	if request.ExpectedGeneration == 0 || request.ExpectedGeneration != reference.OnlineGeneration {
		fail(409, "POOL_SCOPE_GENERATION_CHANGED")
		return
	}
	current, ready := store.Current()
	previousRef := current.Reference
	hadFreshScope := ready && bootstrap.CheckFresh(current, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge) == nil
	missing := false
	present := map[string]bool{}
	for _, pool := range current.Pools {
		present[pool.PoolID] = true
	}
	for _, id := range request.PoolIDs {
		if !present[id] {
			missing = true
		}
	}
	if missing || !ready || bootstrap.CheckFresh(current, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge) != nil {
		ctx, cancel := context.WithTimeout(r.Context(), time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
		defer cancel()
		if err := s.reanchorScope(ctx, *selected, false, request.PoolIDs); err != nil {
			retained, live := store.Current()
			pending, pendingErr := store.Prepared()
			owner := s.captureRuntimeOwner(*selected)
			if hadFreshScope && live && owner.Valid && retained.Reference == previousRef && pendingErr == nil && pending == nil && !store.NeedsReopen() && bootstrap.CheckFresh(retained, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge) == nil {
				write(503, map[string]any{"error": "POOL_SCOPE_RESTORE_FAILED", "ready": false, "retained_scope_ready": true})
				return
			}
			_ = s.revokeRuntimeSources(ctx, *selected, nil)
			if old, ok := store.Current(); ok {
				_ = s.revokeScope(ctx, *selected, old)
			}
			store.Suspend()
			fail(503, "POOL_SCOPE_RESTORE_FAILED")
			return
		}
		current, ready = store.Current()
	}
	if !ready || bootstrap.CheckFresh(current, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge) != nil {
		fail(503, "POOL_SCOPE_NOT_READY")
		return
	}
	actual := map[string]domain.Pool{}
	for _, p := range current.Pools {
		actual[p.PoolID] = p
	}
	for _, id := range request.PoolIDs {
		if _, ok := actual[id]; !ok {
			fail(503, "POOL_SCOPE_DEPENDENCIES_INCOMPLETE")
			return
		}
	}
	// Re-read after source work: cancellation/fencing cannot return a stale handle.
	if r.Context().Err() != nil {
		return
	}
	final, ok := store.Current()
	if !ok || final.Reference != current.Reference {
		fail(409, "POOL_SCOPE_GENERATION_CHANGED")
		return
	}
	sort.Strings(request.PoolIDs)
	write(200, map[string]any{"ready": true, "scope": "PARSER_STATE_SCOPE", "pool_ids": request.PoolIDs, "snapshot_ref": current.Reference, "history_blocks_readiness": false, "strict_historical_bank_proof": false})
}
