package domain

import (
	"encoding/json"
	"math/big"
	"os"
	"strings"
	"testing"
)

func singletonTestPools(t *testing.T) []Pool {
	t.Helper()
	b, e := os.ReadFile("../../test/contracts/singleton-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Pools []Pool `json:"pools"`
	}
	if e = json.Unmarshal(b, &f); e != nil {
		t.Fatal(e)
	}
	return f.Pools
}
func TestSingletonIdentityAndTypedWire(t *testing.T) {
	for _, p := range singletonTestPools(t) {
		if e := p.Validate(); e != nil {
			t.Fatal(e)
		}
		s := p.SingletonCL
		id, e := SingletonPoolID(p.Protocol, s.Manager, p.Token0, p.Token1, s.LPFee, s.Core.TickSpacing, s.Parameters)
		if e != nil || id != s.PoolID {
			t.Fatal(id, e)
		}
		w, e := SingletonCLToProto(s)
		if e != nil {
			t.Fatal(e)
		}
		back, e := SingletonCLFromProto(w)
		if e != nil || back.PoolID != s.PoolID || back.Core.Ticks[2].LiquidityNet != s.Core.Ticks[2].LiquidityNet {
			t.Fatal(back, e)
		}
	}
}
func TestSingletonRejectsUnknownAccountingAndIdentity(t *testing.T) {
	for _, mode := range []string{"hook", "dynamic-fee", "wrong-poolid", "physical-alias", "manager", "native", "unordered", "protocol-fee", "missing-tick", "v3-tag", "parameters"} {
		t.Run(mode, func(t *testing.T) {
			for _, base := range singletonTestPools(t) {
				p := base.Clone()
				s := p.SingletonCL
				switch mode {
				case "hook":
					s.Hooks = "0x" + strings.Repeat("1", 40)
				case "dynamic-fee":
					s.LPFee = 0x800000
				case "wrong-poolid":
					s.PoolID = "0x" + strings.Repeat("a", 64)
				case "physical-alias":
					p.PoolID = strings.ToUpper(p.PoolID)
				case "manager":
					s.Manager = "0x" + strings.Repeat("5", 40)
				case "native":
					p.Token0 = "0x" + strings.Repeat("0", 40)
				case "unordered":
					p.Token0, p.Token1 = p.Token1, p.Token0
				case "protocol-fee":
					s.ProtocolFee0 = 4001
				case "missing-tick":
					s.Core.Ticks = s.Core.Ticks[:1]
				case "v3-tag":
					p.Protocol = "uniswap-v3"
				case "parameters":
					s.Parameters = "0x" + strings.Repeat("f", 64)
				}
				if p.Validate() == nil {
					t.Fatal("invalid singleton accepted", mode, p.Protocol)
				}
			}
		})
	}
}

func TestBothSingletonCoresFloorNegativeMinimumForGrossLiquidity(t *testing.T) {
	for _, p := range singletonTestPools(t) {
		if p.Protocol != "pancake-infinity-cl" {
			continue
		}
		c := &p.SingletonCL.Core
		n := MaxV3Tick/c.TickSpacing - MinV3Tick/c.TickSpacing + 1
		max := new(big.Int).Quo(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)), big.NewInt(int64(n)))
		c.Ticks[0].LiquidityGross = Amount(max.String())
		if p.Validate() == nil {
			t.Fatal("Infinity accepted V3 truncation max gross")
		}
	}
}

func TestSingletonWireRequiresRawPoolIDDespiteEmptyV4Parameters(t *testing.T) {
	p := singletonTestPools(t)[0]
	p.SingletonCL.PoolID = ""
	if _, e := SingletonCLToProto(p.SingletonCL); e == nil {
		t.Fatal("empty poolId produced unreadable public wire")
	}
}
