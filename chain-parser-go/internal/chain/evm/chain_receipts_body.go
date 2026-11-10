package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/trie"
)

// ChainBodyCommitmentsV1 is opt-in; the default Adapter path remains legacy.
// Its standard execution-header RLP profile applies only to the fingerprints
// below. It is an RPC-anchored commitment proof, not a consensus checkpoint.
const ChainBodyCommitmentsV1 = "known-evm-header-raw-transactions-v1"

func knownBodyGenesis(chainKey string) (string, error) {
	switch chainKey {
	case "evm:1":
		return "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3", nil
	case "evm:10":
		return "0x7ca38a1916c42007829c55e69d3e9a73265554b586a499015373241b8a3fa48b", nil
	case "evm:8453":
		return "0xf712aa9241cc24369b143cf6dce85f0902a9731e70d66818a3a5845b296c73dd", nil
	case "evm:56":
		return "0x0d21840abff46b96c84b2ac9e10e4f5cdaeb5693cb665db62a2f3b02d2d57b5b", nil
	case "evm:137":
		return "0xa9c28ce2141b56c474f1dc504bee9b01eb1bd7d1a507580d5519d4437a97de1b", nil
	case "evm:42161":
		return "0x7ee576b35482195fc49205cec9af72ce14f003b9ae69f6ba0faef4514be8b442", nil
	case "evm:43114":
		return "0x31ced5b9beb7f8782b014660da0cb18cc409f121f408186886e1ca3e8eeca96b", nil
	default:
		return "", errors.New("BLOCK_BODY_CHAIN_UNSUPPORTED")
	}
}

// ReadChainCommittedBlock binds every RPC transaction hash to its raw bytes,
// then binds the ordered raw bytes to transactionsRoot and the standard header
// RLP hash. Native transactions are opaque committed bytes, so no geth native
// transaction parser or unknown-type-to-legacy conversion is involved.
//
// Native Header RLP follows VerifyChainHeaderJSON. Coreth SAE state/receipt
// roots require a separate settling-window proof even after this body succeeds.
// Missing raw RPC support, any malformed response, and budget exhaustion fail
// closed. The caller still owns independent witnesses and the final anchor.
func (a *Adapter) ReadChainCommittedBlock(ctx context.Context, tag string) (RawBlock, error) {
	var empty RawBlock
	if a.Chain.BlockIntegrity != ChainBodyCommitmentsV1 {
		return empty, errors.New("BLOCK_INTEGRITY_PROFILE_UNSUPPORTED")
	}
	genesis, err := knownBodyGenesis(a.Chain.Key)
	if err != nil {
		return empty, err
	}
	if !strings.EqualFold(genesis, a.Chain.Genesis) {
		return empty, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	if a.Client == nil || a.Client.HTTP == nil || a.Client.HTTP.Timeout <= 0 || a.Client.MaxBytes <= 0 {
		return empty, errors.New("BLOCK_INTEGRITY_BUDGET")
	}
	ctx, cancel := context.WithTimeout(ctx, a.Client.HTTP.Timeout)
	defer cancel()
	// Hold one healthy endpoint for this entire bounded proof. ProviderClients
	// preserves its recorder and policy while giving this call its own byte cap.
	_, providers := a.Client.ProviderClients()
	if len(providers) == 0 {
		return empty, errors.New("NO_HEALTHY_PROVIDER")
	}
	provider := providers[0]
	remaining := a.Client.MaxBytes
	charge := func(n int) error {
		if n < 0 || n > remaining {
			return errors.New("BLOCK_INTEGRITY_BUDGET")
		}
		remaining -= n
		return nil
	}
	call := func(method string, params []any) (json.RawMessage, error) {
		if remaining <= 128 {
			return nil, errors.New("BLOCK_INTEGRITY_BUDGET")
		}
		provider.MaxBytes = remaining
		var raw json.RawMessage
		if err := provider.Call(ctx, method, params, &raw); err != nil {
			return nil, err
		}
		if err := charge(len(raw) + 128); err != nil {
			return nil, err
		}
		if err := rejectDuplicateReceiptKeys(raw); err != nil {
			return nil, errors.New("BLOCK_BODY_JSON_INVALID")
		}
		return raw, nil
	}
	identity, err := call("eth_chainId", []any{})
	if err != nil {
		return empty, err
	}
	var chainID string
	if json.Unmarshal(identity, &chainID) != nil {
		return empty, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	id, err := canonicalIndex(chainID)
	if err != nil || a.Chain.Key != "evm:"+strconv.FormatUint(id, 10) {
		return empty, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	origin, err := call("eth_getBlockByNumber", []any{"0x0", false})
	if err != nil {
		return empty, err
	}
	var originHash struct{ Hash string }
	if json.Unmarshal(origin, &originHash) != nil || !strings.EqualFold(originHash.Hash, genesis) {
		return empty, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	raw, err := call("eth_getBlockByNumber", []any{tag, false})
	if err != nil {
		return empty, err
	}
	headerProof, err := VerifyChainHeaderJSON(ctx, a.Chain.Key, raw)
	if err != nil {
		return empty, err
	}
	block := headerProof.Block
	if err = validateV2CompleteHeader(block); err != nil {
		return empty, err
	}
	if strings.HasPrefix(tag, "0x") && tag != block.Number {
		return empty, errors.New("BLOCK_INTEGRITY_HEIGHT")
	}
	if err = validateBlockTransactions(block); err != nil {
		return empty, err
	}
	if len(block.Transactions) > remaining/128 {
		return empty, errors.New("BLOCK_INTEGRITY_BUDGET")
	}
	// Account for retained header/body/transaction-slice data as well as wire
	// results. Raw payloads below are charged separately before retaining them.
	if err = charge(len(raw) + 128*len(block.Transactions)); err != nil {
		return empty, err
	}
	encoded := make(encodedChainReceipts, 0, len(block.Transactions))
	for _, txHash := range block.Transactions {
		if err = ctx.Err(); err != nil {
			return empty, err
		}
		if !receiptHex(txHash, 32) {
			return empty, errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
		}
		raw, err = call("eth_getRawTransactionByHash", []any{txHash})
		if err != nil {
			return empty, err
		}
		var value string
		if json.Unmarshal(raw, &value) != nil || !receiptHex(value, -1) || len(value) <= 2 {
			return empty, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
		width := (len(value) - 2) / 2
		if err = charge(width + 128); err != nil {
			return empty, err
		}
		payload, err := hex.DecodeString(value[2:])
		if err != nil {
			return empty, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
		if crypto.Keccak256Hash(payload) != common.HexToHash(txHash) {
			return empty, errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
		}
		encoded = append(encoded, payload)
	}
	if err = ctx.Err(); err != nil {
		return empty, err
	}
	if types.DeriveSha(encoded, trie.NewStackTrie(nil)) != common.HexToHash(headerProof.TransactionsRoot) {
		return empty, errors.New("BLOCK_INTEGRITY_TRANSACTIONS_ROOT")
	}
	if err = ctx.Err(); err != nil {
		return empty, err
	}
	return block, nil
}
