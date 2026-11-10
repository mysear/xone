package bootstrap

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"reflect"
	"testing"
)

// This fake models a single owner transaction, including an unknown outcome.
// It tests state logic, not Pebble durability or cross-system atomicity.
type embeddedScopeBackend struct {
	raw                                          []byte
	commits                                      map[string]OnlineScopeCommit
	publication                                  *Publication
	publishedJob                                 ScopeJob
	saves, publishes, reads, gates               int
	loadErr, saveErr, publishErr, afterCommitErr error
	beforeFinalGate, readHook                    func()
	ownerFence                                   *Fence
	authorityErr                                 error
	authorityChecks                              int
	afterAuthorityCheck                          func()
}

// Owner authority is independent of retained journal proofs. Legacy fixtures
// leave ownerFence nil; authority tests bind it to a separate current owner.
func (b *embeddedScopeBackend) CheckAuthority(ctx context.Context, fence Fence) error {
	b.authorityChecks++
	if err := ctx.Err(); err != nil {
		return err
	}
	if b.authorityErr != nil {
		return b.authorityErr
	}
	if b.ownerFence != nil && *b.ownerFence != fence {
		return ErrFenceChanged
	}
	if b.afterAuthorityCheck != nil {
		b.afterAuthorityCheck()
	}
	return nil
}

