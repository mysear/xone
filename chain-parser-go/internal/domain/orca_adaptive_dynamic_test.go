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

func TestOrcaAdaptiveDynamicMixedWireAndOldProfileIsolation(t *testing.T) {
	for mask := 0; mask < 8; mask++ {
		p := orcaAdaptiveTestPool(t, nil)
		p.Protocol = "orca-whirlpools-adaptive-dynamic"
		p.AdapterVersion = "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1"
		for i := range p.OrcaWhirlpool.TickArrays {
			p.OrcaWhirlpool.TickArrays[i].Layout = "fixed"
			if mask&(1<<i) != 0 {
				p.OrcaWhirlpool.TickArrays[i].Layout = "dynamic"
			}
		}
		if e := p.Validate(); e != nil {
			t.Fatal(mask, e)
		}
		w, e := OrcaWhirlpoolToProto(p.OrcaWhirlpool)
		if e != nil {
			t.Fatal(e)
		}
		s, e := OrcaWhirlpoolFromProto(w)
		if e != nil || !reflect.DeepEqual(s, p.OrcaWhirlpool) {
			t.Fatal("wire", e)
		}
		for _, old := range []struct{ profile, adapter string }{{"orca-whirlpools-adaptive", "orca-whirlpools/cc683ec9-spl-adaptive-1"}, {"orca-whirlpools-adaptive-rewards", "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1"}} {
			p.Protocol = old.profile
			p.AdapterVersion = old.adapter
			if (p.Validate() == nil) != (mask == 0) {
				t.Fatal(old.profile, mask)
			}
		}
	}
}
func TestOrcaAdaptiveDynamicIndependentWire(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/orca-adaptive-dynamic-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot Snapshot `json:"snapshot"`
		Hex      string   `json:"protobuf_hex"`
	}
	if e = json.Unmarshal(b, &f); e != nil {
		t.Fatal(e)
	}
	raw, e := hex.DecodeString(f.Hex)
	if e != nil {
		t.Fatal(e)
	}
	w := new(pb.OnlineSnapshot)
	if e = proto.Unmarshal(raw, w); e != nil {
		t.Fatal(e)
	}
	s, e := SnapshotFromProto(w)
	if e != nil {
		t.Fatal(e)
	}
	if e = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e != nil {
		t.Fatal(e)
	}
	again, e := SnapshotToProto(s)
	if e != nil || !proto.Equal(again, w) {
		t.Fatal("independent protobuf changed", e)
	}
	if !reflect.DeepEqual(s, f.Snapshot) {
		t.Fatal("independent snapshot mismatch")
	}
}

func TestOrcaAdaptiveDynamicRejectsIncompleteBitmapOracleAndAdapter(t *testing.T) {
	for _, bad := range []string{"count", "sequence", "layout", "bitmap", "oracle", "binding", "reward-count", "reserved", "adapter"} {
		t.Run(bad, func(t *testing.T) {
			p := orcaAdaptiveTestPool(t, nil)
			p.Protocol = "orca-whirlpools-adaptive-dynamic"
			p.AdapterVersion = "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1"
			s := p.OrcaWhirlpool
			s.TickArrays[0].Layout = "dynamic"
			switch bad {
			case "count":
				s.TickArrays = s.TickArrays[:2]
			case "sequence":
				s.TickArrays[2].StartTickIndex += 88
			case "layout":
				s.TickArrays[0].Layout = "sparse"
			case "bitmap":
				s.TickArrays[0].InitializedBitmap = "309485009821345068724781056"
			case "oracle":
				s.Adaptive = nil
			case "binding":
				s.Adaptive.Whirlpool = s.Vault0
			case "reward-count":
				s.RewardInfos = s.RewardInfos[:2]
			case "reserved":
				s.Adaptive.Reserved = s.Adaptive.Reserved[:127]
			case "adapter":
				p.AdapterVersion = "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1"
			}
			if p.Validate() == nil {
				t.Fatal("invalid dynamic profile accepted")
			}
		})
	}
}
