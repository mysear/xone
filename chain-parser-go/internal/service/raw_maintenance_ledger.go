package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/cockroachdb/pebble"
	"strings"
	"time"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/reducer"
)

const rawOwnerLedgerVersion = "raw-owner-ledger-v1"

type rawPinGroup struct {
	Root      ingest.RawRef
	Reference string
	Kind      string
	Binding   string
	Count     int
}
type rawPinGroupData struct {
	Kind    string
	Binding string
	Pins    []RawPin
}
type rawOwnerLedger struct {
	Kind        string
	Record      reducer.OwnerRecord
	ScopeCommit *reducer.ScopeOwnerCommit `json:",omitempty"`
}
type rawOwnerLedgerEntry struct {
	Raw     ingest.RawRef
	Ledger  rawOwnerLedger
	Retired bool
}

func acquisitionGroup(a stagedAcquisition) rawPinGroup {
	return rawPinGroup{Root: a.Proof, Reference: "acquisition-" + a.Group, Kind: "staged-acquisition-v1", Binding: a.Group, Count: len(a.Pins) + 1}
}

// putEvidencePinned admits every maintenance-owned payload against the same
// byte limit used by readEvidence, including metadata and encoded HTTP bodies.
// Rejection precedes Archive.PutPinned, so a larger archive quota cannot admit
// durable evidence which this maintenance configuration cannot subsequently read.
func (m *RawMaintenance) putEvidencePinned(ctx context.Context, key string, owner ingest.Owner, data []byte, reference string) (ingest.RawRef, error) {
	if ctx.Err() != nil {
		return ingest.RawRef{}, ctx.Err()
	}
	if int64(len(data)) > m.cfg.MaxRawBytes {
		return ingest.RawRef{}, ingest.ErrBudget
	}
	state := m.chains[key]
	if state == nil || owner.Chain != key {
		return ingest.RawRef{}, ingest.ErrOwnership
	}
	return state.recorder.archive.PutPinned(owner, data, reference)
}