func (b *embeddedScopeBackend) Load() ([]byte, error) { return bytes.Clone(b.raw), b.loadErr }
func (b *embeddedScopeBackend) boundary(gate func() error) error {
	if gate != nil {
		b.gates++
		if err := gate(); err != nil {
			return err
		}
	}
	if b.beforeFinalGate != nil {
		b.beforeFinalGate()
	}
	if gate != nil {
		b.gates++
		if err := gate(); err != nil {
			return err
		}
	}
	return nil
}
func (b *embeddedScopeBackend) Save(raw []byte, gate func() error) error {
	b.saves++
	if b.saveErr != nil {
		return b.saveErr
	}
	if err := b.boundary(gate); err != nil {
		return err
	}
	b.raw = bytes.Clone(raw)
	return b.afterCommitErr
}
func (b *embeddedScopeBackend) Publish(raw []byte, j ScopeJob, p Publication, gate func() error) error {
	b.publishes++
	if b.publishErr != nil {
		return b.publishErr
	}
	if err := b.boundary(gate); err != nil {
		return err
	}
	b.raw = bytes.Clone(raw)
	b.publishedJob = cloneScope(j)
	copy := cloneScope(p)
	b.publication = &copy
	if b.commits == nil {
		b.commits = map[string]OnlineScopeCommit{}
	}
	b.commits[j.ID] = OnlineScopeCommit{JobID: j.ID, Fence: j.Fence, Cursor: p.Cursor, CommitID: "owner-commit"}
	return b.afterCommitErr
}
func (b *embeddedScopeBackend) OnlineCommit(_ context.Context, id string) (OnlineScopeCommit, error) {
	b.reads++
	if b.readHook != nil {
		b.readHook()
	}
	p, ok := b.commits[id]
	if !ok {
		return OnlineScopeCommit{}, ErrScopeStoreConflict
	}
	return p, nil
}
func embeddedScopeFixture(t *testing.T) (*ScopeController, *EmbeddedScopeStore, *embeddedScopeBackend, *scopeSource, EnsurePoolScopeRequest, *uint64) {
	t.Helper()
	old, _, src, r := scopeFixture(t)
	now := uint64(1000)
	b := &embeddedScopeBackend{}
	limits := FileScopeLimits{MaxJobs: 2, MaxAccountBytes: 8, MaxRetainedJobs: 8, MaxSnapshotBytes: 1 << 20, AttemptLeaseMS: 10, MaxAuthorityLeaseMS: 100}
	s, err := OpenEmbeddedScopeStore(b, r.Fence, limits, func() uint64 { return now })
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	if _, err = s.Current(context.Background(), r.Fence.ChainKey); !errors.Is(err, ErrFenceChanged) {
		t.Fatal("open retained authority", err)
	}
	if err = s.Authorize(context.Background(), r.Fence, 1100); err != nil {
		t.Fatal(err)
	}
	ctl, err := NewScopeController(old.config, s, func() uint64 { return now })
	if err != nil {
		t.Fatal(err)
	}
	return ctl, s, b, src, r, &now
}
func embeddedScopeDisk(t *testing.T, raw []byte) scopeDisk {
	t.Helper()
	var e scopeEnvelope
	if err := json.Unmarshal(raw, &e); err != nil {
		t.Fatal(err)
	}
	if e.Format != "xone-online-recovery-v1" || e.Checksum != digestScopeBytes([]byte(e.Payload)) {
		t.Fatal("missing complete envelope")
	}
	var d scopeDisk
	if err := json.Unmarshal([]byte(e.Payload), &d); err != nil {
		t.Fatal(err)
	}
	return d
}
func embeddedScopeEncode(t *testing.T, d scopeDisk) []byte {
	t.Helper()
	p, err := json.Marshal(d)
	if err != nil {
		t.Fatal(err)
	}
	raw, err := json.Marshal(scopeEnvelope{Format: "xone-online-recovery-v1", Checksum: digestScopeBytes(p), Payload: string(p)})
	if err != nil {
		t.Fatal(err)
	}
	return raw
}
func embeddedScopeReady(t *testing.T, c *ScopeController, src *scopeSource, r EnsurePoolScopeRequest) ScopeJob {
	t.Helper()
	j, err := c.EnsurePoolScope(context.Background(), r)
	if err != nil {
		t.Fatal(err)
	}
	if err = c.Recover(context.Background(), j.ID, src); err != nil {
		t.Fatal(err)
	}
	return j
}
func TestEmbeddedScopeAtomicPublicationAndCapabilities(t *testing.T) {
	ctl, s, b, src, r, _ := embeddedScopeFixture(t)
	var _ EmbeddedScopeBackend = b
	var _ ProductionTransactions = s
	if _, ok := any(&FileScopeStore{}).(ProductionTransactions); ok {
		t.Fatal("file store acquired online capability")
	}
	if s.ReadinessScope() != "ONLINE_OWNER_COMMIT" || (&FileScopeStore{}).ReadinessScope() != "RECOVERY_DATA_ONLY" {
		t.Fatal("wrong capability")
	}
	j := embeddedScopeReady(t, ctl, src, r)
	d := embeddedScopeDisk(t, b.raw)
	if b.publishes != 1 || b.publication == nil || d.Jobs[j.ID].Job.Phase != ScopedReady || !reflect.DeepEqual(d.Data[j.ID], *b.publication) {
		t.Fatal("publication split from owner transaction")
	}
	if b.publishedJob.Phase != TailAligning || b.publishedJob.Attempt != 1 || b.publishedJob.BufferID == "" || b.publishedJob.BaselineHash == "" {
		t.Fatal(b.publishedJob)
	}
	p, err := s.OnlineCommit(context.Background(), j.ID)
	if err != nil || p.JobID != j.ID || p.Cursor != r.Anchor.Cursor || p.Fence != r.Fence || p.CommitID == "" {
		t.Fatal(p, err)
	}
	prod, err := NewProductionStore(s)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = prod.Claim(context.Background(), j.ID, r.Fence); err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(context.Background(), j.ID, src); err != nil || b.publishes != 1 {
		t.Fatal(err, b.publishes)
	}
	if b.gates == 0 {
		t.Fatal("owner never ran gate")
	}
}
func TestEmbeddedScopeRestartRevokesReadyAndProof(t *testing.T) {
	ctl, s, b, src, r, now := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, r)
	session := embeddedScopeDisk(t, b.raw).Session
	s.Close()
	reopened, err := OpenEmbeddedScopeStore(b, r.Fence, s.limits, func() uint64 { return *now })
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	d := embeddedScopeDisk(t, b.raw)
	if d.Session != session+1 || d.Jobs[j.ID].Job.Phase != ScopeFailed || d.Jobs[j.ID].DeadlineMS != 0 || d.Jobs[j.ID].Job.Failure != "RESTART_REVALIDATION_REQUIRED" {
		t.Fatal(d)
	}
	if _, err = reopened.OnlineCommit(context.Background(), j.ID); err == nil {
		t.Fatal("restart trusted old proof")
	}
	if err = reopened.Authorize(context.Background(), r.Fence, 1100); err != nil {
		t.Fatal(err)
	}
	if _, err = reopened.OnlineCommit(context.Background(), j.ID); err == nil {
		t.Fatal("authorization revived old proof")
	}
	if _, err = reopened.RecoveryData(j.ID); err == nil {
		t.Fatal("restart exposed old publication")
	}
	c, err := NewScopeController(ctl.config, reopened, func() uint64 { return *now })
	if err != nil {
		t.Fatal(err)
	}
	if err = c.Recover(context.Background(), j.ID, src); err != nil {
		t.Fatal(err)
	}
	if b.publishes != 2 || b.publishedJob.Attempt != 2 {
		t.Fatal("restart skipped fresh recovery")
	}
}
func TestEmbeddedScopePublishBoundaryRechecks(t *testing.T) {
	for _, kind := range []string{"cancel", "attempt-expired", "authority-expired", "anchor-expired"} {
		t.Run(kind, func(t *testing.T) {
			ctl, s, b, src, r, now := embeddedScopeFixture(t)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if kind == "anchor-expired" {
				s.limits.AttemptLeaseMS = 100
			}
			j, err := ctl.EnsurePoolScope(ctx, r)
			if err != nil {
				t.Fatal(err)
			}
			src.hook = func() {
				b.beforeFinalGate = func() {
					switch kind {
					case "cancel":
						cancel()
					case "attempt-expired":
						*now = 1010
					case "authority-expired":
						*now = 1100
					case "anchor-expired":
						*now = 1050
					}
				}
			}
			err = ctl.Recover(ctx, j.ID, src)
			want := map[string]error{"cancel": context.Canceled, "attempt-expired": ErrScopeStoreConflict, "authority-expired": ErrFenceChanged, "anchor-expired": ErrScopeStale}[kind]
			if !errors.Is(err, want) {
				t.Fatalf("want %v got %v", want, err)
			}
			if b.publishes != 1 || b.publication != nil || len(b.commits) != 0 || embeddedScopeDisk(t, b.raw).Jobs[j.ID].Job.Phase == ScopedReady {
				t.Fatal("rejected boundary exposed READY")
			}
		})
	}
}
func TestEmbeddedScopeAttemptsAndFence(t *testing.T) {
	ctl, s, b, _, r, now := embeddedScopeFixture(t)
	ctx := context.Background()
	j, err := ctl.EnsurePoolScope(ctx, r)
	if err != nil {
		t.Fatal(err)
	}
	first, err := s.Claim(ctx, j.ID, r.Fence)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = s.Claim(ctx, j.ID, r.Fence); !errors.Is(err, ErrBusy) {
		t.Fatal(err)
	}
	*now = 1010
	second, err := s.Claim(ctx, j.ID, r.Fence)
	if err != nil || second.Attempt != first.Attempt+1 {
		t.Fatal(second, err)
	}
	first.Phase = LiveBuffering
	before := bytes.Clone(b.raw)
	if err = s.Checkpoint(ctx, first); !errors.Is(err, ErrScopeStoreConflict) {
		t.Fatal("stale attempt checkpoint", err)
	}
	if err = s.Publish(ctx, first, Publication{}); !errors.Is(err, ErrScopeStoreConflict) {
		t.Fatal("stale attempt publish", err)
	}
	if !bytes.Equal(before, b.raw) {
		t.Fatal("stale attempt modified ledger")
	}
	f := r.Fence
	f.WriterEpoch++
	if err = s.Authorize(ctx, f, 1100); err != nil {
		t.Fatal(err)
	}
	second.Phase = LiveBuffering
	if err = s.Checkpoint(ctx, second); !errors.Is(err, ErrFenceChanged) {
		t.Fatal("stale fence wrote", err)
	}
	if _, err = s.Claim(ctx, j.ID, r.Fence); !errors.Is(err, ErrFenceChanged) {
		t.Fatal(err)
	}
	if err = s.Authorize(ctx, r.Fence, 1100); !errors.Is(err, ErrFenceChanged) {
		t.Fatal("fence regressed", err)
	}
}
func TestEmbeddedScopeSaveFailureAndUnknownPublicationFailClosed(t *testing.T) {
	for _, kind := range []string{"save", "publish-before", "publish-unknown"} {
		t.Run(kind, func(t *testing.T) {
			ctl, s, b, src, r, now := embeddedScopeFixture(t)
			ctx := context.Background()
			j, err := ctl.EnsurePoolScope(ctx, r)
			if err != nil {
				t.Fatal(err)
			}
			fault := errors.New("owner transaction outcome")
			if kind == "save" {
				memory, raw := cloneScope(s.disk), bytes.Clone(b.raw)
				b.saveErr = fault
				_, err = s.Claim(ctx, j.ID, r.Fence)
				if !reflect.DeepEqual(memory, s.disk) || !bytes.Equal(raw, b.raw) {
					t.Fatal("failed save partially updated state")
				}
			} else {
				src.hook = func() {
					if kind == "publish-unknown" {
						b.afterCommitErr = fault
					} else {
						b.publishErr = fault
					}
				}
				err = ctl.Recover(ctx, j.ID, src)
				if s.disk.Jobs[j.ID].Job.Phase == ScopedReady {
					t.Fatal("failed publish modified memory")
				}
				committed := embeddedScopeDisk(t, b.raw).Jobs[j.ID].Job.Phase == ScopedReady
				if committed != (kind == "publish-unknown") {
					t.Fatal("unexpected owner outcome", committed)
				}
			}
			if !errors.Is(err, fault) {
				t.Fatal(err)
			}
			if _, err = s.Job(j.ID); !errors.Is(err, ErrScopeStoreClosed) {
				t.Fatal("uncertain handle readable", err)
			}
			if _, err = s.OnlineCommit(ctx, j.ID); !errors.Is(err, ErrScopeStoreClosed) {
				t.Fatal("uncertain proof exposed", err)
			}
			if _, err = s.Claim(ctx, j.ID, r.Fence); !errors.Is(err, ErrScopeStoreClosed) {
				t.Fatal("uncertain retry", err)
			}
			if err = s.Authorize(ctx, r.Fence, 1100); !errors.Is(err, ErrScopeStoreClosed) {
				t.Fatal("uncertainty renewed away", err)
			}
			if _, err = s.PruneSuperseded(ctx); !errors.Is(err, ErrScopeStoreClosed) {
				t.Fatal("uncertain ledger pruned", err)
			}
			b.saveErr, b.publishErr, b.afterCommitErr, b.beforeFinalGate = nil, nil, nil, nil
			reopened, err := OpenEmbeddedScopeStore(b, r.Fence, s.limits, func() uint64 { return *now })
			if err != nil {
				t.Fatal(err)
			}
			defer reopened.Close()
			if reopened.disk.Jobs[j.ID].Job.Phase != ScopeFailed {
				t.Fatal("reconcile trusted old ready")
			}
		})
	}
}
func TestEmbeddedScopeOnlineCommitValidatesLedgerAndProof(t *testing.T) {
	for _, kind := range []string{"job", "commit", "fence", "cursor", "missing", "age", "late-age", "late-authority", "cancel"} {
		t.Run(kind, func(t *testing.T) {
			ctl, s, b, src, r, now := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, r)
			p := b.commits[j.ID]
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			switch kind {
			case "job":
				p.JobID = "wrong"
			case "commit":
				p.CommitID = ""
			case "fence":
				p.Fence.WriterEpoch++
			case "cursor":
				p.Cursor++
			case "age":
				*now = 1050
			case "late-age":
				b.readHook = func() { *now = 1050 }
			case "late-authority":
				b.readHook = func() { *now = 1100 }
			case "cancel":
				b.readHook = cancel
			}
			b.commits[j.ID] = p
			if kind == "missing" {
				delete(b.commits, j.ID)
			}
			if p, err := s.OnlineCommit(ctx, j.ID); err == nil || p.CommitID != "" {
				t.Fatal("invalid proof exposed", p, err)
			}
		})
	}
}
func TestEmbeddedScopeLoadValidationAndRestartFailure(t *testing.T) {
	ctl, s, original, src, r, now := embeddedScopeFixture(t)
	embeddedScopeReady(t, ctl, src, r)
	valid := bytes.Clone(original.raw)
	for _, kind := range []string{"empty", "format", "checksum", "payload", "jobs", "job-shape", "publication", "session-overflow", "fence", "size", "load", "restart-save"} {
		t.Run(kind, func(t *testing.T) {
			b := &embeddedScopeBackend{raw: bytes.Clone(valid)}
			limits := s.limits
			d := embeddedScopeDisk(t, valid)
			want := ErrScopeStoreCorrupt
			switch kind {
			case "empty":
				b.raw = []byte{}
			case "format", "checksum", "payload":
				var e scopeEnvelope
				if err := json.Unmarshal(valid, &e); err != nil {
					t.Fatal(err)
				}
				switch kind {
				case "format":
					e.Format = "other"
				case "checksum":
					e.Checksum = "bad"
				case "payload":
					e.Payload = "{"
					e.Checksum = digestScopeBytes([]byte(e.Payload))
				}
				b.raw, _ = json.Marshal(e)
			case "jobs":
				d.Jobs = nil
				b.raw = embeddedScopeEncode(t, d)
			case "job-shape":
				for id, e := range d.Jobs {
					e.Job.Dependencies = nil
					d.Jobs[id] = e
				}
				b.raw = embeddedScopeEncode(t, d)
			case "publication":
				for id, p := range d.Data {
					p.Accounts = nil
					d.Data[id] = p
				}
				b.raw = embeddedScopeEncode(t, d)
			case "session-overflow":
				d.Session = ^uint64(0)
				b.raw = embeddedScopeEncode(t, d)
			case "fence":
				d.Fence.WriterEpoch++
				b.raw = embeddedScopeEncode(t, d)
				want = ErrFenceChanged
			case "size":
				limits.MaxSnapshotBytes = 256
			case "load":
				want = errors.New("load failed")
				b.loadErr = want
			case "restart-save":
				want = errors.New("restart save failed")
				b.saveErr = want
			}
			before := bytes.Clone(b.raw)
			opened, err := OpenEmbeddedScopeStore(b, r.Fence, limits, func() uint64 { return *now })
			if !errors.Is(err, want) || opened != nil {
				t.Fatal(opened, err, want)
			}
			if !bytes.Equal(before, b.raw) {
				t.Fatal("failed open changed state")
			}
			if kind != "restart-save" && b.saves != 0 {
				t.Fatal("invalid state rewritten")
			}
		})
	}
	if opened, err := OpenEmbeddedScopeStore(nil, r.Fence, s.limits, func() uint64 { return *now }); opened != nil || !errors.Is(err, ErrScopeData) {
		t.Fatal(opened, err)
	}
}
func TestEmbeddedScopeZeroTailTamperingRejected(t *testing.T) {
	ctl, _, b, src, r, _ := embeddedScopeFixture(t)
	src.candidates[0].Anchor = r.Anchor
	src.candidates[0].ContentHash = BaselineHash(src.candidates[0])
	src.tailOverride = func(tail *ScopeTail) { tail.Events = 0; tail.Accounts[0].Data = []byte{9} }
	j, err := ctl.EnsurePoolScope(context.Background(), r)
	if err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(context.Background(), j.ID, src); !errors.Is(err, ErrScopeData) {
		t.Fatal("zero-tail tamper accepted", err)
	}
	if b.publishes != 0 || len(b.commits) != 0 {
		t.Fatal("tampered baseline reached owner")
	}
}
func TestEmbeddedScopePruneSuperseded(t *testing.T) {
	ctl, s, b, src, r, _ := embeddedScopeFixture(t)
	ctx := context.Background()
	j := embeddedScopeReady(t, ctl, src, r)
	if n, err := s.PruneSuperseded(ctx); err != nil || n != 0 {
		t.Fatal("pruned current generation", n, err)
	}
	current := r.Fence
	current.Generation++
	if err := s.Authorize(ctx, current, 1100); err != nil {
		t.Fatal(err)
	}
	if n, err := s.PruneSuperseded(ctx); err != nil || n != 1 {
		t.Fatal(n, err)
	}
	d := embeddedScopeDisk(t, b.raw)
	if len(d.Jobs) != 0 || len(d.Data) != 0 {
		t.Fatal("old ledger data retained")
	}
	if b.commits[j.ID].CommitID == "" {
		t.Fatal("prune deleted owner journal")
	}
	if n, err := s.PruneSuperseded(ctx); err != nil || n != 0 {
		t.Fatal(n, err)
	}
}

