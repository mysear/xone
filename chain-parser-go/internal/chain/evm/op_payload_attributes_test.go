package evm

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/ethereum/go-ethereum/core/types"
)

// Mutate the payload recipient while maintaining the execution header hash and
// transaction positions. These inputs still pass generic commitment checks:
// the missing rule is the fixed OP derivation attribute, not a malformed hash.
func opPayloadWithRecipient(t *testing.T, raw json.RawMessage, recipient string) json.RawMessage {
	t.Helper()
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		t.Fatal(err)
	}
	fields["miner"], _ = json.Marshal(recipient)
	changed, err := json.Marshal(fields)
	if err != nil {
		t.Fatal(err)
	}
	var header types.Header
	if err = json.Unmarshal(changed, &header); err != nil {
		t.Fatal(err)
	}
	encodedHash, _ := json.Marshal(header.Hash().Hex())
	fields["hash"] = encodedHash
	var transactions []map[string]json.RawMessage
	if err = json.Unmarshal(fields["transactions"], &transactions); err != nil {
		t.Fatal(err)
	}
	for _, transaction := range transactions {
		transaction["blockHash"] = encodedHash
	}
	fields["transactions"], _ = json.Marshal(transactions)
	changed, err = json.Marshal(fields)
	if err != nil {
		t.Fatal(err)
	}
	return changed
}

func TestOPPayloadFeeRecipient(t *testing.T) {
	for _, name := range []string{
		"op-mainnet-withdrawals/origin-144644465.json.gz",
		"op-mainnet-batches/portal-origin-26454436.json.gz",
	} {
		t.Run(name, func(t *testing.T) {
			var evidence OPOriginEvidence
			readOriginFixture(t, name, &evidence)
			ctx := context.Background()
			body, err := VerifyChainBlockJSON(ctx, "evm:10", evidence.Block, 32<<20)
			if err != nil {
				t.Fatal(err)
			}
			timestamp, err := canonicalIndex(body.Block.Timestamp)
			if err != nil {
				t.Fatal(err)
			}
			if _, err = opBlockOrigin(&body, timestamp); err != nil {
				t.Fatal("original payload rejected", err)
			}
			for _, recipient := range []string{
				"0x0000000000000000000000000000000000000000",
				"0x4200000000000000000000000000000000000012",
			} {
				t.Run(recipient, func(t *testing.T) {
					changed := opPayloadWithRecipient(t, evidence.Block, recipient)
					body, err := VerifyChainBlockJSON(ctx, "evm:10", changed, 32<<20)
					if err != nil {
						t.Fatal("mutation must retain valid header/body commitments", err)
					}
					if _, err = opBlockOrigin(&body, timestamp); err == nil || err.Error() != "OP_ORIGIN_FEE_RECIPIENT" {
						t.Fatal("noncanonical OP recipient accepted", err)
					}
				})
			}
		})
	}
}
