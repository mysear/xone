package bootstrap

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func fileScopeFixture(t *testing.T) (*ScopeController, *FileScopeStore, *scopeSource, EnsurePoolScopeRequest, *uint64, string, FileScopeLimits) {
	t.Helper()
	old, _, src, req := scopeFixture(t)
	now := uint64(1000)
	root := filepath.Join(t.TempDir(), "scope")
	limits := FileScopeLimits{MaxJobs: 2, MaxAccountBytes: 8, MaxRetainedJobs: 8, MaxSnapshotBytes: 1 << 20, AttemptLeaseMS: 10, MaxAuthorityLeaseMS: 100}
	store, e := OpenFileScopeStore(root, req.Fence, limits, func() uint64 { return now })
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { store.Close() })
	if _, e = store.Current(context.Background(), req.Fence.ChainKey); !errors.Is(e, ErrFenceChanged) {
		t.Fatal("opening must require fresh authority", e)
	}
	if e = store.Authorize(context.Background(), req.Fence, 1100); e != nil {
		t.Fatal(e)
	}
	ctl, e := NewScopeController(old.config, store, func() uint64 { return now })
	if e != nil {
		t.Fatal(e)
	}
	return ctl, store, src, req, &now, root, limits
}
func TestFileScopeRestartRequiresFreshRecovery(t *testing.T) {
	ctl, store, src, req, now, root, limits := fileScopeFixture(t)
	job, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil {
		t.Fatal(e)
	}
	if e = ctl.Recover(context.Background(), job.ID, src); e != nil {
		t.Fatal(e)
	}
	saved, e := store.Job(job.ID)
	if e != nil || saved.Phase != ScopedReady {
		t.Fatal(saved, e)
	}
	data, e := store.RecoveryData(job.ID)
	if e != nil || len(data.Accounts) != 2 {
		t.Fatal(data, e)
	}
	if e = store.Close(); e != nil {
		t.Fatal(e)
	}
	reopened, e := OpenFileScopeStore(root, req.Fence, limits, func() uint64 { return *now })
	if e != nil {
		t.Fatal(e)
	}
	defer reopened.Close()
	saved, e = reopened.Job(job.ID)
	if e != nil || saved.Phase == ScopedReady || saved.Failure != "RESTART_REVALIDATION_REQUIRED" {
		t.Fatal(saved, e)
	}
	if _, e = reopened.Current(context.Background(), req.Fence.ChainKey); !errors.Is(e, ErrFenceChanged) {
		t.Fatal(e)
	}
	if e = reopened.Authorize(context.Background(), req.Fence, 1100); e != nil {
		t.Fatal(e)
	}
	again, e := NewScopeController(ctl.config, reopened, func() uint64 { return *now })
	if e != nil {
		t.Fatal(e)
	}
	if e = again.Recover(context.Background(), job.ID, src); e != nil {
		t.Fatal(e)
	}
	if len(src.order) != 12 {
		t.Fatal("restart trusted old ready", src.order)
	}
}
func TestFileScopeLockChecksumAndBoundaries(t *testing.T) {
	_, store, _, req, now, root, limits := fileScopeFixture(t)
	if _, e := OpenFileScopeStore(root, req.Fence, limits, func() uint64 { return *now }); !errors.Is(e, ErrScopeStoreLocked) {
		t.Fatal("exclusive writer required", e)
	}
	store.Close()
	state := filepath.Join(root, "state.json")
	b, e := os.ReadFile(state)
	if e != nil {
		t.Fatal(e)
	}
	b[len(b)/2] ^= 1
	if e = os.WriteFile(state, b, 0600); e != nil {
		t.Fatal(e)
	}
	if _, e = OpenFileScopeStore(root, req.Fence, limits, func() uint64 { return *now }); !errors.Is(e, ErrScopeStoreCorrupt) {
		t.Fatal("corruption accepted", e)
	}
	link := filepath.Join(t.TempDir(), "link")
	if e = os.Symlink(root, link); e != nil {
		t.Fatal(e)
	}
	if _, e = OpenFileScopeStore(link, req.Fence, limits, func() uint64 { return *now }); !errors.Is(e, ErrScopeStoreUnsafe) {
		t.Fatal("symlink root", e)
	}
	other := t.TempDir()
	if e = os.Symlink(state, filepath.Join(other, "lock")); e != nil {
		t.Fatal(e)
	}
	if _, e = OpenFileScopeStore(other, req.Fence, limits, func() uint64 { return *now }); !errors.Is(e, ErrScopeStoreUnsafe) {
		t.Fatal("symlink lock", e)
	}
}
func TestFileScopeAttemptLeaseCapacityAndConflict(t *testing.T) {
	ctl, store, _, req, now, _, _ := fileScopeFixture(t)
	ctx := context.Background()
	j, e := ctl.EnsurePoolScope(ctx, req)
	if e != nil {
		t.Fatal(e)
	}
	dup, e := ctl.EnsurePoolScope(ctx, req)
	if e != nil || dup.ID != j.ID {
		t.Fatal(e)
	}
	first, e := store.Claim(ctx, j.ID, req.Fence)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = store.Claim(ctx, j.ID, req.Fence); !errors.Is(e, ErrBusy) {
		t.Fatal(e)
	}
	*now += 11
	second, e := store.Claim(ctx, j.ID, req.Fence)
	if e != nil || second.Attempt != first.Attempt+1 {
		t.Fatal(second, e)
	}
	first.Phase = Restoring
	if e = store.Checkpoint(ctx, first); !errors.Is(e, ErrScopeStoreConflict) {
		t.Fatal("old attempt wrote", e)
	}
	second.Phase = TailAligning
	if e = store.Checkpoint(ctx, second); !errors.Is(e, ErrScopeStoreConflict) {
		t.Fatal("illegal phase jump", e)
	}
	var wg sync.WaitGroup
	var accepted int
	var mu sync.Mutex
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			r := req
			r.Anchor.Hash = string(rune('a' + i))
			_, err := ctl.EnsurePoolScope(ctx, r)
			mu.Lock()
			defer mu.Unlock()
			if err == nil {
				accepted++
			} else if !errors.Is(err, ErrBudget) {
				t.Error(err)
			}
		}(i)
	}
	wg.Wait()
	if accepted != 1 {
		t.Fatal("quota overrun", accepted)
	}
}
func TestFileScopePublishCancelFenceAndAuthority(t *testing.T) {
	for _, kind := range []string{"cancel", "fence", "authority"} {
		t.Run(kind, func(t *testing.T) {
			ctl, store, src, req, now, _, _ := fileScopeFixture(t)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			src.hook = func() {
				switch kind {
				case "cancel":
					cancel()
				case "fence":
					f := req.Fence
					f.Generation++
					if e := store.Authorize(context.Background(), f, 1100); e != nil {
						t.Fatal(e)
					}
				case "authority":
					*now = 1101
				}
			}
			j, e := ctl.EnsurePoolScope(ctx, req)
			if e != nil {
				t.Fatal(e)
			}
			if e = ctl.Recover(ctx, j.ID, src); e == nil {
				t.Fatal("published despite lost authority")
			}
			if _, e = store.RecoveryData(j.ID); e == nil {
				t.Fatal("partial data visible")
			}
		})
	}
}
func TestFileHistoryDurableIsolatedAndBounded(t *testing.T) {
	root := filepath.Join(t.TempDir(), "history")
	limits := FileHistoryLimits{MaxRecords: 2, MaxBytes: 3, MaxSnapshotBytes: 1 << 20}
	store, e := OpenFileHistoryStore(root, "sol", limits)
	if e != nil {
		t.Fatal(e)
	}
	ctx := context.Background()
	r := HistoryRecord{ChainKey: "sol", Namespace: "history_replay", Cursor: 2, Payload: []byte{1}}
	budget := HistoryLimits{MaxJobs: 1, MaxBytes: 3}
	if e = store.AppendHistory(ctx, r, budget); e != nil {
		t.Fatal(e)
	}
	if e = store.AppendHistory(ctx, r, budget); e != nil {
		t.Fatal(e)
	}
	r.Payload = []byte{2}
	if e = store.AppendHistory(ctx, r, budget); !errors.Is(e, ErrScopeStoreConflict) {
		t.Fatal(e)
	}
	store.Close()
	store, e = OpenFileHistoryStore(root, "sol", limits)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	records, e := store.Records()
	if e != nil || len(records) != 1 || records[0].Cursor != 2 || records[0].Payload[0] != 1 {
		t.Fatal(records, e)
	}
	r.Cursor = 3
	r.Payload = []byte{2, 3, 4}
	if e = store.AppendHistory(ctx, r, budget); !errors.Is(e, ErrBudget) {
		t.Fatal(e)
	}
	r.Namespace = "online"
	if e = store.AppendHistory(ctx, r, budget); !errors.Is(e, ErrHistoryNamespace) {
		t.Fatal(e)
	}
}

