package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/cockroachdb/pebble"
	"io"
	"net/http"
	"net/url"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

// These helpers run under the existing chain actor (Start is before serving).
// Only ProviderRuntime.Run runs independently. Its goroutine never reads the
// service's session map. The final publication gate reads atomics/catalog state,
// never a reducer lock or RPC, so it can execute inside the owner Pebble batch.
type runtimeOwnerView struct {
	Ref        domain.SnapshotRef
	DeadlineMS uint64
	Valid      bool
}
type runtimeSourceState struct {
	owner     atomic.Pointer[runtimeOwnerView]
	closed    atomic.Bool
	busy      atomic.Bool
	mu        sync.Mutex
	next      time.Time
	lastFence CatalogFence
	lastError string
	checkedAt time.Time
}

func runtimeFence(ref domain.SnapshotRef) CatalogFence {
	return CatalogFence{Assignment: ref.OwnerAssignmentVersion, WriterEpoch: ref.WriterEpoch, CanonicalEpoch: ref.CanonicalEpoch, Generation: ref.OnlineGeneration}
}
func runtimeAnchor(ref domain.SnapshotRef) ingest.CatalogAnchor {
	return ingest.CatalogAnchor{Chain: ref.Block.ChainKey, Height: ref.Block.Number, Hash: ref.Block.Hash, Generation: ref.OnlineGeneration}
}
func (r *runtimeSourceState) check(ctx context.Context, f CatalogFence) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	v := r.owner.Load()
	if r.closed.Load() || v == nil || !v.Valid || runtimeFence(v.Ref) != f || v.DeadlineMS != 0 && uint64(time.Now().UnixMilli()) >= v.DeadlineMS {
		return ingest.ErrFactoryStale
	}
	return nil
}

// initRuntimeSources is called once after adapters/raw recorders/stores have
// been constructed. Repeated calls are idempotent. Configuration errors fail
// construction; catalog qualification errors after a baseline do not revoke it.
func (s *Service) initRuntimeSources() error {
	if s.source != nil {
		return nil
	}
	if s.providerRuntime == nil {
		s.providerRuntime = map[string]*ProviderRuntime{}
	}
	if s.catalogRuntime == nil {
		s.catalogRuntime = map[string]*CatalogRuntime{}
	}
	for _, ch := range s.cfg.Config.Chains {
		if err := ch.ProviderProbe.Validate(ch.Key); err != nil {
			return err
		}
		if err := ch.DynamicCatalog.Validate(ch.Key); err != nil {
			return err
		}
		if ch.ProviderProbe == nil {
			if ch.DynamicCatalog != nil {
				return errors.New("CATALOG_PROVIDER_REQUIRED")
			}
			continue
		}
		a := s.adapters[ch.Key]
		if a == nil {
			return errors.New("RUNTIME_NORMAL_ADAPTER_REQUIRED")
		}
		p := s.providerRuntime[ch.Key]
		if p == nil {
			var err error
			p, err = NewProviderRuntime(a.Client, ch.Key, ch.Genesis, *ch.ProviderProbe)
			if err != nil {
				return err
			}
			p.normal = &runtimeSourceState{}
			s.providerRuntime[ch.Key] = p
		}
		if p.normal == nil {
			p.normal = &runtimeSourceState{}
		}
		key := ch.Key
		p.SetObserver(func(ctx context.Context, purpose ProviderReadPurpose, operation func(context.Context) error) error {
			kind := RawObserveProbe
			if purpose == ProviderReadAnchor {
				kind = RawObserveAnchor
			}
			return s.rawMaintenance.Observe(ctx, key, kind, operation)
		})

		if _, installed := a.Client.HTTP.Transport.(*runtimeRPCTransport); !installed {
			upstream := a.Client.HTTP.Transport
			if upstream == nil {
				upstream = http.DefaultTransport
			}
			ids := map[string]string{}
			_, endpoints := a.Client.ProviderClients()
			for _, endpoint := range endpoints {
				parsed, err := url.Parse(endpoint.URLs[0])
				if err != nil {
					return err
				}
				ids[parsed.String()] = endpoint.Providers()[0].ID
			}
			cloned := *a.Client.HTTP
			cloned.Transport = &runtimeRPCTransport{upstream: upstream, provider: p, ids: ids}
			a.Client.HTTP = &cloned
		}

		if ch.DynamicCatalog == nil || s.catalogRuntime[ch.Key] != nil {
			continue
		}
		raw := s.rawSources[ch.Key]
		if raw == nil {
			return errors.New("CATALOG_RAW_ARCHIVE_REQUIRED")
		}
		state := p.normal
		catalog, err := OpenCatalogRuntime(CatalogRuntimeOptions{Path: filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), "runtime-catalog", strings.ReplaceAll(ch.Key, ":", "-")), Chain: ch, Config: *ch.DynamicCatalog, Client: a.Client, Providers: p, Archive: raw.archive, CheckFence: state.check, VerifyBlock: func(ctx context.Context, b ingest.FactoryBlock) error { return s.verifyRuntimeFactoryProof(ctx, ch, b) }})
		if err != nil {
			return err
		}
		s.catalogRuntime[ch.Key] = catalog
		chain := ch
		prior := a.OnComplete
		a.OnComplete = func(ctx context.Context, b evm.RawBlock, receipts []evm.Receipt) error {
			if prior != nil {
				if err := prior(ctx, b, receipts); err != nil {
					return err
				}
			}
			return s.captureRuntimeComplete(ctx, chain, b, receipts)
		}

	}
	return nil
}

