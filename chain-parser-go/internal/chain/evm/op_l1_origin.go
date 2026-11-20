package evm

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"xone/chain-parser-go/internal/provenance"
)

type OPL1Origin struct {
	BaseFeeWei string `json:"base_fee_wei"`
	Number     uint64 `json:"number"`
	Hash       string `json:"hash"`
	Timestamp  uint64 `json:"timestamp"`
	Sequence   uint64 `json:"sequence"`
}

func parseOPOrigin(first []byte, timestamp uint64) (opDepositWire, OPL1Origin, error) {
	deposit, err := decodeOPDeposit(first)
	if err != nil || deposit.From != common.HexToAddress("0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001") || common.BytesToAddress(deposit.To) != common.HexToAddress("0x4200000000000000000000000000000000000015") || deposit.Mint.Sign() != 0 || deposit.Value.Sign() != 0 || deposit.System {
		return opDepositWire{}, OPL1Origin{}, errors.New("OP_PACKED_ATTRIBUTES_DEPOSIT_IDENTITY")
	}
	format, _, _, _ := opPackedRules(timestamp)
	if timestamp < opEcotoneTime {
		format = "Bedrock"
	}
	data := deposit.Data
	expected := map[string]struct {
		selector string
		length   int
	}{"Bedrock": {"015d8eb9", 260}, "Ecotone": {"440a5e20", 164}, "Isthmus": {"098999be", 176}, "Jovian": {"3db6be2b", 178}}[format]
	if len(data) != expected.length || hex.EncodeToString(data[:4]) != expected.selector {
		return opDepositWire{}, OPL1Origin{}, errors.New("OP_PACKED_ATTRIBUTES_ACTIVATION_ABI")
	}
	var number, sequence, l1Time uint64
	l1Hash := common.BytesToHash(data[100:132])
	baseFee := data[36:68]
	if format == "Bedrock" {
		baseFee = data[68:100]
		if number, err = opAttributeUint64(data[4:36]); err != nil {
			return opDepositWire{}, OPL1Origin{}, err
		}
		l1Time, err = opAttributeUint64(data[36:68])
		if err != nil {
			return opDepositWire{}, OPL1Origin{}, err
		}
		sequence, err = opAttributeUint64(data[132:164])
		if err != nil {
			return opDepositWire{}, OPL1Origin{}, err
		}
	} else {
		number = binary.BigEndian.Uint64(data[28:36])
		sequence, l1Time = binary.BigEndian.Uint64(data[12:20]), binary.BigEndian.Uint64(data[20:28])
	}
	if l1Time > timestamp || deposit.SourceHash != opL1InfoSourceHash(l1Hash, sequence) {
		return opDepositWire{}, OPL1Origin{}, errors.New("OP_PACKED_ATTRIBUTES_SOURCE_IDENTITY")
	}
	return deposit, OPL1Origin{BaseFeeWei: new(big.Int).SetBytes(baseFee).String(), Number: number, Hash: l1Hash.Hex(), Timestamp: l1Time, Sequence: sequence}, nil
}

// OPOriginEvidence carries the OP execution block and L1 headers in descending
// height order, including both the signed execution head and the origin.
type OPOriginEvidence struct {
	Block   json.RawMessage   `json:"block"`
	Headers []json.RawMessage `json:"headers"`
}

type OPOriginReport struct {
	OPBlockHash       string     `json:"op_block_hash"`
	OPBlockNumber     uint64     `json:"op_block_number"`
	Origin            OPL1Origin `json:"l1_origin"`
	FinalizedL1Number uint64     `json:"finalized_l1_number"`
	FinalizedL1Hash   string     `json:"finalized_l1_hash"`
	Authority         string     `json:"authority"`
}

