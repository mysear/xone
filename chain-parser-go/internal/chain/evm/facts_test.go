package evm

import (
	"fmt"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestSwapFactsUseFullReceiptPositionAndRawIntegerAmounts(t *testing.T) {
	b := domain.BlockRef{ChainKey: "evm:31337", Number: 100, Hash: "0x" + strings.Repeat("1", 64), ParentHash: "0x" + strings.Repeat("0", 64), TimestampMS: 1000}
	pool := config.PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2", Token0: "A", Token1: "B"}
	log := Log{Address: pool.ID, BlockHash: b.Hash, TxHash: "0x" + strings.Repeat("a", 64), TxIndex: "0x0", Index: "0x3", Topics: []string{V2SwapTopic, "0x" + strings.Repeat("0", 64), "0x" + strings.Repeat("0", 64)}, Data: "0x" + fmt.Sprintf("%064x%064x%064x%064x", 10, 0, 0, 20)}
	facts, e := DecodeSwapFacts(b, []Receipt{{Logs: []Log{log}}}, []config.PoolConfig{pool})
	if e != nil || len(facts) != 1 {
		t.Fatal(facts, e)
	}
	if facts[0].EventID != domain.SwapEventID(b, 0, 3) || facts[0].Amount0In != "10" || facts[0].Amount1Out != "20" {
		t.Fatal(facts)
	}
	log.Data = "0x01"
	if _, e = DecodeSwapFacts(b, []Receipt{{Logs: []Log{log}}}, []config.PoolConfig{pool}); e == nil {
		t.Fatal("malformed swap silently skipped")
	}
}
