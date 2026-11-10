package bootstrap

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"sync"
	"syscall"
)

var (
	ErrScopeStoreLocked   = errors.New("SCOPE_STORE_LOCKED")
	ErrScopeStoreCorrupt  = errors.New("SCOPE_STORE_CORRUPT")
	ErrScopeStoreUnsafe   = errors.New("SCOPE_STORE_UNSAFE_PATH")
	ErrScopeStoreConflict = errors.New("SCOPE_STORE_CONFLICT")
	ErrScopeStoreClosed   = errors.New("SCOPE_STORE_CLOSED_OR_UNCERTAIN")
)

// FileScopeStore is a real durable recovery-data store. SCOPED_READY here means
// the entire recovery dataset is on disk, NEVER that the service/router can quote.
// Production online publication must use its existing prepared/journal transaction
// and validate the same fence, attempt, lease and cursor there. Two file/DB writes
// cannot be composed into an atomic online commit by calling this store first.
// Unix local filesystems only: advisory flock, file fsync, atomic rename, dir fsync.
// The configured root must be private; do not use shared/network filesystems.
type FileScopeLimits struct{ MaxJobs, MaxAccountBytes, MaxRetainedJobs, MaxSnapshotBytes, AttemptLeaseMS, MaxAuthorityLeaseMS uint64 }
type storedScopeJob struct {
	Job        ScopeJob `json:"job"`
	Session    uint64   `json:"session"`
	DeadlineMS uint64   `json:"deadline_ms"`
}
type scopeDisk struct {
	Session uint64                    `json:"session"`
	Fence   Fence                     `json:"fence"`
	Jobs    map[string]storedScopeJob `json:"jobs"`
	Data    map[string]Publication    `json:"data"`
}
type FileScopeStore struct {
	mu             sync.Mutex
	file           *scopeAtomicFile
	disk           scopeDisk
	limits         FileScopeLimits
	now            func() uint64
	authorityUntil uint64
	fenceFloor     Fence // handle-local accepted authority high-water, including failed writes
}
type scopeEnvelope struct {
	Format   string `json:"format"`
	Checksum string `json:"checksum"`
	Payload  string `json:"payload"`
}
type scopeAtomicFile struct {
	root, format      string
	max               uint64
	lock, dir         *os.File
	closed, uncertain bool
	external          EmbeddedScopeBackend
}

func safeScopeEntry(path string) (os.FileInfo, error) {
	info, e := os.Lstat(path)
	if e != nil {
		return nil, e
	}
	if !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 {
		return nil, ErrScopeStoreUnsafe
	}
	if st, ok := info.Sys().(*syscall.Stat_t); !ok || st.Nlink != 1 || uint32(os.Geteuid()) != st.Uid {
		return nil, ErrScopeStoreUnsafe
	}
	return info, nil
}
func openScopeAtomic(root, format string, max uint64) (*scopeAtomicFile, []byte, error) {
	if !filepath.IsAbs(root) || max < 256 || max > 64<<20 {
		return nil, nil, ErrScopeStoreUnsafe
	}
	rootCreated := false
	if info, e := os.Lstat(root); e == nil {
		if !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
			return nil, nil, ErrScopeStoreUnsafe
		}
	} else if !os.IsNotExist(e) {
		return nil, nil, e
	} else {
		rootCreated = true
	}
	if e := os.MkdirAll(root, 0700); e != nil {
		return nil, nil, e
	}
	resolved, e := filepath.EvalSymlinks(root)
	if e != nil {
		return nil, nil, e
	}
	root = resolved
	info, e := os.Stat(root)
	if e != nil {
		return nil, nil, e
	}
	if info.Mode().Perm()&0077 != 0 {
		return nil, nil, ErrScopeStoreUnsafe
	}
	if st, ok := info.Sys().(*syscall.Stat_t); !ok || st.Uid != uint32(os.Geteuid()) {
		return nil, nil, ErrScopeStoreUnsafe
	}
	// Persist newly created directory entries too, including recursive parents.
	if rootCreated {
		for parent := filepath.Dir(root); ; parent = filepath.Dir(parent) {
			dir, e := os.Open(parent)
			if e != nil {
				return nil, nil, e
			}
			e = dir.Sync()
			ce := dir.Close()
			if e == nil {
				e = ce
			}
			if e != nil {
				return nil, nil, e
			}
			if parent == filepath.Dir(parent) {
				break
			}
		}
	}
	lockPath := filepath.Join(root, "lock")
	lock, e := os.OpenFile(lockPath, os.O_CREATE|os.O_RDWR|syscall.O_NOFOLLOW, 0600)
	if e != nil {
		return nil, nil, ErrScopeStoreUnsafe
	}
	fail := func(e error) (*scopeAtomicFile, []byte, error) { lock.Close(); return nil, nil, e }
	li, e := safeScopeEntry(lockPath)
	if e != nil {
		return fail(ErrScopeStoreUnsafe)
	}
	fi, e := lock.Stat()
	if e != nil || !os.SameFile(li, fi) {
		return fail(ErrScopeStoreUnsafe)
	}
	if e = syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); e != nil {
		return fail(ErrScopeStoreLocked)
	}
	dir, e := os.Open(root)
	if e != nil {
		return fail(e)
	}
	f := &scopeAtomicFile{root: root, format: format, max: max, lock: lock, dir: dir}
	cleanup := func(e error) (*scopeAtomicFile, []byte, error) { f.close(); return nil, nil, e }
	pending := filepath.Join(root, "pending")
	if _, e := safeScopeEntry(pending); e == nil {
		if e = os.Remove(pending); e != nil {
			return cleanup(e)
		}
	} else if !os.IsNotExist(e) {
		return cleanup(ErrScopeStoreUnsafe)
	}
	state := filepath.Join(root, "state.json")
	si, e := safeScopeEntry(state)
	if os.IsNotExist(e) {
		return f, nil, nil
	}
	if e != nil {
		return cleanup(ErrScopeStoreUnsafe)
	}
	if uint64(si.Size()) > max {
		return cleanup(ErrScopeStoreCorrupt)
	}
	in, e := os.OpenFile(state, os.O_RDONLY|syscall.O_NOFOLLOW, 0)
	if e != nil {
		return cleanup(e)
	}
	stat, e := in.Stat()
	if e != nil || !os.SameFile(si, stat) {
		in.Close()
		return cleanup(ErrScopeStoreUnsafe)
	}
	data, e := io.ReadAll(io.LimitReader(in, int64(max)+1))
	in.Close()
	if e != nil {
		return cleanup(e)
	}
	if uint64(len(data)) > max {
		return cleanup(ErrScopeStoreCorrupt)
	}
	payload, e := decodeScopeEnvelope(data, format, max)
	if e != nil {
		return cleanup(e)
	}
	return f, payload, nil
}

