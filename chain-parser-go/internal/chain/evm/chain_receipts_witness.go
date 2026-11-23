package evm

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"

	"xone/chain-parser-go/internal/config"
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
		reader, err := newNativeProofReader(ctx, &witness, current)
		if err != nil {
			return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "body", i, expected.Block.Number, err)
		}
		err = reader.verifyReceiptWitness(a.Chain, expected, digest, mode, i)
		reader.cancel()
		*remaining = reader.budget.remaining
		if err != nil {
			return err
		}
	}
	return nil
}

// One proof reader owns the endpoint, deadline and remaining byte allowance for
// this witness. Only its own verified body can supply the receipt proof.
func (r *nativeProofReader) verifyReceiptWitness(chain config.Chain, expected VerifiedReceiptCoverage, digest, mode string, index int) error {
	tag := expected.Block.Number
	body, err := r.readBlockProof(tag)
	if err != nil {
		return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "body", index, tag, err)
	}
	before := r.budget.remaining
	observed, err := r.readReceiptCoverage(body.Block, mode, &body)
	if err != nil {
		return witnessFailure("RECEIPT_WITNESS_INVALID", "receipts", index, tag, err)
	}
	// Bor observations outside the transactions root can still affect business
	// scope. Check eligibility on each witness before comparing canonical digests.
	if err = VerifyNativeBusinessCoverage(chain, observed); err != nil {
		return err
	}
	encoded, err := json.Marshal(observed.Evidence)
	if err != nil {
		return err
	}
	if extra := len(encoded) - (before - r.budget.remaining); extra > 0 {
		if err = r.budget.charge(extra); err != nil {
			return errors.New("NATIVE_WITNESS_BUDGET")
		}
	}
	observedDigest, err := receiptDigest(r.ctx, observed.CanonicalBlock, observed.CanonicalReceipts)
	if err != nil {
		return err
	}
	if !sameReceiptAnchor(expected.CanonicalBlock, observed.CanonicalBlock) || digest != observedDigest || observed.NativeSemanticsID != expected.NativeSemanticsID || observed.ReceiptRoot != expected.ReceiptRoot || observed.WindowStart != expected.WindowStart || observed.WindowEnd != expected.WindowEnd || !sameNativeStateAnchor(expected.StateAnchor, observed.StateAnchor) {
		return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
	}
	if err = r.checkProviderIdentity(); err != nil {
		return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "body_recheck", index, tag, err)
	}
	after, err := r.recheckWitnessBody(body)
	if err != nil {
		return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "body_recheck", index, tag, err)
	}
	if !sameReceiptAnchor(observed.CanonicalBlock, after) {
		return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
	}
	if observed.Evidence.SAE != nil {
		if err = r.recheckSettlingHeader(observed.Evidence.SAE.Headers); err != nil {
			return witnessFailure("RECEIPT_WITNESS_UNAVAILABLE", "settlement_recheck", index, tag, err)
		}
	}
	return r.ctx.Err()
}

// Reacquire the numbered body after receipts. A byte-identical Ethereum
// response was already proved on this endpoint. Native formats that may need
// additional raw transaction observations go through the full verifier again.
func (r *nativeProofReader) recheckWitnessBody(previous ChainBlockBodyProof) (RawBlock, error) {
	tag := previous.Block.Number
	if r.chainKey != "evm:1" || previous.verifiedRPCBodyDigest == ([32]byte{}) {
		next, err := r.readBlockProof(tag)
		return next.CanonicalBlock, err
	}
	raw, err := r.call("eth_getBlockByNumber", []any{tag, true})
	if err != nil {
		return RawBlock{}, err
	}
	if err = r.budget.charge(len(raw)); err != nil {
		return RawBlock{}, err
	}
	if sha256.Sum256(raw) == previous.verifiedRPCBodyDigest {
		return previous.CanonicalBlock, r.ctx.Err()
	}
	header, body, err := r.decodeFullBlock(tag, raw)
	if err != nil {
		return RawBlock{}, err
	}
	next, err := r.proveBody(header, body)
	return next.CanonicalBlock, err
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
