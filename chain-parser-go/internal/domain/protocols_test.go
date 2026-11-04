package domain

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestDexVersionProfilesCannotMixFeeLayoutOrIdentity(t *testing.T) {
	for _, name := range []string{"pancake-v1", "pancake-v2", "pancake-v3", "sushiswap-v2"} {
		t.Run(name, func(t *testing.T) {
			profile, ok := Protocol(name)
			if !ok {
				t.Fatal("missing profile")
			}
			raw, e := os.ReadFile("../../test/contracts/v3-state.json")
			if e != nil {
				t.Fatal(e)
			}
			var p Pool
			if e = json.Unmarshal(raw, &p); e != nil {
				t.Fatal(e)
			}
			p.Protocol = name
			p.AdapterVersion = profile.AdapterVersion
			if profile.Family == "v2" {
				p.V3 = nil
				p.V2 = &V2State{Factory: "0x" + strings.Repeat("4", 40), CodeSHA256: strings.Repeat("a", 64)}
				p.Reserve0 = "1000000"
				p.Reserve1 = "1000000"
				p.FeeNumerator = profile.FeeNumerator
				p.FeeDenominator = profile.FeeDenominator
			}
			if e = p.Validate(); e != nil {
				t.Fatal(e)
			}
			p.AdapterVersion = "uniswap-v2/1"
			if p.Validate() == nil {
				t.Fatal("wrong version accepted")
			}
			p.AdapterVersion = profile.AdapterVersion
			if profile.Family == "v2" {
				p.FeeNumerator++
				if p.Validate() == nil {
					t.Fatal("wrong fee accepted")
				}
				p.FeeNumerator--
				p.V2 = nil
				if p.Validate() == nil {
					t.Fatal("missing code/factory accepted")
				}
			}
		})
	}
	if _, ok := Protocol("pancake-v4"); ok {
		t.Fatal("unsupported version guessed")
	}
}

func TestVersionedStatesKeepFactoryAndFeeProtocolInWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s Snapshot
	if e = json.Unmarshal(raw, &s); e != nil {
		t.Fatal(e)
	}
	p := s.Pools[0]
	p.PoolID = "0x" + strings.Repeat("3", 40)
	p.Protocol = "pancake-v2"
	p.AdapterVersion = "pancake-v2/core-1"
	p.FeeNumerator = 25
	p.FeeDenominator = 10000
	p.V2 = &V2State{Factory: "0x" + strings.Repeat("4", 40), CodeSHA256: strings.Repeat("a", 64)}
	s.Pools = []Pool{p}
	s.Reference.ScopeHash, _ = ScopeHash(s.Pools)
	w, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	got, e := SnapshotFromProto(w)
	if e != nil || got.Pools[0].V2 == nil || *got.Pools[0].V2 != *p.V2 {
		t.Fatal(got, e)
	}
	raw, e = os.ReadFile("../../test/contracts/v3-state.json")
	if e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(raw, &p); e != nil {
		t.Fatal(e)
	}
	p.V2 = nil
	p.Protocol = "pancake-v3"
	p.AdapterVersion = "pancake-v3/core-1"
	p.V3.FeeProtocol = 209718400
	s.Pools = []Pool{p}
	s.Reference.ScopeHash, _ = ScopeHash(s.Pools)
	w, e = SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	got, e = SnapshotFromProto(w)
	if e != nil || got.Pools[0].V3.FeeProtocol != 209718400 {
		t.Fatal(got, e)
	}
}

func TestEVMVersionCannotBeRelabelledAsSolana(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/v3-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(raw, &p); e != nil {
		t.Fatal(e)
	}
	p.ChainKey = "solana:localnet"
	p.Protocol = "pancake-v3"
	p.AdapterVersion = "pancake-v3/core-1"
	if p.Validate() == nil {
		t.Fatal("EVM protocol accepted on Solana state")
	}
}

func TestNamedPoolAddressAliasesRejectedAtWireBoundary(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/v3-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(raw, &p); e != nil {
		t.Fatal(e)
	}
	p.Protocol = "pancake-v2"
	p.AdapterVersion = "pancake-v2/core-1"
	p.V3 = nil
	p.V2 = &V2State{Factory: "0x" + strings.Repeat("4", 40), CodeSHA256: strings.Repeat("a", 64)}
	p.FeeNumerator = 25
	p.FeeDenominator = 10000
	p.Reserve0 = "1000000"
	p.Reserve1 = "1000000"
	p.PoolID = "0x" + strings.Repeat("ab", 20)
	if e = p.Validate(); e != nil {
		t.Fatal(e)
	}
	p.PoolID = "0x" + strings.Repeat("AB", 20)
	if p.Validate() == nil {
		t.Fatal("same EVM address alias accepted")
	}
}

func TestLegacyUniswapAddressShapedIDCannotBypassAliasGuard(t *testing.T) {
	raw, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s Snapshot
	if e = json.Unmarshal(raw, &s); e != nil {
		t.Fatal(e)
	}
	p := s.Pools[0]
	p.PoolID = "0x" + strings.Repeat("AB", 20)
	if p.Validate() == nil {
		t.Fatal("legacy label bypassed physical address guard")
	}
	p.PoolID = "symbolic-offline-fixture"
	if e = p.Validate(); e != nil {
		t.Fatal("existing symbolic fixture invalidated", e)
	}
}
