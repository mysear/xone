package evm

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

func TestHistoryFactsPublicReceiptsWithoutStateCalls(t *testing.T) {
	fixture := newReceiptWitnessRPC()
	server := fixture.serve(t)
	defer server.Close()
	adapter := witnessAdapter(server.URL)
	batch, e := adapter.ReadFacts(context.Background(), fixture.block.Number)
	if e != nil {
		t.Fatal(e)
	}
	if fixture.stateReads.Load() != 0 {
		t.Fatal("history requested account state")
	}
	if len(batch.Blocks) != 1 {
		t.Fatal(batch)
	}
	if e = batch.Validate("BLOCK_COMMIT", batch.Blocks[0].Block); e != nil {
		t.Fatal(e)
	}
}
func TestHistoryFactsRejectRootTamperAndCancellation(t *testing.T) {
	fixture := newReceiptWitnessRPC()
	fixture.bad = "drop-final-log"
	server := fixture.serve(t)
	defer server.Close()
	adapter := witnessAdapter(server.URL)
	if _, e := adapter.ReadFacts(context.Background(), fixture.block.Number); e == nil {
		t.Fatal("tampered root accepted")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, e := adapter.ReadFacts(ctx, fixture.block.Number); e == nil {
		t.Fatal("cancellation ignored")
	}
}

func TestHistoryFactsFactoryCandidatesAndSwapArePublicOnly(t *testing.T) {
	fixture := newReceiptWitnessRPC()
	address := "0x" + strings.Repeat("aa", 20)
	factory := "0x" + strings.Repeat("bb", 20)
	fixture.receipts[0].Logs[0].Address = address
	fixture.receipts[0].Logs[0].Topics = []string{V2SwapTopic, "0x" + strings.Repeat("0", 64), "0x" + strings.Repeat("0", 64)}
	fixture.receipts[0].Logs[0].Data = "0x" + fmt.Sprintf("%064x%064x%064x%064x", 10, 0, 0, 20)
	fixture.receipts[2].Logs[0].Address = factory
	fixture.receipts[2].Logs[0].Topics = []string{ingest.V2PairCreatedTopic, "0x" + fmt.Sprintf("%064x", 1), "0x" + fmt.Sprintf("%064x", 2)}
	fixture.receipts[2].Logs[0].Data = "0x" + fmt.Sprintf("%064x%064x", 3, 1)
	server := fixture.serve(t)
	adapter := witnessAdapter(server.URL)
	adapter.Chain.Pools = []config.PoolConfig{{ID: address, Protocol: "uniswap-v2", Token0: "token0", Token1: "token1"}}
	adapter.Chain.DynamicCatalog = &config.DynamicCatalogConfig{MaxCandidates: 8, Sources: []config.FactoryDeployment{{Address: factory, Version: "uniswap-v2", ProvenanceID: "test"}}}
	facts, candidates, e := adapter.ReadFactsAndCatalog(context.Background(), fixture.block.Number)
	if e != nil || len(facts.Blocks[0].Swaps) != 1 || len(candidates) != 1 || candidates[0].Pool != "0x"+fmt.Sprintf("%040x", 3) {
		t.Fatal(facts, candidates, e)
	}
	if fixture.stateReads.Load() != 0 {
		t.Fatal("read historical account state")
	}
}

func TestHistoryFactsRejectWitnessDisagreementAndAnchorChange(t *testing.T) {
	good := newReceiptWitnessRPC()
	one := good.serve(t)
	bad := newReceiptWitnessRPC()
	bad.bad = "different-data"
	two := bad.serve(t)
	adapter := witnessAdapter(one.URL, two.URL)
	if _, e := adapter.ReadFacts(context.Background(), good.block.Number); e == nil {
		t.Fatal("accepted divergent receipt witness")
	}
	changed := newReceiptWitnessRPC()
	changed.bad = "root-after-receipts"
	server := changed.serve(t)
	adapter = witnessAdapter(server.URL)
	if _, e := adapter.ReadFacts(context.Background(), changed.block.Number); e == nil {
		t.Fatal("accepted changed anchor")
	}
}

func TestHistoryReviewDatasetBindsRealSwapAndFactoryDecoderScope(t *testing.T) {
	f := newReceiptWitnessRPC()
	pool := "0x" + strings.Repeat("bb", 20)
	f.receipts[0].Logs[0].Address = pool
	f.receipts[0].Logs[0].Topics = []string{V2SwapTopic, "0x" + strings.Repeat("0", 64), "0x" + strings.Repeat("0", 64)}
	f.receipts[0].Logs[0].Data = "0x" + fmt.Sprintf("%064x%064x%064x%064x", 10, 0, 0, 20)
	server := f.serve(t)
	a := witnessAdapter(server.URL)
	a.Chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("aa", 20), Protocol: "uniswap-v2", Token0: "a", Token1: "b"}}
	idA, e := HistoryDatasetID(a.Chain)
	if e != nil {
		t.Fatal(e)
	}
	empty, e := a.ReadFacts(context.Background(), f.block.Number)
	if e != nil || len(empty.Blocks[0].Swaps) != 0 {
		t.Fatal(empty, e)
	}
	b := a.Chain.Pools[0]
	b.ID = pool
	a.Chain.Pools = append(a.Chain.Pools, b)
	idB, e := HistoryDatasetID(a.Chain)
	if e != nil || idA == idB {
		t.Fatal("pool addition reused dataset", idA, idB, e)
	}
	facts, e := a.ReadFacts(context.Background(), f.block.Number)
	if e != nil || len(facts.Blocks[0].Swaps) != 1 {
		t.Fatal(facts, e)
	}
	a.Chain.Pools[0], a.Chain.Pools[1] = a.Chain.Pools[1], a.Chain.Pools[0]
	idReordered, e := HistoryDatasetID(a.Chain)
	if e != nil || idReordered != idB {
		t.Fatal("order changed scope identity", e)
	}
	a.Chain.DynamicCatalog = &config.DynamicCatalogConfig{MaxCandidates: 8, Sources: []config.FactoryDeployment{{Address: "factory-A", Version: "uniswap-v2", ProvenanceID: "deployment-A", Since: 1}}}
	idFactoryA, _ := HistoryDatasetID(a.Chain)
	a.Chain.DynamicCatalog.Sources[0].Until = 100
	idFactoryB, _ := HistoryDatasetID(a.Chain)
	if idFactoryA == idFactoryB || idFactoryA == idB {
		t.Fatal("factory deployment interval not bound")
	}
}