func TestEmbeddedScopePruneRetainsCurrentActiveAndIncomparable(t *testing.T) {
	for _, kind := range []string{"current-generation", "active", "future-writer", "foreign-chain"} {
		t.Run(kind, func(t *testing.T) {
			ctl, s, b, src, r, _ := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, r)
			f := r.Fence
			f.Generation++
			if err := s.Authorize(context.Background(), f, 1100); err != nil {
				t.Fatal(err)
			}
			// Exercise the retention predicate, including states that normal
			// Authorize would already revoke, without an extra transition.
			entry := s.disk.Jobs[j.ID]
			switch kind {
			case "current-generation":
				entry.Job.Fence = f
			case "active":
				entry.Job.Phase = TailAligning
			case "future-writer":
				entry.Job.Fence.WriterEpoch = f.WriterEpoch + 1
			case "foreign-chain":
				entry.Job.Fence.ChainIdentity = "other-genesis"
			}
			s.disk.Jobs[j.ID] = entry
			before, saves := cloneScope(s.disk), b.saves
			if n, err := s.PruneSuperseded(context.Background()); err != nil || n != 0 {
				t.Fatal(n, err)
			}
			if !reflect.DeepEqual(before, s.disk) || b.saves != saves {
				t.Fatal("retained ledger changed")
			}
		})
	}
}

