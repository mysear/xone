package evm

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"math/big"
	"strings"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
)

// ChainHeaderCommitment records what the header actually commits. Under SAE,
// StateRoot and ReceiptsRoot describe the settled window, not this block's
// execution. VerifySAEReceiptCoverage resolves their heights explicitly.
type NitroHeaderInfo struct {
	SendRoot      string `json:"send_root"`
	SendCount     uint64 `json:"send_count"`
	L1BlockNumber uint64 `json:"l1_block_number"`
	ArbOSVersion  uint64 `json:"arbos_version"`
	CollectTips   bool   `json:"collect_tips"`
}

type ChainHeaderCommitment struct {
	Nitro            *NitroHeaderInfo `json:"nitro,omitempty"`
	Block            RawBlock         `json:"block"`
	TransactionsRoot string           `json:"transactions_root"`
	StateRoot        string           `json:"state_root"`
	ReceiptsRoot     string           `json:"receipts_root"`
	LogsBloom        string           `json:"logs_bloom"`
	SettledHeight    *uint64          `json:"settled_height,omitempty"`
	HeaderEncoding   string           `json:"header_encoding"`
}

var avalancheGraniteTime = uint64(time.Date(2025, 11, 19, 16, 0, 0, 0, time.UTC).Unix())
var avalancheHeliconTime = uint64(time.Date(2026, 9, 22, 15, 0, 0, 0, time.UTC).Unix())

// VerifyChainHeaderJSON follows pinned native upstream Header serializers.
// Nitro uses ordinary Ethereum Header RLP: its native metadata is carried in
// Extra/MixDigest/Nonce, all of which are hashed. Coreth inserts ExtDataHash at
// slot 15 and has its own optional tail, including Granite and SAE fields.
func VerifyChainHeaderJSON(ctx context.Context, chainKey string, raw json.RawMessage) (ChainHeaderCommitment, error) {
	var out ChainHeaderCommitment
	if err := ctx.Err(); err != nil {
		return out, err
	}
	switch chainKey {
	case "evm:1", "evm:10", "evm:8453", "evm:56", "evm:137", "evm:42161", "evm:43114":
	default:
		return out, errors.New("BLOCK_BODY_CHAIN_UNSUPPORTED")
	}
	if err := rejectDuplicateReceiptKeys(raw); err != nil {
		return out, err
	}
	var fields map[string]json.RawMessage
	var header types.Header
	if json.Unmarshal(raw, &fields) != nil || json.Unmarshal(raw, &header) != nil || json.Unmarshal(raw, &out.Block) != nil || header.Number == nil || !header.Number.IsUint64() {
		return out, errors.New("BLOCK_INTEGRITY_HEADER")
	}
	for _, name := range []string{"parentHash", "sha3Uncles", "miner", "stateRoot", "transactionsRoot", "receiptsRoot", "logsBloom", "difficulty", "number", "gasLimit", "gasUsed", "timestamp", "extraData", "mixHash", "nonce", "hash"} {
		if len(fields[name]) == 0 || string(fields[name]) == "null" {
			return out, errors.New("BLOCK_INTEGRITY_HEADER_FIELD_MISSING")
		}
	}
	for _, name := range []string{"difficulty", "number", "gasLimit", "gasUsed", "timestamp", "baseFeePerGas", "blobGasUsed", "excessBlobGas"} {
		if _, err := headerQuantity(fields[name], false); err != nil {
			return out, err
		}
	}
	number, err := canonicalIndex(out.Block.Number)
	timestamp, timeErr := canonicalIndex(out.Block.Timestamp)
	if err != nil || timeErr != nil || number != header.Number.Uint64() || timestamp != header.Time {
		return out, errors.New("BLOCK_INTEGRITY_HEADER_POSITION")
	}
	var encoded []byte
	if chainKey == "evm:43114" {
		encoded, err = encodeCorethHeader(&header, fields)
		out.HeaderEncoding = "coreth-header-extdata-granite-sae-rlp/v1"
		if err != nil {
			return out, err
		}
		if header.Time >= avalancheHeliconTime {
			value, err := headerQuantity(fields["settledHeight"], true)
			if err != nil || value == nil || !value.IsUint64() {
				return out, errors.New("SAE_SETTLED_HEIGHT_INVALID")
			}
			height := value.Uint64()
			if height >= number && number != 0 {
				return out, errors.New("SAE_SETTLED_HEIGHT_INVALID")
			}
			out.SettledHeight = &height
		}
	} else {
		encoded, err = rlp.EncodeToBytes(&header)
		if err != nil {
			return out, err
		}
		out.HeaderEncoding = "ethereum-execution-header-rlp/v1"
		if chainKey == "evm:42161" {
			out.HeaderEncoding = "arbitrum-nitro-header-rlp/v1"
		}
	}
	if !receiptHex(out.Block.Hash, 32) || crypto.Keccak256Hash(encoded) != common.HexToHash(out.Block.Hash) {
		return out, errors.New("BLOCK_INTEGRITY_HEADER_HASH")
	}
	if err := ctx.Err(); err != nil {
		return out, err
	}
	if chainKey == "evm:42161" && len(header.Extra) == 32 && header.BaseFee != nil && header.Difficulty != nil && header.Difficulty.Cmp(big.NewInt(1)) == 0 {
		version := binary.BigEndian.Uint64(header.MixDigest[16:24])
		out.Nitro = &NitroHeaderInfo{SendRoot: "0x" + common.Bytes2Hex(header.Extra), SendCount: binary.BigEndian.Uint64(header.MixDigest[:8]), L1BlockNumber: binary.BigEndian.Uint64(header.MixDigest[8:16]), ArbOSVersion: version, CollectTips: version == 9 || header.MixDigest[25]&1 == 1}
	}
	out.Block = canonicalReceiptBlock(out.Block)
	out.TransactionsRoot = header.TxHash.Hex()
	out.StateRoot = header.Root.Hex()
	out.ReceiptsRoot = header.ReceiptHash.Hex()
	out.LogsBloom = "0x" + common.Bytes2Hex(header.Bloom[:])
	return out, nil
}

