package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func orcaAdaptiveConfig(t *testing.T) Config {
	t.Helper()
	c := solanaProfileRouterConfig(t).Config
	b, err := os.ReadFile("../../test/contracts/orca-state.json")
	if err != nil {
		t.Fatal(err)
	}
	var p domain.Pool
	if err = json.Unmarshal(b, &p); err != nil {
		t.Fatal(err)
	}
	s := p.OrcaWhirlpool
	ch := c.Chains[len(c.Chains)-1]
	ch.Key, ch.Genesis, ch.Owner, ch.Confirmation = "solana:localnet", "localnet", "chain-parser-rust", "finalized"
	ch.Pools = []PoolConfig{{ID: p.PoolID, Protocol: "orca-whirlpools-adaptive", Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: 1000000, ProgramID: s.ProgramID, Accounts: []string{p.PoolID, s.Vault0, s.Vault1, s.WhirlpoolsConfig, p.Token0, p.Token1}, OrcaWhirlpool: &OrcaWhirlpoolConfig{ExpectedProgramdata: s.Programdata, ExpectedProgramCodeSHA256: s.ProgramCodeSHA256, ExpectedDeployedSlot: s.DeployedSlot, ExpectedTokenProgramCodeSHA256: s.TokenProgramCodeSHA256, TickArrayStarts: []int32{-5632, 0, 5632}, MaxSteps: 512}}}
	c.Chains = []Chain{ch}
	return c
}

func TestOrcaAdaptiveConfigFixedThreeArrayEnvelope(t *testing.T) {
	c := orcaAdaptiveConfig(t)
	if err := c.Validate("dex-router"); err != nil {
		t.Fatal(err)
	}
	for name, starts := range map[string][]int32{"two": {-5632, 0}, "four": {-5632, 0, 5632, 11264}, "gap": {-5632, 0, 11264}, "unaligned": {-5631, 1, 5633}, "non-tick-stride": {-1, 0, 1}, "wide-spacing": {-5767168, 0, 5767168}, "negative-overflow": {-2147483648, 0, 2147483647}, "out-of-range": {563200, 568832, 574464}} {
		t.Run(name, func(t *testing.T) {
			c := orcaAdaptiveConfig(t)
			c.Chains[0].Pools[0].OrcaWhirlpool.TickArrayStarts = starts
			if c.Validate("dex-router") == nil {
				t.Fatal("invalid adaptive array envelope accepted")
			}
		})
	}
	// Existing Orca profiles retain their 3..7 increasing-start contract.
	for _, profile := range []string{"orca-whirlpools-static", "orca-whirlpools-rewards"} {
		c := orcaAdaptiveConfig(t)
		c.Chains[0].Pools[0].Protocol = profile
		c.Chains[0].Pools[0].OrcaWhirlpool.TickArrayStarts = []int32{-5632, 0, 5632, 11264}
		if err := c.Validate("dex-router"); err != nil {
			t.Fatal(profile, err)
		}
	}
}
