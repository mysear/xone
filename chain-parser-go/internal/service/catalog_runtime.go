package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/cockroachdb/pebble"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

// CatalogFence must come from the durable chain owner, never a request body.
// The chain actor must compare it again when incorporating Configs into a scope
// prepared/journal transaction. This separate DB is not a journal commit.
type CatalogFence struct{ Assignment, WriterEpoch, CanonicalEpoch, Generation uint64 }

func catalogFenceFollows(a, b CatalogFence) bool {
	return a.Assignment > 0 && a.WriterEpoch > 0 && a.CanonicalEpoch > 0 && a.Generation > 0 && a.Assignment >= b.Assignment && a.WriterEpoch >= b.WriterEpoch && a.CanonicalEpoch >= b.CanonicalEpoch && a.Generation >= b.Generation
}

type RuntimeCatalogEntry struct {
	TemplateBinding *V3TemplateBinding `json:"template_binding,omitempty"`
	Admission       ingest.PoolAdmission
	Config          config.PoolConfig
	Pool            domain.Pool
}
type RuntimeCatalogSnapshot struct {
	Namespace ingest.CatalogNamespace
	Fence     CatalogFence
	Anchor    ingest.CatalogAnchor
	Revision  uint64
	Ready     bool
	LastError string
	Coverage  string // bounded creation events, not an exhaustive market directory
	Entries   []RuntimeCatalogEntry
}
type CatalogRuntimeOptions struct {
	Path      string
	Chain     config.Chain
	Config    config.DynamicCatalogConfig
	Client    *rpc.Client
	Providers *ProviderRuntime
	Archive   *ingest.Archive
	// When nil, the built-in HTTP receipt-root verifier is used. An override
	// for another chain must bind the exact receipt/log set to archived HTTP bytes and
	// the chain's receipt integrity rule. A caller's event assertion is insufficient.
	VerifyBlock func(context.Context, ingest.FactoryBlock) error
	// CheckFence checks current durable owner/session state. It must not reenter
	// this catalog. Final scope publication remains under the chain actor's fence.
	CheckFence func(context.Context, CatalogFence) error
}
type catalogSavedBlock struct {
	Namespace ingest.CatalogNamespace
	Block     ingest.FactoryBlock
}
type catalogDisk struct {
	RebuildRequired bool
	Version         int
	Chain           string
	Fence           CatalogFence
	Revision        uint64
	Blocks          []catalogSavedBlock
	Revoked         []string
	Online, History []RuntimeCatalogEntry
}
type catalogEnvelope struct {
	Digest string
	Data   json.RawMessage
}

var ErrCatalogRebuildRequired = errors.New("CATALOG_REBUILD_REQUIRED")

var catalogKey = []byte("runtime-catalog/v1")

// CatalogRuntime owns a bounded Pebble hint directory. Restart never publishes
// saved numeric state: Begin replays archived proofs and Revalidate rereads H.
// Mutations are nonqueued; Revoke cancels an in-flight actor immediately.
type CatalogRuntime struct {
	mu                      sync.Mutex
	busy                    atomic.Bool
	options                 CatalogRuntimeOptions
	db                      *pebble.DB
	disk                    catalogDisk
	revision                uint64
	active, closed, blocked bool
	anchor                  ingest.CatalogAnchor
	discovery               *ingest.FactoryDiscovery
	candidates              map[ingest.CatalogNamespace]map[string]ingest.FactoryCandidate
	staged                  map[string]RuntimeCatalogEntry
	client                  *rpc.Client
	cancel                  context.CancelFunc
	online, history         RuntimeCatalogSnapshot
}

