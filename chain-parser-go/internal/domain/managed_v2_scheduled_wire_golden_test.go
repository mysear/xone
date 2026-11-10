package domain

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"os"
	"reflect"
	"testing"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protoreflect"
	pb "xone/common/protoc/golang/dexpb"
)

// Synthetic syntax/domain contract only; copied code identities attest no deployment.
func scheduledWireGolden(t *testing.T) (Snapshot, *pb.OnlineSnapshot, []byte) {
	t.Helper()
	data, e := os.ReadFile("../../test/contracts/managed-v2-scheduled-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot  Snapshot `json:"snapshot"`
		Hex       string   `json:"protobuf_hex"`
		SHA       string   `json:"protobuf_sha256"`
		Authority string   `json:"authority"`
	}
	if e = json.Unmarshal(data, &f); e != nil {
		t.Fatal(e)
	}
	if f.Authority != "synthetic-scheduled-state-wire-contract-no-native-execution-proof" || f.Snapshot.Source != "SYNTHETIC_MANAGED_SCHEDULED_WIRE" {
		t.Fatal("synthetic authority missing")
	}
	raw, e := hex.DecodeString(f.Hex)
	if e != nil {
		t.Fatal(e)
	}
	h := sha256.Sum256(raw)
	if hex.EncodeToString(h[:]) != f.SHA {
		t.Fatal("C++ wire digest")
	}
	var w pb.OnlineSnapshot
	if e = proto.Unmarshal(raw, &w); e != nil {
		t.Fatal(e)
	}
	if e = f.Snapshot.Validate(f.Snapshot.Reference.OwnerService, 1); e != nil {
		t.Fatal(e)
	}
	return f.Snapshot, &w, raw
}

func TestManagedScheduledWireGoldenRoundtrip(t *testing.T) {
	s, w, raw := scheduledWireGolden(t)
	encoded, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	b, e := proto.Marshal(encoded)
	if e != nil || !bytes.Equal(b, raw) {
		t.Fatal("Go differs from C++ protoc", e)
	}
	round, e := SnapshotFromProto(w)
	if e != nil || !reflect.DeepEqual(round, s) {
		t.Fatal("complete Snapshot/Pool/schedule roundtrip", e)
	}
	a, _ := Hash(s)
	h, _ := Hash(round)
	if a != h {
		t.Fatal("canonical H changed")
	}
	p := s.Pools[0].Clone()
	p.MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord = "0"
	if s.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord == "0" {
		t.Fatal("schedule clone aliases")
	}
	v, e := s.Pools[0].MultiAsset.ManagedV2.Schedule.At(1767225662)
	if e != nil {
		t.Fatal(e)
	}
	sum := new(big.Int)
	for _, w := range v.Weights {
		sum.Add(sum, w.Big())
	}
	if sum.String() != "1000000000000000002" {
		t.Fatal("literal ONE+2 getter normalized")
	}
	schedule := s.Pools[0].MultiAsset.ManagedV2.Schedule
	for _, tc := range []struct {
		time uint64
		sum  string
	}{{v.WeightStart, "1000000000000000001"}, {v.WeightEnd, "1000000000000000000"}} {
		view, err := schedule.At(tc.time)
		if err != nil {
			t.Fatal(err)
		}
		total := new(big.Int)
		for _, weight := range view.Weights {
			total.Add(total, weight.Big())
		}
		if total.String() != tc.sum {
			t.Fatalf("start/target sum at %d: %s", tc.time, total)
		}
	}
	m := w.Pools[0].MultiAsset.ManagedV2
	if m.ProtoReflect().Descriptor().Fields().Len() != 42 {
		t.Fatal("39 required fields plus optional schedule40/BPT41/circuit42 expected")
	}
	f := m.ProtoReflect().Descriptor().Fields().ByName("schedule")
	if f.Number() != 40 || !f.HasPresence() {
		t.Fatal("schedule must be optional message40")
	}
	if m.GradualFeeEnd == nil || *m.GradualFeeEnd != 0 || m.Paused == nil || *m.Paused {
		t.Fatal("known 0/false presence lost")
	}
}

