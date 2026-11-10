package evm

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"fmt"
	"hash"
	"slices"
	"strings"
)

// Normalize only actual 32-byte hexadecimal identities; legacy synthetic test
// aliases remain exact strings. Position quantities still use canonicalIndex.
func canonicalReceiptIdentity(value string) string {
	if len(value) != 66 || !strings.HasPrefix(value, "0x") {
		return value
	}
	if _, err := hex.DecodeString(value[2:]); err != nil {
		return value
	}
	return strings.ToLower(value)
}

func canonicalReceiptBlock(block RawBlock) RawBlock {
	block.Hash = canonicalReceiptIdentity(block.Hash)
	block.ParentHash = canonicalReceiptIdentity(block.ParentHash)
	block.Transactions = append([]string{}, block.Transactions...)
	for i := range block.Transactions {
		block.Transactions[i] = canonicalReceiptIdentity(block.Transactions[i])
	}
	return block
}

// A block's aggregate retained receipts share the existing MaxBytes input
// budget. The charge includes all retained string data and conservative struct,
// slice and pointer overhead; it is intentionally stricter than wire size.
func chargeReceipt(remaining *int, receipt Receipt) bool {
	charge := func(n int) bool {
		if n < 0 || n > *remaining {
			return false
		}
		*remaining -= n
		return true
	}
	if !charge(256 + len(receipt.BlockHash) + len(receipt.TxHash) + len(receipt.TxIndex) + len(receipt.Type) + len(receipt.Status) + len(receipt.PostState) + len(receipt.CumulativeGasUsed) + len(receipt.LogsBloom) + len(receipt.GasUsed) + len(receipt.DepositNonce) + len(receipt.DepositReceiptVersion)) {
		return false
	}
	for _, log := range receipt.Logs {
		if !charge(192 + len(log.BlockHash) + len(log.TxHash) + len(log.TxIndex) + len(log.Index) + len(log.Address) + len(log.Data)) {
			return false
		}
		for _, topic := range log.Topics {
			if !charge(16 + len(topic)) {
				return false
			}
		}
	}
	return true
}
func (a *Adapter) readReceipts(ctx context.Context, block RawBlock) ([]Receipt, error) {
	if a.Client.MaxBytes < 0 || len(block.Transactions) > a.Client.MaxBytes/128 {
		return nil, errors.New("RECEIPT_BLOCK_BUDGET")
	}
	remaining := a.Client.MaxBytes
	receipts := make([]Receipt, 0, len(block.Transactions))
	for _, tx := range block.Transactions {
		if ctx.Err() != nil {
			return nil, errors.New("RPC_DEADLINE")
		}
		var receipt Receipt
		if err := a.Client.Call(ctx, "eth_getTransactionReceipt", []any{tx}, &receipt); err != nil {
			return nil, err
		}
		if !chargeReceipt(&remaining, receipt) {
			return nil, errors.New("RECEIPT_BLOCK_BUDGET")
		}
		canonicalizeReadReceipt(&receipt)
		receipts = append(receipts, receipt)
	}
	return receipts, nil
}

