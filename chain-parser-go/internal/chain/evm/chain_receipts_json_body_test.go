package evm

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func retainedFullChainBlock(t *testing.T, name string) json.RawMessage {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, name, "full-block.json"))
	if err != nil {
		t.Fatal(err)
	}
	var envelope struct {
		Response struct{ Result json.RawMessage }
	}
	if err = json.Unmarshal(raw, &envelope); err != nil {
		t.Fatal(err)
	}
	return envelope.Response.Result
}
func TestNativeJSONBodyActualPublicBlocks(t *testing.T) {
	for _, name := range []string{"arbitrum", "avalanche", "avalanche-current"} {
		t.Run(name, func(t *testing.T) {
			v := loadChainReceiptVector(t, name)
			proof, err := VerifyChainBlockJSON(context.Background(), v.ChainKey, retainedFullChainBlock(t, name), 16<<20)
			if err != nil {
				t.Fatal(err)
			}
			if !sameReceiptAnchor(v.Block, proof.Block) || len(proof.RawTransactions) != len(v.Block.Transactions) {
				t.Fatal(proof.Block)
			}
			if name == "arbitrum" && proof.RawTransactions[0][:4] != "0x6a" {
				t.Fatal("native internal transaction was not encoded", proof.RawTransactions[0])
			}
		})
	}
}
func TestNativeJSONBodyRejectsUnknownAndTamperedNative(t *testing.T) {
	for _, fault := range []string{"type", "chain", "data", "position", "hash", "missing"} {
		t.Run(fault, func(t *testing.T) {
			var full map[string]json.RawMessage
			json.Unmarshal(retainedFullChainBlock(t, "arbitrum"), &full)
			var transactions []map[string]json.RawMessage
			json.Unmarshal(full["transactions"], &transactions)
			values := map[string]struct{ Field, Value string }{"type": {"type", "0x67"}, "chain": {"chainId", "0x1"}, "data": {"input", "0x00"}, "position": {"transactionIndex", "0x1"}, "hash": {"hash", "0x01"}, "missing": {"input", ""}}
			mutation := values[fault]
			if fault == "missing" {
				delete(transactions[0], mutation.Field)
			} else {
				transactions[0][mutation.Field], _ = json.Marshal(mutation.Value)
			}
			full["transactions"], _ = json.Marshal(transactions)
			raw, _ := json.Marshal(full)
			if _, err := VerifyChainBlockJSON(context.Background(), "evm:42161", raw, 16<<20); err == nil {
				t.Fatal("accepted native JSON mutation", fault)
			}
		})
	}
}

func TestNativeArchivedRawRemainsUnsupportedBeforeJSONProof(t *testing.T) {
	for _, name := range []string{"arbitrum", "avalanche"} {
		v := loadChainReceiptVector(t, name)
		header, err := VerifyChainHeaderJSON(context.Background(), v.ChainKey, retainedChainHeader(t, name))
		if err != nil {
			t.Fatal(err)
		}
		raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, name, "body.json"))
		if err != nil {
			t.Fatal(err)
		}
		var body struct {
			RawTransactions []struct{ Response struct{ Result string } } `json:"raw_transactions"`
		}
		json.Unmarshal(raw, &body)
		raws := []string{}
		for _, r := range body.RawTransactions {
			raws = append(raws, r.Response.Result)
		}
		budget := receiptProofBudget{remaining: 16 << 20}
		if err := verifyRawBodyCommitment(context.Background(), v.ChainKey, header, raws, &budget); err == nil {
			t.Fatal("public raw 0x must remain a rejection", name)
		}
	}
}
