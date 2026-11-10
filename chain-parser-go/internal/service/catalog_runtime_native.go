package service

import (
	"context"
	"encoding/json"
	"errors"
	"strings"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/ingest"
)

// A normal OnComplete archive literal carries its transient native coverage into
// this explicit sidecar. Replay verifies the original inputs rather than trust
// flags, and never infers successor authority from canonical receipts alone.
func (p factoryHTTPProof) MarshalJSON() ([]byte, error) {
	type wire factoryHTTPProof
	copy := p
	if copy.NativeEvidence == nil && copy.Header.NativeCoverage != nil {
		copy.NativeEvidence = copy.Header.NativeCoverage.Evidence
		copy.NativeSemanticsID = copy.Header.NativeCoverage.NativeSemanticsID
	}
	return json.Marshal(wire(copy))
}
func (r *CatalogRuntime) projectHTTPProof(p factoryHTTPProof) (ingest.FactoryBlock, error) {
	if r.options.Chain.BlockIntegrity != evm.ChainNativeCommitmentsV2 {
		return r.projectHTTPProofUnchecked(p)
	}
	ctx := p.Header.NativeReadContext()
	if ctx == nil {
		return ingest.FactoryBlock{}, errors.New("FACTORY_NATIVE_RESPONSIBILITY_CONTEXT_REQUIRED")
	}
	return r.projectHTTPProofContext(ctx, p)
}
func (r *CatalogRuntime) projectHTTPProofContext(ctx context.Context, p factoryHTTPProof) (ingest.FactoryBlock, error) {
	if err := r.verifyFactoryHTTPProof(ctx, p); err != nil {
		return ingest.FactoryBlock{}, err
	}
	return r.projectHTTPProofUnchecked(p)
}
func (r *CatalogRuntime) verifyFactoryHTTPProof(ctx context.Context, p factoryHTTPProof) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	chain := r.options.Chain
	if chain.BlockIntegrity != evm.ChainNativeCommitmentsV2 {
		return evm.VerifyChainReceiptRoot(ctx, chain.Key, p.Header, p.Receipts)
	}
	evidence, id := p.NativeEvidence, p.NativeSemanticsID
	if evidence == nil && p.Header.NativeCoverage != nil {
		evidence = p.Header.NativeCoverage.Evidence
		id = p.Header.NativeCoverage.NativeSemanticsID
	}
	if evidence == nil || id == "" {
		return errors.New("FACTORY_NATIVE_EVIDENCE_REQUIRED")
	}
	limits, err := evm.NativeRuntimeLimits(chain)
	if err != nil {
		return err
	}
	limits.MaxBytes = min(limits.MaxBytes, int(r.options.Config.MaxBytes))
	if !strings.EqualFold(evidence.GenesisHash, chain.Genesis) {
		return errors.New("FACTORY_NATIVE_FINGERPRINT")
	}
	coverage, err := evm.VerifyNativeReceiptEvidence(ctx, chain.Key, p.Header, *evidence, limits)
	if err != nil {
		return err
	}
	if id != coverage.NativeSemanticsID {
		return errors.New("FACTORY_NATIVE_SEMANTICS_MISMATCH")
	}
	chain.Pools = nil
	chain.DynamicCatalog = &r.options.Config
	if err = evm.VerifyNativeBusinessCoverage(chain, coverage); err != nil {
		return err
	}
	expected, err := json.Marshal(coverage.CanonicalReceipts)
	if err != nil {
		return err
	}
	actual, err := json.Marshal(p.Receipts)
	if err != nil {
		return err
	}
	if string(expected) != string(actual) {
		return errors.New("FACTORY_NATIVE_RECEIPTS_PROJECTION_MISMATCH")
	}
	return ctx.Err()
}
