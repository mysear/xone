package config

import (
	"encoding/json"
	"os"
	"testing"
)

func TestPancakeLmConfigRequiresPinnedRolesAndFiniteLimits(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/pancake-lm-state.json")
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
		if !c.Config.validatePancakeLm("evm:31337") {
			t.Fatal("valid config rejected")
		}
	}
	original := f.Cases[0].Config
	for mode := 0; mode < 13; mode++ {
		p := original
		s := *p.PancakeLm
		p.PancakeLm = &s
		switch mode {
		case 0:
			p.PancakeLm = nil
		case 1:
			s.ExpectedMasterChefCodeSHA256 = ""
		case 2:
			s.ExpectedLmPoolCodeSHA256 = ""
		case 3:
			s.ExpectedFactoryCodeSHA256 = ""
		case 4:
			s.TokenPolicy = "taxed"
		case 5:
			s.ExpectedLmPool = p.ID
		case 6:
			s.MaxObservations = 0
		case 7:
			s.MaxObservations = 4097
		case 8:
			s.MaxSteps = 0
		case 9:
			s.ExpectedCake = "0x0000000000000000000000000000000000000000"
		case 10:
			p.Protocol = "pancake-v3"
		case 11:
			p.Accounts = append([]string{}, p.Token0)
		default:
			s.ExpectedToken0CodeSHA256 = ""
		}
		if p.validatePancakeLm("evm:31337") {
			t.Fatal("bad config accepted", mode)
		}
	}
}
