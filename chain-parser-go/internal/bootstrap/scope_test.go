package bootstrap

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"reflect"
	"sync"
	"testing"
)

// This is a transactional test double, not evidence of disk/crash durability.
type scopeStore struct {
	mu                  sync.Mutex
	current             Fence
	jobs                map[string]ScopeJob
	maxActive, maxBytes uint64
	events              []string
	published           *Publication
	failAt              ScopePhase
	changeAt            ScopePhase
	publishErr          bool
}

func (s *scopeStore) Current(context.Context, string) (Fence, error) { return s.current, nil }
func (s *scopeStore) Admit(_ context.Context, a Admission) (ScopeJob, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if j, ok := s.jobs[a.Job.ID]; ok {
		return j, nil
	}
	var n, b uint64
	for _, j := range s.jobs {
		if !j.Phase.Terminal() {
			n++
			b += j.ReservedBytes
		}
	}
	if n >= a.Limits.MaxJobs || b > a.Limits.MaxAccountBytes || a.Job.ReservedBytes > a.Limits.MaxAccountBytes-b {
		return ScopeJob{}, ErrBudget
	}
	s.jobs[a.Job.ID] = a.Job
	return a.Job, nil
}
func (s *scopeStore) Claim(_ context.Context, id string, f Fence) (ScopeJob, error) {
	j := s.jobs[id]
	if j.Phase == ScopedReady {
		return j, nil
	}
	if j.Attempt > 0 && !j.Phase.Terminal() {
		return j, ErrBusy
	}
	j.Attempt++
	j.Phase = Admitted
	s.jobs[id] = j
	return j, nil
}
func (s *scopeStore) Checkpoint(_ context.Context, j ScopeJob) error {
	s.events = append(s.events, string(j.Phase))
	if s.changeAt == j.Phase {
		s.current.Generation++
	}
	if s.failAt == j.Phase {
		return errors.New("disk")
	}
	s.jobs[j.ID] = j
	return nil
}
func (s *scopeStore) Fail(_ context.Context, j ScopeJob) error { s.jobs[j.ID] = j; return nil }
func (s *scopeStore) Publish(ctx context.Context, j ScopeJob, p Publication) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if s.publishErr {
		return errors.New("disk")
	}
	if s.current != j.Fence {
		return ErrFenceChanged
	}
	j.Phase = ScopedReady
	s.jobs[j.ID] = j
	s.published = &p
	s.events = append(s.events, "publish")
	return nil
}

type scopeSource struct {
	tailOverride   func(*ScopeTail)
	bufferOverride func(*StreamBuffer)
	store          *scopeStore
	candidates     []BaselineCandidate
	data           map[string][]byte
	badShard, gap  bool
	hook           func()
	order          []string
}

