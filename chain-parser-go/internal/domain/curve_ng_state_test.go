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

func TestCurveNgRequiresItsOwnFullStateAndVersion(t *testing.T) {
	if _, ok := Protocol("curve-stableswap-ng"); !ok {
		t.Fatal("NG live profile not implemented")
	}
	raw := `{"chain_key":"evm:31337","pool_id":"0x0000000000000000000000000000000000000010","protocol":"curve-stableswap-ng","token0":"0x0000000000000000000000000000000000000001","token1":"0x0000000000000000000000000000000000000002","reserve0":"1000000","reserve1":"1000000","fee_numerator":4000000,"fee_denominator":10000000000,"adapter_version":"curve-stableswap-ng/3332cd44-standard-1","token_behavior":"standard","multi_asset":{"code_sha256":"DIGEST","vault":"","vault_code_sha256":"","pool_id":"","assets":[{"token":"0x0000000000000000000000000000000000000001","balance":"1000000","scaling":"1000000000000000000","code_sha256":"DIGEST"},{"token":"0x0000000000000000000000000000000000000002","balance":"1000000","scaling":"1000000000000000000","code_sha256":"DIGEST"}],"weights":[],"amplification":"10000","admin_fee":5000000000}}
`
	var p Pool
	if e := json.Unmarshal([]byte(strings.ReplaceAll(raw, "DIGEST", strings.Repeat("a", 64))), &p); e != nil {
		t.Fatal(e)
	}
	if p.Validate() == nil {
		t.Fatal("incomplete NG oracle state admitted")
	}
}

func curveNgPool(t *testing.T) Pool {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/curve-ng-service-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var d struct {
		Cases []struct {
			Pool Pool `json:"pool"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &d); e != nil {
		t.Fatal(e)
	}
	return d.Cases[0].Pool
}
func TestCurveNgCompleteAccountingAndOracleDomain(t *testing.T) {
	p := curveNgPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	q := p.Clone()
	q.MultiAsset.CurveNG.AdminBalances[0] = "0"
	q.MultiAsset.CurveNG.LastPricesPacked[0] = "1"
	if p.MultiAsset.CurveNG.AdminBalances[0] == "0" || p.MultiAsset.CurveNG.LastPricesPacked[0] == "1" {
		t.Fatal("NG oracle clone alias")
	}
	for _, mutate := range []func(*Pool){func(p *Pool) { p.MultiAsset.CurveNG = nil }, func(p *Pool) { p.MultiAsset.CurveNG.AdminBalances[0] = "0" }, func(p *Pool) { p.MultiAsset.CurveNG.LastPricesPacked = nil }, func(p *Pool) { p.MultiAsset.CurveNG.MAExpTime = 0 }, func(p *Pool) { p.MultiAsset.CurveNG.BlockHash = "0x" + strings.Repeat("0", 64) }, func(p *Pool) { p.MultiAsset.CurveNG.Timestamp = 0 }, func(p *Pool) { p.MultiAsset.CurveNG.OffpegFeeMultiplier = "999999999999999999999999" }, func(p *Pool) { p.Protocol = "curve-plain3-basic"; p.AdapterVersion = "curve-plain3-basic/factory-1" }, func(p *Pool) { p.FeeNumerator = 5000000001 }} {
		q := p.Clone()
		mutate(&q)
		if q.Validate() == nil {
			t.Fatal("bad NG snapshot admitted", q)
		}
	}
}
func TestIndependentCurveNgStateProtoAndSourceBank(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-ng-service-wire.json")
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
	s, e := SnapshotFromProto(&message)
	if e != nil {
		t.Fatal(e)
	}
	a, _ := Hash(d.Snapshot)
	b, _ := Hash(s)
	if a != b {
		t.Fatal("NG full source state lost")
	}
	w, e := SnapshotToProto(s)
	if e != nil || !proto.Equal(w, &message) {
		t.Fatal("NG protobuf lost", e)
	}
	bad := s
	bad.Pools = append([]Pool{}, s.Pools...)
	bad.Pools[0] = s.Pools[0].Clone()
	bad.Pools[0].MultiAsset.CurveNG.BlockNumber++
	if bad.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
		t.Fatal("another bank admitted")
	}
	bad = s
	bad.Pools = append([]Pool{}, s.Pools...)
	bad.Pools[0] = s.Pools[0].Clone()
	bad.Pools[0].MultiAsset.CurveNG.Timestamp++
	if bad.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
		t.Fatal("another timestamp admitted")
	}
}