func TestEmbeddedScopePruneFailureIsAtomic(t *testing.T) {
	for _, kind := range []string{"save", "cancel", "authority"} {
		t.Run(kind, func(t *testing.T) {
			ctl, s, b, src, r, now := embeddedScopeFixture(t)
			embeddedScopeReady(t, ctl, src, r)
			f := r.Fence
			f.Generation++
			if err := s.Authorize(context.Background(), f, 1100); err != nil {
				t.Fatal(err)
			}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			want := errors.New("prune save failed")
			switch kind {
			case "save":
				b.saveErr = want
			case "cancel":
				want = context.Canceled
				b.beforeFinalGate = cancel
			case "authority":
				want = ErrFenceChanged
				b.beforeFinalGate = func() { *now = 1100 }
			}
			before, raw := cloneScope(s.disk), bytes.Clone(b.raw)
			if n, err := s.PruneSuperseded(ctx); n != 0 || !errors.Is(err, want) {
				t.Fatal(n, err)
			}
			if !reflect.DeepEqual(before, s.disk) || !bytes.Equal(raw, b.raw) {
				t.Fatal("failed prune partially updated ledger")
			}
		})
	}
}

func TestEmbeddedScopeGenerationsRemainBoundedWithPrune(t *testing.T) {
	ctl, s, b, src, r, now := embeddedScopeFixture(t)
	limits := s.limits
	limits.MaxJobs, limits.MaxRetainedJobs, limits.MaxSnapshotBytes = 1, 1, 8192
	config := ctl.config
	s.Close()
	for generation := uint64(2); generation < 22; generation++ {
		r.Fence.Generation = generation
		next, err := OpenEmbeddedScopeStore(b, r.Fence, limits, func() uint64 { return *now })
		if err != nil {
			t.Fatal(generation, err)
		}
		if err = next.Authorize(context.Background(), r.Fence, 1100); err != nil {
			t.Fatal(err)
		}
		if _, err = next.PruneSuperseded(context.Background()); err != nil {
			t.Fatal(err)
		}
		config.Fence = r.Fence
		config.Limits.MaxJobs = 1
		r.Budget = config.Limits
		c, err := NewScopeController(config, next, func() uint64 { return *now })
		if err != nil {
			t.Fatal(err)
		}
		src.candidates[0].Fence = r.Fence
		src.candidates[0].ContentHash = BaselineHash(src.candidates[0])
		embeddedScopeReady(t, c, src, r)
		d := embeddedScopeDisk(t, b.raw)
		if len(d.Jobs) != 1 || len(d.Data) != 1 || uint64(len(b.raw)) > limits.MaxSnapshotBytes {
			t.Fatal("retention exceeded", generation)
		}
		if err = next.Close(); err != nil {
			t.Fatal(err)
		}
	}
	if len(b.commits) != 20 {
		t.Fatal("owner journal history was pruned", len(b.commits))
	}
}