// startRuntimeSources runs before the baseline loop. A historical probe error
// is recorded separately; verifyRuntimeAnchor gates the actual current source.
func (s *Service) startRuntimeSources(ctx context.Context) {
	if s.source != nil || s.providerCancel != nil {
		return
	}
	run, cancel := context.WithCancel(ctx)
	s.providerCancel = cancel
	for _, p := range s.providerRuntime {
		if p.normal != nil && p.normal.closed.Load() {
			continue
		}
		_ = p.Refresh(run)
		s.providerWG.Add(1)
		go func(p *ProviderRuntime) { defer s.providerWG.Done(); p.Run(run) }(p)
	}
}
func (s *Service) closeRuntimeSources() {
	for _, p := range s.providerRuntime {
		if p.normal != nil {
			p.normal.closed.Store(true)
			p.normal.owner.Store(&runtimeOwnerView{})
		}
	}
	if s.providerCancel != nil {
		s.providerCancel()
		s.providerWG.Wait()
	}
	for _, catalog := range s.catalogRuntime {
		_ = catalog.Close()
	}
	for _, p := range s.providerRuntime {
		p.client.HTTP.CloseIdleConnections()
	}
}

// captureRuntimeOwner must run outside reducer locks. CheckFence subsequently
// compares this immutable actor view without reentering the reducer. The owner
// prepared/journal transaction remains responsible for authoritative fencing.
func (s *Service) captureRuntimeOwner(ch config.Chain) *runtimeOwnerView {
	v := &runtimeOwnerView{}
	if store := s.stores[ch.Key]; store != nil {
		v.Ref = store.DurableRef()
		current, ready := store.Current()
		v.Valid = ready && current.Reference == v.Ref && v.Ref.OwnerService == ch.Owner && v.Ref.OwnerAssignmentVersion == ch.Assignment && !store.ReorgPending()
		if s.cfg.Config.Journal.Transport == "kafka" {
			w := s.sessions[ch.Key]
			if w == nil {
				v.Valid = false
			} else {
				v.DeadlineMS = w.lease.ExpiresMS
				if w.lease.WriterEpoch != v.Ref.WriterEpoch || w.lease.Generation != v.Ref.OnlineGeneration || w.lease.CanonicalEpoch != v.Ref.CanonicalEpoch {
					v.Valid = false
				}
			}
		}
	}
	if p := s.providerRuntime[ch.Key]; p != nil && p.normal != nil {
		p.normal.owner.Store(v)
	}
	return v
}

// verifyRuntimeAnchor is called after each actual HTTP state/header read and
// before publication. It does not perform historical probes. Unconfigured or
// injected sources retain their existing source admission rules.
func (s *Service) verifyRuntimeAnchor(ctx context.Context, ch config.Chain, block domain.BlockRef) error {
	p := s.providerRuntime[ch.Key]
	if p == nil {
		return ctx.Err()
	}
	if p.normal != nil && p.normal.closed.Load() {
		return ingest.ErrClosed
	}
	generation := s.stores[ch.Key].DurableRef().OnlineGeneration
	if generation == 0 {
		generation = 1
	}
	return p.VerifyAnchor(ctx, ingest.CatalogAnchor{Chain: ch.Key, Height: block.Number, Hash: block.Hash, Generation: generation})
}

// tickRuntimeSources runs once after a bounded normal sync window, including
// unchanged-head ticks. Discovery follows durable owner journal sequence, never
// a sampled head or a wall-clock interval. The helper's errors describe retained
// sidecar work; they do not revoke an already qualified static baseline.
func (s *Service) tickRuntimeSources(ctx context.Context, ch config.Chain) error {
	return s.rawMaintenance.Observe(ctx, ch.Key, RawObserveProbe, func(observed context.Context) error { return s.tickRuntimeSourcesObserved(observed, ch) })
}
func (s *Service) tickRuntimeSourcesObserved(ctx context.Context, ch config.Chain) error {
	p := s.providerRuntime[ch.Key]
	if p == nil || p.normal == nil {
		return ctx.Err()
	}
	state := p.normal
	if state.closed.Load() {
		return ingest.ErrClosed
	}
	owner := s.captureRuntimeOwner(ch)
	catalog := s.catalogRuntime[ch.Key]
	if !owner.Valid {
		if catalog != nil {
			return s.revokeRuntimeSources(ctx, ch, nil)
		}
		return ingest.ErrFactoryStale
	}
	if catalog == nil {
		return nil
	}
	if !state.busy.CompareAndSwap(false, true) {
		return ingest.ErrFactoryBusy
	}
	defer state.busy.Store(false)
	state.mu.Lock()
	due := !time.Now().Before(state.next)
	changed := state.lastFence != runtimeFence(owner.Ref)
	state.mu.Unlock()
	if !due && !changed {
		return nil
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(ch.DynamicCatalog.TimeoutMS)*time.Millisecond)
	defer cancel()
	finish := func(err error) error {
		state.mu.Lock()
		defer state.mu.Unlock()
		state.checkedAt = time.Now().UTC()
		state.lastError = ""
		state.lastFence = runtimeFence(owner.Ref)
		state.next = time.Time{}
		if err != nil {
			state.lastError = err.Error()
			delay := time.Duration(s.cfg.Config.Journal.PollMS) * time.Millisecond
			if delay <= 0 || delay > time.Second {
				delay = time.Second
			}
			state.next = time.Now().Add(delay)
		}
		return err
	}
	if err := s.initializeRuntimeDiscovery(ch, owner.Ref); err != nil {
		return finish(err)
	}
	catalog.mu.Lock()
	needsAnchor := !catalog.active || catalog.disk.Fence != runtimeFence(owner.Ref)
	catalog.mu.Unlock()
	if needsAnchor {
		if err := s.ensureRuntimeCatalogAnchor(ctx, ch, owner.Ref); err != nil {
			return finish(err)
		}
	}
	return finish(s.drainRuntimeDiscovery(ctx, ch, owner.Ref))
}

func (s *Service) ensureRuntimeCatalogAnchor(ctx context.Context, ch config.Chain, ref domain.SnapshotRef) error {
	catalog := s.catalogRuntime[ch.Key]
	a, f := runtimeAnchor(ref), runtimeFence(ref)
	catalog.mu.Lock()
	same := catalog.active && catalog.disk.Fence == f && catalog.anchor == a
	catalog.mu.Unlock()
	if same {
		return nil
	}
	if err := s.providerRuntime[ch.Key].VerifyAnchor(ctx, a); err != nil {
		return err
	}
	return catalog.Revalidate(ctx, a, f)
}

