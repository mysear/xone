package domain

import (
	"strings"
	"testing"
)

func factBlock(n uint64, h, parent string) BlockRef {
	return BlockRef{ChainKey: "evm:31337", Number: n, Hash: "0x" + strings.Repeat(h, 64), ParentHash: "0x" + strings.Repeat(parent, 64), TimestampMS: 1000}
}
func TestNormalizedFactIdentityAndBranchValidation(t *testing.T) {
	b := factBlock(101, "2", "1")
	f := SwapFact{EventID: SwapEventID(b, 0, 3), TxHash: "0x" + strings.Repeat("a", 64), TxIndex: 0, LogIndex: 3, PoolID: "pool", Token0: "A", Token1: "B", Amount0In: "10", Amount1In: "0", Amount0Out: "0", Amount1Out: "20"}
	batch := FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []FactBlock{{Block: b, Swaps: []SwapFact{f}}}}
	if e := batch.Validate("BLOCK_COMMIT", b); e != nil {
		t.Fatal(e)
	}
	broken := batch
	broken.Blocks = []FactBlock{{Block: b, Swaps: []SwapFact{f, f}}}
	if e := broken.Validate("BLOCK_COMMIT", b); e == nil {
		t.Fatal("duplicate position accepted")
	}
	f.EventID = "tx-hash-only"
	broken.Blocks[0].Swaps = []SwapFact{f}
	if e := broken.Validate("BLOCK_COMMIT", b); e == nil {
		t.Fatal("unstable identity accepted")
	}
	replacement := factBlock(101, "3", "1")
	batch.Blocks = []FactBlock{{Block: replacement, Swaps: []SwapFact{}}}
	batch.RevertedBlocks = []BlockRef{b}
	if e := batch.Validate("REORG_COMMIT", replacement); e != nil {
		t.Fatal(e)
	}
	batch.RevertedBlocks[0].ParentHash = "wrong"
	if e := batch.Validate("REORG_COMMIT", replacement); e == nil {
		t.Fatal("revert/new branch ancestor mismatch")
	}
}
