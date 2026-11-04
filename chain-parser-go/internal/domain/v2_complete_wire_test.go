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

func TestV2CompleteIndependentCPPWireAndKnownZeroPresence(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/v2-complete-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Cases []struct {
			Name     string   `json:"name"`
			Snapshot Snapshot `json:"snapshot"`
			Hex      string   `json:"protobuf_hex"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	if len(f.Cases) != 3 {
		t.Fatal("wire inventory")
	}
	for _, c := range f.Cases {
		bytes, e := hex.DecodeString(c.Hex)
		if e != nil {
			t.Fatal(e)
		}
		var w pb.OnlineSnapshot
		if e = proto.Unmarshal(bytes, &w); e != nil {
			t.Fatal(e)
		}
		got, e := SnapshotFromProto(&w)
		if e != nil {
			t.Fatal(c.Name, e)
		}
		if !reflect.DeepEqual(got, c.Snapshot) {
			t.Fatal("independent wire differs", c.Name)
		}
		back, e := SnapshotToProto(c.Snapshot)
		if e != nil || !proto.Equal(back, &w) {
			t.Fatal("wire encoding changed", c.Name, e)
		}
	}
}