func decodeScopeEnvelope(data []byte, format string, max uint64) ([]byte, error) {
	var envelope scopeEnvelope
	if uint64(len(data)) > max || json.Unmarshal(data, &envelope) != nil || envelope.Format != format || digestScopeBytes([]byte(envelope.Payload)) != envelope.Checksum {
		return nil, ErrScopeStoreCorrupt
	}
	return []byte(envelope.Payload), nil
}
func digestScopeBytes(b []byte) string { return frameRawHash(b) }

func (f *scopeAtomicFile) encode(value any) ([]byte, error) {
	if f.closed || f.uncertain {
		return nil, ErrScopeStoreClosed
	}
	payload, e := json.Marshal(value)
	if e != nil {
		return nil, e
	}
	data, e := json.Marshal(scopeEnvelope{Format: f.format, Checksum: digestScopeBytes(payload), Payload: string(payload)})
	if e != nil {
		return nil, e
	}
	if uint64(len(data)) > f.max {
		return nil, ErrBudget
	}
	return data, nil
}

// write's gate is evaluated after pending fsync, immediately before rename, while
// the caller holds the transaction mutex. A post-rename I/O error poisons this
// handle: the result is uncertain and must be reconciled by reopening, not retried.
func (f *scopeAtomicFile) write(value any, gate func() error) error {
	data, e := f.encode(value)
	if e != nil {
		return e
	}
	if f.external != nil {
		e = f.external.Save(data, gate)
		if e != nil {
			// A backend error may follow a successful owner commit. Never retry
			// from this handle's older in-memory ledger without reconciliation.
			f.uncertain = true
		}
		return e
	}
	path := filepath.Join(f.root, "pending")
	out, e := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY|syscall.O_NOFOLLOW, 0600)
	if e != nil {
		return e
	}
	defer os.Remove(path)
	if _, e = out.Write(data); e == nil {
		e = out.Sync()
	}
	ce := out.Close()
	if e == nil {
		e = ce
	}
	if e != nil {
		return e
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	if e = os.Rename(path, filepath.Join(f.root, "state.json")); e != nil {
		return e
	}
	if e = f.dir.Sync(); e != nil {
		f.uncertain = true
		return e
	}
	return nil
}
func (f *scopeAtomicFile) publish(value any, job ScopeJob, publication Publication, gate func() error) error {
	if f.external == nil {
		return f.write(value, gate)
	}
	data, e := f.encode(value)
	if e != nil {
		return e
	}
	if e = f.external.Publish(data, cloneScope(job), cloneScope(publication), gate); e != nil {
		f.uncertain = true
	}
	return e
}

