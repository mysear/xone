package evm

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
)

type BorReceiptProof struct {
	ChainKey         string            `json:"chain_key"`
	GenesisHash      string            `json:"genesis_hash"`
	Header           json.RawMessage   `json:"header"`
	TransactionsJSON []json.RawMessage `json:"transactions_json,omitempty"`
	RawTransactions  []string          `json:"raw_transactions,omitempty"`
}

type ReceiptEventScope string

const (
	CanonicalTransactionEvents ReceiptEventScope = "canonical-transaction-events"
	WholeBlockEvents           ReceiptEventScope = "whole-block-events"
)

type BusinessLogSelection struct {
	Scope              ReceiptEventScope `json:"scope"`
	Qualified          bool              `json:"qualified"`
	Logs               []Log             `json:"logs"`
	UnprovenSystemLogs []Log             `json:"unproven_system_logs"`
}

// SelectBusinessLogs never includes uncommitted system observations in Logs.
// Even when no observed system log matches a business address, it does not
// claim absence of omitted system events. Whole-block admission remains closed.
func (c VerifiedReceiptCoverage) SelectBusinessLogs(addresses []string, scope ReceiptEventScope) (BusinessLogSelection, error) {
	out := BusinessLogSelection{Scope: scope, Logs: []Log{}, UnprovenSystemLogs: []Log{}}
	if scope != CanonicalTransactionEvents && scope != WholeBlockEvents {
		return out, errors.New("RECEIPT_EVENT_SCOPE_INVALID")
	}
	if !c.CanonicalBodyProven || !c.CanonicalReceiptsProven {
		return out, errors.New("RECEIPT_CANONICAL_SCOPE_UNPROVEN")
	}
	if scope == WholeBlockEvents {
		if err := c.RequireWholeBlockEvents(); err != nil {
			return out, err
		}
	}
	if len(addresses) == 0 || len(addresses) > 4096 {
		return out, errors.New("RECEIPT_BUSINESS_SCOPE_INVALID")
	}
	selected := map[string]bool{}
	for _, address := range addresses {
		if !receiptHex(address, 20) {
			return out, errors.New("RECEIPT_BUSINESS_SCOPE_INVALID")
		}
		selected[strings.ToLower(address)] = true
	}
	for _, receipt := range c.CanonicalReceipts {
		for _, log := range receipt.Logs {
			if selected[strings.ToLower(log.Address)] {
				out.Logs = append(out.Logs, log)
			}
		}
	}
	for _, observation := range c.SystemObservations {
		for _, log := range observation.Receipt.Logs {
			if selected[strings.ToLower(log.Address)] {
				out.UnprovenSystemLogs = append(out.UnprovenSystemLogs, log)
			}
		}
	}
	if len(out.UnprovenSystemLogs) > 0 {
		return out, errors.New("RECEIPT_BUSINESS_SYSTEM_EVENTS_UNPROVEN")
	}
	out.Qualified = true
	return out, nil
}

func derivedBorReceiptHash(block RawBlock) (string, error) {
	number, err := canonicalIndex(block.Number)
	if err != nil || !receiptHex(block.Hash, 32) {
		return "", errors.New("BOR_BLOCK_IDENTITY_INVALID")
	}
	var height [8]byte
	binary.BigEndian.PutUint64(height[:], number)
	key := append([]byte("matic-bor-receipt-"), height[:]...)
	key = append(key, common.FromHex(block.Hash)...)
	return crypto.Keccak256Hash(key).Hex(), nil
}

