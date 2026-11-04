package domain

import (
	"encoding/json"
	"fmt"
	"os"
	"testing"
)

func TestSingletonAdvancedFrozenIdentity(t *testing.T) {
	raw, err := os.ReadFile("../../test/contracts/singleton-advanced-quotes.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Cases []struct {
			Snapshot SingletonAdvancedState `json:"snapshot"`
		} `json:"cases"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	if len(fixture.Cases) < 240 {
		t.Fatal("missing advanced cases")
	}
	for _, c := range fixture.Cases {
		s := c.Snapshot
		if err = s.Validate(); err != nil {
			t.Fatal(err)
		}
		if s.KeyFee == SingletonDynamicFeeFlag {
			s.Snapshot.LPFee = (s.Snapshot.LPFee + 1) % 1000001
			if err = s.Validate(); err != nil {
				t.Fatal(err)
			}
		}
		s.SnapshotBlockHash = ""
		if s.Validate() == nil {
			t.Fatal("missing anchor accepted")
		}
	}
}

func TestSingletonAdvancedRejectsCallbacksAndAmbiguousFeeFlags(t *testing.T) {
	raw, err := os.ReadFile("../../test/contracts/singleton-advanced-quotes.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Cases []struct {
			Snapshot SingletonAdvancedState `json:"snapshot"`
		} `json:"cases"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	for _, protocol := range []string{"uniswap-v4", "pancake-infinity-cl"} {
		var base SingletonAdvancedState
		for _, c := range fixture.Cases {
			if c.Snapshot.Protocol == protocol && c.Snapshot.KeyFee == SingletonDynamicFeeFlag {
				base = c.Snapshot
				break
			}
		}
		flags := []uint16{0x80, 0x40, 8, 4}
		if protocol == "pancake-infinity-cl" {
			flags = []uint16{0x40, 0x80, 0x400, 0x800}
		}
		for _, flag := range flags {
			bad := base
			if protocol == "uniswap-v4" {
				bad.Snapshot.Hooks = fmt.Sprintf("0x%040x", uint32(0x10000)+uint32(flag))
			} else {
				bad.Snapshot.Parameters = fmt.Sprintf("0x%064x", uint32(60<<16)|uint32(flag))
			}
			bad.Snapshot.PoolID, err = bad.KeyID()
			if err != nil {
				t.Fatal(err)
			}
			if err = bad.Validate(); err == nil || err.Error() != "SINGLETON_HOOK_EXECUTION_REQUIRED" {
				t.Fatalf("callback validation: %v", err)
			}
		}
		for _, fee := range []uint32{0x800001, 0x400000, 1000001} {
			bad := base
			bad.KeyFee = fee
			if _, err = bad.KeyID(); err == nil {
				t.Fatal("ambiguous fee flag accepted")
			}
		}
	}
}
