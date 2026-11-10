package domain

import (
	"encoding/json"
	"os"
	"reflect"
	"testing"
)

func TestVelodromeOriginalReferenceStatesPreserveVersionAndWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/velodrome-v2-quotes.json")
	if e != nil {
		t.Fatal(e)
	}
	var vectors struct {
		Cases []struct {
			Pool Pool `json:"pool"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &vectors); e != nil {
		t.Fatal(e)
	}
	if len(vectors.Cases) != 216 {
		t.Fatal("incomplete original-reference domain")
	}
	raw, e = os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var base Snapshot
	if e = json.Unmarshal(raw, &base); e != nil {
		t.Fatal(e)
	}
	for _, c := range vectors.Cases {
		p := c.Pool
		if e = p.Validate(); e != nil {
			t.Fatal(e)
		}
		snap := base
		snap.Source = "PROVIDED_LOCAL_STATE"
		snap.Reference.Block.ChainKey = p.ChainKey
		snap.Pools = []Pool{p}
		snap.Reference.ScopeHash, e = ScopeHash(snap.Pools)
		if e != nil {
			t.Fatal(e)
		}
		snap.Reference.AdapterSetHash, e = Hash([]string{p.AdapterVersion})
		if e != nil {
			t.Fatal(e)
		}
		wire, err := SnapshotToProto(snap)
		if err != nil {
			t.Fatal(err)
		}
		actual, err := SnapshotFromProto(wire)
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(actual.Pools, snap.Pools) {
			t.Fatal("Velodrome state or version changed in shared wire")
		}
		forged := p
		forged.AdapterVersion = "aerodrome-v2-stable/pool-1"
		if forged.Validate() == nil {
			t.Fatal("another source version accepted")
		}
	}
}
