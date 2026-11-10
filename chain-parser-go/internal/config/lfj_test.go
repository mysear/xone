package config

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestLfjConfigIsCompleteBoundedAndIndependent(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/lfj-v21-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Cases []struct {
			Config json.RawMessage `json:"config"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	if len(data.Cases) != 96 {
		t.Fatal("incomplete LFJ config corpus")
	}
	for _, c := range data.Cases {
		var p PoolConfig
		if e = json.Unmarshal(c.Config, &p); e != nil {
			t.Fatal(e)
		}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if e = cfg.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(e)
		}
	}
	for bad := 0; bad < 15; bad++ {
		var p PoolConfig
		if e = json.Unmarshal(data.Cases[0].Config, &p); e != nil {
			t.Fatal(e)
		}
		switch bad {
		case 0:
			p.Lfj = nil
		case 1:
			p.Lfj.TokenPolicy = "unknown-extensions"
		case 2:
			p.Lfj.BinStep = 0
		case 3:
			p.Lfj.MaxBins = 0
		case 4:
			p.Lfj.MaxSteps = 0
		case 5:
			p.Lfj.ExpectedImplementation = p.ID
		case 6:
			p.Lfj.ExpectedToken0CodeSHA256 = ""
		case 7:
			p.Protocol = "uniswap-v2"
		case 8:
			p.V2 = &V2PoolConfig{ExpectedFactory: "0x" + strings.Repeat("1", 40), ExpectedCodeSHA256: strings.Repeat("a", 64)}
		case 9:
			p.Lfj.MaxBins = 4097
		case 10:
			p.Lfj.MaxSteps = 8193
		case 11:
			p.FeeNumerator = 1
		case 12:
			p.FeeDenominator = 1000000
		case 13:
			p.Lfj.ExpectedFactory = p.Token0
		case 14:
			p.Accounts = []string{p.Token0}
		}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal("invalid LFJ config accepted", bad)
		}
	}
}
