package domain

import (
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"reflect"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func managedScheduledPool(t *testing.T) Pool {
	t.Helper()
	p := managedV2Synthetic(t)
	p.Protocol = ManagedV2ScheduledProtocol
	p.AdapterVersion = ManagedV2ScheduledAdapterVersion
	s := p.MultiAsset
	m := s.ManagedV2
	raw := managedScheduleFixture()
	m.Schedule = &raw
	m.ManagementAUMFee = "0"
	m.PendingAUM = "0"
	m.ActualSupply = m.VirtualSupply
	v, e := raw.At(m.Timestamp)
	if e != nil {
		t.Fatal(e)
	}
	s.Weights = v.Weights
	for i := range s.Assets {
		s.Assets[i].Scaling = v.Scaling[i]
	}
	p.FeeNumerator = v.SwapFee.Big().Uint64()
	m.GradualWeightEnd = v.WeightEnd
	m.GradualFeeEnd = v.FeeEnd
	return p
}
func TestManagedScheduledProfileRequiresRawAndPreservesStaticBoundary(t *testing.T) {
	p := managedScheduledPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal("complete scheduled source rejected", e)
	}
	for _, mutate := range []func(*Pool){
		func(p *Pool) { p.MultiAsset.ManagedV2.Schedule = nil },
		func(p *Pool) { p.Protocol = ManagedV2Protocol; p.AdapterVersion = ManagedV2AdapterVersion },
		func(p *Pool) { p.AdapterVersion = ManagedV2AdapterVersion },
		func(p *Pool) { p.FeeNumerator++ }, func(p *Pool) { p.MultiAsset.Weights[0] = "800000000000000001" },
		func(p *Pool) { p.MultiAsset.ManagedV2.SwapsEnabled = false },
		func(p *Pool) { p.MultiAsset.Assets[1].Scaling = "1000000000000000000" },
	} {
		q := p.Clone()
		mutate(&q)
		if q.Validate() == nil {
			t.Fatal("schedule/profile/source getter disagreement accepted")
		}
	}
	if e := managedV2Synthetic(t).Validate(); e != nil {
		t.Fatal("old profile boundary changed", e)
	}
}
func TestManagedScheduledWireClonePresenceAndSourceIdentity(t *testing.T) {
	p := managedScheduledPool(t)
	s := managedV2Snapshot(t, p)
	if e := s.Validate("test", 1); e != nil {
		t.Fatal(e)
	}
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	if w.ManagedV2.Schedule == nil || w.ManagedV2.Schedule.PoolStateWord == nil || len(w.ManagedV2.Schedule.Tokens) != 2 {
		t.Fatal("raw packed state omitted")
	}
	if w.ManagedV2.ProtoReflect().Descriptor().Fields().ByName("schedule").Number() != 40 {
		t.Fatal("new schema tag not independent")
	}
	raw, e := proto.Marshal(w)
	if e != nil {
		t.Fatal(e)
	}
	var decoded pb.MultiAssetState
	if e = proto.Unmarshal(raw, &decoded); e != nil {
		t.Fatal(e)
	}
	q, e := MultiAssetFromProto(&decoded)
	if e != nil || !reflect.DeepEqual(q, p.MultiAsset) {
		t.Fatal("complete scheduled wire did not round trip", e)
	}
	cloned := p.Clone()
	cloned.MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord = "0"
	if reflect.DeepEqual(cloned, p) || p.MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord == "0" {
		t.Fatal("raw schedule clone aliases source")
	}
	for _, mutate := range []func(*pb.MultiAssetState){
		func(w *pb.MultiAssetState) { w.ManagedV2.Schedule.PoolStateWord = nil },
		func(w *pb.MultiAssetState) { w.ManagedV2.Schedule.Tokens = nil },
		func(w *pb.MultiAssetState) { w.ManagedV2.Schedule.Tokens[0].StateWord = nil },
		func(w *pb.MultiAssetState) { w.ManagedV2.Schedule.Tokens[0].Token = "" },
	} {
		bad := proto.Clone(w).(*pb.MultiAssetState)
		mutate(bad)
		if _, e = MultiAssetFromProto(bad); e == nil {
			t.Fatal("unknown word or token silently defaulted")
		}
	}
	derived := p.Clone()
	m := derived.MultiAsset.ManagedV2
	m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp}
	if e = derived.Validate(); e != nil {
		t.Fatal(e)
	}
	if _, e = MultiAssetToProto(derived.MultiAsset); e == nil {
		t.Fatal("derived scheduled wire accepted")
	}
	if managedV2Snapshot(t, derived).Validate("test", 1) == nil {
		t.Fatal("derived scheduled snapshot accepted")
	}
	bytes, e := json.Marshal(p)
	if e != nil {
		t.Fatal(e)
	}
	var object map[string]any
	json.Unmarshal(bytes, &object)
	object["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)["schedule"] = nil
	bytes, _ = json.Marshal(object)
	var bad Pool
	if json.Unmarshal(bytes, &bad) == nil {
		t.Fatal("explicit unknown schedule accepted")
	}
}