// revokeRuntimeSources is called before suspend, lease loss, or reorg handling.
// It invalidates publication capsules even if cancellation prevents disk work.
func (s *Service) revokeRuntimeSources(ctx context.Context, ch config.Chain, orphanHashes []string) error {
	p := s.providerRuntime[ch.Key]
	if p != nil && p.normal != nil {
		p.normal.owner.Store(&runtimeOwnerView{})
		p.normal.mu.Lock()
		p.normal.next = time.Time{}
		p.normal.lastFence = CatalogFence{}
		p.normal.lastError = "CATALOG_SCOPE_REVOKED"
		p.normal.mu.Unlock()
	}
	catalog := s.catalogRuntime[ch.Key]
	if catalog == nil {
		return nil
	}
	f := runtimeFence(s.stores[ch.Key].DurableRef())
	catalog.mu.Lock()
	floor := catalog.disk.Fence
	catalog.mu.Unlock()
	if f.Assignment < floor.Assignment {
		f.Assignment = floor.Assignment
	}
	if f.WriterEpoch < floor.WriterEpoch {
		f.WriterEpoch = floor.WriterEpoch
	}
	if f.CanonicalEpoch < floor.CanonicalEpoch {
		f.CanonicalEpoch = floor.CanonicalEpoch
	}
	if f.Generation < floor.Generation {
		f.Generation = floor.Generation
	}
	if !catalogFenceFollows(f, f) {
		return nil
	}
	if err := s.revokeRuntimeDiscovery(ch, orphanHashes); err != nil {
		return err
	}

	// Invalidate memory on a canceled caller too; persistence uses a separately
	// bounded cleanup context because cancellation must not leave online visible.
	if ctx.Err() != nil {
		cleanup, cancel := context.WithTimeout(context.Background(), time.Duration(ch.DynamicCatalog.TimeoutMS)*time.Millisecond)
		defer cancel()
		return catalog.Revoke(cleanup, orphanHashes, f)
	}
	return catalog.Revoke(ctx, orphanHashes, f)
}

// runtimeCatalogChain merges only admitted configs. Static IDs always retain
// their configured identity; candidate/history entries never enter this list.
func (s *Service) runtimeCatalogChain(ch config.Chain) config.Chain {
	out := ch
	out.Pools = append([]config.PoolConfig(nil), ch.Pools...)
	catalog := s.catalogRuntime[ch.Key]
	if catalog == nil {
		return out
	}
	snap := catalog.CatalogSnapshot(ingest.CatalogOnline)
	if !snap.Ready {
		return out
	}
	if runtimeFence(s.stores[ch.Key].DurableRef()) != snap.Fence {
		return out
	}
	known := map[string]bool{}
	for _, p := range out.Pools {
		known[p.ID] = true
	}
	for _, entry := range snap.Entries {
		if !known[entry.Config.ID] {
			known[entry.Config.ID] = true
			out.Pools = append(out.Pools, entry.Config)
		}
	}
	return out
}

type RuntimeScopeSelection struct {
	Chain      string
	Owner      string
	Assignment uint64
	Base       domain.SnapshotRef
	Frozen     FrozenCatalogScope
}

// freezeRuntimeScope receives the targetScopeChain result, before allocating
// its next generation. Static identities are read from immutable root config.
func (s *Service) freezeRuntimeScope(ctx context.Context, selected config.Chain) (*RuntimeScopeSelection, error) {
	static := map[string]config.PoolConfig{}
	found := false
	for _, ch := range s.cfg.Config.Chains {
		if ch.Key == selected.Key {
			found = true
			for _, p := range ch.Pools {
				static[p.ID] = p
			}
			break
		}
	}
	if !found {
		return nil, bootstrap.ErrUnknownPool
	}
	var ids []string
	seen := map[string]bool{}
	for _, p := range selected.Pools {
		if seen[p.ID] {
			return nil, ingest.ErrFactoryAdmission
		}
		seen[p.ID] = true
		if expected, ok := static[p.ID]; ok {
			a, _ := domain.Hash(p)
			b, _ := domain.Hash(expected)
			if a != b {
				return nil, ingest.ErrFactoryAdmission
			}
		} else {
			ids = append(ids, p.ID)
		}
	}
	if len(ids) == 0 {
		return nil, nil
	}
	owner := s.captureRuntimeOwner(selected)
	if !owner.Valid {
		return nil, ingest.ErrFactoryStale
	}
	catalog := s.catalogRuntime[selected.Key]
	if catalog == nil {
		return nil, bootstrap.ErrUnknownPool
	}
	frozen, err := catalog.Freeze(ctx, runtimeFence(owner.Ref), ids)
	if err != nil {
		return nil, err
	}
	configs := frozen.Configs()
	expected := map[string]string{}
	for _, p := range configs {
		expected[p.ID], _ = domain.Hash(p)
	}
	for _, p := range selected.Pools {
		if want, ok := expected[p.ID]; ok {
			actual, _ := domain.Hash(p)
			if actual != want {
				return nil, ingest.ErrFactoryAdmission
			}
		}
	}
	return &RuntimeScopeSelection{Chain: selected.Key, Owner: selected.Owner, Assignment: selected.Assignment, Base: owner.Ref, Frozen: frozen}, nil
}