func TestFileScopeRejectsReservationUndercharge(t *testing.T) {
	ctl, store, _, req, _, _, _ := fileScopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	req.Anchor.Hash = "next"
	j.ID = ""
	j.Anchor = req.Anchor
	j.ID = scopeJobID(j)
	j.ReservedBytes = 1
	if _, e := store.Admit(context.Background(), Admission{Job: j, Limits: j.Budget}); !errors.Is(e, ErrScopeStoreConflict) {
		t.Fatalf("undercharged dependencies: %v", e)
	}
}
func TestFileScopePublishUsesPersistedStage(t *testing.T) {
	ctl, s, src, r, _, _, _ := fileScopeFixture(t)
	ctx := context.Background()
	j, _ := ctl.EnsurePoolScope(ctx, r)
	j, _ = s.Claim(ctx, j.ID, r.Fence)
	j.Phase = LiveBuffering
	if e := s.Checkpoint(ctx, j); e != nil {
		t.Fatal(e)
	}
	j.BufferID = "session"
	j.Phase = BaselineAcquiring
	if e := s.Checkpoint(ctx, j); e != nil {
		t.Fatal(e)
	}
	j.BaselineHash = src.candidates[0].ContentHash
	j.Cursor = 10
	j.Phase = Restoring
	if e := s.Checkpoint(ctx, j); e != nil {
		t.Fatal(e)
	}
	j.RestoredAccounts = 2
	j.Phase = TailAligning
	if e := s.Checkpoint(ctx, j); e != nil {
		t.Fatal(e)
	}
	j.Cursor = j.Anchor.Cursor
	j.BaselineHash = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
	p := Publication{Fence: j.Fence, Anchor: j.Anchor, PoolIDs: j.PoolIDs, Dependencies: j.Dependencies, BaselineHash: j.BaselineHash, BufferID: j.BufferID, Cursor: j.Cursor}
	for _, d := range j.Dependencies {
		p.Accounts = append(p.Accounts, ScopeAccount{ID: d.ID, Data: src.data[d.ID], Anchor: j.Anchor})
	}
	if e := s.Publish(ctx, j, p); !errors.Is(e, ErrScopeStoreConflict) {
		t.Fatalf("changed manifest after verification: %v", e)
	}
}

