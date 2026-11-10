package domain

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func TestManagedV2CircuitIndependentCPPField42(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-v2-breaker-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot  Snapshot `json:"snapshot"`
		Hex       string   `json:"protobuf_hex"`
		SHA       string   `json:"protobuf_sha256"`
		Authority string   `json:"authority"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	if f.Authority != "synthetic-breaker-state-wire-contract-no-native-execution-proof" {
		t.Fatal("synthetic authority")
	}
	if e = f.Snapshot.Validate(f.Snapshot.Reference.OwnerService, 1); e != nil {
		t.Fatal(e)
	}
	data, e := hex.DecodeString(f.Hex)
	if e != nil {
		t.Fatal(e)
	}
	sum := sha256.Sum256(data)
	if hex.EncodeToString(sum[:]) != f.SHA {
		t.Fatal("wire digest")
	}
	w := new(pb.OnlineSnapshot)
	if e = proto.Unmarshal(data, w); e != nil {
		t.Fatal(e)
	}
	s, e := SnapshotFromProto(w)
	if e != nil || !reflect.DeepEqual(s, f.Snapshot) {
		t.Fatal("full CPP Snapshot", e)
	}
	q, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	encoded, e := proto.Marshal(q)
	if e != nil || !bytes.Equal(encoded, data) {
		t.Fatal("CPP/Go bytes differ", e)
	}
	m := w.Pools[0].MultiAsset.ManagedV2
	if m.ProtoReflect().Descriptor().Fields().ByName("circuit_breakers").Number() != 42 {
		t.Fatal("new tag")
	}
	if m.CircuitBreakers.Tokens[0].StateWord.Uint256Be[0] < 128 {
		t.Fatal("high word lost")
	}
	for i := 0; i < 8; i++ {
		q := proto.Clone(w).(*pb.OnlineSnapshot)
		m := q.Pools[0].MultiAsset.ManagedV2
		if i == 0 {
			m.CircuitBreakers = nil
		} else {
			c := m.CircuitBreakers.Tokens[0]
			switch i {
			case 1:
				c.StateWord = nil
			case 2:
				c.ReferenceBptPrice = nil
			case 3:
				c.ReferenceWeight = nil
			case 4:
				c.LowerPercentage = nil
			case 5:
				c.UpperPercentage = nil
			case 6:
				c.LowerBptPriceBound = nil
			case 7:
				c.UpperBptPriceBound = nil
			}
		}
		if _, e := SnapshotFromProto(q); e == nil {
			t.Fatal("required field accepted", i)
		}
	}
}
func TestManagedV2CircuitOriginalProjectionCompleteWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-v2-breaker.json")
	if e != nil {
		t.Fatal(e)
	}
	data, e := os.ReadFile("../../test/contracts/managed-v2-breaker-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var v struct {
		Authority string `json:"authority"`
		Original  struct {
			SHA   string `json:"sha256"`
			Bytes int    `json:"bytes"`
		} `json:"original_raw"`
		Banks []struct {
			Pools []struct {
				Pool Pool `json:"pool"`
			} `json:"pools"`
		} `json:"banks"`
	}
	if e = json.Unmarshal(data, &v); e != nil {
		t.Fatal(e)
	}
	sum := sha256.Sum256(raw)
	if v.Authority != "authenticated-original-managed-v2-breaker-observations" || v.Original.SHA != hex.EncodeToString(sum[:]) || v.Original.Bytes != len(raw) {
		t.Fatal("original source identity")
	}
	count := 0
	for _, bank := range v.Banks {
		for _, entry := range bank.Pools {
			p := entry.Pool
			if e = p.Validate(); e != nil {
				t.Fatal(e)
			}
			w, e := MultiAssetToProto(p.MultiAsset)
			if e != nil {
				t.Fatal(e)
			}
			q, e := MultiAssetFromProto(w)
			if e != nil || !reflect.DeepEqual(q, p.MultiAsset) {
				t.Fatal("full source wire", e)
			}
			count++
		}
	}
	if count != 600 {
		t.Fatal("original Pool inventory", count)
	}
}
