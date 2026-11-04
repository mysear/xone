package config

import (
	"encoding/json"
	"os"
	"testing"
)

func TestCurveNGConfigRestrictsIdentityTokensAndStandardRuntimeLayout(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../test/contracts/curve-ng-service-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Cases []struct {
			Config PoolConfig `json:"config"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	original := data.Cases[0].Config
	for i, mutate := range []func(*PoolConfig){func(p *PoolConfig) {}, func(p *PoolConfig) {
		p.MultiAsset.ExpectedImplementation = p.ID
		p.MultiAsset.ExpectedImplementationCodeSHA256 = p.MultiAsset.ExpectedCodeSHA256
	}, func(p *PoolConfig) { p.MultiAsset.TokenPolicy = "" }, func(p *PoolConfig) { p.MultiAsset.Tokens = p.MultiAsset.Tokens[:1] }, func(p *PoolConfig) {
		p.MultiAsset.ExpectedVault = p.ID
		p.MultiAsset.ExpectedVaultCodeSHA256 = p.MultiAsset.ExpectedCodeSHA256
	}, func(p *PoolConfig) { p.FeeNumerator = 5000000001 }, func(p *PoolConfig) { p.Protocol = "curve-plain3-basic" }, func(p *PoolConfig) { p.MultiAsset.Tokens[1].ExpectedCodeSHA256 = "" }} {
		var p PoolConfig
		b, _ := json.Marshal(original)
		json.Unmarshal(b, &p)
		mutate(&p)
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		err := cfg.Config.Validate("chain-parser-go")
		if i == 0 && err != nil {
			t.Fatal(err)
		}
		if i > 0 && err == nil {
			t.Fatal("unsupported NG config accepted", i)
		}
	}
}
