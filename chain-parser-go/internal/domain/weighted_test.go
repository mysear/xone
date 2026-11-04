package domain

import (
	"encoding/json"
	"math/big"
	"strings"
	"testing"
)

func weightedPool(t *testing.T) Pool {
	t.Helper()
	raw := `{"chain_key":"evm:31337","pool_id":"0x0000000000000000000000000000000000000003","protocol":"balancer-v2-weighted2-equal","token0":"0x0000000000000000000000000000000000000001","token1":"0x0000000000000000000000000000000000000002","reserve0":"1000000000000000000000000","reserve1":"1000000000000","fee_numerator":1000000000000000,"fee_denominator":1000000000000000000,"adapter_version":"balancer-v2-weighted2/equal-1","token_behavior":"standard","weighted":{"code_sha256":"DIGEST","vault":"0x0000000000000000000000000000000000000005","vault_code_sha256":"DIGEST","pool_id":"0x0000000000000000000000000000000000000003000200000000000000000001","scaling0":"1000000000000000000","scaling1":"1000000000000000000000000000000","weight0":"500000000000000000","weight1":"500000000000000000","token0_code_sha256":"DIGEST","token1_code_sha256":"DIGEST"}}`
	var p Pool
	if e := json.Unmarshal([]byte(strings.ReplaceAll(raw, "DIGEST", strings.Repeat("a", 64))), &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestWeightedProfileAndOwnStateRequired(t *testing.T) {
	if _, ok := Protocol("balancer-v2-weighted2-equal"); !ok {
		t.Fatal("missing weighted profile")
	}
	p := weightedPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	// An unknown version must never accept a weighted payload.
	p.Protocol = "balancer-v2-managed"
	if e := p.Validate(); e == nil {
		t.Fatal("weighted payload admitted by unknown profile")
	}
}

func TestWeightedRejectsModelMixingParametersAndMalformedWire(t *testing.T) {
	for _, mutate := range []func(*Pool){
		func(p *Pool) { p.Weighted = nil }, func(p *Pool) { p.Stable = &StableState{} }, func(p *Pool) { p.V2 = &V2State{} }, func(p *Pool) { p.RaydiumCLMM = &RaydiumCLMMState{} },
		func(p *Pool) { p.Weighted.Weight0 = "600000000000000000" }, func(p *Pool) { p.Weighted.Scaling0 = "1000000000000000001" }, func(p *Pool) { p.Weighted.PoolID = p.PoolID + strings.Repeat("0", 24) }, func(p *Pool) { p.Weighted.Token0CodeSHA256 = "" }, func(p *Pool) { p.TokenBehavior = "tax" }, func(p *Pool) { p.FeeNumerator = 999999999999 }, func(p *Pool) { p.FeeNumerator = 100000000000000001 }, func(p *Pool) { p.AdapterVersion = "balancer-v2-stable2/stable-1" }, func(p *Pool) { p.Token0, p.Token1 = p.Token1, p.Token0 }, func(p *Pool) { p.Reserve0 = "5192296858534827628530496329220096" }, func(p *Pool) { p.ChainKey = "solana:localnet" },
	} {
		p := weightedPool(t)
		mutate(&p)
		if p.Validate() == nil {
			t.Fatalf("invalid model admitted: %+v", p)
		}
	}
	p := weightedPool(t)
	w, e := WeightedToProto(p.Weighted)
	if e != nil {
		t.Fatal(e)
	}
	got, e := WeightedFromProto(w)
	if e != nil || *got != *p.Weighted {
		t.Fatal(got, e)
	}
	w.Weight0.Uint256Be = []byte{1}
	if _, e = WeightedFromProto(w); e == nil {
		t.Fatal("short amount accepted")
	}
	w, e = WeightedToProto(p.Weighted)
	if e != nil {
		t.Fatal(e)
	}
	w.PoolId = w.PoolId[1:]
	if _, e = WeightedFromProto(w); e == nil {
		t.Fatal("short id accepted")
	}
}

func TestWeightedCloneDoesNotShareMutableEnvelope(t *testing.T) {
	p := weightedPool(t)
	q := p.Clone()
	q.Weighted.Weight0 = "1"
	q.Weighted.Scaling0 = "1"
	q.Weighted.Vault = "changed"
	q.Weighted.Token0CodeSHA256 = "changed"
	if p.Weighted.Weight0 != "500000000000000000" || p.Weighted.Scaling0 != "1000000000000000000" || p.Weighted.Vault == "changed" || p.Weighted.Token0CodeSHA256 == "changed" {
		t.Fatal("clone mutated source weighted state")
	}
}

func TestWeightedStaticVersionSupportsFullImmutableWeightDomain(t *testing.T) {
	p := weightedPool(t)
	p.Protocol = "balancer-v2-weighted2-static"
	p.AdapterVersion = "balancer-v2-weighted2/static-1"
	p.Weighted.Weight0 = "800000000000000000"
	p.Weighted.Weight1 = "200000000000000000"
	if e := p.Validate(); e != nil {
		t.Fatal("missing independent static weight model", e)
	}
	p.Protocol = "balancer-v2-weighted2-equal"
	p.AdapterVersion = "balancer-v2-weighted2/equal-1"
	if p.Validate() == nil {
		t.Fatal("legacy equal profile widened")
	}
}

func TestWeightedStaticAdmissionAndAdapterMatrix(t *testing.T) {
	for _, w := range []string{"10000000000000000", "10000000000000001", "333333333333333333", "370000000000000000", "500000000000000000", "800000000000000000", "990000000000000000"} {
		p := weightedPool(t)
		p.Protocol = "balancer-v2-weighted2-static"
		p.AdapterVersion = "balancer-v2-weighted2/static-1"
		p.Weighted.Weight0 = Amount(w)
		p.Weighted.Weight1 = Amount(new(big.Int).Sub(big.NewInt(1000000000000000000), p.Weighted.Weight0.Big()).String())
		if e := p.Validate(); e != nil {
			t.Fatal(w, e)
		}
		wire, e := WeightedToProto(p.Weighted)
		if e != nil {
			t.Fatal(e)
		}
		back, e := WeightedFromProto(wire)
		if e != nil || *back != *p.Weighted {
			t.Fatal("static weights lost", e)
		}
		p.AdapterVersion = "balancer-v2-weighted2/equal-1"
		if p.Validate() == nil {
			t.Fatal("crossed adapter accepted")
		}
		p.Protocol = "balancer-v2-weighted2-equal"
		if (p.Validate() == nil) != (w == "500000000000000000") {
			t.Fatal("equal profile domain changed", w)
		}
	}
	for _, w := range []string{"0", "9999999999999999", "990000000000000001"} {
		p := weightedPool(t)
		p.Protocol = "balancer-v2-weighted2-static"
		p.AdapterVersion = "balancer-v2-weighted2/static-1"
		p.Weighted.Weight0 = Amount(w)
		p.Weighted.Weight1 = Amount(new(big.Int).Sub(big.NewInt(1000000000000000000), p.Weighted.Weight0.Big()).String())
		if p.Validate() == nil {
			t.Fatal("bad minimum weight", w)
		}
	}
}
