package config

import (
	"encoding/json"
	"testing"
)

func TestEthereumBlockIntegrityConfigRejectsUnknownOrWrongChainProfiles(t *testing.T) {
	loaded, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, tc := range []struct {
		name, chain, profile string
		valid                bool
	}{
		{"ethereum", "evm:1", "ethereum-header-transactions-v1", true},
		{"ethereum-without-profile", "evm:1", "", true},
		{"unknown", "evm:1", "future", false},
		{"base-is-not-ethereum-consensus", "evm:8453", "ethereum-header-transactions-v1", false},
		{"solana", "solana:mainnet-beta", "ethereum-header-transactions-v1", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := loaded.Config
			c.Chains = append([]Chain(nil), c.Chains...)
			c.Chains[0].Key = tc.chain
			b, _ := json.Marshal(map[string]string{"block_integrity": tc.profile})
			if json.Unmarshal(b, &c.Chains[0]) != nil {
				t.Fatal("test config decode")
			}
			err := c.Validate("chain-parser-go")
			if (err == nil) != tc.valid {
				t.Fatalf("valid=%v err=%v", tc.valid, err)
			}
		})
	}
}

func TestKnownEVMBodyAndBulkModesAreExplicitAndDoNotWidenEthereumProfile(t *testing.T) {
	loaded, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, key := range []string{"evm:1", "evm:10", "evm:56", "evm:8453", "evm:137"} {
		current := loaded.Config
		current.Chains = append([]Chain(nil), current.Chains...)
		current.Chains[0].Key = key
		current.Chains[0].BlockIntegrity = "known-evm-header-raw-transactions-v1"
		current.Chains[0].ReceiptsMode = "bulk-auto"
		if e = current.Validate("chain-parser-go"); e != nil {
			t.Fatal(key, e)
		}
		current.Chains[0].ReceiptsMode = "auto"
		if e = current.Validate("chain-parser-go"); e == nil {
			t.Fatal("unknown bulk fallback policy accepted")
		}
	}
	current := loaded.Config
	current.Chains = append([]Chain(nil), current.Chains...)
	current.Chains[0].Key = "evm:8453"
	current.Chains[0].BlockIntegrity = "ethereum-header-transactions-v1"
	if e = current.Validate("chain-parser-go"); e == nil {
		t.Fatal("Ethereum profile silently widened")
	}
}
