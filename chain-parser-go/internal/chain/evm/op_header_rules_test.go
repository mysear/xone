package evm

import (
	"context"
	"encoding/json"
	"math/big"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
)

// Rehash each changed header and update transaction positions without changing
// their signed bytes or trie root. These are semantically invalid OP payloads,
// not corrupt hashes that would already fail the generic commitment check.
func opBlockWithHeaderMutation(t *testing.T, original json.RawMessage, change func(*types.Header)) json.RawMessage {
	t.Helper()
	var fields map[string]json.RawMessage
	var header types.Header
	if err := json.Unmarshal(original, &fields); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(original, &header); err != nil {
		t.Fatal(err)
	}
	change(&header)
	encoded, err := json.Marshal(&header)
	if err != nil {
		t.Fatal(err)
	}
	var updated map[string]json.RawMessage
	if err = json.Unmarshal(encoded, &updated); err != nil {
		t.Fatal(err)
	}
	for name, value := range updated {
		fields[name] = value
	}
	blockHash, _ := json.Marshal(header.Hash().Hex())
	fields["hash"] = blockHash
	var transactions []map[string]json.RawMessage
	if err = json.Unmarshal(fields["transactions"], &transactions); err != nil {
		t.Fatal(err)
	}
	for _, transaction := range transactions {
		transaction["blockHash"] = blockHash
	}
	fields["transactions"], err = json.Marshal(transactions)
	if err != nil {
		t.Fatal(err)
	}
	changed, err := json.Marshal(fields)
	if err != nil {
		t.Fatal(err)
	}
	return changed
}

func TestOPMainnetFixedHeaderRules(t *testing.T) {
	ctx := context.Background()
	for _, fixture := range []string{
		"op-mainnet-withdrawals/origin-144644465.json.gz",
		"op-mainnet-batches/portal-origin-26454436.json.gz",
	} {
		t.Run(fixture, func(t *testing.T) {
			var evidence OPOriginEvidence
			readOriginFixture(t, fixture, &evidence)
			body, err := VerifyChainBlockJSON(ctx, "evm:10", evidence.Block, 32<<20)
			if err != nil {
				t.Fatal("original mainnet block rejected", err)
			}
			semantics, err := ResolveNativeSemantics("evm:10", body.HeaderCommitment)
			if err != nil || !semantics.Admitted {
				t.Fatal("original mainnet native rules unavailable", semantics, err)
			}
			for _, fault := range []struct {
				name, want string
				change     func(*types.Header)
			}{
				{"difficulty", "OP_HEADER_DIFFICULTY", func(h *types.Header) { h.Difficulty = big.NewInt(1) }},
				{"nonce", "OP_HEADER_NONCE", func(h *types.Header) { h.Nonce = types.EncodeNonce(1) }},
				{"uncles", "OP_HEADER_UNCLE_HASH", func(h *types.Header) { h.UncleHash = common.HexToHash("0x1") }},
				{"gas limit", "OP_HEADER_GAS_LIMIT", func(h *types.Header) { h.GasLimit = params.MaxGasLimit + 1 }},
				{"gas used", "OP_HEADER_GAS_USED", func(h *types.Header) { h.GasUsed = h.GasLimit + 1 }},
			} {
				t.Run(fault.name, func(t *testing.T) {
					changed := opBlockWithHeaderMutation(t, evidence.Block, fault.change)
					var fields map[string]json.RawMessage
					var transactions struct{ Transactions []struct{ Hash string } }
					if err := json.Unmarshal(changed, &fields); err != nil {
						t.Fatal(err)
					}
					if err := json.Unmarshal(changed, &transactions); err != nil {
						t.Fatal(err)
					}
					var hashes []string
					for _, transaction := range transactions.Transactions {
						hashes = append(hashes, transaction.Hash)
					}
					fields["transactions"], _ = json.Marshal(hashes)
					headerOnly, err := json.Marshal(fields)
					if err != nil {
						t.Fatal(err)
					}
					// Check only the shared RLP serializer/hash here, without granting
					// another chain's provenance to this OP payload.
					if _, err := VerifyChainHeaderJSON(ctx, "evm:1", headerOnly); err != nil {
						t.Fatal("rehashing control failed", err)
					}
					if _, err := VerifyChainBlockJSON(ctx, "evm:10", changed, 32<<20); err == nil || err.Error() != fault.want {
						t.Fatal("invalid OP fixed header accepted", err)
					}
				})
			}
		})
	}
}

func TestOPMainnetFixedHeaderBedrockBoundary(t *testing.T) {
	ctx := context.Background()
	for _, tc := range []struct {
		name      string
		height    uint64
		timestamp uint64
		admitted  bool
	}{
		{"legacy genesis", 0, 0, false},
		{"pre Bedrock", opBedrockHeight - 1, opBedrockTime - 1, false},
		{"migrated genesis", opBedrockHeight, opBedrockTime, true},
		{"post Bedrock", opBedrockHeight + 1, opBedrockTime + opBlockSeconds, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			// This synthetic boundary header does not claim actual chain inclusion.
			header := &types.Header{Number: new(big.Int).SetUint64(tc.height), Time: tc.timestamp, Difficulty: big.NewInt(0), UncleHash: types.EmptyUncleHash, GasLimit: params.MaxGasLimit, GasUsed: params.MaxGasLimit}
			raw, err := json.Marshal(header)
			if err != nil {
				t.Fatal(err)
			}
			verified, err := VerifyChainHeaderJSON(ctx, "evm:10", raw)
			if err != nil {
				t.Fatal("maximum valid gas boundary rejected", err)
			}
			semantics, err := ResolveNativeSemantics("evm:10", verified)
			if err != nil || semantics.Admitted != tc.admitted {
				t.Fatal("native semantics admission changed", semantics, err)
			}
			header.Difficulty = big.NewInt(1)
			header.Nonce = types.EncodeNonce(1)
			header.UncleHash = common.HexToHash("0x1")
			header.GasLimit = params.MaxGasLimit + 1
			header.GasUsed = header.GasLimit + 1
			raw, err = json.Marshal(header)
			if err != nil {
				t.Fatal(err)
			}
			_, err = VerifyChainHeaderJSON(ctx, "evm:10", raw)
			if !tc.admitted {
				if err != nil {
					t.Fatal("legacy header handling changed", err)
				}
			} else if err == nil || err.Error() != "OP_HEADER_DIFFICULTY" {
				t.Fatal("Bedrock rules not enforced at activation", err)
			}
		})
	}
}
