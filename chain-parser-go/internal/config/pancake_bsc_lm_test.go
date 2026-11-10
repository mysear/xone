package config

import (
	"encoding/json"
	"os"
	"testing"
)

func TestPancakeBsc4ConfigRequiresThreePinnedDistinctPredecessors(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/pancake-bsc4-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Cases []struct {
			Config PoolConfig `json:"config"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	for _, c := range f.Cases {
		if !c.Config.validatePancakeBscLm("evm:31337") {
			t.Fatal("valid source rejected")
		}
	}
	for mode := 0; mode < 6; mode++ {
		p := f.Cases[2].Config
		wrapper := *p.PancakeBscLm
		p.PancakeBscLm = &wrapper
		wrapper.Predecessors = append([]BscPredecessorConfig{}, wrapper.Predecessors...)
		switch mode {
		case 0:
			p.PancakeBscLm = nil
		case 1:
			wrapper.Predecessors = wrapper.Predecessors[:2]
		case 2:
			wrapper.Predecessors[0].Address = p.Token0
		case 3:
			wrapper.Predecessors[1].Address = wrapper.Predecessors[0].Address
		case 4:
			wrapper.Predecessors[0].CodeSHA256 = ""
		default:
			p.Protocol = "pancake-v3-lm-context"
		}
		if p.validatePancakeBscLm("evm:31337") {
			t.Fatal("invalid source accepted", mode)
		}
	}
}