func (s *scopeSource) StartBuffer(context.Context, ScopeJob) (StreamBuffer, error) {
	s.order = append(s.order, "buffer")
	buffer := StreamBuffer{ID: "wal-session", FirstCursor: 11, Durable: true}
	if s.bufferOverride != nil {
		s.bufferOverride(&buffer)
	}
	return buffer, nil
}
func (s *scopeSource) FixAnchor(_ context.Context, j ScopeJob, _ StreamBuffer) (ScopeAnchor, error) {
	s.order = append(s.order, "anchor")
	return j.Anchor, nil
}
func (s *scopeSource) Candidates(context.Context, ScopeJob, ScopeAnchor) ([]BaselineCandidate, error) {
	return s.candidates, nil
}
func (s *scopeSource) RestoreShard(_ context.Context, _ ScopeJob, c BaselineCandidate, ids []string) ([]ScopeAccount, error) {
	s.order = append(s.order, "shard")
	out := []ScopeAccount{}
	for _, id := range ids {
		out = append(out, ScopeAccount{ID: id, Data: s.data[id], Anchor: c.Anchor})
	}
	if s.badShard {
		out[0].ID = "alien"
	}
	return out, nil
}
func (s *scopeSource) AlignTail(_ context.Context, j ScopeJob, _ StreamBuffer, c BaselineCandidate, _ []ScopeAccount) (ScopeTail, error) {
	s.order = append(s.order, "tail")
	if s.hook != nil {
		s.hook()
	}
	accounts := []ScopeAccount{}
	for _, d := range j.Dependencies {
		accounts = append(accounts, ScopeAccount{ID: d.ID, Data: s.data[d.ID], Anchor: j.Anchor})
	}
	tail := ScopeTail{From: c.Anchor, To: j.Anchor, Complete: !s.gap, Events: 1, Bytes: 2, Accounts: accounts}
	if s.tailOverride != nil {
		s.tailOverride(&tail)
	}
	return tail, nil
}
func (s *scopeSource) ReleaseBuffer(context.Context, StreamBuffer) error {
	s.order = append(s.order, "release")
	return nil
}
func digestBytes(b []byte) string { h := sha256.Sum256(b); return hex.EncodeToString(h[:]) }
func scopeFixture(t *testing.T) (*ScopeController, *scopeStore, *scopeSource, EnsurePoolScopeRequest) {
	t.Helper()
	f := Fence{ChainKey: "sol", ChainIdentity: "genesis", Owner: "owner", Assignment: 1, Generation: 2, CanonicalEpoch: 3, WriterEpoch: 4, AdapterVersion: "a1", SchemaVersion: "s1"}
	st := &scopeStore{current: f, jobs: map[string]ScopeJob{}}
	cfg := ScopeConfig{Fence: f, Limits: ScopeLimits{MaxJobs: 2, MaxAccountBytes: 8, MaxTailEvents: 2, MaxTailBytes: 8, MaxTailAgeMS: 50, MaxBaselineAgeMS: 100, ShardAccounts: 1}, Catalog: map[string][]ScopeDependency{"p": {{ID: "a", MaxBytes: 2}, {ID: "b", MaxBytes: 2}}, "q": {{ID: "c", MaxBytes: 2}}}, TrustedSources: []string{"trusted"}}
	ctl, err := NewScopeController(cfg, st, func() uint64 { return 1000 })
	if err != nil {
		t.Fatal(err)
	}
	req := EnsurePoolScopeRequest{Fence: f, Anchor: ScopeAnchor{Height: 11, Hash: "h11", TimestampMS: 990, Cursor: 11}, PoolIDs: []string{"p"}, Budget: cfg.Limits, HistoryAfter: 1}
	src := &scopeSource{store: st, data: map[string][]byte{"a": {1}, "b": {2}}}
	candidate := BaselineCandidate{Fence: f, Anchor: ScopeAnchor{Height: 10, Hash: "h10", TimestampMS: 980, Cursor: 10}, Source: "trusted", Kind: HealthyPeer, PoolIDs: []string{"p"}, Dependencies: []ScopeDependency{{ID: "a", MaxBytes: 2}, {ID: "b", MaxBytes: 2}}, AccountHashes: map[string]string{"a": digestBytes([]byte{1}), "b": digestBytes([]byte{2})}}
	candidate.ContentHash = BaselineHash(candidate)
	src.candidates = []BaselineCandidate{candidate}
	return ctl, st, src, req
}
func TestScopeAdmission(t *testing.T) {
	ctl, st, _, req := scopeFixture(t)
	a, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil {
		t.Fatal(e)
	}
	req.PoolIDs = []string{"p", "p"}
	b, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil || a.ID != b.ID || len(st.jobs) != 1 {
		t.Fatalf("idempotency: %v %v", b, e)
	}
	req.PoolIDs = []string{"unknown"}
	if _, e = ctl.EnsurePoolScope(context.Background(), req); !errors.Is(e, ErrUnknownPool) {
		t.Fatal(e)
	}
	req.PoolIDs = []string{"p"}
	req.Budget.MaxAccountBytes = 3
	if _, e = ctl.EnsurePoolScope(context.Background(), req); !errors.Is(e, ErrBudget) {
		t.Fatal(e)
	}
	req.Budget = ctl.config.Limits
	req.Fence.Generation++
	if _, e = ctl.EnsurePoolScope(context.Background(), req); !errors.Is(e, ErrFenceChanged) {
		t.Fatal(e)
	}
	// Concurrent quota enforcement belongs to the store transaction, including duplicates.
	st.current = ctl.config.Fence
	req.Fence = st.current
	req.PoolIDs = []string{"q"}
	req.Anchor.Hash = "other"
	if _, e = ctl.EnsurePoolScope(context.Background(), req); e != nil {
		t.Fatal(e)
	}
	req.Anchor.Hash = "third"
	if _, e = ctl.EnsurePoolScope(context.Background(), req); !errors.Is(e, ErrBudget) {
		t.Fatal(e)
	}
}
func TestScopeRecoveryAtomic(t *testing.T) {
	ctl, st, src, req := scopeFixture(t)
	j, e := ctl.EnsurePoolScope(context.Background(), req)
	if e != nil {
		t.Fatal(e)
	}
	if e = ctl.Recover(context.Background(), j.ID, src); e != nil {
		t.Fatal(e)
	}
	if st.jobs[j.ID].Phase != ScopedReady || st.published == nil {
		t.Fatal("not published")
	}
	if !reflect.DeepEqual(src.order, []string{"buffer", "anchor", "shard", "shard", "tail", "release"}) {
		t.Fatal(src.order)
	}
	if st.published.HistoryGap == nil || st.published.HistoryGap.After != 1 {
		t.Fatal("missing historical gap")
	}
	if st.published.Fence != req.Fence || len(st.published.Accounts) != 2 {
		t.Fatal("scope lost")
	}
	if e = ctl.Recover(context.Background(), j.ID, src); e != nil || len(src.order) != 6 {
		t.Fatal("ready retry must not run IO", e)
	}
}
func TestScopeRecoveryFailClosed(t *testing.T) {
	for _, kind := range []string{"shard", "hash", "gap", "cancel", "epoch", "checkpoint", "publish"} {
		t.Run(kind, func(t *testing.T) {
			ctl, st, src, req := scopeFixture(t)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			switch kind {
			case "shard":
				src.badShard = true
			case "hash":
				src.data["a"] = []byte{9}
			case "gap":
				src.gap = true
			case "cancel":
				src.hook = cancel
			case "epoch":
				src.hook = func() { st.current.CanonicalEpoch++ }
			case "checkpoint":
				st.failAt = Restoring
			case "publish":
				st.publishErr = true
			}
			j, e := ctl.EnsurePoolScope(ctx, req)
			if e != nil {
				t.Fatal(e)
			}
			if e = ctl.Recover(ctx, j.ID, src); e == nil {
				t.Fatal("expected failure")
			}
			if st.published != nil || st.jobs[j.ID].Phase == ScopedReady {
				t.Fatal("partial ready")
			}
			if kind == "gap" && (st.jobs[j.ID].Phase != HeadFirstRequired || st.jobs[j.ID].HistoryGap == nil) {
				t.Fatal("HEAD_FIRST and durable gap required")
			}
		})
	}
}
func TestScopeBaselineValidation(t *testing.T) {
	ctl, _, src, req := scopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	for _, kind := range []string{"identity", "version", "scope", "age", "hash", "untrusted", "dependency"} {
		t.Run(kind, func(t *testing.T) {
			c := src.candidates[0]
			c.Dependencies = append([]ScopeDependency{}, c.Dependencies...)
			switch kind {
			case "identity":
				c.Fence.ChainIdentity = "fork"
			case "version":
				c.Fence.AdapterVersion = "old"
			case "scope":
				c.PoolIDs = []string{"q"}
			case "age":
				c.Anchor.TimestampMS = 100
			case "hash":
				c.ContentHash = "bad"
			case "untrusted":
				c.Source = "unknown"
			case "dependency":
				c.Dependencies = c.Dependencies[:1]
			}
			if kind != "hash" {
				c.ContentHash = BaselineHash(c)
			}
			if _, e := SelectScopeBaseline(j, []BaselineCandidate{c}, ctl.config, 1000); !errors.Is(e, ErrNoBaseline) {
				t.Fatal(e)
			}
		})
	}
}

