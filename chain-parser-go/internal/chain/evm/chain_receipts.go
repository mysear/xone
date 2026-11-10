package evm

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
)

// These profiles describe receipt encodings, not chain consensus or activation
// proofs. Source commits and public RPC observations are retained under
// integration/test/source/chain-receipt-vectors. Custom EVM chains retain the
// strict Ethereum profile; native types require an explicit chain selection.
type chainReceiptProfile uint8

const (
	receiptEthereum chainReceiptProfile = iota
	receiptOP
	receiptNitro
	receiptBSC
	receiptBor
	receiptCoreth
)

func receiptProfile(chainKey string) (chainReceiptProfile, error) {
	switch chainKey {
	case "evm:10", "evm:8453":
		return receiptOP, nil
	case "evm:42161":
		return receiptNitro, nil
	case "evm:56":
		return receiptBSC, nil
	case "evm:137":
		return receiptBor, nil
	case "evm:43114":
		return receiptCoreth, nil
	default:
		// Empty keys occur in historical in-process fixtures. They grant no native
		// encoding permissions, just like arbitrary configured EVM chain IDs.
		if chainKey == "" || strings.HasPrefix(chainKey, "evm:") {
			return receiptEthereum, nil
		}
		return 0, errors.New("RECEIPT_CHAIN_UNSUPPORTED")
	}
}

// VerifyChainReceiptRoot validates the complete RPC transaction/receipt/log
// identities, then hashes an explicitly selected chain encoding. A matching
// root authenticates receipts only relative to the supplied RPC header.
func VerifyChainReceiptRoot(ctx context.Context, chainKey string, block RawBlock, receipts []Receipt) error {
	if ctx.Err() != nil {
		return errors.New("RPC_DEADLINE")
	}
	profile, err := receiptProfile(chainKey)
	if err != nil {
		return err
	}
	if profile == receiptEthereum {
		return VerifyReceiptRoot(ctx, block, receipts)
	}
	encoded, err := canonicalReceiptEncodings(ctx, profile, block, receipts)
	if err != nil {
		return err
	}
	root := types.DeriveSha(encoded, trie.NewStackTrie(nil))
	if ctx.Err() != nil {
		return errors.New("RPC_DEADLINE")
	}
	if root != common.HexToHash(block.ReceiptsRoot) {
		return errors.New("RECEIPT_ROOT_MISMATCH")
	}
	return nil
}

func canonicalReceiptEncodings(ctx context.Context, profile chainReceiptProfile, block RawBlock, receipts []Receipt) (encodedChainReceipts, error) {
	if err := ValidateReceipts(block, receipts); err != nil {
		return nil, err
	}
	if !receiptHex(block.ReceiptsRoot, 32) {
		return nil, errors.New("RECEIPT_ROOT_MISSING_OR_INVALID")
	}
	encoded := make(encodedChainReceipts, len(receipts))
	ordered := make([]*Receipt, len(receipts))
	for i := range receipts {
		index, _ := canonicalIndex(receipts[i].TxIndex)
		ordered[index] = &receipts[i]
	}
	var previous uint64
	for i, r := range ordered {
		if ctx.Err() != nil {
			return nil, errors.New("RPC_DEADLINE")
		}
		kind, err := receiptKind(r)
		if err != nil {
			return nil, err
		}
		if profile == receiptBor {
			if err = validateBorReceipt(block, r, kind); err != nil {
				return nil, err
			}
		}
		native := false
		switch profile {
		case receiptOP:
			native = kind == 0x7e
		case receiptNitro:
			switch kind {
			case 0x64, 0x65, 0x66, 0x68, 0x69, 0x6a, 0x78:
				native = true
			}
		case receiptBor:
			native = kind == 0x7f
		}
		// AvalancheGo v1.15.3 pins libevm d398bc4dbe41, whose receipt
		// EncodeIndex still enumerates 0..3; this is not an old-Coreth guess.
		if (kind > types.SetCodeTxType && !native) || (profile == receiptCoreth && kind > types.BlobTxType) {
			return nil, errors.New("RECEIPT_TYPE_UNSUPPORTED")
		}
		copy := *r
		// Only the enumerated native envelopes share these four base fields. No
		// unrecognized type is ever reinterpreted as a legacy receipt.
		if native {
			copy.Type = "0x0"
		}
		decoded, err := consensusReceipt(&copy)
		if err != nil {
			return nil, err
		}
		if native && kind != 0x78 && r.PostState != "" {
			return nil, errors.New("RECEIPT_OUTCOME_INVALID")
		}
		if r.invalidChainEncoding {
			return nil, errors.New("RECEIPT_CHAIN_FIELDS_INVALID")
		}
		nonce, version, err := depositReceiptFields(profile, kind, r)
		if err != nil {
			return nil, err
		}
		gas := decoded.CumulativeGasUsed
		if gas < previous {
			return nil, errors.New("RECEIPT_CUMULATIVE_GAS_ORDER")
		}
		zeroAllowed := profile == receiptBSC || (profile == receiptNitro && native) || (profile == receiptOP && native) || (profile == receiptBor && kind == 0x7f)
		if gas == previous && (!zeroAllowed || r.GasUsed != "0x0") {
			return nil, errors.New("RECEIPT_CUMULATIVE_GAS_ORDER")
		}
		if r.GasUsed != "" {
			used, err := canonicalIndex(r.GasUsed)
			if err != nil || used != gas-previous {
				return nil, errors.New("RECEIPT_GAS_USED_INVALID")
			}
		}
		previous = gas
		outcome := decoded.PostState
		if len(outcome) == 0 {
			outcome = []byte{}
			if decoded.Status == 1 {
				outcome = []byte{1}
			}
		}
		fields := []any{outcome, gas, decoded.Bloom, decoded.Logs}
		// op-geth Receipts.EncodeIndex: Regolith's nonce is intentionally absent
		// from the trie until Canyon's version is present. Zero is still present.
		if version != nil {
			fields = append(fields, *nonce, *version)
		}
		payload, err := rlp.EncodeToBytes(fields)
		if err != nil {
			return nil, errors.New("RECEIPT_ENCODING_INVALID")
		}
		if kind != 0 && !(profile == receiptNitro && kind == 0x78) {
			payload = append([]byte{byte(kind)}, payload...)
		}
		encoded[i] = payload
	}
	return encoded, nil
}

