package bootstrap

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"errors"
	"fmt"
	"slices"
	"sort"
	"strconv"
	"strings"
)

var (
	ErrBudget            = errors.New("SCOPE_BUDGET")
	ErrUnknownPool       = errors.New("SCOPE_UNKNOWN_POOL")
	ErrFenceChanged      = errors.New("SCOPE_FENCE_CHANGED")
	ErrBusy              = errors.New("SCOPE_BUSY")
	ErrNoBaseline        = errors.New("SCOPE_NO_TRUSTED_BASELINE")
	ErrHeadFirst         = errors.New("SCOPE_HEAD_FIRST_REQUIRED")
	ErrScopeData         = errors.New("SCOPE_DATA_INVALID")
	ErrScopeStale        = errors.New("SCOPE_ANCHOR_STALE")
	ErrHistoryCapability = errors.New("HISTORY_CAPABILITY")
	ErrHistoryNamespace  = errors.New("HISTORY_NAMESPACE")
)

// Fence is the CURRENT owner-authorized online namespace. The service must obtain
// it from its active lease/config, never from an untrusted caller's claims.
type Fence struct {
	ChainKey       string `json:"chain_key"`
	ChainIdentity  string `json:"chain_identity"`
	Owner          string `json:"owner"`
	Assignment     uint64 `json:"assignment"`
	Generation     uint64 `json:"online_generation"`
	CanonicalEpoch uint64 `json:"canonical_epoch"`
	WriterEpoch    uint64 `json:"writer_epoch"`
	AdapterVersion string `json:"adapter_version"`
	SchemaVersion  string `json:"schema_version"`
}

// Cursor is the durable recovery-journal sequence for this Fence, not a raw
// block height, RPC slot or per-session write_version. Source adapters must
// prove that candidate and buffer cursors share that same timeline.
type ScopeAnchor struct {
	Height      uint64 `json:"height"`
	Hash        string `json:"hash"`
	TimestampMS uint64 `json:"timestamp_ms"`
	Cursor      uint64 `json:"cursor"`
}

// MaxBytes includes every versioned account required by the pool, including
// dynamic dependencies. Incomplete catalog discovery must never be admitted.
type ScopeDependency struct {
	ID       string `json:"id"`
	MaxBytes uint64 `json:"max_bytes"`
}
type ScopeLimits struct {
	MaxJobs          uint64 `json:"max_jobs"`
	MaxAccountBytes  uint64 `json:"max_account_bytes"`
	MaxTailEvents    uint64 `json:"max_tail_events"`
	MaxTailBytes     uint64 `json:"max_tail_bytes"`
	MaxTailAgeMS     uint64 `json:"max_tail_age_ms"`
	MaxBaselineAgeMS uint64 `json:"max_baseline_age_ms"`
	ShardAccounts    uint64 `json:"shard_accounts"`
}
type ScopeConfig struct {
	Fence          Fence
	Limits         ScopeLimits
	Catalog        map[string][]ScopeDependency
	TrustedSources []string
}
type EnsurePoolScopeRequest struct {
	Fence        Fence       `json:"fence"`
	Anchor       ScopeAnchor `json:"anchor"`
	PoolIDs      []string    `json:"pool_ids"`
	Budget       ScopeLimits `json:"budget"`
	HistoryAfter uint64      `json:"history_after"`
}
type ScopePhase string

const (
	Admitted          ScopePhase = "ADMITTED"
	LiveBuffering     ScopePhase = "LIVE_BUFFERING"
	BaselineAcquiring ScopePhase = "BASELINE_ACQUIRING"
	Restoring         ScopePhase = "RESTORING"
	TailAligning      ScopePhase = "TAIL_ALIGNING"
	ScopedReady       ScopePhase = "SCOPED_READY"
	ScopeFailed       ScopePhase = "FAILED"
	HeadFirstRequired ScopePhase = "HEAD_FIRST_REQUIRED"
)

func (p ScopePhase) Terminal() bool {
	return p == ScopedReady || p == ScopeFailed || p == HeadFirstRequired
}

type HistoryGap struct {
	After   uint64 `json:"after"`
	Through uint64 `json:"through"`
	Reason  string `json:"reason"`
}

