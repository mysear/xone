package service

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
	"xone/chain-parser-go/internal/checkpoint"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

func automaticCheckpointService(t *testing.T, recent bool) (*Service, *checkpointObjects, []checkpoint.Publication) {
	return automaticCheckpointServiceOptions(t, recent, 100, 19, false)
}
func automaticCheckpointServiceOptions(t *testing.T, recent bool, interval, maxFiles uint64, pending bool) (*Service, *checkpointObjects, []checkpoint.Publication) {
	t.Helper()
	source := testSnapshotSource(t)
	var seeds []checkpoint.Publication
	s, objects := checkpointWorkerServiceConfigured(t, interval, context.Background(), func(cfg *config.Snapshot) {
		policy := serviceReclaimPolicy()
		policy.AutoReclaim = true
		cfg.Config.API.AuthToken = cfg.Token
		cfg.Config.Journal.TimeoutMS = 5000
		c := cfg.Config.Journal.CheckpointStore
		c.LocalRetention = &policy
		c.LocalBudget = &config.CheckpointBudget{MaxBytes: 1 << 20, MaxFiles: maxFiles, MaxEntries: 64}
		remote, e := checkpoint.NewRemote(*c, cfg.Config.Journal.SourceAuthTokens[c.BaseURL], cfg.Config.Journal.MaxBytes, 5*time.Second)
		if e != nil {
			t.Fatal(e)
		}
		defer remote.Close()
		var last journal.Bootstrap
		snapshot, e := source(context.Background(), cfg.Config.Chains[0])
		if e != nil {
			t.Fatal(e)
		}
		dir := filepath.Join(cfg.Config.State.Path, "checkpoints", "evm-31337")
		for i := 0; i < 6; i++ {
			copy := snapshot
			copy.Reference.OnlineGeneration = uint64(i + 1)
			copy.Reference.JournalSeq = uint64(i + 1)
			boot := journal.Bootstrap{Snapshot: copy, Cursor: journal.Cursor{Topic: "test.worker-checkpoint", Partition: 0, Offset: 9007199254741201 + int64(i*4)}}
			p, e := checkpoint.WriteBootstrap(dir, boot, cfg.Config.Journal.MaxBytes, nil)
			if e != nil {
				t.Fatal(e)
			}
			if _, e = remote.Publish(context.Background(), p, nil); e != nil {
				t.Fatal(e)
			}
			if !recent {
				old := time.Now().Add(-48 * time.Hour)
				if e = os.Chtimes(p.ManifestPath, old, old); e != nil {
					t.Fatal(e)
				}
			}
			seeds = append(seeds, p)
			last = boot
		}
		if pending {
			plan, e := remote.PlanRetention(dir, last, policy, time.Now(), func() error { return nil })
			if e != nil {
				t.Fatal(e)
			}
			stopped := errors.New("OLD_OWNER_STOP_AFTER_QUARANTINE")
			_, e = remote.ApplyRetention(context.Background(), dir, last, policy, checkpoint.RetentionApplyRequest{PlanID: plan.PlanID, ObservedAtMS: plan.ObservedAtMS}, time.Now(), func() error {
				entries, _ := filepath.Glob(filepath.Join(dir, "retirements", plan.PlanID, "*"))
				if len(entries) > 0 {
					return stopped
				}
				return nil
			})
			if !errors.Is(e, stopped) {
				t.Fatal("old owner did not leave an actual pending quarantine", e)
			}
		}
	}, source)
	return s, objects, seeds
}
func automaticCurrent(t *testing.T, s *Service, generation uint64, offset int64) journal.Bootstrap {
	t.Helper()
	store := s.stores["evm:31337"]
	snapshot, ok := store.Current()
	if !ok {
		t.Fatal("owner missing")
	}
	snapshot.Reference.OnlineGeneration = generation
	snapshot.Reference.JournalSeq = generation
	boot := journal.Bootstrap{Snapshot: snapshot, Cursor: journal.Cursor{Topic: "test.worker-checkpoint", Partition: 0, Offset: offset}}
	if e := store.Prepare(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot}); e != nil {
		t.Fatal(e)
	}
	if e := store.ApplyPrepared(boot.Cursor); e != nil {
		t.Fatal(e)
	}
	return boot
}
func autoCatalog(s *Service) (checkpoint.RemotePublication, bool) {
	w := workerRequest(s, context.Background(), "GET", "/internal/chains/evm:31337/checkpoint-catalog")
	var p checkpoint.RemotePublication
	if w.Code != 200 || json.Unmarshal(w.Body.Bytes(), &p) != nil {
		return p, false
	}
	return p, true
}
func TestNormalCheckpointWorkerAutomaticallyReclaimsOneBatchUnderQuotaAndPublishesCurrentState(t *testing.T) {
	s, objects, seeds := automaticCheckpointService(t, false)
	current := automaticCurrent(t, s, 7, 9007199254741251)
	var latest checkpoint.RemotePublication
	workerWait(t, func() bool {
		p, ok := autoCatalog(s)
		if ok {
			latest = p
		}
		return ok && p.Reference.Reference == current.Snapshot.Reference
	})
	if s.checkpointAutoCompletions.Load() != 1 {
		t.Fatal("cleanup loop exceeded one needed batch", s.checkpointAutoCompletions.Load())
	}
	for _, p := range seeds[:2] {
		if _, e := os.Stat(filepath.Dir(p.ManifestPath)); !os.IsNotExist(e) {
			t.Fatal("old cache not released", e)
		}
	}
	for _, p := range seeds[4:] {
		if _, e := checkpoint.ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal("recent backups removed", e)
		}
	}
	store := s.stores["evm:31337"]
	actual, e := store.Bootstrap()
	if e != nil || actual.Snapshot.Reference != current.Snapshot.Reference || actual.Cursor != current.Cursor {
		t.Fatal("automatic maintenance changed owner cursor", e)
	}
	status := workerRequest(s, context.Background(), "GET", "/internal/checkpoints/status")
	var metrics map[string]any
	if status.Code != 200 || json.Unmarshal(status.Body.Bytes(), &metrics) != nil || metrics["auto_reclaim_enabled"] != true {
		t.Fatal(status.Code, status.Body.String())
	}
	objects.mu.Lock()
	max := objects.maxActive
	objects.mu.Unlock()
	if max != 1 {
		t.Fatal("automatic maintenance opened parallel HTTP lanes", max)
	}
	if out := os.Getenv("XONE_CHECKPOINT_AUTO_RECLAIM_OUTPUT"); out != "" {
		os.MkdirAll(out, 0700)
		raw, e := json.MarshalIndent(map[string]any{"catalog": latest, "status": metrics, "owner_state_unchanged": true, "two_latest_seed_backups_still_decode": true, "max_active_object_requests": max, "verification_scope": "NORMAL_GO_PERIODIC_WORKER_SEEDED_OLD_CONFIRMED_CACHE_QUOTA_TRIGGER_ONE_BATCH_AND_CURRENT_PUBLICATION"}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(out, "automatic-quota-reclaim.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
	s.Close()
}
func TestNormalCheckpointWorkerQuotaWithNoOldEligibleCopiesDoesNotSpinOrWriteCleanupJournal(t *testing.T) {
	s, _, seeds := automaticCheckpointService(t, true)
	automaticCurrent(t, s, 7, 9007199254741259)
	workerWait(t, func() bool { return s.checkpointAutoNoop.Load() > 0 })
	if s.checkpointAutoCompletions.Load() != 0 {
		t.Fatal("young protected backups were reclaimed")
	}
	dir := filepath.Join(s.cfg.Config.State.Path, "checkpoints", "evm-31337")
	if _, e := os.Stat(filepath.Join(dir, "retirements")); !os.IsNotExist(e) {
		t.Fatal("no-op plans accumulated journal", e)
	}
	for _, p := range seeds {
		if _, e := checkpoint.ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal(e)
		}
	}
	if _, ready := autoCatalog(s); ready {
		t.Fatal("quota rejection fabricated current checkpoint")
	}
	// A completed cycle sleeps, rather than making unbounded attempts to free space.
	before := s.checkpointAutoAttempts.Load()
	time.Sleep(150 * time.Millisecond)
	after := s.checkpointAutoAttempts.Load()
	if after-before > 2 {
		t.Fatal("automatic cleanup busy-looped", before, after)
	}
	s.Close()
}

func TestNormalCheckpointAutomaticPendingRecoveryAndQuotaCleanupAreSeparateFiniteCycles(t *testing.T) {
	// Let the real startup cycle settle without a fabricated cursor, then exercise
	// the exact worker cycle synchronously while its next configured tick is far away.
	s, _, seeds := automaticCheckpointServiceOptions(t, false, 3600000, 13, true)
	workerWait(t, func() bool { return s.checkpointCycles.Load() > 0 })
	current := automaticCurrent(t, s, 7, 9007199254741267)
	chains := s.cfg.Config.Chains
	result, code := s.publishCheckpointChains(context.Background(), chains, true)
	if code != "CHECKPOINT_LOCAL_BUDGET_REJECTED" || len(result.Public) != 0 || s.checkpointAutoCompletions.Load() != 1 {
		t.Fatal("one cycle resumed and reselected another operation", code, s.checkpointAutoCompletions.Load())
	}
	dir := filepath.Join(s.cfg.Config.State.Path, "checkpoints", "evm-31337")
	c := s.cfg.Config.Journal.CheckpointStore
	remote, e := checkpoint.NewRemote(*c, s.cfg.Config.Journal.SourceAuthTokens[c.BaseURL], s.cfg.Config.Journal.MaxBytes, 5*time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer remote.Close()
	first, e := remote.ReclaimStatus(dir)
	if e != nil || !first.Applied || first.TotalObjects != 2 {
		t.Fatal(first, e)
	}
	if _, ok := autoCatalog(s); ok {
		t.Fatal("failed budget cycle advertised current state")
	}
	result, code = s.publishCheckpointChains(context.Background(), chains, true)
	if code != "" || len(result.Public) != 1 || s.checkpointAutoCompletions.Load() != 2 {
		t.Fatal("next cycle did not advance bounded maintenance", code, s.checkpointAutoCompletions.Load())
	}
	final, e := remote.ReclaimStatus(dir)
	if e != nil || final.TotalObjects != 4 {
		t.Fatal(final, e)
	}
	if out := os.Getenv("XONE_CHECKPOINT_AUTO_RECLAIM_OUTPUT"); out != "" {
		if e = os.MkdirAll(out, 0700); e != nil {
			t.Fatal(e)
		}
		raw, e := json.MarshalIndent(map[string]any{"first_cycle_code": "CHECKPOINT_LOCAL_BUDGET_REJECTED", "first_cycle_completed_operation": first, "second_cycle_code": code, "second_cycle_completed_operation": final, "completed_operations": s.checkpointAutoCompletions.Load(), "verification_scope": "EXACT_NORMAL_WORKER_CYCLE_PENDING_RECOVERY_AND_NEW_SELECTION_ARE_DISTINCT_BOUNDED_ROUNDS"}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(out, "automatic-pending-cycle-boundaries.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}

	p, ok := autoCatalog(s)
	if !ok || p.Reference.Reference != current.Snapshot.Reference {
		t.Fatal("current archive never published", ok)
	}
	for _, seed := range seeds[4:] {
		if _, e = checkpoint.ReadBootstrap(seed.ManifestPath, seed.ManifestSHA256, 8<<20); e != nil {
			t.Fatal(e)
		}
	}
	s.Close()
}

func TestNormalCheckpointAutomaticReclaimRequiresLiveRemoteBackupAndRecoversLater(t *testing.T) {
	s, objects, seeds := automaticCheckpointService(t, false)
	objects.mu.Lock()
	objects.fail = true
	objects.mu.Unlock()
	current := automaticCurrent(t, s, 7, 9007199254741273)
	workerWait(t, func() bool { return s.checkpointAutoFailures.Load() > 0 })
	if s.checkpointAutoCompletions.Load() != 0 {
		t.Fatal("failed remote backups qualified cleanup")
	}
	for _, p := range seeds {
		if _, e := checkpoint.ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal("remote outage consumed local backup", e)
		}
	}
	if _, ok := autoCatalog(s); ok {
		t.Fatal("failed cleanup fabricated current checkpoint")
	}
	objects.mu.Lock()
	objects.fail = false
	objects.mu.Unlock()
	workerWait(t, func() bool { p, ok := autoCatalog(s); return ok && p.Reference.Reference == current.Snapshot.Reference })
	if s.checkpointAutoCompletions.Load() != 1 {
		t.Fatal(s.checkpointAutoCompletions.Load())
	}
	s.Close()
}
func TestNormalCheckpointAutomaticReclaimCloseCancelsBlockedRemoteBeforeLocalRelease(t *testing.T) {
	s, objects, seeds := automaticCheckpointService(t, false)
	objects.mu.Lock()
	objects.hold = true
	objects.mu.Unlock()
	automaticCurrent(t, s, 7, 9007199254741279)
	select {
	case <-objects.entered:
	case <-time.After(4 * time.Second):
		t.Fatal("automatic maintenance never reached backup read")
	}
	done := make(chan struct{})
	go func() { s.Close(); close(done) }()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("automatic cleanup prevented shutdown")
	}
	workerWait(t, func() bool {
		objects.mu.Lock()
		defer objects.mu.Unlock()
		return objects.cancelled > 0 && objects.active == 0
	})
	for _, p := range seeds {
		if _, e := checkpoint.ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal("cancelled automatic read removed local data", e)
		}
	}
	if s.checkpointAutoCompletions.Load() != 0 || s.checkpointActive.Load() {
		t.Fatal("closed worker reported a new completion")
	}
}
func TestNormalCheckpointManualPublicationDoesNotSilentlyInvokeAutomaticReclamation(t *testing.T) {
	s, _, seeds := automaticCheckpointServiceOptions(t, false, 3600000, 19, false)
	workerWait(t, func() bool { return s.checkpointCycles.Load() > 0 })
	automaticCurrent(t, s, 7, 9007199254741283)
	w := workerRequest(s, context.Background(), "POST", "/internal/checkpoints")
	if w.Code != 503 || s.checkpointAutoAttempts.Load() != 0 {
		t.Fatal("manual publication gained a hidden destructive step", w.Code, w.Body.String(), s.checkpointAutoAttempts.Load())
	}
	for _, p := range seeds {
		if _, e := checkpoint.ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal(e)
		}
	}
	s.Close()
}
