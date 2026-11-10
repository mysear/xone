package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"sync"
	"time"

	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
)

const RawBaselineDecoderVersion = "baseline-state/public-adapter-v1"
const RawBaselineStateVersion = "baseline-state-capture-v1"
const RawBaselineOwnerKind = "raw-baseline-owner-v1"
const (
	RawObserveIdentity      = "verify-identity"
	RawObserveProbe         = "provider-probe"
	RawObserveAnchor        = "verify-anchor"
	RawObserveHeader        = "header"
	RawObserveChainBaseline = "chain-baseline"
)

func observationPurpose(p string) bool {
	return p == RawObserveIdentity || p == RawObserveProbe || p == RawObserveAnchor || p == RawObserveHeader || p == RawObserveChainBaseline
}
func (m *RawMaintenance) beginResponsibleCapture(ctx context.Context, key, kind string) (context.Context, *rawCapture, func(), error) {
	if ctx.Err() != nil {
		return nil, nil, nil, ctx.Err()
	}
	m.mu.Lock()
	if m.closed || m.chains[key] == nil {
		m.mu.Unlock()
		return nil, nil, nil, errors.New("RAW_CAPTURE_CLOSED_OR_CHAIN")
	}
	child, cancel := context.WithCancel(ctx)
	g := &rawCapture{ID: rawGroup(), Chain: key, Max: m.cfg.MaxPinsPerCapture, Kind: kind}
	m.captureCancels[g] = cancel
	m.wg.Add(1)
	m.mu.Unlock()
	var once sync.Once
	finish := func() {
		once.Do(func() {
			g.mu.Lock()
			g.Sealed = true
			g.mu.Unlock()
			cancel()
			m.mu.Lock()
			delete(m.captureCancels, g)
			m.mu.Unlock()
			m.wg.Done()
		})
	}
	return context.WithValue(child, rawCaptureKey{}, g), g, finish, nil
}

// BeginBaselineCapture covers the complete normal baseline acquisition. Finish
// merely seals/cancels it, never releases inputs. Only the same-batch scope and
// owner-record proof can discharge these pins.
func (m *RawMaintenance) BeginBaselineCapture(ctx context.Context, key string) (context.Context, func(), error) {
	if m == nil {
		return ctx, func() {}, nil
	}
	if existing, _ := ctx.Value(rawCaptureKey{}).(*rawCapture); existing != nil {
		if existing.Chain != key || existing.Kind != "baseline" {
			return nil, nil, errors.New("RAW_BASELINE_RESPONSIBILITY_REQUIRED")
		}
		return ctx, func() {}, nil
	}
	child, _, finish, e := m.beginResponsibleCapture(ctx, key, "baseline")
	return child, finish, e
}

// Observe owns an independent read/decoder lifecycle, not a fact commit. A
// nested observation inherits its outer fact/history/baseline responsibility.
// An error, cancellation or missing completion creates no release authority.
func (m *RawMaintenance) Observe(ctx context.Context, key, purpose string, operation func(context.Context) error) error {
	if operation == nil || !observationPurpose(purpose) {
		return errors.New("RAW_OBSERVATION_PURPOSE")
	}
	if m == nil {
		return operation(ctx)
	}
	if g, _ := ctx.Value(rawCaptureKey{}).(*rawCapture); g != nil {
		if g.Chain != key {
			return ingest.ErrOwnership
		}
		return operation(ctx)
	}
	ctx, g, finish, e := m.beginResponsibleCapture(ctx, key, "observation")
	if e != nil {
		return e
	}
	defer finish()
	bounded, cancel := context.WithTimeout(ctx, m.operationTimeout())
	defer cancel()
	if e = operation(bounded); e != nil {
		return errors.Join(e, bounded.Err())
	}
	if bounded.Err() != nil {
		return bounded.Err()
	}
	g.mu.Lock()
	g.Sealed = true
	pins := append([]RawPin(nil), g.Pins...)
	g.mu.Unlock()
	if len(pins) == 0 {
		return nil
	}
	return m.completeObservation(bounded, key, purpose, g.ID, pins)
}
func cloneRawRecords(records map[string]json.RawMessage) map[string]json.RawMessage {
	out := make(map[string]json.RawMessage, len(records))
	for k, v := range records {
		out[k] = append(json.RawMessage(nil), v...)
	}
	return out
}
func fenceDigest(fence json.RawMessage) (string, error) {
	var v any
	decoder := json.NewDecoder(bytes.NewReader(fence))
	decoder.UseNumber()
	// Keep full-width owner epochs/generations; float64 would alias adjacent
	// uint64 fences above 2^53. Valid also rejects trailing JSON values.
	if !json.Valid(fence) || decoder.Decode(&v) != nil {
		return "", ingest.ErrCorrupt
	}
	canonical, e := json.Marshal(v)
	if e != nil {
		return "", e
	}
	return rawDigest(canonical), nil
}

