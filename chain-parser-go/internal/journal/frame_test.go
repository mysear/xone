package journal

import (
	"encoding/json"
	"os"
	"sort"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func fixture(t *testing.T) domain.Snapshot {
	t.Helper()
	b, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s domain.Snapshot
	if e = json.Unmarshal(b, &s); e != nil {
		t.Fatal(e)
	}
	return s
}
func TestCommitBarrierAndChunkIntegrity(t *testing.T) {
	s := fixture(t)
	frames, e := Encode(Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, 1024)
	if e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(8 << 20)
	for i, f := range frames {
		unit, e := a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
		if i < len(frames)-1 && unit != nil {
			t.Fatal("partial published")
		}
		if i == len(frames)-1 && (unit == nil || unit.Snapshot.Reference != s.Reference) {
			t.Fatal("missing commit")
		}
	}
	a = NewAssembler(8 << 20)
	a.Push(frames[0])
	if _, e = a.Push(frames[len(frames)-1]); e == nil {
		t.Fatal("missing chunks accepted")
	}
	frames[1].DataHex = "00"
	a = NewAssembler(8 << 20)
	for _, f := range frames {
		_, e = a.Push(f)
		if e != nil {
			return
		}
	}
	t.Fatal("bad checksum accepted")
}
func TestUnknownSchemaAndInterleavingRejected(t *testing.T) {
	s := fixture(t)
	frames, _ := Encode(Unit{Kind: "BLOCK_COMMIT", Snapshot: s}, 1024)
	frames[0].SchemaVersion = 2
	if _, e := NewAssembler(8 << 20).Push(frames[0]); e == nil {
		t.Fatal("unknown schema")
	}
	frames[0].SchemaVersion = 1
	a := NewAssembler(8 << 20)
	a.Push(frames[0])
	if _, e := a.Push(frames[0]); e == nil {
		t.Fatal("interleaved commit")
	}
}
func TestCrossLanguageJournalGolden(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/journal-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(b, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(8 << 20)
	var last *Unit
	for _, f := range frames {
		last, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if last == nil || last.Snapshot.Reference != fixture(t).Reference {
		t.Fatal("cross-language frame contract")
	}
}
func TestProtobufWireRejectsJSONFrames(t *testing.T) {
	s := fixture(t)
	frames, e := Encode(Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, 1024)
	if e != nil {
		t.Fatal(e)
	}
	b, _ := json.Marshal(frames[0])
	if _, e = DecodeFrame(append([]byte(`{"schema_version":1,`), b[1:]...)); e == nil {
		t.Fatal("duplicate field")
	}
	if _, e = DecodeFrame(append([]byte(`{"unknown":1,`), b[1:]...)); e == nil {
		t.Fatal("unknown field")
	}
}
func TestKafkaFrameActuallyUsesProtobuf(t *testing.T) {
	frames, e := Encode(Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: fixture(t)}, 1024)
	if e != nil {
		t.Fatal(e)
	}
	for _, frame := range frames {
		raw, e := EncodeFrame(frame)
		if e != nil {
			t.Fatal(e)
		}
		if len(raw) == 0 || raw[0] == '{' {
			t.Fatal("JSON on Kafka wire")
		}
		back, e := DecodeFrame(raw)
		if e != nil || back.Reference != frame.Reference || back.DataHex != frame.DataHex {
			t.Fatal(back, e)
		}
	}
}

func TestReorgBeginWireBarrier(t *testing.T) {
	s := fixture(t)
	s.Reference.CanonicalEpoch++
	s.Reference.JournalSeq++
	frames, e := Encode(Unit{Kind: "REORG_BEGIN", Snapshot: s}, 1024)
	if e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var got *Unit
	for _, f := range frames {
		got, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if got == nil || got.Kind != "REORG_BEGIN" || got.Snapshot.Reference != s.Reference {
		t.Fatal("lost reorg barrier")
	}
}

func TestNormalizedFactsSurviveBinaryJournal(t *testing.T) {
	s := fixture(t)
	fact := domain.SwapFact{EventID: domain.SwapEventID(s.Reference.Block, 0, 1), TxHash: "0x" + strings.Repeat("a", 64), PoolID: s.Pools[0].PoolID, Token0: s.Pools[0].Token0, Token1: s.Pools[0].Token1, LogIndex: 1, Amount0In: "10", Amount1In: "0", Amount0Out: "0", Amount1Out: "20"}
	u := Unit{Kind: "BLOCK_COMMIT", Snapshot: s, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: s.Reference.Block, Swaps: []domain.SwapFact{fact}}}}}
	raw, e := EncodeUnit(u)
	if e != nil {
		t.Fatal(e)
	}
	got, e := DecodeUnit(raw)
	if e != nil || got.NormalizedFacts == nil || got.NormalizedFacts.Blocks[0].Swaps[0] != fact {
		t.Fatal(got, e)
	}
}

