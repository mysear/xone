package config

import (
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func completeV2Config(t *testing.T) Config {
	t.Helper()
	s, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	s.Config.Chains[0].Pools = []PoolConfig{{ID: "0x" + strings.Repeat("1", 40), Protocol: domain.V2CompleteProtocol, Token0: "0x" + strings.Repeat("2", 40), Token1: "0x" + strings.Repeat("3", 40), FeeNumerator: 3, FeeDenominator: 1000, V2: &V2PoolConfig{ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedCodeSHA256: domain.V2CompletePairCodeSHA256}}}
	return s.Config
}
func TestV2CompleteConfigPinsExactLayoutAndCanonicalPoolIdentity(t *testing.T) {
	c := completeV2Config(t)
	if e := c.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	for _, mutate := range []func(*PoolConfig){func(p *PoolConfig) { p.V2.ExpectedCodeSHA256 = strings.Repeat("a", 64) }, func(p *PoolConfig) { p.V2 = nil }, func(p *PoolConfig) { p.V2.ExpectedFactory = "0x" + strings.Repeat("0", 40) }, func(p *PoolConfig) { p.Token0 = "0x" + strings.Repeat("0", 40) }, func(p *PoolConfig) { p.Token0, p.Token1 = p.Token1, p.Token0 }, func(p *PoolConfig) { p.ProgramID = "unexpected-solana-program" }, func(p *PoolConfig) { p.Accounts = []string{"unused"} }, func(p *PoolConfig) { p.FeeNumerator++ }} {
		q := completeV2Config(t)
		mutate(&q.Chains[0].Pools[0])
		if q.Validate("chain-parser-go") == nil {
			t.Fatal("invalid full V2 config admitted", q.Chains[0].Pools[0])
		}
	}
}
