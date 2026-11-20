package evm

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
)

// OPPackedFeeInputs is an optional post-Ecotone source projection. The parser
// records the admitted rule and complete signed bytes, never fee amounts.
// Authority is only the same RPC header/body/receipts commitment as coverage.
type OPPackedFeeInputs struct {
	Version            string              `json:"version"`
	Authority          string              `json:"authority"`
	ChainKey           string              `json:"chain_key"`
	GenesisHash        string              `json:"genesis_hash"`
	NativeSemanticsID  string              `json:"native_semantics_id"`
	ActivationID       string              `json:"activation_id"`
	BlockHash          string              `json:"block_hash"`
	BlockNumber        uint64              `json:"block_number"`
	ExecutionTimestamp uint64              `json:"execution_timestamp"`
	TransactionsRoot   string              `json:"transactions_root"`
	ReceiptsRoot       string              `json:"receipts_root"`
	HeaderSHA256       string              `json:"header_sha256"`
	DepositHash        string              `json:"deposit_hash"`
	DepositSourceHash  string              `json:"deposit_source_hash"`
	AttributesCalldata string              `json:"attributes_calldata"`
	AttributesSHA256   string              `json:"attributes_sha256"`
	AttributesFormat   string              `json:"attributes_format"`
	DataFeeRule        string              `json:"data_fee_rule"`
	OperatorFeeRule    string              `json:"operator_fee_rule"`
	UpgradeBlock       bool                `json:"upgrade_block"`
	UserTransactions   []OPPackedUserInput `json:"user_transactions"`
}

type OPPackedUserInput struct {
	OPUserFeeInput
	GasUsed uint64 `json:"gas_used"`
}

const (
	opEcotoneTime = uint64(1710374401)
	opFjordTime   = uint64(1720627201)
	opIsthmusTime = uint64(1746806401)
	opJovianTime  = uint64(1764691201)
)

// Both admitted chain schedules are pinned by ResolveNativeSemantics. An
// activation block still carries the previous L1-info encoding. The four
// upgrade blocks contain only deposit/upgrade transactions: no user fee is
// inferred from absent operator parameters or uninitialized upgrade state.
func opPackedRules(timestamp uint64) (format, dataRule, operatorRule string, upgrade bool) {
	format, dataRule, operatorRule = "Ecotone", "Ecotone", "NotActive"
	if timestamp >= opFjordTime {
		dataRule = "Fjord"
	}
	if timestamp > opIsthmusTime {
		format, operatorRule = "Isthmus", "Isthmus"
	}
	if timestamp >= opJovianTime {
		operatorRule = "Jovian"
	}
	if timestamp > opJovianTime {
		format = "Jovian"
	}
	if timestamp == opEcotoneTime {
		format, dataRule = "Bedrock", "RegolithActivationDepositsOnly"
	}
	if timestamp == opIsthmusTime {
		operatorRule = "IsthmusActivationDepositsOnly"
	}
	upgrade = timestamp == opEcotoneTime || timestamp == opFjordTime || timestamp == opIsthmusTime || timestamp == opJovianTime
	return
}

