package evm

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestV2CompleteOriginalSwapFactsKeepBothDonatedInputs(t *testing.T) {
	raw, e := os.ReadFile("../../../test/contracts/v2-complete.json")
	if e != nil {
		t.Fatal(e)
	}
	var bank struct {
		Cases []struct {
			Name     string      `json:"name"`
			Pool     domain.Pool `json:"pool"`
			Evidence struct {
				Receipt Receipt  `json:"receipt"`
				Header  RawBlock `json:"execution_block_header"`
			} `json:"evidence"`
			Expected struct {
				Status string `json:"status"`
				Event  *struct {
					Amount0In  domain.Amount `json:"amount0_in"`
					Amount1In  domain.Amount `json:"amount1_in"`
					Amount0Out domain.Amount `json:"amount0_out"`
					Amount1Out domain.Amount `json:"amount1_out"`
				} `json:"swap_event"`
			} `json:"expected"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &bank); e != nil {
		t.Fatal(e)
	}
	success, rejected := 0, 0
	for _, c := range bank.Cases {
		ref, e := blockReference(c.Pool.ChainKey, c.Evidence.Header)
		if e != nil {
			t.Fatal(e)
		}
		p := config.PoolConfig{ID: c.Pool.PoolID, Protocol: domain.V2CompleteProtocol, Token0: c.Pool.Token0, Token1: c.Pool.Token1}
		facts, e := DecodeSwapFacts(ref, []Receipt{c.Evidence.Receipt}, []config.PoolConfig{p})
		if e != nil {
			t.Fatal(e)
		}
		if c.Expected.Status == "revert" {
			if len(facts) != 0 {
				t.Fatal("reverted swap fabricated facts", c.Name)
			}
			rejected++
			continue
		}
		if len(facts) != 1 || c.Expected.Event == nil {
			t.Fatalf("original complete swap fact missing %s count=%d", c.Name, len(facts))
		}
		actual, expected := facts[0], c.Expected.Event
		if actual.Amount0In != expected.Amount0In || actual.Amount1In != expected.Amount1In || actual.Amount0Out != expected.Amount0Out || actual.Amount1Out != expected.Amount1Out {
			t.Fatal("raw Swap amounts changed", c.Name)
		}
		if actual.EventID != domain.SwapEventID(ref, actual.TxIndex, actual.LogIndex) {
			t.Fatal("wrong source event identity")
		}
		success++
	}
	if success != 20 || rejected != 4 {
		t.Fatal("native fact inventory", success, rejected)
	}
}
