package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestMultiAssetConfigRequiresFullOrderedTokenProof(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../test/contracts/multi-asset-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var d struct {
		Pools []domain.Pool `json:"pools"`
	}
	if e = json.Unmarshal(raw, &d); e != nil {
		t.Fatal(e)
	}
	p := d.Pools[0]
	s := p.MultiAsset
	v := &MultiAssetConfig{ExpectedCodeSHA256: s.CodeSHA256, ExpectedVault: s.Vault, ExpectedVaultCodeSHA256: s.VaultCodeSHA256, PoolID: s.PoolID, TokenPolicy: "standard-no-bpt"}
	for _, a := range s.Assets {
		v.Tokens = append(v.Tokens, MultiTokenConfig{Token: a.Token, ExpectedCodeSHA256: a.CodeSHA256})
	}
	pc := PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, MultiAsset: v}
	cfg.Config.Chains[0].Pools = []PoolConfig{pc}
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	v.Tokens[2].ExpectedCodeSHA256 = ""
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("unproven third token admitted")
	}
	v.Tokens[2].ExpectedCodeSHA256 = s.Assets[2].CodeSHA256
	v.TokenPolicy = ""
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("token policy omitted")
	}
}
