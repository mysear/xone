package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
)

// The acquisition marker precedes all baseline reads. EVM's source block is
// addressed by its immutable hash; the normal bounded collector resumes H+1
// after this fixed baseline. A zero-length tail proves only this exact H scope,
// never receipt completeness, consensus or a Solana historical bank.
func (s *Service) beginScopeAcquisition(chain config.Chain) (string, error) {
	raw, e := json.Marshal(map[string]any{"chain_key": chain.Key, "chain_identity": chain.Genesis, "owner": chain.Owner, "assignment": chain.Assignment, "config_hash": s.cfg.PublicHash, "previous": s.stores[chain.Key].DurableRef(), "started_at_unix_ns": time.Now().UnixNano()})
	if e != nil {
		return "", e
	}
	return s.stores[chain.Key].BeginScopeBuffer(raw)
}

type ownerScopeBackend struct {
	maintenance  *RawMaintenance
	runtimeGate  func() error
	fence        bootstrap.Fence
	ledgerDigest string
	ctx          context.Context
	store        *reducer.Store
	session      *writerSession
	snapshot     domain.Snapshot
}

func (b *ownerScopeBackend) Load() ([]byte, error) {
	raw, err := b.store.ScopeState()
	if err == nil {
		b.ledgerDigest = reducer.ScopeLedgerDigest(raw)
	}
	return raw, err
}
func (b *ownerScopeBackend) Save(raw []byte, gate func() error) error {
	candidate, err := ownerLedgerFence(raw)
	if err != nil {
		return err
	}
	if candidate != b.fence {
		return bootstrap.ErrFenceChanged
	}
	if err = b.checkLease(context.Background()); err != nil {
		return err
	}
	if err = b.store.CompareScopeState(b.ledgerDigest, raw, b.authorityGate(gate)); err != nil {
		return err
	}
	b.ledgerDigest = reducer.ScopeLedgerDigest(raw)
	return nil
}
func ownerLedgerFence(raw []byte) (bootstrap.Fence, error) {
	var envelope struct {
		Format   string `json:"format"`
		Checksum string `json:"checksum"`
		Payload  string `json:"payload"`
	}
	if json.Unmarshal(raw, &envelope) != nil || envelope.Format != "xone-online-recovery-v1" {
		return bootstrap.Fence{}, bootstrap.ErrScopeData
	}
	sum := sha256.Sum256([]byte(envelope.Payload))
	if hex.EncodeToString(sum[:]) != envelope.Checksum {
		return bootstrap.Fence{}, bootstrap.ErrScopeData
	}
	var ledger struct {
		Fence bootstrap.Fence `json:"fence"`
	}
	if json.Unmarshal([]byte(envelope.Payload), &ledger) != nil {
		return bootstrap.Fence{}, bootstrap.ErrScopeData
	}
	return ledger.Fence, nil
}
func (b *ownerScopeBackend) checkLease(ctx context.Context) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	if b.session == nil {
		return nil
	}
	lease := b.session.lease
	if lease.Chain != b.fence.ChainKey || lease.WriterEpoch != b.fence.WriterEpoch || lease.Generation != b.fence.Generation || lease.CanonicalEpoch != b.fence.CanonicalEpoch {
		return bootstrap.ErrFenceChanged
	}
	bounded, cancel := context.WithTimeout(ctx, time.Duration(b.session.cfg.Config.Control.RenewalMS)*time.Millisecond)
	defer cancel()
	return b.session.pg.Check(bounded, lease, 0)
}
func (b *ownerScopeBackend) CheckAuthority(ctx context.Context, expected bootstrap.Fence) error {
	actual := b.store.DurableRef()
	if actual.OnlineGeneration > expected.Generation || actual.WriterEpoch > expected.WriterEpoch || actual.CanonicalEpoch > expected.CanonicalEpoch {
		return bootstrap.ErrFenceChanged
	}
	if expected != b.fence {
		return bootstrap.ErrFenceChanged
	}
	raw, e := b.store.ScopeState()
	if e != nil {
		return e
	}
	active, e := ownerLedgerFence(raw)
	if e != nil {
		return e
	}
	if active != expected {
		return bootstrap.ErrFenceChanged
	}
	return b.checkLease(ctx)
}
func (b *ownerScopeBackend) authorityGate(gate func() error) func() error {
	return func() error {
		if b.runtimeGate != nil {
			if err := b.runtimeGate(); err != nil {
				return err
			}
		}
		if gate != nil {
			if e := gate(); e != nil {
				return e
			}
		}
		// This final gate runs under Store.mu. Remote PG.Check belongs to
		// CheckAuthority/Save and writerSession's prepare/publish boundaries.
		// Only the chain actor's already-observed lease may be read here.
		if b.session != nil {
			lease := b.session.lease
			if lease.Chain != b.fence.ChainKey || lease.WriterEpoch != b.fence.WriterEpoch || lease.Generation != b.fence.Generation || lease.CanonicalEpoch != b.fence.CanonicalEpoch || lease.ExpiresMS <= uint64(time.Now().UnixMilli()) {
				return bootstrap.ErrFenceChanged
			}
		}
		if gate != nil {
			return gate()
		}
		return nil
	}
}
func (b *ownerScopeBackend) OnlineCommit(ctx context.Context, id string) (bootstrap.OnlineScopeCommit, error) {
	if e := ctx.Err(); e != nil {
		return bootstrap.OnlineScopeCommit{}, e
	}
	if e := b.CheckAuthority(ctx, b.fence); e != nil {
		return bootstrap.OnlineScopeCommit{}, e
	}
	proof, e := b.store.ScopeCommit(id)
	if e != nil {
		return bootstrap.OnlineScopeCommit{}, e
	}
	var fence bootstrap.Fence
	if json.Unmarshal(proof.Fence, &fence) != nil {
		return bootstrap.OnlineScopeCommit{}, bootstrap.ErrScopeData
	}
	return bootstrap.OnlineScopeCommit{JobID: proof.JobID, Fence: fence, Cursor: proof.Reference.JournalSeq, CommitID: proof.CommitID}, ctx.Err()
}
func (b *ownerScopeBackend) Publish(raw []byte, j bootstrap.ScopeJob, p bootstrap.Publication, gate func() error) error {
	if e := b.ctx.Err(); e != nil {
		return e
	}
	ref := b.snapshot.Reference
	if j.Fence != p.Fence || p.Anchor.Hash != ref.Block.Hash || p.Anchor.Height != ref.Block.Number || p.Anchor.TimestampMS != ref.Block.TimestampMS || p.Cursor != ref.JournalSeq || p.Fence.Generation != ref.OnlineGeneration || p.Fence.CanonicalEpoch != ref.CanonicalEpoch || p.Fence.WriterEpoch != ref.WriterEpoch || p.Fence.AdapterVersion != ref.AdapterSetHash || p.Fence.SchemaVersion != fmt.Sprint(ref.SchemaVersion) || p.Fence.Owner != ref.OwnerService || p.Fence.Assignment != ref.OwnerAssignmentVersion || p.Fence.ChainKey != ref.Block.ChainKey {
		return bootstrap.ErrFenceChanged
	}
	expected := map[string][]byte{}
	for _, pool := range b.snapshot.Pools {
		data, e := json.Marshal(pool)
		if e != nil {
			return e
		}
		expected["pool:"+pool.PoolID] = data
	}
	if len(expected) != len(p.Accounts) {
		return bootstrap.ErrScopeData
	}
	seen := map[string]bool{}
	for _, account := range p.Accounts {
		data, ok := expected[account.ID]
		if !ok || seen[account.ID] || account.Anchor != p.Anchor || string(data) != string(account.Data) {
			return bootstrap.ErrScopeData
		}
		seen[account.ID] = true
	}
	if e := b.CheckAuthority(b.ctx, p.Fence); e != nil {
		return e
	}
	fence, e := json.Marshal(p.Fence)
	if e != nil {
		return e
	}
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b.snapshot}
	var records map[string]json.RawMessage
	if b.maintenance != nil {
		records, e = b.maintenance.BaselineOwnerRecords(b.ctx, unit, j.ID, fence)
		if e != nil {
			return e
		}
	}
	var err error
	if b.session != nil {
		if len(records) > 0 {
			err = b.session.publishScopeRecordsCAS(b.ctx, unit, raw, j.ID, fence, b.ledgerDigest, records, b.authorityGate(gate))
		} else {
			err = b.session.publishScopeCAS(b.ctx, unit, raw, j.ID, fence, b.ledgerDigest, b.authorityGate(gate))
		}
	} else {
		if len(records) > 0 {
			err = b.store.InstallBaselineScopeRecordsCAS(b.snapshot, raw, j.ID, fence, b.ledgerDigest, records, b.authorityGate(gate))
		} else {
			err = b.store.InstallBaselineScopeCAS(b.snapshot, raw, j.ID, fence, b.ledgerDigest, b.authorityGate(gate))
		}
	}
	if err != nil {
		return err
	}
	b.ledgerDigest = reducer.ScopeLedgerDigest(raw)
	if b.maintenance != nil && len(records) > 0 {
		return b.maintenance.AcknowledgeOwnerRecords(unit, records)
	}
	return nil
}

