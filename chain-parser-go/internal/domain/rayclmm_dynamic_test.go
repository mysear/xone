package domain

import (
	"encoding/json"
	"testing"
)

func dynamicTestPool(t *testing.T) Pool {
	p := seedTestPool(t, 1)
	p.Protocol = "raydium-clmm-dynamic"
	p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-dynamic-1"
	p.RaydiumCLMM.DynamicFee = &RaydiumCLMMDynamicFee{FilterPeriod: 10, DecayPeriod: 60, ReductionFactor: 5000, DynamicFeeControl: 1000, MaxVolatilityAccumulator: 1000000, TickSpacingIndexReference: 0, VolatilityReference: 20000, VolatilityAccumulator: 30000, LastUpdateTimestamp: p.RaydiumCLMM.BankUnixTimestamp}
	return p
}
func TestRayCLMMDynamicStateProfileAndWire(t *testing.T) {
	p := dynamicTestPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	w, e := RaydiumCLMMToProto(p.RaydiumCLMM)
	if e != nil {
		t.Fatal(e)
	}
	back, e := RaydiumCLMMFromProto(w)
	if e != nil || back.DynamicFee == nil || *back.DynamicFee != *p.RaydiumCLMM.DynamicFee {
		t.Fatal(back, e)
	}
	clone := p.Clone()
	clone.RaydiumCLMM.DynamicFee.VolatilityAccumulator++
	if clone.RaydiumCLMM.DynamicFee.VolatilityAccumulator == p.RaydiumCLMM.DynamicFee.VolatilityAccumulator {
		t.Fatal("dynamicclonealias")
	}
	p.RaydiumCLMM.DynamicFee.LastUpdateTimestamp = p.RaydiumCLMM.BankUnixTimestamp + 1
	if e = p.Validate(); e != nil {
		t.Fatal("sourcefuturetimestampsaturates", e)
	}
}
func TestRayCLMMDynamicParametersAndOldProfileRejection(t *testing.T) {
	for _, mode := range []string{"missing", "filter-zero", "filter-width", "decay", "reduction", "control", "maxwidth", "reference", "accumulator", "index", "timestamp", "oldprofile"} {
		t.Run(mode, func(t *testing.T) {
			p := dynamicTestPool(t)
			d := p.RaydiumCLMM.DynamicFee
			switch mode {
			case "missing":
				p.RaydiumCLMM.DynamicFee = nil
			case "filter-zero":
				d.FilterPeriod = 0
			case "filter-width":
				d.FilterPeriod = 65536
			case "decay":
				d.DecayPeriod = d.FilterPeriod
			case "reduction":
				d.ReductionFactor = 10000
			case "control":
				d.DynamicFeeControl = 100000
			case "maxwidth":
				d.MaxVolatilityAccumulator = 4294967295
			case "reference":
				d.VolatilityReference = d.MaxVolatilityAccumulator + 1
			case "accumulator":
				d.VolatilityAccumulator = d.MaxVolatilityAccumulator + 1
			case "index":
				d.TickSpacingIndexReference = 2147483647
			case "timestamp":
				d.LastUpdateTimestamp = 4294967296
			case "oldprofile":
				p.Protocol = "raydium-clmm-custom-seed"
				p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-custom-seed-1"
			}
			if p.Validate() == nil {
				t.Fatal("invaliddynamic", mode)
			}
		})
	}
	p := rayclmmTestPool(t)
	raw, e := json.Marshal(p.RaydiumCLMM)
	if e != nil {
		t.Fatal(e)
	}
	var doc map[string]any
	_ = json.Unmarshal(raw, &doc)
	if _, exists := doc["dynamic_fee"]; exists {
		t.Fatal("legacydynamicnotomitted")
	}
}