// ScopeJob is durable control state, not a live in-memory readiness flag. A
// reclaimed attempt always restarts buffering and restores every shard anew.
type ScopeJob struct {
	ID string `json:"id"`
	EnsurePoolScopeRequest
	Dependencies     []ScopeDependency `json:"dependencies"`
	ReservedBytes    uint64            `json:"reserved_bytes"`
	Phase            ScopePhase        `json:"phase"`
	Attempt          uint64            `json:"attempt"`
	BufferID         string            `json:"buffer_id"`
	BaselineHash     string            `json:"baseline_hash"`
	Cursor           uint64            `json:"cursor"`
	RestoredAccounts uint64            `json:"restored_accounts"`
	Failure          string            `json:"failure"`
	HistoryGap       *HistoryGap       `json:"history_gap,omitempty"`
}
type Admission struct {
	Job    ScopeJob
	Limits ScopeLimits
}

// ScopeStore MUST implement all writes as durable transactions in the existing
// owner store (Pebble/RocksDB/prepared journal), shared across controller instances:
//   - Admit checks the active Fence/lease, deduplicates ID and reserves aggregate
//     job + account-byte quotas atomically. Terminal jobs release reservations.
//   - Claim returns ready unchanged, otherwise exclusively leases a new Attempt;
//     busy attempts fail. After a crash only an expired/fenced attempt is reclaimed.
//     A terminal retry must atomically reacquire the original aggregate quotas.
//   - Checkpoint/Fail CAS on ID+Attempt under the current lease. Fail records the
//     error/history gap and drops all unpublished staging, even after cancellation.
//   - Publish rechecks context, CURRENT Fence, lease AND Attempt in the same
//     transaction as all account state, cursor, history-gap metadata and READY.
//     Recheck the original anchor age on READY reads and at the final commit
//     gate after staging I/O; do not renew its freshness deadline.
//     No shard is visible early. For an online owner DB adapter the same
//     transaction MUST include its prepared journal/generation visibility.
//
// FileScopeStore supplies durable RECOVERY_DATA_ONLY output. It cannot publish
// the existing owner DB or authorize quote readiness. ProductionStore below
// delegates directly to an online DB transaction backend. Terminal retention/GC
// is an explicit store policy.
type ScopeStore interface {
	Current(context.Context, string) (Fence, error)
	Admit(context.Context, Admission) (ScopeJob, error)
	Claim(context.Context, string, Fence) (ScopeJob, error)
	Checkpoint(context.Context, ScopeJob) error
	Fail(context.Context, ScopeJob) error
	Publish(context.Context, ScopeJob, Publication) error
}
type BaselineKind uint8

const (
	HealthyPeer BaselineKind = iota
	RecentSnapshot
	CurrentChainState
)

type BaselineCandidate struct {
	Fence         Fence
	Anchor        ScopeAnchor
	Source        string
	Kind          BaselineKind
	PoolIDs       []string
	Dependencies  []ScopeDependency
	AccountHashes map[string]string
	ContentHash   string
}
type StreamBuffer struct {
	ID          string
	FirstCursor uint64
	Durable     bool
}
type ScopeAccount struct {
	ID     string
	Data   []byte
	Anchor ScopeAnchor
}
type ScopeTail struct {
	From, To      ScopeAnchor
	Complete      bool
	Events, Bytes uint64
	Accounts      []ScopeAccount
}
type Publication struct {
	Fence                  Fence
	Anchor                 ScopeAnchor
	PoolIDs                []string
	Dependencies           []ScopeDependency
	Accounts               []ScopeAccount
	BaselineHash, BufferID string
	Cursor                 uint64
	HistoryGap             *HistoryGap
}

// RecoverySource must bound network/WAL allocation BEFORE reading by job.Budget
// and dependency.MaxBytes, and honor ctx. Complete is a verified continuous
// source-session boundary, not merely receipt of a head notification. Restore
// verifies versioned bytes at candidate.Anchor; current RPC context alone does
// not establish a historical bank or continuous Yellowstone stream.
// AlignTail returns the complete final scope at job.Anchor, never partial deltas.
// ReleaseBuffer only releases this attempt's handle; it must not erase durable
// journal data still needed by the committed generation.
type RecoverySource interface {
	StartBuffer(context.Context, ScopeJob) (StreamBuffer, error)
	FixAnchor(context.Context, ScopeJob, StreamBuffer) (ScopeAnchor, error)
	Candidates(context.Context, ScopeJob, ScopeAnchor) ([]BaselineCandidate, error)
	RestoreShard(context.Context, ScopeJob, BaselineCandidate, []string) ([]ScopeAccount, error)
	AlignTail(context.Context, ScopeJob, StreamBuffer, BaselineCandidate, []ScopeAccount) (ScopeTail, error)
	ReleaseBuffer(context.Context, StreamBuffer) error
}
type ScopeController struct {
	config ScopeConfig
	store  ScopeStore
	now    func() uint64
}

