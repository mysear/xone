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

func managedFeePoolJSON(t *testing.T) map[string]any {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/multi-asset-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Pools []map[string]any `json:"pools"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	p := f.Pools[0]
	p["protocol"] = "balancer-v2-managed-fees"
	p["adapter_version"] = "balancer-v2-managed-fees/e281ff40-bpt-1"
	p["multi_asset"].(map[string]any)["managed_fees"] = map[string]any{"total_supply": "1000000000000000000000000", "protocol_swap_fee": "300000000000000000", "management_swap_fee": "100000000000000000", "owner": "0x0000000000000000000000000000000000000090", "protocol_collector": "0x0000000000000000000000000000000000000091", "owner_balance": "700000000000000000000000", "protocol_collector_balance": "200000000000000000000000", "timestamp": 1767225600, "block_number": 100, "block_hash": "0x" + strings.Repeat("a", 64)}
	return p
}
func parseManagedFeePool(t *testing.T, p map[string]any) Pool {
	t.Helper()
	raw, e := json.Marshal(p)
	if e != nil {
		t.Fatal(e)
	}
	var q Pool
	if e = json.Unmarshal(raw, &q); e != nil {
		t.Fatal(e)
	}
	return q
}
func TestManagedFeesProfileRequiresCompleteSupplyAndFeeLedger(t *testing.T) {
	p := parseManagedFeePool(t, managedFeePoolJSON(t))
	if e := p.Validate(); e != nil {
		t.Fatal("new Managed fee ledger not admitted:", e)
	}
	for _, bad := range []string{"missing", "zero-supply", "fee-range", "oversupply", "duplicate-recipient", "unknown-time", "wrong-hash", "legacy", "above-native-swap-fee"} {
		t.Run(bad, func(t *testing.T) {
			raw := managedFeePoolJSON(t)
			state := raw["multi_asset"].(map[string]any)
			fees := state["managed_fees"].(map[string]any)
			switch bad {
			case "missing":
				delete(state, "managed_fees")
			case "zero-supply":
				fees["total_supply"] = "0"
			case "fee-range":
				fees["protocol_swap_fee"] = "500000000000000001"
			case "oversupply":
				fees["protocol_collector_balance"] = "400000000000000000000000"
			case "duplicate-recipient":
				fees["protocol_collector"] = fees["owner"]
			case "unknown-time":
				fees["timestamp"] = 0
			case "wrong-hash":
				fees["block_hash"] = "0x" + strings.Repeat("0", 64)
			case "legacy":
				raw["protocol"] = "balancer-v2-managed-snapshot"
				raw["adapter_version"] = "balancer-v2-managed-snapshot/no-bpt-fees-1"
			case "above-native-swap-fee":
				raw["fee_numerator"] = uint64(800000000000000001)
			}
			if parseManagedFeePool(t, raw).Validate() == nil {
				t.Fatal("bad Managed fee ledger admitted", bad)
			}
		})
	}
	raw := managedFeePoolJSON(t)
	raw["fee_numerator"] = uint64(800000000000000000)
	if e := parseManagedFeePool(t, raw).Validate(); e != nil {
		t.Fatal("original 80% swap fee boundary rejected", e)
	}
	raw = managedFeePoolJSON(t)
	fees := raw["multi_asset"].(map[string]any)["managed_fees"].(map[string]any)
	fees["protocol_collector"] = fees["owner"]
	fees["protocol_collector_balance"] = fees["owner_balance"]
	if e := parseManagedFeePool(t, raw).Validate(); e != nil {
		t.Fatal("same recipient must be coalesced", e)
	}
}

func TestManagedFeesWireCloneAndSnapshotIdentity(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-fees-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot    Snapshot `json:"snapshot"`
		ProtobufHex string   `json:"protobuf_hex"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	bytes, e := hex.DecodeString(f.ProtobufHex)
	if e != nil {
		t.Fatal(e)
	}
	var w pb.OnlineSnapshot
	if e = proto.Unmarshal(bytes, &w); e != nil {
		t.Fatal(e)
	}
	s, e := SnapshotFromProto(&w)
	if e != nil {
		t.Fatal(e)
	}
	a, _ := Hash(s)
	b, _ := Hash(f.Snapshot)
	if a != b {
		t.Fatal("Managed BPT state lost in independent wire")
	}
	out, e := SnapshotToProto(s)
	if e != nil || !proto.Equal(out, &w) {
		t.Fatal("Managed BPT wire lost", e)
	}
	q := s.Pools[0].Clone()
	q.MultiAsset.ManagedFees.OwnerBalance = "1"
	if s.Pools[0].MultiAsset.ManagedFees.OwnerBalance == "1" {
		t.Fatal("fee ledger clone alias")
	}
	for _, bad := range []string{"block", "time", "hash"} {
		q := s
		q.Pools = append([]Pool{}, s.Pools...)
		q.Pools[0] = s.Pools[0].Clone()
		m := q.Pools[0].MultiAsset.ManagedFees
		switch bad {
		case "block":
			m.BlockNumber++
		case "time":
			m.Timestamp++
		case "hash":
			m.BlockHash = "0x" + strings.Repeat("b", 64)
		}
		if q.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
			t.Fatal("foreign bank accepted", bad)
		}
	}
	w.Pools[0].MultiAsset.ManagedFees.TotalSupply = nil
	if _, e = SnapshotFromProto(&w); e == nil {
		t.Fatal("missing required supply silently became zero")
	}
}
