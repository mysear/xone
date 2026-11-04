package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestOrcaConfigRequiresSolanaStaticArrayProof(t *testing.T) {
	c := solanaProfileRouterConfig(t)
	b, e := os.ReadFile("../../test/contracts/orca-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p domain.Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	s := p.OrcaWhirlpool
	cfg := PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: 1000000, ProgramID: s.ProgramID, Accounts: []string{p.PoolID, s.Vault0, s.Vault1, s.WhirlpoolsConfig, p.Token0, p.Token1}, OrcaWhirlpool: &OrcaWhirlpoolConfig{ExpectedProgramdata: s.Programdata, ExpectedProgramCodeSHA256: s.ProgramCodeSHA256, ExpectedDeployedSlot: s.DeployedSlot, ExpectedTokenProgramCodeSHA256: s.TokenProgramCodeSHA256, TickArrayStarts: []int32{-5632, 0, 5632}, MaxSteps: 512}}
	ch := c.Config.Chains[len(c.Config.Chains)-1]
	ch.Key = "solana:localnet"
	ch.Genesis = "localnet"
	ch.Owner = "chain-parser-rust"
	ch.Confirmation = "finalized"
	ch.Pools = []PoolConfig{cfg}
	c.Config.Chains = []Chain{ch}
	if e = c.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	c.Config.Chains[0].Confirmation = "confirmed"
	if c.Config.Validate("dex-router") == nil {
		t.Fatal("unfinalized Orca accepted")
	}
	c.Config.Chains[0].Confirmation = "finalized"
	c.Config.Chains[0].Pools[0].OrcaWhirlpool.TickArrayStarts = []int32{0, 0, 5632}
	if c.Config.Validate("dex-router") == nil {
		t.Fatal("duplicate arrays accepted")
	}
}
