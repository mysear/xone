package evm

import (
	"context"
	"errors"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

type NativeHistoryEvidence struct {
	SchemaVersion     uint64                    `json:"schema_version"`
	ScopeDatasetID    string                    `json:"scope_dataset_id"`
	DatasetID         string                    `json:"dataset_id"`
	NativeSemanticsID string                    `json:"native_semantics_id"`
	Block             RawBlock                  `json:"block"`
	Evidence          NativeReceiptEvidence     `json:"evidence"`
	Facts             domain.FactBatch          `json:"facts"`
	Candidates        []ingest.FactoryCandidate `json:"candidates"`
}

// NativeHistoryDatasetID adds the actual activation/header semantics to the
// closed, order-independent pool/factory/decoder scope. The queue scope ID
// remains distinct so crossing a fork does not silently change claim scope.
func NativeHistoryDatasetID(chain config.Chain, coverage VerifiedReceiptCoverage) (string, error) {
	scope, err := HistoryDatasetID(chain)
	if err != nil {
		return "", err
	}
	if chain.BlockIntegrity != ChainNativeCommitmentsV2 || coverage.ChainKey != chain.Key || coverage.NativeSemanticsID == "" || coverage.NativeSemantics == nil {
		return "", errors.New("HISTORY_NATIVE_SEMANTICS_REQUIRED")
	}
	return domain.Hash(struct{ Version, Scope, NativeSemantics, Activation, ReceiptScope string }{"native-history-dataset/1", scope, coverage.NativeSemanticsID, coverage.ActivationID, coverage.NativeSemantics.ReceiptCoverageScope})
}
func buildNativeHistoryEvidence(chain config.Chain, b RawBlock, batch domain.FactBatch, candidates []ingest.FactoryCandidate) (NativeHistoryEvidence, error) {
	var out NativeHistoryEvidence
	if b.NativeCoverage == nil || b.NativeCoverage.Evidence == nil {
		return out, errors.New("HISTORY_NATIVE_EVIDENCE_REQUIRED")
	}
	scope, err := HistoryDatasetID(chain)
	if err != nil {
		return out, err
	}
	dataset, err := NativeHistoryDatasetID(chain, *b.NativeCoverage)
	if err != nil {
		return out, err
	}
	return NativeHistoryEvidence{SchemaVersion: 1, ScopeDatasetID: scope, DatasetID: dataset, NativeSemanticsID: b.NativeCoverage.NativeSemanticsID, Block: b, Evidence: *b.NativeCoverage.Evidence, Facts: batch, Candidates: candidates}, nil
}
func VerifyNativeHistoryEvidence(ctx context.Context, chain config.Chain, e NativeHistoryEvidence) (domain.FactBatch, []ingest.FactoryCandidate, error) {
	var empty domain.FactBatch
	limits, err := NativeRuntimeLimits(chain)
	if err != nil {
		return empty, nil, err
	}
	scope, err := HistoryDatasetID(chain)
	if err != nil || e.SchemaVersion != 1 || scope != e.ScopeDatasetID {
		return empty, nil, errors.New("HISTORY_NATIVE_SCOPE_MISMATCH")
	}
	coverage, err := VerifyNativeReceiptEvidence(ctx, chain.Key, e.Block, e.Evidence, limits)
	if err != nil {
		return empty, nil, err
	}
	if err = VerifyNativeBusinessCoverage(chain, coverage); err != nil {
		return empty, nil, err
	}
	dataset, err := NativeHistoryDatasetID(chain, coverage)
	if err != nil || dataset != e.DatasetID || coverage.NativeSemanticsID != e.NativeSemanticsID {
		return empty, nil, errors.New("HISTORY_NATIVE_DATASET_MISMATCH")
	}
	ref, err := blockReference(chain.Key, coverage.Block)
	if err != nil {
		return empty, nil, err
	}
	facts, err := DecodeSwapFacts(ref, coverage.CanonicalReceipts, chain.Pools)
	if err != nil {
		return empty, nil, err
	}
	adapter := Adapter{Chain: chain}
	candidates, err := adapter.historyCandidates(ctx, ref, coverage.CanonicalReceipts)
	if err != nil {
		return empty, nil, err
	}
	batch := domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: ref, Swaps: facts}}}
	if err = batch.Validate("BLOCK_COMMIT", ref); err != nil {
		return empty, nil, err
	}
	expected, _ := domain.Hash(batch)
	actual, _ := domain.Hash(e.Facts)
	if expected != actual {
		return empty, nil, errors.New("HISTORY_NATIVE_FACT_PROJECTION_MISMATCH")
	}
	expected, _ = domain.Hash(candidates)
	actual, _ = domain.Hash(e.Candidates)
	if expected != actual {
		return empty, nil, errors.New("HISTORY_NATIVE_FACTORY_PROJECTION_MISMATCH")
	}
	return batch, candidates, ctx.Err()
}

func nativeRegistryID(chainKey string) (string, error) {
	genesis, err := knownBodyGenesis(chainKey)
	if err != nil {
		return "", err
	}
	// Source pins commit the activation tables; the registry version commits the
	// selection/encoding contract. Actual per-block ArbOS/fork IDs live above.
	header := ChainHeaderCommitment{Block: RawBlock{Number: "0x0", Timestamp: "0x0"}, HeaderEncoding: "native-activation-registry"}
	descriptor, err := ResolveNativeSemantics(chainKey, header)
	if err != nil {
		return "", err
	}
	return domain.Hash(struct {
		Version, Chain, Genesis string
		Pins                    map[string]string
		Coreth                  []nativeTimeFork
	}{"native-header-body-receipt-state/activation-registry-v2", chainKey, genesis, descriptor.SourcePins, corethNativeTimeForks})
}