func TestProtocNormalizedFactsGolden(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/normalized-facts.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var got *Unit
	for _, f := range frames {
		got, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if got == nil || got.NormalizedFacts == nil || got.NormalizedFacts.Blocks[0].Swaps[0].Amount0In != "115792089237316195423570985008687907853269984665640564039457584007913129639935" {
		t.Fatal(got)
	}
}

func TestCanonicalUnitIdentityIgnoresPrivateRepeatedFieldShape(t *testing.T) {
	s := fixture(t)
	u := Unit{Kind: "BLOCK_COMMIT", Snapshot: s, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: s.Reference.Block}}}}
	raw, e := EncodeUnit(u)
	if e != nil {
		t.Fatal(e)
	}
	decoded, e := DecodeUnit(raw)
	if e != nil {
		t.Fatal(e)
	}
	a, e := u.CanonicalHash()
	if e != nil {
		t.Fatal(e)
	}
	b, e := decoded.CanonicalHash()
	if e != nil || a != b {
		t.Fatal("wire-equivalent units conflict", e)
	}
}

func TestIndependentProtocV3ProofAndSignedNet(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/v3-journal-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var got *Unit
	for _, frame := range frames {
		got, e = a.Push(frame)
		if e != nil {
			t.Fatal(e)
		}
	}
	if got == nil || got.Snapshot.Pools[0].V3 == nil || got.Snapshot.Pools[0].V3.Ticks[2].LiquidityNet != "-2000000000" {
		t.Fatal("wire proof or sign lost")
	}
}

