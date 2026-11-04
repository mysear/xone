package domain

import (
	"bytes"
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func raydiumTestPool(t *testing.T) Pool {
	t.Helper()
	b, e := os.ReadFile("../../test/contracts/raydium-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestRaydiumCreatorStateHasOwnIdentityAndWire(t *testing.T) {
	p := raydiumTestPool(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	w, e := RaydiumCPMMToProto(p.RaydiumCPMM)
	if e != nil {
		t.Fatal(e)
	}
	s, e := RaydiumCPMMFromProto(w)
	if e != nil || *s != *p.RaydiumCPMM {
		t.Fatal("creator state lost", e)
	}
	clone := p.Clone()
	clone.RaydiumCPMM.CreatorFees0++
	if clone.RaydiumCPMM.CreatorFees0 == p.RaydiumCPMM.CreatorFees0 {
		t.Fatal("mutable state alias")
	}
}
func TestRaydiumCreatorRejectsUnknownAccounting(t *testing.T) {
	for _, mode := range []string{"chain", "adapter", "program", "hash", "key", "mode", "fees", "reserve", "bank", "open", "foreign-state"} {
		t.Run(mode, func(t *testing.T) {
			p := raydiumTestPool(t)
			s := p.RaydiumCPMM
			switch mode {
			case "chain":
				p.ChainKey = "evm:31337"
			case "adapter":
				p.AdapterVersion = "raydium-cpmm/archive-v1"
			case "program":
				s.ProgramID = p.Token0
			case "hash":
				s.ProgramCodeSHA256 = "aa"
			case "key":
				s.Vault0 = "1"
			case "mode":
				s.CreatorFeeOn = 3
			case "fees":
				s.FundFeeRate = 1000000
			case "reserve":
				p.Reserve0 = "1000000001"
			case "bank":
				s.DeployedSlot = s.BankSlot + 1
			case "open":
				s.OpenTime = s.BankUnixTimestamp + 1
			case "foreign-state":
				p.Protocol = "fixture-cpmm"
			}
			if p.Validate() == nil {
				t.Fatal("invalid Raydium state accepted", mode)
			}
		})
	}
	p := raydiumTestPool(t)
	p.RaydiumCPMM.ProgramCodeSHA256 = strings.Repeat("A", 64)
	if _, e := RaydiumCPMMToProto(p.RaydiumCPMM); e == nil {
		t.Fatal("invalid digest encoded")
	}
}

func TestRaydiumGlobalAuthorityMustMatchPinnedProgram(t *testing.T) {
	p := raydiumTestPool(t)
	p.RaydiumCPMM.Authority = encode58(bytes.Repeat([]byte{11}, 32))
	p.RaydiumCPMM.AuthorityBump = 252
	if p.Validate() == nil {
		t.Fatal("wrong global PDA accepted")
	}
}

func TestRaydiumProofMetadataCannotMixProgramsOrFutureEpoch(t *testing.T) {
	p := raydiumTestPool(t)
	p.RaydiumCPMM.Programdata = encode58(bytes.Repeat([]byte{12}, 32))
	if p.Validate() == nil {
		t.Fatal("unrelated ProgramData accepted")
	}
	p = raydiumTestPool(t)
	p.RaydiumCPMM.RecentEpoch = p.RaydiumCPMM.BankEpoch + 1
	if p.Validate() == nil {
		t.Fatal("future epoch metadata accepted")
	}
}