func OpenCatalogRuntime(o CatalogRuntimeOptions) (*CatalogRuntime, error) {
	if err := o.Config.Validate(o.Chain.Key); err != nil {
		return nil, err
	}
	if o.Path == "" || o.Client == nil || o.Providers == nil || o.Archive == nil || o.CheckFence == nil {
		return nil, errors.New("CATALOG_RUNTIME_OPTIONS_INVALID")
	}
	if o.Providers.client != o.Client || o.Providers.chain != o.Chain.Key || o.Providers.genesis != o.Chain.Genesis {
		return nil, errors.New("CATALOG_PROVIDER_BINDING_INVALID")
	}
	// Detach slices owned by configuration reload callers.
	o.Config.Sources = append([]config.FactoryDeployment(nil), o.Config.Sources...)
	for i := range o.Config.Sources {
		if profile := o.Config.Sources[i].PoolTemplate; profile != nil {
			copy := *profile
			o.Config.Sources[i].PoolTemplate = &copy
		}
	}

	absolute, err := filepath.Abs(o.Path)
	if err != nil {
		return nil, err
	}
	o.Path = absolute
	db, err := pebble.Open(o.Path, &pebble.Options{})
	if err != nil {
		return nil, err
	}
	r := &CatalogRuntime{options: o, db: db, disk: catalogDisk{Version: 1, Chain: o.Chain.Key}}
	raw, closer, err := db.Get(catalogKey)
	if err == nil {
		if int64(len(raw)) > o.Config.MaxBytes {
			_ = closer.Close()
			_ = db.Close()
			return nil, ingest.ErrBudget
		}
		var e catalogEnvelope
		err = json.Unmarshal(raw, &e)
		if err == nil {
			sum := sha256.Sum256(e.Data)
			if hex.EncodeToString(sum[:]) != e.Digest {
				err = ingest.ErrCorrupt
			} else {
				err = json.Unmarshal(e.Data, &r.disk)
			}
		}
		_ = closer.Close()
	} else if errors.Is(err, pebble.ErrNotFound) {
		err = nil
	}
	if err == nil && (r.disk.Version != 1 || r.disk.Chain != o.Chain.Key || len(r.disk.Blocks) > o.Config.MaxCandidates || len(r.disk.Revoked) > o.Config.MaxCandidates || len(r.disk.Online)+len(r.disk.History) > o.Config.MaxPools) {
		err = ingest.ErrCorrupt
	}
	if err != nil {
		_ = db.Close()
		return nil, err
	}
	r.revision = r.disk.Revision
	r.blocked = r.disk.RebuildRequired
	if !r.blocked {
		for _, saved := range r.disk.Blocks {
			if saved.Namespace != ingest.CatalogOnline && saved.Namespace != ingest.CatalogHistory {
				_ = db.Close()
				return nil, ingest.ErrCorrupt
			}
			if err = o.Archive.PinRaw(saved.Block.Raw, r.pinReference(saved.Namespace, saved.Block.Anchor.Hash)); err != nil {
				_ = db.Close()
				return nil, err
			}
		}
	}
	r.emptySnapshotsLocked()
	return r, nil
}
func runtimeDigest(s string) bool {
	if len(s) != 66 || !strings.HasPrefix(s, "0x") {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil && strings.Trim(s[2:], "0") != ""
}
func (r *CatalogRuntime) emptySnapshotsLocked() {
	r.online = RuntimeCatalogSnapshot{Namespace: ingest.CatalogOnline, Fence: r.disk.Fence, Revision: r.revision, Coverage: "bounded-verified-creation-events"}
	r.history = RuntimeCatalogSnapshot{Namespace: ingest.CatalogHistory, Fence: r.disk.Fence, Revision: r.revision, Coverage: "bounded-verified-creation-events"}
	r.online.LastError = "CATALOG_REVALIDATION_REQUIRED"
	r.history.LastError = "CATALOG_REVALIDATION_REQUIRED"
}
func (r *CatalogRuntime) saveLocked() error {
	data, err := json.Marshal(r.disk)
	if err != nil {
		return err
	}
	sum := sha256.Sum256(data)
	raw, err := json.Marshal(catalogEnvelope{hex.EncodeToString(sum[:]), data})
	if err != nil {
		return err
	}
	if int64(len(raw)) > r.options.Config.MaxBytes {
		return ingest.ErrBudget
	}
	return r.db.Set(catalogKey, raw, pebble.Sync)
}
func (r *CatalogRuntime) revoked(hash string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.revokedLocked(hash)
}
func (r *CatalogRuntime) revokedLocked(hash string) bool {
	for _, h := range r.disk.Revoked {
		if strings.EqualFold(h, hash) {
			return true
		}
	}
	return false
}
func (r *CatalogRuntime) operation(ctx context.Context) (context.Context, func(), uint64, error) {
	if ctx.Err() != nil {
		return nil, nil, 0, ctx.Err()
	}
	if !r.busy.CompareAndSwap(false, true) {
		return nil, nil, 0, ingest.ErrFactoryBusy
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(r.options.Config.TimeoutMS)*time.Millisecond)
	r.mu.Lock()
	if r.closed || r.blocked {
		err := ingest.ErrClosed
		if r.blocked {
			err = ErrCatalogRebuildRequired
		}
		r.mu.Unlock()
		cancel()
		r.busy.Store(false)
		return nil, nil, 0, err
	}
	revision := r.revision
	r.cancel = cancel
	r.mu.Unlock()
	return ctx, func() { cancel(); r.mu.Lock(); r.cancel = nil; r.mu.Unlock(); r.busy.Store(false) }, revision, nil
}
func (r *CatalogRuntime) check(ctx context.Context, revision uint64) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	r.mu.Lock()
	valid := !r.closed && !r.blocked && r.active && revision == r.revision
	f := r.disk.Fence
	r.mu.Unlock()
	if !valid {
		return ingest.ErrFactoryStale
	}
	return r.options.CheckFence(ctx, f)
}