func NewScopeController(c ScopeConfig, s ScopeStore, now func() uint64) (*ScopeController, error) {
	if s == nil || now == nil || !validFence(c.Fence) || !validLimits(c.Limits) || len(c.TrustedSources) == 0 {
		return nil, ErrScopeData
	}
	catalog := map[string][]ScopeDependency{}
	for id, deps := range c.Catalog {
		if id == "" || len(deps) == 0 {
			return nil, ErrScopeData
		}
		copyDeps := append([]ScopeDependency(nil), deps...)
		sort.Slice(copyDeps, func(i, j int) bool { return copyDeps[i].ID < copyDeps[j].ID })
		for i, d := range copyDeps {
			if d.ID == "" || d.MaxBytes == 0 || (i > 0 && d.ID == copyDeps[i-1].ID) {
				return nil, ErrScopeData
			}
		}
		catalog[id] = copyDeps
	}
	c.Catalog = catalog
	c.TrustedSources = append([]string(nil), c.TrustedSources...)
	return &ScopeController{config: c, store: s, now: now}, nil
}
func validFence(f Fence) bool {
	return f.ChainKey != "" && f.ChainIdentity != "" && f.Owner != "" && f.Assignment > 0 && f.Generation > 0 && f.CanonicalEpoch > 0 && f.WriterEpoch > 0 && f.AdapterVersion != "" && f.SchemaVersion != ""
}
func validLimits(b ScopeLimits) bool {
	return b.MaxJobs > 0 && b.MaxAccountBytes > 0 && b.MaxTailEvents > 0 && b.MaxTailBytes > 0 && b.MaxTailAgeMS > 0 && b.MaxBaselineAgeMS > 0 && b.ShardAccounts > 0
}
func minScope(a, b uint64) uint64 {
	if a < b {
		return a
	}
	return b
}
func capLimits(a, b ScopeLimits) ScopeLimits {
	return ScopeLimits{minScope(a.MaxJobs, b.MaxJobs), minScope(a.MaxAccountBytes, b.MaxAccountBytes), minScope(a.MaxTailEvents, b.MaxTailEvents), minScope(a.MaxTailBytes, b.MaxTailBytes), minScope(a.MaxTailAgeMS, b.MaxTailAgeMS), minScope(a.MaxBaselineAgeMS, b.MaxBaselineAgeMS), minScope(a.ShardAccounts, b.ShardAccounts)}
}
func canonicalPools(ids []string) []string {
	a := append([]string(nil), ids...)
	sort.Strings(a)
	out := []string{}
	for _, id := range a {
		if len(out) == 0 || out[len(out)-1] != id {
			out = append(out, id)
		}
	}
	return out
}
func (c *ScopeController) check(ctx context.Context, f Fence) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	current, e := c.store.Current(ctx, f.ChainKey)
	if e != nil {
		return e
	}
	if current != f || f != c.config.Fence {
		return ErrFenceChanged
	}
	return nil
}

// EnsurePoolScope is called AFTER HTTP authentication/authorization. Acceptance
// means a durable admitted job, not quote readiness. ID is stable across pool order.
func (c *ScopeController) EnsurePoolScope(ctx context.Context, r EnsurePoolScopeRequest) (ScopeJob, error) {
	if e := c.check(ctx, r.Fence); e != nil {
		return ScopeJob{}, e
	}
	if r.Anchor.Hash == "" || !validLimits(r.Budget) || len(r.PoolIDs) == 0 {
		return ScopeJob{}, ErrScopeData
	}
	r.PoolIDs = canonicalPools(r.PoolIDs)
	r.Budget = capLimits(r.Budget, c.config.Limits)
	deps := map[string]uint64{}
	for _, pool := range r.PoolIDs {
		ds, ok := c.config.Catalog[pool]
		if !ok {
			return ScopeJob{}, ErrUnknownPool
		}
		for _, d := range ds {
			if old, ok := deps[d.ID]; ok && old != d.MaxBytes {
				return ScopeJob{}, ErrScopeData
			}
			deps[d.ID] = d.MaxBytes
		}
	}
	j := ScopeJob{EnsurePoolScopeRequest: r, Phase: Admitted}
	for id, b := range deps {
		if b > r.Budget.MaxAccountBytes-j.ReservedBytes {
			return ScopeJob{}, ErrBudget
		}
		j.ReservedBytes += b
		j.Dependencies = append(j.Dependencies, ScopeDependency{ID: id, MaxBytes: b})
	}
	sort.Slice(j.Dependencies, func(i, k int) bool { return j.Dependencies[i].ID < j.Dependencies[k].ID })
	j.ID = scopeJobID(j)
	return c.store.Admit(ctx, Admission{Job: j, Limits: r.Budget})
}