func (f *scopeAtomicFile) close() error {
	if f.closed {
		return nil
	}
	f.closed = true
	var e, le error
	if f.dir != nil {
		e = f.dir.Close()
	}
	if f.lock != nil {
		le = f.lock.Close()
	}
	if e != nil {
		return e
	}
	return le
}
func cloneScope[T any](v T) T {
	b, _ := json.Marshal(v)
	var out T
	_ = json.Unmarshal(b, &out)
	return out
}

func validScopePhase(p ScopePhase) bool {
	switch p {
	case Admitted, LiveBuffering, BaselineAcquiring, Restoring, TailAligning, ScopedReady, ScopeFailed, HeadFirstRequired:
		return true
	}
	return false
}
func validScopeJobShape(j ScopeJob) bool {
	if !validScopePhase(j.Phase) || !validFence(j.Fence) || !validLimits(j.Budget) || j.Anchor.Hash == "" || len(j.PoolIDs) == 0 || len(j.Dependencies) == 0 || !slices.Equal(j.PoolIDs, canonicalPools(j.PoolIDs)) || j.ID != scopeJobID(j) {
		return false
	}
	var sum uint64
	for i, d := range j.Dependencies {
		if d.ID == "" || d.MaxBytes == 0 || (i > 0 && j.Dependencies[i-1].ID >= d.ID) || sum > j.Budget.MaxAccountBytes || d.MaxBytes > j.Budget.MaxAccountBytes-sum {
			return false
		}
		sum += d.MaxBytes
	}
	return sum == j.ReservedBytes
}
func persistedScopeStage(a, b ScopeJob) bool {
	return a.BufferID == b.BufferID && a.BaselineHash == b.BaselineHash && a.RestoredAccounts == b.RestoredAccounts && reflect.DeepEqual(a.HistoryGap, b.HistoryGap) && a.Failure == b.Failure && b.Failure == "" && a.Cursor <= b.Cursor
}

// ReadinessScope prevents callers from interpreting this file's SCOPED_READY as
// an authoritative online/quote commit. Only recovery data has been persisted.
func (s *FileScopeStore) ReadinessScope() string { return "RECOVERY_DATA_ONLY" }

func validFileScopeLimits(b FileScopeLimits) bool {
	return b.MaxJobs > 0 && b.MaxAccountBytes > 0 && b.MaxRetainedJobs >= b.MaxJobs && b.MaxRetainedJobs <= 100000 && b.MaxSnapshotBytes >= 256 && b.MaxSnapshotBytes <= 64<<20 && b.AttemptLeaseMS > 0 && b.MaxAuthorityLeaseMS > 0
}
func OpenFileScopeStore(root string, fence Fence, limits FileScopeLimits, now func() uint64) (*FileScopeStore, error) {
	if !validFence(fence) || !validFileScopeLimits(limits) || now == nil {
		return nil, ErrScopeData
	}
	file, payload, e := openScopeAtomic(root, "xone-online-recovery-v1", limits.MaxSnapshotBytes)
	if e != nil {
		return nil, e
	}
	return openScopeStore(file, payload, fence, limits, now)
}