type historySink struct{ records []HistoryRecord }

func (s *historySink) AppendHistory(_ context.Context, r HistoryRecord, _ HistoryLimits) error {
	s.records = append(s.records, r)
	return nil
}
func TestScopeHistoryIsolation(t *testing.T) {
	sink := &historySink{}
	h, e := NewHistoryWriter(sink, "sol", "cap-secret", HistoryLimits{MaxJobs: 1, MaxBytes: 3})
	if e != nil {
		t.Fatal(e)
	}
	r := HistoryRecord{ChainKey: "sol", Namespace: "history_replay", Cursor: 2, Payload: []byte{1}}
	if e = h.Write(context.Background(), "wrong", r); !errors.Is(e, ErrHistoryCapability) {
		t.Fatal(e)
	}
	r.Namespace = "online"
	if e = h.Write(context.Background(), "cap-secret", r); !errors.Is(e, ErrHistoryNamespace) {
		t.Fatal(e)
	}
	r.Namespace = "history_replay"
	r.Payload = []byte{1, 2, 3, 4}
	if e = h.Write(context.Background(), "cap-secret", r); !errors.Is(e, ErrBudget) {
		t.Fatal(e)
	}
	r.Payload = []byte{1}
	if e = h.Write(context.Background(), "cap-secret", r); e != nil || len(sink.records) != 1 {
		t.Fatal(e)
	}
}

