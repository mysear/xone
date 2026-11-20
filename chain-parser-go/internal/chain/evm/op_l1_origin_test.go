package evm

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"os"
	"testing"

	"xone/chain-parser-go/internal/provenance"
)

func readOriginFixture(t *testing.T, name string, target any) {
	t.Helper()
	f, err := os.Open("../../../../integration/test/source/" + name)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		t.Fatal(err)
	}
	defer gz.Close()
	if err = json.NewDecoder(gz).Decode(target); err != nil {
		t.Fatal(err)
	}
}

func TestOPHistoricalSignedOrigin(t *testing.T) {
	var archived provenance.OnlineBeaconProof
	readOriginFixture(t, "beacon-fork-handover/electra-fulu.json.gz", &archived)
	proof, err := provenance.VerifyExecutionFinality("0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c", archived.Bootstrap, archived.Finality, archived.Updates)
	if err != nil {
		t.Fatal(err)
	}
	var evidence OPOriginEvidence
	readOriginFixture(t, "op-mainnet-withdrawals/origin-144644465.json.gz", &evidence)
	ctx := context.Background()
	report, err := VerifyOPOrigin(ctx, proof, evidence)
	if err != nil || report.Origin.Number != 23942644 || report.Origin.Hash != "0x3259dfa10a19ae66f8e819ef1790b178fdcf98ffd0dfc7d97565079d26ff50a6" || report.OPBlockNumber != 144644465 || report.FinalizedL1Number != 23942645 {
		t.Fatal(report, err)
	}
	for _, tc := range []struct {
		name    string
		headers []json.RawMessage
		want    string
	}{
		{"missing origin", evidence.Headers[:1], "OP_ORIGIN_ANCESTRY_WINDOW"},
		{"repeated tip", []json.RawMessage{evidence.Headers[0], evidence.Headers[0]}, "OP_ORIGIN_ANCESTRY_LINK"},
		{"wrong tip", []json.RawMessage{evidence.Headers[1], evidence.Headers[0]}, "OP_ORIGIN_ANCESTRY_LINK"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			bad := evidence
			bad.Headers = tc.headers
			if out, e := VerifyOPOrigin(ctx, proof, bad); e == nil || e.Error() != tc.want || out.Authority != "" {
				t.Fatal(out, e)
			}
		})
	}
}
