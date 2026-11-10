package evm

import (
	"context"
	"encoding/json"
	"errors"
	"math/big"
	"strings"

	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/trie"
)

// This policy verifies Ethereum execution commitments against the RPC header.
// It does not authenticate a consensus checkpoint or a provider failure domain.
// Other chains require their own header/body/receipt rule profile.
func (a *Adapter) ethereumBlock(ctx context.Context, tag string) (RawBlock, error) {
	var block RawBlock
	if a.Chain.BlockIntegrity != "ethereum-header-transactions-v1" || a.Chain.Key != "evm:1" {
		return block, errors.New("BLOCK_INTEGRITY_PROFILE_UNSUPPORTED")
	}
	if a.Client.HTTP.Timeout <= 0 || a.Client.MaxBytes <= 0 {
		return block, errors.New("BLOCK_INTEGRITY_BUDGET")
	}
	ctx, cancel := context.WithTimeout(ctx, a.Client.HTTP.Timeout)
	defer cancel()
	var raw json.RawMessage
	if err := a.Client.Call(ctx, "eth_getBlockByNumber", []any{tag, true}, &raw); err != nil {
		return block, err
	}
	var header types.Header
	var body struct {
		Number       string            `json:"number"`
		Hash         string            `json:"hash"`
		ParentHash   string            `json:"parentHash"`
		Timestamp    string            `json:"timestamp"`
		ReceiptsRoot string            `json:"receiptsRoot"`
		Transactions []json.RawMessage `json:"transactions"`
	}
	if json.Unmarshal(raw, &header) != nil || header.Number == nil || !header.Number.IsUint64() || json.Unmarshal(raw, &body) != nil {
		return block, errors.New("BLOCK_INTEGRITY_HEADER")
	}
	block = RawBlock{Number: body.Number, Hash: body.Hash, ParentHash: body.ParentHash, Timestamp: body.Timestamp, ReceiptsRoot: body.ReceiptsRoot, Transactions: []string{}}
	if !receiptHex(block.Hash, 32) || !strings.EqualFold(header.Hash().Hex(), block.Hash) {
		return block, errors.New("BLOCK_INTEGRITY_HEADER_HASH")
	}
	if err := validateV2CompleteHeader(block); err != nil {
		return block, err
	}
	number, err := canonicalIndex(block.Number)
	if err != nil || number != header.Number.Uint64() || block.Timestamp != "0x"+new(big.Int).SetUint64(header.Time).Text(16) {
		return block, errors.New("BLOCK_INTEGRITY_HEADER_POSITION")
	}
	if strings.HasPrefix(tag, "0x") && tag != block.Number {
		return block, errors.New("BLOCK_INTEGRITY_HEIGHT")
	}
	if body.Transactions == nil {
		return block, errors.New("BLOCK_TRANSACTIONS_MISSING")
	}
	// One full-block RPC avoids requiring a nonstandard raw-transaction method
	// or multiplying the network budget by the block's transaction count.
	remaining := a.Client.MaxBytes - len(raw)
	if remaining < 0 || len(body.Transactions) > remaining/1024 {
		return block, errors.New("BLOCK_INTEGRITY_BUDGET")
	}
	txs := make(types.Transactions, 0, len(body.Transactions))
	seen := map[string]bool{}
	for i, encoded := range body.Transactions {
		if err := ctx.Err(); err != nil {
			return block, err
		}
		if len(encoded) > (remaining-1024)/2 {
			return block, errors.New("BLOCK_INTEGRITY_BUDGET")
		}
		remaining -= 2*len(encoded) + 1024
		var position struct {
			Hash        string `json:"hash"`
			BlockHash   string `json:"blockHash"`
			BlockNumber string `json:"blockNumber"`
			Index       string `json:"transactionIndex"`
		}
		tx := new(types.Transaction)
		if json.Unmarshal(encoded, &position) != nil || json.Unmarshal(encoded, tx) != nil {
			return block, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
		index, err := canonicalIndex(position.Index)
		if err != nil || index != uint64(i) || position.BlockNumber != block.Number || !strings.EqualFold(position.BlockHash, block.Hash) {
			return block, errors.New("BLOCK_INTEGRITY_TRANSACTION_POSITION")
		}
		if !receiptHex(position.Hash, 32) || !strings.EqualFold(tx.Hash().Hex(), position.Hash) || seen[strings.ToLower(position.Hash)] {
			return block, errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
		}
		if tx.Protected() && tx.ChainId().Cmp(big.NewInt(1)) != 0 {
			return block, errors.New("BLOCK_INTEGRITY_TRANSACTION_CHAIN")
		}
		seen[strings.ToLower(position.Hash)] = true
		block.Transactions = append(block.Transactions, position.Hash)
		txs = append(txs, tx)
	}
	if err := ctx.Err(); err != nil {
		return block, err
	}
	if types.DeriveSha(txs, trie.NewStackTrie(nil)) != header.TxHash {
		return block, errors.New("BLOCK_INTEGRITY_TRANSACTIONS_ROOT")
	}
	if err := ctx.Err(); err != nil {
		return block, err
	}
	return canonicalReceiptBlock(block), nil
}
