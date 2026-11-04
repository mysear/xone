package domain

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func metaPoolJSON(t *testing.T) map[string]any {
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
	s := p["multi_asset"].(map[string]any)
	assets := s["assets"].([]any)[:2]
	a0 := assets[0].(map[string]any)
	a1 := assets[1].(map[string]any)
	a1["token"] = "0x6c3f90f043a72fa612cbac8115ee7e52bde6e490"
	a1["scaling"] = "1000000000000000000"
	s["assets"] = assets
	s["weights"] = []any{}
	s["amplification"] = "10000"
	s["admin_fee"] = uint64(5000000000)
	s["vault"] = ""
	s["vault_code_sha256"] = ""
	s["pool_id"] = ""
	p["protocol"] = "curve-metausd-direct"
	p["adapter_version"] = "curve-metausd-direct/5272fd2b-fixed-base-1"
	p["fee_numerator"] = uint64(4000000)
	p["fee_denominator"] = uint64(10000000000)
	p["token0"] = a0["token"]
	p["token1"] = a1["token"]
	p["reserve0"] = a0["balance"]
	p["reserve1"] = a1["balance"]
	impl := "0x0000000000000000000000000000000000000099"
	code, _ := hex.DecodeString("363d3d373d3d3d363d73" + impl[2:] + "5af43d82803e903d91602b57fd5bf3")
	digest := sha256.Sum256(code)
	s["code_sha256"] = hex.EncodeToString(digest[:])
	s["curve_meta"] = map[string]any{"implementation": impl, "implementation_code_sha256": "c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d", "base_pool": "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7", "base_pool_code_sha256": strings.Repeat("b", 64), "base_virtual_price": "1234567890123456789", "stored_balances": []any{a0["balance"], a1["balance"]}, "admin_balances": []any{"0", "0"}, "timestamp": uint64(1767225600), "block_number": uint64(100), "block_hash": "0x" + strings.Repeat("a", 64)}
	return p
}
func parseMeta(t *testing.T, raw map[string]any) Pool {
	t.Helper()
	b, e := json.Marshal(raw)
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestMetaUSDRequiresOwnVersionAndCompleteAccounting(t *testing.T) {
	if e := parseMeta(t, metaPoolJSON(t)).Validate(); e != nil {
		t.Fatal("Meta direct profile unavailable", e)
	}
	for _, bad := range []string{"missing", "base-pool", "base-code", "virtual-price", "stored", "admin-length", "lp-token", "lp-scale", "weights", "time", "block-hash", "legacy", "fee"} {
		t.Run(bad, func(t *testing.T) {
			raw := metaPoolJSON(t)
			s := raw["multi_asset"].(map[string]any)
			m := s["curve_meta"].(map[string]any)
			switch bad {
			case "missing":
				delete(s, "curve_meta")
			case "base-pool":
				m["base_pool"] = raw["pool_id"]
			case "base-code":
				m["base_pool_code_sha256"] = ""
			case "virtual-price":
				m["base_virtual_price"] = "0"
			case "stored":
				m["stored_balances"].([]any)[0] = "0"
			case "admin-length":
				m["admin_balances"] = []any{"0"}
			case "lp-token":
				a := s["assets"].([]any)[1].(map[string]any)
				a["token"] = "0x0000000000000000000000000000000000000008"
				raw["token1"] = a["token"]
			case "lp-scale":
				s["assets"].([]any)[1].(map[string]any)["scaling"] = "1000000000000000000000000000000"
			case "weights":
				s["weights"] = []any{"1", "1"}
			case "time":
				m["timestamp"] = 0
			case "block-hash":
				m["block_hash"] = "0x" + strings.Repeat("0", 64)
			case "legacy":
				raw["protocol"] = "curve-plain3-basic"
				raw["adapter_version"] = "curve-plain3-basic/factory-1"
			case "fee":
				raw["fee_numerator"] = uint64(100000001)
			}
			if parseMeta(t, raw).Validate() == nil {
				t.Fatal("bad Meta pool admitted", bad)
			}
		})
	}
}
func TestMetaReadOnlyBaseDependencyAndSharedSourceAreEnforced(t *testing.T) {
	p := parseMeta(t, metaPoolJSON(t))
	q := p.Clone()
	q.PoolID = "0x0000000000000000000000000000000000000020"
	if e := ValidateCurveMetaDependencies([]Pool{p, q}); e != nil {
		t.Fatal(e)
	}
	for _, bad := range []string{"mutable", "vp", "base-code", "lp-code", "source"} {
		q := p.Clone()
		q.PoolID = "0x0000000000000000000000000000000000000020"
		switch bad {
		case "mutable":
			q.PoolID = CurveMetaBasePool
		case "vp":
			q.MultiAsset.CurveMeta.BaseVirtualPrice = "1000000000000000000"
		case "base-code":
			q.MultiAsset.CurveMeta.BasePoolCodeSHA256 = strings.Repeat("d", 64)
		case "lp-code":
			q.MultiAsset.Assets[1].CodeSHA256 = strings.Repeat("d", 64)
		case "source":
			q.MultiAsset.CurveMeta.Timestamp++
		}
		if ValidateCurveMetaDependencies([]Pool{p, q}) == nil {
			t.Fatal("unsafe base dependency admitted", bad)
		}
	}
	q = p.Clone()
	q.MultiAsset.CurveMeta.StoredBalances[0] = "1"
	q.MultiAsset.CurveMeta.AdminBalances[1] = "1"
	if p.MultiAsset.CurveMeta.StoredBalances[0] == "1" || p.MultiAsset.CurveMeta.AdminBalances[1] == "1" {
		t.Fatal("Meta clone aliases balance vectors")
	}
}
func TestIndependentMetaProtoPreservesSourceAndRejectsForeignBank(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-meta-wire.json")
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
		t.Fatal("Meta source/state lost")
	}
	out, e := SnapshotToProto(s)
	if e != nil || !proto.Equal(out, &w) {
		t.Fatal("Meta wire mismatch", e)
	}
	for _, bad := range []string{"block", "time", "hash"} {
		q := s
		q.Pools = append([]Pool{}, s.Pools...)
		q.Pools[0] = s.Pools[0].Clone()
		m := q.Pools[0].MultiAsset.CurveMeta
		switch bad {
		case "block":
			m.BlockNumber++
		case "time":
			m.Timestamp++
		case "hash":
			m.BlockHash = "0x" + strings.Repeat("b", 64)
		}
		if q.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
			t.Fatal("foreign Meta source admitted", bad)
		}
	}
}
