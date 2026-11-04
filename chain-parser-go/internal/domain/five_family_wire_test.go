package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func TestIndependentFiveFamilyWirePreservesFullPhysicalAndRewardState(t *testing.T) {
	for _, chain := range []string{"evm", "solana", "rayclmm"} {
		t.Run(chain, func(t *testing.T) {
			raw, e := os.ReadFile(map[string]string{"evm": "../../test/contracts/five-family-evm-wire.json", "solana": "../../test/contracts/five-family-solana-wire.json", "rayclmm": "../../test/contracts/five-family-rayclmm-wire.json"}[chain])
			if e != nil {
				t.Fatal(e)
			}
			var d struct {
				Snapshot    Snapshot `json:"snapshot"`
				ProtobufHex string   `json:"protobuf_hex"`
			}
			if e = json.Unmarshal(raw, &d); e != nil {
				t.Fatal(e)
			}
			wire, e := hex.DecodeString(d.ProtobufHex)
			if e != nil {
				t.Fatal(e)
			}
			var message pb.OnlineSnapshot
			if e = proto.Unmarshal(wire, &message); e != nil {
				t.Fatal(e)
			}
			got, e := SnapshotFromProto(&message)
			if e != nil {
				t.Fatal(e)
			}
			before, _ := Hash(d.Snapshot)
			after, _ := Hash(got)
			if before != after {
				a, _ := json.Marshal(d.Snapshot)
				b, _ := json.Marshal(got)
				t.Fatal("fullstate changed", string(a), string(b))
			}
			for _, p := range got.Pools {
				if e = p.Validate(); e != nil {
					t.Fatal(p.Protocol, e)
				}
			}
			if chain == "evm" {
				bad := got
				bad.Pools = append([]Pool{}, got.Pools...)
				for i, p := range bad.Pools {
					if p.SingletonAdvanced != nil {
						bad.Pools[i] = p.Clone()
						bad.Pools[i].SingletonAdvanced.SnapshotBlockHash = "0x" + strings.Repeat("cd", 32)
						break
					}
				}
				if bad.Validate(bad.Reference.OwnerService, bad.Reference.OwnerAssignmentVersion) == nil {
					t.Fatal("advanced pool from another block entered snapshot")
				}
			}
			again, e := SnapshotToProto(got)
			if e != nil {
				t.Fatal(e)
			}
			if !proto.Equal(again, &message) {
				t.Fatal("protobuf state changed")
			}
		})
	}
}
