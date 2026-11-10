package service

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

// NewChainHistoryRuntime creates the normal public RPC runner. Call once per
// chain after opening the owner Store, passing the same Scheduler for all chains.
// No RPC occurs here. Start/Tick verification failures persist as retry later.
// It deliberately constructs rpc.New rather than using ProviderClients: those
// children share their parent's live recorder. Archive and DLQ are independent.
func (s *Service) NewChainHistoryRuntime(key string, scheduler *HistoryScheduler) (*HistoryRuntime, error) {
	return s.newChainHistoryRuntime(key, scheduler, nil)
}

// NewChainHistoryRuntimeForScope opens directly in an admitted/frozen dataset,
// avoiding a transient rebind to the static root config on runner reconstruction.
func (s *Service) NewChainHistoryRuntimeForScope(key string, scheduler *HistoryScheduler, scope config.Chain, expected domain.SnapshotRef) (*HistoryRuntime, error) {
	return s.newChainHistoryRuntime(key, scheduler, &historyScopeBinding{scope, expected})
}

type historyScopeBinding struct {
	Scope    config.Chain
	Expected domain.SnapshotRef
}

func (s *Service) newChainHistoryRuntime(key string, scheduler *HistoryScheduler, binding *historyScopeBinding) (*HistoryRuntime, error) {
	if scheduler == nil {
		return nil, errors.New("HISTORY_SHARED_SCHEDULER_REQUIRED")
	}
	store := s.stores[key]
	if store == nil {
		return nil, errors.New("HISTORY_UNKNOWN_CHAIN")
	}
	var chain config.Chain
	found := false
	for _, ch := range s.cfg.Config.Chains {
		if ch.Key == key {
			chain = ch
			found = true
			break
		}
	}
	if !found {
		return nil, errors.New("HISTORY_UNKNOWN_CHAIN")
	}
	chain, e := freezeHistoryScope(chain)
	if e != nil {
		return nil, e
	}
	var expectedOwner *domain.SnapshotRef
	if binding != nil {
		frozen, err := freezeHistoryScope(binding.Scope)
		if err != nil {
			return nil, err
		}
		if err = validateHistoryScope(frozen, chain); err != nil {
			return nil, err
		}
		chain = frozen
		expected := binding.Expected
		expectedOwner = &expected
	}
	datasetID, e := evm.HistoryDatasetID(chain)
	if e != nil {
		return nil, e
	}
	timeout := time.Duration(s.cfg.Config.Journal.TimeoutMS) * time.Millisecond
	if timeout <= 0 || timeout > 10*time.Second {
		timeout = 10 * time.Second
	}
	queue, e := store.OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: key, DatasetID: datasetID, ExpectedOwner: expectedOwner, ClaimTTL: timeout + 5*time.Second})
	if e != nil {
		return nil, e
	}
	opts := HistoryRuntimeOptions{Queue: queue, Config: s.cfg.Config.History, Scheduler: scheduler, Timeout: timeout}
	if !opts.Config.Enabled || opts.Config.MaxShare == 0 {
		runtime, e := NewHistoryRuntime(opts)
		if e != nil {
			return nil, e
		}
		runtime.updateScope = historyScopeUpdater(queue, nil, chain)
		return runtime, nil
	}
	if len(chain.RPCURLs) == 0 || len(chain.RPCURLs) > 8 {
		return nil, errors.New("HISTORY_SOURCE_COUNT_BUDGET")
	}
	maxBytes := s.cfg.Config.Journal.MaxBytes
	if maxBytes <= 0 || maxBytes > 4<<20 {
		maxBytes = 4 << 20
	}
	client := rpc.New(append([]string(nil), chain.RPCURLs...), timeout, maxBytes)
	limits := ingest.ArchiveLimits{MaxBytes: 64 << 20, MaxRecords: 4096, MaxRecordBytes: 16 << 20, ReservedBytes: 4 << 20, ReservedRecords: 64}
	if raw := s.cfg.Config.Journal.RawEvidence; raw != nil {
		if raw.MaxBytes < limits.MaxBytes {
			limits.MaxBytes = raw.MaxBytes
		}
		if raw.MaxRecords < limits.MaxRecords {
			limits.MaxRecords = raw.MaxRecords
		}
		if raw.MaxRecordBytes < limits.MaxRecordBytes {
			limits.MaxRecordBytes = raw.MaxRecordBytes
		}
		if limits.ReservedBytes >= limits.MaxBytes {
			limits.ReservedBytes = limits.MaxBytes / 8
		}
		if limits.ReservedRecords >= limits.MaxRecords {
			limits.ReservedRecords = limits.MaxRecords / 8
		}
	}
	archive, e := ingest.OpenArchive(filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), "history", "raw-evidence", strings.ReplaceAll(key, ":", "-")), limits)
	if e != nil {
		client.HTTP.CloseIdleConnections()
		return nil, e
	}
	source := &historyPublicSource{adapter: evm.Adapter{Client: client, Chain: chain}, recorder: newRawRecorder(key, archive), maxBytes: 4 * maxBytes}
	source.installNativeHistoryEvidence()
	client.SetRecorder(source.record)
	opts.ReadFacts = source.read
	opts.RawArchive = archive
	opts.MaxRawPins = 512
	if records := archive.Limits().MaxRecords / 4; records < opts.MaxRawPins {
		opts.MaxRawPins = records
	}
	if opts.MaxRawPins < 1 {
		opts.MaxRawPins = 1
	}
	runtime, e := NewHistoryRuntime(opts)
	if e != nil {
		archive.Close()
		client.HTTP.CloseIdleConnections()
		return nil, e
	}
	runtime.updateScope = historyScopeUpdater(queue, source, chain)
	runtime.afterCommit = func(group string) {
		source.mu.Lock()
		if !source.identityVerified {
			source.identityVerified = true
			source.identityReceipt = group
		}
		source.mu.Unlock()
	}
	runtime.closeSource = func() { client.HTTP.CloseIdleConnections(); _ = archive.Close() }
	return runtime, nil
}

