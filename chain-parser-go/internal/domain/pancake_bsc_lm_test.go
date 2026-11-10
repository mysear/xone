package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func TestPancakeBsc4IndependentCppWireKeepsRawAndInheritedTickState(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/pancake-bsc4-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Vectors []struct {
			Snapshot Snapshot `json:"snapshot"`
			Hex      string   `json:"protobuf_hex"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	if len(f.Vectors) != 4 {
		t.Fatal("missing independent vectors")
	}
	for _, v := range f.Vectors {
		if e = v.Snapshot.Validate("chain-parser-go", 1); e != nil {
			t.Fatal(e)
		}
		b, e := hex.DecodeString(v.Hex)
		if e != nil {
			t.Fatal(e)
		}
		var wire pb.OnlineSnapshot
		if e = proto.Unmarshal(b, &wire); e != nil {
			t.Fatal(e)
		}
		original := proto.Clone(&wire)
		actual, e := SnapshotFromProto(&wire)
		if e != nil || !reflect.DeepEqual(actual, v.Snapshot) {
			t.Fatal(e, "wire decode")
		}
		if !proto.Equal(original, &wire) {
			t.Fatal("decode mutated source protobuf")
		}
		encoded, e := SnapshotToProto(v.Snapshot)
		if e != nil || !proto.Equal(encoded, &wire) {
			t.Fatal(e, "wire encode")
		}
		s := v.Snapshot.Pools[0].PancakeBscLm
		copy := v.Snapshot.Pools[0].Clone()
		copy.PancakeBscLm.Predecessors[0].Ticks[0].RewardGrowthOutside = "101"
		if reflect.DeepEqual(copy.PancakeBscLm.Predecessors, s.Predecessors) {
			t.Fatal("source clone aliases")
		}
	}
}
func TestPancakeBsc4MissingMigrationInputsNeverBecomeZero(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/pancake-bsc4-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Cases []struct {
			Pool Pool `json:"pool"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	source := data.Cases[10].Pool.PancakeBscLm
	b, _ := json.Marshal(source)
	for _, key := range []string{"ledger", "flags", "predecessors", "initialization"} {
		var object map[string]json.RawMessage
		json.Unmarshal(b, &object)
		delete(object, key)
		missing, _ := json.Marshal(object)
		var out PancakeBscLmState
		if json.Unmarshal(missing, &out) == nil {
			t.Fatal("missing", key)
		}
		object[key] = json.RawMessage("null")
		null, _ := json.Marshal(object)
		if json.Unmarshal(null, &out) == nil {
			t.Fatal("null", key)
		}
	}
	w, e := PancakeBscLmStateToProto(source)
	if e != nil {
		t.Fatal(e)
	}
	w.Initialization = nil
	if _, e = PancakeBscLmStateFromProto(w); e == nil {
		t.Fatal("missing false")
	}
	w, e = PancakeBscLmStateToProto(source)
	if e != nil {
		t.Fatal(e)
	}
	w.Flags[0].Migrated = nil
	if _, e = PancakeBscLmStateFromProto(w); e == nil {
		t.Fatal("missing flag")
	}
}

func TestPancakeBsc4ExtraPredecessorAndContradictoryBankReject(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/pancake-bsc4-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Cases []struct {
			Pool Pool `json:"pool"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	p := f.Cases[10].Pool
	bad := p.Clone()
	bad.PancakeBscLm.Predecessors = append(bad.PancakeBscLm.Predecessors, bad.PancakeBscLm.Predecessors[0])
	if bad.Validate() == nil {
		t.Fatal("extra predecessor")
	}
	bad = p.Clone()
	bad.PancakeBscLm.Predecessors[0].Address = p.Token0
	if bad.Validate() == nil {
		t.Fatal("token alias")
	}
	other := p.Clone()
	other.PancakeBscLm.Predecessors[0].Ticks[0].RewardGrowthOutside = "999"
	if ValidatePoolDependencies([]Pool{p, other}) == nil {
		t.Fatal("contradictory readonly bank")
	}
}
