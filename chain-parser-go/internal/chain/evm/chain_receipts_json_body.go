package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"math/big"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
)

const ChainNativeCommitmentsV2 = "known-evm-native-commitments-v2"

type ChainBlockBodyProof struct {
	ObservedHead                    *RawBlock                  `json:"observed_head,omitempty"`
	CanonicalBlock                  RawBlock                   `json:"canonical_block"`
	UncommittedSystemTransactionIDs []string                   `json:"uncommitted_system_transaction_ids"`
	BodyCoverageScope               string                     `json:"body_coverage_scope"`
	NativeSemantics                 *NativeSemanticsDescriptor `json:"native_semantics,omitempty"`
	ProofBytesUsed                  int                        `json:"proof_bytes_used"`
	Block                           RawBlock                   `json:"block"`
	Header                          json.RawMessage            `json:"header"`
	HeaderCommitment                ChainHeaderCommitment      `json:"header_commitment"`
	RawTransactions                 []string                   `json:"raw_transactions"`
	TransactionEncodingAuthority    string                     `json:"transaction_encoding_authority"`
}

// VerifyChainBlockJSON reconstructs signed standard transaction encodings and
// only explicitly enumerated native layouts from a complete public JSON block.
// Unknown native layouts require raw bytes; they never become legacy. Every
// reconstructed encoding must match its txID and the native header's body root.
func VerifyChainBlockJSON(ctx context.Context, chainKey string, full json.RawMessage, maxBytes int) (ChainBlockBodyProof, error) {
	var out ChainBlockBodyProof
	if err := ctx.Err(); err != nil {
		return out, err
	}
	if maxBytes <= 0 || maxBytes > 64<<20 || len(full) > maxBytes/2 {
		return out, errors.New("BLOCK_INTEGRITY_BUDGET")
	}
	if err := rejectDuplicateReceiptKeys(full); err != nil {
		return out, err
	}
	var fields map[string]json.RawMessage
	var body struct{ Transactions []json.RawMessage }
	if json.Unmarshal(full, &fields) != nil || json.Unmarshal(full, &body) != nil || body.Transactions == nil {
		return out, errors.New("BLOCK_TRANSACTIONS_MISSING")
	}
	budget := receiptProofBudget{remaining: maxBytes}
	if err := budget.charge(2*len(full) + 128*len(body.Transactions)); err != nil {
		return out, err
	}
	hashes := make([]string, len(body.Transactions))
	for i, raw := range body.Transactions {
		var p struct{ Hash string }
		if json.Unmarshal(raw, &p) != nil || !receiptHex(p.Hash, 32) {
			return out, errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
		}
		hashes[i] = canonicalReceiptIdentity(p.Hash)
	}
	fields["transactions"], _ = json.Marshal(hashes)
	headerJSON, err := json.Marshal(fields)
	if err != nil {
		return out, err
	}
	header, err := VerifyChainHeaderJSON(ctx, chainKey, headerJSON)
	if err != nil {
		return out, err
	}
	if err = validateBlockTransactions(header.Block); err != nil {
		return out, err
	}
	raws := make([]string, 0, len(body.Transactions))
	for i, raw := range body.Transactions {
		if err = ctx.Err(); err != nil {
			return out, err
		}
		payload, err := encodeChainTransactionJSON(chainKey, header.Block, uint64(i), raw)
		if err != nil {
			return out, err
		}
		if err = budget.charge(3*len(payload) + 128); err != nil {
			return out, err
		}
		raws = append(raws, "0x"+hex.EncodeToString(payload))
	}
	if err = verifyRawBodyCommitment(ctx, chainKey, header, raws, &budget); err != nil {
		return out, err
	}
	return ChainBlockBodyProof{CanonicalBlock: header.Block, BodyCoverageScope: "canonical-header-transactions", Block: header.Block, Header: headerJSON, HeaderCommitment: header, RawTransactions: raws, TransactionEncodingAuthority: "standard-signed-json+enumerated-native-rlp"}, nil
}

