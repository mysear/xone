package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

func TestNativeBNBHistorySwapFactDatasetAndLegacyArchive(t *testing.T) {
	ctx := context.Background()
	v := loadChainReceiptVector(t, "bsc")
	load := func(name string, out any) {
		t.Helper()
		raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "bsc", name+".json"))
		if err != nil {
			t.Fatal(err)
		}
		if err = json.Unmarshal(raw, out); err != nil {
			t.Fatal(err)
		}
	}
	var captured []struct {
		Request  struct{ Method string }
		Response struct{ Result json.RawMessage }
	}
	load("rpc", &captured)
	var proof ExecutionReceiptBlock
	for _, r := range captured {
		if r.Request.Method == "eth_getBlockByNumber" {
			proof.Header = r.Response.Result
			break
		}
	}
	var body struct {
		GenesisHash     string `json:"genesis_hash"`
		RawTransactions []struct {
			Request  struct{ Params []string }
			Response struct{ Result string }
		} `json:"raw_transactions"`
	}
	load("body", &body)
	transactions := map[string]string{}
	for _, r := range body.RawTransactions {
		transactions[r.Request.Params[0]] = r.Response.Result
	}
	for _, tx := range v.Block.Transactions {
		proof.RawTransactions = append(proof.RawTransactions, transactions[tx])
	}
	proof.Receipts = v.Receipts
	header, err := VerifyChainHeaderJSON(ctx, v.ChainKey, proof.Header)
	if err != nil {
		t.Fatal(err)
	}
	semantics, err := ResolveNativeSemantics(v.ChainKey, header)
	if err != nil {
		t.Fatal(err)
	}
	chain := config.Chain{Key: v.ChainKey, Genesis: body.GenesisHash, BlockIntegrity: ChainNativeCommitmentsV2,
		NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
	legacy := chain
	// Pin a test admitted pool scope to one real Swap-emitting receipt address.
	// Current contract identity at pinned mainnet pools is checked by the owner.
	pool := config.PoolConfig{ID: "0x52f3eabf77ce5d3c9d96d9d298153936b0e2fd44", Protocol: "pancake-v2", Token0: "token0", Token1: "token1",
		V2: &config.V2PoolConfig{ExpectedFactory: "0x" + strings.Repeat("11", 20), ExpectedCodeSHA256: strings.Repeat("ab", 32)}}
	chain.Pools = []config.PoolConfig{pool}
	limits, err := NativeRuntimeLimits(chain)
	if err != nil {
		t.Fatal(err)
	}
	evidence := NativeReceiptEvidence{SchemaVersion: 1, ChainKey: v.ChainKey, GenesisHash: body.GenesisHash, NativeSemanticsID: semantics.ID, Kind: "own-header-receipts", Block: &proof}
	coverage, err := VerifyNativeReceiptEvidence(ctx, v.ChainKey, v.Block, evidence, limits)
	if err != nil {
		t.Fatal(err)
	}
	ref, err := blockReference(v.ChainKey, coverage.Block)
	if err != nil {
		t.Fatal(err)
	}
	canonical := coverage.Block
	canonical.NativeCoverage = &coverage
	version := swapDecoderVersion(chain)
	facts, err := decodeSwapFacts(ref, coverage.CanonicalReceipts, chain.Pools, version)
	if err != nil || len(facts) != 3 || version != domain.NativeBNBV2SwapDecoder {
		t.Fatal(facts, err)
	}
	if first := facts[0]; first.LogIndex != 8 || first.TxHash != "0xe1ef2c67ff1c0a87449213bdf18352ecc6cf9972f6b864ac3a053fcafa9ca421" ||
		first.Amount1In != "134697929870442109124" || first.Amount0Out != "7913541731018265147" {
		t.Fatal(first)
	}
	archive, err := buildNativeHistoryEvidence(chain, canonical, domain.FactBatch{SchemaVersion: 1, DecoderVersion: version,
		Blocks: []domain.FactBlock{{Block: ref, Swaps: facts}}}, nil)
	if err != nil {
		t.Fatal(err)
	}
	old, err := buildNativeHistoryEvidence(legacy, canonical, domain.FactBatch{SchemaVersion: 1, DecoderVersion: domain.LegacyV2SwapDecoder,
		Blocks: []domain.FactBlock{{Block: ref, Swaps: []domain.SwapFact{}}}}, nil)
	if err != nil || old.ScopeDatasetID == archive.ScopeDatasetID || old.DatasetID == archive.DatasetID {
		t.Fatal("old scope reused", err)
	}
	for _, record := range []NativeHistoryEvidence{old, archive} {
		batch, _, err := VerifyNativeHistoryEvidence(ctx, chain, record)
		if err != nil || batch.DecoderVersion != record.Facts.DecoderVersion || len(batch.Blocks[0].Swaps) != len(record.Facts.Blocks[0].Swaps) {
			t.Fatal(batch, err)
		}
	}
	changed := chain
	changedPool, changedIdentity := pool, *pool.V2
	changedIdentity.ExpectedCodeSHA256 = strings.Repeat("cd", 32)
	changedPool.V2 = &changedIdentity
	changed.Pools = []config.PoolConfig{changedPool}
	if _, _, err = VerifyNativeHistoryEvidence(ctx, changed, archive); err == nil || err.Error() != "HISTORY_NATIVE_SCOPE_MISMATCH" {
		t.Fatal("accepted an archive for a different pool runtime", err)
	}
	archive.Facts.Blocks[0].Swaps = old.Facts.Blocks[0].Swaps
	if _, _, err = VerifyNativeHistoryEvidence(ctx, chain, archive); err == nil || err.Error() != "HISTORY_NATIVE_FACT_PROJECTION_MISMATCH" {
		t.Fatal("accepted an empty projection as Pancake coverage", err)
	}
}

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