type currentOwnerRecoverySource struct {
	service   *Service
	chain     config.Chain
	snapshot  domain.Snapshot
	buffer    string
	data      map[string][]byte
	candidate bootstrap.BaselineCandidate
}

func (r *currentOwnerRecoverySource) StartBuffer(ctx context.Context, j bootstrap.ScopeJob) (bootstrap.StreamBuffer, error) {
	if e := ctx.Err(); e != nil {
		return bootstrap.StreamBuffer{}, e
	}
	if _, e := r.service.stores[r.chain.Key].ScopeBuffer(r.buffer); e != nil {
		return bootstrap.StreamBuffer{}, e
	}
	return bootstrap.StreamBuffer{ID: r.buffer, FirstCursor: j.Anchor.Cursor, Durable: true}, nil
}
func (r *currentOwnerRecoverySource) FixAnchor(ctx context.Context, j bootstrap.ScopeJob, buffer bootstrap.StreamBuffer) (bootstrap.ScopeAnchor, error) {
	if buffer.ID != r.buffer {
		return bootstrap.ScopeAnchor{}, bootstrap.ErrScopeData
	}
	if adapter := r.service.adapters[r.chain.Key]; adapter != nil {
		block, e := adapter.Header(ctx, fmt.Sprintf("0x%x", r.snapshot.Reference.Block.Number))
		if e != nil {
			return bootstrap.ScopeAnchor{}, e
		}
		if block != r.snapshot.Reference.Block {
			return bootstrap.ScopeAnchor{}, errors.New("SCOPE_BASELINE_ANCHOR_CHANGED")
		}
	}
	return j.Anchor, ctx.Err()
}
func (r *currentOwnerRecoverySource) Candidates(ctx context.Context, _ bootstrap.ScopeJob, _ bootstrap.ScopeAnchor) ([]bootstrap.BaselineCandidate, error) {
	return []bootstrap.BaselineCandidate{r.candidate}, ctx.Err()
}
func (r *currentOwnerRecoverySource) RestoreShard(ctx context.Context, j bootstrap.ScopeJob, c bootstrap.BaselineCandidate, ids []string) ([]bootstrap.ScopeAccount, error) {
	if c.ContentHash != r.candidate.ContentHash || c.Fence != j.Fence {
		return nil, bootstrap.ErrScopeData
	}
	result := make([]bootstrap.ScopeAccount, 0, len(ids))
	var bytes uint64
	for _, id := range ids {
		if e := ctx.Err(); e != nil {
			return nil, e
		}
		data, ok := r.data[id]
		if !ok || uint64(len(data)) > j.Budget.MaxAccountBytes-bytes {
			return nil, bootstrap.ErrBudget
		}
		bytes += uint64(len(data))
		result = append(result, bootstrap.ScopeAccount{ID: id, Data: append([]byte(nil), data...), Anchor: c.Anchor})
	}
	return result, nil
}
func (r *currentOwnerRecoverySource) AlignTail(ctx context.Context, j bootstrap.ScopeJob, _ bootstrap.StreamBuffer, c bootstrap.BaselineCandidate, accounts []bootstrap.ScopeAccount) (bootstrap.ScopeTail, error) {
	if c.Anchor != j.Anchor {
		return bootstrap.ScopeTail{}, bootstrap.ErrHeadFirst
	}
	var bytes uint64
	for _, a := range accounts {
		if e := ctx.Err(); e != nil {
			return bootstrap.ScopeTail{}, e
		}
		if uint64(len(a.Data)) > j.Budget.MaxTailBytes-bytes {
			return bootstrap.ScopeTail{}, bootstrap.ErrBudget
		}
		bytes += uint64(len(a.Data))
	}
	return bootstrap.ScopeTail{From: c.Anchor, To: j.Anchor, Complete: true, Events: 0, Bytes: bytes, Accounts: accounts}, ctx.Err()
}
func (r *currentOwnerRecoverySource) ReleaseBuffer(_ context.Context, b bootstrap.StreamBuffer) error {
	return r.service.stores[r.chain.Key].ReleaseScopeBuffer(b.ID)
}

