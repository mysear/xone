package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestOrcaAdaptiveDynamicConfigBoundaries(t *testing.T) {
	c := solanaProfileRouterConfig(t).Config
	raw, err := os.ReadFile("../../test/contracts/orca-adaptive-dynamic-wire.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct{ Snapshot domain.Snapshot }
	if err := json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	p := fixture.Snapshot.Pools[0]
	s := p.OrcaWhirlpool
	ch := c.Chains[len(c.Chains)-1]
	ch.Key, ch.Genesis, ch.Owner, ch.Confirmation = p.ChainKey, "localnet", "chain-parser-rust", "finalized"
	ch.Pools = []PoolConfig{{
		ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1,
		FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, ProgramID: s.ProgramID,
		Accounts: []string{p.PoolID, s.Vault0, s.Vault1, s.WhirlpoolsConfig, p.Token0, p.Token1},
		OrcaWhirlpool: &OrcaWhirlpoolConfig{
			ExpectedProgramdata: s.Programdata, ExpectedProgramCodeSHA256: s.ProgramCodeSHA256,
			ExpectedDeployedSlot: s.DeployedSlot, ExpectedTokenProgramCodeSHA256: s.TokenProgramCodeSHA256,
			MaxSteps: s.MaxSteps,
		},
	}}
	c.Chains = []Chain{ch}
	for _, starts := range [][]int32{{-5632, 0, 5632}, {-5632, 0, 5632, 11264}, {-5632, 0, 5632, 11264, 16896, 22528, 28160}} {
		c.Chains[0].Pools[0].OrcaWhirlpool.TickArrayStarts = starts
		if e := c.Validate("dex-router"); e != nil {
			t.Fatal(starts, e)
		}
	}
	for _, starts := range [][]int32{{-5632, 0}, {-5632, 0, 5632, 16896}, {-5632, 0, 5720}, {-5631, 1, 5633},
		{-5632, 0, 5632, 11264, 16896, 22528, 28160, 33792}} {
		c.Chains[0].Pools[0].OrcaWhirlpool.TickArrayStarts = starts
		if c.Validate("dex-router") == nil {
			t.Fatal("invalid envelope accepted", starts)
		}
	}
}
