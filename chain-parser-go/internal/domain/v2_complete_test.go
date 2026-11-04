package domain

import (
	"bytes"
	"encoding/json"
	"google.golang.org/protobuf/reflect/protoreflect"
	"math/big"
	"reflect"
	"strings"
	"testing"
)

func completeV2Example() Pool {
	return Pool{ChainKey: "evm:31337", PoolID: "0x" + strings.Repeat("1", 40), Protocol: V2CompleteProtocol, Token0: "0x" + strings.Repeat("2", 40), Token1: "0x" + strings.Repeat("3", 40), Reserve0: "1000", Reserve1: "2000", FeeNumerator: 3, FeeDenominator: 1000, AdapterVersion: V2CompleteAdapterVersion, TokenBehavior: "standard", V2: &V2State{Factory: "0x" + strings.Repeat("4", 40), CodeSHA256: V2CompletePairCodeSHA256, Swap: &V2SwapState{RawBalance0: "1050", RawBalance1: "2000", BlockTimestampLast: 1790812700, Price0CumulativeLast: "340282366920938463463374607431768211456", Price1CumulativeLast: "680564733841876926926749214863536422912", Unlocked: true, FactoryCodeSHA256: V2CompleteFactoryCodeSHA256, BankBlockNumber: 12, BankBlockHash: "0x" + strings.Repeat("a", 64), BankUnixTimestamp: 1790812800}}}
}
func completeV2Snapshot() Snapshot {
	p := completeV2Example()
	scope, _ := ScopeHash([]Pool{p})
	adapters, _ := Hash([]string{p.AdapterVersion})
	return Snapshot{Reference: SnapshotRef{Block: BlockRef{ChainKey: p.ChainKey, Number: 12, Hash: p.V2.Swap.BankBlockHash, ParentHash: "0x" + strings.Repeat("b", 64), TimestampMS: 1790812800000}, CanonicalEpoch: 1, JournalSeq: 1, SchemaVersion: 1, AdapterSetHash: adapters, OnlineGeneration: 1, ScopeHash: scope, WriterEpoch: 1, OwnerService: "chain-parser-go", OwnerAssignmentVersion: 1}, Pools: []Pool{p}, HistoryGaps: []Gap{}, Source: "PROVIDED_LOCAL_STATE"}
}
func TestV2CompleteUnsyncedFullWidthWireAndLegacyIsolation(t *testing.T) {
	s := completeV2Snapshot()
	if e := s.Validate("chain-parser-go", 1); e != nil {
		t.Fatal(e)
	}
	clone := s.Pools[0].Clone()
	clone.V2.Swap.RawBalance0 = "0"
	if s.Pools[0].V2.Swap.RawBalance0 != "1050" {
		t.Fatal("clone shares mutable swap state")
	}
	w, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	decoded, e := SnapshotFromProto(w)
	if e != nil {
		t.Fatal(e)
	}
	if !reflect.DeepEqual(decoded, s) {
		t.Fatal("full state/wire lost")
	}
	for _, name := range []string{"uniswap-v2", "pancake-v2", "sushiswap-v2"} {
		p := completeV2Example()
		p.Protocol = name
		profile, _ := Protocol(name)
		p.AdapterVersion = profile.AdapterVersion
		if profile.FeeDenominator != 0 {
			p.FeeNumerator = profile.FeeNumerator
			p.FeeDenominator = profile.FeeDenominator
		}
		if p.Validate() == nil {
			t.Fatal("legacy admitted complete state", name)
		}
		p.V2.Swap = nil
		if e := p.Validate(); e != nil {
			t.Fatal("legacy changed", name, e)
		}
		b, _ := json.Marshal(p.V2)
		if strings.Contains(string(b), "swap") {
			t.Fatal("old JSON expanded")
		}
	}
}
func TestV2CompleteRejectsMissingUnknownWidthsCodesLocksAndBankConflict(t *testing.T) {
	mutations := map[string]func(*Pool){"missing": func(p *Pool) { p.V2.Swap = nil }, "pair": func(p *Pool) { p.V2.CodeSHA256 = strings.Repeat("a", 64) }, "factory": func(p *Pool) { p.V2.Swap.FactoryCodeSHA256 = strings.Repeat("a", 64) }, "locked": func(p *Pool) { p.V2.Swap.Unlocked = false }, "reserve": func(p *Pool) { p.Reserve0 = Amount(new(big.Int).Lsh(big.NewInt(1), 112).String()) }, "raw": func(p *Pool) { p.V2.Swap.RawBalance0 = Amount(new(big.Int).Lsh(big.NewInt(1), 256).String()) }, "hash": func(p *Pool) { p.V2.Swap.BankBlockHash = "0x00" }, "time": func(p *Pool) { p.V2.Swap.BankUnixTimestamp = ^uint64(0) }, "behavior": func(p *Pool) { p.TokenBehavior = "fee-on-transfer" }, "adapter": func(p *Pool) { p.AdapterVersion = "uniswap-v2/1" }}
	for name, mutate := range mutations {
		t.Run(name, func(t *testing.T) {
			p := completeV2Example()
			mutate(&p)
			if p.Validate() == nil {
				t.Fatal("invalid state admitted")
			}
		})
	}
	for bad := 0; bad < 3; bad++ {
		s := completeV2Snapshot()
		switch bad {
		case 0:
			s.Reference.Block.Number++
		case 1:
			s.Reference.Block.Hash = "0x" + strings.Repeat("c", 64)
		default:
			s.Reference.Block.TimestampMS++
		}
		if s.Validate("chain-parser-go", 1) == nil {
			t.Fatal("bank conflict", bad)
		}
	}
}