// openScopeStore is shared by file and owner-embedded persistence so restart,
// validation and authority revocation have exactly the same semantics.
func openScopeStore(file *scopeAtomicFile, payload []byte, fence Fence, limits FileScopeLimits, now func() uint64) (*FileScopeStore, error) {
	var e error
	fail := func(e error) (*FileScopeStore, error) { file.close(); return nil, e }
	disk := scopeDisk{Fence: fence, Jobs: map[string]storedScopeJob{}, Data: map[string]Publication{}}
	if payload != nil {
		if e = json.Unmarshal(payload, &disk); e != nil || disk.Jobs == nil || disk.Data == nil || !validFence(disk.Fence) || disk.Session == ^uint64(0) || uint64(len(disk.Jobs)) > limits.MaxRetainedJobs {
			return fail(ErrScopeStoreCorrupt)
		}
		if !scopeFenceAdvance(disk.Fence, fence) {
			return fail(ErrFenceChanged)
		}
		for id, entry := range disk.Jobs {
			if id != entry.Job.ID || !validScopeJobShape(entry.Job) {
				return fail(ErrScopeStoreCorrupt)
			}
		}
		for id, p := range disk.Data {
			j, ok := disk.Jobs[id]
			if !ok || !validPublication(j.Job, p) {
				return fail(ErrScopeStoreCorrupt)
			}
		}
	}
	if file.external != nil {
		// Reclaim already-terminal older generations before adding restart
		// metadata, even when the old envelope exactly fills its byte budget.
		// Validate every record first; never prune away corrupt evidence.
		pruneSupersededScope(&disk, fence)
	}
	disk.Session++
	disk.Fence = fence
	for id, entry := range disk.Jobs {
		entry.Job.Phase = ScopeFailed
		entry.Job.Failure = "RESTART_REVALIDATION_REQUIRED"
		entry.DeadlineMS = 0
		disk.Jobs[id] = entry
	}
	if e = file.write(disk, nil); e != nil {
		return fail(e)
	}
	return &FileScopeStore{file: file, disk: disk, limits: limits, now: now, fenceFloor: fence}, nil
}
func scopeFenceAdvance(old, next Fence) bool {
	// Versions are opaque identities, not ordered strings. Replacing either one
	// needs a strictly newer numerical fence; all counters must remain monotonic.
	sameVersions := old.AdapterVersion == next.AdapterVersion && old.SchemaVersion == next.SchemaVersion
	numericAdvance := next.Assignment > old.Assignment || next.Generation > old.Generation || next.CanonicalEpoch > old.CanonicalEpoch || next.WriterEpoch > old.WriterEpoch
	return old.ChainKey == next.ChainKey && old.ChainIdentity == next.ChainIdentity && next.Assignment >= old.Assignment && next.Generation >= old.Generation && next.CanonicalEpoch >= old.CanonicalEpoch && next.WriterEpoch >= old.WriterEpoch && (old.Owner == next.Owner || next.Assignment > old.Assignment) && (sameVersions || numericAdvance)
}
func (s *FileScopeStore) live(ctx context.Context, f Fence) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	if s.file.closed || s.file.uncertain {
		return ErrScopeStoreClosed
	}
	if f != s.disk.Fence || s.authorityUntil <= s.now() {
		return ErrFenceChanged
	}
	return nil
}

// Authorize is called only with the active externally verified owner lease. The
// file store does not acquire a PG lease or Kafka fence. Every reopen requires it.
func (s *FileScopeStore) Authorize(ctx context.Context, f Fence, until uint64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if !validFence(f) || !scopeFenceAdvance(s.fenceFloor, f) {
		return ErrFenceChanged
	}
	gate := func() error {
		if e := ctx.Err(); e != nil {
			return e
		}
		now := s.now()
		if until <= now || until-now > s.limits.MaxAuthorityLeaseMS {
			return ErrFenceChanged
		}
		return nil
	}
	if e := gate(); e != nil {
		return e
	}
	// An accepted replacement fence invalidates the old handle authority BEFORE
	// fallible encoding/fsync/rename. Keep its floor even when persistence fails.
	if f != s.disk.Fence {
		s.authorityUntil = 0
		s.fenceFloor = f
	}
	next := cloneScope(s.disk)
	if f != next.Fence {
		next.Fence = f
		if next.Session == ^uint64(0) {
			return ErrScopeStoreConflict
		}
		next.Session++
		for id, entry := range next.Jobs {
			entry.Job.Phase = ScopeFailed
			entry.Job.Failure = "FENCE_CHANGED"
			entry.DeadlineMS = 0
			next.Jobs[id] = entry
		}
	}
	if e := s.file.write(next, gate); e != nil {
		return e
	}
	s.disk = next
	s.authorityUntil = until
	return nil
}
func (s *FileScopeStore) Current(ctx context.Context, chain string) (Fence, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.currentScope(ctx, chain)
}

// currentScope requires s.mu to be held.
func (s *FileScopeStore) currentScope(ctx context.Context, chain string) (Fence, error) {
	if chain != s.disk.Fence.ChainKey {
		return Fence{}, ErrFenceChanged
	}
	if e := s.live(ctx, s.disk.Fence); e != nil {
		return Fence{}, e
	}
	return s.disk.Fence, nil
}
func (s *FileScopeStore) capacity(d scopeDisk, except string, j ScopeJob, b ScopeLimits) error {
	var jobs, bytes uint64
	maxJobs := minScope(b.MaxJobs, s.limits.MaxJobs)
	maxBytes := minScope(b.MaxAccountBytes, s.limits.MaxAccountBytes)
	for id, entry := range d.Jobs {
		if id == except || entry.Job.Phase.Terminal() {
			continue
		}
		if entry.Job.ReservedBytes > maxBytes-bytes {
			return ErrBudget
		}
		bytes += entry.Job.ReservedBytes
		jobs++
	}
	if jobs >= maxJobs || j.ReservedBytes > maxBytes-bytes {
		return ErrBudget
	}
	return nil
}
func (s *FileScopeStore) Admit(ctx context.Context, a Admission) (ScopeJob, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.admitScope(ctx, a)
}

