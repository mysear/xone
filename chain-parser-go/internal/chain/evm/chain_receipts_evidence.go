package evm

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
)

// NativeReceiptEvidence is the complete replayable input, not serialized trust
// flags. Replay re-verifies native header/body/receipt/settlement commitments
// and requires the same fork-specific semantics ID. Store it with raw archive
// ownership in FactoryRuntime/History rather than storing canonical receipts
// alone and losing their successor or system-isolation authority.
type NativeReceiptEvidence struct {
	SchemaVersion       uint64                 `json:"schema_version"`
	ChainKey            string                 `json:"chain_key"`
	GenesisHash         string                 `json:"genesis_hash"`
	NativeSemanticsID   string                 `json:"native_semantics_id"`
	Kind                string                 `json:"kind"`
	SAE                 *SAEExecutionProof     `json:"sae,omitempty"`
	Bor                 *BorReceiptProof       `json:"bor,omitempty"`
	Block               *ExecutionReceiptBlock `json:"block,omitempty"`
	BorObservedReceipts []Receipt              `json:"bor_observed_receipts,omitempty"`
}

func VerifyNativeReceiptEvidence(ctx context.Context, chainKey string, target RawBlock, evidence NativeReceiptEvidence, limits ReceiptProofLimits) (VerifiedReceiptCoverage, error) {
	var out VerifiedReceiptCoverage
	if err := limits.validate(); err != nil {
		return out, err
	}
	if err := ctx.Err(); err != nil {
		return out, err
	}
	genesis, err := knownBodyGenesis(chainKey)
	if err != nil {
		return out, err
	}
	if evidence.SchemaVersion != 1 || evidence.ChainKey != chainKey || !strings.EqualFold(evidence.GenesisHash, genesis) || evidence.NativeSemanticsID == "" {
		return out, errors.New("NATIVE_EVIDENCE_IDENTITY")
	}
	switch evidence.Kind {
	case "sae-settling-window":
		if evidence.SAE == nil || evidence.Bor != nil || evidence.Block != nil || len(evidence.BorObservedReceipts) > 0 || evidence.SAE.ChainKey != chainKey || !strings.EqualFold(evidence.SAE.GenesisHash, genesis) {
			return out, errors.New("NATIVE_EVIDENCE_AUTHORITY_INVALID")
		}
		out, err = VerifySAEExecutionProof(ctx, target, *evidence.SAE, limits)
	case "bor-canonical-system-isolation":
		if evidence.Bor == nil || evidence.SAE != nil || evidence.Block != nil || evidence.Bor.ChainKey != chainKey || !strings.EqualFold(evidence.Bor.GenesisHash, genesis) {
			return out, errors.New("NATIVE_EVIDENCE_AUTHORITY_INVALID")
		}
		out, err = VerifyBorReceiptCoverage(ctx, target, evidence.BorObservedReceipts, *evidence.Bor, limits)
	case "own-header-receipts":
		if evidence.Block == nil || evidence.SAE != nil || evidence.Bor != nil || len(evidence.BorObservedReceipts) > 0 {
			return out, errors.New("NATIVE_EVIDENCE_AUTHORITY_INVALID")
		}
		block := *evidence.Block
		budget := receiptProofBudget{remaining: limits.MaxBytes}
		if err = budget.charge(2*len(block.Header) + 256); err != nil {
			return out, err
		}
		header, e := VerifyChainHeaderJSON(ctx, chainKey, block.Header)
		if e != nil {
			return out, e
		}
		if !sameReceiptAnchor(target, header.Block) {
			return out, errors.New("NATIVE_EVIDENCE_TARGET_IDENTITY")
		}
		if chainKey == "evm:137" || header.SettledHeight != nil {
			return out, errors.New("NATIVE_EVIDENCE_AUTHORITY_INVALID")
		}
		if len(header.Block.Transactions) > limits.MaxTransactions {
			return out, errors.New("RECEIPT_PROOF_TRANSACTION_BUDGET")
		}
		for _, receipt := range block.Receipts {
			if !chargeReceipt(&budget.remaining, receipt) {
				return out, errors.New("RECEIPT_PROOF_BUDGET")
			}
		}
		if err = verifyExecutionBody(ctx, chainKey, header, block, &budget); err != nil {
			return out, err
		}
		if err = VerifyChainReceiptRoot(ctx, chainKey, header.Block, block.Receipts); err != nil {
			return out, err
		}
		height, _ := canonicalIndex(target.Number)
		accountEncoding := "ethereum-account-rlp/v1"
		if chainKey == "evm:43114" {
			accountEncoding = "coreth-account-held-xchain-rlp/v1"
		}
		out = VerifiedReceiptCoverage{ChainKey: chainKey, Block: target, CanonicalBlock: target, CanonicalReceipts: block.Receipts, CanonicalBodyProven: true, CanonicalReceiptsProven: true, WholeBlockReceiptEventsProven: true, SystemObservationCoverage: "all-canonical-receipts-committed", ReceiptAuthority: "own-header-receipts-root", ReceiptRoot: header.ReceiptsRoot, WindowStart: height, WindowEnd: height, StateAnchor: &ExecutionStateAnchor{Height: height, BlockHash: target.Hash, StateRoot: header.StateRoot, SettlingHeight: height, SettlingHash: target.Hash, Authority: "native-post-execution-header-state-root", AccountEncodingID: accountEncoding}}
		err = attachNativeSemantics(&out, header)
		out.ProofBytesUsed = limits.MaxBytes - budget.remaining
	default:
		return out, errors.New("NATIVE_EVIDENCE_KIND_UNSUPPORTED")
	}
	if err != nil {
		return VerifiedReceiptCoverage{}, err
	}
	if out.NativeSemanticsID != evidence.NativeSemanticsID {
		return VerifiedReceiptCoverage{}, errors.New("NATIVE_EVIDENCE_SEMANTICS_MISMATCH")
	}
	copy := evidence
	out.Evidence = &copy
	return out, nil
}

// DecodeNativeReceiptEvidence provides the archive byte/depth/duplicate-key
// boundary before any large evidence structure is allocated by JSON decoding.
func DecodeNativeReceiptEvidence(raw json.RawMessage, maxBytes int) (NativeReceiptEvidence, error) {
	var evidence NativeReceiptEvidence
	if maxBytes <= 0 || maxBytes > 64<<20 || len(raw) > maxBytes {
		return evidence, errors.New("NATIVE_EVIDENCE_BUDGET")
	}
	if err := rejectDuplicateReceiptKeys(raw); err != nil {
		return evidence, err
	}
	if json.Unmarshal(raw, &evidence) != nil {
		return evidence, errors.New("NATIVE_EVIDENCE_ENCODING")
	}
	return evidence, nil
}