func headerQuantity(raw json.RawMessage, required bool) (*big.Int, error) {
	if len(raw) == 0 || string(raw) == "null" {
		if required {
			return nil, errors.New("BLOCK_HEADER_QUANTITY_MISSING")
		}
		return nil, nil
	}
	var value string
	if json.Unmarshal(raw, &value) != nil || !strings.HasPrefix(value, "0x") || len(value) < 3 {
		return nil, errors.New("BLOCK_HEADER_QUANTITY_INVALID")
	}
	n, ok := new(big.Int).SetString(value[2:], 16)
	if !ok || n.Sign() < 0 || n.BitLen() > 256 || value != "0x"+n.Text(16) {
		return nil, errors.New("BLOCK_HEADER_QUANTITY_INVALID")
	}
	return n, nil
}

func encodeCorethHeader(header *types.Header, fields map[string]json.RawMessage) ([]byte, error) {
	// Neither withdrawalsRoot nor requestsHash belongs to this upstream layout.
	for _, name := range []string{"withdrawalsRoot", "requestsHash"} {
		if len(fields[name]) > 0 && string(fields[name]) != "null" {
			return nil, errors.New("CORETH_HEADER_LAYOUT_INVALID")
		}
	}
	standard, err := rlp.EncodeToBytes(header)
	if err != nil {
		return nil, err
	}
	var items []rlp.RawValue
	if err = rlp.DecodeBytes(standard, &items); err != nil || len(items) < 15 {
		return nil, errors.New("CORETH_HEADER_LAYOUT_INVALID")
	}
	items = items[:15]
	var extHash string
	if json.Unmarshal(fields["extDataHash"], &extHash) != nil || !receiptHex(extHash, 32) {
		return nil, errors.New("CORETH_EXT_DATA_HASH_INVALID")
	}
	encoded, _ := rlp.EncodeToBytes(common.HexToHash(extHash))
	items = append(items, encoded)
	names := []string{"baseFeePerGas", "extDataGasUsed", "blockGasCost", "blobGasUsed", "excessBlobGas", "parentBeaconBlockRoot", "timestampMilliseconds", "minDelayExcess", "targetExponent", "minPriceExponent", "settledHeight", "settledGasUnix", "settledGasNumerator", "settledExcess"}
	tail := make([]rlp.RawValue, len(names))
	last := -1
	for i, name := range names {
		raw := fields[name]
		present := len(raw) > 0 && string(raw) != "null"
		required := (header.Time >= avalancheGraniteTime && i >= 6 && i <= 7) || (header.Time >= avalancheHeliconTime && i >= 8)
		forbidden := (header.Time < avalancheGraniteTime && i >= 6) || (header.Time < avalancheHeliconTime && i >= 8)
		if required && !present {
			return nil, errors.New("CORETH_ACTIVATION_FIELD_MISSING")
		}
		if forbidden && present {
			return nil, errors.New("CORETH_ACTIVATION_FIELD_UNEXPECTED")
		}
		if !present {
			tail[i] = rlp.RawValue{0x80}
			continue
		}
		last = i
		if name == "parentBeaconBlockRoot" {
			var hash string
			if json.Unmarshal(raw, &hash) != nil || !receiptHex(hash, 32) {
				return nil, errors.New("CORETH_HEADER_LAYOUT_INVALID")
			}
			tail[i], err = rlp.EncodeToBytes(common.HexToHash(hash))
		} else {
			n, e := headerQuantity(raw, true)
			if e != nil {
				return nil, e
			}
			if i >= 3 && !n.IsUint64() {
				return nil, errors.New("CORETH_HEADER_QUANTITY_OVERFLOW")
			}
			if name == "timestampMilliseconds" && n.Uint64()/1000 != header.Time {
				return nil, errors.New("CORETH_HEADER_TIMESTAMP_INVALID")
			}
			tail[i], err = rlp.EncodeToBytes(n)
		}
		if err != nil {
			return nil, err
		}
	}
	if last >= 0 {
		items = append(items, tail[:last+1]...)
	}
	return rlp.EncodeToBytes(items)
}