func (s *Service) publishRecoveredScope(ctx context.Context, chain config.Chain, snapshot domain.Snapshot, buffer string, previous domain.SnapshotRef, selection *RuntimeScopeSelection) error {
	store := s.stores[chain.Key]
	if len(snapshot.Pools) == 0 {
		return errors.New("EMPTY_POOL_SCOPE")
	}
	now := func() uint64 { return uint64(time.Now().UnixMilli()) }
	ref := snapshot.Reference
	fence := bootstrap.Fence{ChainKey: chain.Key, ChainIdentity: chain.Genesis, Owner: chain.Owner, Assignment: chain.Assignment, Generation: ref.OnlineGeneration, CanonicalEpoch: ref.CanonicalEpoch, WriterEpoch: ref.WriterEpoch, AdapterVersion: ref.AdapterSetHash, SchemaVersion: fmt.Sprint(ref.SchemaVersion)}
	budget := uint64(s.cfg.Config.Journal.MaxBytes)
	limits := bootstrap.ScopeLimits{MaxJobs: 1, MaxAccountBytes: budget, MaxTailEvents: s.cfg.Config.Startup.MaxTail, MaxTailBytes: budget, MaxTailAgeMS: s.cfg.Config.Startup.MaxAge, MaxBaselineAgeMS: s.cfg.Config.Startup.MaxAge, ShardAccounts: 32}
	stateLimit := budget*4 + 1<<20
	if stateLimit > 64<<20 {
		stateLimit = 64 << 20
	}
	diskLimits := bootstrap.FileScopeLimits{MaxJobs: 1, MaxAccountBytes: budget, MaxRetainedJobs: 64, MaxSnapshotBytes: stateLimit, AttemptLeaseMS: s.cfg.Config.Control.LeaseTTLMS, MaxAuthorityLeaseMS: s.cfg.Config.Control.LeaseTTLMS}
	backend := &ownerScopeBackend{maintenance: s.rawMaintenance, runtimeGate: s.runtimeScopeGate(ctx, selection, snapshot), fence: fence, ctx: ctx, store: store, session: s.sessions[chain.Key], snapshot: snapshot}
	if old := s.recoveries[chain.Key]; old != nil {
		if e := old.Close(); e != nil {
			return e
		}
		delete(s.recoveries, chain.Key)
	}
	engine, e := bootstrap.OpenEmbeddedScopeStore(backend, fence, diskLimits, now)
	if e != nil {
		return e
	}
	s.recoveries[chain.Key] = engine
	until := now() + s.cfg.Config.Control.LeaseTTLMS
	if backend.session != nil {
		until = uint64(backend.session.renewed.Add(time.Duration(s.cfg.Config.Control.LeaseTTLMS) * time.Millisecond).UnixMilli())
	}
	if e = engine.Authorize(ctx, fence, until); e != nil {
		return e
	}
	if _, e = engine.PruneSuperseded(ctx); e != nil {
		return e
	}
	source := &currentOwnerRecoverySource{service: s, chain: chain, snapshot: snapshot, buffer: buffer, data: map[string][]byte{}}
	catalog := map[string][]bootstrap.ScopeDependency{}
	hashes := map[string]string{}
	ids := make([]string, 0, len(snapshot.Pools))
	dependencies := []bootstrap.ScopeDependency{}
	var total uint64
	for _, pool := range snapshot.Pools {
		data, err := json.Marshal(pool)
		if err != nil {
			return err
		}
		if uint64(len(data)) > budget-total {
			return bootstrap.ErrBudget
		}
		total += uint64(len(data))
		id := "pool:" + pool.PoolID
		dep := bootstrap.ScopeDependency{ID: id, MaxBytes: uint64(len(data))}
		catalog[pool.PoolID] = []bootstrap.ScopeDependency{dep}
		dependencies = append(dependencies, dep)
		ids = append(ids, pool.PoolID)
		source.data[id] = data
		hash := sha256.Sum256(data)
		hashes[id] = hex.EncodeToString(hash[:])
	}
	sort.Strings(ids)
	sort.Slice(dependencies, func(i, j int) bool { return dependencies[i].ID < dependencies[j].ID })
	anchor := bootstrap.ScopeAnchor{Height: ref.Block.Number, Hash: ref.Block.Hash, TimestampMS: ref.Block.TimestampMS, Cursor: ref.JournalSeq}
	source.candidate = bootstrap.BaselineCandidate{Fence: fence, Anchor: anchor, Source: "owner-current-source:" + chain.Key, Kind: bootstrap.CurrentChainState, PoolIDs: ids, Dependencies: dependencies, AccountHashes: hashes}
	source.candidate.ContentHash = bootstrap.BaselineHash(source.candidate)
	controller, e := bootstrap.NewScopeController(bootstrap.ScopeConfig{Fence: fence, Limits: limits, Catalog: catalog, TrustedSources: []string{source.candidate.Source}}, engine, now)
	if e != nil {
		return e
	}
	job, e := controller.EnsurePoolScope(ctx, bootstrap.EnsurePoolScopeRequest{Fence: fence, Anchor: anchor, PoolIDs: ids, Budget: limits, HistoryAfter: previous.JournalSeq})
	if e != nil {
		return e
	}
	if e = controller.Recover(ctx, job.ID, source); e != nil {
		return e
	}
	proof, e := engine.OnlineCommit(ctx, job.ID)
	if e != nil {
		return e
	}
	if proof.Fence != fence || proof.Cursor != ref.JournalSeq {
		return bootstrap.ErrScopeData
	}
	s.log.Event("info", "scope_owner_committed", map[string]any{"chain_key": chain.Key, "job_id": job.ID, "online_generation": ref.OnlineGeneration, "scope_pool_count": len(ids), "history_blocks_readiness": false, "source_kind": "fixed-current-EVM-state", "tail_events": 0})
	return nil
}