// The length-framed SHA-256 format is shared with Rust and independent of JSON
// map ordering. BaselineHash binds the full manifest and each account content hash.
func frameRawHash(bytes []byte) string { h := sha256.Sum256(bytes); return hex.EncodeToString(h[:]) }

func frameHash(parts []string) string {
	var b strings.Builder
	for _, p := range parts {
		b.WriteString(strconv.Itoa(len(p)))
		b.WriteByte(':')
		b.WriteString(p)
	}
	h := sha256.Sum256([]byte(b.String()))
	return hex.EncodeToString(h[:])
}
func fenceParts(f Fence) []string {
	return []string{f.ChainKey, f.ChainIdentity, f.Owner, strconv.FormatUint(f.Assignment, 10), strconv.FormatUint(f.Generation, 10), strconv.FormatUint(f.CanonicalEpoch, 10), strconv.FormatUint(f.WriterEpoch, 10), f.AdapterVersion, f.SchemaVersion}
}
func anchorParts(a ScopeAnchor) []string {
	return []string{strconv.FormatUint(a.Height, 10), a.Hash, strconv.FormatUint(a.TimestampMS, 10), strconv.FormatUint(a.Cursor, 10)}
}
func manifestParts(f Fence, a ScopeAnchor, pools []string, deps []ScopeDependency) []string {
	p := append(fenceParts(f), anchorParts(a)...)
	ids := canonicalPools(pools)
	p = append(p, strconv.Itoa(len(ids)))
	p = append(p, ids...)
	ds := append([]ScopeDependency(nil), deps...)
	sort.Slice(ds, func(i, j int) bool { return ds[i].ID < ds[j].ID })
	p = append(p, strconv.Itoa(len(ds)))
	for _, d := range ds {
		p = append(p, d.ID, strconv.FormatUint(d.MaxBytes, 10))
	}
	return p
}
func scopeJobID(j ScopeJob) string {
	return frameHash(append([]string{"scope-job-v1"}, manifestParts(j.Fence, j.Anchor, j.PoolIDs, j.Dependencies)...))
}
func BaselineHash(c BaselineCandidate) string {
	p := append([]string{"scope-baseline-v1"}, manifestParts(c.Fence, c.Anchor, c.PoolIDs, c.Dependencies)...)
	p = append(p, c.Source, strconv.Itoa(int(c.Kind)))
	keys := make([]string, 0, len(c.AccountHashes))
	for id := range c.AccountHashes {
		keys = append(keys, id)
	}
	sort.Strings(keys)
	p = append(p, strconv.Itoa(len(keys)))
	for _, id := range keys {
		p = append(p, id, c.AccountHashes[id])
	}
	return frameHash(p)
}
func fresh(ts, now, max uint64) bool { return ts <= now && now-ts <= max }
func sameDependencies(a, b []ScopeDependency) bool {
	if len(a) != len(b) {
		return false
	}
	ds := append([]ScopeDependency(nil), a...)
	sort.Slice(ds, func(i, j int) bool { return ds[i].ID < ds[j].ID })
	for i, d := range ds {
		if d != b[i] {
			return false
		}
	}
	return true
}

// Use the original anchor timestamp and admitted age budget on every check.
// Reading READY or renewing authority must never renew the data's deadline.
func requireScopeFresh(j ScopeJob, now uint64) error {
	if !fresh(j.Anchor.TimestampMS, now, j.Budget.MaxTailAgeMS) {
		return ErrScopeStale
	}
	return nil
}