// admitScope requires s.mu to be held.
func (s *FileScopeStore) admitScope(ctx context.Context, a Admission) (ScopeJob, error) {
	j := cloneScope(a.Job)
	if e := s.live(ctx, j.Fence); e != nil {
		return ScopeJob{}, e
	}
	if j.Phase != Admitted || j.Attempt != 0 || !validScopeJobShape(j) || a.Limits != j.Budget {
		return ScopeJob{}, ErrScopeStoreConflict
	}
	if existing, ok := s.disk.Jobs[j.ID]; ok {
		if !sameScopeIdentity(existing.Job, j) {
			return ScopeJob{}, ErrScopeStoreConflict
		}
		result := cloneScope(existing.Job)
		if result.Phase == ScopedReady {
			if e := s.live(ctx, result.Fence); e != nil {
				return ScopeJob{}, e
			}
			if e := requireScopeFresh(result, s.now()); e != nil {
				return ScopeJob{}, e
			}
		}
		return result, nil
	}
	if uint64(len(s.disk.Jobs)) >= s.limits.MaxRetainedJobs {
		return ScopeJob{}, ErrBudget
	}
	if e := s.capacity(s.disk, "", j, a.Limits); e != nil {
		return ScopeJob{}, e
	}
	next := cloneScope(s.disk)
	next.Jobs[j.ID] = storedScopeJob{Job: j, Session: next.Session}
	if e := s.file.write(next, func() error { return s.live(ctx, j.Fence) }); e != nil {
		return ScopeJob{}, e
	}
	s.disk = next
	return cloneScope(j), nil
}
func sameScopeIdentity(a, b ScopeJob) bool {
	return a.Fence == b.Fence && a.Anchor == b.Anchor && reflect.DeepEqual(a.PoolIDs, b.PoolIDs) && reflect.DeepEqual(a.Dependencies, b.Dependencies) && a.ReservedBytes == b.ReservedBytes
}
func immutableScopeJob(a, b ScopeJob) bool {
	return sameScopeIdentity(a, b) && a.ID == b.ID && a.Budget == b.Budget && a.HistoryAfter == b.HistoryAfter
}
func (s *FileScopeStore) Claim(ctx context.Context, id string, f Fence) (ScopeJob, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.claimScope(ctx, id, f)
}

