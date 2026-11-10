package domain

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func TestLfjIndependentCppWireCloneAndSourceBank(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/lfj-v21-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Vectors []struct {
			Snapshot Snapshot `json:"snapshot"`
			Hex      string   `json:"protobuf_hex"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	if len(data.Vectors) != 4 {
		t.Fatal("incomplete LFJ C++ vectors")
	}
	for _, v := range data.Vectors {
		if e = v.Snapshot.Validate("chain-parser-go", 1); e != nil {
			t.Fatal(e)
		}
		bytes, e := hex.DecodeString(v.Hex)
		if e != nil {
			t.Fatal(e)
		}
		var message pb.OnlineSnapshot
		if e = proto.Unmarshal(bytes, &message); e != nil {
			t.Fatal(e)
		}
		actual, e := SnapshotFromProto(&message)
		if e != nil || !reflect.DeepEqual(actual, v.Snapshot) {
			t.Fatal("LFJ C++ decode drift", e)
		}
		round, e := SnapshotToProto(v.Snapshot)
		if e != nil || !proto.Equal(round, &message) {
			t.Fatal("LFJ independent typed wire drift", e)
		}
		copy := v.Snapshot.Pools[0].Clone()
		copy.Lfj.Bins[0].Reserve0 = "42"
		if copy.Lfj.Bins[0].Reserve0 == v.Snapshot.Pools[0].Lfj.Bins[0].Reserve0 {
			t.Fatal("LFJ shallow bin clone")
		}
		if len(copy.Lfj.OracleSamples) > 0 {
			copy.Lfj.OracleSamples[0].Data = "0"
			if v.Snapshot.Pools[0].Lfj.OracleSamples[0].Data == "0" {
				t.Fatal("LFJ shallow oracle clone")
			}
		}
		for bad := 0; bad < 12; bad++ {
			q := proto.Clone(&message).(*pb.OnlineSnapshot)
			s := q.Pools[0].Lfj
			switch bad {
			case 0:
				s.Parameters = nil
			case 1:
				s.Bins[0].Reserve0 = nil
			case 2:
				s.CodeSha256 = nil
			case 3:
				s.Bins[0] = nil
			case 4:
				s.RawReserve0 = nil
			case 5:
				s.BlockHash = nil
			case 6:
				s.MaxSteps = 0
			case 7:
				s.Bins = s.Bins[:len(s.Bins)-1]
			case 8:
				s.Parameters.Uint256Be = s.Parameters.Uint256Be[:31]
			case 9:
				q.Pools[0].Lfj = nil
			case 10:
				s.Timestamp = nil
			case 11:
				s.BlockNumber = nil
			}
			decoded, err := SnapshotFromProto(q)
			if err == nil {
				err = decoded.Validate("chain-parser-go", 1)
			}
			if err == nil {
				t.Fatal("invalid LFJ wire accepted", bad)
			}
		}
	}
}

func TestLfjJSONRequiredObservedZerosRemainDistinctFromMissingNullAndDuplicates(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/lfj-v21-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var file map[string]json.RawMessage
	if e = json.Unmarshal(raw, &file); e != nil {
		t.Fatal(e)
	}
	var vectors []struct {
		Snapshot Snapshot `json:"snapshot"`
	}
	if e = json.Unmarshal(file["vectors"], &vectors); e != nil {
		t.Fatal(e)
	}
	state := vectors[0].Snapshot.Pools[0].Lfj
	valid, e := json.Marshal(state)
	if e != nil {
		t.Fatal(e)
	}
	for _, key := range []string{"block_number", "timestamp", "parameters", "bins", "oracle_samples", "max_steps"} {
		for _, null := range []bool{false, true} {
			var fields map[string]json.RawMessage
			json.Unmarshal(valid, &fields)
			if null {
				fields[key] = json.RawMessage("null")
			} else {
				delete(fields, key)
			}
			bad, _ := json.Marshal(fields)
			var out LfjState
			if json.Unmarshal(bad, &out) == nil {
				t.Fatal("missing/null LFJ value accepted", key, null)
			}
		}
	}
	for _, bad := range []string{string(valid[:len(valid)-1]) + `,"timestamp":` + fmt.Sprint(state.Timestamp) + `}`, string(valid[:len(valid)-1]) + `,"unexpected":0}`} {
		var out LfjState
		if json.Unmarshal([]byte(bad), &out) == nil {
			t.Fatal("duplicate/unknown LFJ value accepted")
		}
	}
	last := vectors[len(vectors)-1].Snapshot
	w, e := SnapshotToProto(last)
	if e != nil {
		t.Fatal(e)
	}
	if w.Pools[0].Lfj.Timestamp == nil || w.Pools[0].Lfj.BlockNumber == nil || *w.Pools[0].Lfj.Timestamp != 0 || *w.Pools[0].Lfj.BlockNumber != 0 {
		t.Fatal("observed zero bank fields lost presence")
	}
	decoded, e := SnapshotFromProto(w)
	if e != nil || !reflect.DeepEqual(last, decoded) {
		t.Fatal("observed zero bank roundtrip failed", e)
	}
}
