package service

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"sync"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
)

type RawPin struct {
	Raw       ingest.RawRef
	Reference string
}
type rawCaptureKey struct{}
type rawCapture struct {
	mu           sync.Mutex
	ID, Chain    string
	Max          int
	Pins         []RawPin
	Kind         string
	Sealed       bool
	OwnerUnitID  string
	OwnerRecords map[string]json.RawMessage
}
type stagedAcquisition struct {
	Kind  string
	Group string
	Raw   ingest.RawRef
	Base  domain.SnapshotRef
	Pins  []RawPin
	Proof ingest.RawRef `json:"-"`
}

func rawDigest(b []byte) string { sum := sha256.Sum256(b); return hex.EncodeToString(sum[:]) }
func rawGroup() string {
	var b [32]byte
	if _, e := rand.Read(b[:]); e != nil {
		panic(e)
	}
	return hex.EncodeToString(b[:])
}

// Replay decode calls this only inside enter(), which already registers its
// cancellation and wait group. Normal reads use beginResponsibleCapture.
func (m *RawMaintenance) newCapture(key string) (context.Context, *rawCapture) {
	g := &rawCapture{ID: rawGroup(), Chain: key, Max: m.cfg.MaxPinsPerCapture, Kind: "fact"}
	return context.WithValue(context.Background(), rawCaptureKey{}, g), g
}

type maintenanceReader struct {
	m      *RawMaintenance
	key    string
	base   domain.SnapshotRef
	reader stateBlockReader
}

func (m *RawMaintenance) ProtectReader(key string, base domain.SnapshotRef, reader stateBlockReader) stateBlockReader {
	return maintenanceReader{m, key, base, reader}
}
func (r maintenanceReader) ReadComplete(ctx context.Context, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	if r.m.chains[r.key] == nil {
		return domain.BlockRef{}, nil, nil, errors.New("RAW_CHAIN")
	}
	ctx, group, finish, e := r.m.beginResponsibleCapture(ctx, r.key, "fact")
	if e != nil {
		return domain.BlockRef{}, nil, nil, e
	}
	// Completion seals and unregisters this read; it never releases its inputs.
	defer finish()
	block, pools, swaps, e := r.reader.ReadComplete(ctx, tag)
	if e != nil {
		return block, pools, swaps, e
	}
	raw, e := json.Marshal(stagedState{block, pools, swaps})
	if e != nil || int64(len(raw)) > r.m.cfg.MaxRawBytes {
		return block, nil, nil, ingest.ErrBudget
	}
	ref, e := r.m.putEvidencePinned(ctx, r.key, ingest.Owner{Chain: r.key, Height: block.Number, Hash: block.Hash, ProviderID: "derived-state-from-recorded-rpc"}, raw, "staged-"+group.ID)
	if e != nil {
		return block, nil, nil, e
	}
	group.mu.Lock()
	pins := append([]RawPin(nil), group.Pins...)
	group.mu.Unlock()
	pins = append(pins, RawPin{ref, "staged-" + group.ID})
	acq := stagedAcquisition{Kind: "staged-acquisition-v1", Group: group.ID, Raw: ref, Base: r.base, Pins: pins}
	encoded, e := json.Marshal(acq)
	if e != nil {
		return block, nil, nil, e
	}
	acq.Proof, e = r.m.putEvidencePinned(ctx, r.key, ingest.Owner{Chain: r.key, Height: block.Number, Hash: rawDigest(encoded), ProviderID: "staged-acquisition-v1"}, encoded, "acquisition-"+group.ID)
	if e != nil {
		return block, nil, nil, e
	}
	r.m.mu.Lock()
	r.m.chains[r.key].acquisitions = append(r.m.chains[r.key].acquisitions, acq)
	r.m.mu.Unlock()
	return block, pools, swaps, nil
}
func acquisitionPins(a stagedAcquisition) []RawPin {
	out := append([]RawPin(nil), a.Pins...)
	if a.Proof.ID != "" {
		out = append(out, RawPin{a.Proof, "acquisition-" + a.Group})
	}
	return out
}

type rawOwnerProof struct {
	Kind             string
	Intent           *ingest.ReplayIntent `json:",omitempty"`
	CommitID         string
	Reference        domain.SnapshotRef
	Pins             []RawPin      `json:",omitempty"`
	Groups           []rawPinGroup `json:",omitempty"`
	Raw              ingest.RawRef
	DecoderVersion   string
	ScopeJob         string `json:",omitempty"`
	ScopeFenceDigest string `json:",omitempty"`
}

