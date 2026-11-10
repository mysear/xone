package evm

import (
	"context"
	"encoding/json"
	"testing"
)

func TestNativeEvidenceArchiveRoundTripReverifiesAuthorities(t *testing.T) {
	target, proof := retainedSAEProof(t)
	coverage, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if coverage.Evidence == nil {
		t.Fatal("verified successor authority missing from archival evidence")
	}
	raw, err := json.Marshal(coverage.Evidence)
	if err != nil {
		t.Fatal(err)
	}
	var restored NativeReceiptEvidence
	if err = json.Unmarshal(raw, &restored); err != nil {
		t.Fatal(err)
	}
	verified, err := VerifyNativeReceiptEvidence(context.Background(), "evm:43114", target, restored, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if verified.NativeSemanticsID != coverage.NativeSemanticsID || verified.StateAnchor.StateRoot != coverage.StateAnchor.StateRoot {
		t.Fatal("archive changed native authority")
	}
	restored.NativeSemanticsID = "old-generic-ethereum-label"
	if _, err := VerifyNativeReceiptEvidence(context.Background(), "evm:43114", target, restored, nativeProofTestLimits()); err == nil {
		t.Fatal("native semantics binding was ignored")
	}
}
func TestNativeEvidenceBorArchiveCannotUpgradeWholeBlockEvents(t *testing.T) {
	target, receipts, proof := retainedBorProof(t)
	coverage, err := VerifyBorReceiptCoverage(context.Background(), target, receipts, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	raw, _ := json.Marshal(coverage.Evidence)
	var restored NativeReceiptEvidence
	if err = json.Unmarshal(raw, &restored); err != nil {
		t.Fatal(err)
	}
	verified, err := VerifyNativeReceiptEvidence(context.Background(), "evm:137", target, restored, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if verified.WholeBlockReceiptEventsProven || len(verified.CanonicalReceipts) != 389 || len(verified.SystemObservations) != 1 {
		t.Fatal(verified)
	}
}
