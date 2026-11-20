package evm

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"

	"xone/chain-parser-go/internal/rpc"
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
	for i, provider := range providers {
		if err = ctx.Err(); err != nil {
			return err
		}
		current := limits
		current.MaxBytes = *remaining
		witness := Adapter{Client: provider, Chain: a.Chain}
		body, err := witness.ReadNativeBlockProof(ctx, expected.Block.Number, current)
		if err != nil {
			return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "body", i, expected.Block.Number, err)
		}
		*remaining -= body.ProofBytesUsed
		if *remaining <= 0 {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		current.MaxBytes = *remaining
		// Ethereum receipts commit to the body we just fetched from this witness.
		// Reprove its raw transactions locally and reread the canonical block after
		// receipts. Never substitute the primary provider's body here.
		var cached *ChainBlockBodyProof
		if a.Chain.Key == "evm:1" {
			cached = &body
		}
		observed, err := witness.readNativeReceiptCoverageCached(ctx, body.Block, mode, current, cached)
		if err != nil {
			return witnessFailure("RECEIPT_WITNESS_INVALID", "receipts", i, expected.Block.Number, err)
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
		after, recheckCost, err := witness.recheckWitnessBody(ctx, body, current)
		if err != nil {
			return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "body_recheck", i, expected.Block.Number, err)
		}
		*remaining -= recheckCost
		if *remaining < 0 {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		if !sameReceiptAnchor(observed.CanonicalBlock, after) {
			return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
		}
		current.MaxBytes = *remaining
		used, err := witness.recheckNativeSettling(ctx, observed, current)
		if err != nil {
			return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "settlement_recheck", i, expected.Block.Number, err)
		}
		if used > *remaining {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
		*remaining -= used
	}
	return nil
}

// Reacquire the numbered body from this witness after receipts. If its complete
// Ethereum response matches the body just proved on this reader's endpoint,
// the signed transactions and header have already been decoded and verified.
// Other representations are proved from the newly received bytes.
func (a *Adapter) recheckWitnessBody(ctx context.Context, previous ChainBlockBodyProof, limits ReceiptProofLimits) (RawBlock, int, error) {
	tag := previous.Block.Number
	if a.Chain.Key != "evm:1" || previous.verifiedRPCBodyDigest == ([32]byte{}) {
		next, err := a.ReadNativeBlockProof(ctx, tag, limits)
		return next.CanonicalBlock, next.ProofBytesUsed, err
	}
	r, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return RawBlock{}, 0, err
	}
	defer r.cancel()
	raw, err := r.call("eth_getBlockByNumber", []any{tag, true})
	if err != nil {
		return RawBlock{}, 0, err
	}
	if err = r.budget.charge(len(raw)); err != nil {
		return RawBlock{}, 0, err
	}
	if sha256.Sum256(raw) == previous.verifiedRPCBodyDigest {
		return previous.CanonicalBlock, limits.MaxBytes - r.budget.remaining, r.ctx.Err()
	}
	header, body, err := r.decodeFullBlock(tag, raw)
	if err != nil {
		return RawBlock{}, 0, err
	}
	next, err := r.proveBody(header, body)
	return next.CanonicalBlock, next.ProofBytesUsed, err
}

func sameNativeStateAnchor(a, b *ExecutionStateAnchor) bool {
	if a == nil || b == nil {
		return a == nil && b == nil
	}
	return *a == *b
}

// NativeWitnessError preserves the established public error code. The owner
// logs only bounded classifications and the ordinal among healthy witnesses.
// Provider URLs and response bodies never enter the diagnostic fields.
type NativeWitnessError struct {
	code         string
	Phase        string
	WitnessIndex int
	HeightTag    string
	Class        rpc.ErrorClass
}

func (e *NativeWitnessError) Error() string { return e.code }
func witnessFailure(code, phase string, index int, height string, cause error) *NativeWitnessError {
	class := rpc.Classify(cause)
	switch cause.Error() {
	case "RECEIPT_PROOF_BUDGET", "RECEIPT_PROOF_TRANSACTION_BUDGET", "NATIVE_WITNESS_BUDGET", "NATIVE_RUNTIME_PROOF_BUDGET":
		class = rpc.BudgetExceeded
	}
	return &NativeWitnessError{code: code, Phase: phase, WitnessIndex: index, HeightTag: height, Class: class}
}