// One source is entered by at most one scheduler worker. All successful response
// bodies, including VerifyEVM and witnesses, share an aggregate transfer budget
// of 4*MaxResponseBytes (<=16MiB). Per-response RPC buffers are <=4MiB; receipt
// collections are separately bounded by that same per-response cap, with primary
// and one witness live at once. JSON/Go object overhead and transient copy cost
// are bounded by these byte/item limits, not a promise of exact process RSS.
type historyClaimContextKey struct{}
type historyEvidenceContextKey struct{}
type historyStepEvidence struct{ Parents []string }
type historyPublicSource struct {
	identityReceipt     string
	mu                  sync.RWMutex
	adapter             evm.Adapter
	recorder            *rawRecorder
	maxBytes, remaining int
	identityVerified    bool
	seen                map[string]ingest.RawRef
}

func (h *historyPublicSource) record(ctx context.Context, provider, method string, params json.RawMessage, response []byte) error {
	cost := len(params) + len(response)
	if cost > h.remaining {
		return errors.New("HISTORY_SOURCE_BUFFER_BUDGET")
	}
	h.remaining -= cost
	group, _ := ctx.Value(rawCaptureKey{}).(*rawCapture)
	key := ""
	if group != nil {
		digest := sha256.New()
		for _, part := range [][]byte{[]byte(provider), []byte(method), params, response} {
			_ = binary.Write(digest, binary.BigEndian, uint64(len(part)))
			_, _ = digest.Write(part)
		}
		key = hex.EncodeToString(digest.Sum(nil))
		// The earlier exact response is already durable and held by this group.
		// Only raw content/pin ownership is shared; the RPC/re-anchor still executes.
		if ref, ok := h.seen[key]; ok {
			h.recorder.mu.Lock()
			h.recorder.recent = &ref
			h.recorder.mu.Unlock()
			return ctx.Err()
		}
	}
	if e := h.recorder.capture(ctx, provider, method, params, response); e != nil {
		return e
	}
	if group != nil {
		if h.seen == nil {
			h.seen = map[string]ingest.RawRef{}
		}
		if ref, ok := h.recorder.last(); ok {
			h.seen[key] = ref
		}
	}
	return nil
}
func (h *historyPublicSource) read(ctx context.Context, height uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
	h.remaining = h.maxBytes
	h.seen = map[string]ingest.RawRef{}
	h.recorder.mu.Lock()
	h.recorder.recent = nil
	h.recorder.mu.Unlock()
	h.mu.RLock()
	adapter := h.adapter
	verified := h.identityVerified
	identityReceipt := h.identityReceipt
	h.mu.RUnlock()
	var batch domain.FactBatch
	if claim, ok := ctx.Value(historyClaimContextKey{}).(reducer.HistoryClaim); ok {
		id, e := evm.HistoryDatasetID(adapter.Chain)
		if e != nil || id != claim.DatasetID {
			return batch, nil, reducer.ErrHistoryDataset
		}
	}
	// Verify every fixed endpoint before this independent source is first used.
	// RPC URLs/chain/genesis cannot change through UpdateScope; rebuilding creates
	// a fresh client and repeats verification. Quarantined endpoints stay fenced.
	var err error
	if !verified {
		err = adapter.Client.VerifyEVM(ctx, adapter.Chain.Key, adapter.Chain.Genesis)
	} else if evidence, ok := ctx.Value(historyEvidenceContextKey{}).(*historyStepEvidence); ok && identityReceipt != "" {
		evidence.Parents = []string{identityReceipt}
	}
	var candidates []ingest.FactoryCandidate
	if err == nil {
		batch, candidates, err = adapter.ReadFactsAndCatalog(ctx, fmt.Sprintf("0x%x", height))
	}
	if err != nil {
		// Persist only a fixed, non-sensitive code; recorder/DLQ never touch online.
		if e := h.recorder.quarantine("HISTORY_SOURCE_RETRY", "history"); e != nil {
			err = errors.Join(err, e)
		}
		return domain.FactBatch{}, nil, err
	}
	hints := make([]reducer.HistoryCandidate, 0, len(candidates))
	for _, c := range candidates {
		hints = append(hints, reducer.HistoryCandidate{PoolID: c.Pool, Protocol: c.Version, Token0: c.Token0, Token1: c.Token1, Factory: c.Factory})
	}
	return batch, hints, nil
}

