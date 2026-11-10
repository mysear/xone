package domain

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestStableProtocolsRequireTheirOwnStateAndParameters(t *testing.T) {
	for _, name := range []string{"velodrome-v2-volatile", "velodrome-v2-stable", "aerodrome-v2-volatile", "aerodrome-v2-stable", "curve-plain2-basic", "balancer-v2-stable2"} {
		t.Run(name, func(t *testing.T) {
			profile, ok := Protocol(name)
			if !ok {
				t.Fatal("missing stable profile")
			}
			p := Pool{ChainKey: "evm:31337", PoolID: "0x" + strings.Repeat("3", 40), Protocol: name, Token0: "A", Token1: "B", Reserve0: "1000000000000000000", Reserve1: "1000000000000000000", FeeNumerator: 4, FeeDenominator: profile.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
			if p.Validate() == nil {
				t.Fatal("missing model state accepted")
			}
			p.Stable = &StableState{CodeSHA256: strings.Repeat("a", 64), Rate0: "1000000000000000000", Rate1: "1000000000000000000", Amplification: "0", MaxIterations: 255}
			if profile.Family == "aero" {
				p.Stable.Factory = "0x" + strings.Repeat("4", 40)
				p.Stable.FactoryCodeSHA256 = strings.Repeat("b", 64)
			}
			if profile.Family == "curve" {
				p.FeeNumerator = 4000000
				p.Stable.Amplification = "10000"
				p.Stable.AdminFee = 5000000000
			}
			if profile.Family == "balancer" {
				p.FeeNumerator = 1000000000000000
				p.Stable.Amplification = "100000"
				p.Stable.Vault = "0x" + strings.Repeat("5", 40)
				p.Stable.VaultCodeSHA256 = strings.Repeat("c", 64)
				p.Stable.PoolID = "0x" + strings.Repeat("d", 64)
			}
			if e := p.Validate(); e != nil {
				t.Fatal(e)
			}
			p.Stable.Rate0 = "0"
			if p.Validate() == nil {
				t.Fatal("unproven scale accepted")
			}
		})
	}
}

func TestStableParametersSurviveSharedWire(t *testing.T) {
	raw, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var snap Snapshot
	if e = json.Unmarshal(raw, &snap); e != nil {
		t.Fatal(e)
	}
	p := snap.Pools[0]
	p.PoolID = "0x" + strings.Repeat("3", 40)
	p.Protocol = "curve-plain2-basic"
	p.AdapterVersion = "curve-plain2-basic/factory-5"
	p.FeeNumerator = 4000000
	p.FeeDenominator = 10000000000
	p.Stable = &StableState{CodeSHA256: strings.Repeat("a", 64), Rate0: "1000000000000000000", Rate1: "1000000000000000000000000000000", Amplification: "10000", AdminFee: 5000000000, MaxIterations: 255}
	snap.Pools = []Pool{p}
	snap.Reference.ScopeHash, _ = ScopeHash(snap.Pools)
	wire, e := SnapshotToProto(snap)
	if e != nil {
		t.Fatal(e)
	}
	got, e := SnapshotFromProto(wire)
	if e != nil || got.Pools[0].Stable == nil || *got.Pools[0].Stable != *p.Stable {
		t.Fatal("stable rates/admin parameters lost", e)
	}
}

func TestStableDomainRejectsContractImpossibleFeeRanges(t *testing.T) {
	for _, item := range []struct {
		protocol string
		fee      uint64
	}{{"curve-plain2-basic", 1}, {"balancer-v2-stable2", 1}} {
		profile, _ := Protocol(item.protocol)
		p := Pool{ChainKey: "evm:31337", PoolID: "0x" + strings.Repeat("3", 40), Protocol: item.protocol, Token0: "A", Token1: "B", Reserve0: "1000000", Reserve1: "1000000", FeeNumerator: item.fee, FeeDenominator: profile.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard", Stable: &StableState{CodeSHA256: strings.Repeat("a", 64), Rate0: "1000000000000000000", Rate1: "1000000000000000000", Amplification: "100000", AdminFee: 5000000000, MaxIterations: 255}}
		if item.protocol == "balancer-v2-stable2" {
			p.Stable.AdminFee = 0
			p.Stable.Vault = "0x" + strings.Repeat("5", 40)
			p.Stable.VaultCodeSHA256 = strings.Repeat("b", 64)
			p.Stable.PoolID = "0x" + strings.Repeat("3", 64)
		}
		if p.Validate() == nil {
			t.Fatal("fee outside concrete contract bounds accepted", item.protocol)
		}
	}
}
