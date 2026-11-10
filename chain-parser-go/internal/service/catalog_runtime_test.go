package service

import (
	"context"
	"encoding/json"
	"errors"
	"path/filepath"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

func TestCatalogRuntimeRequiresProofAndOwner(t *testing.T) {
	if _, err := OpenCatalogRuntime(CatalogRuntimeOptions{Path: filepath.Join(t.TempDir(), "catalog"), Chain: config.Chain{Key: "evm:1"}}); err == nil {
		t.Fatal("unwired catalog opened")
	}
}
func TestCatalogRuntimeFenceOrdering(t *testing.T) {
	current := CatalogFence{Assignment: 1, WriterEpoch: 2, CanonicalEpoch: 3, Generation: 4}
	for _, bad := range []CatalogFence{{1, 1, 3, 5}, {1, 2, 2, 5}, {1, 2, 3, 3}, {0, 2, 3, 4}} {
		if catalogFenceFollows(bad, current) {
			t.Fatal("fence rollback", bad)
		}
	}
	if !catalogFenceFollows(current, current) {
		t.Fatal("idempotent fence")
	}
}
func TestCatalogRuntimePersistenceIsHintAndRevocationSurvivesRestart(t *testing.T) {
	// Test the durable boundary without granting admissions to caller entries.
	options := catalogTestOptions(t)
	r, err := OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	f := CatalogFence{1, 1, 1, 1}
	if err = r.Revoke(ctx, []string{runtimeHash("f")}, f); err != nil {
		t.Fatal(err)
	}
	if s := r.CatalogSnapshot(ingest.CatalogOnline); s.Ready || len(s.Entries) != 0 {
		t.Fatal(s)
	}
	if err = r.Close(); err != nil {
		t.Fatal(err)
	}
	r, err = OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	if s := r.CatalogSnapshot(ingest.CatalogOnline); s.Ready {
		t.Fatal("disk bypassed revalidation")
	}
	if err = r.Admit(ctx, ingest.CatalogOnline, []ingest.FactoryCandidate{{ID: "forged"}}); !errors.Is(err, ingest.ErrFactoryStale) {
		t.Fatal("candidate bypassed anchor", err)
	}
	if !r.revoked(runtimeHash("f")) {
		t.Fatal("orphan tombstone lost")
	}
}

func catalogTestOptions(t *testing.T) CatalogRuntimeOptions { return catalogFixture(t).options }

func TestCatalogRuntimeBudgetCancelAndAdmissionCandidateGate(t *testing.T) {
	r, f := openCatalogFixture(t)
	ctx := context.Background()
	c, err := r.Discover(ctx, ingest.CatalogOnline, f.block(t))
	if err != nil {
		t.Fatal(err)
	}
	forged := c[0]
	forged.Pool = "0x0000000000000000000000000000000000000099"
	if r.Admit(ctx, ingest.CatalogOnline, []ingest.FactoryCandidate{forged}) == nil {
		t.Fatal("forged candidate accepted")
	}
	canceled, cancel := context.WithCancel(ctx)
	cancel()
	if r.Admit(canceled, ingest.CatalogOnline, c) == nil {
		t.Fatal("canceled admission")
	}
	r.options.Config.MaxBytes = 1024
	if r.Admit(ctx, ingest.CatalogOnline, c) == nil {
		t.Fatal("unbounded persistent state")
	}
	if r.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("failed persistence published")
	}
}