// AddHistorySourceStatus augments the normal authenticated status response.
// The caller owns the map; it must never derive online readiness from history.
func AddHistorySourceStatus(ctx context.Context, status map[string]any, history *HistoryRuntime) {
	if history == nil {
		status["history"] = map[string]any{"enabled": false}
		return
	}
	st, e := history.SourceStatus(ctx)
	status["history"] = st
	if e != nil {
		status["history_status_error"] = "HISTORY_STATUS_UNAVAILABLE"
	}
}

func freezeHistoryScope(chain config.Chain) (config.Chain, error) {
	raw, e := json.Marshal(chain)
	if e != nil || len(raw) > 4<<20 {
		return config.Chain{}, errors.New("HISTORY_SCOPE_BUDGET")
	}
	var frozen config.Chain
	e = json.Unmarshal(raw, &frozen)
	return frozen, e
}

func validateHistoryScope(scope, base config.Chain) error {
	if scope.Key != base.Key || scope.Owner != base.Owner || scope.Assignment != base.Assignment || scope.Genesis != base.Genesis || scope.BlockIntegrity != base.BlockIntegrity {
		return errors.New("HISTORY_SCOPE_IDENTITY")
	}
	a, _ := json.Marshal(scope.RPCURLs)
	b, _ := json.Marshal(base.RPCURLs)
	if string(a) != string(b) {
		return errors.New("HISTORY_SCOPE_TRANSPORT_CHANGED")
	}
	return nil
}
func historyScopeUpdater(queue *reducer.HistoryQueue, source *historyPublicSource, base config.Chain) func(context.Context, config.Chain, domain.SnapshotRef) error {
	return func(ctx context.Context, scope config.Chain, expected domain.SnapshotRef) error {
		frozen, e := freezeHistoryScope(scope)
		if e != nil {
			return e
		}
		if e = validateHistoryScope(frozen, base); e != nil {
			return e
		}
		id, e := evm.HistoryDatasetID(frozen)
		if e != nil {
			return e
		}
		if e = queue.BindDataset(ctx, id, expected); e != nil {
			return e
		}
		if source != nil {
			source.mu.Lock()
			source.adapter.Chain = frozen
			source.mu.Unlock()
		}
		return nil
	}
}