func projectOPPackedFeeInputs(ctx context.Context, coverage VerifiedReceiptCoverage, header ChainHeaderCommitment, block ExecutionReceiptBlock, budget *receiptProofBudget) (*OPPackedFeeInputs, error) {
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
	if timestamp < opEcotoneTime {
		return nil, nil
	}
	if !coverage.CanonicalBodyProven || !coverage.CanonicalReceiptsProven || coverage.NativeSemantics == nil || coverage.NativeSemanticsID == "" || !sameReceiptAnchor(coverage.Block, header.Block) {
		return nil, errors.New("OP_PACKED_BODY_AUTHORITY_REQUIRED")
	}
	// Reject a stale/foreign descriptor rather than using serialized trust.
	semantics, err := ResolveNativeSemantics(coverage.ChainKey, header)
	if err != nil || !semantics.Admitted || semantics.ID != coverage.NativeSemanticsID || semantics.ActivationID != coverage.ActivationID {
		return nil, errors.New("OP_PACKED_NATIVE_SEMANTICS_MISMATCH")
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
	if len(raws) == 0 || len(raws) != len(header.Block.Transactions) || len(block.Receipts) != len(raws) {
		return nil, errors.New("OP_PACKED_BODY_INCOMPLETE")
	}
	if err = budget.charge(3072 + len(block.Header)); err != nil {
		return nil, err
	}
	for _, raw := range raws {
		if err = budget.charge(2*len(raw) + 640); err != nil {
			return nil, err
		}
		if !receiptHex(raw, -1) || len(raw) <= 2 {
			return nil, errors.New("OP_PACKED_TRANSACTION_ENCODING")
		}
	}
	deposit, _, err := parseOPOrigin(common.FromHex(raws[0]), timestamp)
	if err != nil {
		return nil, err
	}
	format, dataRule, operatorRule, upgrade := opPackedRules(timestamp)
	data := deposit.Data
	var compact bytes.Buffer
	if err = json.Compact(&compact, block.Header); err != nil {
		return nil, err
	}
	height, _ := canonicalIndex(header.Block.Number)
	genesis, _ := knownBodyGenesis(coverage.ChainKey)
	out := &OPPackedFeeInputs{Version: "op-packed-committed-fee-inputs/1", Authority: "RPC_HEADER_BODY_RECEIPTS_COMMITMENT_NOT_CONSENSUS", ChainKey: coverage.ChainKey, GenesisHash: genesis, NativeSemanticsID: semantics.ID, ActivationID: semantics.ActivationID, BlockHash: header.Block.Hash, BlockNumber: height, ExecutionTimestamp: timestamp, TransactionsRoot: header.TransactionsRoot, ReceiptsRoot: header.ReceiptsRoot, HeaderSHA256: fmt.Sprintf("%x", sha256.Sum256(compact.Bytes())), DepositHash: header.Block.Transactions[0], DepositSourceHash: deposit.SourceHash.Hex(), AttributesCalldata: "0x" + hex.EncodeToString(data), AttributesSHA256: fmt.Sprintf("%x", sha256.Sum256(data)), AttributesFormat: format, DataFeeRule: dataRule, OperatorFeeRule: operatorRule, UpgradeBlock: upgrade, UserTransactions: []OPPackedUserInput{}}
	chainID := uint64(10)
	if coverage.ChainKey == "evm:8453" {
		chainID = 8453
	}
	previous := uint64(0)
	for i, raw := range raws {
		if err = ctx.Err(); err != nil {
			return nil, err
		}
		cumulative, err := canonicalIndex(block.Receipts[i].CumulativeGasUsed)
		if err != nil || cumulative < previous {
			return nil, errors.New("OP_PACKED_COMMITTED_GAS")
		}
		gas := cumulative - previous
		previous = cumulative
		encoded := common.FromHex(raw)
		if crypto.Keccak256Hash(encoded) != common.HexToHash(header.Block.Transactions[i]) {
			return nil, errors.New("OP_PACKED_TRANSACTION_IDENTITY")
		}
		if encoded[0] == 0x7e {
			if _, err := decodeOPDeposit(encoded); err != nil {
				return nil, err
			}
			continue
		}
		if upgrade {
			return nil, errors.New("OP_UPGRADE_BLOCK_USER_TRANSACTION_NOT_ADMITTED")
		}
		var tx types.Transaction
		if err = tx.UnmarshalBinary(encoded); err != nil || (tx.Type() > types.DynamicFeeTxType && (tx.Type() != types.SetCodeTxType || timestamp <= opIsthmusTime)) || (tx.Protected() && tx.ChainId().Cmp(new(big.Int).SetUint64(chainID)) != 0) {
			return nil, errors.New("OP_PACKED_USER_TRANSACTION_ENCODING")
		}
		sender, err := types.Sender(types.NewPragueSigner(new(big.Int).SetUint64(chainID)), &tx)
		if err != nil {
			return nil, errors.New("OP_PACKED_USER_TRANSACTION_SIGNATURE")
		}
		out.UserTransactions = append(out.UserTransactions, OPPackedUserInput{OPUserFeeInput: OPUserFeeInput{Index: uint64(i), Hash: header.Block.Transactions[i], EncodedTransaction: raw, Sender: sender.Hex()}, GasUsed: gas})
	}
	return out, nil
}