// Length-prefix every field while streaming into SHA256. No JSON marshal,
// generic map decode or copied whole-block log representation is retained.
func hashReceiptString(h hash.Hash, value string) {
	var width [8]byte
	binary.BigEndian.PutUint64(width[:], uint64(len(value)))
	h.Write(width[:])
	h.Write([]byte(value))
}
func receiptDigest(ctx context.Context, block RawBlock, receipts []Receipt) (string, error) {
	ordered := make([]*Receipt, len(block.Transactions))
	for i := range receipts {
		if ctx.Err() != nil {
			return "", errors.New("RPC_DEADLINE")
		}
		index, err := canonicalIndex(receipts[i].TxIndex)
		if err != nil || index >= uint64(len(ordered)) || ordered[index] != nil {
			return "", errors.New("RECEIPT_IDENTITY")
		}
		ordered[index] = &receipts[i]
	}
	h := sha256.New()
	hashReceiptString(h, "xone-receipt-witness/1")
	for _, receipt := range ordered {
		if receipt == nil {
			return "", errors.New("INCOMPLETE_RECEIPTS")
		}
		if ctx.Err() != nil {
			return "", errors.New("RPC_DEADLINE")
		}
		hashReceiptString(h, canonicalReceiptIdentity(receipt.BlockHash))
		hashReceiptString(h, canonicalReceiptIdentity(receipt.TxHash))
		hashReceiptString(h, receipt.TxIndex)
		hashReceiptString(h, fmt.Sprint(len(receipt.Logs)))
		for _, log := range receipt.Logs {
			if ctx.Err() != nil {
				return "", errors.New("RPC_DEADLINE")
			}
			if err := validateReceiptLogEncoding(log); err != nil {
				return "", err
			}
			for _, field := range []string{canonicalReceiptIdentity(log.BlockHash), canonicalReceiptIdentity(log.TxHash), log.TxIndex, log.Index, strings.ToLower(log.Address), strings.ToLower(log.Data), fmt.Sprint(log.Removed), fmt.Sprint(len(log.Topics))} {
				hashReceiptString(h, field)
			}
			for _, topic := range log.Topics {
				hashReceiptString(h, strings.ToLower(topic))
			}
		}
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func sameReceiptAnchor(expected, actual RawBlock) bool {
	return canonicalReceiptIdentity(actual.ReceiptsRoot) == canonicalReceiptIdentity(expected.ReceiptsRoot) && actual.Number == expected.Number && canonicalReceiptIdentity(actual.Hash) == canonicalReceiptIdentity(expected.Hash) && canonicalReceiptIdentity(actual.ParentHash) == canonicalReceiptIdentity(expected.ParentHash) && actual.Timestamp == expected.Timestamp && slices.EqualFunc(actual.Transactions, expected.Transactions, func(a, b string) bool { return canonicalReceiptIdentity(a) == canonicalReceiptIdentity(b) })
}

// With multiple configured endpoints, every healthy endpoint must independently
// reproduce the complete block/receipt view. No majority can outvote a differing
// source, and losing the second qualified source does not silently reduce proof.
// Each complete-read path also verifies the consensus receipt root. The roots
// remain anchored to RPC headers rather than independently verified consensus.
func (a *Adapter) verifyReceiptWitnesses(ctx context.Context, block RawBlock, receipts []Receipt) error {
	configured, providers := a.Client.ProviderClients()
	if configured < 2 {
		return nil
	}
	if len(providers) < 2 {
		return errors.New("RECEIPT_WITNESS_SOURCE_REQUIRED")
	}
	expected, err := receiptDigest(ctx, block, receipts)
	if err != nil {
		return err
	}
	for _, provider := range providers {
		witness := Adapter{Client: provider, Chain: a.Chain}
		anchor, err := witness.Block(ctx, block.Number)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_UNAVAILABLE")
		}
		if !sameReceiptAnchor(block, anchor) {
			return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
		}
		observed, err := witness.ReadReceiptsWithMode(ctx, anchor, a.Chain.ReceiptsMode)
		if err != nil {
			var invalid *receiptValidationError
			if errors.As(err, &invalid) {
				return errors.New("RECEIPT_WITNESS_INVALID")
			}
			return errors.New("RECEIPT_WITNESS_UNAVAILABLE")
		}
		if VerifyChainReceiptRoot(ctx, a.Chain.Key, anchor, observed) != nil {
			return errors.New("RECEIPT_WITNESS_INVALID")
		}
		digest, err := receiptDigest(ctx, anchor, observed)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_INVALID")
		}
		if digest != expected {
			return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
		}
		// Transaction-receipt RPC has no blockHash selector. Verify the endpoint's
		// canonical block again after its last receipt, under the outer total budget.
		after, err := witness.Block(ctx, block.Number)
		if err != nil {
			return errors.New("RECEIPT_WITNESS_UNAVAILABLE")
		}
		if !sameReceiptAnchor(anchor, after) {
			return errors.New("RECEIPT_PROVIDER_DISAGREEMENT")
		}
	}
	return nil
}