// BaselineOwnerRecords must enter PrepareScopeRecordsCAS / the matching local
// InstallBaselineScopeRecordsCAS together with the scope publication. Reading a
// successful Snapshot or applying these records in a later transaction is not
// an acknowledgement. job/fence bind the actual durable ScopeCommit row.
func (m *RawMaintenance) BaselineOwnerRecords(ctx context.Context, unit journal.Unit, job string, fence json.RawMessage) (map[string]json.RawMessage, error) {
	if m == nil {
		return nil, nil
	}
	g, _ := ctx.Value(rawCaptureKey{}).(*rawCapture)
	if g == nil || g.Kind != "baseline" || g.Chain != unit.Snapshot.Reference.Block.ChainKey || unit.Kind != "ONLINE_BASELINE_COMMIT" {
		return nil, errors.New("RAW_BASELINE_CAPTURE_REQUIRED")
	}
	if len(unit.Snapshot.Pools) == 0 {
		// D0 is a completed chain observation, not a qualified pool Snapshot.
		// Keep domain/reducer readiness rules and issue only a weak Observe ticket.
		return nil, errors.New("RAW_D0_OBSERVATION_REQUIRED")
	}
	id, e := unit.CanonicalHash()
	if e != nil {
		return nil, e
	}
	if job == "" {
		return nil, errors.New("RAW_POOL_BASELINE_SCOPE_JOB_REQUIRED")
	}
	fd, e := fenceDigest(fence)
	if e != nil {
		return nil, e
	}
	g.mu.Lock()
	if len(g.OwnerRecords) > 0 {
		records := cloneRawRecords(g.OwnerRecords)
		matches := g.OwnerUnitID == id
		g.mu.Unlock()
		if !matches {
			return nil, ingest.ErrOwnership
		}
		for _, payload := range records {
			var p rawOwnerProof
			if json.Unmarshal(payload, &p) != nil || p.ScopeJob != job || p.ScopeFenceDigest != fd {
				return nil, ingest.ErrOwnership
			}
		}
		return records, nil
	}
	if g.Sealed || len(g.Pins) == 0 {
		g.mu.Unlock()
		return nil, errors.New("RAW_BASELINE_INPUTS_INCOMPLETE")
	}
	g.Sealed = true
	g.OwnerUnitID = id
	pins := append([]RawPin(nil), g.Pins...)
	g.mu.Unlock()
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	var owner string
	var assignment uint64
	for _, chain := range m.s.cfg.Config.Chains {
		if chain.Key == g.Chain {
			owner = chain.Owner
			assignment = chain.Assignment
			break
		}
	}
	if e = unit.Snapshot.Validate(owner, assignment); e != nil {
		return nil, e
	}
	snapshot, e := json.Marshal(unit.Snapshot)
	if e != nil || int64(len(snapshot)) > m.cfg.MaxRawBytes {
		return nil, ingest.ErrBudget
	}
	reference := "baseline-state-" + g.ID
	ref, e := m.putEvidencePinned(ctx, g.Chain, ingest.Owner{Chain: g.Chain, Height: unit.Snapshot.Reference.Block.Number, Hash: rawDigest(snapshot), ProviderID: RawBaselineStateVersion}, snapshot, reference)
	if e != nil {
		return nil, e
	}
	pins = append(pins, RawPin{Raw: ref, Reference: reference})
	root, e := m.savePinGroup(ctx, g.Chain, g.ID, unit.Snapshot.Reference.Block.Number, pins)
	if e != nil {
		return nil, e
	}
	p := rawOwnerProof{Kind: RawBaselineOwnerKind, CommitID: id, Reference: unit.Snapshot.Reference, Groups: []rawPinGroup{root}, Raw: ref, DecoderVersion: RawBaselineDecoderVersion, ScopeJob: job, ScopeFenceDigest: fd}
	payload, e := json.Marshal(p)
	if e != nil || len(payload) > 65536 {
		return nil, ingest.ErrBudget
	}
	records := map[string]json.RawMessage{"online-replay/" + g.ID: payload}
	g.mu.Lock()
	g.OwnerRecords = cloneRawRecords(records)
	g.mu.Unlock()
	return records, nil
}

// baselineStateMatches is a pure content check; callers use it before creating
// or trusting a baseline acknowledgement, in addition to the scope DB proof.
func baselineStateMatches(raw []byte, unit journal.Unit) bool {
	expected, e := json.Marshal(unit.Snapshot)
	return e == nil && bytes.Equal(raw, expected)
}

func (m *RawMaintenance) operationTimeout() time.Duration {
	return time.Duration(m.cfg.TimeoutMS) * time.Millisecond
}