// runtimeScopeGate permits the controlled old-generation -> prepared-next-
// generation transition. It validates actual target identities/state, while
// keeping the old catalog capsule valid until the owner's commit is published.
// It performs no Store access, so wrap it into every owner prepare/apply gate.
func (s *Service) runtimeScopeGate(ctx context.Context, selection *RuntimeScopeSelection, target domain.Snapshot) func() error {
	// Called by the chain actor before entering the owner lock. A successful
	// renewal/allocation may extend the deadline without changing the frozen
	// source generation. Consume that actor lease view, never reread Store here.
	if selection != nil {
		p, w := s.providerRuntime[selection.Chain], s.sessions[selection.Chain]
		ref := target.Reference
		if p != nil && p.normal != nil && w != nil && w.chain.Owner == selection.Owner && w.chain.Assignment == selection.Assignment && w.lease.Chain == selection.Chain && w.lease.WriterEpoch == ref.WriterEpoch && w.lease.Generation == ref.OnlineGeneration && w.lease.CanonicalEpoch == ref.CanonicalEpoch {
			view := p.normal.owner.Load()
			if view != nil && view.Valid && runtimeFence(view.Ref) == selection.Frozen.Fence && w.lease.ExpiresMS > view.DeadlineMS {
				copy := *view
				copy.DeadlineMS = w.lease.ExpiresMS
				p.normal.owner.Store(&copy)
			}
		}
	}
	// Header identity for the target is checked by the normal reader. Recheck
	// live capability freshness at the verified scope anchor, including static
	// selections. Capture the anchor before taking the reducer owner lock.
	capabilityAnchor := runtimeAnchor(target.Reference)
	if selection != nil {
		capabilityAnchor = selection.Frozen.Anchor
	} else if p := s.providerRuntime[target.Reference.Block.ChainKey]; p != nil && p.normal != nil {
		if view := p.normal.owner.Load(); view != nil && view.Valid && view.Ref.Block.Number <= target.Reference.Block.Number {
			capabilityAnchor = runtimeAnchor(view.Ref)
		}
	}
	return func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if p := s.providerRuntime[target.Reference.Block.ChainKey]; p != nil {
			if err := p.Gate(capabilityAnchor); err != nil {
				return err
			}
		}
		// Even a legacy cache clone without OnComplete cannot create unlimited
		// undiscovered online commits. This reads only catalogue metadata while
		// under the owner lock; a new baseline generation remains HEAD_FIRST.
		key := target.Reference.Block.ChainKey
		if p, c := s.providerRuntime[key], s.catalogRuntime[key]; p != nil && p.normal != nil && c != nil {
			if p.normal.closed.Load() {
				return ingest.ErrClosed
			}
			view := p.normal.owner.Load()
			if view != nil && view.Valid && target.Reference.OnlineGeneration == view.Ref.OnlineGeneration && target.Reference.Block.Number > view.Ref.Block.Number {
				var capacity uint64
				for _, chain := range s.cfg.Config.Chains {
					if chain.Key == key && chain.DynamicCatalog != nil {
						capacity = uint64(s.runtimeDiscoveryCapacity(chain))
						break
					}
				}
				c.mu.Lock()
				queue, err := loadRuntimeDiscoveryLocked(c)
				c.mu.Unlock()
				if err != nil {
					return err
				}
				if queue.Cursor == 0 {
					return ErrRuntimeDiscoveryGap
				}
				if capacity == 0 || target.Reference.JournalSeq > queue.Cursor && target.Reference.JournalSeq-queue.Cursor > capacity {
					return ingest.ErrBudget
				}
			}
		}
		if selection == nil {
			return nil
		}
		base, ref := selection.Base, target.Reference
		if ref.Block.ChainKey != selection.Chain || ref.OwnerService != selection.Owner || ref.OwnerAssignmentVersion != selection.Assignment || ref.CanonicalEpoch != base.CanonicalEpoch || ref.WriterEpoch < base.WriterEpoch || ref.OnlineGeneration < base.OnlineGeneration || ref.JournalSeq <= base.JournalSeq || ref.Block.Number < selection.Frozen.Anchor.Height || !runtimeDigest(ref.Block.Hash) {
			return ingest.ErrFactoryStale
		}
		catalog := s.catalogRuntime[selection.Chain]
		if catalog == nil {
			return ingest.ErrFactoryStale
		}
		if err := catalog.ValidateFrozen(ctx, selection.Frozen); err != nil {
			return err
		}
		pools := map[string]domain.Pool{}
		for _, p := range target.Pools {
			if _, exists := pools[p.PoolID]; exists {
				return ingest.ErrFactoryAdmission
			}
			pools[p.PoolID] = p
		}
		for _, entry := range selection.Frozen.Entries {
			p, ok := pools[entry.Config.ID]
			if !ok || p.ChainKey != selection.Chain || p.Protocol != entry.Config.Protocol || p.Token0 != entry.Config.Token0 || p.Token1 != entry.Config.Token1 || p.AdapterVersion != entry.Pool.AdapterVersion || p.FeeNumerator != entry.Pool.FeeNumerator || p.FeeDenominator != entry.Pool.FeeDenominator {
				return ingest.ErrFactoryAdmission
			}
			if err := p.Validate(); err != nil {
				return err
			}
			if v := entry.Config.V2; v != nil {
				if p.V2 == nil || p.V2.Factory != v.ExpectedFactory || p.V2.CodeSHA256 != v.ExpectedCodeSHA256 {
					return ingest.ErrFactoryAdmission
				}
			}
			if v := entry.Config.V3; v != nil {
				if p.V3 == nil || p.V3.Factory != v.ExpectedFactory || p.V3.CodeSHA256 != v.ExpectedCodeSHA256 || p.V3.FactoryCodeSHA256 != v.ExpectedFactoryCodeSHA256 || entry.Pool.V3 == nil || p.V3.TickSpacing != entry.Pool.V3.TickSpacing {
					return ingest.ErrFactoryAdmission
				}
			}
		}
		return nil
	}
}

// runtimeScopeAdapter replaces adapterForCurrentScope in the normal reader.
// Missing dynamic identities fail closed instead of silently shrinking scope.
func (s *Service) runtimeScopeAdapter(ctx context.Context, ch config.Chain, current domain.Snapshot) (*evm.Adapter, *RuntimeScopeSelection, error) {
	base := s.adapters[ch.Key]
	if base == nil {
		return nil, nil, errors.New("RUNTIME_NORMAL_ADAPTER_REQUIRED")
	}
	if err := s.verifyRuntimeAnchor(ctx, ch, current.Reference.Block); err != nil {
		return nil, nil, err
	}
	merged := s.runtimeCatalogChain(ch)
	known := map[string]config.PoolConfig{}
	for _, p := range merged.Pools {
		known[p.ID] = p
	}
	selected := ch
	selected.Pools = nil
	for _, p := range current.Pools {
		cfg, ok := known[p.PoolID]
		if !ok {
			return nil, nil, bootstrap.ErrUnknownPool
		}
		selected.Pools = append(selected.Pools, cfg)
	}
	frozen, err := s.freezeRuntimeScope(ctx, selected)
	if err != nil {
		return nil, nil, err
	}
	return base.CopyWithChain(selected), frozen, nil
}

