package config

import (
	"encoding/json"
	"os"
	"testing"
)

func TestWeightedConfigExplicitTokenPolicyAndModelIsolation(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../test/contracts/weighted-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Configs []PoolConfig `json:"configs"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Chains[0].Pools = data.Configs
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	for _, mutate := range []func(*PoolConfig){func(p *PoolConfig) { p.Weighted = nil }, func(p *PoolConfig) { p.Weighted.TokenPolicy = "" }, func(p *PoolConfig) { p.Weighted.ExpectedToken0CodeSHA256 = "" }, func(p *PoolConfig) { p.Protocol = "uniswap-v2" }, func(p *PoolConfig) { p.Stable = &StablePoolConfig{} }, func(p *PoolConfig) { p.Weighted.PoolID = p.ID + "000000000000000000000001" }, func(p *PoolConfig) { p.FeeNumerator = 0 }} {
		var fresh PoolConfig
		single, _ := json.Marshal(data.Configs[0])
		json.Unmarshal(single, &fresh)
		mutate(&fresh)
		cfg.Config.Chains[0].Pools = []PoolConfig{fresh}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatalf("invalid weighted config accepted: %+v", fresh)
		}
	}
}

func TestWeightedStaticConfigExplicitTokenPolicyAndModelIsolation(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../test/contracts/weighted-static-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Configs []PoolConfig `json:"configs"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Chains[0].Pools = data.Configs
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	for _, mutate := range []func(*PoolConfig){func(p *PoolConfig) { p.Weighted = nil }, func(p *PoolConfig) { p.Weighted.TokenPolicy = "" }, func(p *PoolConfig) { p.Weighted.ExpectedToken0CodeSHA256 = "" }, func(p *PoolConfig) { p.Protocol = "uniswap-v2" }, func(p *PoolConfig) { p.Stable = &StablePoolConfig{} }, func(p *PoolConfig) { p.Weighted.PoolID = p.ID + "000000000000000000000001" }, func(p *PoolConfig) { p.FeeNumerator = 0 }} {
		var fresh PoolConfig
		single, _ := json.Marshal(data.Configs[0])
		json.Unmarshal(single, &fresh)
		mutate(&fresh)
		cfg.Config.Chains[0].Pools = []PoolConfig{fresh}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatalf("invalid weighted config accepted: %+v", fresh)
		}
	}
}
