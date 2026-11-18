package evm

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
)

// These are source facts only. Rust owns data-fee arithmetic. RPC header/body
// commitments are not independent consensus or a current fee-price oracle.
type OPRegolithFeeInputs struct {
	Version            string           `json:"version"`
	Authority          string           `json:"authority"`
	ChainKey           string           `json:"chain_key"`
	GenesisHash        string           `json:"genesis_hash"`
	NativeSemanticsID  string           `json:"native_semantics_id"`
	BlockNumber        uint64           `json:"block_number"`
	BlockHash          string           `json:"block_hash"`
	ExecutionTimestamp uint64           `json:"execution_timestamp"`
	TransactionsRoot   string           `json:"transactions_root"`
	HeaderSHA256       string           `json:"header_sha256"`
	DepositHash        string           `json:"deposit_hash"`
	DepositSourceHash  string           `json:"deposit_source_hash"`
	AttributesCalldata string           `json:"attributes_calldata"`
	AttributesSHA256   string           `json:"attributes_sha256"`
	UserTransactions   []OPUserFeeInput `json:"user_transactions"`
}

type OPUserFeeInput struct {
	Index              uint64 `json:"index"`
	Hash               string `json:"hash"`
	EncodedTransaction string `json:"encoded_transaction"`
	Sender             string `json:"sender"`
}

type opDepositWire struct {
	SourceHash common.Hash
	From       common.Address
	To         []byte
	Mint       *big.Int
	Value      *big.Int
	Gas        uint64
	System     bool
	Data       []byte
}

func decodeOPDeposit(raw []byte) (opDepositWire, error) {
	var deposit opDepositWire
	if len(raw) < 2 || raw[0] != 0x7e || rlp.DecodeBytes(raw[1:], &deposit) != nil || (len(deposit.To) != 0 && len(deposit.To) != 20) || deposit.Mint == nil || deposit.Value == nil || deposit.Mint.BitLen() > 256 || deposit.Value.BitLen() > 256 {
		return deposit, errors.New("OP_FEE_DEPOSIT_ENCODING")
	}
	return deposit, nil
}

func opAttributeUint64(word []byte) (uint64, error) {
	if len(word) != 32 || new(big.Int).SetBytes(word).BitLen() > 64 {
		return 0, errors.New("OP_FEE_ATTRIBUTES_ABI")
	}
	return binary.BigEndian.Uint64(word[24:]), nil
}

// L1-info source domain 1 binds the L1 block hash and L2 sequence number.
// It checks internal identity, not independent L1 inclusion or finality.
func opL1InfoSourceHash(l1Hash common.Hash, sequence uint64) common.Hash {
	var identity [64]byte
	copy(identity[:32], l1Hash[:])
	binary.BigEndian.PutUint64(identity[56:], sequence)
	inner := crypto.Keccak256Hash(identity[:])
	var domain [64]byte
	domain[31] = 1
	copy(domain[32:], inner[:])
	return crypto.Keccak256Hash(domain[:])
}