// claimScope requires s.mu to be held.
func (s *FileScopeStore) claimScope(ctx context.Context, id string, f Fence) (ScopeJob, error) {
	if e := s.live(ctx, f); e != nil {
		return ScopeJob{}, e
	}
	entry, ok := s.disk.Jobs[id]
	if !ok || entry.Job.Fence != f {
		return ScopeJob{}, ErrScopeStoreConflict
	}
	if entry.Job.Phase == ScopedReady && entry.Session == s.disk.Session {
		result := cloneScope(entry.Job)
		if e := s.live(ctx, result.Fence); e != nil {
			return ScopeJob{}, e
		}
		if e := requireScopeFresh(result, s.now()); e != nil {
			return ScopeJob{}, e
		}
		return result, nil
	}
	if !entry.Job.Phase.Terminal() && entry.Job.Attempt > 0 && entry.Session == s.disk.Session && entry.DeadlineMS > s.now() {
		return ScopeJob{}, ErrBusy
	}
	if e := s.capacity(s.disk, id, entry.Job, entry.Job.Budget); e != nil {
		return ScopeJob{}, e
	}
	if entry.Job.Attempt == ^uint64(0) || s.limits.AttemptLeaseMS > ^uint64(0)-s.now() {
		return ScopeJob{}, ErrScopeStoreConflict
	}
	entry.Job.Attempt++
	entry.Job.Phase = Admitted
	entry.Session = s.disk.Session
	entry.DeadlineMS = minScope(s.now()+s.limits.AttemptLeaseMS, s.authorityUntil)
	next := cloneScope(s.disk)
	next.Jobs[id] = entry
	delete(next.Data, id)
	if e := s.file.write(next, func() error { return s.live(ctx, f) }); e != nil {
		return ScopeJob{}, e
	}
	s.disk = next
	return cloneScope(entry.Job), nil
}
func (s *FileScopeStore) attempt(ctx context.Context, j ScopeJob) (storedScopeJob, error) {
	if e := s.live(ctx, j.Fence); e != nil {
		return storedScopeJob{}, e
	}
	entry, ok := s.disk.Jobs[j.ID]
	if !ok || entry.Session != s.disk.Session || entry.Job.Attempt == 0 || entry.Job.Attempt != j.Attempt || entry.DeadlineMS <= s.now() || entry.Job.Phase.Terminal() || !immutableScopeJob(entry.Job, j) {
		return storedScopeJob{}, ErrScopeStoreConflict
	}
	return entry, nil
}
func scopePhaseStep(old, next ScopePhase) bool {
	return old == Admitted && next == LiveBuffering || old == LiveBuffering && next == BaselineAcquiring || old == BaselineAcquiring && next == Restoring || old == Restoring && (next == Restoring || next == TailAligning)
}
func (s *FileScopeStore) Checkpoint(ctx context.Context, j ScopeJob) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	entry, e := s.attempt(ctx, j)
	if e != nil {
		return e
	}
	if !scopePhaseStep(entry.Job.Phase, j.Phase) || j.RestoredAccounts > uint64(len(j.Dependencies)) {
		return ErrScopeStoreConflict
	}
	entry.Job = cloneScope(j)
	next := cloneScope(s.disk)
	next.Jobs[j.ID] = entry
	if e = s.file.write(next, func() error { _, e := s.attempt(ctx, j); return e }); e != nil {
		return e
	}
	s.disk = next
	return nil
}
func (s *FileScopeStore) Fail(ctx context.Context, j ScopeJob) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	entry, e := s.attempt(ctx, j)
	if e != nil {
		return e
	}
	if j.Phase != ScopeFailed && j.Phase != HeadFirstRequired {
		return ErrScopeStoreConflict
	}
	entry.Job = cloneScope(j)
	entry.DeadlineMS = 0
	next := cloneScope(s.disk)
	next.Jobs[j.ID] = entry
	delete(next.Data, j.ID)
	if e = s.file.write(next, func() error { _, e := s.attempt(ctx, j); return e }); e != nil {
		return e
	}
	s.disk = next
	return nil
}
func validPublication(j ScopeJob, p Publication) bool {
	if !sameScopeIdentity(j, ScopeJob{EnsurePoolScopeRequest: EnsurePoolScopeRequest{Fence: p.Fence, Anchor: p.Anchor, PoolIDs: p.PoolIDs}, Dependencies: p.Dependencies, ReservedBytes: j.ReservedBytes}) || p.Cursor != p.Anchor.Cursor || p.BaselineHash != j.BaselineHash || p.BufferID != j.BufferID || len(p.BaselineHash) != 64 || p.BufferID == "" || !reflect.DeepEqual(p.HistoryGap, j.HistoryGap) {
		return false
	}
	n, e := validateAccounts(p.Accounts, j.Dependencies, p.Anchor, nil)
	return e == nil && n <= j.Budget.MaxAccountBytes
}
func (s *FileScopeStore) Publish(ctx context.Context, j ScopeJob, p Publication) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	entry, e := s.attempt(ctx, j)
	if e != nil {
		return e
	}
	if e = requireScopeFresh(j, s.now()); e != nil {
		return e
	}
	if !persistedScopeStage(entry.Job, j) || entry.Job.Phase != TailAligning || j.Phase != TailAligning || j.Cursor != j.Anchor.Cursor || j.RestoredAccounts != uint64(len(j.Dependencies)) || !validPublication(j, p) {
		return ErrScopeStoreConflict
	}
	entry.Job = cloneScope(j)
	entry.Job.Phase = ScopedReady
	entry.DeadlineMS = 0
	next := cloneScope(s.disk)
	next.Jobs[j.ID] = entry
	next.Data[j.ID] = cloneScope(p)
	if e = s.file.publish(next, j, p, func() error {
		if _, e := s.attempt(ctx, j); e != nil {
			return e
		}
		return requireScopeFresh(j, s.now())
	}); e != nil {
		return e
	}
	s.disk = next
	return nil
}
func (s *FileScopeStore) Job(id string) (ScopeJob, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.scopeJob(id)
}

// scopeJob requires s.mu to be held.
func (s *FileScopeStore) scopeJob(id string) (ScopeJob, error) {
	if s.file.closed || s.file.uncertain {
		return ScopeJob{}, ErrScopeStoreClosed
	}
	entry, ok := s.disk.Jobs[id]
	if !ok {
		return ScopeJob{}, ErrScopeStoreConflict
	}
	result := cloneScope(entry.Job)
	if result.Phase == ScopedReady {
		if e := s.live(context.Background(), result.Fence); e != nil {
			return ScopeJob{}, e
		}
		if e := requireScopeFresh(result, s.now()); e != nil {
			return ScopeJob{}, e
		}
	}
	return result, nil
}