type productionTransactions struct {
	*scopeStore
	proof OnlineScopeCommit
}

func (s *productionTransactions) OnlineCommit(context.Context, string) (OnlineScopeCommit, error) {
	return s.proof, nil
}
func TestProductionStoreDelegatesSingleOnlineBackend(t *testing.T) {
	old, memory, src, req := scopeFixture(t)
	tx := &productionTransactions{scopeStore: memory}
	store, e := NewProductionStore(tx)
	if e != nil {
		t.Fatal(e)
	}
	ctl, e := NewScopeController(old.config, store, func() uint64 { return 1000 })
	if e != nil {
		t.Fatal(e)
	}
	j, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil {
		t.Fatal(e)
	}
	if e = ctl.Recover(context.Background(), j.ID, src); e != nil {
		t.Fatal(e)
	}
	tx.proof = OnlineScopeCommit{JobID: j.ID, Fence: req.Fence, Cursor: req.Anchor.Cursor, CommitID: "owner-prepared-commit"}
	if _, e = store.OnlineCommit(context.Background(), j.ID); e != nil {
		t.Fatal(e)
	}
	memory.current.Generation++
	if _, e = store.OnlineCommit(context.Background(), j.ID); !errors.Is(e, ErrFenceChanged) {
		t.Fatal(e)
	}
}