func TestEmbeddedScopePublishRejectsReplacedFence(t *testing.T) {
	ctl, s, b, src, r, _ := embeddedScopeFixture(t)
	src.hook = func() {
		f := r.Fence
		f.WriterEpoch++
		if err := s.Authorize(context.Background(), f, 1100); err != nil {
			t.Fatal(err)
		}
	}
	j, err := ctl.EnsurePoolScope(context.Background(), r)
	if err != nil {
		t.Fatal(err)
	}
	if err = ctl.Recover(context.Background(), j.ID, src); !errors.Is(err, ErrFenceChanged) {
		t.Fatal(err)
	}
	if b.publishes != 0 || len(b.commits) != 0 {
		t.Fatal("replaced fence reached owner publication")
	}
}

func TestEmbeddedScopeFailedReplacementNeverRestoresOldAuthority(t *testing.T) {
	ctl, s, b, src, r, _ := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, r)
	f := r.Fence
	f.WriterEpoch++
	f.AdapterVersion = "a0"
	fault := errors.New("replacement write failed")
	b.saveErr = fault
	before := cloneScope(s.disk)
	if err := s.Authorize(context.Background(), f, 1100); !errors.Is(err, fault) {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(before, s.disk) || s.authorityUntil != 0 || s.fenceFloor != f {
		t.Fatal("failed replacement changed ledger or retained authority")
	}
	if _, err := s.OnlineCommit(context.Background(), j.ID); err == nil {
		t.Fatal("failed replacement retained proof")
	}
	if err := s.Authorize(context.Background(), r.Fence, 1100); !errors.Is(err, ErrFenceChanged) {
		t.Fatal("old fence renewed", err)
	}
}

func TestEmbeddedScopeNewGenerationOpenPrunesBeforeRestartBudget(t *testing.T) {
	ctl, s, b, src, r, now := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, r)
	limits := s.limits
	// Revoking READY adds a failure reason. With no spare bytes, callers could
	// otherwise fail Open before they can run the explicit prune operation.
	limits.MaxSnapshotBytes = uint64(len(b.raw))
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	f := r.Fence
	f.Generation++
	opened, err := OpenEmbeddedScopeStore(b, f, limits, func() uint64 { return *now })
	if err != nil {
		t.Fatal("new generation cannot reclaim full terminal ledger", err)
	}
	defer opened.Close()
	d := embeddedScopeDisk(t, b.raw)
	if len(d.Jobs) != 0 || len(d.Data) != 0 || d.Fence != f {
		t.Fatal("superseded terminal data retained", d)
	}
	if b.commits[j.ID].CommitID == "" {
		t.Fatal("owner journal deleted")
	}
	if _, err = opened.Current(context.Background(), f.ChainKey); !errors.Is(err, ErrFenceChanged) {
		t.Fatal("automatic pruning authorized owner", err)
	}
}

