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

func TestOrcaAdaptiveRewardsProfileWireAndIsolation(t *testing.T) {
	p := orcaAdaptiveTestPool(t, nil)
	p.Protocol = "orca-whirlpools-adaptive-rewards"
	p.AdapterVersion = "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1"
	s := p.OrcaWhirlpool
	s.RewardInfos[0].Mint = p.Token0
	s.RewardInfos[0].Vault = s.Vault0
	s.RewardInfos[0].EmissionsPerSecondX64 = "340282366920938463463374607431768211455"
	s.RewardInfos[0].GrowthGlobalX64 = "20"
	if err := p.Validate(); err != nil {
		t.Fatal(err)
	}
	w, e := OrcaWhirlpoolToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	r, e := OrcaWhirlpoolFromProto(w)
	if e != nil || !reflect.DeepEqual(r, s) {
		t.Fatal("reward+adaptive roundtrip", e)
	}
	p.Protocol = "orca-whirlpools-adaptive"
	p.AdapterVersion = "orca-whirlpools/cc683ec9-spl-adaptive-1"
	if p.Validate() == nil {
		t.Fatal("old profile accepted initialized rewards")
	}
}
func TestOrcaAdaptiveRewardsIndependentWire(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/orca-adaptive-rewards-wire.json")
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