type runtimeCatalogStatus struct {
	Enabled   bool                 `json:"enabled"`
	Ready     bool                 `json:"ready"`
	PoolCount int                  `json:"pool_count"`
	Fence     CatalogFence         `json:"fence"`
	Anchor    ingest.CatalogAnchor `json:"anchor"`
	Revision  uint64               `json:"revision"`
	Coverage  string               `json:"coverage"`
	LastError string               `json:"last_error,omitempty"`
}

// runtimeSourceStatus is registered on the existing authenticated source-evidence
// route. It has no network/maintenance side effects and emits no RPC URL/auth.
func (s *Service) runtimeSourceStatus(w http.ResponseWriter, r *http.Request) {
	key := r.PathValue("chain")
	if s.stores[key] == nil {
		http.Error(w, "UNKNOWN_CHAIN", 404)
		return
	}
	status := map[string]any{"enabled": s.rawSources[key] != nil, "source_kind": "raw-json-rpc-response", "consensus_verified": false, "r02_complete": false}
	for _, ch := range s.cfg.Config.Chains {
		if ch.Key == key {
			status["block_integrity_profile"] = ch.BlockIntegrity
			break
		}
	}
	if raw := s.rawSources[key]; raw != nil {
		status["limits"] = raw.archive.Limits()
	}
	if p := s.providerRuntime[key]; p != nil {
		status["providers"] = p.Snapshot()
		if p.normal != nil {
			p.normal.mu.Lock()
			status["catalog_next_check"] = p.normal.next
			status["catalog_checked_at"] = p.normal.checkedAt
			status["catalog_error"] = p.normal.lastError
			p.normal.mu.Unlock()
		}
	}
	summary := runtimeCatalogStatus{}
	if catalog := s.catalogRuntime[key]; catalog != nil {
		catalog.mu.Lock()
		snap := catalog.online
		summary = runtimeCatalogStatus{Enabled: true, Ready: catalog.active && !catalog.closed && !catalog.blocked && snap.Ready, PoolCount: len(snap.Entries), Fence: snap.Fence, Anchor: snap.Anchor, Revision: snap.Revision, Coverage: snap.Coverage, LastError: snap.LastError}
		catalog.mu.Unlock()
		if summary.Ready && s.providerRuntime[key].Gate(summary.Anchor) != nil {
			summary.Ready = false
			summary.LastError = ErrProviderGate.Error()
		}
		if p := s.providerRuntime[key]; summary.Ready && p != nil && p.normal != nil {
			if err := p.normal.check(r.Context(), summary.Fence); err != nil {
				summary.Ready = false
				summary.LastError = err.Error()
			}
		}
	}
	status["catalog"] = summary
	if summary.Enabled {
		if progress, err := s.runtimeDiscoveryProgress(key); err == nil {
			status["discovery"] = progress
		} else {
			status["discovery_error"] = err.Error()
		}
	}
	if summary.Enabled {
		status["factory_version_evidence"] = "configured-deployment-code-pins"
		verified, total := s.catalogRuntime[key].officialTemplatePoolCounts()
		status["official_v3_template_pool_count"] = verified
		status["public_factory_artifact_bound"] = total > 0 && verified == total
		if verified > 0 {
			status["factory_version_evidence"] = "official-v3-template-and-factory-source-pins"
		}
	}
	s.addNormalHistoryStatus(r.Context(), key, status)
	status["recovery"] = s.scopeRecoveryStatus(key)
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(status)
}

// Preserve every endpoint for header/receipt witnesses. Only state RPC traffic
// is filtered; capability probes explicitly bypass this filter to measure it.
type runtimeRPCTransport struct {
	upstream http.RoundTripper
	provider *ProviderRuntime
	ids      map[string]string
}

func (t *runtimeRPCTransport) RoundTrip(r *http.Request) (*http.Response, error) {
	if t.provider.normal != nil && t.provider.normal.closed.Load() {
		return nil, ingest.ErrClosed
	}
	if bypass, _ := r.Context().Value(runtimeProbeContextKey{}).(bool); bypass {
		return t.upstream.RoundTrip(r)
	}
	if r.GetBody == nil {
		return nil, ErrProviderGate
	}
	body, err := r.GetBody()
	if err != nil {
		return nil, err
	}
	defer body.Close()
	var request struct {
		Method string `json:"method"`
	}
	if err = json.NewDecoder(io.LimitReader(body, int64(t.provider.client.MaxBytes)+1)).Decode(&request); err != nil {
		return nil, err
	}
	switch request.Method {
	case "eth_call", "eth_getCode", "eth_getBalance", "eth_getStorageAt":
		allowed := false
		id := t.ids[r.URL.String()]
		for _, state := range t.provider.Snapshot().Providers {
			if state.ProviderID == id && providerQualified(state) && time.Since(state.CheckedAt) <= time.Duration(t.provider.cfg.MaxAgeMS)*time.Millisecond {
				allowed = request.Method != "eth_getStorageAt" || state.CurrentStoragePinned.Status == rpc.Supported
				break
			}
		}
		if !allowed {
			return nil, ErrProviderGate
		}
	}
	return t.upstream.RoundTrip(r)
}

var runtimeDiscoveryKey = []byte("normal-discovery/v1")
var ErrRuntimeDiscoveryPending = errors.New("RUNTIME_DISCOVERY_PENDING")
var ErrRuntimeDiscoveryGap = errors.New("RUNTIME_DISCOVERY_COVERAGE_GAP")

