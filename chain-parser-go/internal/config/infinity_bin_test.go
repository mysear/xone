package config

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestInfinityBinConfigIdentityPolicyAndBudgets(t *testing.T) {
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile("../../test/contracts/infinity-bin-wire.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Snapshot domain.Snapshot `json:"snapshot"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	p := fixture.Snapshot.Pools[0]
	v := p.InfinityBin
	original := PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, InfinityBin: &InfinityBinConfig{Manager: v.Manager, ExpectedManagerCodeSHA256: v.ManagerCodeSHA256, Vault: v.Vault, ExpectedVaultCodeSHA256: v.VaultCodeSHA256, PoolID: v.PoolID, BinStep: v.BinStep, LPFee: v.LPFee, ExpectedToken0CodeSHA256: v.Token0CodeSHA256, ExpectedToken1CodeSHA256: v.Token1CodeSHA256, TokenPolicy: "standard", MaxBins: v.MaxBins, MaxTreeWords: v.MaxTreeWords, MaxSteps: v.MaxSteps}}
	for name, mutate := range map[string]func(*PoolConfig){
		"valid": func(p *PoolConfig) {}, "missing": func(p *PoolConfig) { p.InfinityBin = nil }, "legacy": func(p *PoolConfig) { p.Protocol = "uniswap-v2" }, "other model": func(p *PoolConfig) { p.V3 = &V3PoolConfig{} }, "program": func(p *PoolConfig) { p.ProgramID = "unexpected" }, "accounts": func(p *PoolConfig) { p.Accounts = []string{"unexpected"} }, "physical": func(p *PoolConfig) { p.ID = p.InfinityBin.PoolID }, "tokens reversed": func(p *PoolConfig) { p.Token0, p.Token1 = p.Token1, p.Token0 }, "policy": func(p *PoolConfig) { p.InfinityBin.TokenPolicy = "" }, "fee": func(p *PoolConfig) { p.FeeNumerator++ }, "denom": func(p *PoolConfig) { p.FeeDenominator = 10000 }, "manager code": func(p *PoolConfig) { p.InfinityBin.ExpectedManagerCodeSHA256 = "" }, "vault code": func(p *PoolConfig) { p.InfinityBin.ExpectedVaultCodeSHA256 = "" }, "token code": func(p *PoolConfig) { p.InfinityBin.ExpectedToken0CodeSHA256 = strings.Repeat("A", 64) }, "native token": func(p *PoolConfig) { p.Token0 = domain.SingletonZeroAddress }, "vault asset": func(p *PoolConfig) { p.InfinityBin.Vault = p.Token0 }, "manager asset": func(p *PoolConfig) { p.InfinityBin.Manager = p.Token1 }, "step": func(p *PoolConfig) { p.InfinityBin.BinStep = 101 }, "lp fee": func(p *PoolConfig) { p.InfinityBin.LPFee = 100001; p.FeeNumerator = 100001 }, "bins zero": func(p *PoolConfig) { p.InfinityBin.MaxBins = 0 }, "bins limit": func(p *PoolConfig) { p.InfinityBin.MaxBins = 1025 }, "words zero": func(p *PoolConfig) { p.InfinityBin.MaxTreeWords = 0 }, "words limit": func(p *PoolConfig) { p.InfinityBin.MaxTreeWords = 1025 }, "steps zero": func(p *PoolConfig) { p.InfinityBin.MaxSteps = 0 }, "steps limit": func(p *PoolConfig) { p.InfinityBin.MaxSteps = 4097 },
	} {
		t.Run(name, func(t *testing.T) {
			b, _ := json.Marshal(original)
			var p PoolConfig
			if err := json.Unmarshal(b, &p); err != nil {
				t.Fatal(err)
			}
			mutate(&p)
			cfg.Config.Chains[0].Pools = []PoolConfig{p}
			err := cfg.Config.Validate("chain-parser-go")
			if name == "valid" && err != nil {
				t.Fatal(err)
			}
			if name != "valid" && err == nil {
				t.Fatal("invalid infinity bin config admitted")
			}
		})
	}
}
