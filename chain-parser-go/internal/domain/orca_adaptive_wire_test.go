package domain

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"reflect"
	"testing"

	"google.golang.org/protobuf/proto"
	pb "xone/common/protoc/golang/dexpb"
)

func TestOrcaAdaptiveIndependentCompleteWireSnapshot(t *testing.T) {
	raw, err := os.ReadFile("../../test/contracts/orca-adaptive-wire.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Snapshot       Snapshot `json:"snapshot"`
		ProtobufHex    string   `json:"protobuf_hex"`
		ProtobufSHA256 string   `json:"protobuf_sha256"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	wire, err := hex.DecodeString(fixture.ProtobufHex)
	if err != nil {
		t.Fatal(err)
	}
	digest := sha256.Sum256(wire)
	if hex.EncodeToString(digest[:]) != fixture.ProtobufSHA256 {
		t.Fatal("independent wire digest changed")
	}
	var message pb.OnlineSnapshot
	if err = proto.Unmarshal(wire, &message); err != nil {
		t.Fatal(err)
	}
	got, err := SnapshotFromProto(&message)
	if err != nil {
		t.Fatal(err)
	}
	if err = got.Validate(got.Reference.OwnerService, got.Reference.OwnerAssignmentVersion); err != nil {
		t.Fatal(err)
	}
	wantHash, err := Hash(fixture.Snapshot)
	if err != nil {
		t.Fatal(err)
	}
	gotHash, err := Hash(got)
	if err != nil {
		t.Fatal(err)
	}
	if wantHash != gotHash {
		t.Fatal("independent complete snapshot changed")
	}
	if got.Source != fixture.Snapshot.Source || !reflect.DeepEqual(got.HistoryGaps, fixture.Snapshot.HistoryGaps) {
		t.Fatal("source/history changed")
	}
	again, err := SnapshotToProto(got)
	if err != nil {
		t.Fatal(err)
	}
	if !proto.Equal(again, &message) {
		t.Fatal("independent complete protobuf round trip changed")
	}
	jsonRoundTrip, err := json.Marshal(got)
	if err != nil {
		t.Fatal(err)
	}
	var parsed Snapshot
	if err = json.Unmarshal(jsonRoundTrip, &parsed); err != nil {
		t.Fatal(err)
	}
	if err = parsed.Validate(parsed.Reference.OwnerService, parsed.Reference.OwnerAssignmentVersion); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(parsed, got) {
		t.Fatal("complete JSON round trip changed")
	}
	copied := got.Pools[0].Clone()
	copied.OrcaWhirlpool.Adaptive.ConstantsReserved[15] ^= 1
	copied.OrcaWhirlpool.Adaptive.VariablesReserved[15] ^= 1
	copied.OrcaWhirlpool.Adaptive.Reserved[127] ^= 1
	copied.OrcaWhirlpool.RewardInfos[0].Extension[31] ^= 1
	originalHash, _ := Hash(got)
	if originalHash != gotHash {
		t.Fatal("independent snapshot clone aliases original")
	}
	bad := got
	bad.Pools = []Pool{got.Pools[0].Clone()}
	bad.Pools[0].OrcaWhirlpool.BankSlot++
	if bad.Validate(bad.Reference.OwnerService, bad.Reference.OwnerAssignmentVersion) == nil {
		t.Fatal("adaptive state from another bank entered snapshot")
	}
}
