package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"strings"
	"testing"
	"xone/common/protoc/golang/dexpb"
)

func TestCrossLanguageGolden(t *testing.T) {
	var golden struct {
		SnapshotHash string `json:"snapshot_sha256"`
		Scope        string `json:"scope_hash"`
		Wire         string `json:"protobuf_amount_hex"`
	}
	b, e := os.ReadFile("../../test/contracts/snapshot-golden.json")
	if e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(b, &golden); e != nil {
		t.Fatal(e)
	}
	b, e = os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var snapshot Snapshot
	if e = json.Unmarshal(b, &snapshot); e != nil {
		t.Fatal(e)
	}
	h, e := Hash(snapshot)
	if e != nil || h != golden.SnapshotHash {
		t.Fatal("snapshot hash", h, e)
	}
	if e = snapshot.Validate("chain-parser-go", 1); e != nil {
		t.Fatal(e)
	}
	n, e := AmountFromBytes(mustHex(t, strings.Repeat("ff", 32)))
	if e != nil {
		t.Fatal(e)
	}
	raw, _ := n.Bytes()
	wire, e := proto.Marshal(&dexpb.Amount{Uint256Be: raw})
	if e != nil || hex.EncodeToString(wire) != golden.Wire {
		t.Fatal("wire contract", e)
	}
	var msg dexpb.Amount
	if e = proto.Unmarshal(wire, &msg); e != nil {
		t.Fatal(e)
	}
	round, e := AmountFromBytes(msg.Uint256Be)
	if e != nil || round != n {
		t.Fatal("wire roundtrip", e)
	}
	snapshot.Reference.SchemaVersion = 2
	if snapshot.Validate("chain-parser-go", 1) == nil {
		t.Fatal("unknown schema accepted")
	}
}
func TestSnapshotWireUsesFullWidthBigEndian(t *testing.T) {
	raw, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s Snapshot
	if e = json.Unmarshal(raw, &s); e != nil {
		t.Fatal(e)
	}
	s.Pools[0].Reserve0 = "1"
	encoded, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	amount := encoded.Pools[0].Reserve0.Uint256Be
	if len(amount) != 32 || amount[0] != 0 || amount[31] != 1 {
		t.Fatal("wrong endianness")
	}
	encoded.Pools[0].Reserve0.Uint256Be = amount[1:]
	if _, e = SnapshotFromProto(encoded); e == nil {
		t.Fatal("narrowed amount accepted")
	}
}