// Called only after the same header/body/receipts were reverified, by both the
// normal reader and archive replay. Missing inputs in the admitted historical
// interval fail closed; unsupported chains/forks keep this projection absent.
func projectOPRegolithFeeInputs(ctx context.Context, coverage VerifiedReceiptCoverage, header ChainHeaderCommitment, block ExecutionReceiptBlock, budget *receiptProofBudget) (*OPRegolithFeeInputs, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if coverage.ChainKey != "evm:10" && coverage.ChainKey != "evm:8453" {
		return nil, nil
	}
	timestamp, err := canonicalIndex(header.Block.Timestamp)
	if err != nil {
		return nil, err
	}
	activation := uint64(1686068903)
	chainID := uint64(10)
	if coverage.ChainKey == "evm:8453" {
		activation, chainID = 1686789347, 8453
	}
	if timestamp < activation || timestamp >= 1710374401 {
		return nil, nil
	}
	if !coverage.CanonicalBodyProven || !coverage.CanonicalReceiptsProven || coverage.NativeSemanticsID == "" || !sameReceiptAnchor(coverage.Block, header.Block) {
		return nil, errors.New("OP_FEE_BODY_AUTHORITY_REQUIRED")
	}
	raws := block.RawTransactions
	if len(raws) == 0 && len(block.TransactionsJSON) > 0 {
		for i, value := range block.TransactionsJSON {
			payload, err := encodeChainTransactionJSON(coverage.ChainKey, header.Block, uint64(i), value)
			if err != nil {
				return nil, err
			}
			if err = budget.charge(2*len(payload) + 128); err != nil {
				return nil, err
			}
			raws = append(raws, "0x"+hex.EncodeToString(payload))
		}
	}
	if len(raws) == 0 || len(raws) != len(header.Block.Transactions) {
		return nil, errors.New("OP_FEE_BODY_INCOMPLETE")
	}
	// Reserve retained projection space before decoding or constructing it.
	if err = budget.charge(2048 + len(block.Header)); err != nil {
		return nil, err
	}
	for _, raw := range raws {
		if err = budget.charge(2*len(raw) + 512); err != nil {
			return nil, err
		}
	}
	first, err := hex.DecodeString(raws[0][2:])
	if err != nil {
		return nil, err
	}
	deposit, err := decodeOPDeposit(first)
	if err != nil {
		return nil, err
	}
	if deposit.From != common.HexToAddress("0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001") || common.BytesToAddress(deposit.To) != common.HexToAddress("0x4200000000000000000000000000000000000015") || deposit.Mint.Sign() != 0 || deposit.Value.Sign() != 0 || deposit.System {
		return nil, errors.New("OP_FEE_ATTRIBUTES_DEPOSIT_IDENTITY")
	}
	data := deposit.Data
	if len(data) != 260 || hex.EncodeToString(data[:4]) != "015d8eb9" {
		return nil, errors.New("OP_FEE_ATTRIBUTES_ABI")
	}
	for _, offset := range []int{4, 36, 132} {
		value, err := opAttributeUint64(data[offset : offset+32])
		if err != nil || (offset == 36 && value > timestamp) {
			return nil, errors.New("OP_FEE_ATTRIBUTES_ABI")
		}
	}
	sequence, _ := opAttributeUint64(data[132:164])
	if deposit.SourceHash != opL1InfoSourceHash(common.BytesToHash(data[100:132]), sequence) {
		return nil, errors.New("OP_FEE_ATTRIBUTES_SOURCE_HASH")
	}
	height, _ := canonicalIndex(header.Block.Number)
	genesis, _ := knownBodyGenesis(coverage.ChainKey)
	// encoding/json compacts RawMessage when persisting an archive. Hash the
	// compact representation so that saving/reloading cannot break provenance.
	var compactHeader bytes.Buffer
	if err = json.Compact(&compactHeader, block.Header); err != nil {
		return nil, err
	}
	out := &OPRegolithFeeInputs{Version: "op-regolith-committed-fee-inputs/1", Authority: "RPC_HEADER_BODY_COMMITMENT_NOT_CONSENSUS", ChainKey: coverage.ChainKey, GenesisHash: genesis, NativeSemanticsID: coverage.NativeSemanticsID, BlockNumber: height, BlockHash: header.Block.Hash, ExecutionTimestamp: timestamp, TransactionsRoot: header.TransactionsRoot, HeaderSHA256: fmt.Sprintf("%x", sha256.Sum256(compactHeader.Bytes())), DepositHash: header.Block.Transactions[0], AttributesCalldata: "0x" + hex.EncodeToString(data), AttributesSHA256: fmt.Sprintf("%x", sha256.Sum256(data)), UserTransactions: []OPUserFeeInput{}}
	out.DepositSourceHash = deposit.SourceHash.Hex()
	for i, raw := range raws {
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		encoded, err := hex.DecodeString(raw[2:])
		if err != nil || len(encoded) == 0 || crypto.Keccak256Hash(encoded) != common.HexToHash(header.Block.Transactions[i]) {
			return nil, errors.New("OP_FEE_TRANSACTION_IDENTITY")
		}
		if encoded[0] == 0x7e {
			if _, err = decodeOPDeposit(encoded); err != nil {
				return nil, err
			}
			continue
		}
		var tx types.Transaction
		if err = tx.UnmarshalBinary(encoded); err != nil || tx.Type() > types.DynamicFeeTxType || (tx.Protected() && tx.ChainId().Cmp(new(big.Int).SetUint64(chainID)) != 0) {
			return nil, errors.New("OP_FEE_USER_TRANSACTION_ENCODING")
		}
		sender, err := types.Sender(types.NewLondonSigner(new(big.Int).SetUint64(chainID)), &tx)
		if err != nil {
			return nil, errors.New("OP_FEE_USER_TRANSACTION_SIGNATURE")
		}
		out.UserTransactions = append(out.UserTransactions, OPUserFeeInput{Index: uint64(i), Hash: header.Block.Transactions[i], EncodedTransaction: raw, Sender: sender.Hex()})
	}
	return out, ctx.Err()
}
