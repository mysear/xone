package bootstrap

import "context"

// EmbeddedScopeBackend persists recovery control state in the existing ONLINE
// owner DB. Load returns nil only for an absent ledger; otherwise it returns the
// complete scope envelope (format, checksum and payload), as passed to Save.
// Before Load returns, the owner must reconcile any pending/unknown prepared or
// journal commit. One active store handle exclusively owns this ledger; opening
// another is a restart and requires closing the previous handle first.
//
// Save atomically replaces that complete ledger. Publish atomically installs raw
// (the entire next ledger, with READY), the complete publication, online state,
// cursor and the owner journal/commit proof. job is the validated TAIL_ALIGNING
// attempt; raw contains its SCOPED_READY successor. A file write followed by an
// owner DB write does not satisfy this contract.
//
// The backend must call a non-nil gate at the final atomic visibility boundary,
// after staging I/O, and also enforce the owner's current lease/fence there. It
// may call gate repeatedly during staging/installation, synchronously before
// returning; it must not retain it or reenter the store (its mutex is held).
// A nil gate occurs only for the constructor's restart/revocation Save, before
// authorization. Successful writes must be durable before returning nil. Any
// write error poisons the store handle since the commit outcome may be unknown;
// reopen only after owner reconciliation. Save, including pruning, never deletes
// owner journal history. OnlineCommit reads committed owner proof, not prepared
// state. The owner, not this store's Close, owns the backend's lifecycle.
//
// CheckAuthority independently verifies the owner's CURRENT active lease and
// complete fence, including the durable current ledger/CAS state; historical
// committed proof and the scope handle's cached lease are insufficient. It must
// fail closed on revoked/changed/unknown authority and honor context cancellation.
// It is called before and after current/READY/proof reads while the store mutex
// is held, so it must not reenter the scope store. Constructor Load/Save precedes
// authorization and deliberately does not call CheckAuthority: a fresh owner can
// advance/revoke the persisted ledger before Authorize. Final write fencing stays
// inside the backend's own transaction; read checks do not replace that gate.
type EmbeddedScopeBackend interface {
	Load() ([]byte, error)
	CheckAuthority(context.Context, Fence) error
	Save(raw []byte, gate func() error) error
	Publish(raw []byte, job ScopeJob, publication Publication, gate func() error) error
	OnlineCommit(context.Context, string) (OnlineScopeCommit, error)
}

// EmbeddedScopeStore reuses the file store's complete recovery state machine but
// persists exclusively through the owner backend. ONLINE_OWNER_COMMIT describes
// owner commit capability; the router must independently apply the journal.
type EmbeddedScopeStore struct {
	*FileScopeStore
	backend EmbeddedScopeBackend
}

var _ ProductionTransactions = (*EmbeddedScopeStore)(nil)

// OpenEmbeddedScopeStore validates the complete envelope and atomically revokes
// all old attempts/READY before returning. It also prunes already-terminal jobs
// and data from strictly older generations in that same Save, before encoding the
// restart metadata. Previously active jobs are retained and revoked, never
// pruned during open. Every open needs fresh Authorize.
func OpenEmbeddedScopeStore(backend EmbeddedScopeBackend, fence Fence, limits FileScopeLimits, now func() uint64) (*EmbeddedScopeStore, error) {
	if backend == nil || !validFence(fence) || !validFileScopeLimits(limits) || now == nil {
		return nil, ErrScopeData
	}
	raw, err := backend.Load()
	if err != nil {
		return nil, err
	}
	file := &scopeAtomicFile{format: "xone-online-recovery-v1", max: limits.MaxSnapshotBytes, external: backend}
	var payload []byte
	if raw != nil {
		payload, err = decodeScopeEnvelope(raw, file.format, file.max)
		if err != nil {
			return nil, err
		}
	}
	store, err := openScopeStore(file, payload, fence, limits, now)
	if err != nil {
		return nil, err
	}
	return &EmbeddedScopeStore{FileScopeStore: store, backend: backend}, nil
}

func (s *EmbeddedScopeStore) ReadinessScope() string { return "ONLINE_OWNER_COMMIT" }

// checkOwnerAuthority requires s.mu. It intentionally does not require the
// local scope lease: a revoked/non-READY Job remains inspectable after reopen,
// provided the actual owner independently authorizes this fence.
func (s *EmbeddedScopeStore) checkOwnerAuthority(ctx context.Context, fence Fence) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if s.file.closed || s.file.uncertain {
		return ErrScopeStoreClosed
	}
	if fence != s.disk.Fence {
		return ErrFenceChanged
	}
	if err := s.backend.CheckAuthority(ctx, fence); err != nil {
		return err
	}
	return ctx.Err()
}

// The mutex spans both owner checks and the local read/transaction, including
// Claim/Admit READY fast paths. No result escapes a failed post-read check.
func readEmbeddedScope[T any](s *EmbeddedScopeStore, ctx context.Context, read func() (T, error), validate func(T) error) (T, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	var zero T
	fence := s.disk.Fence
	if err := s.checkOwnerAuthority(ctx, fence); err != nil {
		return zero, err
	}
	result, err := read()
	if err != nil {
		return zero, err
	}
	if err = s.checkOwnerAuthority(ctx, fence); err != nil {
		return zero, err
	}
	if err = validate(result); err != nil {
		return zero, err
	}
	return result, nil
}

