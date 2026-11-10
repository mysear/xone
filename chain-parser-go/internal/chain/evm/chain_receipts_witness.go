package evm

import (
	"context"
	"encoding/json"
	"errors"
)

// VerifyNativeReceiptWitnesses independently reacquires native header/body,
// canonical receipts and SAE successor commitments from every healthy source.
// It never reapplies an own-header root to an SAE target. Uncommitted Bor
// observations are not part of the canonical witness digest or voting proof.
func (a *Adapter) VerifyNativeReceiptWitnesses(ctx context.Context, expected VerifiedReceiptCoverage, mode string, limits ReceiptProofLimits) error {
	_, err := a.VerifyNativeReceiptWitnessesUsage(ctx, expected, mode, limits)
	return err
}
func (a *Adapter) VerifyNativeReceiptWitnessesUsage(ctx context.Context, expected VerifiedReceiptCoverage, mode string, limits ReceiptProofLimits) (used int, err error) {
	remaining := limits.MaxBytes
	defer func() { used = limits.MaxBytes - remaining }()
	err = a.verifyNativeReceiptWitnessesBudget(ctx, expected, mode, limits, &remaining)
	return used, err
}
func (a *Adapter) verifyNativeReceiptWitnessesBudget(ctx context.Context, expected VerifiedReceiptCoverage, mode string, limits ReceiptProofLimits, remaining *int) error {
	if err := limits.validate(); err != nil {
		return err
	}
	if a.Client == nil || a.Client.HTTP == nil || a.Client.HTTP.Timeout <= 0 {
		return errors.New("NATIVE_WITNESS_SOURCE_INVALID")
	}
	if expected.ChainKey != a.Chain.Key || !expected.CanonicalBodyProven || !expected.CanonicalReceiptsProven || expected.Evidence == nil {
		return errors.New("NATIVE_WITNESS_EXPECTED_PROOF_REQUIRED")
	}
	ctx, cancel := context.WithTimeout(ctx, a.Client.HTTP.Timeout)
	defer cancel()
	configured, providers := a.Client.ProviderClients()
	if configured < 2 {
		return nil
	}
	if len(providers) < 2 {
		return errors.New("RECEIPT_WITNESS_SOURCE_REQUIRED")
	}
	// Recheck the expected proof, so callers cannot upgrade serialized flags.
	expected, err := VerifyNativeReceiptEvidence(ctx, a.Chain.Key, expected.Block, *expected.Evidence, limits)
	if err != nil {
		return err
	}
	if expected.ProofBytesUsed > *remaining {
		return errors.New("NATIVE_WITNESS_BUDGET")
	}
	*remaining -= expected.ProofBytesUsed
	if err = VerifyNativeBusinessCoverage(a.Chain, expected); err != nil {
		return err
	}
	digest, err := receiptDigest(ctx, expected.CanonicalBlock, expected.CanonicalReceipts)
	if err != nil {
		return err
	}
	for _, provider := range providers {
		if err = ctx.Err(); err != nil {
			return err
		}
		current := limits
		current.MaxBytes = *remaining
		witness := Adapter{Client: provider, Chain: a.Chain}
		body, err := witness.ReadNativeBlockProof(ctx, expected.Block.Number, current)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_UNAVAILABLE")
		}
		*remaining -= body.ProofBytesUsed
		if *remaining <= 0 {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		current.MaxBytes = *remaining
		observed, err := witness.ReadNativeReceiptCoverage(ctx, body.Block, mode, current)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_INVALID")
		}
		// A witness may retain uncommitted system observations omitted by the
		// primary. Canonical digest agreement cannot qualify affected business
		// scope; every observed source must pass the same scope gate.
		if err = VerifyNativeBusinessCoverage(a.Chain, observed); err != nil {
			return err
		}
		encoded, err := json.Marshal(observed.Evidence)
		if err != nil {
			return err
		}
		// Bound retained per-source proof payload even when the reader's conservative
		// work estimate is unavailable on historical evidence.
		charge := max(observed.ProofBytesUsed, len(encoded))
		if charge > *remaining {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		*remaining -= charge
		observedDigest, err := receiptDigest(ctx, observed.CanonicalBlock, observed.CanonicalReceipts)
		if err != nil {
			return err
		}
		if !sameReceiptAnchor(expected.CanonicalBlock, observed.CanonicalBlock) || digest != observedDigest || observed.NativeSemanticsID != expected.NativeSemanticsID || observed.ReceiptRoot != expected.ReceiptRoot || observed.WindowStart != expected.WindowStart || observed.WindowEnd != expected.WindowEnd || !sameNativeStateAnchor(expected.StateAnchor, observed.StateAnchor) {
			return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
		}
		current.MaxBytes = *remaining
		after, err := witness.ReadNativeBlockProof(ctx, expected.Block.Number, current)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_UNAVAILABLE")
		}
		*remaining -= after.ProofBytesUsed
		if *remaining < 0 {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		if !sameReceiptAnchor(observed.CanonicalBlock, after.CanonicalBlock) {
			return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
		}
		current.MaxBytes = *remaining
		used, err := witness.recheckNativeSettling(ctx, observed, current)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_UNAVAILABLE")
		}
		if used > *remaining {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		*remaining -= used
	}
	return nil
}
func sameNativeStateAnchor(a, b *ExecutionStateAnchor) bool {
	if a == nil || b == nil {
		return a == nil && b == nil
	}
	return *a == *b
}