// VerifyOPOrigin binds the first committed deposit's L1 reference to the signed
// Ethereum execution chain. L2 batch availability and derivation are separate.
func VerifyOPOrigin(ctx context.Context, proof *provenance.VerifiedExecutionFinality, evidence OPOriginEvidence) (OPOriginReport, error) {
	var empty OPOriginReport
	if proof == nil {
		return empty, errors.New("OP_ORIGIN_BEACON_REQUIRED")
	}
	head, hash := proof.ExecutionBlock()
	if head == 0 || len(evidence.Headers) == 0 || len(evidence.Headers) > 129 {
		return empty, errors.New("OP_ORIGIN_ANCESTRY_WINDOW")
	}
	const budget = 32 << 20
	remaining := budget - len(evidence.Block)
	for _, header := range evidence.Headers {
		if len(header) > remaining {
			return empty, errors.New("OP_ORIGIN_BYTE_LIMIT")
		}
		remaining -= len(header)
	}
	body, err := VerifyChainBlockJSON(ctx, "evm:10", evidence.Block, budget)
	if err != nil {
		return empty, err
	}
	timestamp, err := canonicalIndex(body.Block.Timestamp)
	if err != nil {
		return empty, err
	}
	if timestamp < 1686068903 || len(body.RawTransactions) == 0 {
		return empty, errors.New("OP_ORIGIN_ATTRIBUTES_REQUIRED")
	}
	_, origin, err := parseOPOrigin(common.FromHex(body.RawTransactions[0]), timestamp)
	if err != nil {
		return empty, err
	}
	if origin.Number > head || head-origin.Number != uint64(len(evidence.Headers)-1) {
		return empty, errors.New("OP_ORIGIN_ANCESTRY_WINDOW")
	}
	if err = verifyOriginAncestry(ctx, head, hash, origin, func(height uint64) (json.RawMessage, error) { return evidence.Headers[head-height], nil }); err != nil {
		return empty, err
	}
	number, _ := canonicalIndex(body.Block.Number)
	return OPOriginReport{OPBlockHash: body.Block.Hash, OPBlockNumber: number, Origin: origin, FinalizedL1Number: head, FinalizedL1Hash: hash, Authority: "signed-ethereum-mainnet-l1-origin"}, ctx.Err()
}

func verifyOriginAncestry(ctx context.Context, head uint64, hash string, origin OPL1Origin, read func(uint64) (json.RawMessage, error)) error {
	return verifyOriginRange(ctx, head, hash, []OPL1Origin{origin}, read)
}

// origins lists distinct referenced epochs in ascending order. Walk each L1
// header once, checking every referenced hash and timestamp along the way.
func verifyOriginRange(ctx context.Context, head uint64, hash string, origins []OPL1Origin, read func(uint64) (json.RawMessage, error)) error {
	if len(origins) == 0 {
		return errors.New("OP_ORIGIN_ANCESTRY_WINDOW")
	}
	first := origins[0].Number
	last := len(origins) - 1
	if first > head || head-first > 128 || origins[last].Number > head {
		return errors.New("OP_ORIGIN_ANCESTRY_WINDOW")
	}
	expected := hash
	for h := head; ; h-- {
		raw, err := read(h)
		if err != nil {
			return err
		}
		header, err := VerifyChainHeaderJSON(ctx, "evm:1", raw)
		if err != nil {
			return err
		}
		height, err := canonicalIndex(header.Block.Number)
		if err != nil || height != h || header.Block.Hash != expected {
			return errors.New("OP_ORIGIN_ANCESTRY_LINK")
		}
		if height == origins[last].Number {
			origin := origins[last]
			time, err := canonicalIndex(header.Block.Timestamp)
			if err != nil || time != origin.Timestamp || expected != origin.Hash {
				return errors.New("OP_ORIGIN_L1_MISMATCH")
			}
			if header.ethBaseFeeWei == "" || header.ethBaseFeeWei != origin.BaseFeeWei {
				return errors.New("OP_ORIGIN_L1_BASE_FEE_MISMATCH")
			}
			if last == 0 {
				return ctx.Err()
			}
			last--
		}
		if height == first {
			return errors.New("OP_ORIGIN_ANCESTRY_LINK")
		}
		expected = header.Block.ParentHash
	}
}
