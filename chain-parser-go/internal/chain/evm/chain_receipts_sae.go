package evm

import (
	"context"
	"errors"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/trie"
)

// VerifySAEExecutionProof authenticates the flattened receipt window committed
// by the first successor header settling the target. Upstream blocks.Settles
// defines this as (parent.settledHeight, child.settledHeight], in height order.
// The resulting state root applies only to the final height of that interval.
func VerifySAEExecutionProof(ctx context.Context, target RawBlock, proof SAEExecutionProof, limits ReceiptProofLimits) (VerifiedReceiptCoverage, error) {
	var out VerifiedReceiptCoverage
	if err := ctx.Err(); err != nil {
		return out, err
	}
	if err := limits.validate(); err != nil {
		return out, err
	}
	genesis, _ := knownBodyGenesis("evm:43114")
	if proof.ChainKey != "evm:43114" || !strings.EqualFold(proof.GenesisHash, genesis) {
		return out, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	if len(proof.Headers) < 2 || len(proof.Headers) > limits.MaxHeaders || len(proof.Window) < 1 || len(proof.Window) > limits.MaxWindowBlocks {
		return out, errors.New("SAE_PROOF_RANGE_INVALID")
	}
	budget := receiptProofBudget{remaining: limits.MaxBytes}
	targetHeight, err := canonicalIndex(target.Number)
	if err != nil {
		return out, err
	}
	headers := make([]ChainHeaderCommitment, len(proof.Headers))
	var firstHeight uint64
	for i, raw := range proof.Headers {
		if err = budget.charge(2*len(raw) + 256); err != nil {
			return out, err
		}
		header, e := VerifyChainHeaderJSON(ctx, proof.ChainKey, raw)
		if e != nil {
			return out, e
		}
		height, _ := canonicalIndex(header.Block.Number)
		if header.SettledHeight == nil {
			return out, errors.New("SAE_ACTIVATION_REQUIRED")
		}
		if i == 0 {
			firstHeight = height
		} else {
			if height != firstHeight+uint64(i) || header.Block.ParentHash != headers[i-1].Block.Hash || *header.SettledHeight < *headers[i-1].SettledHeight {
				return out, errors.New("SAE_HEADER_CHAIN_INVALID")
			}
			if *headers[i-1].SettledHeight < targetHeight && *header.SettledHeight >= targetHeight && i != len(proof.Headers)-1 {
				return out, errors.New("SAE_FIRST_SETTLING_HEADER_REQUIRED")
			}
		}
		headers[i] = header
	}
	anchor := headers[len(headers)-1]
	parent := headers[len(headers)-2]
	anchorHeight, _ := canonicalIndex(anchor.Block.Number)
	if anchorHeight <= targetHeight || anchorHeight-targetHeight > limits.MaxLookahead || *parent.SettledHeight >= targetHeight || *anchor.SettledHeight < targetHeight {
		return out, errors.New("SAE_SETTLING_WINDOW_INVALID")
	}
	start, end := *parent.SettledHeight+1, *anchor.SettledHeight
	if start != firstHeight || end < start || end-start+1 != uint64(len(proof.Window)) || end-start+1 > uint64(limits.MaxWindowBlocks) || targetHeight < start || targetHeight > end {
		return out, errors.New("SAE_WINDOW_INCOMPLETE")
	}
	if !sameReceiptAnchor(target, headers[targetHeight-start].Block) {
		return out, errors.New("SAE_TARGET_IDENTITY")
	}
	encoded := encodedChainReceipts{}
	var targetReceipts []Receipt
	var bloom types.Bloom
	transactions := 0
	for i, block := range proof.Window {
		if err = budget.charge(2*len(block.Header) + 256); err != nil {
			return out, err
		}
		header, e := VerifyChainHeaderJSON(ctx, proof.ChainKey, block.Header)
		if e != nil {
			return out, e
		}
		if !sameReceiptAnchor(header.Block, headers[i].Block) {
			return out, errors.New("SAE_WINDOW_BLOCK_IDENTITY")
		}
		transactions += len(header.Block.Transactions)
		if transactions > limits.MaxTransactions {
			return out, errors.New("RECEIPT_PROOF_TRANSACTION_BUDGET")
		}
		for _, receipt := range block.Receipts {
			if !chargeReceipt(&budget.remaining, receipt) {
				return out, errors.New("RECEIPT_PROOF_BUDGET")
			}
		}
		if e = verifyExecutionBody(ctx, proof.ChainKey, header, block, &budget); e != nil {
			return out, e
		}
		values, e := canonicalReceiptEncodings(ctx, receiptCoreth, header.Block, block.Receipts)
		if e != nil {
			return out, e
		}
		encoded = append(encoded, values...)
		for _, r := range block.Receipts {
			raw := common.FromHex(r.LogsBloom)
			for j := range bloom {
				bloom[j] |= raw[j]
			}
		}
		if start+uint64(i) == targetHeight {
			targetReceipts = block.Receipts
		}
	}
	root := types.DeriveSha(encoded, trie.NewStackTrie(nil))
	if root != common.HexToHash(anchor.ReceiptsRoot) {
		return out, errors.New("SAE_RECEIPT_ROOT_MISMATCH")
	}
	if "0x"+common.Bytes2Hex(bloom[:]) != anchor.LogsBloom {
		return out, errors.New("SAE_WINDOW_BLOOM_MISMATCH")
	}
	if err = ctx.Err(); err != nil {
		return out, err
	}
	stateBlock := headers[end-start].Block
	out = VerifiedReceiptCoverage{
		ChainKey: proof.ChainKey, Block: target, CanonicalBlock: target, CanonicalReceipts: targetReceipts,
		CanonicalBodyProven: true, CanonicalReceiptsProven: true, WholeBlockReceiptEventsProven: true, SystemObservationCoverage: "all-evm-receipts-committed",
		NativeSemanticsID: "avalanche-mainnet/coreth-helicon-sae@983815fd993acf846a27749b598f0fea4d876a75", ActivationID: "helicon-2026-09-22T15:00:00Z",
		ReceiptAuthority: "sae-settling-window", ReceiptRoot: root.Hex(), WindowStart: start, WindowEnd: end,
		StateAnchor: &ExecutionStateAnchor{Height: end, BlockHash: stateBlock.Hash, StateRoot: anchor.StateRoot, SettlingHeight: anchorHeight, SettlingHash: anchor.Block.Hash, Authority: "coreth-sae-settled-state-root", AccountEncodingID: "coreth-account-held-xchain-rlp/v1"},
	}
	if err = attachNativeSemantics(&out, headers[targetHeight-start]); err != nil {
		return VerifiedReceiptCoverage{}, err
	}
	out.ProofBytesUsed = limits.MaxBytes - budget.remaining
	proofCopy := proof
	out.Evidence = &NativeReceiptEvidence{SchemaVersion: 1, ChainKey: proof.ChainKey, GenesisHash: proof.GenesisHash, NativeSemanticsID: out.NativeSemanticsID, Kind: "sae-settling-window", SAE: &proofCopy}
	return out, nil
}
