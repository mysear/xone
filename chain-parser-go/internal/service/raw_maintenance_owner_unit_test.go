package service

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"

	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func TestRawMaintenanceReorgCaptureBindsAllFactBlocksAndKeepsUnconfirmedPins(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	ctx, finish, e := m.BeginOwnerCapture(context.Background(), key, "reorg")
	if e != nil {
		t.Fatal(e)
	}
	defer finish()
	current, ok := s.stores[key].Current()
	if !ok {
		t.Fatal("baseline")
	}
	for _, kind := range []string{"REORG_BEGIN", "ONLINE_SCOPE_REVOKED"} {
		records, e := m.OwnerUnitRecords(ctx, journal.Unit{Kind: kind, Snapshot: current})
		if e != nil || len(records) != 0 {
			t.Fatal("control publication fabricated evidence", records, e)
		}
	}
	// Both real source reads share one responsibility. Controls above must not seal it.
	first, _, _, e := s.adapters[key].ReadComplete(ctx, "0x64")
	if e != nil {
		t.Fatal(e)
	}
	last, pools, swaps, e := s.adapters[key].ReadComplete(ctx, "0x65")
	if e != nil {
		t.Fatal(e)
	}
	next := current
	next.Reference.Block = last
	next.Pools = pools
	reverted := first
	unit := journal.Unit{Kind: "REORG_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{
		SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: first}, {Block: last, Swaps: swaps}}, RevertedBlocks: []domain.BlockRef{reverted},
	}}
	records, e := m.OwnerUnitRecords(ctx, unit)
	if e != nil {
		t.Fatal(e)
	}
	if len(records) != 1 {
		t.Fatal(records)
	}
	for _, payload := range records {
		if len(payload) > 65536 {
			t.Fatal("unbounded owner payload")
		}
		var proof rawOwnerProof
		if e = json.Unmarshal(payload, &proof); e != nil {
			t.Fatal(e)
		}
		raw, e := s.rawSources[key].archive.Read(proof.Raw)
		if e != nil {
			t.Fatal(e)
		}
		expected, e := json.Marshal(unit)
		if e != nil || !bytes.Equal(raw, expected) {
			t.Fatal("owner evidence lost part of REORG_COMMIT", e)
		}
		if proof.Kind != RawReorgOwnerKind || proof.DecoderVersion != RawReorgDecoderVersion || len(proof.Groups) != 1 {
			t.Fatal(proof)
		}
	}
	if e = m.AcknowledgeOwnerRecords(unit, records); e == nil {
		t.Fatal("source-read completion was accepted as owner commit")
	}
	changed := unit
	batch := *unit.NormalizedFacts
	batch.RevertedBlocks = append([]domain.BlockRef(nil), batch.RevertedBlocks...)
	batch.RevertedBlocks[0].Hash = "0x9999999999999999999999999999999999999999999999999999999999999999"
	changed.NormalizedFacts = &batch
	if _, e = m.OwnerUnitRecords(ctx, changed); e == nil {
		t.Fatal("different full unit reused sealed group proof")
	}
	g := ctx.Value(rawCaptureKey{}).(*rawCapture)
	g.mu.Lock()
	pins := append([]RawPin(nil), g.Pins...)
	g.mu.Unlock()
	if len(pins) < 2 {
		t.Fatal("source reads not in reorg group", pins)
	}
	status, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	for _, pin := range pins {
		found := false
		for _, r := range status {
			if r.Raw == pin.Raw {
				found = true
				if r.ActiveReferences == 0 || r.Reclaimed {
					t.Fatal("unconfirmed reorg released input", r)
				}
			}
		}
		if !found {
			t.Fatal("missing reorg input", pin)
		}
	}
}

func TestRawMaintenanceFenceDigestKeepsFullWidthOwnerIdentity(t *testing.T) {
	a, e := fenceDigest(json.RawMessage(`{"writer_epoch":9007199254740992,"generation":1}`))
	if e != nil {
		t.Fatal(e)
	}
	b, e := fenceDigest(json.RawMessage(`{"writer_epoch":9007199254740993,"generation":1}`))
	if e != nil {
		t.Fatal(e)
	}
	if a == b {
		t.Fatal("distinct uint64 owner fences collapsed to one authority digest")
	}
	same, e := fenceDigest(json.RawMessage(`{ "generation":1, "writer_epoch":9007199254740992 }`))
	if e != nil || same != a {
		t.Fatal("object ordering changed fence identity", same, e)
	}
}

func TestRawMaintenanceZeroPoolOwnerProofRequiresObservationResponsibility(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	ctx, finish, e := m.BeginBaselineCapture(context.Background(), key)
	if e != nil {
		t.Fatal(e)
	}
	defer finish()
	current, ok := s.stores[key].Current()
	if !ok {
		t.Fatal("baseline")
	}
	current.Pools = nil
	_, e = m.BaselineOwnerRecords(ctx, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: current}, "", nil)
	if e == nil || e.Error() != "RAW_D0_OBSERVATION_REQUIRED" {
		t.Fatal("empty pools must use explicit observation-only lifecycle", e)
	}
}