// RecoveryData is only staged recovery output, never an online readiness proof.
func (s *FileScopeStore) RecoveryData(id string) (Publication, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.recoveryData(id)
}

// recoveryData requires s.mu to be held.
func (s *FileScopeStore) recoveryData(id string) (Publication, error) {
	if e := s.live(context.Background(), s.disk.Fence); e != nil {
		return Publication{}, e
	}
	entry, ok := s.disk.Jobs[id]
	p, exists := s.disk.Data[id]
	if !ok || !exists || entry.Session != s.disk.Session || entry.Job.Phase != ScopedReady {
		return Publication{}, ErrScopeStoreConflict
	}
	result := cloneScope(p)
	if e := s.live(context.Background(), entry.Job.Fence); e != nil {
		return Publication{}, e
	}
	if e := requireScopeFresh(entry.Job, s.now()); e != nil {
		return Publication{}, e
	}
	return result, nil
}
func (s *FileScopeStore) Close() error { s.mu.Lock(); defer s.mu.Unlock(); return s.file.close() }

// FileHistoryStore deliberately uses a DIFFERENT root, snapshot, lock and mutex.
// It cannot publish online state. History writes never take the online mutex.
type FileHistoryLimits struct{ MaxRecords, MaxBytes, MaxSnapshotBytes uint64 }
type historyDisk struct {
	Chain   string          `json:"chain"`
	Records []HistoryRecord `json:"records"`
}
type FileHistoryStore struct {
	mu     sync.Mutex
	file   *scopeAtomicFile
	disk   historyDisk
	limits FileHistoryLimits
}

func OpenFileHistoryStore(root, chain string, b FileHistoryLimits) (*FileHistoryStore, error) {
	if chain == "" || b.MaxRecords == 0 || b.MaxRecords > 100000 || b.MaxBytes == 0 {
		return nil, ErrScopeData
	}
	file, payload, e := openScopeAtomic(root, "xone-history-replay-v1", b.MaxSnapshotBytes)
	if e != nil {
		return nil, e
	}
	d := historyDisk{Chain: chain, Records: []HistoryRecord{}}
	if payload != nil {
		if e = json.Unmarshal(payload, &d); e != nil || d.Chain != chain || uint64(len(d.Records)) > b.MaxRecords {
			file.close()
			return nil, ErrScopeStoreCorrupt
		}
		var bytes uint64
		seen := map[uint64]bool{}
		for _, r := range d.Records {
			if r.ChainKey != chain || r.Namespace != "history_replay" || seen[r.Cursor] || len(r.Payload) == 0 || uint64(len(r.Payload)) > b.MaxBytes-bytes {
				file.close()
				return nil, ErrScopeStoreCorrupt
			}
			seen[r.Cursor] = true
			bytes += uint64(len(r.Payload))
		}
	}
	if e = file.write(d, nil); e != nil {
		file.close()
		return nil, e
	}
	return &FileHistoryStore{file: file, disk: d, limits: b}, nil
}
func (s *FileHistoryStore) AppendHistory(ctx context.Context, r HistoryRecord, b HistoryLimits) error {
	if !s.mu.TryLock() {
		return ErrBusy
	}
	defer s.mu.Unlock()
	if s.file.closed || s.file.uncertain {
		return ErrScopeStoreClosed
	}
	if r.ChainKey != s.disk.Chain || r.Namespace != "history_replay" {
		return ErrHistoryNamespace
	}
	if b.MaxJobs == 0 || len(r.Payload) == 0 || uint64(len(r.Payload)) > b.MaxBytes {
		return ErrBudget
	}
	var n uint64
	for _, old := range s.disk.Records {
		if old.Cursor == r.Cursor {
			if bytes.Equal(old.Payload, r.Payload) {
				return ctx.Err()
			}
			return ErrScopeStoreConflict
		}
		n += uint64(len(old.Payload))
	}
	limit := minScope(b.MaxBytes, s.limits.MaxBytes)
	if uint64(len(s.disk.Records)) >= s.limits.MaxRecords || n > limit || uint64(len(r.Payload)) > limit-n {
		return ErrBudget
	}
	next := cloneScope(s.disk)
	next.Records = append(next.Records, cloneScope(r))
	if e := s.file.write(next, ctx.Err); e != nil {
		return e
	}
	s.disk = next
	return nil
}
func (s *FileHistoryStore) Records() ([]HistoryRecord, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.file.closed || s.file.uncertain {
		return nil, ErrScopeStoreClosed
	}
	return cloneScope(s.disk.Records), nil
}
func (s *FileHistoryStore) Close() error { s.mu.Lock(); defer s.mu.Unlock(); return s.file.close() }

