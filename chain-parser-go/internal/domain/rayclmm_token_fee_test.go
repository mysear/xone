package domain

import "testing"

func TestRayCLMMTokenFeeExplicitProfileAndWire(t *testing.T) {
	for _, mode := range []uint32{1, 2} {
		p := rayclmmTestPool(t)
		p.Protocol = "raydium-clmm-token-fee"
		p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-token-fee-1"
		p.RaydiumCLMM.CollectFeeOn = mode
		if e := p.Validate(); e != nil {
			t.Fatal(e)
		}
		w, e := RaydiumCLMMToProto(p.RaydiumCLMM)
		if e != nil {
			t.Fatal(e)
		}
		s, e := RaydiumCLMMFromProto(w)
		if e != nil || s.CollectFeeOn != mode {
			t.Fatal(s, e)
		}
		p.Protocol = "raydium-clmm-static"
		p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-static-1"
		if p.Validate() == nil {
			t.Fatal("old profile accepts token fee")
		}
	}
	for _, mode := range []uint32{0, 3, 255, 256} {
		p := rayclmmTestPool(t)
		p.Protocol = "raydium-clmm-token-fee"
		p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-token-fee-1"
		p.RaydiumCLMM.CollectFeeOn = mode
		if p.Validate() == nil {
			t.Fatal("unknown fee mode", mode)
		}
	}
}