func (s *EmbeddedScopeStore) validateJobRead(ctx context.Context, job ScopeJob) error {
	if err := s.live(ctx, job.Fence); err != nil {
		return err
	}
	if job.Phase == ScopedReady {
		return requireScopeFresh(job, s.now())
	}
	return nil
}

func (s *EmbeddedScopeStore) Current(ctx context.Context, chain string) (Fence, error) {
	return readEmbeddedScope(s, ctx, func() (Fence, error) { return s.currentScope(ctx, chain) },
		func(fence Fence) error { return s.live(ctx, fence) })
}
func (s *EmbeddedScopeStore) Admit(ctx context.Context, admission Admission) (ScopeJob, error) {
	return readEmbeddedScope(s, ctx, func() (ScopeJob, error) { return s.admitScope(ctx, admission) },
		func(job ScopeJob) error { return s.validateJobRead(ctx, job) })
}
func (s *EmbeddedScopeStore) Claim(ctx context.Context, id string, fence Fence) (ScopeJob, error) {
	return readEmbeddedScope(s, ctx, func() (ScopeJob, error) { return s.claimScope(ctx, id, fence) },
		func(job ScopeJob) error { return s.validateJobRead(ctx, job) })
}
func (s *EmbeddedScopeStore) Job(id string) (ScopeJob, error) {
	ctx := context.Background()
	return readEmbeddedScope(s, ctx, func() (ScopeJob, error) { return s.scopeJob(id) },
		func(job ScopeJob) error {
			if job.Phase == ScopedReady {
				return s.validateJobRead(ctx, job)
			}
			return nil
		})
}
func (s *EmbeddedScopeStore) RecoveryData(id string) (Publication, error) {
	ctx := context.Background()
	return readEmbeddedScope(s, ctx, func() (Publication, error) { return s.recoveryData(id) },
		func(publication Publication) error {
			if err := s.live(ctx, publication.Fence); err != nil {
				return err
			}
			return requireScopeFresh(s.disk.Jobs[id].Job, s.now())
		})
}

// OnlineCommit exposes proof only for this session's authorized, fresh READY
// ledger entry, and rechecks that authority/freshness after the owner DB read.
func (s *EmbeddedScopeStore) OnlineCommit(ctx context.Context, id string) (OnlineScopeCommit, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	gate := func() error {
		if err := s.live(ctx, s.disk.Fence); err != nil {
			return err
		}
		entry, ok := s.disk.Jobs[id]
		if !ok || entry.Session != s.disk.Session || entry.Job.Phase != ScopedReady || entry.Job.Fence != s.disk.Fence {
			return ErrScopeStoreConflict
		}
		if err := s.checkOwnerAuthority(ctx, entry.Job.Fence); err != nil {
			return err
		}
		// CheckAuthority can perform I/O; leases and age can expire during it.
		if err := s.live(ctx, entry.Job.Fence); err != nil {
			return err
		}
		return requireScopeFresh(entry.Job, s.now())
	}
	if err := gate(); err != nil {
		return OnlineScopeCommit{}, err
	}
	proof, err := s.backend.OnlineCommit(ctx, id)
	if err != nil {
		return OnlineScopeCommit{}, err
	}
	if err = gate(); err != nil {
		return OnlineScopeCommit{}, err
	}
	if proof.JobID != id || proof.CommitID == "" || !validFence(proof.Fence) || proof.Cursor != s.disk.Jobs[id].Job.Cursor {
		return OnlineScopeCommit{}, ErrScopeStoreConflict
	}
	if proof.Fence != s.disk.Fence {
		return OnlineScopeCommit{}, ErrFenceChanged
	}
	return proof, nil
}

// PruneSuperseded atomically removes only terminal jobs/data from strictly older
// generations whose entire fence is superseded by the current authority. Current
// generation and active jobs are retained. Work is bounded by MaxRetainedJobs;
// callers should prune after Authorize on each new generation, before admission.
// The owner canonical journal/commit history remains in the backend. An uncertain
// handle cannot prune; reconciliation and reopen must resolve its outcome first.
func (s *EmbeddedScopeStore) PruneSuperseded(ctx context.Context) (uint64, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	gate := func() error { return s.live(ctx, s.disk.Fence) }
	if err := gate(); err != nil {
		return 0, err
	}
	next := cloneScope(s.disk)
	removed := pruneSupersededScope(&next, next.Fence)
	if removed == 0 {
		return 0, nil
	}
	if err := s.file.write(next, gate); err != nil {
		return 0, err
	}
	s.disk = next
	return removed, nil
}

// pruneSupersededScope runs only on a private next ledger after full validation.
// The caller must persist it atomically before replacing the live in-memory copy.
func pruneSupersededScope(next *scopeDisk, current Fence) uint64 {
	var removed uint64
	for id, entry := range next.Jobs {
		fence := entry.Job.Fence
		if entry.Job.Phase.Terminal() && fence.Generation < current.Generation && scopeFenceAdvance(fence, current) {
			delete(next.Jobs, id)
			delete(next.Data, id)
			removed++
		}
	}
	return removed
}