func TestManagedScheduledParentUsesExactRawViewWithoutWeightSumTolerance(t *testing.T) {
	p := managedScheduledPool(t)
	multi := p.MultiAsset
	m := multi.ManagedV2
	m.Timestamp = 1767225662
	m.Schedule.PoolStateWord = managedScheduleWord(scheduleEntry("1767225661", 0), scheduleEntry("1767226662", 32), scheduleEntry("0", 64), scheduleEntry("0", 96), scheduleEntry("10000000000000000", 128), scheduleEntry("10000000000000000", 190), scheduleEntry("1", 252), scheduleEntry("1", 255))
	starts := []string{"499600399600399601", "300499500499500499", "199900099900099901"}
	ends := []string{"200000000000000000", "300000000000000000", "500000000000000000"}
	m.Schedule.Tokens = nil
	third := multi.Assets[0]
	third.Token = "0x0000000000000000000000000000000000000003"
	multi.Assets = append(multi.Assets, third)
	m.TokenInfo = append(m.TokenInfo, m.TokenInfo[0])
	for i := range starts {
		m.Schedule.Tokens = append(m.Schedule.Tokens, ManagedV2ScheduledToken{Token: multi.Assets[i].Token, StateWord: managedScheduleWord(scheduleEntry(starts[i], 0), scheduleEntry(ends[i], 64))})
	}
	view, e := m.Schedule.At(m.Timestamp)
	if e != nil {
		t.Fatal(e)
	}
	multi.Weights = view.Weights
	for i := range multi.Assets {
		multi.Assets[i].Scaling = view.Scaling[i]
	}
	m.GradualWeightEnd = view.WeightEnd
	m.GradualFeeEnd = view.FeeEnd
	p.FeeNumerator = view.SwapFee.Big().Uint64()
	if e = p.Validate(); e != nil {
		t.Fatal("original ONE+2 view rejected", e)
	}
	if !reflect.DeepEqual(multi.Weights, []Amount{"499301098501897704", "300499001498002497", "200199900000099801"}) {
		t.Fatal("scheduled current weights were normalized")
	}
	w, e := MultiAssetToProto(multi)
	if e != nil {
		t.Fatal(e)
	}
	decoded, e := MultiAssetFromProto(w)
	if e != nil || !reflect.DeepEqual(decoded, multi) {
		t.Fatal("raw-dependent current view lost", e)
	}
	old := p.Clone()
	old.Protocol = ManagedV2Protocol
	old.AdapterVersion = ManagedV2AdapterVersion
	old.MultiAsset.ManagedV2.Schedule = nil
	if old.Validate() == nil {
		t.Fatal("static profile silently gained scheduled weight semantics")
	}
	for _, mutate := range []func(*Pool){
		func(p *Pool) { p.MultiAsset.Weights[0] = "499301098501897702" }, // Sum ONE would be wrong too.
		func(p *Pool) { p.MultiAsset.ManagedV2.Schedule.Tokens[0].Token = p.MultiAsset.Assets[1].Token },
		func(p *Pool) { p.MultiAsset.ManagedV2.GradualWeightEnd++ },
		func(p *Pool) { p.MultiAsset.ManagedV2.AllowlistEnabled = true },
	} {
		q := p.Clone()
		mutate(&q)
		if q.Validate() == nil {
			t.Fatal("source schedule/getter disagreement accepted")
		}
	}
}