func samePools(a, b []string) bool {
	x := canonicalPools(a)
	if len(x) != len(b) || len(a) != len(x) {
		return false
	}
	for i, id := range x {
		if id != b[i] {
			return false
		}
	}
	return true
}
func SelectScopeBaseline(j ScopeJob, candidates []BaselineCandidate, cfg ScopeConfig, now uint64) (BaselineCandidate, error) {
	var best BaselineCandidate
	found := false
	insufficient := false
	for _, b := range candidates {
		trusted := false
		for _, s := range cfg.TrustedSources {
			if b.Source == s {
				trusted = true
			}
		}
		if !trusted || b.Kind > CurrentChainState || b.Fence != j.Fence || !samePools(b.PoolIDs, j.PoolIDs) || !sameDependencies(b.Dependencies, j.Dependencies) || b.Anchor.Hash == "" || !fresh(b.Anchor.TimestampMS, now, j.Budget.MaxBaselineAgeMS) || b.Anchor.Height > j.Anchor.Height || b.Anchor.Cursor > j.Anchor.Cursor || b.Anchor.TimestampMS > j.Anchor.TimestampMS || (b.Anchor.Height == j.Anchor.Height && b.Anchor.Hash != j.Anchor.Hash) || len(b.AccountHashes) != len(j.Dependencies) || b.ContentHash != BaselineHash(b) {
			continue
		}
		valid := true
		for _, d := range j.Dependencies {
			v := b.AccountHashes[d.ID]
			raw, e := hex.DecodeString(v)
			if e != nil || len(raw) != 32 || strings.ToLower(v) != v {
				valid = false
			}
		}
		if !valid {
			continue
		}
		if j.Anchor.Cursor-b.Anchor.Cursor > j.Budget.MaxTailEvents || j.Anchor.TimestampMS-b.Anchor.TimestampMS > j.Budget.MaxTailAgeMS {
			insufficient = true
			continue
		}
		if !found || b.Kind < best.Kind || (b.Kind == best.Kind && (b.Anchor.Cursor > best.Anchor.Cursor || (b.Anchor.Cursor == best.Anchor.Cursor && b.Anchor.TimestampMS > best.Anchor.TimestampMS))) {
			best = b
			found = true
		}
	}
	if !found {
		if insufficient {
			return BaselineCandidate{}, ErrHeadFirst
		}
		return BaselineCandidate{}, ErrNoBaseline
	}
	return best, nil
}
func validateAccounts(accounts []ScopeAccount, deps []ScopeDependency, anchor ScopeAnchor, hashes map[string]string) (uint64, error) {
	if len(accounts) != len(deps) {
		return 0, ErrScopeData
	}
	expected := map[string]uint64{}
	for _, d := range deps {
		expected[d.ID] = d.MaxBytes
	}
	seen := map[string]bool{}
	var total uint64
	for _, a := range accounts {
		budget, ok := expected[a.ID]
		if !ok || seen[a.ID] || a.Anchor != anchor || len(a.Data) == 0 || uint64(len(a.Data)) > budget {
			return 0, ErrScopeData
		}
		seen[a.ID] = true
		if hashes != nil {
			h := sha256.Sum256(a.Data)
			if hex.EncodeToString(h[:]) != hashes[a.ID] {
				return 0, ErrScopeData
			}
		}
		if uint64(len(a.Data)) > ^uint64(0)-total {
			return 0, ErrBudget
		}
		total += uint64(len(a.Data))
	}
	return total, nil
}

// Validate a reclaimed durable record against the current immutable catalog and
// policy. Disk corruption/config changes must not silently reduce pool coverage.
func (c *ScopeController) validateJob(j ScopeJob) error {
	if !validLimits(j.Budget) || capLimits(j.Budget, c.config.Limits) != j.Budget {
		return ErrBudget
	}
	if j.Anchor.Hash == "" || len(j.PoolIDs) == 0 || !slices.Equal(j.PoolIDs, canonicalPools(j.PoolIDs)) {
		return ErrScopeData
	}
	expected := map[string]uint64{}
	for _, pool := range j.PoolIDs {
		deps, ok := c.config.Catalog[pool]
		if !ok {
			return ErrUnknownPool
		}
		for _, d := range deps {
			if old, ok := expected[d.ID]; ok && old != d.MaxBytes {
				return ErrScopeData
			}
			expected[d.ID] = d.MaxBytes
		}
	}
	var total uint64
	deps := []ScopeDependency{}
	for id, b := range expected {
		if b > j.Budget.MaxAccountBytes-total {
			return ErrBudget
		}
		total += b
		deps = append(deps, ScopeDependency{ID: id, MaxBytes: b})
	}
	sort.Slice(deps, func(i, k int) bool { return deps[i].ID < deps[k].ID })
	if !slices.Equal(j.Dependencies, deps) || total != j.ReservedBytes || scopeJobID(j) != j.ID {
		return ErrScopeData
	}
	return nil
}