// VerifyBorReceiptCoverage proves canonical membership against both body and
// receipt roots, while returning the non-member state-sync receipt as a typed
// RPC observation. Splitting is determined before root verification by the
// official derived identity, exact final position and pre-Madhugiri activation;
// there is no root-mismatch retry or arbitrary zero-gas suffix deletion.
func VerifyBorReceiptCoverage(ctx context.Context, target RawBlock, receipts []Receipt, proof BorReceiptProof, limits ReceiptProofLimits) (VerifiedReceiptCoverage, error) {
	var out VerifiedReceiptCoverage
	if err := ctx.Err(); err != nil {
		return out, err
	}
	if err := limits.validate(); err != nil {
		return out, err
	}
	genesis, _ := knownBodyGenesis("evm:137")
	if proof.ChainKey != "evm:137" || !strings.EqualFold(proof.GenesisHash, genesis) {
		return out, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	budget := receiptProofBudget{remaining: limits.MaxBytes}
	if err := budget.charge(2*len(proof.Header) + 256); err != nil {
		return out, err
	}
	header, err := VerifyChainHeaderJSON(ctx, proof.ChainKey, proof.Header)
	if err != nil {
		return out, err
	}
	if !sameReceiptAnchor(target, header.Block) {
		return out, errors.New("BOR_PROOF_TARGET_IDENTITY")
	}
	if err = ValidateReceipts(target, receipts); err != nil {
		return out, err
	}
	if len(receipts) > limits.MaxTransactions {
		return out, errors.New("RECEIPT_PROOF_TRANSACTION_BUDGET")
	}
	for _, receipt := range receipts {
		if !chargeReceipt(&budget.remaining, receipt) {
			return out, errors.New("RECEIPT_PROOF_BUDGET")
		}
	}
	height, _ := canonicalIndex(target.Number)
	preMadhugiri := height < 80084800
	derived, err := derivedBorReceiptHash(target)
	if err != nil {
		return out, err
	}
	count := len(target.Transactions)
	systemIndex := -1
	for i, hash := range target.Transactions {
		if strings.EqualFold(hash, derived) {
			if !preMadhugiri || i != count-1 || systemIndex >= 0 {
				return out, errors.New("BOR_SYSTEM_POSITION_INVALID")
			}
			systemIndex = i
		}
	}
	canonical := target
	canonical.Transactions = append([]string{}, target.Transactions...)
	observations := []SystemReceiptObservation{}
	canonicalReceipts := make([]Receipt, 0, len(receipts))
	for _, receipt := range receipts {
		index, _ := canonicalIndex(receipt.TxIndex)
		if int(index) == systemIndex {
			if receipt.invalidChainEncoding || receipt.GasUsed != "0x0" || receipt.DepositNonce != "" || receipt.DepositReceiptVersion != "" || (receipt.Type != "" && receipt.Type != "0x0") {
				return out, errors.New("BOR_SYSTEM_ENCODING_INVALID")
			}
			if _, err = consensusReceipt(&receipt); err != nil {
				return out, err
			}
			observations = append(observations, SystemReceiptObservation{Receipt: receipt, Authority: "bor-derived-identity/rpc-log-observation", LogsContentCommitted: false})
		} else {
			canonicalReceipts = append(canonicalReceipts, receipt)
		}
	}
	if systemIndex >= 0 {
		canonical.Transactions = canonical.Transactions[:systemIndex]
	}
	if len(proof.TransactionsJSON) > 0 && len(proof.RawTransactions) > 0 {
		return out, errors.New("BLOCK_BODY_AUTHORITY_AMBIGUOUS")
	}
	nativeBody := ExecutionReceiptBlock{RawTransactions: proof.RawTransactions}
	if len(proof.TransactionsJSON) > 0 {
		if len(proof.TransactionsJSON) != len(target.Transactions) {
			return out, errors.New("BOR_JSON_BODY_INCOMPLETE")
		}
		if systemIndex >= 0 {
			var pos struct{ Hash, BlockHash, BlockNumber, TransactionIndex string }
			if json.Unmarshal(proof.TransactionsJSON[systemIndex], &pos) != nil || !strings.EqualFold(pos.Hash, derived) || canonicalReceiptIdentity(pos.BlockHash) != target.Hash || pos.BlockNumber != target.Number || pos.TransactionIndex != fmt.Sprintf("0x%x", systemIndex) {
				return out, errors.New("BOR_SYSTEM_POSITION_INVALID")
			}
		}
		nativeBody.TransactionsJSON = proof.TransactionsJSON[:len(canonical.Transactions)]
	}
	canonicalHeader := header
	canonicalHeader.Block = canonical
	if err = verifyExecutionBody(ctx, proof.ChainKey, canonicalHeader, nativeBody, &budget); err != nil {
		return out, err
	}
	if err = VerifyChainReceiptRoot(ctx, proof.ChainKey, canonical, canonicalReceipts); err != nil {
		return out, err
	}
	activation := "bor-post-madhugiri-native-state-sync"
	systemCoverage := "native-system-receipts-root-committed"
	if preMadhugiri {
		activation = "bor-pre-madhugiri-canonical-only"
		systemCoverage = "rpc-system-observations-uncommitted-no-absence-proof"
	}
	out = VerifiedReceiptCoverage{
		ChainKey: proof.ChainKey, Block: target, CanonicalBlock: canonical, CanonicalReceipts: canonicalReceipts, CanonicalBodyProven: true, CanonicalReceiptsProven: true,
		WholeBlockReceiptEventsProven: !preMadhugiri, SystemObservations: observations, SystemObservationCoverage: systemCoverage,
		NativeSemanticsID: "polygon-mainnet/" + activation + "@f8fc9081e424cea8ec0b6ec44d2e66c137970e8b", ActivationID: activation, ReceiptAuthority: "bor-canonical-header-roots", ReceiptRoot: header.ReceiptsRoot, WindowStart: height, WindowEnd: height,
		StateAnchor: &ExecutionStateAnchor{Height: height, BlockHash: target.Hash, StateRoot: header.StateRoot, SettlingHeight: height, SettlingHash: target.Hash, Authority: "bor-post-execution-header-state-root", AccountEncodingID: "ethereum-account-rlp/v1"},
	}
	if err = attachNativeSemantics(&out, header); err != nil {
		return VerifiedReceiptCoverage{}, err
	}
	out.ProofBytesUsed = limits.MaxBytes - budget.remaining
	proofCopy := proof
	out.Evidence = &NativeReceiptEvidence{SchemaVersion: 1, ChainKey: proof.ChainKey, GenesisHash: proof.GenesisHash, NativeSemanticsID: out.NativeSemanticsID, Kind: "bor-canonical-system-isolation", Bor: &proofCopy, BorObservedReceipts: receipts}
	return out, nil
}
