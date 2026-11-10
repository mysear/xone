package service

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"xone/chain-parser-go/internal/ingest"
)

func TestRuntimeSourcesPinIntentRecoversBeforeAndAfterArchiveWrite(t *testing.T) {
	for _, phase := range []string{"before-put", "after-put", "canceled-after-put"} {
		t.Run(phase, func(t *testing.T) {
			s, ch, snap := runtimeQueueFixture(t)
			c := s.catalogRuntime[ch.Key]
			proof := catalogHTTPProofFixture()
			proof.Header.Number = "0xb"
			proof.Header.Hash = runtimeHash("e")
			proof.Header.ParentHash = snap.Reference.Block.Hash
			for i := range proof.Receipts {
				proof.Receipts[i].BlockHash = proof.Header.Hash
				for j := range proof.Receipts[i].Logs {
					proof.Receipts[i].Logs[j].BlockHash = proof.Header.Hash
				}
			}
			raw, err := json.Marshal(proof)
			if err != nil {
				t.Fatal(err)
			}
			owner := ingest.Owner{Chain: ch.Key, Height: 11, Hash: proof.Header.Hash, ProviderID: "verified-normal-complete"}
			ref, err := ingest.FactoryProofReference(owner, raw)
			if err != nil {
				t.Fatal(err)
			}
			observation := runtimeObservedBlock{Height: 11, Hash: proof.Header.Hash, Source: runtimeFence(snap.Reference), Raw: ref}
			observation.Pin = runtimeDiscoveryPin(c, observation.Source, observation.Hash)
			intent := runtimeDiscoveryIntent{Version: 1, Observation: observation, Proof: raw}
			c.mu.Lock()
			queue, err := loadRuntimeDiscoveryLocked(c)
			if err == nil {
				err = prepareRuntimeDiscoveryIntentLocked(c, queue, intent)
			}
			c.mu.Unlock()
			if err != nil {
				t.Fatal(err)
			}
			if phase != "before-put" {
				written, err := c.options.Archive.PutPinned(owner, raw, observation.Pin)
				if err != nil || written != ref {
					t.Fatal(written, err)
				}
			}
			if phase == "canceled-after-put" {
				ctx, cancel := context.WithCancel(context.Background())
				cancel()
				if err = s.recoverRuntimeDiscoveryIntent(ctx, ch); !errors.Is(err, context.Canceled) {
					t.Fatal("canceled intent acquired completion authority", err)
				}
			}
			options := c.options
			if err = c.Close(); err != nil {
				t.Fatal(err)
			}
			reopened, err := OpenCatalogRuntime(options)
			if err != nil {
				t.Fatal(err)
			}
			s.catalogRuntime[ch.Key] = reopened
			t.Cleanup(func() { _ = reopened.Close() })
			for i := 0; i < 2; i++ {
				if err = s.recoverRuntimeDiscoveryIntent(context.Background(), ch); err != nil {
					t.Fatal("restart intent replay", err)
				}
			}
			reopened.mu.Lock()
			queue, err = loadRuntimeDiscoveryLocked(reopened)
			pending, e := loadRuntimeDiscoveryIntentLocked(reopened)
			reopened.mu.Unlock()
			if err != nil || e != nil || pending != nil || len(queue.Observed) != 1 || queue.Observed[0] != observation {
				t.Fatal("intent did not become exactly one durable observation", queue, err, e)
			}
			if err = s.revokeRuntimeDiscovery(ch, []string{observation.Hash}); err != nil {
				t.Fatal(err)
			}
			if err = s.flushRuntimeDiscoveryReleases(context.Background(), ch); err != nil {
				t.Fatal(err)
			}
			records, err := options.Archive.RawRecords()
			if err != nil {
				t.Fatal(err)
			}
			for _, record := range records {
				if record.Raw == ref && record.ActiveReferences != 0 {
					t.Fatal("recovered intent leaked its exact raw pin", record)
				}
			}
		})
	}
}
