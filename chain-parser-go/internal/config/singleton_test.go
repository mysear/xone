package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestSingletonConfigBindsManagerAndCompleteKey(t *testing.T) {
	loaded, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	b, e := os.ReadFile("../../test/contracts/singleton-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Pools []domain.Pool `json:"pools"`
	}
	if e = json.Unmarshal(b, &f); e != nil {
		t.Fatal(e)
	}
	for _, p := range f.Pools {
		s := p.SingletonCL
		cfg := PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, SingletonCL: &SingletonCLPoolConfig{Manager: s.Manager, ExpectedManagerCodeSHA256: s.ManagerCodeSHA256, PoolID: s.PoolID, LPFee: s.LPFee, TickSpacing: s.Core.TickSpacing, Parameters: s.Parameters, TickWordRadius: 1, MaxInitializedTicks: 32, MaxSteps: 512}}
		loaded.Config.Chains[0].Pools = []PoolConfig{cfg}
		if e = loaded.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(p.Protocol, e)
		}
		cfg.SingletonCL.PoolID = "0xdead"
		loaded.Config.Chains[0].Pools[0] = cfg
		if loaded.Config.Validate("chain-parser-go") == nil {
			t.Fatal("bad key accepted")
		}
	}
}
