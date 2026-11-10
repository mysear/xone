package config

import (
	"encoding/json"
	"testing"
)

func TestNativeProofConfigurationRequiresKnownChainAndFiniteExplicitLimits(t *testing.T) {
	loaded, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	valid := map[string]any{"max_bytes": 16 << 20, "max_headers": 64, "max_window_blocks": 16, "max_transactions": 4096, "max_lookahead": 32}
	check := func(t *testing.T, key, profile string, proof any, want bool) {
		t.Helper()
		c := loaded.Config
		c.Chains = append([]Chain(nil), c.Chains...)
		c.Chains[0].Key = key
		raw, _ := json.Marshal(map[string]any{"block_integrity": profile, "native_proof": proof})
		if err := json.Unmarshal(raw, &c.Chains[0]); err != nil {
			t.Fatal(err)
		}
		if err := c.Validate("chain-parser-go"); (err == nil) != want {
			t.Fatalf("chain=%s profile=%s proof=%v want=%v error=%v", key, profile, proof, want, err)
		}
	}
	for _, key := range []string{"evm:1", "evm:10", "evm:56", "evm:8453", "evm:137", "evm:42161", "evm:43114"} {
		t.Run(key, func(t *testing.T) { check(t, key, "known-evm-native-commitments-v2", valid, true) })
	}
	check(t, "evm:31337", "known-evm-native-commitments-v2", valid, false)
	check(t, "evm:1", "known-evm-native-commitments-v2", nil, false)
	check(t, "evm:1", "ethereum-header-transactions-v1", valid, false)
	check(t, "evm:1", "", valid, false)
	for field, bad := range map[string]int{"max_bytes": 65 << 20, "max_headers": 513, "max_window_blocks": 129, "max_transactions": 100001, "max_lookahead": 513} {
		for _, value := range []int{0, bad} {
			copy := make(map[string]any, len(valid))
			for key, v := range valid {
				copy[key] = v
			}
			copy[field] = value
			check(t, "evm:43114", "known-evm-native-commitments-v2", copy, false)
		}
	}
}
