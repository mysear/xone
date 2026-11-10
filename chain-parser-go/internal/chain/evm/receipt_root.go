package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/trie"
)

// Preserve optional-field presence: omitted legacy type is compatible, while
// supplied type/status/root values must be valid and outcomes unambiguous.
func (r *Receipt) UnmarshalJSON(data []byte) error {
	if err := rejectDuplicateReceiptKeys(data); err != nil {
		return err
	}
	type fields Receipt
	var decoded fields
	envelope := struct {
		*fields
		Type                  json.RawMessage `json:"type"`
		Status                json.RawMessage `json:"status"`
		PostState             json.RawMessage `json:"root"`
		GasUsed               json.RawMessage `json:"gasUsed"`
		DepositNonce          json.RawMessage `json:"depositNonce"`
		DepositReceiptVersion json.RawMessage `json:"depositReceiptVersion"`
	}{fields: &decoded}
	if err := json.Unmarshal(data, &envelope); err != nil {
		return err
	}
	if envelope.Type != nil {
		if json.Unmarshal(envelope.Type, &decoded.Type) != nil || decoded.Type == "" {
			decoded.invalidTypeEncoding = true
		}
	}
	if envelope.Status != nil && (json.Unmarshal(envelope.Status, &decoded.Status) != nil || decoded.Status == "") {
		decoded.invalidOutcomeEncoding = true
	}
	if envelope.PostState != nil && (json.Unmarshal(envelope.PostState, &decoded.PostState) != nil || decoded.PostState == "") {
		decoded.invalidOutcomeEncoding = true
	}
	if envelope.Status != nil && envelope.PostState != nil {
		decoded.invalidOutcomeEncoding = true
	}
	for _, field := range []struct {
		raw   json.RawMessage
		value *string
	}{
		{envelope.GasUsed, &decoded.GasUsed},
		{envelope.DepositNonce, &decoded.DepositNonce},
		{envelope.DepositReceiptVersion, &decoded.DepositReceiptVersion},
	} {
		if field.raw != nil {
			if json.Unmarshal(field.raw, field.value) != nil {
				decoded.invalidChainEncoding = true
			}
			if _, err := canonicalIndex(*field.value); err != nil {
				decoded.invalidChainEncoding = true
			}
		}
	}
	*r = Receipt(decoded)
	return nil
}

// VerifyReceiptRoot verifies the consensus receipt encoding against the root
// advertised by this RPC block. The header itself is still an RPC trust anchor;
// this is not independent header, transaction-root or consensus verification.
// Only Ethereum receipt types 0..4 are supported. Chain-specific envelopes must
// receive an explicit adapter instead of falling back to the legacy encoding.
func VerifyReceiptRoot(ctx context.Context, block RawBlock, receipts []Receipt) error {
	if ctx.Err() != nil {
		return errors.New("RPC_DEADLINE")
	}
	if err := ValidateReceipts(block, receipts); err != nil {
		return err
	}
	if !receiptHex(block.ReceiptsRoot, 32) {
		return errors.New("RECEIPT_ROOT_MISSING_OR_INVALID")
	}
	ordered := make(types.Receipts, len(receipts))
	for i := range receipts {
		if ctx.Err() != nil {
			return errors.New("RPC_DEADLINE")
		}
		r := &receipts[i]
		index, _ := canonicalIndex(r.TxIndex) // checked by ValidateReceipts
		decoded, err := consensusReceipt(r)
		if err != nil {
			return err
		}
		ordered[index] = decoded
	}
	var previous uint64
	for _, receipt := range ordered {
		if receipt.CumulativeGasUsed <= previous {
			return errors.New("RECEIPT_CUMULATIVE_GAS_ORDER")
		}
		previous = receipt.CumulativeGasUsed
	}
	root := types.DeriveSha(ordered, trie.NewStackTrie(nil))
	if ctx.Err() != nil {
		return errors.New("RPC_DEADLINE")
	}
	if root != common.HexToHash(block.ReceiptsRoot) {
		return errors.New("RECEIPT_ROOT_MISMATCH")
	}
	return nil
}

func consensusReceipt(r *Receipt) (*types.Receipt, error) {
	if r.invalidTypeEncoding {
		return nil, errors.New("RECEIPT_TYPE_UNSUPPORTED")
	}
	if r.invalidOutcomeEncoding {
		return nil, errors.New("RECEIPT_OUTCOME_INVALID")
	}
	var kind uint64
	var err error
	// Older legacy RPCs omit type. No non-legacy receipt can match a root
	// under that interpretation without a hash collision.
	if r.Type != "" {
		kind, err = canonicalIndex(r.Type)
		if err != nil || kind > types.SetCodeTxType {
			return nil, errors.New("RECEIPT_TYPE_UNSUPPORTED")
		}
	}
	gas, err := canonicalIndex(r.CumulativeGasUsed)
	if err != nil {
		return nil, errors.New("RECEIPT_CUMULATIVE_GAS_INVALID")
	}
	if !receiptHex(r.LogsBloom, 256) {
		return nil, errors.New("RECEIPT_BLOOM_INVALID")
	}
	out := &types.Receipt{Type: uint8(kind), CumulativeGasUsed: gas, Bloom: types.BytesToBloom(common.FromHex(r.LogsBloom)), Logs: make([]*types.Log, 0, len(r.Logs))}
	if r.PostState != "" {
		if kind != types.LegacyTxType || r.Status != "" || !receiptHex(r.PostState, 32) {
			return nil, errors.New("RECEIPT_OUTCOME_INVALID")
		}
		out.PostState, _ = hex.DecodeString(r.PostState[2:])
	} else {
		status, err := canonicalIndex(r.Status)
		if err != nil || status > 1 {
			return nil, errors.New("RECEIPT_OUTCOME_INVALID")
		}
		out.Status = status
	}
	for _, log := range r.Logs {
		// All fixed widths and hex encodings were validated before conversion.
		decoded := &types.Log{Address: common.HexToAddress(log.Address), Data: common.FromHex(log.Data), Topics: make([]common.Hash, len(log.Topics))}
		for i, topic := range log.Topics {
			decoded.Topics[i] = common.HexToHash(topic)
		}
		out.Logs = append(out.Logs, decoded)
	}
	if types.CreateBloom(out) != out.Bloom {
		return nil, errors.New("RECEIPT_BLOOM_MISMATCH")
	}
	return out, nil
}