func TestProtocDifferentDexVersionStatesRemainDistinct(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/dex-version-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	assembler := NewAssembler(1 << 20)
	var unit *Unit
	for _, f := range frames {
		unit, e = assembler.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if unit == nil || len(unit.Snapshot.Pools) != 6 {
		t.Fatal("version scope lost")
	}
	for _, p := range unit.Snapshot.Pools {
		profile, ok := domain.Protocol(p.Protocol)
		if !ok || profile.AdapterVersion != p.AdapterVersion {
			t.Fatal("wrong version")
		}
		if p.Protocol == "pancake-v3" && p.V3.FeeProtocol != 209718400 {
			t.Fatal("wide fee lost")
		}
		if profile.Family == "v2" && p.V2 == nil {
			t.Fatal("identity lost")
		}
	}
}

func TestIndependentStableProtocolWireProof(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/stable-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var unit *Unit
	for _, f := range frames {
		unit, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if unit == nil || len(unit.Snapshot.Pools) != 5 {
		t.Fatal("missing stable scopes")
	}
	for _, p := range unit.Snapshot.Pools {
		if p.Protocol == "aerodrome-slipstream" {
			if p.V3 == nil || p.V3.FactoryCodeSHA256 == "" {
				t.Fatal("Slipstream dependency missing")
			}
		} else if p.Stable == nil || p.Stable.Rate0 == "0" {
			t.Fatal("stable state missing")
		}
	}
}

func TestIndependentSingletonProtocolWireProof(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/singleton-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var unit *Unit
	for _, f := range frames {
		unit, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if unit == nil || len(unit.Snapshot.Pools) != 2 {
		t.Fatal("missing singleton scopes")
	}
	for _, p := range unit.Snapshot.Pools {
		if e = p.Validate(); e != nil {
			t.Fatal(e)
		}
		if p.SingletonCL == nil || p.V3 != nil || p.SingletonCL.ProtocolFee1 != 500 || p.SingletonCL.Core.Ticks[2].LiquidityNet != "-2000000000" {
			t.Fatal("singleton identity or core lost")
		}
	}
}

func TestIndependentRaydiumCreatorProtocWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/raydium-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var u *Unit
	for _, f := range frames {
		u, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if u == nil || len(u.Snapshot.Pools) != 1 {
		t.Fatal("Raydium state missing")
	}
	p := u.Snapshot.Pools[0]
	s := p.RaydiumCPMM
	if s == nil || s.Programdata != domain.RaydiumProgramData || s.CreatorFees0 != 700 || s.CreatorFees1 != 1700 || s.BankSlot != u.Snapshot.Reference.Block.Number {
		t.Fatal("Raydium identity/fees/bank lost")
	}
	if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
		t.Fatal(e)
	}
}

func TestIndependentOrcaQ64TickArrayProtocWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/orca-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var u *Unit
	for _, f := range frames {
		u, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if u == nil || len(u.Snapshot.Pools) != 1 {
		t.Fatal("missing Orca")
	}
	p := u.Snapshot.Pools[0]
	s := p.OrcaWhirlpool
	if s == nil || s.SqrtPriceX64 != "18446744073709551616" || s.Programdata != domain.OrcaProgramData || s.TickArrays[0].Ticks[0].LiquidityNet != "1000000000" {
		t.Fatal("Orca q64/ticks lost")
	}
	if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
		t.Fatal(e)
	}
}

func TestIndependentRayCLMMBitmapAndObservationProtocWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/rayclmm-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var u *Unit
	for _, f := range frames {
		u, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if u == nil || len(u.Snapshot.Pools) != 1 {
		t.Fatal("missing CLMM")
	}
	s := u.Snapshot.Pools[0].RaydiumCLMM
	if s == nil || len(s.Bitmap) != 16 || len(s.Observations) != 100 || s.Bitmap[7] != (uint64(1)<<63) || s.TickArrays[1].Ticks[0].LiquidityNet != "-1000000000" {
		t.Fatal("CLMM proof lost")
	}
	if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
		t.Fatal(e)
	}
	for _, mode := range []string{"bump", "array"} {
		encoded, e := domain.SnapshotToProto(u.Snapshot)
		if e != nil {
			t.Fatal(e)
		}
		state := encoded.Pools[0].RaydiumClmm
		if mode == "bump" {
			state.PoolBump ^= 1
		} else {
			state.TickArrays[0].Address = domain.OrcaProgramData
		}
		if _, e = domain.SnapshotFromProto(encoded); e == nil {
			t.Fatal("forged PDA admitted from protobuf", mode)
		}
	}

}

func TestIndependentRayCLMMTokenFeeProtocModes(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/rayclmm-token-fee-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var vectors []struct {
		CollectFeeOn uint32  `json:"collect_fee_on"`
		Frames       []Frame `json:"frames"`
	}
	if e = json.Unmarshal(raw, &vectors); e != nil {
		t.Fatal(e)
	}
	if len(vectors) != 2 {
		t.Fatal("missing modes")
	}
	for _, v := range vectors {
		a := NewAssembler(1 << 20)
		var u *Unit
		for _, f := range v.Frames {
			u, e = a.Push(f)
			if e != nil {
				t.Fatal(e)
			}
		}
		if u == nil {
			t.Fatal("missing state")
		}
		p := u.Snapshot.Pools[0]
		if p.RaydiumCLMM.CollectFeeOn != v.CollectFeeOn {
			t.Fatal("fee mode lost")
		}
		if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
			t.Fatal(e)
		}
		for _, mode := range []uint32{0, 3, 255} {
			w, e := domain.SnapshotToProto(u.Snapshot)
			if e != nil {
				t.Fatal(e)
			}
			w.Pools[0].RaydiumClmm.CollectFeeOn = mode
			if _, e = domain.SnapshotFromProto(w); e == nil {
				t.Fatal("invalid fee mode admitted", mode)
			}
		}
		w, e := domain.SnapshotToProto(u.Snapshot)
		if e != nil {
			t.Fatal(e)
		}
		w.Pools[0].Protocol = "raydium-clmm-static"
		w.Pools[0].AdapterVersion = "raydium-clmm/6d7a0f82-spl-static-1"
		if _, e = domain.SnapshotFromProto(w); e == nil {
			t.Fatal("old profile admitted token fee")
		}
	}
}