type encodedChainReceipts [][]byte

func (r encodedChainReceipts) Len() int                           { return len(r) }
func (r encodedChainReceipts) EncodeIndex(i int, w *bytes.Buffer) { w.Write(r[i]) }

func receiptKind(r *Receipt) (uint64, error) {
	if r.invalidTypeEncoding {
		return 0, errors.New("RECEIPT_TYPE_UNSUPPORTED")
	}
	if r.Type == "" {
		return 0, nil
	}
	kind, err := canonicalIndex(r.Type)
	if err != nil || kind > 0x7f {
		return 0, errors.New("RECEIPT_TYPE_UNSUPPORTED")
	}
	return kind, nil
}
func depositReceiptFields(profile chainReceiptProfile, kind uint64, r *Receipt) (*uint64, *uint64, error) {
	if profile != receiptOP || kind != 0x7e {
		if r.DepositNonce != "" || r.DepositReceiptVersion != "" {
			return nil, nil, errors.New("RECEIPT_DEPOSIT_FIELDS_UNEXPECTED")
		}
		return nil, nil, nil
	}
	var nonce, version *uint64
	if r.DepositNonce != "" {
		n, err := canonicalIndex(r.DepositNonce)
		if err != nil {
			return nil, nil, errors.New("RECEIPT_DEPOSIT_NONCE_INVALID")
		}
		nonce = &n
	}
	if r.DepositReceiptVersion != "" {
		v, err := canonicalIndex(r.DepositReceiptVersion)
		if err != nil || v != 1 || nonce == nil {
			return nil, nil, errors.New("RECEIPT_DEPOSIT_VERSION_INVALID")
		}
		version = &v
	}
	return nonce, version, nil
}

// Before Madhugiri, Bor's derived state-sync RPC transaction/receipt is outside
// the block's transactionsRoot and receiptsRoot. Its logs cannot be proved by
// receiptsRoot. Fail closed instead of silently dropping a suffix or certifying
// these uncommitted logs. Post-Madhugiri 0x7f receipts ARE ordinary trie members.
func validateBorReceipt(block RawBlock, r *Receipt, kind uint64) error {
	const madhugiriMainnet = uint64(80084800) // pinned Bor params/config.go
	number, err := canonicalIndex(block.Number)
	if err != nil {
		return errors.New("RECEIPT_BOR_BLOCK_NUMBER_INVALID")
	}
	if number < madhugiriMainnet && kind == 0x7f {
		return errors.New("RECEIPT_TYPE_UNSUPPORTED")
	}
	if receiptHex(block.Hash, 32) {
		var height [8]byte
		binary.BigEndian.PutUint64(height[:], number)
		key := append([]byte("matic-bor-receipt-"), height[:]...)
		key = append(key, common.FromHex(block.Hash)...)
		if strings.EqualFold(r.TxHash, crypto.Keccak256Hash(key).Hex()) {
			return errors.New("RECEIPT_BOR_SYNTHETIC_UNCOMMITTED")
		}
	}
	return nil
}
