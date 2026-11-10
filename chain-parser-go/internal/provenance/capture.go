package provenance

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"strings"

	"github.com/ethereum/go-ethereum/core/types"
)

type CaptureRequest struct {
	Primary     string
	Witness     string
	ChainID     string
	BlockTag    string
	Addresses   []string
	StorageKeys map[string][]string
}
type Caller func(context.Context, string, string, any, any) error

// Capture records one immutable H. Witness agreement establishes cross-checking,
// never provider failure-domain independence or chain consensus authority.
func Capture(ctx context.Context, request CaptureRequest, call Caller) (Evidence, Report, error) {
	var evidence Evidence
	var report Report
	if request.Primary == "" || request.Witness == "" || request.Primary == request.Witness || len(request.Addresses) == 0 || len(request.Addresses) > 64 {
		return evidence, report, invalid("CAPTURE_SCOPE")
	}
	if request.BlockTag != "finalized" && request.BlockTag != "safe" {
		if _, err := quantity(request.BlockTag); err != nil {
			return evidence, report, invalid("CAPTURE_BLOCK")
		}
	}
	chain, ok := new(big.Int).SetString(request.ChainID, 10)
	if !ok || chain.Sign() <= 0 || chain.Text(10) != request.ChainID {
		return evidence, report, invalid("CHAIN")
	}
	seen := map[string]bool{}
	for _, address := range request.Addresses {
		if _, err := data(address, 20); err != nil || seen[strings.ToLower(address)] {
			return evidence, report, invalid("CAPTURE_ADDRESS")
		}
		seen[strings.ToLower(address)] = true
		keys := request.StorageKeys[address]
		if len(keys) > 256 {
			return evidence, report, invalid("CAPTURE_STORAGE_LIMIT")
		}
		for _, key := range keys {
			if _, err := data(key, 32); err != nil {
				return evidence, report, err
			}
		}
	}
	for _, source := range []string{request.Primary, request.Witness} {
		var value string
		if err := call(ctx, source, "eth_chainId", []any{}, &value); err != nil {
			return evidence, report, err
		}
		got, err := quantity(value)
		if err != nil || got.Cmp(chain) != 0 {
			return evidence, report, invalid("CAPTURE_CHAIN_ID")
		}
	}
	var block json.RawMessage
	if err := call(ctx, request.Primary, "eth_getBlockByNumber", []any{request.BlockTag, true}, &block); err != nil {
		return evidence, report, err
	}
	var header types.Header
	if json.Unmarshal(block, &header) != nil || header.Number == nil || !header.Number.IsUint64() {
		return evidence, report, invalid("HEADER")
	}
	if request.BlockTag != "finalized" && request.BlockTag != "safe" && "0x"+header.Number.Text(16) != request.BlockTag {
		return evidence, report, invalid("CAPTURE_BLOCK_NUMBER")
	}
	var fields struct {
		Hash         string            `json:"hash"`
		Transactions []json.RawMessage `json:"transactions"`
	}
	if json.Unmarshal(block, &fields) != nil || !strings.EqualFold(fields.Hash, header.Hash().Hex()) || fields.Transactions == nil {
		return evidence, report, invalid("CAPTURE_HEADER_OR_BODY")
	}
	evidence = Evidence{SchemaVersion: 1, ChainID: request.ChainID, ExpectedBlockHash: header.Hash().Hex(), Block: block, RawTransactions: []string{}, Deployments: []Deployment{}}
	for _, value := range fields.Transactions {
		var tx types.Transaction
		if json.Unmarshal(value, &tx) != nil {
			return Evidence{}, report, invalid("TRANSACTION_ENCODING")
		}
		raw, err := tx.MarshalBinary()
		if err != nil {
			return Evidence{}, report, invalid("TRANSACTION_ENCODING")
		}
		evidence.RawTransactions = append(evidence.RawTransactions, "0x"+hex.EncodeToString(raw))
	}
	checkHeader := func(source string) error {
		var other types.Header
		if err := call(ctx, source, "eth_getBlockByNumber", []any{"0x" + header.Number.Text(16), false}, &other); err != nil {
			return err
		}
		if other.Hash() != header.Hash() {
			return invalid("CAPTURE_HEADER_CONTRADICTION")
		}
		return nil
	}
	if err := checkHeader(request.Witness); err != nil {
		return Evidence{}, report, err
	}
	anchor := map[string]any{"blockHash": header.Hash().Hex(), "requireCanonical": true}
	for _, address := range request.Addresses {
		var code string
		if err := call(ctx, request.Primary, "eth_getCode", []any{address, anchor}, &code); err != nil {
			return Evidence{}, report, err
		}
		keys := request.StorageKeys[address]
		if keys == nil {
			keys = []string{}
		}
		var account AccountProof
		if err := call(ctx, request.Primary, "eth_getProof", []any{address, keys, anchor}, &account); err != nil {
			return Evidence{}, report, err
		}
		if len(account.StorageProof) != len(keys) {
			return Evidence{}, report, invalid("CAPTURE_STORAGE_INCOMPLETE")
		}
		for i, key := range keys {
			got, err := quantity(account.StorageProof[i].Key)
			if err != nil {
				raw, parse := data(account.StorageProof[i].Key, 32)
				if parse != nil {
					return Evidence{}, report, invalid("CAPTURE_STORAGE_KEY")
				}
				got = new(big.Int).SetBytes(raw)
			}
			expected, _ := data(key, 32)
			if got.Cmp(new(big.Int).SetBytes(expected)) != 0 {
				return Evidence{}, report, invalid("CAPTURE_STORAGE_KEY")
			}
			account.StorageProof[i].Key = key
		}
		evidence.Deployments = append(evidence.Deployments, Deployment{Address: address, Code: code, Proof: account})
	}
	if err := checkHeader(request.Primary); err != nil {
		return Evidence{}, report, err
	}
	report, err := Verify(evidence)
	return evidence, report, err
}