type runtimeObservedBlock struct {
	Height uint64
	Hash   string
	Source CatalogFence
	Raw    ingest.RawRef
	Pin    string
}
type runtimeRawRelease struct {
	Raw ingest.RawRef
	Pin string
}
type runtimeDiscoveryQueue struct {
	Version  int
	Chain    string
	Cursor   uint64
	Boundary domain.SnapshotRef
	Covered  domain.SnapshotRef
	Observed []runtimeObservedBlock
	Releases []runtimeRawRelease
	Orphaned uint64
}

func (s *Service) runtimeDiscoveryCapacity(ch config.Chain) int {
	n := uint64(ch.DynamicCatalog.MaxCandidates) + s.cfg.Config.Startup.MaxTail
	if n > 100000 {
		n = 100000
	}
	if n < 1 {
		n = 1
	}
	return int(n)
}
func loadRuntimeDiscoveryLocked(c *CatalogRuntime) (runtimeDiscoveryQueue, error) {
	q := runtimeDiscoveryQueue{Version: 1, Chain: c.options.Chain.Key}
	raw, close, err := c.db.Get(runtimeDiscoveryKey)
	if errors.Is(err, pebble.ErrNotFound) {
		return q, nil
	}
	if err != nil {
		return q, err
	}
	defer close.Close()
	if int64(len(raw)) > c.options.Config.MaxBytes {
		return q, ingest.ErrBudget
	}
	var envelope catalogEnvelope
	if err = json.Unmarshal(raw, &envelope); err != nil {
		return q, err
	}
	sum := sha256.Sum256(envelope.Data)
	if envelope.Digest != hex.EncodeToString(sum[:]) {
		return q, ingest.ErrCorrupt
	}
	if err = json.Unmarshal(envelope.Data, &q); err != nil {
		return q, err
	}
	if q.Version != 1 || q.Chain != c.options.Chain.Key || len(q.Observed) > 100000 || len(q.Releases) > 100000 {
		return q, ingest.ErrCorrupt
	}
	return q, nil
}
func saveRuntimeDiscoveryLocked(c *CatalogRuntime, q runtimeDiscoveryQueue) error {
	if c.closed {
		return ingest.ErrClosed
	}
	raw, err := runtimeDiscoveryEnvelope(q, c.options.Config.MaxBytes)
	if err != nil {
		return err
	}
	return c.db.Set(runtimeDiscoveryKey, raw, pebble.Sync)
}
func (s *Service) initializeRuntimeDiscovery(ch config.Chain, ref domain.SnapshotRef) error {
	c := s.catalogRuntime[ch.Key]
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return ingest.ErrClosed
	}
	q, err := loadRuntimeDiscoveryLocked(c)
	if err != nil {
		return err
	}
	if q.Cursor != 0 {
		return nil
	}
	q.Cursor = ref.JournalSeq
	q.Boundary = ref
	q.Covered = ref
	return saveRuntimeDiscoveryLocked(c, q)
}

