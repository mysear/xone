package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
)

type RawReplayRequest struct {
	DeadLetterID       string                 `json:"dead_letter_id"`
	IncidentID         string                 `json:"incident_id"`
	Namespace          ingest.ReplayNamespace `json:"namespace"`
	Attempt            uint64                 `json:"attempt"`
	ExpectedGeneration uint64                 `json:"expected_generation"`
	PendingAttemptID   string                 `json:"pending_attempt_id,omitempty"`
}
type rawReplayOutput struct {
	Snapshot domain.Snapshot
	Swaps    []domain.SwapFact
	Pins     []RawPin      `json:",omitempty"`
	Groups   []rawPinGroup `json:",omitempty"`
}

func replayVersions(pools []domain.Pool) string {
	seen := map[string]bool{}
	var versions []string
	for _, p := range pools {
		if !seen[p.AdapterVersion] {
			seen[p.AdapterVersion] = true
			versions = append(versions, p.AdapterVersion)
		}
	}
	sort.Strings(versions)
	h, _ := domain.Hash(versions)
	return h
}
func (m *RawMaintenance) Replay(ctx context.Context, key string, request RawReplayRequest) (ingest.ReplayRecord, error) {
	ctx, done, e := m.enter(ctx)
	if e != nil {
		return ingest.ReplayRecord{}, e
	}
	defer done()
	ctx, cancel := context.WithTimeout(ctx, time.Duration(m.cfg.ReplayTimeoutMS)*time.Millisecond)
	defer cancel()
	state := m.chains[key]
	if state == nil {
		return ingest.ReplayRecord{}, errors.New("RAW_CHAIN")
	}
	limits := ingest.ReplayLimits{MaxRawBytes: m.cfg.MaxRawBytes, MaxDecodedBytes: m.cfg.MaxDecodedBytes, Timeout: time.Duration(m.cfg.ReplayTimeoutMS) * time.Millisecond}
	confirm := func(c context.Context, i ingest.ReplayIntent) (ingest.CommitReceipt, error) {
		if c.Err() != nil {
			return ingest.CommitReceipt{}, c.Err()
		}
		return m.confirmOwner(key, i)
	}
	if request.PendingAttemptID != "" {
		if request.Namespace != "" || request.DeadLetterID != "" || request.IncidentID != "" || request.Attempt != 0 || request.ExpectedGeneration != 0 {
			return ingest.ReplayRecord{}, ingest.ErrOwnership
		}
		pending, e := state.recorder.archive.PendingReplays()
		if e != nil {
			return ingest.ReplayRecord{}, e
		}
		for _, r := range pending {
			if r.AttemptID == request.PendingAttemptID {
				cb := ingest.ReplayCallbacks{}
				if r.Spec.Namespace == ingest.NamespaceHistory {
					cb.ConfirmHistory = confirm
				} else {
					cb.ConfirmOnline = confirm
				}
				result, e := state.recorder.archive.Replay(ctx, r.Spec, limits, cb)
				if e == nil && result.Status == ingest.Resolved {
					e = m.retireReplay(ctx, key, result)
				}
				return result, e
			}
		}
		return ingest.ReplayRecord{}, errors.New("RAW_PENDING_ATTEMPT_NOT_FOUND")
	}
	if request.Namespace != ingest.NamespaceOnline && request.Namespace != ingest.NamespaceHistory {
		return ingest.ReplayRecord{}, ingest.ErrOwnership
	}
	letters, e := state.recorder.archive.DeadLetters()
	if e != nil {
		return ingest.ReplayRecord{}, e
	}
	var letter *ingest.DeadLetter
	for i := range letters {
		if ingest.DeadLetterID(letters[i]) == request.DeadLetterID {
			copy := letters[i]
			letter = &copy
			break
		}
	}
	if letter == nil {
		return ingest.ReplayRecord{}, ingest.ErrOwnership
	}
	if letter.Raw.Owner.Chain != key || letter.Raw.Owner.ProviderID != "derived-state-from-recorded-rpc" {
		return ingest.ReplayRecord{}, errors.New("RAW_REPLAY_REQUIRES_STAGED_STATE_NOT_HTTP")
	}
	incident, e := state.recorder.archive.CurrentIncident(request.DeadLetterID)
	if e != nil || incident.ID != request.IncidentID {
		return ingest.ReplayRecord{}, errors.Join(ingest.ErrFailureIncident, e)
	}
	if letter.Raw.Bytes > m.cfg.MaxRawBytes {
		return ingest.ReplayRecord{}, ingest.ErrBudget
	}
	raw, e := state.recorder.archive.Read(letter.Raw)
	if e != nil {
		return ingest.ReplayRecord{}, e
	}
	var original stagedState
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&original) != nil || original.Block.ChainKey != key || original.Block.Number != letter.Raw.Owner.Height || original.Block.Hash != letter.Raw.Owner.Hash {
		return ingest.ReplayRecord{}, errors.New("RAW_STAGED_IDENTITY")
	}
	if !m.s.syncMu.TryLock() {
		return ingest.ReplayRecord{}, ingest.ErrReplayBusy
	}
	store := m.s.stores[key]
	base, ready := store.Current()
	if !ready {
		m.s.syncMu.Unlock()
		return ingest.ReplayRecord{}, errors.New("RAW_OWNER_NOT_ACTIVE")
	}
	if request.ExpectedGeneration != base.Reference.OnlineGeneration {
		m.s.syncMu.Unlock()
		return ingest.ReplayRecord{}, ingest.ErrReplayFence
	}
	var chain config.Chain
	for _, ch := range m.s.cfg.Config.Chains {
		if ch.Key == key {
			chain = ch
			break
		}
	}
	// Freeze the current admitted catalog without invoking runtimeScopeAdapter:
	// that helper may verify providers over HTTP. Only local descriptor/capsule
	// selection belongs under this actor; all provider reads happen after unlock.
	activeAdapter := m.s.adapters[key]
	if activeAdapter == nil {
		m.s.syncMu.Unlock()
		return ingest.ReplayRecord{}, errors.New("RAW_NORMAL_ADAPTER_REQUIRED")
	}
	merged := m.s.runtimeCatalogChain(chain)
	admitted := map[string]config.PoolConfig{}
	for _, p := range merged.Pools {
		admitted[p.ID] = p
	}
	selected := chain
	selected.Pools = nil
	for _, p := range base.Pools {
		cfg, ok := admitted[p.PoolID]
		if !ok {
			m.s.syncMu.Unlock()
			return ingest.ReplayRecord{}, errors.New("RAW_SCOPE_NOT_ADMITTED")
		}
		selected.Pools = append(selected.Pools, cfg)
	}
	selection, e := m.s.freezeRuntimeScope(ctx, selected)
	if e != nil {
		m.s.syncMu.Unlock()
		return ingest.ReplayRecord{}, e
	}
	normal := activeAdapter.CopyWithChain(selected)
	// Select only configured/admitted identities from the original staged scope.
	configs := map[string]config.PoolConfig{}
	for _, p := range chain.Pools {
		configs[p.ID] = p
	}
	for _, p := range normal.Chain.Pools {
		configs[p.ID] = p
	}
	chain.Pools = nil
	for _, p := range original.Pools {
		cfg, ok := configs[p.PoolID]
		if !ok || cfg.Protocol != p.Protocol {
			m.s.syncMu.Unlock()
			return ingest.ReplayRecord{}, errors.New("RAW_SCOPE_NOT_ADMITTED")
		}
		chain.Pools = append(chain.Pools, cfg)
	}
	frozenRaw, _ := json.Marshal(chain)
	_ = json.Unmarshal(frozenRaw, &chain)
	source := &evm.Adapter{Client: normal.Client, Chain: chain}
	if request.Namespace == ingest.NamespaceOnline {
		source.OnComplete = normal.OnComplete
	}
	reference := base.Reference
	writer := m.s.sessions[key]
	if m.s.cfg.Config.Journal.Transport == "kafka" && writer == nil {
		m.s.syncMu.Unlock()
		return ingest.ReplayRecord{}, errors.New("RAW_ACTIVE_LEASE_REQUIRED")
	}
	spec := ingest.ReplaySpec{DeadLetterID: request.DeadLetterID, IncidentID: incident.ID, Raw: letter.Raw, DecoderVersion: m.cfg.DecoderVersion, Namespace: request.Namespace, WriterID: reference.OwnerService + "/" + strconv.FormatUint(reference.OwnerAssignmentVersion, 10), Fence: reference.WriterEpoch, Generation: strconv.FormatUint(reference.OnlineGeneration, 10), Attempt: request.Attempt}
	m.s.syncMu.Unlock()
	if request.Namespace == ingest.NamespaceOnline {
		if e := m.s.verifyRuntimeAnchor(ctx, chain, reference.Block); e != nil {
			return ingest.ReplayRecord{}, e
		}
	}
	oldScope, _ := domain.ScopeHash(original.Pools)
	oldVersions := replayVersions(original.Pools)
	m.mu.Lock()
	var provenance *stagedAcquisition
	for _, a := range state.acquisitions {
		if a.Raw == letter.Raw && a.Base.OnlineGeneration == reference.OnlineGeneration && a.Base.WriterEpoch == reference.WriterEpoch && a.Base.ScopeHash == reference.ScopeHash && a.Base.AdapterSetHash == reference.AdapterSetHash {
			copy := a
			provenance = &copy
			break
		}
	}
	m.mu.Unlock()
	if request.Namespace == ingest.NamespaceOnline && (provenance == nil || oldScope != reference.ScopeHash || oldVersions != reference.AdapterSetHash || original.Block.Number != reference.Block.Number+1) {
		return ingest.ReplayRecord{}, errors.New("RAW_ONLINE_REQUIRES_CURRENT_GENERATION_NEXT_CONTIGUOUS")
	}
	cb := ingest.ReplayCallbacks{Decode: func(c context.Context, s ingest.ReplaySpec, _ []byte, max int64) ([]byte, error) {
		_, capture := m.newCapture(key)
		c = context.WithValue(c, rawCaptureKey{}, capture)
		// All block receipts and all admitted state are read through the normal public
		// Adapter, never decoded from an isolated old HTTP response or caller payload.
		block, pools, swaps, e := source.ReadComplete(c, fmt.Sprintf("0x%x", s.Raw.Owner.Height))
		if e != nil {
			return nil, e
		}
		if block.Number != s.Raw.Owner.Height || block.Hash != s.Raw.Owner.Hash {
			return nil, errors.New("RAW_ANCHOR_CHANGED")
		}
		if e = source.ConfirmBlock(c, block); e != nil {
			return nil, e
		}
		scope, e := domain.ScopeHash(pools)
		if e != nil {
			return nil, e
		}
		if scope != oldScope || replayVersions(pools) != oldVersions {
			return nil, errors.New("RAW_ADAPTER_OR_SCOPE_VERSION_CHANGED")
		}
		next := base
		next.Reference.Block = block
		next.Reference.ScopeHash = scope
		next.Reference.AdapterSetHash = replayVersions(pools)
		next.Pools = pools
		if s.Namespace == ingest.NamespaceOnline {
			if block.ParentHash != reference.Block.Hash {
				return nil, errors.New("RAW_PARENT_CHANGED")
			}
			next.Reference.JournalSeq++
		} else {
			next.Source = "HISTORY_REPAIR_FULL_PUBLIC_ADAPTER"
		}
		if e = next.Validate(chain.Owner, chain.Assignment); e != nil {
			return nil, e
		}
		if s.Namespace == ingest.NamespaceOnline {
			if e = bootstrap.CheckFresh(next, uint64(time.Now().UnixMilli()), m.s.cfg.Config.Startup.MaxAge); e != nil {
				return nil, e
			}
		}
		capture.mu.Lock()
		pins := append([]RawPin(nil), capture.Pins...)
		capture.mu.Unlock()
		group, e := m.savePinGroup(c, key, s.ID(), s.Raw.Owner.Height, pins)
		if e != nil {
			return nil, e
		}
		groups := []rawPinGroup{group}
		if provenance != nil && s.Namespace == ingest.NamespaceOnline {
			groups = append(groups, acquisitionGroup(*provenance))
		}
		output, e := json.Marshal(rawReplayOutput{Snapshot: next, Swaps: swaps, Groups: groups})
		if e != nil || int64(len(output)) > max {
			return nil, ingest.ErrBudget
		}
		return output, nil
	}}
	commit := func(c context.Context, i ingest.ReplayIntent, output []byte) (ingest.CommitReceipt, error) {
		if rawDigest(output) != i.OutputDigest {
			return ingest.CommitReceipt{}, ingest.ErrCorrupt
		}
		var result rawReplayOutput
		if json.Unmarshal(output, &result) != nil {
			return ingest.CommitReceipt{}, ingest.ErrCorrupt
		}
		if !m.s.syncMu.TryLock() {
			return ingest.CommitReceipt{}, ingest.ErrReplayBusy
		}
		defer m.s.syncMu.Unlock()
		current, ok := store.Current()
		if !ok || current.Reference != reference || m.s.sessions[key] != writer {
			return ingest.CommitReceipt{}, ingest.ErrReplayFence
		}
		gate := func() error {
			if c.Err() != nil {
				return c.Err()
			}
			if i.Spec != spec || i.AttemptID != spec.ID() || i.Spec.Raw != letter.Raw || i.Spec.IncidentID != incident.ID || i.Spec.DecoderVersion != RawStagedDecoderVersion || i.Spec.Fence != reference.WriterEpoch || i.Spec.Generation != strconv.FormatUint(reference.OnlineGeneration, 10) || m.s.stores[key] != store || m.s.sessions[key] != writer || m.s.adapters[key] == nil || m.s.adapters[key].Client != source.Client || m.s.adapters[key].Chain.Owner != chain.Owner || m.s.adapters[key].Chain.Assignment != chain.Assignment || m.s.adapters[key].Chain.Genesis != chain.Genesis {
				return ingest.ErrReplayFence
			}
			if writer != nil {
				l := writer.lease
				if l.Chain != key || l.Holder != m.s.cfg.Config.Control.InstanceID || l.WriterEpoch != i.Spec.Fence || l.Generation != reference.OnlineGeneration || l.CanonicalEpoch != reference.CanonicalEpoch || uint64(time.Now().UnixMilli())+m.s.cfg.Config.Control.MinCommitMS >= l.ExpiresMS {
					return ingest.ErrReplayFence
				}
			}
			if i.Spec.Namespace == ingest.NamespaceOnline {
				return m.s.runtimeScopeGate(c, selection, result.Snapshot)()
			}
			return nil
		}
		if e := gate(); e != nil {
			return ingest.CommitReceipt{}, e
		}
		unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: result.Snapshot}
		if hasV2Scope(unit.Snapshot.Pools) {
			unit.NormalizedFacts = &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: unit.Snapshot.Reference.Block, Swaps: result.Swaps}}}
		}
		kind := "BLOCK_COMMIT"
		commitID, e := unit.CanonicalHash()
		if i.Spec.Namespace == ingest.NamespaceHistory {
			kind = "HISTORY_REPLAY_COMMIT"
			commitID, e = reducer.HistoryOwnerCommitID(result.Snapshot)
		}
		if e != nil {
			return ingest.CommitReceipt{}, e
		}
		proof := rawOwnerProof{Kind: kind, Intent: &i, CommitID: commitID, Reference: result.Snapshot.Reference, Pins: result.Pins, Groups: result.Groups, Raw: i.Spec.Raw, DecoderVersion: i.Spec.DecoderVersion}
		payload, e := json.Marshal(proof)
		if e != nil || len(payload) > 65536 {
			return ingest.CommitReceipt{}, ingest.ErrBudget
		}
		records := map[string]json.RawMessage{string(i.Spec.Namespace) + "-replay/" + i.AttemptID: payload}
		if i.Spec.Namespace == ingest.NamespaceHistory {
			e = store.CommitHistoryOwnerRecords(result.Snapshot, records, gate)
		} else if writer != nil {
			e = writer.publishOwnerRecords(c, unit, records, gate)
		} else {
			e = store.ApplyLocalOwnerRecords(unit, records, gate)
		}
		if e != nil {
			return ingest.CommitReceipt{}, e
		}
		if m.afterOwnerCommit != nil {
			if e = m.afterOwnerCommit(); e != nil {
				return ingest.CommitReceipt{}, e
			}
		}
		return m.confirmOwner(key, i)
	}
	if spec.Namespace == ingest.NamespaceHistory {
		cb.CommitHistory = commit
		cb.ConfirmHistory = confirm
	} else {
		cb.CommitOnline = commit
		cb.ConfirmOnline = confirm
	}
	result, e := state.recorder.archive.Replay(ctx, spec, limits, cb)
	if e == nil && result.Status == ingest.Resolved {
		e = m.retireReplay(ctx, key, result)
	}
	return result, e
}

