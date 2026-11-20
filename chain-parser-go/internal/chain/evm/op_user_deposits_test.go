package evm

import (
	"context"
	"encoding/json"
	"slices"
	"testing"
	"xone/chain-parser-go/internal/provenance"
)

func TestOPPortalDepositsFromSignedReceipts(t *testing.T) {
	var beacon provenance.OnlineBeaconProof
	readOriginFixture(t, "op-mainnet-batches/beacon-26454447.json.gz", &beacon)
	proof, err := provenance.VerifyExecutionFinality("0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7", beacon.Bootstrap, beacon.Finality, beacon.Updates)
	if err != nil {
		t.Fatal(err)
	}
	var input OPOriginEvidence
	readOriginFixture(t, "op-mainnet-batches/portal-origin-26454436.json.gz", &input)
	ctx := context.Background()
	report, err := VerifyOPOrigin(ctx, proof, input)
	if err != nil || report.UserDepositHashes == nil || !slices.Equal(*report.UserDepositHashes, []string{"0x4ef2f95c5a1de8c2010056acb0966e61c7fc27cdf6f604ae8a32c163f3835093"}) {
		t.Fatal(report, err)
	}
	var l1 RawBlock
	var receipts []Receipt
	if err := json.Unmarshal(input.Headers[len(input.Headers)-1], &l1); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(input.OriginReceipts, &receipts); err != nil {
		t.Fatal(err)
	}
	body, err := VerifyChainBlockJSON(ctx, "evm:10", input.Block, 32<<20)
	if err != nil {
		t.Fatal(err)
	}
	slices.Reverse(receipts)
	if _, err = verifyPortalDeposits(ctx, l1, receipts, body); err != nil {
		t.Fatal(err)
	}
	if _, err = verifyPortalDeposits(ctx, l1, receipts[1:], body); err == nil {
		t.Fatal("omitted receipt accepted")
	}
	second := body.RawTransactions[1]
	body.RawTransactions[1] = body.RawTransactions[0]
	if _, err = verifyPortalDeposits(ctx, l1, receipts, body); err == nil || err.Error() != "OP_PORTAL_DEPOSIT_CONTENT" {
		t.Fatal(err)
	}
	body.RawTransactions[1] = second
	body.RawTransactions = append(body.RawTransactions, second)
	if _, err = verifyPortalDeposits(ctx, l1, receipts, body); err == nil || err.Error() != "OP_PORTAL_EXTRA_DEPOSIT" {
		t.Fatal(err)
	}
}