// OnlineScopeCommit is proof metadata read from the owner DB's committed journal.
// The router must still apply that commit to its local view before quoting.
type OnlineScopeCommit struct {
	JobID    string
	Fence    Fence
	Cursor   uint64
	CommitID string
}

// ProductionTransactions is implemented by the existing ONLINE DB/prepared owner
// adapter. Its Publish transaction must include recovery job/attempt/fence/cursor,
// complete pool state AND the owner journal. FileScopeStore deliberately does not
// implement this interface. No file-then-DB transaction is hidden in this adapter.
// OnlineCommit exposes proof only for a committed READY scope. Claim's READY
// fast path must be read-only and revalidate that scope's original freshness,
// context and current authority after its last owner I/O. ProductionStore calls
// it directly as the final proof check; a retained journal record alone cannot
// authorize reclaiming or restarting a non-READY scope through this read path.
type ProductionTransactions interface {
	ScopeStore
	OnlineCommit(context.Context, string) (OnlineScopeCommit, error)
}
type ProductionStore struct{ backend ProductionTransactions }

func NewProductionStore(backend ProductionTransactions) (*ProductionStore, error) {
	if backend == nil {
		return nil, ErrScopeData
	}
	return &ProductionStore{backend: backend}, nil
}
func (s *ProductionStore) ReadinessScope() string { return "ONLINE_OWNER_COMMIT" }
func (s *ProductionStore) Current(ctx context.Context, chain string) (Fence, error) {
	current, e := s.backend.Current(ctx, chain)
	if e != nil {
		return Fence{}, e
	}
	return current, nil
}
func (s *ProductionStore) Admit(ctx context.Context, a Admission) (ScopeJob, error) {
	j, e := s.backend.Admit(ctx, a)
	if e != nil {
		return ScopeJob{}, e
	}
	return j, nil
}
func (s *ProductionStore) Claim(ctx context.Context, id string, f Fence) (ScopeJob, error) {
	j, e := s.backend.Claim(ctx, id, f)
	if e != nil {
		return ScopeJob{}, e
	}
	if j.Phase == ScopedReady {
		proof, e := s.OnlineCommit(ctx, id)
		if e != nil {
			return ScopeJob{}, e
		}
		if j.ID != id || proof.Fence != j.Fence || proof.Cursor != j.Cursor {
			return ScopeJob{}, ErrScopeStoreConflict
		}
	}
	if e = ctx.Err(); e != nil {
		return ScopeJob{}, e
	}
	return j, nil
}
func (s *ProductionStore) Checkpoint(ctx context.Context, j ScopeJob) error {
	return s.backend.Checkpoint(ctx, j)
}
func (s *ProductionStore) Fail(ctx context.Context, j ScopeJob) error { return s.backend.Fail(ctx, j) }
func (s *ProductionStore) Publish(ctx context.Context, j ScopeJob, p Publication) error {
	if !validPublication(j, p) {
		return ErrScopeStoreConflict
	}
	return s.backend.Publish(ctx, j, p)
}
func (s *ProductionStore) OnlineCommit(ctx context.Context, id string) (OnlineScopeCommit, error) {
	p, e := s.backend.OnlineCommit(ctx, id)
	if e != nil {
		return OnlineScopeCommit{}, e
	}
	if p.JobID != id || p.CommitID == "" || !validFence(p.Fence) {
		return OnlineScopeCommit{}, ErrScopeStoreConflict
	}
	current, e := s.backend.Current(ctx, p.Fence.ChainKey)
	if e != nil {
		return OnlineScopeCommit{}, e
	}
	if current != p.Fence {
		return OnlineScopeCommit{}, ErrFenceChanged
	}
	// Current can perform owner I/O after the proof's freshness check. Make a
	// direct backend READY read the final I/O, so that backend's own clock and
	// admitted age budget are checked after the actual last authority read.
	// Do not call s.Claim here: it would enter this proof wrapper recursively.
	ready, e := s.backend.Claim(ctx, id, current)
	if e != nil {
		return OnlineScopeCommit{}, e
	}
	if ready.ID != id || ready.Phase != ScopedReady || ready.Fence != current || ready.Cursor != p.Cursor {
		return OnlineScopeCommit{}, ErrScopeStoreConflict
	}
	if e = ctx.Err(); e != nil {
		return OnlineScopeCommit{}, e
	}
	return p, nil
}
