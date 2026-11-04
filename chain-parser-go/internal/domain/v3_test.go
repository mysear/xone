package domain

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func v3TestPool() Pool {
	return Pool{ChainKey: "evm:31337", PoolID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v3", Token0: "A", Token1: "B", Reserve0: "0", Reserve1: "0", FeeNumerator: 3000, FeeDenominator: 1000000, AdapterVersion: "uniswap-v3/core-1", TokenBehavior: "standard", V3: &V3State{SqrtPriceX96: "79228162514264337593543950336", Tick: 0, Liquidity: "1000000", TickSpacing: 60, Factory: "0x" + strings.Repeat("1", 40), CodeSHA256: strings.Repeat("a", 64), MaxSteps: 512, TickWords: []V3BitmapWord{{WordPos: -1, Bitmap: "0"}, {WordPos: 0, Bitmap: "0"}}, Ticks: []V3Tick{}}}
}
func TestV3ProofAndTickPriceValidation(t *testing.T) {
	p := v3TestPool()
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	p.V3.TickWords[1].Bitmap = "1"
	if p.Validate() == nil {
		t.Fatal("bitmap initialized tick missing")
	}
	p.V3.Ticks = []V3Tick{{Tick: 0, LiquidityGross: "100", LiquidityNet: "100"}}
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	p.V3.SqrtPriceX96 = "1"
	if p.Validate() == nil {
		t.Fatal("invalid price accepted")
	}
}
func TestTickMathCanonicalBoundaries(t *testing.T) {
	for tick, want := range map[int32]string{0: "79228162514264337593543950336", -887272: "4295128739", 887272: "1461446703485210103287273052203988822378723970342"} {
		got, e := SqrtRatioAtTick(tick)
		if e != nil || got.String() != want {
			t.Fatal(tick, got, e)
		}
	}
}

func TestV3SnapshotProtobufKeepsSignedLiquidityAndProof(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/v3-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var pool Pool
	if e = json.Unmarshal(raw, &pool); e != nil {
		t.Fatal(e)
	}
	base, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s Snapshot
	if e = json.Unmarshal(base, &s); e != nil {
		t.Fatal(e)
	}
	s.Pools = []Pool{pool}
	s.Reference.ScopeHash, _ = ScopeHash(s.Pools)
	wire, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	got, e := SnapshotFromProto(wire)
	if e != nil {
		t.Fatal(e)
	}
	if got.Pools[0].V3 == nil || got.Pools[0].V3.Ticks[2].LiquidityNet != "-2000000000" {
		t.Fatal("V3 state lost or signed net changed")
	}
}

func TestMinimumInitializationPriceIsAValidV3State(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/v3-minimum-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(raw, &p); e != nil {
		t.Fatal(e)
	}
	if e = p.Validate(); e != nil {
		t.Fatal("valid initialized minimum rejected", e)
	}
}

func TestV3OptionalFactoryDigestCannotPublishUnreadableWire(t *testing.T) {
	for _, hash := range []string{"aa", "zz" + strings.Repeat("a", 62), strings.Repeat("A", 64)} {
		t.Run(hash[:2], func(t *testing.T) {
			p := v3TestPool()
			p.V3.FactoryCodeSHA256 = hash
			if p.Validate() == nil {
				t.Fatal("invalid optional factory digest accepted by domain")
			}
			if _, e := V3ToProto(p.V3); e == nil {
				t.Fatal("invalid optional factory digest published")
			}
		})
	}
	for _, hash := range []string{"", strings.Repeat("b", 64)} {
		p := v3TestPool()
		p.V3.FactoryCodeSHA256 = hash
		wire, e := V3ToProto(p.V3)
		if e != nil {
			t.Fatal(e)
		}
		got, e := V3FromProto(wire)
		if e != nil || got.FactoryCodeSHA256 != hash {
			t.Fatal("valid factory identity lost", e)
		}
	}
}
