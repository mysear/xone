package evm

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func retainedChainHeader(t *testing.T, name string) json.RawMessage {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, name, "vector.json"))
	if err != nil {
		t.Fatal(err)
	}
	var v struct{ Block json.RawMessage }
	if err = json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	return v.Block
}
func TestChainHeadersOfficialPublic(t *testing.T) {
	for _, name := range []string{"arbitrum", "avalanche", "avalanche-current", "bsc", "optimism-regolith"} {
		t.Run(name, func(t *testing.T) {
			v := loadChainReceiptVector(t, name)
			proof, err := VerifyChainHeaderJSON(context.Background(), v.ChainKey, retainedChainHeader(t, name))
			if err != nil {
				t.Fatal(err)
			}
			if proof.Block.Hash != v.Block.Hash || proof.Block.Number != v.Block.Number || proof.TransactionsRoot == "" {
				t.Fatal(proof)
			}
		})
	}
}
func TestChainHeadersRejectNativeFieldTampering(t *testing.T) {
	for _, name := range []string{"arbitrum", "avalanche", "avalanche-current"} {
		fields := []string{"stateRoot", "transactionsRoot", "extraData", "nonce"}
		if name != "arbitrum" {
			fields = append(fields, "extDataHash", "extDataGasUsed", "blockGasCost")
		}
		if name == "avalanche-current" {
			fields = append(fields, "timestampMilliseconds", "minDelayExcess", "targetExponent", "minPriceExponent", "settledHeight", "settledGasUnix", "settledGasNumerator", "settledExcess")
		}
		for _, field := range fields {
			t.Run(name+"/"+field, func(t *testing.T) {
				v := loadChainReceiptVector(t, name)
				var header map[string]json.RawMessage
				json.Unmarshal(retainedChainHeader(t, name), &header)
				var value string
				json.Unmarshal(header[field], &value)
				if strings.HasSuffix(value, "1") {
					value = value[:len(value)-1] + "2"
				} else {
					value = value[:len(value)-1] + "1"
				}
				header[field], _ = json.Marshal(value)
				raw, _ := json.Marshal(header)
				if _, err := VerifyChainHeaderJSON(context.Background(), v.ChainKey, raw); err == nil {
					t.Fatal("native field not committed", field)
				}
			})
		}
	}
}