// Begin revokes any old visible catalog before checking the new anchor. Failure
// leaves no ready catalog. Existing saved blocks remain replayable hints only.
func (r *CatalogRuntime) Begin(ctx context.Context, anchor ingest.CatalogAnchor, f CatalogFence) error {
	ctx, done, _, err := r.operation(ctx)
	if err != nil {
		return err
	}
	defer done()
	r.mu.Lock()
	if !catalogFenceFollows(f, r.disk.Fence) || anchor.Generation != f.Generation || anchor.Chain != r.options.Chain.Key || !runtimeDigest(anchor.Hash) || r.revokedLocked(anchor.Hash) {
		r.mu.Unlock()
		return ingest.ErrFactoryStale
	}
	r.revision++
	revision := r.revision
	r.active = false
	r.disk.Fence = f
	r.disk.Revision = revision
	r.disk.Online = nil
	r.disk.History = nil
	r.emptySnapshotsLocked()
	err = r.saveLocked()
	if err != nil {
		r.blocked = true
		r.mu.Unlock()
		return err
	}
	blocks := append([]catalogSavedBlock(nil), r.disk.Blocks...)
	r.mu.Unlock()
	if err = r.options.CheckFence(ctx, f); err != nil {
		return err
	}
	clients := r.options.Providers.QualifiedClients(anchor)
	if len(clients) == 0 {
		return ErrProviderGate
	}
	client := clients[0]
	if err = r.verifyAnchor(ctx, client, anchor); err != nil {
		return err
	}
	candidates := map[ingest.CatalogNamespace]map[string]ingest.FactoryCandidate{ingest.CatalogOnline: {}, ingest.CatalogHistory: {}}
	staged := map[string]RuntimeCatalogEntry{}
	discovery, err := r.makeDiscovery(client, revision, staged)
	if err != nil {
		return err
	}
	sort.SliceStable(blocks, func(i, j int) bool {
		return blocks[i].Namespace == ingest.CatalogOnline && blocks[j].Namespace != ingest.CatalogOnline
	})
	historyCtx, historyCancel := context.WithTimeout(ctx, time.Duration(r.options.Config.TimeoutMS)*time.Millisecond/4)
	defer historyCancel()
	historyError := ""
	for _, saved := range blocks {
		if saved.Namespace != ingest.CatalogOnline && saved.Namespace != ingest.CatalogHistory {
			return ingest.ErrCorrupt
		}
		if r.revoked(saved.Block.Anchor.Hash) {
			continue
		}
		replayCtx := ctx
		if saved.Namespace == ingest.CatalogHistory {
			if historyError != "" {
				continue
			}
			replayCtx = historyCtx
		}
		found, e := discovery.Discover(replayCtx, saved.Block)
		if e != nil {
			if saved.Namespace == ingest.CatalogHistory && ctx.Err() == nil {
				historyError = e.Error()
				candidates[ingest.CatalogHistory] = map[string]ingest.FactoryCandidate{}
				continue
			}
			return e
		}
		for _, c := range found {
			candidates[saved.Namespace][c.ID] = c
		}
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if err = r.options.CheckFence(ctx, f); err != nil {
		return err
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if revision != r.revision || r.closed {
		return ingest.ErrFactoryStale
	}
	r.anchor = anchor
	r.client = client
	r.discovery = discovery
	r.candidates = candidates
	r.staged = staged
	r.history.LastError = historyError
	r.active = true
	return nil
}

// Discover is the only candidate ingress. No scope graph consumes this result.
func (r *CatalogRuntime) Discover(ctx context.Context, namespace ingest.CatalogNamespace, b ingest.FactoryBlock) ([]ingest.FactoryCandidate, error) {
	ctx, done, revision, err := r.operation(ctx)
	if err != nil {
		return nil, err
	}
	defer done()
	return r.discover(ctx, namespace, b, revision)
}
func (r *CatalogRuntime) discover(ctx context.Context, namespace ingest.CatalogNamespace, b ingest.FactoryBlock, revision uint64) ([]ingest.FactoryCandidate, error) {
	var err error
	if namespace != ingest.CatalogOnline && namespace != ingest.CatalogHistory {
		return nil, ingest.ErrFactoryCandidate
	}
	if err = r.check(ctx, revision); err != nil {
		return nil, err
	}
	if r.revoked(b.Anchor.Hash) {
		return nil, ingest.ErrFactoryStale
	}
	r.mu.Lock()
	d := r.discovery
	r.mu.Unlock()
	candidates, err := d.Discover(ctx, b)
	if err != nil {
		return nil, err
	}
	if len(candidates) == 0 {
		return candidates, nil
	}
	if err = r.check(ctx, revision); err != nil {
		return nil, err
	}
	// Serialize and decode to sever receipt/log/topic slice ownership.
	encoded, err := json.Marshal(b)
	if err != nil {
		return nil, err
	}
	if int64(len(encoded)) > r.options.Config.MaxBytes {
		return nil, ingest.ErrBudget
	}
	var detached ingest.FactoryBlock
	if err = json.Unmarshal(encoded, &detached); err != nil {
		return nil, err
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if revision != r.revision || !r.active {
		return nil, ingest.ErrFactoryStale
	}
	oldBlocks := r.disk.Blocks
	exists := false
	for _, saved := range oldBlocks {
		if saved.Namespace == namespace && saved.Block.Anchor == b.Anchor {
			exists = true
			break
		}
	}
	if !exists {
		if len(oldBlocks) >= r.options.Config.MaxCandidates {
			return nil, ingest.ErrBudget
		}
		if err = r.options.Archive.PinRaw(b.Raw, r.pinReference(namespace, b.Anchor.Hash)); err != nil {
			return nil, err
		}
		r.disk.Blocks = append(append([]catalogSavedBlock(nil), oldBlocks...), catalogSavedBlock{namespace, detached})
	}
	if err = r.saveLocked(); err != nil {
		r.disk.Blocks = oldBlocks
		return nil, err
	}
	for _, c := range candidates {
		r.candidates[namespace][c.ID] = c
	}
	return candidates, nil
}
func (r *CatalogRuntime) Admit(ctx context.Context, namespace ingest.CatalogNamespace, candidates []ingest.FactoryCandidate) error {
	ctx, done, revision, err := r.operation(ctx)
	if err != nil {
		return err
	}
	defer done()
	if err = r.check(ctx, revision); err != nil {
		r.failAdmission(revision, "")
		return err
	}
	if len(candidates) > r.options.Config.MaxCandidates {
		return ingest.ErrBudget
	}
	r.mu.Lock()
	d := r.discovery
	anchor := r.anchor
	for _, c := range candidates {
		if got, ok := r.candidates[namespace][c.ID]; !ok || got != c {
			r.mu.Unlock()
			return ingest.ErrFactoryCandidate
		}
	}
	r.mu.Unlock()
	if len(candidates) == 0 {
		return ingest.ErrFactoryCandidate
	}
	err = d.Admit(ctx, namespace, anchor, candidates)
	if err != nil {
		r.failAdmission(revision, namespace)
		return err
	}
	err = r.check(ctx, revision)
	if err != nil {
		r.failAdmission(revision, "")
	}
	return err
}
func (r *CatalogRuntime) failAdmission(revision uint64, namespace ingest.CatalogNamespace) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if revision != r.revision || r.closed {
		return
	}
	if namespace == ingest.CatalogHistory {
		r.disk.History = nil
		r.history.Ready = false
		r.history.Entries = nil
		r.history.LastError = "CATALOG_HISTORY_ADMISSION_FAILED"
	} else {
		r.active = false
		r.disk.Online = nil
		r.disk.History = nil
		r.emptySnapshotsLocked()
		r.online.LastError = "CATALOG_ONLINE_ADMISSION_FAILED"
	}
	if r.saveLocked() != nil {
		r.blocked = true
	}
}

// Revalidate performs restart recovery: archived event proofs are checked in
// Begin, then every candidate is admitted against one current HTTP anchor.
func (r *CatalogRuntime) Revalidate(ctx context.Context, anchor ingest.CatalogAnchor, f CatalogFence) error {
	if err := r.Begin(ctx, anchor, f); err != nil {
		return err
	}
	for _, ns := range []ingest.CatalogNamespace{ingest.CatalogOnline, ingest.CatalogHistory} {
		r.mu.Lock()
		var candidates []ingest.FactoryCandidate
		for _, c := range r.candidates[ns] {
			candidates = append(candidates, c)
		}
		r.mu.Unlock()
		sort.Slice(candidates, func(i, j int) bool { return candidates[i].ID < candidates[j].ID })
		if len(candidates) > 0 {
			if err := r.Admit(ctx, ns, candidates); err != nil {
				if ns == ingest.CatalogHistory && ctx.Err() == nil {
					continue
				}
				return err
			}
		}
	}
	return nil
}

// Revoke may run while RPC is in flight. Epoch cancellation and revision checks
// prevent late publication. Tombstones survive restart and reject orphan replay.
func (r *CatalogRuntime) Revoke(ctx context.Context, hashes []string, f CatalogFence) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed {
		return ingest.ErrClosed
	}
	if !catalogFenceFollows(f, r.disk.Fence) {
		return ingest.ErrFactoryStale
	}
	for _, h := range hashes {
		if !runtimeDigest(h) {
			return ingest.ErrFactoryEvent
		}
	}
	r.revision++
	r.disk.Revision = r.revision
	r.disk.Fence = f
	r.active = false
	if r.cancel != nil {
		r.cancel()
	}
	r.disk.Online = nil
	r.disk.History = nil
	r.emptySnapshotsLocked()
	for _, h := range hashes {
		if !r.revokedLocked(h) {
			if len(r.disk.Revoked) >= r.options.Config.MaxCandidates {
				return errors.Join(ingest.ErrBudget, r.persistRebuildBarrierLocked())
			}
			r.disk.Revoked = append(r.disk.Revoked, strings.ToLower(h))
		}
	}
	blocks := make([]catalogSavedBlock, 0, len(r.disk.Blocks))
	var removed []catalogSavedBlock
	for _, b := range r.disk.Blocks {
		if !r.revokedLocked(b.Block.Anchor.Hash) {
			blocks = append(blocks, b)
		} else {
			removed = append(removed, b)
		}
	}
	r.disk.Blocks = blocks
	if err := r.saveLocked(); err != nil {
		return errors.Join(err, r.persistRebuildBarrierLocked())
	}
	for _, saved := range removed {
		if err := r.options.Archive.ReleaseRaw(saved.Block.Raw, r.pinReference(saved.Namespace, saved.Block.Anchor.Hash)); err != nil {
			return err
		}
	}
	return nil
}

// A compact durable barrier prevents restart from forgetting an overflowed
// revocation. Recovery requires rebuilding the hint directory from public data.
func (r *CatalogRuntime) persistRebuildBarrierLocked() error {
	r.blocked = true
	r.disk.RebuildRequired = true
	r.disk.Blocks = nil
	r.disk.Online = nil
	r.disk.History = nil
	r.disk.Revoked = nil
	return r.saveLocked()
}
func (r *CatalogRuntime) CatalogSnapshot(ns ingest.CatalogNamespace) RuntimeCatalogSnapshot {
	r.mu.Lock()
	defer r.mu.Unlock()
	s := r.online
	if ns == ingest.CatalogHistory {
		s = r.history
	} else if ns != ingest.CatalogOnline {
		return RuntimeCatalogSnapshot{}
	}
	if s.Ready && r.options.Providers.Gate(s.Anchor) != nil {
		s.Ready = false
		s.Entries = nil
		s.LastError = ErrProviderGate.Error()
	}
	if !r.active || r.blocked || r.closed {
		s.Ready = false
		s.Entries = nil
		if r.blocked {
			s.LastError = ErrCatalogRebuildRequired.Error()
		}
	}
	raw, _ := json.Marshal(s)
	var detached RuntimeCatalogSnapshot
	_ = json.Unmarshal(raw, &detached)
	return detached
}
func (r *CatalogRuntime) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed {
		return nil
	}
	r.closed = true
	r.active = false
	r.revision++
	if r.cancel != nil {
		r.cancel()
	}
	return r.db.Close()
}

