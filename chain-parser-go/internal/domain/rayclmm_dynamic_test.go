package domain

import (
	"encoding/json"
	"os"
	"testing"
)

func TestRayCLMMAdvancedNativeCorpusWire(t *testing.T) {
	path := os.Getenv("XONE_RAYCLMM_ADVANCED_CORPUS")
	if path == "" {
		t.Skip("requires the original generate-advanced corpus")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var corpus struct {
		Cases []struct {
			Pool     Pool
			Expected struct {
				NextPool Pool `json:"next_pool"`
			}
		}
	}
	if err := json.Unmarshal(raw, &corpus); err != nil {
		t.Fatal(err)
	}
	if len(corpus.Cases) != 24 {
		t.Fatalf("expected 24 native cases, got %d", len(corpus.Cases))
	}
	for _, c := range corpus.Cases {
		for _, pool := range []Pool{c.Pool, c.Expected.NextPool} {
			if err := pool.Validate(); err != nil {
				t.Fatal(err)
			}
			wire, err := RaydiumCLMMToProto(pool.RaydiumCLMM)
			if err != nil {
				t.Fatal(err)
			}
			state, err := RaydiumCLMMFromProto(wire)
			if err != nil {
				t.Fatal(err)
			}
			actual, _ := Hash(state)
			expected, _ := Hash(pool.RaydiumCLMM)
			if actual != expected {
				t.Fatalf("advanced wire state mismatch: %s != %s", actual, expected)
			}
		}
	}
}

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
				p.RaydiumCLMM.TickSpacing = 2
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
			err := p.Validate()
			if mode == "maxwidth" && (err == nil || err.Error() != "RAYCLMM_DYNAMIC_PARAMETERS") {
				t.Fatalf("expected dynamic fee width rejection, got %v", err)
			}
			if err == nil {
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
