package ingest

import (
	"context"
	"errors"
	"testing"
)

func TestFactoryGlobalGenerationAndRevocation(t *testing.T) {
	d, b, _ := factoryFixture(t)
	candidates, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	anchor := CatalogAnchor{Chain: b.Chain, Height: 10, Hash: headHash("ee"), Generation: 2}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); err != nil {
		t.Fatal(err)
	}
	if err = d.RevokeBlock(context.Background(), b.Chain, b.Anchor.Hash); err != nil {
		t.Fatal(err)
	}
	if len(d.Snapshot(CatalogOnline)) != 0 {
		t.Fatal("reorg admission visible")
	}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); !errors.Is(err, ErrFactoryCandidate) {
		t.Fatal(err)
	}
	if _, err = d.Discover(context.Background(), b); !errors.Is(err, ErrFactoryStale) {
		t.Fatal("reorg event rediscovered", err)
	}
}

func TestFactoryGenerationFloorCoversNewPoolsAndDoesNotCountReplacedStateTwice(t *testing.T) {
	d, b, _ := factoryFixture(t)
	d.options.Limits.MaxPools = 1
	candidates, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	anchor := CatalogAnchor{Chain: b.Chain, Height: 10, Hash: headHash("ee"), Generation: 2}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); err != nil {
		t.Fatal(err)
	}
	next := anchor
	next.Generation = 3
	if err = d.Admit(context.Background(), CatalogOnline, next, candidates); err != nil {
		t.Fatal("replacement counted as new capacity", err)
	}
	stale := anchor
	stale.Height = 11
	if err = d.Admit(context.Background(), CatalogOnline, stale, candidates); !errors.Is(err, ErrFactoryStale) {
		t.Fatal("global generation floor ignored", err)
	}
}

func TestFactoryRevokedAdmissionAnchorCannotBeReused(t *testing.T) {
	d, b, _ := factoryFixture(t)
	c, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	a := CatalogAnchor{Chain: b.Chain, Height: 10, Hash: headHash("ee"), Generation: 1}
	if err = d.Admit(context.Background(), CatalogOnline, a, c); err != nil {
		t.Fatal(err)
	}
	if err = d.RevokeBlock(context.Background(), b.Chain, a.Hash); err != nil {
		t.Fatal(err)
	}
	if err = d.Admit(context.Background(), CatalogOnline, a, c); !errors.Is(err, ErrFactoryStale) {
		t.Fatal("revoked HTTP anchor reused", err)
	}
}

func TestFactorySourceFilterKeepsCompleteBlockIntegrity(t *testing.T) {
	for _, mode := range []string{"mixed", "only-unconfigured", "outside-height", "bad-position", "bad-proof"} {
		t.Run(mode, func(t *testing.T) {
			d, b, _ := factoryFixture(t)
			d.options.FilterSources = true
			unrelated := b.Receipts[0].Logs[0]
			unrelated.Address = factoryAddr(99)
			unrelated.LogIndex = 1
			b.Receipts[0].Logs = append(b.Receipts[0].Logs, unrelated)
			expected := 1
			switch mode {
			case "only-unconfigured":
				b.Receipts[0].Logs[0].Address = factoryAddr(98)
				expected = 0
			case "outside-height":
				source := d.sources[b.Chain+":"+factoryAddr(10)]
				source.Since = b.Anchor.Height + 1
				d.sources[b.Chain+":"+source.Address] = source
				expected = 0
			case "bad-position":
				b.Receipts[0].Logs[1].LogIndex = 9
			case "bad-proof":
				d.options.VerifyBlock = func(context.Context, FactoryBlock) error { return errors.New("bad full receipt proof") }
			}
			candidates, err := d.Discover(context.Background(), b)
			if mode == "bad-position" || mode == "bad-proof" {
				if err == nil {
					t.Fatal("filtered source bypassed proof/location validation")
				}
				return
			}
			if err != nil || len(candidates) != expected {
				t.Fatal(candidates, err)
			}
		})
	}
}

func TestFactoryProofIntentReferenceMatchesArchiveIdentity(t *testing.T) {
	d, b, _ := factoryFixture(t)
	owner := b.Raw.Owner
	owner.ProviderID = "factory-proof-intent"
	data := []byte("bounded factory proof")
	expected, err := FactoryProofReference(owner, data)
	if err != nil {
		t.Fatal(err)
	}
	actual, err := d.options.Archive.PutPinned(owner, data, "factory-intent-reference")
	if err != nil || actual != expected {
		t.Fatal("intent identity diverged from actual archive record", actual, expected, err)
	}
	if _, err = FactoryProofReference(Owner{}, data); err == nil {
		t.Fatal("invalid intent owner accepted")
	}
}