func TestScopeRejectsCorruptedPersistedJob(t *testing.T) {
	ctl, st, src, req := scopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	// A disk record may not silently turn the declared two-account pool into one.
	j.Dependencies = j.Dependencies[:1]
	j.ReservedBytes = 2
	st.jobs[j.ID] = j
	src.candidates[0].Dependencies = j.Dependencies
	delete(src.candidates[0].AccountHashes, "b")
	src.candidates[0].ContentHash = BaselineHash(src.candidates[0])
	if e := ctl.Recover(context.Background(), j.ID, src); !errors.Is(e, ErrScopeData) {
		t.Fatalf("corrupt job accepted: %v", e)
	}
	if st.published != nil {
		t.Fatal("published an incomplete catalog")
	}
}
func TestScopeTailMustCoverCursorInterval(t *testing.T) {
	ctl, st, src, req := scopeFixture(t)
	src.tailOverride = func(tail *ScopeTail) { tail.Events = 0 }
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	if e := ctl.Recover(context.Background(), j.ID, src); !errors.Is(e, ErrHeadFirst) {
		t.Fatalf("cursor interval accepted without events: %v", e)
	}
	if st.published != nil {
		t.Fatal("published a gap")
	}
}
func TestScopeAdmissionConcurrentQuota(t *testing.T) {
	ctl, st, _, req := scopeFixture(t)
	var wg sync.WaitGroup
	var successes int
	var mu sync.Mutex
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			r := req
			r.Anchor.Hash = fmt.Sprintf("h%d", i)
			_, err := ctl.EnsurePoolScope(context.Background(), r)
			mu.Lock()
			defer mu.Unlock()
			if err == nil {
				successes++
			} else if !errors.Is(err, ErrBudget) {
				t.Error(err)
			}
		}(i)
	}
	wg.Wait()
	if successes != 2 || len(st.jobs) != 2 {
		t.Fatalf("quota escaped: %d jobs=%d", successes, len(st.jobs))
	}
}
func TestScopeBufferAndTailBounds(t *testing.T) {
	for _, kind := range []string{"durable", "retention", "events", "bytes", "anchor", "account_bytes"} {
		t.Run(kind, func(t *testing.T) {
			ctl, st, src, req := scopeFixture(t)
			switch kind {
			case "durable":
				src.bufferOverride = func(b *StreamBuffer) { b.Durable = false }
			case "retention":
				src.bufferOverride = func(b *StreamBuffer) { b.FirstCursor = 30 }
			case "events":
				src.tailOverride = func(tail *ScopeTail) { tail.Events = 3 }
			case "bytes":
				src.tailOverride = func(tail *ScopeTail) { tail.Bytes = 9 }
			case "anchor":
				src.tailOverride = func(tail *ScopeTail) { tail.To.Hash = "fork" }
			case "account_bytes":
				src.tailOverride = func(tail *ScopeTail) { tail.Accounts[0].Data = []byte{1, 2, 3} }
			}
			j, _ := ctl.EnsurePoolScope(context.Background(), req)
			if err := ctl.Recover(context.Background(), j.ID, src); err == nil {
				t.Fatal("bound accepted")
			}
			if st.published != nil {
				t.Fatal("partial publish")
			}
		})
	}
}
func TestScopeReclaimedAttemptStartsOver(t *testing.T) {
	ctl, st, src, req := scopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	// Model store-authorized expired lease reclamation, NOT process durability.
	j.Attempt = 1
	j.Phase = ScopeFailed
	j.Cursor = 999
	j.RestoredAccounts = 2
	j.BufferID = "expired"
	st.jobs[j.ID] = j
	if err := ctl.Recover(context.Background(), j.ID, src); err != nil {
		t.Fatal(err)
	}
	if st.jobs[j.ID].Attempt != 2 || src.order[0] != "buffer" || st.published.BufferID == "expired" {
		t.Fatal("resumed an unverified cursor")
	}
}

