package evm

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/ethereum/go-ethereum/common"
)

func TestOPChannelMainnetInterval(t *testing.T) {
	ctx := context.Background()
	var bodies []json.RawMessage
	readOriginFixture(t, "op-mainnet-batches/interval-26453040-26453042.json.gz", &bodies)
	var next OPChannelSubmission
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &next.Evidence)
	prev := OPChannelSubmission{Evidence: OPBatchEvidence{Block: bodies[0]}}
	next.Intervening = []OPChannelIntervalBlock{{Block: bodies[1]}, {Block: bodies[2]}}
	const sender = "0x6887246668a3b87f54deb3b94ba47a6f63f32985"
	left := OPBatchReport{Sender: sender, L1Number: 26453040, L1Hash: "0xdc0dc2cf7e58abde1ca2a0bd68ca8ca0cea754f657ee2ce07b42522c80becc90", TxHash: "0x62a3b9962143cd078589fd195cabcbe69fec479d3bc263d6b8111f326c9cc54f"}
	right := OPBatchReport{Sender: sender, L1Number: 26453043, L1Hash: "0x52fd70e5648c3caee20f1a2b51566ebaca6c5caba35880f668c3fa8cb9bc5239", TxHash: next.Evidence.TxHash}
	if err := verifyOPChannelInterval(ctx, prev, left, next, right); err != nil {
		t.Fatal(err)
	}
	bad := next
	bad.Intervening = next.Intervening[:1]
	if err := verifyOPChannelInterval(ctx, prev, left, bad, right); err == nil || err.Error() != "OP_CHANNEL_INTERVAL_LENGTH" {
		t.Fatal(err)
	}
	bad.Intervening = []OPChannelIntervalBlock{next.Intervening[1], next.Intervening[0]}
	if err := verifyOPChannelInterval(ctx, prev, left, bad, right); err == nil || err.Error() != "OP_CHANNEL_INTERVAL_LINK" {
		t.Fatal(err)
	}
	body, err := VerifyChainBlockJSON(ctx, "evm:1", bodies[0], 32<<20)
	if err != nil {
		t.Fatal(err)
	}
	if err = scanOPInboxRange(ctx, body, "", "", common.Address{}, nil); err == nil || err.Error() != "OP_CHANNEL_INTERVAL_AUTHORITY" {
		t.Fatal(err)
	}
	if err = scanOPInboxRange(ctx, body, "", "", common.HexToAddress(sender), nil); err == nil || err.Error() != "OP_CHANNEL_INTERVENING_SUBMISSION" {
		t.Fatal(err)
	}
}