// OwnerRecordsForUnit is used under the normal chain actor, before the same
// prepared+apply owner batch. No pin release is inferred from cursor proximity.
func (m *RawMaintenance) OwnerRecordsForUnit(unit journal.Unit) (map[string]json.RawMessage, error) {
	id, e := unit.CanonicalHash()
	if e != nil {
		return nil, e
	}
	m.mu.Lock()
	state := m.chains[unit.Snapshot.Reference.Block.ChainKey]
	var captures []stagedAcquisition
	if state != nil {
		captures = append(captures, state.acquisitions...)
	}
	m.mu.Unlock()
	records := map[string]json.RawMessage{}
	for _, a := range captures {
		r := unit.Snapshot.Reference
		if a.Raw.Owner.Height != r.Block.Number || a.Raw.Owner.Hash != r.Block.Hash || a.Base.OnlineGeneration != r.OnlineGeneration || a.Base.WriterEpoch != r.WriterEpoch || a.Base.ScopeHash != r.ScopeHash || a.Base.AdapterSetHash != r.AdapterSetHash {
			continue
		}
		data, e := state.recorder.archive.Read(a.Raw)
		if e != nil {
			return nil, e
		}
		var staged stagedState
		if json.Unmarshal(data, &staged) != nil || staged.Block != r.Block {
			return nil, errors.New("RAW_CAPTURE_UNIT_IDENTITY")
		}
		observed, e := domain.Hash(staged.Pools)
		if e != nil {
			return nil, e
		}
		published, e := domain.Hash(unit.Snapshot.Pools)
		if e != nil || observed != published {
			return nil, errors.New("RAW_CAPTURE_UNIT_STATE_CONFLICT")
		}
		if hasV2Scope(staged.Pools) {
			if unit.NormalizedFacts == nil || len(unit.NormalizedFacts.Blocks) != 1 || unit.NormalizedFacts.Blocks[0].Block != staged.Block {
				return nil, errors.New("RAW_CAPTURE_UNIT_FACTS_CONFLICT")
			}
			expected, _ := domain.Hash(staged.Swaps)
			actual, _ := domain.Hash(unit.NormalizedFacts.Blocks[0].Swaps)
			if expected != actual {
				return nil, errors.New("RAW_CAPTURE_UNIT_FACTS_CONFLICT")
			}
		}
		p := rawOwnerProof{Kind: "raw-capture-owner-v1", CommitID: id, Reference: r, Groups: []rawPinGroup{acquisitionGroup(a)}, Raw: a.Raw, DecoderVersion: RawStagedDecoderVersion}
		encoded, e := json.Marshal(p)
		if e != nil || len(encoded) > 65536 {
			return nil, ingest.ErrBudget
		}
		records["online-replay/"+a.Group] = encoded
	}
	return records, nil
}
func (m *RawMaintenance) releaseProof(key string, p rawOwnerProof) error {
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(m.cfg.TimeoutMS)*time.Millisecond)
	defer cancel()
	return m.releaseProofContext(ctx, key, p)
}
func (m *RawMaintenance) AcknowledgeOwnerRecords(unit journal.Unit, records map[string]json.RawMessage) error {
	key := unit.Snapshot.Reference.Block.ChainKey
	id, e := unit.CanonicalHash()
	if e != nil {
		return e
	}
	for k, payload := range records {
		record, e := m.s.stores[key].OwnerRecord(k)
		if e != nil || record.CommitID != id || record.Reference != unit.Snapshot.Reference || !bytes.Equal(record.Payload, payload) {
			return errors.New("RAW_OWNER_PROOF_UNCONFIRMED")
		}
		var p rawOwnerProof
		if json.Unmarshal(payload, &p) != nil {
			return errors.New("RAW_OWNER_PROOF_INVALID")
		}
		ctx, cancel := context.WithTimeout(context.Background(), time.Duration(m.cfg.TimeoutMS)*time.Millisecond)
		if p.Kind == RawReorgOwnerKind {
			data, err := m.readEvidence(ctx, key, p.Raw)
			expected, encodeErr := json.Marshal(unit)
			if err != nil || encodeErr != nil || !bytes.Equal(data, expected) {
				cancel()
				return errors.New("RAW_OWNER_UNIT_INPUT_MISMATCH")
			}
		}
		if p.Kind == RawBaselineOwnerKind {
			data, err := m.readEvidence(ctx, key, p.Raw)
			if err != nil || !baselineStateMatches(data, unit) {
				cancel()
				return errors.Join(errors.New("RAW_BASELINE_INPUT_MISMATCH"), err)
			}
		}
		entry, err := m.persistOwnerLedger(ctx, key, record)
		if err == nil {
			err = m.finishRetirement(ctx, key, entry)
		}
		cancel()
		if err != nil {
			return err
		}
	}
	return nil
}