func encodeChainTransactionJSON(chainKey string, block RawBlock, index uint64, raw json.RawMessage) ([]byte, error) {
	if err := rejectDuplicateReceiptKeys(raw); err != nil {
		return nil, err
	}
	var fields map[string]json.RawMessage
	var position struct {
		Hash             string
		BlockHash        string
		BlockNumber      string
		TransactionIndex string
		Type             string
	}
	if json.Unmarshal(raw, &fields) != nil || json.Unmarshal(raw, &position) != nil {
		return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
	}
	at, err := canonicalIndex(position.TransactionIndex)
	if err != nil || at != index || position.BlockNumber != block.Number || canonicalReceiptIdentity(position.BlockHash) != block.Hash || !receiptHex(position.Hash, 32) {
		return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_POSITION")
	}
	var kind uint64
	if position.Type != "" {
		kind, err = canonicalIndex(position.Type)
		if err != nil {
			return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
	}
	expectedChain, err := strconv.ParseUint(strings.TrimPrefix(chainKey, "evm:"), 10, 64)
	if err != nil {
		return nil, err
	}
	if chainKey == "evm:43114" && kind > types.BlobTxType {
		return nil, errors.New("NATIVE_TRANSACTION_TYPE_UNSUPPORTED")
	}
	var payload []byte
	switch {
	case kind <= types.SetCodeTxType:
		tx := new(types.Transaction)
		if json.Unmarshal(raw, tx) != nil {
			return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
		if tx.Protected() && tx.ChainId().Cmp(new(big.Int).SetUint64(expectedChain)) != 0 {
			return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_CHAIN")
		}
		payload, err = tx.MarshalBinary()
	case chainKey == "evm:42161" && kind == 0x6a:
		chain, e := headerQuantity(fields["chainId"], true)
		if e != nil || !chain.IsUint64() || chain.Uint64() != expectedChain {
			return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_CHAIN")
		}
		var input string
		if json.Unmarshal(fields["input"], &input) != nil || !receiptHex(input, -1) {
			return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
		data := common.FromHex(input)
		var encoded []byte
		encoded, err = rlp.EncodeToBytes([]any{chain, data})
		payload = append([]byte{0x6a}, encoded...)
	case (chainKey == "evm:10" || chainKey == "evm:8453") && kind == 0x7e:
		payload, err = encodeOPDepositJSON(fields)
	case chainKey == "evm:42161" && kind == 0x78:
		return nil, errors.New("ARBITRUM_CLASSIC_TX_ID_AUTHORITY_REQUIRED")
	default:
		return nil, errors.New("NATIVE_TRANSACTION_RAW_REQUIRED")
	}
	if err != nil {
		return nil, err
	}
	if crypto.Keccak256Hash(payload) != common.HexToHash(position.Hash) {
		return nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
	}
	return payload, nil
}

func encodeOPDepositJSON(fields map[string]json.RawMessage) ([]byte, error) {
	var source, from, to, input string
	if json.Unmarshal(fields["sourceHash"], &source) != nil || !receiptHex(source, 32) || json.Unmarshal(fields["from"], &from) != nil || !receiptHex(from, 20) || json.Unmarshal(fields["input"], &input) != nil || !receiptHex(input, -1) {
		return nil, errors.New("OP_DEPOSIT_TRANSACTION_INVALID")
	}
	toBytes := []byte{}
	if value := fields["to"]; len(value) == 0 {
		return nil, errors.New("OP_DEPOSIT_TRANSACTION_INVALID")
	} else if string(value) != "null" {
		if json.Unmarshal(value, &to) != nil || !receiptHex(to, 20) {
			return nil, errors.New("OP_DEPOSIT_TRANSACTION_INVALID")
		}
		toBytes = common.FromHex(to)
	}
	mint, err := headerQuantity(fields["mint"], false)
	if err != nil {
		return nil, err
	}
	if mint == nil {
		mint = new(big.Int)
	}
	value, err := headerQuantity(fields["value"], true)
	if err != nil {
		return nil, err
	}
	gas, err := headerQuantity(fields["gas"], true)
	if err != nil || !gas.IsUint64() {
		return nil, errors.New("OP_DEPOSIT_TRANSACTION_INVALID")
	}
	system := false
	if raw := fields["isSystemTx"]; len(raw) > 0 {
		if string(raw) == "null" || json.Unmarshal(raw, &system) != nil {
			return nil, errors.New("OP_DEPOSIT_TRANSACTION_INVALID")
		}
	}
	encoded, err := rlp.EncodeToBytes([]any{common.HexToHash(source), common.HexToAddress(from), toBytes, mint, value, gas.Uint64(), system, common.FromHex(input)})
	if err != nil {
		return nil, err
	}
	return append([]byte{0x7e}, encoded...), nil
}