func TestIndependentRayCLMMExtensionProtocWordsAndAdmission(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/rayclmm-bitmap-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var vectors []struct {
		CollectFeeOn uint32  `json:"collect_fee_on"`
		Frames       []Frame `json:"frames"`
	}
	if e = json.Unmarshal(raw, &vectors); e != nil {
		t.Fatal(e)
	}
	if len(vectors) != 2 {
		t.Fatal("missingbitmapvectors")
	}
	for _, v := range vectors {
		a := NewAssembler(1 << 20)
		var u *Unit
		for _, f := range v.Frames {
			u, e = a.Push(f)
			if e != nil {
				t.Fatal(e)
			}
		}
		if u == nil {
			t.Fatal("missingstate")
		}
		s := u.Snapshot.Pools[0].RaydiumCLMM
		if s.BitmapExtension == nil || s.BitmapExtension.Positive[111] != uint64(1)<<63 || s.BitmapExtension.Negative[7] != (uint64(1)<<63)|1 {
			t.Fatal("extensionhighbitslost")
		}
		if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
			t.Fatal(e)
		}
		for _, mode := range []string{"missing", "width", "pda", "profile"} {
			w, e := domain.SnapshotToProto(u.Snapshot)
			if e != nil {
				t.Fatal(e)
			}
			p := w.Pools[0]
			switch mode {
			case "missing":
				p.RaydiumClmm.BitmapExtension = nil
			case "width":
				p.RaydiumClmm.BitmapExtension.Negative = p.RaydiumClmm.BitmapExtension.Negative[:111]
			case "pda":
				p.RaydiumClmm.BitmapExtension.Address = domain.OrcaProgramData
			case "profile":
				p.Protocol = "raydium-clmm-token-fee"
				p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-token-fee-1"
			}
			if _, e = domain.SnapshotFromProto(w); e == nil {
				t.Fatal("unsafe extension admission", mode)
			}
		}
	}
}

func TestIndependentRayCLMMCustomSeedProtocIdentity(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/rayclmm-seed-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var vectors []struct {
		SeedIndex uint32  `json:"seed_index"`
		Frames    []Frame `json:"frames"`
	}
	if e = json.Unmarshal(raw, &vectors); e != nil {
		t.Fatal(e)
	}
	if len(vectors) != 3 {
		t.Fatal("missingseeds")
	}
	for _, v := range vectors {
		a := NewAssembler(1 << 20)
		var u *Unit
		for _, f := range v.Frames {
			u, e = a.Push(f)
			if e != nil {
				t.Fatal(e)
			}
		}
		if u == nil || u.Snapshot.Pools[0].RaydiumCLMM.SeedIndex != v.SeedIndex {
			t.Fatal("seedlost")
		}
		if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
			t.Fatal(e)
		}
		for _, mode := range []string{"zero", "width", "different-seed", "legacy", "missing-extension"} {
			w, e := domain.SnapshotToProto(u.Snapshot)
			if e != nil {
				t.Fatal(e)
			}
			p := w.Pools[0]
			switch mode {
			case "zero":
				p.RaydiumClmm.SeedIndex = 0
			case "width":
				p.RaydiumClmm.SeedIndex = 65536
			case "different-seed":
				p.RaydiumClmm.SeedIndex ^= 1
			case "legacy":
				p.Protocol = "raydium-clmm-bitmap"
				p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-bitmap-1"
			case "missing-extension":
				p.RaydiumClmm.BitmapExtension = nil
			}
			if _, e = domain.SnapshotFromProto(w); e == nil {
				t.Fatal("unsafe seedidentity", mode)
			}
		}
	}
}

