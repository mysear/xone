package evm

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"os"
	"testing"

	"github.com/ethereum/go-ethereum/params"

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

func TestOPOriginBlobFee(t *testing.T) {
	var input OPOriginEvidence
	readOriginFixture(t, "op-mainnet-batches/portal-origin-26454436.json.gz", &input)
	ctx := context.Background()
	body, err := VerifyChainBlockJSON(ctx, "evm:10", input.Block, 32<<20)
	if err != nil {
		t.Fatal(err)
	}
	ts, err := canonicalIndex(body.Block.Timestamp)
	if err != nil {
		t.Fatal(err)
	}
	origin, err := opBlockOrigin(&body, ts)
	if err != nil || origin.BlobBaseFeeWei != "8298648" {
		t.Fatal(origin, err)
	}
	tip, err := VerifyChainHeaderJSON(ctx, "evm:1", input.Headers[0])
	if err != nil {
		t.Fatal(err)
	}
	height, err := canonicalIndex(tip.Block.Number)
	if err != nil {
		t.Fatal(err)
	}
	read := func(h uint64) (json.RawMessage, error) { return input.Headers[height-h], nil }
	origin.BlobBaseFeeWei = "8298649"
	if err := verifyOriginAncestry(ctx, height, tip.Block.Hash, origin, read); err == nil || err.Error() != "OP_ORIGIN_L1_BLOB_BASE_FEE_MISMATCH" {
		t.Fatal(err)
	}
	cancun := *params.MainnetChainConfig.CancunTime
	if err := verifyOPBlobBaseFee(cancun-1, nil, "1"); err != nil {
		t.Fatal(err)
	}
	for _, excess := range []*uint64{nil, new(uint64)} {
		if excess != nil {
			*excess = ^uint64(0)
		}
		if err := verifyOPBlobBaseFee(cancun, excess, "1"); err == nil {
			t.Fatal("invalid Cancun excess accepted")
		}
	}
}
