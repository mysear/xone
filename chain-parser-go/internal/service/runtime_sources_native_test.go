package service

import (
	"context"
	"encoding/json"
	"strconv"
	"strings"
	"testing"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/ingest"
)

func TestRuntimeFactoryNormalArchivedNativeProofRetainsSettlingAuthority(t *testing.T) {
	f := nativeServicePublicFixture(t, "avalanche-current")
	catalog := nativeServiceCatalog(t, f)
	adapter := &evm.Adapter{Client: f.client, Chain: f.chain}
	ctx := context.Background()
	block, err := adapter.Block(ctx, f.block.Number)
	if err != nil {
		t.Fatal(err)
	}
	block, receipts, err := adapter.ReadQualifiedReceipts(ctx, block, false)
	if err != nil {
		t.Fatal(err)
	}
	proof := factoryHTTPProof{Header: block, Receipts: receipts}
	raw, err := json.Marshal(proof)
	if err != nil {
		t.Fatal(err)
	}
	if err = json.Unmarshal(raw, &proof); err != nil {
		t.Fatal(err)
	}
	projected, err := catalog.projectHTTPProofContext(ctx, proof)
	if err != nil {
		t.Fatal(err)
	}
	height, _ := strconv.ParseUint(strings.TrimPrefix(block.Number, "0x"), 16, 64)
	projected.Raw, err = catalog.options.Archive.Put(ingest.Owner{Chain: f.chain.Key, Height: height, Hash: block.Hash, ProviderID: "verified-normal-complete"}, raw)
	if err != nil {
		t.Fatal(err)
	}
	service := &Service{catalogRuntime: map[string]*CatalogRuntime{f.chain.Key: catalog}, adapters: map[string]*evm.Adapter{f.chain.Key: adapter}}
	if err = service.verifyRuntimeFactoryProofObserved(ctx, f.chain, projected); err != nil {
		t.Fatal("normal runtime discarded valid archived native proof:", err)
	}
	if proof.NativeEvidence == nil || proof.NativeEvidence.SAE == nil {
		t.Fatal("missing native archive sidecar")
	}
	headers := proof.NativeEvidence.SAE.Headers
	var settling evm.RawBlock
	if err = json.Unmarshal(headers[len(headers)-1], &settling); err != nil {
		t.Fatal(err)
	}
	var changed map[string]any
	if err = json.Unmarshal(f.headers[settling.Number], &changed); err != nil {
		t.Fatal(err)
	}
	changed["hash"] = "0x" + strings.Repeat("ff", 32)
	f.headers[settling.Number], _ = json.Marshal(changed)
	if err = service.verifyRuntimeFactoryProofObserved(ctx, f.chain, projected); err == nil {
		t.Fatal("runtime reused changed settling authority while target stayed canonical")
	}
}