func TestFileScopeProcessHelper(t *testing.T) {
	root := os.Getenv("SCOPE_STORE_CHILD_ROOT")
	if root == "" {
		return
	}
	old, _, _, req := scopeFixture(t)
	limits := FileScopeLimits{MaxJobs: 2, MaxAccountBytes: 8, MaxRetainedJobs: 8, MaxSnapshotBytes: 1 << 20, AttemptLeaseMS: 10, MaxAuthorityLeaseMS: 100}
	store, e := OpenFileScopeStore(root, req.Fence, limits, func() uint64 { return 1000 })
	if e != nil {
		t.Fatal(e)
	}
	if e = store.Authorize(context.Background(), req.Fence, 1100); e != nil {
		t.Fatal(e)
	}
	ctl, e := NewScopeController(old.config, store, func() uint64 { return 1000 })
	if e != nil {
		t.Fatal(e)
	}
	j, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil {
		t.Fatal(e)
	}
	j, e = store.Claim(context.Background(), j.ID, j.Fence)
	if e != nil {
		t.Fatal(e)
	}
	j.Phase = LiveBuffering
	if e = store.Checkpoint(context.Background(), j); e != nil {
		t.Fatal(e)
	}
	fmt.Println("SCOPE_SAVED " + j.ID)
	for {
		time.Sleep(time.Minute)
	}
}
func TestFileScopeKilledProcessReopensWithoutOldReady(t *testing.T) {
	ctl, store, src, req, now, root, limits := fileScopeFixture(t)
	seed, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil {
		t.Fatal(e)
	}
	if e = ctl.Recover(context.Background(), seed.ID, src); e != nil {
		t.Fatal(e)
	}
	store.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, os.Args[0], "-test.run=^TestFileScopeProcessHelper$")
	cmd.Env = append(os.Environ(), "SCOPE_STORE_CHILD_ROOT="+root)
	stdout, e := cmd.StdoutPipe()
	if e != nil {
		t.Fatal(e)
	}
	if e = cmd.Start(); e != nil {
		t.Fatal(e)
	}
	defer func() {
		if cmd.ProcessState == nil {
			cmd.Process.Kill()
			cmd.Wait()
		}
	}()
	scanner := bufio.NewScanner(stdout)
	if !scanner.Scan() {
		t.Fatal("child did not persist", scanner.Err())
	}
	line := scanner.Text()
	const prefix = "SCOPE_SAVED "
	if len(line) <= len(prefix) || line[:len(prefix)] != prefix {
		t.Fatal(line)
	}
	id := line[len(prefix):]
	if e = cmd.Process.Kill(); e != nil {
		t.Fatal(e)
	}
	_ = cmd.Wait()
	reopened, e := OpenFileScopeStore(root, req.Fence, limits, func() uint64 { return *now })
	if e != nil {
		t.Fatal(e)
	}
	defer reopened.Close()
	j, e := reopened.Job(id)
	if e != nil || j.Attempt != 2 || j.Phase == ScopedReady {
		t.Fatal(j, e)
	}
	if e = reopened.Authorize(context.Background(), req.Fence, 1100); e != nil {
		t.Fatal(e)
	}
	next, e := reopened.Claim(context.Background(), id, req.Fence)
	if e != nil || next.Attempt != 3 {
		t.Fatal(next, e)
	}
}

