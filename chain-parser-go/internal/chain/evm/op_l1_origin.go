package evm

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"

	"github.com/ethereum/go-ethereum/common"
	"xone/chain-parser-go/internal/provenance"
)

type OPL1Origin struct {
	Number    uint64 `json:"number"`
	Hash      string `json:"hash"`
	Timestamp uint64 `json:"timestamp"`
	Sequence  uint64 `json:"sequence"`
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
	if format == "Bedrock" {
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
	return deposit, OPL1Origin{Number: number, Hash: l1Hash.Hex(), Timestamp: l1Time, Sequence: sequence}, nil
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
	expected := hash
	for i, raw := range evidence.Headers {
		header, err := VerifyChainHeaderJSON(ctx, "evm:1", raw)
		if err != nil {
			return empty, err
		}
		height, err := canonicalIndex(header.Block.Number)
		if err != nil || height != head-uint64(i) || header.Block.Hash != expected {
			return empty, errors.New("OP_ORIGIN_ANCESTRY_LINK")
		}
		if height == origin.Number {
			time, err := canonicalIndex(header.Block.Timestamp)
			if err != nil || time != origin.Timestamp || expected != origin.Hash {
				return empty, errors.New("OP_ORIGIN_L1_MISMATCH")
			}
		}
		expected = header.Block.ParentHash
	}
	number, _ := canonicalIndex(body.Block.Number)
	return OPOriginReport{OPBlockHash: body.Block.Hash, OPBlockNumber: number, Origin: origin, FinalizedL1Number: head, FinalizedL1Hash: hash, Authority: "signed-ethereum-mainnet-l1-origin"}, ctx.Err()
}
