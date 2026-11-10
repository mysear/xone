package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func pancakeWire(t *testing.T) []struct {
	Snapshot Snapshot `json:"snapshot"`
	Hex      string   `json:"protobuf_hex"`
} {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/pancake-lm-wire.json")
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
	return f.Vectors
}
func TestPancakeLmIndependentCppWirePreservesAllFieldsAndObservedZero(t *testing.T) {
	vectors := pancakeWire(t)
	if len(vectors) != 4 {
		t.Fatal("wire cases")
	}
	for _, v := range vectors {
		if e := v.Snapshot.Validate("chain-parser-go", 1); e != nil {
			t.Fatal(e)
		}
		raw, e := hex.DecodeString(v.Hex)
		if e != nil {
			t.Fatal(e)
		}
		var wire pb.OnlineSnapshot
		if e = proto.Unmarshal(raw, &wire); e != nil {
			t.Fatal(e)
		}
		actual, e := SnapshotFromProto(&wire)
		if e != nil || !reflect.DeepEqual(actual, v.Snapshot) {
			t.Fatal(e, "C++ decode")
		}
		back, e := SnapshotToProto(v.Snapshot)
		if e != nil || !proto.Equal(back, &wire) {
			t.Fatal(e, "C++ reencode")
		}
		s := v.Snapshot.Pools[0].PancakeLm
		cloned := v.Snapshot.Pools[0].Clone()
		cloned.PancakeLm.Lm.Ticks[0].RewardGrowthOutside = "101"
		cloned.PancakeLm.Observations[0].TickCumulative = "1"
		if cloned.PancakeLm.Lm.Ticks[0].RewardGrowthOutside == s.Lm.Ticks[0].RewardGrowthOutside || cloned.PancakeLm.Observations[0].TickCumulative == s.Observations[0].TickCumulative {
			t.Fatal("clone aliases original")
		}
	}
}
func TestPancakeLmMissingNullDuplicateOrDerivedSourceNeverBecomesObservedZero(t *testing.T) {
	s := pancakeWire(t)[0].Snapshot.Pools[0].PancakeLm
	raw, _ := json.Marshal(s)
	for _, key := range []string{"tick", "fee_protocol", "derived", "unlocked", "observation_index", "master_chef", "lm"} {
		var object map[string]json.RawMessage
		json.Unmarshal(raw, &object)
		delete(object, key)
		missing, _ := json.Marshal(object)
		var value PancakeLmState
		if json.Unmarshal(missing, &value) == nil {
			t.Fatal("missing accepted", key)
		}
		object[key] = json.RawMessage("null")
		null, _ := json.Marshal(object)
		if json.Unmarshal(null, &value) == nil {
			t.Fatal("null accepted", key)
		}
	}
	duplicate := strings.Replace(string(raw), "\"tick\":0,", "\"tick\":0,\"tick\":0,", 1)
	var value PancakeLmState
	if json.Unmarshal([]byte(duplicate), &value) == nil {
		t.Fatal("duplicate accepted")
	}
	snapshot := pancakeWire(t)[0].Snapshot
	snapshot.Pools[0].PancakeLm.Derived = true
	if snapshot.Validate("chain-parser-go", 1) == nil {
		t.Fatal("derived source accepted")
	}
	encoded, _ := SnapshotToProto(pancakeWire(t)[0].Snapshot)
	encoded.Pools[0].PancakeLm.Tick = nil
	if _, e := SnapshotFromProto(encoded); e == nil {
		t.Fatal("absent optional tick became zero")
	}
}