func TestV2CompleteMandatoryJSONAndWireValuesCannotDefaultUnknownToZero(t *testing.T) {
	fields := []string{"raw_balance0", "raw_balance1", "block_timestamp_last", "price0_cumulative_last", "price1_cumulative_last", "unlocked", "factory_code_sha256", "bank_block_number", "bank_block_hash", "bank_unix_timestamp"}
	for _, name := range fields {
		raw, _ := json.Marshal(completeV2Example())
		var shape map[string]any
		json.Unmarshal(raw, &shape)
		delete(shape["v2"].(map[string]any)["swap"].(map[string]any), name)
		raw, _ = json.Marshal(shape)
		var p Pool
		if e := json.Unmarshal(raw, &p); e == nil && p.Validate() == nil {
			t.Fatalf("missing JSON %s became a known zero", name)
		}
	}
	for _, name := range []string{"block_timestamp_last", "bank_block_number", "bank_unix_timestamp"} {
		w, e := V2SwapToProto(completeV2Example().V2.Swap)
		if e != nil {
			t.Fatal(e)
		}
		message := w.ProtoReflect()
		field := message.Descriptor().Fields().ByName(protoreflect.Name(name))
		message.Clear(field)
		if _, e := V2SwapFromProto(w); e == nil {
			t.Fatal("missing wire value became a known zero", name)
		}
	}
	s := completeV2Snapshot()
	s.Reference.Block.Number = 0
	s.Reference.Block.TimestampMS = 0
	s.Pools[0].V2.Swap.BankBlockNumber = 0
	s.Pools[0].V2.Swap.BankUnixTimestamp = 0
	s.Pools[0].V2.Swap.BlockTimestampLast = 0
	w, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	v, e := SnapshotFromProto(w)
	if e != nil || !reflect.DeepEqual(v, s) {
		t.Fatal("explicit zero must remain known", e)
	}
}

func TestV2CompleteJSONRejectsUnknownCaseAliasesAndDuplicateAmounts(t *testing.T) {
	base, e := json.Marshal(completeV2Example())
	if e != nil {
		t.Fatal(e)
	}
	for _, replacement := range []string{`"raw_balance0":"1050","RAW_BALANCE0":"0"`, `"RAW_BALANCE0":"0","raw_balance0":"1050"`, `"raw_balance0":"1050","Raw_Balance0":"0"`, `"raw_balance0":"1050","raw_balance0":"0"`, `"raw_balance0":"1050","unexpected":"0"`} {
		raw := bytes.Replace(base, []byte(`"raw_balance0":"1050"`), []byte(replacement), 1)
		var p Pool
		if e := json.Unmarshal(raw, &p); e == nil {
			t.Fatalf("alias/unknown or duplicate field admitted: %s", replacement)
		}
	}
}

func TestV2CompleteWireOptionalFieldsOwnIndependentCopies(t *testing.T) {
	s := completeV2Snapshot()
	source := s.Pools[0].V2.Swap
	expected := *source
	w, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	converted := w.Pools[0].V2.Swap
	*converted.BlockTimestampLast = 0
	*converted.BankBlockNumber = 0
	*converted.BankUnixTimestamp = 0
	if *source != expected {
		t.Fatal("protobuf optional pointers mutated the source snapshot")
	}
	s = completeV2Snapshot()
	source = s.Pools[0].V2.Swap
	expected = *source
	w, e = SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	converted = w.Pools[0].V2.Swap
	source.BlockTimestampLast = 1
	source.BankBlockNumber = 2
	source.BankUnixTimestamp = 3
	if *converted.BlockTimestampLast != expected.BlockTimestampLast || *converted.BankBlockNumber != expected.BankBlockNumber || *converted.BankUnixTimestamp != expected.BankUnixTimestamp {
		t.Fatal("source mutation changed an already converted message")
	}
}
