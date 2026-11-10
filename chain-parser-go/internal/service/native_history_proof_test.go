package service

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

func TestNativeHistoryActualPublicOwnedArchiveAndReplay(t *testing.T) {
	f := nativeServicePublicFixture(t, "avalanche-current")
	r := nativeServiceCatalog(t, f)
	h := &historyPublicSource{adapter: evm.Adapter{Client: f.client, Chain: f.chain}, recorder: newRawRecorder(f.chain.Key, r.options.Archive), maxBytes: 16 << 20}
	h.installNativeHistoryEvidence()
	group := &rawCapture{ID: "native-history-public", Chain: f.chain.Key, Max: 64, Kind: "history"}
	ctx := context.WithValue(context.Background(), rawCaptureKey{}, group)
	f.client.SetRecorder(h.record)
	batch, _, err := h.read(ctx, 99642059)
	if err != nil {
		t.Fatal(err)
	}
	if len(batch.Blocks) != 1 || batch.Blocks[0].Block.Hash != f.block.Hash {
		t.Fatal("history exact height changed", batch)
	}
	var proof ingest.RawRef
	for _, pin := range group.Pins {
		if pin.Raw.Owner.ProviderID == "native-history-proof/v2" {
			proof = pin.Raw
		}
	}
	if proof.ID == "" {
		t.Fatal("native successor proof not owned by current capture")
	}
	restored, _, err := ReplayNativeHistoryEvidence(ctx, r.options.Archive, proof, f.chain)
	if err != nil {
		t.Fatal(err)
	}
	if len(restored.Blocks) != 1 || restored.Blocks[0].Block != batch.Blocks[0].Block {
		t.Fatal("history replay changed block")
	}
	raw, err := r.options.Archive.Read(proof)
	if err != nil {
		t.Fatal(err)
	}
	var envelope nativeHistoryArchive
	if err = json.Unmarshal(raw, &envelope); err != nil {
		t.Fatal(err)
	}
	if envelope.Kind != "native-history-proof/v2" || envelope.Evidence.NativeSemanticsID == "" || envelope.Evidence.Evidence.SAE == nil {
		t.Fatal("native proof mislabelled as RPC response")
	}
	if h.adapter.OnHistoryEvidence == nil {
		t.Fatal("source hook absent")
	}
	// No capture owner, sealed capture, wrong chain and exhausted pin budget must
	// backpressure before publishing facts, even when network bytes are valid.
	for _, g := range []*rawCapture{nil, {ID: "sealed", Chain: f.chain.Key, Max: 1, Sealed: true}, {ID: "wrong", Chain: "evm:1", Max: 1}, {ID: "full", Chain: f.chain.Key, Max: 0}} {
		rejected := context.Background()
		if g != nil {
			rejected = context.WithValue(rejected, rawCaptureKey{}, g)
		}
		h.remaining = h.maxBytes
		if err = h.captureNativeHistoryEvidence(rejected, envelope.Evidence); err == nil {
			t.Fatal("unowned native history artifact accepted")
		}
	}
	changed := f.chain
	changed.Pools = []config.PoolConfig{{ID: "0x28c96a719edc7c1ba3fb1bafcadf470ace9dfd75", Protocol: "uniswap-v2"}}
	if _, _, err = ReplayNativeHistoryEvidence(ctx, r.options.Archive, proof, changed); err == nil {
		t.Fatal("history proof replayed under another closed scope")
	}
	envelope.Evidence.Evidence.SAE.Headers = envelope.Evidence.Evidence.SAE.Headers[:1]
	invalid, _ := json.Marshal(envelope)
	altered, err := r.options.Archive.Put(ingest.Owner{Chain: f.chain.Key, Height: 99642059, Hash: rawDigest(invalid), ProviderID: "native-history-proof/v2"}, invalid)
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err = ReplayNativeHistoryEvidence(ctx, r.options.Archive, altered, f.chain); err == nil {
		t.Fatal("missing settling evidence replayed")
	}
	h.adapter.OnHistoryEvidence = func(context.Context, evm.NativeHistoryEvidence) error { return errors.New("sink unavailable") }
	h.remaining = h.maxBytes
	if _, _, err = h.read(ctx, 99642059); err == nil {
		t.Fatal("history discarded sink failure")
	}
}
