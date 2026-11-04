package domain

import (
	"encoding/json"
	"os"
	"testing"
)

func rayclmmTestPool(t *testing.T) Pool {
	t.Helper()
	b, e := os.ReadFile("../../test/contracts/rayclmm-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestRayCLMMIndependentBitmapObservationAndWire(t *testing.T) {
	p := rayclmmTestPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	w, e := RaydiumCLMMToProto(p.RaydiumCLMM)
	if e != nil {
		t.Fatal(e)
	}
	s, e := RaydiumCLMMFromProto(w)
	if e != nil {
		t.Fatal(e)
	}
	a, _ := Hash(s)
	b, _ := Hash(p.RaydiumCLMM)
	if a != b {
		t.Fatal("CLMM state lost")
	}
	c := p.Clone()
	c.RaydiumCLMM.Bitmap[0] = 1
	c.RaydiumCLMM.TickArrays[0].Ticks[0].LiquidityNet = "0"
	c.RaydiumCLMM.Observations[0].TickCumulative = 1
	if c.RaydiumCLMM.Bitmap[0] == p.RaydiumCLMM.Bitmap[0] || c.RaydiumCLMM.TickArrays[0].Ticks[0].LiquidityNet == p.RaydiumCLMM.TickArrays[0].Ticks[0].LiquidityNet || c.RaydiumCLMM.Observations[0].TickCumulative == p.RaydiumCLMM.Observations[0].TickCumulative {
		t.Fatal("CLMM deep clone alias")
	}
}
func TestRayCLMMRejectsUnprovenOrMixedState(t *testing.T) {
	for _, mode := range []string{"bitmap", "arrays", "mint", "epoch", "observation", "fees", "price", "model", "chain"} {
		t.Run(mode, func(t *testing.T) {
			p := rayclmmTestPool(t)
			s := p.RaydiumCLMM
			switch mode {
			case "bitmap":
				s.Bitmap = s.Bitmap[:15]
			case "arrays":
				s.TickArrays[0].Ticks = nil
			case "mint":
				p.Token0 = p.Token1
			case "epoch":
				s.RecentEpoch = s.BankEpoch + 1
			case "observation":
				s.ObservationIndex = 100
			case "fees":
				s.FundFeeRate = 1000000
			case "price":
				s.SqrtPriceX64 = "79226673521066979257578248091"
			case "model":
				p.Protocol = "orca-whirlpools-static"
			case "chain":
				p.ChainKey = "solana:"
			}
			if p.Validate() == nil {
				t.Fatal("unsafe CLMM accepted", mode)
			}
		})
	}
}
func TestRayCLMMQ64UsesOriginalDifferentTickMap(t *testing.T) {
	for tick, want := range map[int32]string{-443636: "4295048016", 0: "18446744073709551616", 443636: "79226673521066979257578248091"} {
		n, e := RayCLMMSqrtPriceAtTick(tick)
		if e != nil || n.String() != want {
			t.Fatal(tick, n, e)
		}
	}
}

func TestRayCLMMActualEntryRequiresStrictOpenTime(t *testing.T) {
	p := rayclmmTestPool(t)
	p.RaydiumCLMM.OpenTime = p.RaydiumCLMM.BankUnixTimestamp
	if p.Validate() == nil {
		t.Fatal("equal open_time violates actual swap entrypoint")
	}
}

func TestRayCLMMProgramDataCannotBelongToAnotherProtocol(t *testing.T) {
	p := rayclmmTestPool(t)
	p.RaydiumCLMM.Programdata = OrcaProgramData
	if p.Validate() == nil {
		t.Fatal("wrong ProgramData accepted")
	}
}

func TestRayCLMMIndependentOriginalTickMathVectors(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/rayclmm-tickmath.json")
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
	if len(f.Vectors) != 2053 {
		t.Fatal("missing tick vectors")
	}
	for _, v := range f.Vectors {
		p, e := RayCLMMSqrtPriceAtTick(v.Tick)
		if e != nil || p.String() != v.SqrtPriceX64 {
			t.Fatal(v.Tick, p, v.SqrtPriceX64, e)
		}
	}
}

func TestRayCLMMRejectsForgedPDABindings(t *testing.T) {
	for _, mode := range []string{"bump", "pool", "vault0", "vault1", "observation", "array"} {
		t.Run(mode, func(t *testing.T) {
			p := rayclmmTestPool(t)
			s := p.RaydiumCLMM
			switch mode {
			case "bump":
				s.PoolBump ^= 1
			case "pool":
				p.PoolID = OrcaProgramData
			case "vault0":
				s.Vault0 = OrcaProgramData
			case "vault1":
				s.Vault1 = OrcaProgramData
			case "observation":
				s.Observation = OrcaProgramData
			case "array":
				s.TickArrays[0].Address = OrcaProgramData
			}
			if p.Validate() == nil {
				t.Fatal("forged PDA accepted", mode)
			}
		})
	}
}
