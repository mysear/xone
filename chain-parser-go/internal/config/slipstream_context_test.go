package config

import (
	"encoding/json"
	"os"
	"testing"
)

func TestSlipstreamContextConfigRequiresKnownModulesAndBoundedOrigins(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/slipstream-context-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var file struct {
		Cases []struct {
			Config json.RawMessage `json:"config"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &file); e != nil {
		t.Fatal(e)
	}
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	if len(file.Cases) != 108 {
		t.Fatal("original config cases missing")
	}
	for _, c := range file.Cases {
		var p PoolConfig
		if e = json.Unmarshal(c.Config, &p); e != nil {
			t.Fatal(e)
		}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if e = cfg.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(e)
		}
	}
	var base PoolConfig
	for _, c := range file.Cases {
		json.Unmarshal(c.Config, &base)
		if base.SlipstreamContext.SwapFeeModule.Kind == "dynamic" {
			break
		}
	}
	encoded, _ := json.Marshal(base)
	for bad := 0; bad < 17; bad++ {
		var p PoolConfig
		json.Unmarshal(encoded, &p)
		s := p.SlipstreamContext
		switch bad {
		case 0:
			p.SlipstreamContext = nil
		case 1:
			s.ExpectedVoterCodeSHA256 = ""
		case 2:
			s.TokenPolicy = "taxed"
		case 3:
			s.Origins = nil
		case 4:
			s.Origins = append(s.Origins, s.Origins[0])
		case 5:
			s.SwapFeeModule.Kind = "unknown"
		case 6:
			s.UnstakedFeeModule.Kind = "dynamic"
		case 7:
			s.MaxObservations = 0
		case 8:
			s.MaxObservations = 4097
		case 9:
			s.MaxSteps = 0
		case 10:
			s.ExpectedImplementation = p.ID
		case 11:
			p.Protocol = "aerodrome-slipstream"
		case 12:
			p.FeeNumerator = 1
		case 13:
			s.Origins = []string{p.Token0, "0x0000000000000000000000000000000000000000"}
		case 14:
			s.SwapFeeModule.Kind = "none"
		case 15:
			s.ExpectedToken0CodeSHA256 = ""
		case 16:
			p.V3 = &V3PoolConfig{}
		}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal("unsafe contextual config accepted", bad)
		}
	}
}