func TestManagedScheduledWireGoldenRejectMutations(t *testing.T) {
	_, w, _ := scheduledWireGolden(t)
	cases := []struct {
		name   string
		mutate func(*pb.OnlineSnapshot)
	}{
		{"missing40", func(q *pb.OnlineSnapshot) { q.Pools[0].MultiAsset.ManagedV2.Schedule = nil }},
		{"word presence", func(q *pb.OnlineSnapshot) { q.Pools[0].MultiAsset.ManagedV2.Schedule.PoolStateWord = nil }},
		{"word zero", func(q *pb.OnlineSnapshot) {
			q.Pools[0].MultiAsset.ManagedV2.Schedule.PoolStateWord.Uint256Be = make([]byte, 32)
		}},
		{"token known zero", func(q *pb.OnlineSnapshot) {
			q.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord.Uint256Be = make([]byte, 32)
		}},
		{"short amount", func(q *pb.OnlineSnapshot) {
			q.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord.Uint256Be = []byte{1}
		}},
		{"padding", func(q *pb.OnlineSnapshot) {
			q.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord.Uint256Be[0] = 1
		}},
		{"duplicate", func(q *pb.OnlineSnapshot) {
			q.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[1].Token = q.Pools[0].Token0
		}},
		{"normalize", func(q *pb.OnlineSnapshot) { q.Pools[0].MultiAsset.Weights[0].Uint256Be[31] -= 2 }},
		{"wrong adapter", func(q *pb.OnlineSnapshot) { q.Pools[0].AdapterVersion = ManagedV2AdapterVersion }},
		{"caller target sum", func(q *pb.OnlineSnapshot) {
			q.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[0].StateWord.Uint256Be[23]++
		}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			q := proto.Clone(w).(*pb.OnlineSnapshot)
			c.mutate(q)
			if _, e := SnapshotFromProto(q); e == nil {
				t.Fatal("invalid scheduled wire accepted")
			}
		})
	}
}

func TestManagedScheduledWireGoldenAllRequiredPresence(t *testing.T) {
	_, w, _ := scheduledWireGolden(t)
	var walk func(protoreflect.Message, string, []protoreflect.FieldNumber)
	// Traverse present messages on the immutable wire and replay each path on a clone.
	walk = func(m protoreflect.Message, label string, path []protoreflect.FieldNumber) {
		fields := m.Descriptor().Fields()
		for i := 0; i < fields.Len(); i++ {
			f := fields.Get(i)
			if m.Descriptor().Name() == "BalancerManagedV2State" && (f.Name() == "bpt" || f.Name() == "circuit_breakers") {
				if m.Has(f) {
					t.Fatal("old scheduled wire acquired BPT capability")
				}
				continue
			}
			if !m.Has(f) {
				t.Fatalf("missing %s.%s", label, f.Name())
			}
			q := proto.Clone(w).(*pb.OnlineSnapshot)
			target := q.Pools[0].MultiAsset.ManagedV2.ProtoReflect()
			for _, n := range path {
				target = target.Get(target.Descriptor().Fields().ByNumber(n)).Message()
			}
			target.Clear(f)
			if _, e := SnapshotFromProto(q); e == nil {
				t.Fatalf("missing %s.%s accepted", label, f.Name())
			}
			if !f.IsList() && f.Kind() == protoreflect.MessageKind && f.Message().Name() != "Amount" {
				next := append(append([]protoreflect.FieldNumber{}, path...), f.Number())
				walk(m.Get(f).Message(), label+"."+string(f.Name()), next)
			}
		}
	}
	walk(w.Pools[0].MultiAsset.ManagedV2.ProtoReflect(), "managed_v2", nil)
	for _, part := range []string{"dependencies", "token_info", "tokens"} {
		count := 5
		if part == "token_info" || part == "tokens" {
			count = 3
		}
		for i := 0; i < count; i++ {
			var source protoreflect.Message
			m := w.Pools[0].MultiAsset.ManagedV2
			switch part {
			case "dependencies":
				source = m.Dependencies[i].ProtoReflect()
			case "token_info":
				source = m.TokenInfo[i].ProtoReflect()
			default:
				source = m.Schedule.Tokens[i].ProtoReflect()
			}
			fs := source.Descriptor().Fields()
			for j := 0; j < fs.Len(); j++ {
				f := fs.Get(j)
				q := proto.Clone(w).(*pb.OnlineSnapshot)
				m := q.Pools[0].MultiAsset.ManagedV2
				var target protoreflect.Message
				switch part {
				case "dependencies":
					target = m.Dependencies[i].ProtoReflect()
				case "token_info":
					target = m.TokenInfo[i].ProtoReflect()
				default:
					target = m.Schedule.Tokens[i].ProtoReflect()
				}
				target.Clear(f)
				if _, e := SnapshotFromProto(q); e == nil {
					t.Fatalf("missing %s[%d].%s accepted", part, i, f.Name())
				}
			}
		}
	}
}