// A catalog may be much larger than the first qualified online scope. Reuse
// only prior pool identities (never their numeric state), restore at most 32 on
// a cold start, and expand an existing scope only through admitted requests.
func (s *Service) targetScopeChain(chain config.Chain, requested []string) (config.Chain, error) {
	if len(chain.Pools) == 0 {
		return chain, nil
	}
	known := map[string]config.PoolConfig{}
	for _, p := range chain.Pools {
		known[p.ID] = p
	}
	wanted := map[string]bool{}
	previous, e := s.stores[chain.Key].PriorPoolIDs()
	if e != nil {
		return chain, e
	}
	for _, id := range previous {
		if _, ok := known[id]; ok {
			wanted[id] = true
		}
	}
	for _, id := range requested {
		if _, ok := known[id]; !ok {
			return chain, bootstrap.ErrUnknownPool
		}
		wanted[id] = true
	}
	if len(wanted) == 0 {
		for _, p := range chain.Pools {
			wanted[p.ID] = true
			if len(wanted) == 32 {
				break
			}
		}
	}
	selected := chain
	selected.Pools = make([]config.PoolConfig, 0, len(wanted))
	for _, p := range chain.Pools {
		if wanted[p.ID] {
			selected.Pools = append(selected.Pools, p)
		}
	}
	return selected, nil
}
func (s *Service) adapterForCurrentScope(chain config.Chain, current domain.Snapshot) *evm.Adapter {
	base := s.adapters[chain.Key]
	if base == nil {
		return nil
	}
	wanted := map[string]bool{}
	for _, p := range current.Pools {
		wanted[p.PoolID] = true
	}
	selected := chain
	selected.Pools = nil
	for _, p := range chain.Pools {
		if wanted[p.ID] {
			selected.Pools = append(selected.Pools, p)
		}
	}
	return base.CopyWithChain(selected)
}