// captureRuntimeComplete is installed only on the normal adapter. It records a
// small observation for every fully verified block and pins a full proof only
// for configured creation emitters/topics. Nothing here grants online scope.
func (s *Service) captureRuntimeComplete(ctx context.Context, ch config.Chain, b evm.RawBlock, receipts []evm.Receipt) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	c := s.catalogRuntime[ch.Key]
	if c == nil {
		return nil
	}
	height, err := strconv.ParseUint(trimRuntimeHex(b.Number), 16, 64)
	if err != nil || b.Number != fmt.Sprintf("0x%x", height) || !runtimeDigest(b.Hash) {
		return ingest.ErrFactoryEvent
	}
	owner := s.providerRuntime[ch.Key].normal.owner.Load()
	if owner == nil || !owner.Valid {
		return ingest.ErrFactoryStale
	}
	if err = s.recoverRuntimeDiscoveryIntent(ctx, ch); err != nil {
		return err
	}
	if err = s.reconcileRuntimeDiscovery(ctx, ch, owner.Ref); err != nil {
		return err
	}
	if err = s.flushRuntimeDiscoveryReleases(ctx, ch); err != nil {
		return err
	}
	matches := false
	for _, receipt := range receipts {
		for _, log := range receipt.Logs {
			if c.observesFactoryCreation(height, log.Address, log.Topics) {
				matches = true
			}
		}
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return ingest.ErrClosed
	}
	q, err := loadRuntimeDiscoveryLocked(c)
	if err != nil {
		return err
	}
	if intent, e := loadRuntimeDiscoveryIntentLocked(c); e != nil {
		return e
	} else if intent != nil {
		return ErrRuntimeDiscoveryPending
	}

	if q.Cursor == 0 {
		q.Cursor = owner.Ref.JournalSeq
		q.Boundary = owner.Ref
		q.Covered = owner.Ref
	}
	if height <= q.Covered.Block.Number && q.Covered.CanonicalEpoch == owner.Ref.CanonicalEpoch {
		return nil
	}
	for _, observed := range q.Observed {
		if observed.Height == height && observed.Hash == b.Hash {
			return nil
		}
	}
	if len(q.Observed)+len(q.Releases) >= s.runtimeDiscoveryCapacity(ch) {
		return ingest.ErrBudget
	}
	observation := runtimeObservedBlock{Height: height, Hash: b.Hash, Source: runtimeFence(owner.Ref)}
	if matches {
		proof := factoryHTTPProof{Header: b, Receipts: receipts}
		if _, err = c.projectHTTPProofContext(ctx, proof); err != nil {
			return err
		}
		raw, err := json.Marshal(proof)
		if err != nil {
			return err
		}
		if int64(len(raw)) > ch.DynamicCatalog.MaxBytes {
			return ingest.ErrBudget
		}
		observation.Pin = runtimeDiscoveryPin(c, observation.Source, b.Hash)
		observation.Raw, err = ingest.FactoryProofReference(ingest.Owner{Chain: ch.Key, Height: height, Hash: b.Hash, ProviderID: "verified-normal-complete"}, raw)
		if err != nil {
			return err
		}
		intent := runtimeDiscoveryIntent{Version: 1, Observation: observation, Proof: raw}
		if err = prepareRuntimeDiscoveryIntentLocked(c, q, intent); err != nil {
			return err
		}
		return finishRuntimeDiscoveryIntentLocked(ctx, c, q, intent)
	}
	q.Observed = append(q.Observed, observation)
	if err = saveRuntimeDiscoveryLocked(c, q); err != nil {
		return err
	}
	return ctx.Err()
}
func (s *Service) flushRuntimeDiscoveryReleases(ctx context.Context, ch config.Chain) error {
	c := s.catalogRuntime[ch.Key]
	for n := 0; n < 32; n++ {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		c.mu.Lock()
		q, err := loadRuntimeDiscoveryLocked(c)
		if err != nil {
			c.mu.Unlock()
			return err
		}
		if len(q.Releases) == 0 {
			c.mu.Unlock()
			return nil
		}
		release := q.Releases[0]
		c.mu.Unlock()
		if err = c.options.Archive.ReleaseRaw(release.Raw, release.Pin); err != nil {
			return err
		}
		c.mu.Lock()
		q, err = loadRuntimeDiscoveryLocked(c)
		if err == nil && len(q.Releases) > 0 && q.Releases[0] == release {
			q.Releases = q.Releases[1:]
			err = saveRuntimeDiscoveryLocked(c, q)
		}
		c.mu.Unlock()
		if err != nil {
			return err
		}
	}
	return ErrRuntimeDiscoveryPending
}
func (s *Service) drainRuntimeDiscovery(ctx context.Context, ch config.Chain, owner domain.SnapshotRef) error {
	c := s.catalogRuntime[ch.Key]
	if err := s.recoverRuntimeDiscoveryIntent(ctx, ch); err != nil {
		return err
	}
	if err := s.reconcileRuntimeDiscovery(ctx, ch, owner); err != nil {
		return err
	}
	if err := s.flushRuntimeDiscoveryReleases(ctx, ch); err != nil {
		return err
	}
	var canonicalView *runtimeCanonicalView
	for n := 0; n < 32; n++ {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		c.mu.Lock()
		q, err := loadRuntimeDiscoveryLocked(c)
		c.mu.Unlock()
		if err != nil {
			return err
		}
		if q.Cursor >= owner.JournalSeq {
			return s.flushRuntimeDiscoveryReleases(ctx, ch)
		}
		records, err := s.stores[ch.Key].Records(q.Cursor, 1)
		if err != nil {
			return err
		}
		if len(records) != 1 || records[0].Reference.JournalSeq != q.Cursor+1 {
			return ErrRuntimeDiscoveryGap
		}
		record := records[0]
		canonical := record.Kind == "BLOCK_COMMIT" && record.Reference.CanonicalEpoch == owner.CanonicalEpoch
		if record.Kind == "BLOCK_COMMIT" && !canonical {
			if canonicalView == nil {
				canonicalView, err = s.runtimeCanonicalView(ch)
				if err != nil {
					return err
				}
			}
			canonical, err = canonicalView.contains(ctx, record.Reference.Block.Number, record.Reference.Block.Hash)
			if err != nil {
				return err
			}
		}
		var observed *runtimeObservedBlock
		if record.Kind == "BLOCK_COMMIT" && canonical {
			for _, entry := range q.Observed {
				if entry.Height == record.Reference.Block.Number && entry.Hash == record.Reference.Block.Hash {
					copy := entry
					observed = &copy
					break
				}
			}
			if observed == nil {
				// A missing callback/cache observation is explicit work, never an empty
				// creation set. Re-read through the normal full verifier and its hook.
				chain := ch
				chain.Pools = nil
				adapter := s.adapters[ch.Key].CopyWithChain(chain)
				block, _, _, err := adapter.ReadComplete(ctx, fmt.Sprintf("0x%x", record.Reference.Block.Number))
				if err != nil {
					return err
				}
				if block != record.Reference.Block {
					return ingest.ErrFactoryStale
				}
				c.mu.Lock()
				q, err = loadRuntimeDiscoveryLocked(c)
				c.mu.Unlock()
				if err != nil {
					return err
				}
				for _, entry := range q.Observed {
					if entry.Height == block.Number && entry.Hash == block.Hash {
						copy := entry
						observed = &copy
						break
					}
				}
				if observed == nil {
					return ErrRuntimeDiscoveryGap
				}
			}
			if observed.Raw.ID != "" {
				if err = s.ensureRuntimeCatalogAnchor(ctx, ch, owner); err != nil {
					return err
				}
				raw, err := c.options.Archive.Read(observed.Raw)
				if err != nil {
					return err
				}
				if int64(len(raw)) > ch.DynamicCatalog.MaxBytes {
					return ingest.ErrBudget
				}
				var proof factoryHTTPProof
				if err = json.Unmarshal(raw, &proof); err != nil {
					return err
				}
				block, err := c.projectHTTPProofContext(ctx, proof)
				if err != nil {
					return err
				}
				block.Raw = observed.Raw
				candidates, err := c.Discover(ctx, ingest.CatalogOnline, block)
				if err != nil {
					return err
				}
				if len(candidates) > 0 {
					if err = c.Admit(ctx, ingest.CatalogOnline, candidates); err != nil {
						return err
					}
				}
			}
		}
		c.mu.Lock()
		q, err = loadRuntimeDiscoveryLocked(c)
		if err != nil {
			c.mu.Unlock()
			return err
		}
		if q.Cursor+1 != record.Reference.JournalSeq {
			c.mu.Unlock()
			return ingest.ErrFactoryStale
		}
		kept := make([]runtimeObservedBlock, 0, len(q.Observed))
		for _, entry := range q.Observed {
			remove := entry.Height == record.Reference.Block.Number && entry.Hash == record.Reference.Block.Hash
			if record.Kind == "ONLINE_BASELINE_COMMIT" && entry.Height <= record.Reference.Block.Number {
				remove = true
			}
			if remove {
				if entry.Raw.ID != "" {
					q.Releases = append(q.Releases, runtimeRawRelease{entry.Raw, entry.Pin})
				}
			} else {
				kept = append(kept, entry)
			}
		}
		q.Observed = kept
		q.Cursor = record.Reference.JournalSeq
		if record.Kind == "ONLINE_BASELINE_COMMIT" {
			q.Boundary = record.Reference
			q.Covered = record.Reference
		} else if record.Kind == "BLOCK_COMMIT" && canonical {
			q.Covered = record.Reference
		} else if record.Kind == "BLOCK_COMMIT" && !canonical {
			q.Orphaned++
		}
		err = saveRuntimeDiscoveryLocked(c, q)
		c.mu.Unlock()
		if err != nil {
			return err
		}
		if err = s.flushRuntimeDiscoveryReleases(ctx, ch); err != nil {
			return err
		}
	}
	c.mu.Lock()
	q, err := loadRuntimeDiscoveryLocked(c)
	c.mu.Unlock()
	if err != nil {
		return err
	}
	if q.Cursor < owner.JournalSeq {
		return ErrRuntimeDiscoveryPending
	}
	return nil
}
func (s *Service) revokeRuntimeDiscovery(ch config.Chain, hashes []string) error {
	if len(hashes) == 0 {
		return nil
	}
	c := s.catalogRuntime[ch.Key]
	bad := map[string]bool{}
	for _, h := range hashes {
		bad[strings.ToLower(h)] = true
	}
	c.mu.Lock()
	q, err := loadRuntimeDiscoveryLocked(c)
	if err != nil {
		c.mu.Unlock()
		return err
	}
	kept := make([]runtimeObservedBlock, 0, len(q.Observed))
	for _, entry := range q.Observed {
		if bad[strings.ToLower(entry.Hash)] {
			q.Orphaned++
			if entry.Raw.ID != "" {
				q.Releases = append(q.Releases, runtimeRawRelease{entry.Raw, entry.Pin})
			}
		} else {
			kept = append(kept, entry)
		}
	}
	q.Observed = kept
	err = saveRuntimeDiscoveryLocked(c, q)
	c.mu.Unlock()
	return err
}
func (s *Service) runtimeDiscoveryProgress(key string) (map[string]any, error) {
	c := s.catalogRuntime[key]
	c.mu.Lock()
	q, err := loadRuntimeDiscoveryLocked(c)
	c.mu.Unlock()
	if err != nil {
		return nil, err
	}
	ref := s.stores[key].DurableRef()
	pending := uint64(0)
	if ref.JournalSeq > q.Cursor {
		pending = ref.JournalSeq - q.Cursor
	}
	c.mu.Lock()
	intent, intentErr := loadRuntimeDiscoveryIntentLocked(c)
	c.mu.Unlock()
	if intentErr != nil {
		return nil, intentErr
	}
	intents := 0
	if intent != nil {
		intents = 1
	}
	return map[string]any{"pending_pin_intents": intents, "coverage_mode": "online-block-commits-after-baseline", "baseline_boundary_height": q.Boundary.Block.Number, "discovery_journal_seq": q.Cursor, "online_journal_seq": ref.JournalSeq, "covered_height": q.Covered.Block.Number, "pending_owner_records": pending, "queued_complete_blocks": len(q.Observed), "pending_pin_releases": len(q.Releases), "orphaned_blocks": q.Orphaned}, nil
}