func TestScopeRegressionFailedFencePersistenceRevokesHandle(t *testing.T) {
	ctl, s, src, r, _, root, _ := fileScopeFixture(t)
	ctx := context.Background()
	j, err := ctl.EnsurePoolScope(ctx, r)
	if err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(ctx, j.ID, src); err != nil {
		t.Fatal(err)
	}
	pending := filepath.Join(root, "pending")
	if err = os.WriteFile(pending, []byte("pre-rename failure"), 0600); err != nil {
		t.Fatal(err)
	}
	next := r.Fence
	next.Generation++
	if err = s.Authorize(ctx, next, 1100); err == nil {
		t.Fatal("pending conflict did not fire")
	}
	if _, err = s.Current(ctx, r.Fence.ChainKey); err == nil {
		t.Error("failed fence advance kept old authority")
	}
	if _, err = s.RecoveryData(j.ID); err == nil {
		t.Error("old ready data survived failed fence advance")
	}
	if _, err = s.Job(j.ID); err == nil {
		t.Error("old ready job is still readable")
	}
	if err = ctl.Recover(ctx, j.ID, src); err == nil {
		t.Error("old controller recovered after accepted fence advance")
	}
	if err = os.Remove(pending); err != nil {
		t.Fatal(err)
	}
	if err = s.Authorize(ctx, r.Fence, 1100); err == nil {
		t.Error("handle accepted old fence again after failed advance")
	}
	if err = s.Authorize(ctx, next, 1100); err != nil {
		t.Fatal("retry valid new fence", err)
	}
	current, err := s.Current(ctx, next.ChainKey)
	if err != nil || current != next {
		t.Fatal(current, err)
	}
}
func TestScopeRegressionZeroEventTailPreservesManifest(t *testing.T) {
	for _, mutate := range []bool{false, true} {
		t.Run(fmt.Sprint(mutate), func(t *testing.T) {
			ctl, s, src, r, _, _, _ := fileScopeFixture(t)
			src.candidates[0].Anchor = r.Anchor
			src.candidates[0].ContentHash = BaselineHash(src.candidates[0])
			src.tailOverride = func(tail *ScopeTail) {
				tail.Events = 0
				if mutate {
					tail.Accounts[0].Data = []byte{99}
				}
			}
			j, err := ctl.EnsurePoolScope(context.Background(), r)
			if err != nil {
				t.Fatal(err)
			}
			err = ctl.Recover(context.Background(), j.ID, src)
			if mutate {
				if err == nil {
					t.Error("zero-event tail replaced manifest-verified bytes")
				}
				if _, err = s.RecoveryData(j.ID); err == nil {
					t.Error("substituted data became ready")
				}
			} else {
				if err != nil {
					t.Fatal(err)
				}
				p, err := s.RecoveryData(j.ID)
				if err != nil || p.Accounts[0].Data[0] != 1 {
					t.Fatal(p, err)
				}
			}
		})
	}
}
func TestScopeRegressionControllerReadyRechecksAge(t *testing.T) {
	ctl, _, src, r := scopeFixture(t)
	j, err := ctl.EnsurePoolScope(context.Background(), r)
	if err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(context.Background(), j.ID, src); err != nil {
		t.Fatal(err)
	}
	calls := len(src.order)
	ctl.now = func() uint64 { return 1040 }
	if err = ctl.Recover(context.Background(), j.ID, src); err != nil {
		t.Fatal("inclusive original deadline", err)
	}
	ctl.now = func() uint64 { return 1050 }
	if err = ctl.Recover(context.Background(), j.ID, src); err == nil {
		t.Error("controller returned stale ready from store")
	}
	if len(src.order) != calls {
		t.Fatal("stale fixed-anchor retry performed source IO")
	}
}
func TestScopeRegressionFileReadyReadsRecheckAge(t *testing.T) {
	ctl, s, src, r, clock, _, _ := fileScopeFixture(t)
	ctx := context.Background()
	j, err := ctl.EnsurePoolScope(ctx, r)
	if err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(ctx, j.ID, src); err != nil {
		t.Fatal(err)
	}
	*clock = 1040
	if _, err = s.RecoveryData(j.ID); err != nil {
		t.Fatal("inclusive original deadline", err)
	}
	*clock = 1050
	calls := len(src.order)
	if err = ctl.Recover(ctx, j.ID, src); err == nil {
		t.Error("Recover returned stale ready")
	}
	if _, err = s.Claim(ctx, j.ID, r.Fence); err == nil {
		t.Error("Claim returned stale ready")
	}
	if _, err = s.Job(j.ID); err == nil {
		t.Error("Job returned stale ready")
	}
	if _, err = s.RecoveryData(j.ID); err == nil {
		t.Error("RecoveryData returned stale data")
	}
	if _, err = ctl.EnsurePoolScope(ctx, r); err == nil {
		t.Error("idempotent admission returned stale ready")
	}
	if len(src.order) != calls {
		t.Error("stale ready performed source IO")
	}
	if _, err = s.Current(ctx, r.Fence.ChainKey); err != nil {
		t.Fatal("freshness test must not expire owner lease", err)
	}
}
func TestScopeRegressionCommitRechecksAgeAfterSnapshotSync(t *testing.T) {
	old, _, src, r := scopeFixture(t)
	root := filepath.Join(t.TempDir(), "scope")
	clock := uint64(1000)
	now := func() uint64 {
		pending, _ := os.ReadFile(filepath.Join(root, "pending"))
		if bytes.Contains(pending, []byte("SCOPED_READY")) {
			clock = 1050
		}
		return clock
	}
	limits := FileScopeLimits{MaxJobs: 2, MaxAccountBytes: 8, MaxRetainedJobs: 8, MaxSnapshotBytes: 1 << 20, AttemptLeaseMS: 100, MaxAuthorityLeaseMS: 100}
	s, err := OpenFileScopeStore(root, r.Fence, limits, now)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	ctx := context.Background()
	if err = s.Authorize(ctx, r.Fence, 1100); err != nil {
		t.Fatal(err)
	}
	ctl, err := NewScopeController(old.config, s, now)
	if err != nil {
		t.Fatal(err)
	}
	j, err := ctl.EnsurePoolScope(ctx, r)
	if err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(ctx, j.ID, src); !errors.Is(err, ErrScopeStale) {
		t.Fatalf("expected freshness rejection with original leases still valid: %v", err)
	}
	if clock != 1050 {
		t.Fatal("commit-boundary fault did not fire")
	}
	raw, err := os.ReadFile(filepath.Join(root, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	var envelope scopeEnvelope
	if err = json.Unmarshal(raw, &envelope); err != nil {
		t.Fatal(err)
	}
	var disk scopeDisk
	if err = json.Unmarshal([]byte(envelope.Payload), &disk); err != nil {
		t.Fatal(err)
	}
	if disk.Jobs[j.ID].Job.Phase == ScopedReady || len(disk.Data) != 0 {
		t.Error("expired READY was persisted")
	}
	if _, err = s.RecoveryData(j.ID); err == nil {
		t.Error("expired recovery data returned")
	}
}

func TestScopeVersionIdentityRequiresNumericFenceAdvance(t *testing.T) {
	for _, field := range []string{"adapter", "schema"} {
		t.Run(field, func(t *testing.T) {
			ctl, s, src, r, _, root, _ := fileScopeFixture(t)
			ctx := context.Background()
			j, err := ctl.EnsurePoolScope(ctx, r)
			if err != nil {
				t.Fatal(err)
			}
			if err = ctl.Recover(ctx, j.ID, src); err != nil {
				t.Fatal(err)
			}
			next := r.Fence
			if field == "adapter" {
				next.AdapterVersion = "a2"
			} else {
				next.SchemaVersion = "s2"
			}
			pending := filepath.Join(root, "pending")
			if err = os.WriteFile(pending, []byte("blocked"), 0600); err != nil {
				t.Fatal(err)
			}
			if err = s.Authorize(ctx, next, 1100); !errors.Is(err, ErrFenceChanged) {
				t.Fatalf("equal numeric fence must reject version replacement before persistence: %v", err)
			}
			current, err := s.Current(ctx, r.Fence.ChainKey)
			if err != nil || current != r.Fence {
				t.Fatalf("rejected replacement changed accepted authority: %v %v", current, err)
			}
			if _, err = s.RecoveryData(j.ID); err != nil {
				t.Fatal("unaccepted replacement revoked valid ready", err)
			}
			if err = os.Remove(pending); err != nil {
				t.Fatal(err)
			}
			if err = s.Authorize(ctx, r.Fence, 1100); err != nil {
				t.Fatal(err)
			}
			if err = ctl.Recover(ctx, j.ID, src); err != nil {
				t.Fatal(err)
			}
		})
	}
}
func TestScopeVersionIdentityAdvancedFenceFailureAndRetry(t *testing.T) {
	for _, field := range []string{"adapter", "schema"} {
		for _, axis := range []string{"assignment", "generation", "canonical", "writer"} {
			t.Run(field+"/"+axis, func(t *testing.T) {
				ctl, s, src, r, _, root, _ := fileScopeFixture(t)
				ctx := context.Background()
				j, err := ctl.EnsurePoolScope(ctx, r)
				if err != nil {
					t.Fatal(err)
				}
				if err = ctl.Recover(ctx, j.ID, src); err != nil {
					t.Fatal(err)
				}
				next := r.Fence
				// Lexically smaller labels are valid identities when the numeric fence advances.
				if field == "adapter" {
					next.AdapterVersion = "a0"
				} else {
					next.SchemaVersion = "s0"
				}
				switch axis {
				case "assignment":
					next.Assignment++
				case "generation":
					next.Generation++
				case "canonical":
					next.CanonicalEpoch++
				case "writer":
					next.WriterEpoch++
				}
				pending := filepath.Join(root, "pending")
				if err = os.WriteFile(pending, []byte("blocked"), 0600); err != nil {
					t.Fatal(err)
				}
				err = s.Authorize(ctx, next, 1100)
				if !errors.Is(err, os.ErrExist) {
					t.Fatalf("advanced identity must reach injected persistence failure: %v", err)
				}
				if _, err = s.Current(ctx, r.Fence.ChainKey); !errors.Is(err, ErrFenceChanged) {
					t.Fatal("accepted replacement retained old authority", err)
				}
				if _, err = s.RecoveryData(j.ID); err == nil {
					t.Fatal("old ready survived accepted replacement")
				}
				if err = ctl.Recover(ctx, j.ID, src); err == nil {
					t.Fatal("old controller recovered")
				}
				if err = os.Remove(pending); err != nil {
					t.Fatal(err)
				}
				if err = s.Authorize(ctx, r.Fence, 1100); !errors.Is(err, ErrFenceChanged) {
					t.Fatal("old identity returned after failed replacement", err)
				}
				if err = s.Authorize(ctx, next, 1100); err != nil {
					t.Fatal("retry accepted new identity", err)
				}
				current, err := s.Current(ctx, next.ChainKey)
				if err != nil || current != next {
					t.Fatal(current, err)
				}
			})
		}
	}
}