// confirmOwner formats a receipt only AFTER validating an immutable record read
// from the same reducer transaction as state/journal/cursor. Missing records,
// still-prepared Kafka transactions and malformed identities remain unknown.
func (m *RawMaintenance) confirmOwner(key string, i ingest.ReplayIntent) (ingest.CommitReceipt, error) {
	if i.Spec.Raw.Owner.Chain != key || i.Spec.Raw.Owner.ProviderID != "derived-state-from-recorded-rpc" {
		return ingest.CommitReceipt{}, ingest.ErrOwnership
	}
	record, e := m.lookupOwnerRecord(key, string(i.Spec.Namespace)+"-replay/"+i.AttemptID)
	if e != nil {
		return ingest.CommitReceipt{}, errors.Join(ingest.ErrCommitUnknown, e)
	}
	var p rawOwnerProof
	d := json.NewDecoder(bytes.NewReader(record.Payload))
	d.DisallowUnknownFields()
	if d.Decode(&p) != nil || p.Intent == nil || *p.Intent != i || p.Raw != i.Spec.Raw || p.DecoderVersion != i.Spec.DecoderVersion || p.CommitID != record.CommitID || p.Reference != record.Reference || p.Reference.Block.ChainKey != key || p.Reference.Block.Hash != i.Spec.Raw.Owner.Hash || p.Reference.Block.Number != i.Spec.Raw.Owner.Height || p.Reference.WriterEpoch != i.Spec.Fence || strconv.FormatUint(p.Reference.OnlineGeneration, 10) != i.Spec.Generation || p.Reference.OwnerService+"/"+strconv.FormatUint(p.Reference.OwnerAssignmentVersion, 10) != i.Spec.WriterID {
		return ingest.CommitReceipt{}, errors.New("RAW_OWNER_RECEIPT_MISMATCH")
	}
	expected := "BLOCK_COMMIT"
	if i.Spec.Namespace == ingest.NamespaceHistory {
		expected = "HISTORY_REPLAY_COMMIT"
	}
	if p.Kind != expected || record.Kind != expected {
		return ingest.CommitReceipt{}, errors.New("RAW_OWNER_NAMESPACE_MISMATCH")
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(m.cfg.TimeoutMS)*time.Millisecond)
	defer cancel()
	if _, e = m.persistOwnerLedger(ctx, key, record); e != nil {
		return ingest.CommitReceipt{}, e
	}
	if e = m.releaseProofContext(ctx, key, p); e != nil {
		return ingest.CommitReceipt{}, e
	}
	return ingest.ReceiptFor(i, ingest.CommitConfirmed, "owner-record/"+record.Key+"/"+record.CommitID+"/"+rawDigest(record.Payload)), nil
}
