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

func TestManagedV2BptIndependentCPPWire(t *testing.T) {
	data, e := os.ReadFile("../../test/contracts/managed-v2-bpt-wire.json")
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
	if f.Authority != "synthetic-bpt-state-wire-contract-no-native-execution-proof" {
		t.Fatal("synthetic authority lost")
	}
	if e = f.Snapshot.Validate(f.Snapshot.Reference.OwnerService, 1); e != nil {
		t.Fatal(e)
	}
	raw, e := hex.DecodeString(f.Hex)
	if e != nil {
		t.Fatal(e)
	}
	digest := sha256.Sum256(raw)
	if hex.EncodeToString(digest[:]) != f.SHA {
		t.Fatal("wire digest")
	}
	w := new(pb.OnlineSnapshot)
	if e = proto.Unmarshal(raw, w); e != nil {
		t.Fatal(e)
	}
	back, e := SnapshotFromProto(w)
	if e != nil || !reflect.DeepEqual(back, f.Snapshot) {
		t.Fatal("C++ full snapshot differs", e)
	}
	encoded, e := SnapshotToProto(f.Snapshot)
	if e != nil {
		t.Fatal(e)
	}
	b, e := proto.Marshal(encoded)
	if e != nil || !bytes.Equal(b, raw) {
		t.Fatal("Go/C++ bytes differ", e)
	}
	m := w.Pools[0].MultiAsset.ManagedV2
	if m.ProtoReflect().Descriptor().Fields().ByName("bpt").Number() != 41 {
		t.Fatal("BPT tag")
	}
	if m.Bpt.BptLastChangeBlock == nil || *m.Bpt.BptLastChangeBlock != 0 || m.Bpt.AllowlistMembers[0].Allowed == nil || *m.Bpt.AllowlistMembers[0].Allowed {
		t.Fatal("known zero/false lost")
	}
	for i := 0; i < 5; i++ {
		q := proto.Clone(w).(*pb.OnlineSnapshot)
		m := q.Pools[0].MultiAsset.ManagedV2
		switch i {
		case 0:
			m.Bpt = nil
		case 1:
			m.Bpt.JoinExitEnabled = nil
		case 2:
			m.Bpt.MinimumBptLocked = nil
		case 3:
			m.Bpt.BptLastChangeBlock = nil
		case 4:
			m.Bpt.AllowlistMembers[0].Allowed = nil
		}
		if _, e := SnapshotFromProto(q); e == nil {
			t.Fatalf("missing BPT field %d accepted", i)
		}
	}
}
func TestManagedV2BptOriginalProjectionCanonicalWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-v2-bpt-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var v struct {
		Authority   string `json:"authority"`
		OriginalRaw struct {
			SHA256 string `json:"sha256"`
			Bytes  int    `json:"bytes"`
		} `json:"original_raw"`
		Banks []struct {
			Pools []struct {
				Pool Pool `json:"pool"`
			} `json:"pools"`
		} `json:"banks"`
	}
	if e = json.Unmarshal(raw, &v); e != nil {
		t.Fatal(e)
	}
	if v.Authority != "authenticated-original-managed-v2-bpt-observations" {
		t.Fatal("projection authority")
	}
	source, e := os.ReadFile("../../test/contracts/managed-v2-bpt.json")
	if e != nil {
		t.Fatal(e)
	}
	sourceDigest := sha256.Sum256(source)
	if hex.EncodeToString(sourceDigest[:]) != v.OriginalRaw.SHA256 || len(source) != v.OriginalRaw.Bytes {
		t.Fatal("projection detached from original bytes")
	}
	count := 0
	for _, b := range v.Banks {
		for _, entry := range b.Pools {
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
				t.Fatal("original full state wire differs", e)
			}
			count++
		}
	}
	if count != 286 {
		t.Fatal("missing supported original pools", count)
	}
}