func (r *CatalogRuntime) makeDiscovery(client *rpc.Client, revision uint64, staged map[string]RuntimeCatalogEntry) (*ingest.FactoryDiscovery, error) {
	cfg := r.options.Config
	sources := make([]ingest.FactorySource, 0, len(cfg.Sources))
	for _, s := range cfg.Sources {
		sources = append(sources, ingest.FactorySource{Chain: r.options.Chain.Key, Address: s.Address, Version: s.Version, ProvenanceID: s.ProvenanceID, Since: s.Since, Until: s.Until})
	}
	return ingest.NewFactoryDiscovery(ingest.FactoryOptions{FilterSources: true, Sources: sources, Archive: r.options.Archive, Limits: ingest.FactoryLimits{Timeout: time.Duration(cfg.TimeoutMS) * time.Millisecond, MaxReceipts: cfg.MaxReceipts, MaxLogs: cfg.MaxLogs, MaxBytes: cfg.MaxBytes, MaxCandidates: cfg.MaxCandidates, MaxPools: cfg.MaxPools}, VerifyBlock: func(ctx context.Context, b ingest.FactoryBlock) error {
		if r.options.VerifyBlock != nil {
			return r.options.VerifyBlock(ctx, b)
		}
		return r.verifyHTTPBlock(ctx, client, b)
	},
		VerifySource: func(ctx context.Context, s ingest.FactorySource, c ingest.Cursor) error {
			return r.verifyFactoryCode(ctx, client, s.Address, c.Hash)
		},
		VerifyCurrent: func(ctx context.Context, a ingest.CatalogAnchor) error {
			if err := r.check(ctx, revision); err != nil {
				return err
			}
			return r.verifyAnchor(ctx, client, a)
		},
		ReadCurrent: func(ctx context.Context, c ingest.FactoryCandidate, a ingest.CatalogAnchor) (ingest.PoolAdmission, error) {
			entry, err := r.readCurrent(ctx, client, c, a)
			if err == nil {
				staged[c.ID] = entry
			}
			return entry.Admission, err
		},
		Publish: func(ctx context.Context, b ingest.CatalogBatch) error { return r.publish(ctx, b, revision, staged) },
	})
}
func (r *CatalogRuntime) publish(ctx context.Context, b ingest.CatalogBatch, revision uint64, staged map[string]RuntimeCatalogEntry) error {
	if err := r.check(ctx, revision); err != nil {
		return err
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if revision != r.revision || !r.active || ctx.Err() != nil {
		return ingest.ErrFactoryStale
	}
	entries := append([]RuntimeCatalogEntry(nil), r.disk.Online...)
	if b.Namespace == ingest.CatalogHistory {
		entries = append([]RuntimeCatalogEntry(nil), r.disk.History...)
	}
	for _, admission := range b.Entries {
		entry, ok := staged[admission.Candidate.ID]
		if !ok || entry.Admission != admission || admission.At != r.anchor || r.revokedLocked(admission.Candidate.BlockHash) {
			return ingest.ErrFactoryAdmission
		}
		replaced := false
		for i, e := range entries {
			if e.Config.ID == entry.Config.ID {
				entries[i] = entry
				replaced = true
				break
			}
		}
		if !replaced {
			entries = append(entries, entry)
		}
	}
	total := len(entries) + len(r.disk.History)
	if b.Namespace == ingest.CatalogHistory {
		total = len(entries) + len(r.disk.Online)
	}
	if total > r.options.Config.MaxPools {
		return ingest.ErrBudget
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Config.ID < entries[j].Config.ID })
	oldOnline, oldHistory := r.disk.Online, r.disk.History
	if b.Namespace == ingest.CatalogOnline {
		r.disk.Online = entries
	} else if b.Namespace == ingest.CatalogHistory {
		r.disk.History = entries
	} else {
		return ingest.ErrFactoryAdmission
	}
	if err := r.saveLocked(); err != nil {
		r.disk.Online = oldOnline
		r.disk.History = oldHistory
		return err
	}
	snapshot := RuntimeCatalogSnapshot{Namespace: b.Namespace, Fence: r.disk.Fence, Anchor: b.Anchor, Revision: revision, Ready: true, Coverage: "bounded-verified-creation-events", Entries: entries}
	if b.Namespace == ingest.CatalogOnline {
		r.online = snapshot
	} else {
		r.history = snapshot
	}
	return nil
}