func TestScopeCrossLanguageManifestGolden(t *testing.T) {
	ctl, _, src, req := scopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	if j.ID != "cf093763d6a226c410f7676a3635fe8dfac2df9f436d1658773250dbb2f61b48" {
		t.Fatal(j.ID)
	}
	if src.candidates[0].ContentHash != "464b2026484bfc692874127d6b8631d71bf432fbcd9abf04ac6bcfa8e13691ce" {
		t.Fatal(src.candidates[0].ContentHash)
	}
}
func TestScopeCandidatePreferenceAndCompleteDependencies(t *testing.T) {
	ctl, _, src, req := scopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	baseline := src.candidates[0]
	current := baseline
	current.Kind = CurrentChainState
	current.Anchor = req.Anchor
	current.ContentHash = BaselineHash(current)
	best, e := SelectScopeBaseline(j, []BaselineCandidate{current, baseline}, ctl.config, 1000)
	if e != nil || best.Kind != HealthyPeer {
		t.Fatalf("priority: %v %v", best, e)
	}
	newer := baseline
	newer.Anchor = req.Anchor
	newer.ContentHash = BaselineHash(newer)
	best, e = SelectScopeBaseline(j, []BaselineCandidate{baseline, newer}, ctl.config, 1000)
	if e != nil || best.Anchor != req.Anchor {
		t.Fatalf("freshest qualified peer: %v %v", best, e)
	}
	// Shared accounts count once, with conflicting size declarations rejected.
	cfg := ctl.config
	cfg.Catalog = map[string][]ScopeDependency{"p": {{ID: "a", MaxBytes: 2}, {ID: "b", MaxBytes: 2}}, "q": {{ID: "b", MaxBytes: 2}}}
	st := &scopeStore{current: cfg.Fence, jobs: map[string]ScopeJob{}}
	c, e := NewScopeController(cfg, st, func() uint64 { return 1000 })
	if e != nil {
		t.Fatal(e)
	}
	req.PoolIDs = []string{"q", "p"}
	j, e = c.EnsurePoolScope(context.Background(), req)
	if e != nil || j.ReservedBytes != 4 || len(j.Dependencies) != 2 {
		t.Fatalf("shared dependency budget: %v %v", j, e)
	}
	cfg.Catalog["q"] = []ScopeDependency{{ID: "b", MaxBytes: 3}}
	c, e = NewScopeController(cfg, st, func() uint64 { return 1000 })
	if e != nil {
		t.Fatal(e)
	}
	if _, e = c.EnsurePoolScope(context.Background(), req); !errors.Is(e, ErrScopeData) {
		t.Fatal(e)
	}
}

func TestScopeBaselineFallsBackWithinTailBudget(t *testing.T) {
	ctl, _, src, req := scopeFixture(t)
	j, _ := ctl.EnsurePoolScope(context.Background(), req)
	peer := src.candidates[0]
	peer.Anchor.Cursor = 1
	peer.Anchor.Height = 1
	peer.ContentHash = BaselineHash(peer)
	current := src.candidates[0]
	current.Kind = CurrentChainState
	current.Anchor = req.Anchor
	current.ContentHash = BaselineHash(current)
	b, e := SelectScopeBaseline(j, []BaselineCandidate{peer, current}, ctl.config, 1000)
	if e != nil || b.Kind != CurrentChainState {
		t.Fatalf("out-of-budget peer blocked current baseline: %v %v", b.Kind, e)
	}
	if _, e = SelectScopeBaseline(j, []BaselineCandidate{peer}, ctl.config, 1000); !errors.Is(e, ErrHeadFirst) {
		t.Fatal("long tail must request HEAD_FIRST", e)
	}
}
