package service

import (
	"context"
	"encoding/json"
	"errors"

	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
)

const RawReorgOwnerKind = "raw-reorg-owner-v1"
const RawOwnerUnitCaptureVersion = "owner-unit-capture-v1"
const RawReorgDecoderVersion = "reorg-unit/public-adapter-v1"

// BeginOwnerCapture owns the entire multi-read reorg, not an individual block.
// Neither nested Observe nor control publications can acknowledge these inputs.
func (m *RawMaintenance) BeginOwnerCapture(ctx context.Context, key, purpose string) (context.Context, func(), error) {
	if m == nil {
		return ctx, func() {}, nil
	}
	if purpose != "reorg" {
		return nil, nil, errors.New("RAW_OWNER_CAPTURE_PURPOSE")
	}
	if g, _ := ctx.Value(rawCaptureKey{}).(*rawCapture); g != nil {
		if g.Chain != key || g.Kind != "owner-reorg" {
			return nil, nil, ingest.ErrOwnership
		}
		return ctx, func() {}, nil
	}
	child, _, finish, e := m.beginResponsibleCapture(ctx, key, "owner-reorg")
	return child, finish, e
}

// OwnerUnitRecords binds the exact complete REORG_COMMIT (including every fact
// block) to a compact group root. It does not relax reducer StageReorg checks.
func (m *RawMaintenance) OwnerUnitRecords(ctx context.Context, unit journal.Unit) (map[string]json.RawMessage, error) {
	if m == nil {
		return nil, nil
	}
	if unit.Kind == "REORG_BEGIN" || unit.Kind == "ONLINE_SCOPE_REVOKED" {
		return nil, nil
	}
	if unit.Kind != "REORG_COMMIT" {
		return nil, errors.New("RAW_OWNER_UNIT_KIND")
	}
	g, _ := ctx.Value(rawCaptureKey{}).(*rawCapture)
	if g == nil || g.Chain != unit.Snapshot.Reference.Block.ChainKey || g.Kind != "owner-reorg" {
		return nil, errors.New("RAW_OWNER_UNIT_CAPTURE_REQUIRED")
	}
	id, e := unit.CanonicalHash()
	if e != nil {
		return nil, e
	}
	g.mu.Lock()
	if len(g.OwnerRecords) > 0 {
		copy := cloneRawRecords(g.OwnerRecords)
		same := g.OwnerUnitID == id
		g.mu.Unlock()
		if !same {
			return nil, ingest.ErrOwnership
		}
		return copy, nil
	}
	if g.Sealed || len(g.Pins) == 0 {
		g.mu.Unlock()
		return nil, errors.New("RAW_OWNER_INPUTS_INCOMPLETE")
	}
	g.Sealed = true
	g.OwnerUnitID = id
	pins := append([]RawPin(nil), g.Pins...)
	g.mu.Unlock()
	payload, e := json.Marshal(unit)
	if e != nil || int64(len(payload)) > m.cfg.MaxRawBytes {
		return nil, ingest.ErrBudget
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	reference := "owner-unit-" + g.ID
	ref, e := m.putEvidencePinned(ctx, g.Chain, ingest.Owner{Chain: g.Chain, Height: unit.Snapshot.Reference.Block.Number, Hash: rawDigest(payload), ProviderID: RawOwnerUnitCaptureVersion}, payload, reference)
	if e != nil {
		return nil, e
	}
	pins = append(pins, RawPin{Raw: ref, Reference: reference})
	root, e := m.savePinGroup(ctx, g.Chain, g.ID, unit.Snapshot.Reference.Block.Number, pins)
	if e != nil {
		return nil, e
	}
	p := rawOwnerProof{Kind: RawReorgOwnerKind, CommitID: id, Reference: unit.Snapshot.Reference, Groups: []rawPinGroup{root}, Raw: ref, DecoderVersion: RawReorgDecoderVersion}
	encoded, e := json.Marshal(p)
	if e != nil || len(encoded) > 65536 {
		return nil, ingest.ErrBudget
	}
	records := map[string]json.RawMessage{"online-replay/" + g.ID: encoded}
	g.mu.Lock()
	g.OwnerRecords = cloneRawRecords(records)
	g.mu.Unlock()
	return records, nil
}
