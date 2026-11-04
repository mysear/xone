package domain

import (
	"encoding/json"
	"os"
	"testing"
)

func orcaTestPool(t *testing.T) Pool {
	t.Helper()
	b, e := os.ReadFile("../../test/contracts/orca-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestOrcaStaticIndependentStateAndWire(t *testing.T) {
	p := orcaTestPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	w, e := OrcaWhirlpoolToProto(p.OrcaWhirlpool)
	if e != nil {
		t.Fatal(e)
	}
	s, e := OrcaWhirlpoolFromProto(w)
	if e != nil {
		t.Fatal(e)
	}
	a, _ := Hash(s)
	b, _ := Hash(p.OrcaWhirlpool)
	if a != b {
		t.Fatal("Orca state lost")
	}
	c := p.Clone()
	c.OrcaWhirlpool.TickArrays[0].Ticks[0].LiquidityNet = "0"
	if c.OrcaWhirlpool.TickArrays[0].Ticks[0].LiquidityNet == p.OrcaWhirlpool.TickArrays[0].Ticks[0].LiquidityNet {
		t.Fatal("Orca mutable alias")
	}
}
func TestOrcaRejectsAdaptiveUnprovenOrWrongModelState(t *testing.T) {
	for _, mode := range []string{"fee-tier", "tick-range", "missing-array", "missing-tick", "bitmap", "fee", "growth", "raw-model", "adapter", "other-model"} {
		t.Run(mode, func(t *testing.T) {
			p := orcaTestPool(t)
			s := p.OrcaWhirlpool
			switch mode {
			case "fee-tier":
				s.FeeTierIndex++
			case "tick-range":
				s.TickCurrentIndex = 443637
			case "missing-array":
				s.TickArrays = s.TickArrays[1:]
			case "missing-tick":
				s.TickArrays[0].Ticks = nil
			case "bitmap":
				s.TickArrays[0].InitializedBitmap = "0"
			case "fee":
				s.ProtocolFeeRate = 10001
			case "growth":
				s.FeeGrowthGlobal0 = "340282366920938463463374607431768211456"
			case "raw-model":
				p.Reserve0 = "1"
			case "adapter":
				p.AdapterVersion = "uniswap-v3/core-1"
			case "other-model":
				p.Protocol = "fixture-cpmm"
			}
			if p.Validate() == nil {
				t.Fatal("invalid Orca accepted", mode)
			}
		})
	}
}
func TestOrcaQ64TickEndpointsAreOriginal(t *testing.T) {
	for tick, want := range map[int32]string{-443636: "4295048016", 0: "18446744073709551616", 443636: "79226673515401279992447579055"} {
		n, e := OrcaSqrtPriceAtTick(tick)
		if e != nil || n.String() != want {
			t.Fatal(tick, n, e)
		}
	}
}

func TestOrcaProgramDataMustBelongToFixedProgram(t *testing.T) {
	p := orcaTestPool(t)
	p.OrcaWhirlpool.Programdata = "DMawCQzbgNTmbzaESc7o6pvL1KAeetY8zA7jNpzntHhU"
	if p.Validate() == nil {
		t.Fatal("foreign ProgramData accepted")
	}
}

func TestOrcaIndependentOriginalTickMathVectors(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/orca-tickmath.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Vectors []struct {
			Tick         int32  `json:"tick"`
			SqrtPriceX64 string `json:"sqrt_price_x64"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(b, &f); e != nil {
		t.Fatal(e)
	}
	if len(f.Vectors) < 1000 {
		t.Fatal("missing tick vectors")
	}
	for _, v := range f.Vectors {
		p, e := OrcaSqrtPriceAtTick(v.Tick)
		if e != nil || p.String() != v.SqrtPriceX64 {
			t.Fatal(v.Tick, p, v.SqrtPriceX64, e)
		}
	}
}

func TestOrcaRejectsMalformedSolanaChainKey(t *testing.T) {
	p := orcaTestPool(t)
	p.ChainKey = "solana:"
	if p.Validate() == nil {
		t.Fatal("invalid chain key accepted")
	}
}
