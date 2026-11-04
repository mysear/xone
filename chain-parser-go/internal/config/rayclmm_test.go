package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestRayCLMMConfigRequiresFinalizedVersionAndContiguousBitmapWindow(t *testing.T) {
	c := solanaProfileRouterConfig(t)
	b, e := os.ReadFile("../../test/contracts/rayclmm-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p domain.Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	s := p.RaydiumCLMM
	cfg := PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: 1000000, ProgramID: s.ProgramID, Accounts: []string{p.PoolID, s.Vault0, s.Vault1, s.AmmConfig, p.Token0, p.Token1, s.Observation}, RaydiumCLMM: &RaydiumCLMMConfig{ExpectedProgramdata: s.Programdata, ExpectedProgramCodeSHA256: s.ProgramCodeSHA256, ExpectedDeployedSlot: s.DeployedSlot, ExpectedTokenProgramCodeSHA256: s.TokenProgramCodeSHA256, TickArrayStarts: []int32{-3840, 0, 3840}, MaxSteps: 512}}
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
	c.Config.Chains[0].Pools[0].Protocol = "raydium-clmm-token-fee"
	if e = c.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	oldProgramData := c.Config.Chains[0].Pools[0].RaydiumCLMM.ExpectedProgramdata
	c.Config.Chains[0].Pools[0].RaydiumCLMM.ExpectedProgramdata = domain.OrcaProgramData
	if c.Config.Validate("dex-router") == nil {
		t.Fatal("foreign ProgramData accepted")
	}
	c.Config.Chains[0].Pools[0].RaydiumCLMM.ExpectedProgramdata = oldProgramData
	c.Config.Chains[0].Confirmation = "confirmed"
	if c.Config.Validate("dex-router") == nil {
		t.Fatal("unfinalized CLMM accepted")
	}
	c.Config.Chains[0].Confirmation = "finalized"
	c.Config.Chains[0].Pools[0].Protocol = "raydium-clmm-custom-seed"
	c.Config.Chains[0].Pools[0].RaydiumCLMM.TickArrayStarts = []int32{30660, 30720, 30780}
	if e = c.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	c.Config.Chains[0].Pools[0].Protocol = "raydium-clmm-dynamic"
	if e = c.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	c.Config.Chains[0].Pools[0].Protocol = "raydium-clmm-bitmap"
	if e = c.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	c.Config.Chains[0].Pools[0].Protocol = "raydium-clmm-static"
	if c.Config.Validate("dex-router") == nil {
		t.Fatal("oldprofile accepts extension range")
	}
	c.Config.Chains[0].Pools[0].Protocol = "raydium-clmm-custom-seed"
	c.Config.Chains[0].Pools[0].RaydiumCLMM.TickArrayStarts = []int32{-61500, -61440, -61380}
	if e = c.Config.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	c.Config.Chains[0].Pools[0].RaydiumCLMM.TickArrayStarts = []int32{0, 1, 3840}
	if c.Config.Validate("dex-router") == nil {
		t.Fatal("unproven window accepted")
	}
}