func TestIndependentRayCLMMDynamicProtocParametersAndAdmission(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/rayclmm-dynamic-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var vectors []struct {
		SeedIndex uint32  `json:"seed_index"`
		Frames    []Frame `json:"frames"`
	}
	if e = json.Unmarshal(raw, &vectors); e != nil {
		t.Fatal(e)
	}
	if len(vectors) != 3 {
		t.Fatal("missingvectors")
	}
	for _, v := range vectors {
		a := NewAssembler(1 << 20)
		var u *Unit
		for _, f := range v.Frames {
			u, e = a.Push(f)
			if e != nil {
				t.Fatal(e)
			}
		}
		if u == nil {
			t.Fatal("missingdynamic")
		}
		d := u.Snapshot.Pools[0].RaydiumCLMM.DynamicFee
		if d == nil || d.TickSpacingIndexReference != -7 || d.VolatilityAccumulator != 30000 || d.LastUpdateTimestamp != u.Snapshot.Pools[0].RaydiumCLMM.BankUnixTimestamp {
			t.Fatal("dynamicstatelost")
		}
		if e = u.Snapshot.Validate("chain-parser-rust", 1); e != nil {
			t.Fatal(e)
		}
		for _, mode := range []string{"missing", "filterwidth", "capwidth", "oldprofile"} {
			w, e := domain.SnapshotToProto(u.Snapshot)
			if e != nil {
				t.Fatal(e)
			}
			p := w.Pools[0]
			switch mode {
			case "missing":
				p.RaydiumClmm.DynamicFee = nil
			case "filterwidth":
				p.RaydiumClmm.DynamicFee.FilterPeriod = 65536
			case "capwidth":
				p.RaydiumClmm.DynamicFee.MaxVolatilityAccumulator = 4294967295
			case "oldprofile":
				p.Protocol = "raydium-clmm-custom-seed"
				p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-custom-seed-1"
			}
			if _, e = domain.SnapshotFromProto(w); e == nil {
				t.Fatal("invaliddynamicadmission", mode)
			}
		}
	}
}

func TestIndependentWeightedProtocolWireProof(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/weighted-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var unit *Unit
	for _, f := range frames {
		unit, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if unit == nil || len(unit.Snapshot.Pools) != 3 {
		t.Fatal("missing stable scopes")
	}
	for _, p := range unit.Snapshot.Pools {
		if p.Weighted == nil || p.Weighted.Weight0 != "500000000000000000" || p.Stable != nil || p.Validate() != nil {
			t.Fatal("weighted field27 lost", p)
		}
	}
}

func TestIndependentWeightedStaticProtocolWireProof(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/weighted-static-frames.json")
	if e != nil {
		t.Fatal(e)
	}
	var frames []Frame
	if e = json.Unmarshal(raw, &frames); e != nil {
		t.Fatal(e)
	}
	a := NewAssembler(1 << 20)
	var unit *Unit
	for _, f := range frames {
		unit, e = a.Push(f)
		if e != nil {
			t.Fatal(e)
		}
	}
	if unit == nil || len(unit.Snapshot.Pools) != 6 {
		t.Fatal("missing stable scopes")
	}
	versions := map[string]bool{}
	for _, p := range unit.Snapshot.Pools {
		versions[p.AdapterVersion] = true
	}
	adapters := []string{}
	for version := range versions {
		adapters = append(adapters, version)
	}
	sort.Strings(adapters)
	expected, e := domain.Hash(adapters)
	if e != nil || expected != unit.Snapshot.Reference.AdapterSetHash {
		t.Fatal("noncanonical adapter-set identity", expected, unit.Snapshot.Reference.AdapterSetHash, e)
	}
	for _, p := range unit.Snapshot.Pools {
		if p.Weighted == nil || p.Protocol != "balancer-v2-weighted2-static" || p.Stable != nil || p.Validate() != nil {
			t.Fatal("weighted field27 lost", p)
		}
	}
}