func (c *ScopeController) Recover(ctx context.Context, id string, source RecoverySource) (result error) {
	if source == nil {
		return ErrScopeData
	}
	if e := c.check(ctx, c.config.Fence); e != nil {
		return e
	}
	j, e := c.store.Claim(ctx, id, c.config.Fence)
	if e != nil {
		return e
	}
	if e = c.check(ctx, j.Fence); e != nil {
		return e
	}
	if j.Phase == ScopedReady {
		if e = c.validateJob(j); e != nil {
			return e
		}
		if e = c.check(ctx, j.Fence); e != nil {
			return e
		}
		return requireScopeFresh(j, c.now())
	}
	defer func() {
		if result != nil {
			j.Failure = result.Error()
			if errors.Is(result, ErrHeadFirst) {
				j.Phase = HeadFirstRequired
				j.HistoryGap = &HistoryGap{After: j.Cursor, Through: j.Anchor.Cursor, Reason: "STREAM_INSUFFICIENT_HEAD_FIRST"}
			} else {
				j.Phase = ScopeFailed
			}
			if e := c.store.Fail(context.WithoutCancel(ctx), j); e != nil {
				result = errors.Join(result, fmt.Errorf("scope failure persistence: %w", e))
			}
		}
	}()
	if e = c.check(ctx, j.Fence); e != nil {
		return e
	}
	if e = c.validateJob(j); e != nil {
		return e
	}
	// A crashed attempt cannot resume a staged cursor into READY. A fresh buffer,
	// manifest and ALL shards are required, irrespective of persisted phase.
	j.BufferID = ""
	j.BaselineHash = ""
	j.RestoredAccounts = 0
	j.Cursor = 0
	j.Failure = ""
	j.HistoryGap = nil
	save := func(p ScopePhase) error {
		if e := c.check(ctx, j.Fence); e != nil {
			return e
		}
		j.Phase = p
		return c.store.Checkpoint(ctx, j)
	}
	if e = save(LiveBuffering); e != nil {
		return e
	}
	buffer, e := source.StartBuffer(ctx, j)
	if e != nil {
		return e
	}
	defer source.ReleaseBuffer(context.WithoutCancel(ctx), buffer)
	if !buffer.Durable || buffer.ID == "" {
		return ErrHeadFirst
	}
	j.BufferID = buffer.ID
	if e = save(BaselineAcquiring); e != nil {
		return e
	}
	anchor, e := source.FixAnchor(ctx, j, buffer)
	if e != nil {
		return e
	}
	if anchor != j.Anchor {
		return ErrScopeData
	}
	if e = requireScopeFresh(j, c.now()); e != nil {
		return e
	}
	candidates, e := source.Candidates(ctx, j, anchor)
	if e != nil {
		return e
	}
	baseline, e := SelectScopeBaseline(j, candidates, c.config, c.now())
	if e != nil {
		return e
	}
	j.BaselineHash = baseline.ContentHash
	j.Cursor = baseline.Anchor.Cursor
	if buffer.FirstCursor > baseline.Anchor.Cursor && buffer.FirstCursor-baseline.Anchor.Cursor > 1 {
		available := []BaselineCandidate{}
		for _, candidate := range candidates {
			if buffer.FirstCursor <= candidate.Anchor.Cursor || buffer.FirstCursor-candidate.Anchor.Cursor <= 1 {
				available = append(available, candidate)
			}
		}
		baseline, e = SelectScopeBaseline(j, available, c.config, c.now())
		if e != nil {
			return ErrHeadFirst
		}
		j.BaselineHash = baseline.ContentHash
		j.Cursor = baseline.Anchor.Cursor
	}
	// Keep an immutable copy: source callbacks receive a candidate containing maps.
	// A zero-event tail must still match these authenticated manifest digests.
	verifiedHashes := make(map[string]string, len(baseline.AccountHashes))
	for id, hash := range baseline.AccountHashes {
		verifiedHashes[id] = hash
	}
	if j.HistoryAfter < baseline.Anchor.Cursor {
		j.HistoryGap = &HistoryGap{After: j.HistoryAfter, Through: baseline.Anchor.Cursor, Reason: "HEAD_FIRST_BASELINE"}
	}
	accounts := []ScopeAccount{}
	shardSize := int(minScope(j.Budget.ShardAccounts, uint64(len(j.Dependencies))))
	if shardSize == 0 {
		return ErrScopeData
	}
	for start := 0; start < len(j.Dependencies); start += shardSize {
		end := start + shardSize
		if end > len(j.Dependencies) {
			end = len(j.Dependencies)
		}
		if e = save(Restoring); e != nil {
			return e
		}
		deps := j.Dependencies[start:end]
		ids := make([]string, len(deps))
		for i, d := range deps {
			ids[i] = d.ID
		}
		shard, e := source.RestoreShard(ctx, j, baseline, ids)
		if e != nil {
			return e
		}
		if _, e = validateAccounts(shard, deps, baseline.Anchor, verifiedHashes); e != nil {
			return e
		}
		accounts = append(accounts, shard...)
		j.RestoredAccounts = uint64(len(accounts))
	}
	if e = save(TailAligning); e != nil {
		return e
	}
	tail, e := source.AlignTail(ctx, j, buffer, baseline, accounts)
	if e != nil {
		return e
	}
	if !tail.Complete || tail.Events != anchor.Cursor-baseline.Anchor.Cursor {
		return ErrHeadFirst
	}
	if tail.From != baseline.Anchor || tail.To != anchor {
		return ErrScopeData
	}
	if tail.Events > j.Budget.MaxTailEvents || tail.Bytes > j.Budget.MaxTailBytes {
		return ErrHeadFirst
	}
	var tailHashes map[string]string
	if tail.Events == 0 {
		tailHashes = verifiedHashes
	}
	n, e := validateAccounts(tail.Accounts, j.Dependencies, anchor, tailHashes)
	if e != nil {
		return e
	}
	if n > tail.Bytes {
		return ErrBudget
	}
	if e = requireScopeFresh(j, c.now()); e != nil {
		return e
	}
	if e = c.check(ctx, j.Fence); e != nil {
		return e
	}
	j.Cursor = anchor.Cursor
	return c.store.Publish(ctx, j, Publication{Fence: j.Fence, Anchor: anchor, PoolIDs: j.PoolIDs, Dependencies: j.Dependencies, Accounts: tail.Accounts, BaselineHash: j.BaselineHash, BufferID: buffer.ID, Cursor: j.Cursor, HistoryGap: j.HistoryGap})
}

