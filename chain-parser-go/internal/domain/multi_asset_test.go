package domain

import (
	"encoding/json"
	"os"
	"testing"
)

func multiFixture(t *testing.T) Pool {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/multi-asset-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var d struct {
		Pools []Pool `json:"pools"`
	}
	if e = json.Unmarshal(raw, &d); e != nil {
		t.Fatal(e)
	}
	return d.Pools[0]
}
func TestMultiAssetPhysicalStateWireAndClone(t *testing.T) {
	p := multiFixture(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	q := p.Clone()
	q.MultiAsset.Assets[2].Balance = "1"
	q.MultiAsset.Weights[0] = "1"
	if p.MultiAsset.Assets[2].Balance == "1" || p.MultiAsset.Weights[0] == "1" {
		t.Fatal("multi asset clone alias")
	}
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	back, e := MultiAssetFromProto(w)
	if e != nil {
		t.Fatal(e)
	}
	a, _ := Hash(p.MultiAsset)
	b, _ := Hash(back)
	if a != b {
		t.Fatal("wire state mismatch", a, b)
	}
	w.Assets[2].Balance.Uint256Be = []byte{1}
	if _, e = MultiAssetFromProto(w); e == nil {
		t.Fatal("short amount accepted")
	}
	for _, mutate := range []func(*Pool){func(p *Pool) { p.Reserve0 = "0" }, func(p *Pool) { p.Token1 = p.Token0 }, func(p *Pool) { p.MultiAsset.Assets[2].Token = p.Token0 }, func(p *Pool) { p.MultiAsset.Weights[2] = "1" }, func(p *Pool) { p.Protocol = "uniswap-v2" }, func(p *Pool) { p.AdapterVersion = "wrong" }, func(p *Pool) { p.MultiAsset.Assets[2].Balance = "5192296858534827628530496329220096" }} {
		q := p.Clone()
		mutate(&q)
		if q.Validate() == nil {
			t.Fatal("invalid physical state admitted", q)
		}
	}
}

func TestMultiAssetClonePreservesRequiredEmptyWeightVector(t *testing.T) {
	p := multiFixture(t)
	p.MultiAsset.Weights = []Amount{}
	clone := p.Clone()
	if clone.MultiAsset.Weights == nil {
		t.Fatal("clone changed [] weights into null")
	}
	a, _ := Hash(p)
	b, _ := Hash(clone)
	if a != b {
		t.Fatal("clone changed canonical state identity")
	}
}