func TestManagedScheduledWireGoldenJSONAndDerived(t *testing.T) {
	s, _, _ := scheduledWireGolden(t)
	raw, _ := json.Marshal(s.Pools[0])
	var original map[string]any
	json.Unmarshal(raw, &original)
	for _, part := range []string{"state", "schedule", "tokens", "dependencies", "token_info"} {
		obj := original["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
		objects := []any{obj}
		if part == "schedule" {
			objects = []any{obj["schedule"]}
		}
		if part == "tokens" {
			objects = obj["schedule"].(map[string]any)["tokens"].([]any)
		}
		if part == "dependencies" || part == "token_info" {
			objects = obj[part].([]any)
		}
		for i, o := range objects {
			for field := range o.(map[string]any) {
				for _, null := range []bool{false, true} {
					var p map[string]any
					json.Unmarshal(raw, &p)
					target := p["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
					if part == "schedule" {
						target = target["schedule"].(map[string]any)
					}
					if part == "tokens" {
						target = target["schedule"].(map[string]any)["tokens"].([]any)[i].(map[string]any)
					}
					if part == "dependencies" || part == "token_info" {
						target = target[part].([]any)[i].(map[string]any)
					}
					if null {
						target[field] = nil
					} else {
						delete(target, field)
					}
					b, _ := json.Marshal(p)
					var bad Pool
					if err := json.Unmarshal(b, &bad); err == nil && (part != "state" || field != "schedule" || null || bad.Validate() == nil) {
						t.Fatalf("missing/null %s.%s accepted", part, field)
					}
				}
			}
		}
	}
	p := s.Pools[0].Clone()
	m := p.MultiAsset.ManagedV2
	m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp}
	if e := p.Validate(); e != nil {
		t.Fatal("derived same H should be locally valid", e)
	}
	s.Pools[0] = p
	if s.Validate(s.Reference.OwnerService, 1) == nil {
		t.Fatal("derived Snapshot accepted")
	}
	if _, e := SnapshotToProto(s); e == nil {
		t.Fatal("derived Snapshot wire accepted")
	}
	if _, e := MultiAssetToProto(p.MultiAsset); e == nil {
		t.Fatal("derived wire accepted")
	}
}

func TestManagedScheduledWireGoldenLegacyRejectsEndedSchedule(t *testing.T) {
	s, _, _ := scheduledWireGolden(t)
	// First construct a fully valid canonical source after end. This ensures the
	// legacy rejection cannot be explained by stale weights/header/time instead.
	p := &s.Pools[0]
	m := p.MultiAsset.ManagedV2
	m.Timestamp = 1767226663
	s.Reference.Block.TimestampMS = m.Timestamp * 1000
	v, e := m.Schedule.At(m.Timestamp)
	if e != nil {
		t.Fatal(e)
	}
	p.MultiAsset.Weights = v.Weights
	if e = s.Validate(s.Reference.OwnerService, 1); e != nil {
		t.Fatal(e)
	}
	w, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	p.Protocol = ManagedV2Protocol
	p.AdapterVersion = ManagedV2AdapterVersion
	s.Reference.AdapterSetHash, _ = Hash([]string{p.AdapterVersion})
	if s.Validate(s.Reference.OwnerService, 1) == nil {
		t.Fatal("legacy accepted ended field40 Snapshot")
	}
	w.Pools[0].Protocol = ManagedV2Protocol
	w.Pools[0].AdapterVersion = ManagedV2AdapterVersion
	h, _ := hex.DecodeString(s.Reference.AdapterSetHash)
	w.Reference.AdapterSetHash = h
	if _, e = SnapshotFromProto(w); e == nil {
		t.Fatal("legacy accepted ended field40 wire")
	}
}

func TestManagedScheduledWireGoldenTokenVectorAndAmounts(t *testing.T) {
	_, w, _ := scheduledWireGolden(t)
	for _, n := range []int{0, 1, 51} {
		q := proto.Clone(w).(*pb.OnlineSnapshot)
		r := q.Pools[0].MultiAsset.ManagedV2.Schedule
		r.Tokens = nil
		for i := 0; i < n; i++ {
			r.Tokens = append(r.Tokens, proto.Clone(w.Pools[0].MultiAsset.ManagedV2.Schedule.Tokens[i%3]).(*pb.ManagedV2ScheduledToken))
		}
		if _, e := SnapshotFromProto(q); e == nil {
			t.Fatalf("token vector length %d accepted", n)
		}
	}
	for _, n := range []int{0, 31, 33} {
		for _, poolWord := range []bool{false, true} {
			q := proto.Clone(w).(*pb.OnlineSnapshot)
			r := q.Pools[0].MultiAsset.ManagedV2.Schedule
			word := r.Tokens[0].StateWord
			if poolWord {
				word = r.PoolStateWord
			}
			word.Uint256Be = make([]byte, n)
			if _, e := SnapshotFromProto(q); e == nil {
				t.Fatal("non32-byte word accepted")
			}
		}
	}
}