// HistoryStore has no online publication method. Its implementation must use
// separate history_replay keys/queue, atomically enforce its OWN aggregate worker
// and byte quotas, and persist cursor/data in one transaction. The authenticated
// service issues a chain-bound opaque token; it is never accepted by ScopeStore.
type HistoryLimits struct{ MaxJobs, MaxBytes uint64 }
type HistoryRecord struct {
	ChainKey, Namespace string
	Cursor              uint64
	Payload             []byte
}
type HistoryStore interface {
	AppendHistory(context.Context, HistoryRecord, HistoryLimits) error
}
type HistoryWriter struct {
	store        HistoryStore
	chain, token string
	limits       HistoryLimits
}

func NewHistoryWriter(s HistoryStore, chain, token string, b HistoryLimits) (*HistoryWriter, error) {
	if s == nil || chain == "" || token == "" || b.MaxJobs == 0 || b.MaxBytes == 0 {
		return nil, ErrScopeData
	}
	return &HistoryWriter{s, chain, token, b}, nil
}
func (h *HistoryWriter) Write(ctx context.Context, token string, r HistoryRecord) error {
	if subtle.ConstantTimeCompare([]byte(h.token), []byte(token)) != 1 {
		return ErrHistoryCapability
	}
	if r.Namespace != "history_replay" || r.ChainKey != h.chain {
		return ErrHistoryNamespace
	}
	if len(r.Payload) == 0 || uint64(len(r.Payload)) > h.limits.MaxBytes {
		return ErrBudget
	}
	if e := ctx.Err(); e != nil {
		return e
	}
	return h.store.AppendHistory(ctx, r, h.limits)
}
