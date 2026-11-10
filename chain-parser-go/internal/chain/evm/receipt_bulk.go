package evm

import (
	"context"
	"errors"

	"xone/chain-parser-go/internal/rpc"
)

type receiptValidationError struct{ cause error }

func (e *receiptValidationError) Error() string { return e.cause.Error() }
func (e *receiptValidationError) Unwrap() error { return e.cause }

const (
	ReceiptsModePerTx    = "per-tx"
	ReceiptsModeBulkAuto = "bulk-auto"
)

// ReadReceiptsWithMode is an explicit opt-in bulk path. Empty mode preserves
// legacy per-transaction I/O. Both paths return only a complete root-verified
// receipt set. The normal caller still owns witness and final anchor checks.
// Method availability is an RPC capability, not chain-consensus verification.
func (a *Adapter) ReadReceiptsWithMode(ctx context.Context, block RawBlock, mode string) ([]Receipt, error) {
	switch mode {
	case "", ReceiptsModePerTx, ReceiptsModeBulkAuto:
	default:
		return nil, errors.New("RECEIPTS_MODE_UNSUPPORTED")
	}
	if ctx.Err() != nil {
		return nil, errors.New("RPC_DEADLINE")
	}
	if err := validateBlockTransactions(block); err != nil {
		return nil, err
	}
	if a.Client.MaxBytes < 0 || len(block.Transactions) > a.Client.MaxBytes/128 {
		return nil, errors.New("RECEIPT_BLOCK_BUDGET")
	}
	var receipts []Receipt
	var err error
	if mode == ReceiptsModeBulkAuto {
		// A hash selector pins this request to the exact anchor. Invalid params is
		// not evidence of unsupported methods and must not trigger fallback.
		err = a.Client.Call(ctx, "eth_getBlockReceipts", []any{block.Hash}, &receipts)
		if err != nil && rpc.Classify(err) != rpc.Unsupported {
			return nil, err
		}
		if err == nil {
			if len(receipts) != len(block.Transactions) {
				return nil, &receiptValidationError{cause: errors.New("INCOMPLETE_RECEIPTS")}
			}
			remaining := a.Client.MaxBytes
			for i := range receipts {
				if ctx.Err() != nil {
					return nil, errors.New("RPC_DEADLINE")
				}
				if !chargeReceipt(&remaining, receipts[i]) {
					return nil, errors.New("RECEIPT_BLOCK_BUDGET")
				}
				canonicalizeReadReceipt(&receipts[i])
			}
		}
	}
	if mode != ReceiptsModeBulkAuto || err != nil {
		receipts, err = a.readReceipts(ctx, block)
		if err != nil {
			return nil, err
		}
	}
	if err = VerifyChainReceiptRoot(ctx, a.Chain.Key, block, receipts); err != nil {
		return nil, &receiptValidationError{cause: err}
	}
	return receipts, nil
}

func canonicalizeReadReceipt(receipt *Receipt) {
	receipt.BlockHash = canonicalReceiptIdentity(receipt.BlockHash)
	receipt.TxHash = canonicalReceiptIdentity(receipt.TxHash)
	for i := range receipt.Logs {
		receipt.Logs[i].BlockHash = canonicalReceiptIdentity(receipt.Logs[i].BlockHash)
		receipt.Logs[i].TxHash = canonicalReceiptIdentity(receipt.Logs[i].TxHash)
	}
}