func (m *RawMaintenance) readEvidence(ctx context.Context, key string, ref ingest.RawRef) ([]byte, error) {
	if ref.Owner.Chain != key || ref.Bytes > m.cfg.MaxRawBytes {
		return nil, ingest.ErrBudget
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	state := m.chains[key]
	data, e := state.recorder.archive.Read(ref)
	if errors.Is(e, ingest.ErrRawReclaimed) {
		manifest, err := state.recorder.archive.ArchivedManifest(ref)
		if err != nil {
			return nil, err
		}
		return state.sink.Read(ctx, manifest, m.cfg.MaxRawBytes)
	}
	return data, e
}
func (m *RawMaintenance) savePinGroup(ctx context.Context, key, binding string, height uint64, pins []RawPin) (rawPinGroup, error) {
	if len(pins) > m.cfg.MaxPinsPerCapture+1 {
		return rawPinGroup{}, ingest.ErrBudget
	}
	payload, e := json.Marshal(rawPinGroupData{Kind: "raw-pin-group-v1", Binding: binding, Pins: pins})
	if e != nil || int64(len(payload)) > m.cfg.MaxRawBytes {
		return rawPinGroup{}, ingest.ErrBudget
	}
	if ctx.Err() != nil {
		return rawPinGroup{}, ctx.Err()
	}
	reference := "pin-group-" + binding
	ref, e := m.putEvidencePinned(ctx, key, ingest.Owner{Chain: key, Height: height, Hash: rawDigest(payload), ProviderID: "raw-pin-group-v1"}, payload, reference)
	return rawPinGroup{Root: ref, Reference: reference, Kind: "raw-pin-group-v1", Binding: binding, Count: len(pins) + 1}, e
}
func (m *RawMaintenance) expandProofPins(ctx context.Context, key string, p rawOwnerProof) ([]RawPin, error) {
	// Legacy small owner records remain readable. New records use root references.
	pins := append([]RawPin(nil), p.Pins...)
	if len(pins) > 2*m.cfg.MaxPinsPerCapture+4 || len(p.Groups) > 4 {
		return nil, ingest.ErrBudget
	}
	for _, g := range p.Groups {
		if g.Count < 1 || g.Count > m.cfg.MaxPinsPerCapture+2 || g.Root.Owner.Chain != key {
			return nil, ingest.ErrOwnership
		}
		data, e := m.readEvidence(ctx, key, g.Root)
		if e != nil {
			return nil, e
		}
		var members []RawPin
		switch g.Kind {
		case "staged-acquisition-v1":
			var a stagedAcquisition
			if json.Unmarshal(data, &a) != nil || a.Kind != g.Kind || a.Group != g.Binding || a.Raw != p.Raw || g.Reference != "acquisition-"+a.Group {
				return nil, ingest.ErrCorrupt
			}
			members = a.Pins
		case "raw-pin-group-v1":
			var group rawPinGroupData
			if json.Unmarshal(data, &group) != nil || group.Kind != g.Kind || group.Binding != g.Binding || g.Reference != "pin-group-"+g.Binding {
				return nil, ingest.ErrCorrupt
			}
			if p.Intent != nil && group.Binding != p.Intent.AttemptID {
				return nil, ingest.ErrOwnership
			}
			members = group.Pins
		default:
			return nil, ingest.ErrCorrupt
		}
		if len(members)+1 != g.Count {
			return nil, ingest.ErrCorrupt
		}
		pins = append(pins, members...)
		pins = append(pins, RawPin{g.Root, g.Reference})
	}
	for _, pin := range pins {
		if pin.Raw.Owner.Chain != key || pin.Reference == "" || len(pin.Reference) > 512 {
			return nil, ingest.ErrOwnership
		}
	}
	return pins, nil
}
func (m *RawMaintenance) lookupOwnerRecord(key, recordKey string) (reducer.OwnerRecord, error) {
	record, e := m.s.stores[key].OwnerRecord(recordKey)
	if e == nil {
		return record, nil
	}
	if !errors.Is(e, pebble.ErrNotFound) {
		return reducer.OwnerRecord{}, e
	}
	m.mu.Lock()
	entry, ok := m.chains[key].ledgers[recordKey]
	m.mu.Unlock()
	if !ok {
		return reducer.OwnerRecord{}, e
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(m.cfg.TimeoutMS)*time.Millisecond)
	defer cancel()
	data, e := m.readEvidence(ctx, key, entry.Raw)
	if e != nil {
		return reducer.OwnerRecord{}, e
	}
	var ledger rawOwnerLedger
	if json.Unmarshal(data, &ledger) != nil || ledger.Kind != rawOwnerLedgerVersion || ledger.Record.Key != recordKey || ledger.Record.CommitID != entry.Ledger.Record.CommitID {
		return reducer.OwnerRecord{}, ingest.ErrCorrupt
	}
	return ledger.Record, nil
}
func decodeOwnerProof(record reducer.OwnerRecord, key string) (rawOwnerProof, error) {
	var p rawOwnerProof
	d := json.NewDecoder(bytes.NewReader(record.Payload))
	d.DisallowUnknownFields()
	if d.Decode(&p) != nil || len(record.CommitID) != 64 || record.CommitID != p.CommitID || record.Reference != p.Reference || p.Raw.Owner.Chain != key {
		return p, ingest.ErrCorrupt
	}
	if p.Intent == nil {
		if !strings.HasPrefix(record.Key, "online-replay/") {
			return p, ingest.ErrOwnership
		}
		switch p.Kind {
		case "raw-capture-owner-v1":
			if record.Kind != "BLOCK_COMMIT" {
				return p, ingest.ErrOwnership
			}
		case RawReorgOwnerKind:
			if record.Kind != "REORG_COMMIT" || p.DecoderVersion != RawReorgDecoderVersion || p.Raw.Owner.ProviderID != RawOwnerUnitCaptureVersion {
				return p, ingest.ErrOwnership
			}
		case RawBaselineOwnerKind:
			if record.Kind != "ONLINE_BASELINE_COMMIT" || p.DecoderVersion != RawBaselineDecoderVersion || p.Raw.Owner.ProviderID != RawBaselineStateVersion || p.ScopeJob == "" || len(p.ScopeFenceDigest) != 64 {
				return p, ingest.ErrOwnership
			}
		default:
			return p, ingest.ErrOwnership
		}
	} else {
		i := *p.Intent
		if i.Spec.Namespace != ingest.NamespaceOnline && i.Spec.Namespace != ingest.NamespaceHistory {
			return p, ingest.ErrOwnership
		}
		if i.AttemptID != i.Spec.ID() || i.Spec.Raw.Owner.ProviderID != "derived-state-from-recorded-rpc" || record.Reference.Block.Hash != p.Raw.Owner.Hash || record.Reference.Block.Number != p.Raw.Owner.Height {
			return p, ingest.ErrOwnership
		}
		expected := "BLOCK_COMMIT"
		if i.Spec.Namespace == ingest.NamespaceHistory {
			expected = "HISTORY_REPLAY_COMMIT"
		}
		if p.Kind != expected || record.Kind != expected || record.Key != string(i.Spec.Namespace)+"-replay/"+i.AttemptID || p.Raw != i.Spec.Raw || p.DecoderVersion != i.Spec.DecoderVersion {
			return p, ingest.ErrOwnership
		}
	}
	return p, nil
}
func (m *RawMaintenance) persistOwnerLedger(ctx context.Context, key string, record reducer.OwnerRecord) (rawOwnerLedgerEntry, error) {
	if _, e := decodeOwnerProof(record, key); e != nil {
		return rawOwnerLedgerEntry{}, e
	}
	m.mu.Lock()
	old, exists := m.chains[key].ledgers[record.Key]
	m.mu.Unlock()
	if exists {
		a, _ := json.Marshal(old.Ledger.Record)
		b, _ := json.Marshal(record)
		if !bytes.Equal(a, b) {
			return old, errors.New("RAW_OWNER_LEDGER_CONFLICT")
		}
		return old, nil
	}
	p, e := decodeOwnerProof(record, key)
	if e != nil {
		return rawOwnerLedgerEntry{}, e
	}
	var scope *reducer.ScopeOwnerCommit
	if p.Kind == RawBaselineOwnerKind {
		scope, e = m.baselineScopeProof(key, p, record.Key)
		if e != nil {
			return rawOwnerLedgerEntry{}, e
		}
	}
	ledger := rawOwnerLedger{Kind: rawOwnerLedgerVersion, Record: record, ScopeCommit: scope}
	data, e := json.Marshal(ledger)
	if e != nil || int64(len(data)) > m.cfg.MaxRawBytes {
		return rawOwnerLedgerEntry{}, ingest.ErrBudget
	}
	if ctx.Err() != nil {
		return rawOwnerLedgerEntry{}, ctx.Err()
	}
	reference := "owner-ledger-" + rawDigest([]byte(record.Key))
	ref, e := m.putEvidencePinned(ctx, key, ingest.Owner{Chain: key, Height: record.Reference.Block.Number, Hash: rawDigest(data), ProviderID: rawOwnerLedgerVersion}, data, reference)
	if e != nil {
		return rawOwnerLedgerEntry{}, e
	}
	entry := rawOwnerLedgerEntry{Raw: ref, Ledger: ledger}
	m.mu.Lock()
	m.chains[key].ledgers[record.Key] = entry
	m.mu.Unlock()
	return entry, nil
}
func (m *RawMaintenance) resolvedAudit(key string, p rawOwnerProof) error {
	if p.Intent == nil {
		return nil
	}
	records, e := m.chains[key].recorder.archive.ReplayAudit(p.Intent.Spec.DeadLetterID)
	if e != nil {
		return e
	}
	for _, r := range records {
		if r.AttemptID == p.Intent.AttemptID && r.Intent == *p.Intent && r.Status == ingest.Resolved && r.Phase == "finished" {
			return nil
		}
	}
	return ingest.ErrCommitUnknown
}
func (m *RawMaintenance) finishRetirement(ctx context.Context, key string, entry rawOwnerLedgerEntry) error {
	if entry.Retired {
		return nil
	}
	p, e := decodeOwnerProof(entry.Ledger.Record, key)
	if e != nil {
		return e
	}
	if e = m.resolvedAudit(key, p); e != nil {
		return e
	}
	// Ledger handoff precedes releases; partial release/quota/cancellation keeps
	// the SQL proof. Replay unknown/active never passes the durable audit gate.
	if e = m.releaseProofContext(ctx, key, p); e != nil {
		return e
	}
	gate := func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		data, e := m.readEvidence(ctx, key, entry.Raw)
		if e != nil {
			return e
		}
		want, _ := json.Marshal(entry.Ledger)
		if !bytes.Equal(data, want) {
			return ingest.ErrCorrupt
		}
		return m.resolvedAudit(key, p)
	}
	record, e := m.s.stores[key].OwnerRecord(entry.Ledger.Record.Key)
	if e == nil {
		if record.CommitID != entry.Ledger.Record.CommitID {
			return ingest.ErrOwnership
		}
		if e = m.s.stores[key].RetireOwnerRecord(record.Key, record.CommitID, gate); e != nil {
			return e
		}
	} else if !errors.Is(e, pebble.ErrNotFound) {
		return e
	} else {
		if e = gate(); e != nil {
			return e
		}
	}
	entry.Retired = true
	m.mu.Lock()
	m.chains[key].ledgers[entry.Ledger.Record.Key] = entry
	m.mu.Unlock()
	return nil
}
func (m *RawMaintenance) releaseProofContext(ctx context.Context, key string, p rawOwnerProof) error {
	pins, e := m.expandProofPins(ctx, key, p)
	if e != nil {
		return e
	}
	for _, pin := range pins {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if e = m.chains[key].recorder.archive.ReleaseRaw(pin.Raw, pin.Reference); e != nil {
			return e
		}
	}
	return nil
}
func (m *RawMaintenance) retireReplay(ctx context.Context, key string, r ingest.ReplayRecord) error {
	if r.Status != ingest.Resolved {
		return nil
	}
	record, e := m.lookupOwnerRecord(key, string(r.Spec.Namespace)+"-replay/"+r.AttemptID)
	if e != nil {
		return e
	}
	entry, e := m.persistOwnerLedger(ctx, key, record)
	if e != nil {
		return e
	}
	return m.finishRetirement(ctx, key, entry)
}
func (m *RawMaintenance) ledgerProofForRaw(ctx context.Context, key string, ref ingest.RawRef) (string, bool, error) {
	m.mu.Lock()
	entries := make([]rawOwnerLedgerEntry, 0, len(m.chains[key].ledgers))
	for _, e := range m.chains[key].ledgers {
		entries = append(entries, e)
	}
	m.mu.Unlock()
	for _, entry := range entries {
		if ctx.Err() != nil {
			return "", false, ctx.Err()
		}
		p, e := decodeOwnerProof(entry.Ledger.Record, key)
		if e != nil {
			return "", false, e
		}
		matches := p.Raw == ref || entry.Raw == ref
		if !matches {
			pins, e := m.expandProofPins(ctx, key, p)
			if e != nil {
				return "", false, e
			}
			for _, pin := range pins {
				if pin.Raw == ref {
					matches = true
					break
				}
			}
		}
		if matches {
			if e = m.finishRetirement(ctx, key, entry); errors.Is(e, ingest.ErrCommitUnknown) {
				return "", false, nil
			} else if e != nil {
				return "", false, e
			}
			return fmt.Sprintf("archive-owner-ledger/%s/%s", entry.Raw.Digest, entry.Ledger.Record.CommitID), true, nil
		}
	}
	return "", false, nil
}
