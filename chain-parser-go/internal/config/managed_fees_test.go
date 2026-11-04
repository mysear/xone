package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestManagedFeeConfigUsesIndependentVersionAndBoundedNativeFees(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../test/contracts/managed-fees-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot domain.Snapshot `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	pool := f.Snapshot.Pools[0]
	s := pool.MultiAsset
	v := &MultiAssetConfig{ExpectedCodeSHA256: s.CodeSHA256, ExpectedVault: s.Vault, ExpectedVaultCodeSHA256: s.VaultCodeSHA256, PoolID: s.PoolID, TokenPolicy: "standard-no-bpt"}
	for _, a := range s.Assets {
		v.Tokens = append(v.Tokens, MultiTokenConfig{Token: a.Token, ExpectedCodeSHA256: a.CodeSHA256})
	}
	original := PoolConfig{ID: pool.PoolID, Protocol: pool.Protocol, Token0: pool.Token0, Token1: pool.Token1, FeeNumerator: 800000000000000000, FeeDenominator: pool.FeeDenominator, MultiAsset: v}
	for i, mutate := range []func(*PoolConfig){func(p *PoolConfig) {}, func(p *PoolConfig) { p.FeeNumerator++ }, func(p *PoolConfig) { p.Protocol = "balancer-v2-managed-snapshot" }, func(p *PoolConfig) { p.MultiAsset.TokenPolicy = "standard-with-own-bpt" }, func(p *PoolConfig) {
		p.MultiAsset.ExpectedImplementation = p.ID
		p.MultiAsset.ExpectedImplementationCodeSHA256 = s.CodeSHA256
	}, func(p *PoolConfig) { p.MultiAsset.Tokens = p.MultiAsset.Tokens[:1] }, func(p *PoolConfig) { p.MultiAsset.Tokens[2].ExpectedCodeSHA256 = "" }, func(p *PoolConfig) { p.MultiAsset.PoolID = p.ID + "0000" + "00000000000000000001" }} {
		var p PoolConfig
		b, _ := json.Marshal(original)
		if e = json.Unmarshal(b, &p); e != nil {
			t.Fatal(e)
		}
		mutate(&p)
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		err := cfg.Config.Validate("chain-parser-go")
		if err == nil {
			t.Fatal("reference-only Managed version reached normal config", i)
		}
	}
}
