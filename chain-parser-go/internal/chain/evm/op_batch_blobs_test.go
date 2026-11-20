package evm

import (
	"bytes"
	"context"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"xone/chain-parser-go/internal/provenance"
)

func TestFinalizedOPBatchWithFiveBlobs(t *testing.T) {
	var observation provenance.OnlineBeaconProof
	readOriginFixture(t, "op-mainnet-batches/beacon-26453043.json.gz", &observation)
	proof, err := provenance.VerifyExecutionFinality("0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7", observation.Bootstrap, observation.Finality, observation.Updates)
	if err != nil {
		t.Fatal(err)
	}
	var evidence OPBatchEvidence
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &evidence)
	sender := common.HexToAddress("0x6887246668a3b87f54deb3b94ba47a6f63f32985")
	ctx := context.Background()
	report, err := VerifyOPBatchBlobs(ctx, proof, sender, evidence)
	if err != nil || report.L1Number != 26453043 || report.FirstBlobIndex != 3 || len(report.VersionedHashes) != 5 {
		t.Fatal(report, err)
	}
	if _, err = VerifyOPBatchBlobs(ctx, proof, common.HexToAddress("0x1234"), evidence); err == nil || err.Error() != "OP_BATCH_SENDER" {
		t.Fatal("sender check", err)
	}
	if err = verifyOPBatchData(ctx, report.FirstBlobIndex, report.VersionedHashes, evidence.Blobs[:7]); err == nil {
		t.Fatal("short blob list")
	}
	if err = verifyOPBatchData(ctx, 2, report.VersionedHashes, evidence.Blobs); err == nil {
		t.Fatal("incorrect first blob index")
	}
	evidence.Blobs[3][1] ^= 1
	if err = verifyOPBatchData(ctx, report.FirstBlobIndex, report.VersionedHashes, evidence.Blobs); err == nil || err.Error() != "OP_BATCH_BLOB_COMMITMENT" {
		t.Fatal("modified blob", err)
	}
}

func TestOPBatchSystemConfigSender(t *testing.T) {
	var observed provenance.OnlineBeaconProof
	readOriginFixture(t, "op-mainnet-batches/beacon-26453871.json.gz", &observed)
	finality, err := provenance.VerifyExecutionFinality("0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7", observed.Bootstrap, observed.Finality, observed.Updates)
	if err != nil {
		t.Fatal(err)
	}
	var evidence OPBatchEvidence
	var proof OPBatchSenderProof
	readOriginFixture(t, "op-mainnet-batches/batch-26453870.json.gz", &evidence)
	readOriginFixture(t, "op-mainnet-batches/authority-26453870.json.gz", &proof)
	evidence.SenderProof = &proof
	ctx := context.Background()
	report, err := VerifyOPBatchBlobs(ctx, finality, common.Address{}, evidence)
	if err != nil || report.SenderAuthority != "op-mainnet-system-config-storage" || report.FirstBlobIndex != 10 || report.L1Number != 26453870 {
		t.Fatal(report, err)
	}
	if _, err = VerifyOPBatchBlobs(ctx, finality, common.HexToAddress("0x1234"), evidence); err == nil || err.Error() != "OP_BATCH_SENDER_POLICY" {
		t.Fatal(err)
	}
	bad := evidence
	bad.Descendants = nil
	if _, err = VerifyOPBatchBlobs(ctx, finality, common.Address{}, bad); err == nil || err.Error() != "OP_BATCH_FINALIZED_L1_BLOCK" {
		t.Fatal(err)
	}
	bad = evidence
	bad.SenderProof = &OPBatchSenderProof{Proxy: proof.Proxy, Implementation: proof.Proxy}
	if _, err = VerifyOPBatchBlobs(ctx, finality, common.Address{}, bad); err == nil || err.Error() != "OP_CONFIG_IMPLEMENTATION" {
		t.Fatal(err)
	}
	changed := bytes.Replace(proof.Proxy, []byte("0x6887246668a3b87f54deb3b94ba47a6f63f32985"), []byte("0x6887246668a3b87f54deb3b94ba47a6f63f32986"), 1)
	if bytes.Equal(changed, proof.Proxy) {
		t.Fatal("fixture sender not found")
	}
	bad.SenderProof = &OPBatchSenderProof{Proxy: changed, Implementation: proof.Implementation}
	if _, err = VerifyOPBatchBlobs(ctx, finality, common.Address{}, bad); err == nil {
		t.Fatal("accepted unproven sender")
	}
}