func (r *CatalogRuntime) deployment(address string) (config.FactoryDeployment, error) {
	for _, s := range r.options.Config.Sources {
		if s.Address == address {
			return s, nil
		}
	}
	return config.FactoryDeployment{}, ingest.ErrFactorySource
}
func (r *CatalogRuntime) verifyFactoryCode(ctx context.Context, c *rpc.Client, address, hash string) error {
	s, err := r.deployment(address)
	if err != nil {
		return err
	}
	var code string
	if err = c.Call(ctx, "eth_getCode", []any{address, map[string]any{"blockHash": hash, "requireCanonical": true}}, &code); err != nil {
		return err
	}
	return catalogCheckCode(code, s.FactoryCodeSHA256)
}
func catalogCheckCode(code, digest string) error {
	if !strings.HasPrefix(code, "0x") || len(code) > 2+2*24576 {
		return ingest.ErrFactorySource
	}
	raw, err := hex.DecodeString(code[2:])
	if err != nil || len(raw) == 0 {
		return ingest.ErrFactorySource
	}
	sum := sha256.Sum256(raw)
	if hex.EncodeToString(sum[:]) != digest {
		return ingest.ErrFactorySource
	}
	return nil
}

// verifyAnchor checks exact HTTP header identity and method-specific selector
// negative controls. Balance capability alone never authorizes these methods.
func (r *CatalogRuntime) verifyAnchor(ctx context.Context, c *rpc.Client, a ingest.CatalogAnchor) error {
	if err := r.options.Providers.Gate(a); err != nil {
		return err
	}
	var b struct{ Number, Hash, ParentHash string }
	if err := c.Call(ctx, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", a.Height), false}, &b); err != nil {
		return err
	}
	if b.Number != fmt.Sprintf("0x%x", a.Height) || !rpc.SameEVMHash(b.Hash, a.Hash) || !runtimeDigest(b.ParentHash) {
		return ingest.ErrFactoryStale
	}
	return r.methodControls(ctx, c, a)
}

func (r *CatalogRuntime) pinReference(ns ingest.CatalogNamespace, hash string) string {
	sum := sha256.Sum256([]byte(r.options.Path))
	return fmt.Sprintf("catalog:v1:%x:%s:%s:%s", sum[:12], r.options.Chain.Key, ns, strings.ToLower(hash))
}
