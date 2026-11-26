package domain

import (
	"strings"
	"testing"
)

func TestCurveMetaBasePoolStateWireRoundTrip(t *testing.T) {
	state := &CurveMetaState{
		Implementation:           "0x1111111111111111111111111111111111111111",
		ImplementationCodeSHA256: CurveMetaRuntimeSHA256,
		BasePool:                 CurveMetaBasePool,
		BasePoolCodeSHA256:       "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
		BaseVirtualPrice:         "1000000000000000000",
		StoredBalances:           []Amount{"1000000000000000000000", "1000000000000000000000"},
		AdminBalances:            []Amount{"0", "0"},
		Timestamp:                1,
		BlockNumber:              2,
		BlockHash:                "0x" + strings.Repeat("1", 64),
		BasePoolState: &CurveMetaBasePoolState{
			Balances:      []Amount{"1000000000000000000000000", "1000000000000000000000000", "1000000000000000000000000"},
			AdminBalances: []Amount{"1", "2", "3"},
			Rates:         []Amount{"1000000000000000000", "1000000000000000000", "1000000000000000000"},
			Amplification: "10000",
			Fee:           4000000,
			AdminFee:      5000000000,
			TotalSupply:   "3000000000000000000000000",
		},
	}
	wire, err := CurveMetaToProto(state)
	if err != nil {
		t.Fatal(err)
	}
	got, err := CurveMetaFromProto(wire)
	if err != nil {
		t.Fatal(err)
	}
	if got.BasePoolState == nil || len(got.BasePoolState.Balances) != 3 || got.BasePoolState.Fee != 4000000 || got.BasePoolState.TotalSupply != state.BasePoolState.TotalSupply {
		t.Fatalf("base state was not preserved: %#v", got.BasePoolState)
	}
}