func (t *runtimeRPCTransport) CloseIdleConnections() {
	if closer, ok := t.upstream.(interface{ CloseIdleConnections() }); ok {
		closer.CloseIdleConnections()
	}
}

// The outbox stores receipts from the normal verifier/cache. Revalidate that
// cached proof using exactly the selected chain encoding, then bind its header
// to the current canonical HTTP view. No receipt refetch or callback recursion.
func (s *Service) verifyRuntimeFactoryProof(ctx context.Context, ch config.Chain, b ingest.FactoryBlock) error {
	return s.rawMaintenance.Observe(ctx, ch.Key, RawObserveAnchor, func(observed context.Context) error { return s.verifyRuntimeFactoryProofObserved(observed, ch, b) })
}
func (s *Service) verifyRuntimeFactoryProofObserved(ctx context.Context, ch config.Chain, b ingest.FactoryBlock) error {
	c := s.catalogRuntime[ch.Key]
	if c == nil || b.Chain != ch.Key {
		return ingest.ErrFactorySource
	}
	raw, err := c.options.Archive.Read(b.Raw)
	if err != nil {
		return err
	}
	if int64(len(raw)) > c.options.Config.MaxBytes {
		return ingest.ErrBudget
	}
	var proof factoryHTTPProof
	if err = json.Unmarshal(raw, &proof); err != nil {
		return err
	}
	projected, err := c.projectHTTPProofContext(ctx, proof)
	if err != nil {
		return err
	}
	projected.Raw = b.Raw
	expected, err := domain.Hash(projected)
	if err != nil {
		return err
	}
	actual, err := domain.Hash(b)
	if err != nil {
		return err
	}
	if actual != expected {
		return ingest.ErrFactoryEvent
	}
	if err = c.verifyFactoryHTTPProof(ctx, proof); err != nil {
		return err
	}
	chain := ch
	chain.Pools = nil
	base := s.adapters[ch.Key]
	if base == nil {
		return errors.New("RUNTIME_NORMAL_ADAPTER_REQUIRED")
	}
	header, err := base.CopyWithChain(chain).Block(ctx, proof.Header.Number)
	if err != nil {
		return err
	}
	expected, err = domain.Hash(proof.Header)
	if err != nil {
		return err
	}
	actual, err = domain.Hash(header)
	if err != nil {
		return err
	}
	if actual != expected {
		return ingest.ErrFactoryStale
	}
	if ch.BlockIntegrity == evm.ChainNativeCommitmentsV2 {
		if proof.NativeEvidence == nil {
			return errors.New("FACTORY_NATIVE_EVIDENCE_REQUIRED")
		}
		return base.CopyWithChain(chain).RecheckNativeReceiptEvidence(ctx, header, *proof.NativeEvidence)
	}
	return ctx.Err()
}
