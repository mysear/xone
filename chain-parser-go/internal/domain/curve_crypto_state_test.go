package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func cryptoPoolJSON(t *testing.T) map[string]any {
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
	for _, a := range assets {
		a.(map[string]any)["scaling"] = "1000000000000000000"
		a.(map[string]any)["balance"] = "1000000000000000000000000"
	}
	s["assets"] = assets
	s["weights"] = []any{}
	s["amplification"] = "400000"
	s["admin_fee"] = uint64(5000000000)
	s["vault"] = ""
	s["vault_code_sha256"] = ""
	s["pool_id"] = ""
	p["protocol"] = "curve-crypto2-full"
	p["adapter_version"] = "curve-crypto2-full/d7d04cd9-standard-1"
	p["fee_numerator"] = uint64(3000000)
	p["fee_denominator"] = uint64(10000000000)
	p["reserve0"] = "1000000000000000000000000"
	p["reserve1"] = p["reserve0"]
	s["curve_crypto"] = map[string]any{"gamma": "10000000000", "out_fee": "30000000", "fee_gamma": "230000000000000", "allowed_extra_profit": "1000000000000000000", "adjustment_step": "146000000000000", "ma_half_time": uint64(600), "invariant": "2000000000000000000000000", "price_scale": "1000000000000000000", "price_oracle": "1001000000000000000", "last_prices": "999000000000000000", "last_prices_timestamp": uint64(1767225600), "xcp_profit": "1000000000000000000", "xcp_profit_a": "1000000000000000000", "virtual_price": "1000000000000000000", "not_adjusted": false, "future_a_gamma_time": uint64(0), "lp_token": "0x0000000000000000000000000000000000000099", "lp_token_code_sha256": CurveCryptoLPCodeSHA256, "total_supply": "1000000000000000000000000", "admin_receiver": "0x0000000000000000000000000000000000000098", "admin_receiver_balance": "0", "raw_balances": []any{p["reserve0"], p["reserve1"]}, "timestamp": uint64(1767225600), "block_number": uint64(100), "block_hash": "0x" + strings.Repeat("a", 64)}
	return p
}
func TestCryptoFullStateRequiresVersionSourceAndCompleteLedger(t *testing.T) {
	parse := func(raw map[string]any) Pool {
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
	if e := parse(cryptoPoolJSON(t)).Validate(); e != nil {
		t.Fatal("full Crypto state missing", e)
	}
	for _, bad := range []string{"missing", "gamma", "fee", "half-time", "scale", "supply", "receiver-balance", "raw-length", "raw-shortfall", "future-time", "time", "legacy", "lp"} {
		t.Run(bad, func(t *testing.T) {
			raw := cryptoPoolJSON(t)
			s := raw["multi_asset"].(map[string]any)
			c := s["curve_crypto"].(map[string]any)
			switch bad {
			case "missing":
				delete(s, "curve_crypto")
			case "gamma":
				c["gamma"] = "1"
			case "fee":
				c["out_fee"] = "10000000001"
			case "half-time":
				c["ma_half_time"] = 0
			case "scale":
				c["price_scale"] = "0"
			case "supply":
				c["total_supply"] = "0"
			case "receiver-balance":
				c["admin_receiver_balance"] = "2000000000000000000000000"
			case "raw-length":
				c["raw_balances"] = []any{"0"}
			case "raw-shortfall":
				c["raw_balances"].([]any)[0] = "0"
			case "future-time":
				c["last_prices_timestamp"] = uint64(1767225601)
			case "time":
				c["timestamp"] = 0
			case "legacy":
				raw["protocol"] = "curve-plain3-basic"
				raw["adapter_version"] = "curve-plain3-basic/factory-1"
			case "lp":
				c["lp_token"] = raw["token0"]
			}
			if parse(raw).Validate() == nil {
				t.Fatal("invalidCrypto admitted", bad)
			}
		})
	}
}

func TestCryptoFullIndependentWireCloneAndSourceBank(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-crypto-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot Snapshot `json:"snapshot"`
		ProtoHex string   `json:"protobuf_hex"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	s := f.Snapshot
	if e = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e != nil {
		t.Fatal(e)
	}
	wire, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	got, e := proto.MarshalOptions{Deterministic: true}.Marshal(wire)
	if e != nil {
		t.Fatal(e)
	}
	want, e := hex.DecodeString(f.ProtoHex)
	if e != nil {
		t.Fatal(e)
	}
	var expected pb.OnlineSnapshot
	if e = proto.Unmarshal(want, &expected); e != nil {
		t.Fatal(e)
	}
	if !proto.Equal(wire, &expected) {
		t.Fatal("independent protobuf differs", hex.EncodeToString(got))
	}
	decoded, e := SnapshotFromProto(&expected)
	if e != nil {
		t.Fatal(e)
	}
	if !reflect.DeepEqual(decoded, s) {
		t.Fatal("wire loses complete Crypto ledger")
	}
	clone := s.Pools[0].Clone()
	clone.MultiAsset.CurveCrypto.RawBalances[0] = "0"
	if s.Pools[0].MultiAsset.CurveCrypto.RawBalances[0] == "0" {
		t.Fatal("shared ledger alias")
	}
	for _, bad := range []string{"number", "hash", "time", "overflow"} {
		t.Run(bad, func(t *testing.T) {
			q := s
			q.Pools = []Pool{s.Pools[0].Clone()}
			c := q.Pools[0].MultiAsset.CurveCrypto
			switch bad {
			case "number":
				c.BlockNumber++
			case "hash":
				c.BlockHash = "0x" + strings.Repeat("b", 64)
			case "time":
				c.Timestamp++
			case "overflow":
				c.Timestamp = ^uint64(0)/1000 + 1
			}
			if q.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
				t.Fatal("foreign Crypto state bank admitted")
			}
		})
	}
	missing := proto.Clone(&expected).(*pb.OnlineSnapshot)
	missing.Pools[0].MultiAsset.CurveCrypto.Gamma = nil
	if _, e = SnapshotFromProto(missing); e == nil {
		t.Fatal("missing Gamma admitted")
	}
}

func TestCryptoAdminMintCannotMutateAnotherQuotedPoolBalance(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-crypto-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot Snapshot `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	s := f.Snapshot
	p := s.Pools[0].Clone()
	q := p.Clone()
	q.PoolID = "0x0000000000000000000000000000000000000020"
	q.MultiAsset.CurveCrypto.LPToken = "0x0000000000000000000000000000000000000097"
	q.MultiAsset.Assets[0].Token = p.MultiAsset.CurveCrypto.LPToken
	q.MultiAsset.Assets[0].CodeSHA256 = p.MultiAsset.CurveCrypto.LPTokenCodeSHA256
	q.Token0 = q.MultiAsset.Assets[0].Token
	s.Pools = []Pool{p, q}
	s.Reference.ScopeHash, _ = ScopeHash(s.Pools)
	if e = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e != nil {
		t.Fatal("baseline dependency is valid", e)
	}
	s.Pools[0].MultiAsset.CurveCrypto.AdminReceiver = q.PoolID
	s.Pools[0].MultiAsset.CurveCrypto.AdminReceiverBalance = q.MultiAsset.CurveCrypto.RawBalances[0]
	if e = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e == nil || !strings.Contains(e.Error(), "CURVE_CRYPTO_MUTABLE_LP_DEPENDENCY") {
		t.Fatal("admin LP mint into another candidate pool admitted", e)
	}
}

func TestCryptoZeroAdminFeeDoesNotCreateMutableLPDependency(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-crypto-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot Snapshot `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	s := f.Snapshot
	p := s.Pools[0].Clone()
	q := p.Clone()
	q.PoolID = "0x0000000000000000000000000000000000000020"
	q.MultiAsset.CurveCrypto.LPToken = "0x0000000000000000000000000000000000000097"
	q.MultiAsset.Assets[0].Token = p.MultiAsset.CurveCrypto.LPToken
	q.MultiAsset.Assets[0].CodeSHA256 = p.MultiAsset.CurveCrypto.LPTokenCodeSHA256
	q.Token0 = q.MultiAsset.Assets[0].Token
	p.MultiAsset.CurveCrypto.AdminReceiver = q.PoolID
	p.MultiAsset.CurveCrypto.AdminReceiverBalance = q.MultiAsset.CurveCrypto.RawBalances[0]
	p.MultiAsset.AdminFee = 0
	s.Pools = []Pool{p, q}
	s.Reference.ScopeHash, _ = ScopeHash(s.Pools)
	if e = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e != nil {
		t.Fatal(e)
	}
}

func TestCryptoRampSchedulesAndWire(t *testing.T) {
	for _, times := range [][2]uint64{{0, 0}, {0, 1}, {1767225590, 1767225610}, {1767225590, 1767225600}, {1767225600, 1767225600}} {
		raw := cryptoPoolJSON(t)
		raw["protocol"] = "curve-crypto2-ramp"
		raw["adapter_version"] = "curve-crypto2-ramp/d7d04cd9-ramp-1"
		c := raw["multi_asset"].(map[string]any)["curve_crypto"].(map[string]any)
		c["future_a_gamma_time"] = times[1]
		c["ramp"] = map[string]any{"initial_amp": "400000", "initial_gamma": "10000000000", "initial_time": times[0]}
		b, _ := json.Marshal(raw)
		var p Pool
		if e := json.Unmarshal(b, &p); e != nil {
			t.Fatal(e)
		}
		if e := p.Validate(); e != nil {
			t.Fatal(e)
		}
		w, e := CurveCryptoToProto(p.MultiAsset.CurveCrypto)
		if e != nil {
			t.Fatal(e)
		}
		decoded, e := CurveCryptoFromProto(w)
		if e != nil || !reflect.DeepEqual(decoded, p.MultiAsset.CurveCrypto) {
			t.Fatal("wire origin lost", e)
		}
		encoded, _ := json.Marshal(decoded)
		var round map[string]any
		json.Unmarshal(encoded, &round)
		want, _ := json.Marshal(c["ramp"])
		got, _ := json.Marshal(round["ramp"])
		if string(want) != string(got) {
			t.Fatal("JSON origin lost", string(got))
		}
	}
}
func TestCryptoRampInvalidOrigins(t *testing.T) {
	for bad := 0; bad < 12; bad++ {
		raw := cryptoPoolJSON(t)
		raw["protocol"] = "curve-crypto2-ramp"
		raw["adapter_version"] = "curve-crypto2-ramp/d7d04cd9-ramp-1"
		c := raw["multi_asset"].(map[string]any)["curve_crypto"].(map[string]any)
		r := map[string]any{"initial_amp": "400000", "initial_gamma": "10000000000", "initial_time": uint64(0)}
		c["ramp"] = r
		switch bad {
		case 0:
			delete(c, "ramp")
		case 1:
			r["initial_time"] = uint64(1767225601)
		case 2:
			r["initial_time"] = uint64(1767225600)
			c["future_a_gamma_time"] = uint64(1767225599)
		case 3:
			c["future_a_gamma_time"] = ^uint64(0)/1000 + 1
		case 4:
			r["initial_amp"] = "3999"
		case 5:
			r["initial_amp"] = "4000000001"
		case 6:
			r["initial_gamma"] = "9999999999"
		case 7:
			r["initial_gamma"] = "20000000000000001"
		case 8:
			r["initial_amp"] = "340282366920938463463374607431768211456"
		case 9:
			r["initial_gamma"] = "340282366920938463463374607431768211456"
		case 10:
			raw["protocol"] = "curve-crypto2-full"
			raw["adapter_version"] = "curve-crypto2-full/d7d04cd9-standard-1"
		case 11:
			r["initial_time"] = ^uint64(0)/1000 + 1
		}
		b, _ := json.Marshal(raw)
		var p Pool
		if e := json.Unmarshal(b, &p); e != nil {
			t.Fatal(e)
		}
		if e := p.Validate(); e == nil {
			t.Fatalf("invalid ramp admitted %d", bad)
		}
	}
}

func TestCryptoRampPublicServiceAndIndependentWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-crypto-ramp-service.json")
	if e != nil {
		t.Fatal(e)
	}
	var service struct {
		Cases []struct {
			Pool Pool `json:"pool"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &service); e != nil {
		t.Fatal(e)
	}
	if len(service.Cases) != 28 {
		t.Fatal("incomplete public corpus")
	}
	for _, row := range service.Cases {
		if e = row.Pool.Validate(); e != nil {
			t.Fatal(e)
		}
	}
	raw, e = os.ReadFile("../../test/contracts/curve-crypto-ramp-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot Snapshot `json:"snapshot"`
		ProtoHex string   `json:"protobuf_hex"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	s := f.Snapshot
	if e = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e != nil {
		t.Fatal(e)
	}
	want, e := hex.DecodeString(f.ProtoHex)
	if e != nil {
		t.Fatal(e)
	}
	var w pb.OnlineSnapshot
	if e = proto.Unmarshal(want, &w); e != nil {
		t.Fatal(e)
	}
	decoded, e := SnapshotFromProto(&w)
	if e != nil || !reflect.DeepEqual(decoded, s) {
		t.Fatal("wire origin lost", e)
	}
	encoded, e := SnapshotToProto(s)
	if e != nil || !proto.Equal(encoded, &w) {
		t.Fatal("independent wire differs", e)
	}
	clone := s.Pools[0].Clone()
	clone.MultiAsset.CurveCrypto.Ramp.InitialAmp = "4000"
	if reflect.DeepEqual(clone.MultiAsset.CurveCrypto.Ramp, s.Pools[0].MultiAsset.CurveCrypto.Ramp) {
		t.Fatal("ramp aliases source")
	}
	for bad := 0; bad < 3; bad++ {
		q := proto.Clone(&w).(*pb.OnlineSnapshot)
		r := q.Pools[0].MultiAsset.CurveCrypto.Ramp
		switch bad {
		case 0:
			r.InitialAmp = nil
		case 1:
			r.InitialGamma = nil
		case 2:
			r.InitialAmp.Uint256Be = make([]byte, 33)
		}
		if _, e = SnapshotFromProto(q); e == nil {
			t.Fatalf("malformed ramp wire %d", bad)
		}
	}
	legacy := cryptoPoolJSON(t)
	raw, _ = json.Marshal(legacy)
	var p Pool
	json.Unmarshal(raw, &p)
	raw, _ = json.Marshal(p.MultiAsset.CurveCrypto)
	var value map[string]any
	json.Unmarshal(raw, &value)
	if _, exists := value["ramp"]; exists {
		t.Fatal("legacy JSON changed")
	}
}