func TestEmbeddedScopeNewGenerationOpenRetainsPreviouslyActive(t *testing.T) {
	ctl, s, b, src, r, now := embeddedScopeFixture(t)
	ready := embeddedScopeReady(t, ctl, src, r)
	q := r
	q.PoolIDs = []string{"q"}
	active, err := ctl.EnsurePoolScope(context.Background(), q)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = s.Claim(context.Background(), active.ID, r.Fence); err != nil {
		t.Fatal(err)
	}
	if err = s.Close(); err != nil {
		t.Fatal(err)
	}
	f := r.Fence
	f.Generation++
	opened, err := OpenEmbeddedScopeStore(b, f, s.limits, func() uint64 { return *now })
	if err != nil {
		t.Fatal(err)
	}
	defer opened.Close()
	d := embeddedScopeDisk(t, b.raw)
	if _, ok := d.Jobs[ready.ID]; ok {
		t.Fatal("old terminal job retained")
	}
	entry, ok := d.Jobs[active.ID]
	if !ok || entry.Job.Phase != ScopeFailed || entry.Job.Failure != "RESTART_REVALIDATION_REQUIRED" || entry.DeadlineMS != 0 {
		t.Fatal("active job lost rather than revoked", entry)
	}
}

func TestEmbeddedScopeNewGenerationValidatesBeforePruning(t *testing.T) {
	ctl, s, b, src, r, now := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, r)
	d := embeddedScopeDisk(t, b.raw)
	p := d.Data[j.ID]
	p.Accounts = nil
	d.Data[j.ID] = p
	b.raw = embeddedScopeEncode(t, d)
	before, saves := bytes.Clone(b.raw), b.saves
	f := r.Fence
	f.Generation++
	if opened, err := OpenEmbeddedScopeStore(b, f, s.limits, func() uint64 { return *now }); opened != nil || !errors.Is(err, ErrScopeStoreCorrupt) {
		t.Fatal("corruption hidden by pruning", opened, err)
	}
	if b.saves != saves || !bytes.Equal(before, b.raw) {
		t.Fatal("invalid ledger rewritten")
	}
}

func TestEmbeddedScopeUnknownSaveDoesNotAdvanceMemory(t *testing.T) {
	ctl, s, b, _, r, _ := embeddedScopeFixture(t)
	j, err := ctl.EnsurePoolScope(context.Background(), r)
	if err != nil {
		t.Fatal(err)
	}
	before := cloneScope(s.disk)
	fault := errors.New("save acknowledgement lost")
	b.afterCommitErr = fault
	if _, err = s.Claim(context.Background(), j.ID, r.Fence); !errors.Is(err, fault) {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(s.disk, before) {
		t.Fatal("unknown save advanced memory")
	}
	if embeddedScopeDisk(t, b.raw).Jobs[j.ID].Job.Attempt != 1 {
		t.Fatal("fault did not follow owner commit")
	}
	if _, err = s.Claim(context.Background(), j.ID, r.Fence); !errors.Is(err, ErrScopeStoreClosed) {
		t.Fatal("unknown save retried", err)
	}
}

func TestEmbeddedScopeOwnerAdvanceRejectsCachedReady(t *testing.T) {
	for _, advance := range []string{"writer", "generation", "lease"} {
		t.Run(advance, func(t *testing.T) {
			ctl, store, b, src, req, _ := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, req)
			owner := req.Fence
			b.ownerFence = &owner
			switch advance {
			case "writer":
				owner.WriterEpoch++
			case "generation":
				owner.Generation++
			case "lease":
				b.authorityErr = ErrFenceChanged
			}
			// Old proof is still committed history but cannot authorize READY.
			if proof, err := b.OnlineCommit(context.Background(), j.ID); err != nil || proof.Fence != req.Fence {
				t.Fatal(proof, err)
			}
			production, err := NewProductionStore(store)
			if err != nil {
				t.Fatal(err)
			}
			checks := map[string]func() error{
				"Current": func() error {
					f, err := store.Current(context.Background(), req.Fence.ChainKey)
					if err != nil && f != (Fence{}) {
						t.Error("current returned fence on failure")
					}
					return err
				},
				"Admit-ready": func() error {
					got, err := store.Admit(context.Background(), Admission{Job: j, Limits: j.Budget})
					if err != nil && got.Phase == ScopedReady {
						t.Error("admit returned READY on failure")
					}
					return err
				},
				"Claim-ready": func() error {
					got, err := store.Claim(context.Background(), j.ID, req.Fence)
					if err != nil && got.Phase == ScopedReady {
						t.Error("claim returned READY on failure")
					}
					return err
				},
				"Job": func() error {
					got, err := store.Job(j.ID)
					if err != nil && got.Phase == ScopedReady {
						t.Error("job returned READY on failure")
					}
					return err
				},
				"RecoveryData": func() error {
					p, err := store.RecoveryData(j.ID)
					if err != nil && len(p.Accounts) != 0 {
						t.Error("returned accounts on failure")
					}
					return err
				},
				"OnlineCommit": func() error {
					p, err := store.OnlineCommit(context.Background(), j.ID)
					if err != nil && p.CommitID != "" {
						t.Error("returned proof on failure")
					}
					return err
				},
				"Production.Claim": func() error {
					got, err := production.Claim(context.Background(), j.ID, req.Fence)
					if err != nil && got.Phase == ScopedReady {
						t.Error("production returned READY on failure")
					}
					return err
				},
				"Production.OnlineCommit": func() error { _, err := production.OnlineCommit(context.Background(), j.ID); return err },
			}
			for name, read := range checks {
				t.Run(name, func(t *testing.T) {
					if err := read(); !errors.Is(err, ErrFenceChanged) {
						t.Fatalf("owner advanced but cached authorization accepted: %v", err)
					}
				})
			}
		})
	}
}