func TestCatalogRuntimeFreezeCannotOutliveRevocationOrMutation(t *testing.T) {
	r, f := openCatalogFixture(t)
	ctx := context.Background()
	c, err := r.Discover(ctx, ingest.CatalogOnline, f.block(t))
	if err != nil {
		t.Fatal(err)
	}
	if err = r.Admit(ctx, ingest.CatalogOnline, c); err != nil {
		t.Fatal(err)
	}
	frozen, err := r.Freeze(ctx, CatalogFence{1, 1, 1, 1}, []string{c[0].Pool})
	if err != nil {
		t.Fatal(err)
	}
	if err = r.ValidateFrozen(ctx, frozen); err != nil {
		t.Fatal(err)
	}
	configs := frozen.Configs()
	configs[0].V2.ExpectedFactory = "tampered"
	if err = r.ValidateFrozen(ctx, frozen); err != nil {
		t.Fatal("config accessor mutated frozen proof", err)
	}
	changed := frozen
	changed.Entries = append([]RuntimeCatalogEntry(nil), frozen.Entries...)
	changed.Entries[0].Pool.Reserve0 = "9999"
	if r.ValidateFrozen(ctx, changed) == nil {
		t.Fatal("mutated frozen entry accepted")
	}
	if err = r.Revoke(ctx, nil, CatalogFence{1, 2, 1, 2}); err != nil {
		t.Fatal(err)
	}
	if r.ValidateFrozen(ctx, frozen) == nil {
		t.Fatal("revoked scope handle accepted")
	}
}

func TestCatalogRuntimeTombstoneOverflowFailsClosedAcrossRestart(t *testing.T) {
	f := catalogFixture(t)
	f.options.Config.MaxCandidates = 1
	r, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	if err = r.Revoke(context.Background(), []string{runtimeHash("d")}, CatalogFence{1, 1, 1, 1}); err != nil {
		t.Fatal(err)
	}
	if r.Revoke(context.Background(), []string{runtimeHash("e")}, CatalogFence{1, 2, 2, 2}) == nil {
		t.Fatal("unbounded tombstones")
	}
	if err = r.Close(); err != nil {
		t.Fatal(err)
	}
	restored, err := OpenCatalogRuntime(f.options)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Close()
	if !restored.blocked {
		t.Fatal("lost overflow revocation barrier on restart")
	}
}

func TestCatalogRuntimeKeepsRawProofPinnedUntilDurableRevocation(t *testing.T) {
	r, f := openCatalogFixture(t)
	ctx := context.Background()
	block := f.block(t)
	if _, err := r.Discover(ctx, ingest.CatalogOnline, block); err != nil {
		t.Fatal(err)
	}
	records, err := f.options.Archive.RawRecords()
	if err != nil {
		t.Fatal(err)
	}
	if len(records) != 1 || records[0].ActiveReferences == 0 {
		t.Fatal("durable catalog proof was unpinned", records)
	}
	if err = r.Revoke(ctx, []string{block.Anchor.Hash}, CatalogFence{1, 2, 2, 2}); err != nil {
		t.Fatal(err)
	}
	records, err = f.options.Archive.RawRecords()
	if err != nil {
		t.Fatal(err)
	}
	if records[0].ActiveReferences != 0 {
		t.Fatal("orphan proof pin leaked after durable revocation", records)
	}
}

func TestCatalogRuntimeMixedFactoryBlockSelectsOnlyConfiguredSourcesAfterProof(t *testing.T) {
	r, f := openCatalogFixture(t)
	block := f.block(t)
	unrelated := block.Receipts[0].Logs[0]
	unrelated.Address = catalogAddr(99)
	unrelated.LogIndex = 1
	block.Receipts[0].Logs = append(block.Receipts[0].Logs, unrelated)
	raw, err := json.Marshal(block)
	if err != nil {
		t.Fatal(err)
	}
	block.Raw, err = f.options.Archive.Put(ingest.Owner{Chain: block.Chain, Height: block.Anchor.Height, Hash: block.Anchor.Hash, ProviderID: "mixed-public-http"}, raw)
	if err != nil {
		t.Fatal(err)
	}
	found, err := r.Discover(context.Background(), ingest.CatalogOnline, block)
	if err != nil || len(found) != 1 || found[0].Factory != catalogAddr(10) {
		t.Fatal("unconfigured event blocked configured factory coverage", found, err)
	}
	f.proofBad.Store(true)
	if _, err = r.Discover(context.Background(), ingest.CatalogOnline, block); err == nil {
		t.Fatal("source selection bypassed full block proof")
	}
}
