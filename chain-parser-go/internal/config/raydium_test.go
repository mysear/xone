package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestRaydiumConfigPreservesSolanaOwnerAndFinalizedVersionPin(t *testing.T) {
	cfg := solanaProfileRouterConfig(t)
	raw, e := os.ReadFile("../../test/contracts/raydium-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var pool domain.Pool
	if e = json.Unmarshal(raw, &pool); e != nil {
		t.Fatal(e)
	}
	s := pool.RaydiumCPMM
	p := PoolConfig{ID: pool.PoolID, Protocol: pool.Protocol, Token0: pool.Token0, Token1: pool.Token1, FeeNumerator: pool.FeeNumerator, FeeDenominator: 1000000, ProgramID: s.ProgramID, Accounts: []string{pool.PoolID, s.Vault0, s.Vault1, s.AmmConfig, pool.Token0, pool.Token1}, RaydiumCPMM: &RaydiumCPMMConfig{ExpectedProgramdata: s.Programdata, ExpectedProgramCodeSHA256: s.ProgramCodeSHA256, ExpectedDeployedSlot: s.DeployedSlot, ExpectedTokenProgramCodeSHA256: s.TokenProgramCodeSHA256}}
	ch := cfg.Config.Chains[len(cfg.Config.Chains)-1]
	ch.Key = "solana:localnet"
	ch.Genesis = "localnet"
	ch.Owner = "chain-parser-rust"
	ch.Confirmation = "finalized"
	ch.Pools = []PoolConfig{p}
	cfg.Config.Chains = []Chain{ch}
	if e = cfg.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Chains[0].Confirmation = "confirmed"
	if cfg.Config.Validate("dex-router") == nil {
		t.Fatal("non-finalized newprofile accepted")
	}
	cfg.Config.Chains[0].Confirmation = "finalized"
	cfg.Config.Chains[0].Pools[0].RaydiumCPMM.ExpectedProgramCodeSHA256 = "aa"
	if cfg.Config.Validate("dex-router") == nil {
		t.Fatal("shortcodehash accepted")
	}
}