func TestEmbeddedScopeOwnerAdvanceDuringProofRead(t *testing.T) {
	for _, advance := range []string{"writer", "generation", "lease"} {
		t.Run(advance, func(t *testing.T) {
			ctl, store, b, src, req, _ := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, req)
			owner := req.Fence
			b.ownerFence = &owner
			b.readHook = func() {
				switch advance {
				case "writer":
					owner.WriterEpoch++
				case "generation":
					owner.Generation++
				case "lease":
					b.authorityErr = ErrFenceChanged
				}
			}
			if proof, err := store.OnlineCommit(context.Background(), j.ID); !errors.Is(err, ErrFenceChanged) || proof.CommitID != "" {
				t.Fatal("read crossed revoked authority", proof, err)
			}
		})
	}
}

func TestEmbeddedScopeReadChecksAuthorityOnBothSides(t *testing.T) {
	for _, method := range []string{"Current", "Admit", "Claim", "Job", "RecoveryData", "OnlineCommit"} {
		t.Run(method, func(t *testing.T) {
			ctl, store, b, src, req, _ := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, req)
			owner := req.Fence
			b.ownerFence = &owner
			b.authorityChecks = 0
			b.afterAuthorityCheck = func() {
				if b.authorityChecks == 1 {
					owner.WriterEpoch++
				}
			}
			var err error
			switch method {
			case "Current":
				_, err = store.Current(context.Background(), req.Fence.ChainKey)
			case "Admit":
				_, err = store.Admit(context.Background(), Admission{Job: j, Limits: j.Budget})
			case "Claim":
				_, err = store.Claim(context.Background(), j.ID, req.Fence)
			case "Job":
				_, err = store.Job(j.ID)
			case "RecoveryData":
				_, err = store.RecoveryData(j.ID)
			case "OnlineCommit":
				_, err = store.OnlineCommit(context.Background(), j.ID)
			}
			if !errors.Is(err, ErrFenceChanged) || b.authorityChecks != 2 {
				t.Fatal("missing post-read owner check", b.authorityChecks, err)
			}
		})
	}
}

func TestEmbeddedScopeReadRechecksLocalValidityAfterOwnerIO(t *testing.T) {
	for _, kind := range []string{"age", "authority", "cancel", "owner-error"} {
		t.Run(kind, func(t *testing.T) {
			ctl, store, b, src, req, now := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, req)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			want := ErrScopeStale
			b.authorityChecks = 0
			b.afterAuthorityCheck = func() {
				if b.authorityChecks != 2 {
					return
				}
				switch kind {
				case "age":
					*now = 1050
				case "authority":
					*now = 1100
				case "cancel":
					cancel()
				}
			}
			switch kind {
			case "authority":
				want = ErrFenceChanged
			case "cancel":
				want = context.Canceled
			case "owner-error":
				want = errors.New("owner authority unavailable")
				b.authorityErr = want
			}
			if job, err := store.Claim(ctx, j.ID, req.Fence); !errors.Is(err, want) || job.Phase == ScopedReady {
				t.Fatal("owner I/O bypassed local final validation", job, err)
			}
		})
	}
}

func TestEmbeddedScopeAuthorityChecksPreserveFreshReopen(t *testing.T) {
	ctl, store, b, src, req, now := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, req)
	if err := store.Close(); err != nil {
		t.Fatal(err)
	}
	owner := req.Fence
	owner.WriterEpoch++
	b.ownerFence = &owner
	checks := b.authorityChecks
	opened, err := OpenEmbeddedScopeStore(b, owner, store.limits, func() uint64 { return *now })
	if err != nil {
		t.Fatal("fresh owner could not advance and revoke ledger", err)
	}
	defer opened.Close()
	if b.authorityChecks != checks {
		t.Fatal("constructor checked authority before its revoke Save")
	}
	if embeddedScopeDisk(t, b.raw).Fence != owner {
		t.Fatal("constructor did not advance ledger")
	}
	// Diagnostic non-READY reads need owner authority, but not local Authorize.
	revoked, err := opened.Job(j.ID)
	if err != nil || revoked.Phase != ScopeFailed || revoked.Failure != "RESTART_REVALIDATION_REQUIRED" {
		t.Fatal(revoked, err)
	}
	if _, err = opened.Current(context.Background(), owner.ChainKey); !errors.Is(err, ErrFenceChanged) {
		t.Fatal("reopen authorized local scope lease", err)
	}
	if err = opened.Authorize(context.Background(), owner, 1100); err != nil {
		t.Fatal(err)
	}
	if current, err := opened.Current(context.Background(), owner.ChainKey); err != nil || current != owner {
		t.Fatal(current, err)
	}
	if proof, err := opened.OnlineCommit(context.Background(), j.ID); err == nil || proof.CommitID != "" {
		t.Fatal("fresh authorization revived old proof", proof, err)
	}
}

func TestProductionScopeFreshnessAfterFinalOwnerIO(t *testing.T) {
	for _, method := range []string{"OnlineCommit", "Claim"} {
		for _, boundary := range []string{"current", "final-ready"} {
			t.Run(method+"/"+boundary, func(t *testing.T) {
				ctl, store, b, src, req, now := embeddedScopeFixture(t)
				j := embeddedScopeReady(t, ctl, src, req)
				production, err := NewProductionStore(store)
				if err != nil {
					t.Fatal(err)
				}
				// OnlineCommit: proof checks 1/2, Current checks 3/4.
				// Claim adds its initial READY checks before that sequence.
				lastCheck := 4
				if method == "Claim" {
					lastCheck += 2
				}
				if boundary == "final-ready" {
					lastCheck += 2
				}
				b.authorityChecks = 0
				b.afterAuthorityCheck = func() {
					if b.authorityChecks == lastCheck {
						*now = 1050
					}
				}
				saves, publishes := b.saves, b.publishes
				if method == "OnlineCommit" {
					proof, err := production.OnlineCommit(context.Background(), j.ID)
					if !errors.Is(err, ErrScopeStale) || proof != (OnlineScopeCommit{}) {
						t.Fatal("expired proof escaped last owner I/O", proof, err, b.authorityChecks)
					}
				} else {
					job, err := production.Claim(context.Background(), j.ID, req.Fence)
					if !errors.Is(err, ErrScopeStale) || !reflect.DeepEqual(job, ScopeJob{}) {
						t.Fatal("expired READY escaped last owner I/O", job, err, b.authorityChecks)
					}
				}
				if *now != 1050 {
					t.Fatal("fault boundary not exercised", b.authorityChecks, lastCheck)
				}
				if b.saves != saves || b.publishes != publishes {
					t.Fatal("ready validation wrote owner state")
				}
			})
		}
	}
}

