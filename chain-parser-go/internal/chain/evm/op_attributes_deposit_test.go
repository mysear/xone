package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
	"xone/chain-parser-go/internal/provenance"
)

func TestOPAttributesDepositGas(t *testing.T) {
	for _, fixture := range []string{"op-mainnet-withdrawals/origin-144644465.json.gz", "op-mainnet-batches/portal-origin-26454436.json.gz"} {
		var input OPOriginEvidence
		readOriginFixture(t, fixture, &input)
		body, err := VerifyChainBlockJSON(context.Background(), "evm:10", input.Block, 32<<20)
		if err != nil {
			t.Fatal(err)
		}
		ts, _ := canonicalIndex(body.Block.Timestamp)
		deposit, _, err := parseOPOrigin(common.FromHex(body.RawTransactions[0]), ts)
		if err != nil || deposit.Gas != 1_000_000 {
			t.Fatal(deposit.Gas, err)
		}
		for _, gas := range []uint64{0, 999_999, 1_000_001, 150_000_000, ^uint64(0)} {
			t.Run(fmt.Sprintf("%s/%d", fixture, gas), func(t *testing.T) {
				bad := deposit
				bad.Gas = gas
				wire, err := rlp.EncodeToBytes(bad)
				if err != nil {
					t.Fatal(err)
				}
				if _, _, err := parseOPOrigin(append([]byte{0x7e}, wire...), ts); err == nil || err.Error() != "OP_PACKED_ATTRIBUTES_GAS" {
					t.Fatal(err)
				}
			})
		}
	}
}

// A provider can change the deposit allowance and recompute all L2 commitments.
// Body integrity alone must not grant that transaction derivation authority.
func TestOPOriginRejectsRecommittedAttributesGas(t *testing.T) {
	var beacon provenance.OnlineBeaconProof
	readOriginFixture(t, "op-mainnet-batches/beacon-26454447.json.gz", &beacon)
	proof, err := provenance.VerifyExecutionFinality("0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7", beacon.Bootstrap, beacon.Finality, beacon.Updates)
	if err != nil {
		t.Fatal(err)
	}
	var input OPOriginEvidence
	readOriginFixture(t, "op-mainnet-batches/portal-origin-26454436.json.gz", &input)
	ctx := context.Background()
	body, err := VerifyChainBlockJSON(ctx, "evm:10", input.Block, 32<<20)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	var txs []map[string]json.RawMessage
	if err = json.Unmarshal(input.Block, &fields); err != nil {
		t.Fatal(err)
	}
	if err = json.Unmarshal(fields["transactions"], &txs); err != nil {
		t.Fatal(err)
	}
	txs[0]["gas"] = json.RawMessage(`"0xf4241"`)
	wire, err := encodeOPDepositJSON(txs[0])
	if err != nil {
		t.Fatal(err)
	}
	txs[0]["hash"], _ = json.Marshal(crypto.Keccak256Hash(wire).Hex())
	encoded := make(encodedChainReceipts, len(body.RawTransactions))
	for i, raw := range body.RawTransactions {
		encoded[i] = common.FromHex(raw)
	}
	encoded[0] = wire
	var header types.Header
	if err = json.Unmarshal(body.Header, &header); err != nil {
		t.Fatal(err)
	}
	header.TxHash = types.DeriveSha(encoded, trie.NewStackTrie(nil))
	hash := header.Hash().Hex()
	fields["transactionsRoot"], _ = json.Marshal(header.TxHash.Hex())
	fields["hash"], _ = json.Marshal(hash)
	for _, tx := range txs {
		tx["blockHash"], _ = json.Marshal(hash)
	}
	fields["transactions"], _ = json.Marshal(txs)
	input.Block, err = json.Marshal(fields)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = VerifyChainBlockJSON(ctx, "evm:10", input.Block, 32<<20); err != nil {
		t.Fatal("mutation must retain valid header and body commitments", err)
	}
	if report, err := VerifyOPOrigin(ctx, proof, input); err == nil || err.Error() != "OP_PACKED_ATTRIBUTES_GAS" || report.Authority != "" {
		t.Fatal(report, err)
	}
}
