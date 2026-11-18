package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func TestEVMConfirmationPolicyRequiresCurrentHeadTagBeforeOpeningResources(t *testing.T) {
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	for _, tag := range []string{"latest", "safe", "finalized"} {
		cfg.Config.Chains[0].Confirmation = tag
		if err = cfg.Config.Validate("chain-parser-go"); err != nil {
			t.Fatal(tag, err)
		}
	}
	for _, tag := range []string{"", "pending", "earliest", "0x1", "0x0", "123", "LATEST", "safe ", " finalized", "finalized\n", "confirmed", "private-input-must-not-leak"} {
		cfg.Config.Chains[0].Confirmation = tag
		if err = cfg.Config.Validate("chain-parser-go"); err == nil || err.Error() != "CONFIG_CHAIN_CONFIRMATION" {
			t.Fatal("non-current head policy admitted", tag, err)
		}
	}
}
func TestEVMConfirmationPolicyExportsCurrentCrossCompilerInputsAndHashes(t *testing.T) {
	raw, err := os.ReadFile("../../config/config-test.yaml")
	if err != nil {
		t.Fatal(err)
	}
	out := os.Getenv("XONE_CONFIRMATION_CONFIG_OUTPUT")
	if out == "" {
		out = t.TempDir()
	}
	if err = os.MkdirAll(out, 0700); err != nil {
		t.Fatal(err)
	}
	var cases []map[string]any
	hashes := map[string]bool{}
	for i, tag := range []string{"latest", "safe", "finalized", "", "pending", "earliest", "0x1", "0x0", "123", "LATEST", "safe ", " finalized", "finalized\n", "confirmed", "private-input-must-not-leak"} {
		var doc map[string]any
		if err = json.Unmarshal(raw, &doc); err != nil {
			t.Fatal(err)
		}
		chain := doc["chains"].([]any)[0].(map[string]any)
		chain["confirmation_tag"] = tag
		doc["state"].(map[string]any)["path"] = filepath.Join(out, "must-not-initialize-state")
		name := fmt.Sprintf("confirmation-%02d.json", i)
		path := filepath.Join(out, name)
		encoded, _ := json.Marshal(doc)
		if err = os.WriteFile(path, encoded, 0600); err != nil {
			t.Fatal(err)
		}
		cfg, err := Load(path, "chain-parser-go")
		row := map[string]any{"file": name, "tag": tag, "valid": i < 3}
		if i < 3 {
			if err != nil {
				t.Fatal(tag, err)
			}
			row["public_hash"] = cfg.PublicHash
			row["economic_hash"] = cfg.EconomicHash
			if hashes[cfg.PublicHash] {
				t.Fatal("policy omitted from public hash")
			}
			hashes[cfg.PublicHash] = true
		} else {
			if err == nil || err.Error() != "CONFIG_CHAIN_CONFIRMATION" {
				t.Fatal(tag, err)
			}
			row["error"] = "CONFIG_CHAIN_CONFIRMATION"
		}
		cases = append(cases, row)
	}
	encoded, _ := json.MarshalIndent(map[string]any{"cases": cases}, "", "  ")
	if err = os.WriteFile(filepath.Join(out, "confirmation-vectors.json"), encoded, 0600); err != nil {
		t.Fatal(err)
	}
	if _, err = os.Stat(filepath.Join(out, "must-not-initialize-state")); !os.IsNotExist(err) {
		t.Fatal("configuration validation initialized state", err)
	}
}