func TestProductionScopeClaimClearsReadyOnProofFailure(t *testing.T) {
	ctl, store, b, src, req, _ := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, req)
	production, err := NewProductionStore(store)
	if err != nil {
		t.Fatal(err)
	}
	owner := req.Fence
	b.ownerFence = &owner
	// The first Claim obtains READY, then the proof read observes owner change.
	b.readHook = func() { owner.WriterEpoch++ }
	job, err := production.Claim(context.Background(), j.ID, req.Fence)
	if !errors.Is(err, ErrFenceChanged) || !reflect.DeepEqual(job, ScopeJob{}) {
		t.Fatal("failed proof retained READY", job, err)
	}
}

// Deliberately violate the usual result-on-error convention at the backend to
// ensure the production wrapper never forwards stale data alongside an error.
type embeddedProductionReadFault struct {
	ProductionTransactions
	job   ScopeJob
	fault error
}

func (b *embeddedProductionReadFault) Claim(context.Context, string, Fence) (ScopeJob, error) {
	return b.job, b.fault
}
func (b *embeddedProductionReadFault) Admit(context.Context, Admission) (ScopeJob, error) {
	return b.job, b.fault
}
func (b *embeddedProductionReadFault) Current(context.Context, string) (Fence, error) {
	return b.job.Fence, b.fault
}

func TestProductionScopeClearsBackendErrorResults(t *testing.T) {
	ctl, store, _, src, req, _ := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, req)
	ready, err := store.Job(j.ID)
	if err != nil {
		t.Fatal(err)
	}
	fault := errors.New("backend read failed")
	production, err := NewProductionStore(&embeddedProductionReadFault{ProductionTransactions: store, job: ready, fault: fault})
	if err != nil {
		t.Fatal(err)
	}
	if got, err := production.Claim(context.Background(), j.ID, req.Fence); !errors.Is(err, fault) || !reflect.DeepEqual(got, ScopeJob{}) {
		t.Error("claim forwarded error data", got, err)
	}
	if got, err := production.Admit(context.Background(), Admission{Job: j, Limits: j.Budget}); !errors.Is(err, fault) || !reflect.DeepEqual(got, ScopeJob{}) {
		t.Error("admit forwarded error data", got, err)
	}
	if got, err := production.Current(context.Background(), req.Fence.ChainKey); !errors.Is(err, fault) || got != (Fence{}) {
		t.Error("current forwarded error data", got, err)
	}
}

func TestProductionScopeFinalReadyValidationIsReadOnly(t *testing.T) {
	for _, method := range []string{"OnlineCommit", "Claim"} {
		t.Run(method, func(t *testing.T) {
			ctl, store, b, src, req, _ := embeddedScopeFixture(t)
			j := embeddedScopeReady(t, ctl, src, req)
			production, err := NewProductionStore(store)
			if err != nil {
				t.Fatal(err)
			}
			before := bytes.Clone(b.raw)
			saves, publishes := b.saves, b.publishes
			b.authorityChecks = 0
			wantChecks := 6
			if method == "OnlineCommit" {
				if p, err := production.OnlineCommit(context.Background(), j.ID); err != nil || p.CommitID == "" {
					t.Fatal(p, err)
				}
			} else {
				wantChecks = 8
				if job, err := production.Claim(context.Background(), j.ID, req.Fence); err != nil || job.Phase != ScopedReady {
					t.Fatal(job, err)
				}
			}
			if b.authorityChecks != wantChecks {
				t.Fatal("unexpected recursion or I/O after final READY validation", b.authorityChecks, wantChecks)
			}
			if !bytes.Equal(before, b.raw) || b.saves != saves || b.publishes != publishes {
				t.Fatal("proof validation reclaimed or wrote a scope")
			}
		})
	}
}

type embeddedProductionInitialClaim struct {
	ProductionTransactions
	initial ScopeJob
	calls   int
}

func (b *embeddedProductionInitialClaim) Claim(ctx context.Context, id string, f Fence) (ScopeJob, error) {
	b.calls++
	if b.calls == 1 {
		return b.initial, nil
	}
	return b.ProductionTransactions.Claim(ctx, id, f)
}
func TestProductionScopeClaimClearsReadyOnProofMismatch(t *testing.T) {
	ctl, store, _, src, req, _ := embeddedScopeFixture(t)
	j := embeddedScopeReady(t, ctl, src, req)
	initial, err := store.Job(j.ID)
	if err != nil {
		t.Fatal(err)
	}
	initial.Cursor++
	backend := &embeddedProductionInitialClaim{ProductionTransactions: store, initial: initial}
	production, err := NewProductionStore(backend)
	if err != nil {
		t.Fatal(err)
	}
	if got, err := production.Claim(context.Background(), j.ID, req.Fence); !errors.Is(err, ErrScopeStoreConflict) || !reflect.DeepEqual(got, ScopeJob{}) {
		t.Fatal("mismatched proof retained READY", got, err)
	}
	if backend.calls != 2 {
		t.Fatal("wrapper recursively claimed", backend.calls)
	}
}
