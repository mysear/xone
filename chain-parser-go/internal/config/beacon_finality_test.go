package config

import (
	"strings"
	"testing"
)

func TestOwnerBeaconPolicy(t *testing.T) {
	p := BeaconFinalityConfig{CheckpointRoot: "0x" + strings.Repeat("12", 32), ProofFile: "proof.json", MaxCheckpointAgeMS: 86400000}
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	base := cfg.Config.Chains[0]
	base.Key = "evm:1"
	base.Genesis = "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
	base.Confirmation = "finalized"
	base.BlockIntegrity = "known-evm-native-commitments-v2"
	base.NativeProof = &NativeProofConfig{MaxBytes: 1 << 20, MaxHeaders: 16, MaxWindowBlocks: 8, MaxTransactions: 256, MaxLookahead: 16}
	for _, change := range []string{"valid", "chain", "genesis", "root", "age", "file", "tag", "integrity"} {
		t.Run(change, func(t *testing.T) {
			c, b := base, p
			switch change {
			case "chain":
				c.Key = "evm:10"
			case "genesis":
				c.Genesis = p.CheckpointRoot
			case "root":
				b.CheckpointRoot = "0x" + strings.Repeat("00", 32)
			case "age":
				b.MaxCheckpointAgeMS = 0
			case "file":
				b.ProofFile = ""
			case "tag":
				c.Confirmation = "safe"
			case "integrity":
				c.BlockIntegrity = ""
			}
			c.BeaconFinality = &b
			cfg.Config.Chains[0] = c
			err := cfg.Config.Validate("chain-parser-go")
			if (err == nil) != (change == "valid") {
				t.Fatal(err)
			}
		})
	}
}

func TestAccountProofRPCConfiguration(t *testing.T) {
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	chain := &cfg.Config.Chains[0]
	chain.Key = "evm:1"
	chain.ProofRPCURL = "https://proof.example/rpc"
	if err = cfg.Config.Validate("chain-parser-go"); err == nil {
		t.Fatal("non-native profile accepted proof route")
	}
	chain.BlockIntegrity = "known-evm-native-commitments-v2"
	chain.NativeProof = &NativeProofConfig{MaxBytes: 1 << 20, MaxHeaders: 16, MaxWindowBlocks: 8, MaxTransactions: 256, MaxLookahead: 16}
	for _, raw := range []string{"https://proof.example/rpc", "http://127.0.0.1:2345", ""} {
		chain.ProofRPCURL = raw
		if err = cfg.Config.Validate("chain-parser-go"); err != nil {
			t.Fatal(err)
		}
	}
	for _, raw := range []string{"file:///proof", "/rpc", "https://user:pass@source.example", "https://source.example/#fragment", "https://source.example/a b"} {
		chain.ProofRPCURL = raw
		if err = cfg.Config.Validate("chain-parser-go"); err == nil || err.Error() != "CONFIG_PROOF_RPC_URL" {
			t.Fatal(err)
		}
	}
}

func TestOPOriginPolicy(t *testing.T) {
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	chain := cfg.Config.Chains[0]
	chain.Key = "evm:10"
	chain.Genesis = "0x7ca38a1916c42007829c55e69d3e9a73265554b586a499015373241b8a3fa48b"
	chain.Confirmation = "finalized"
	chain.BlockIntegrity = "known-evm-native-commitments-v2"
	chain.NativeProof = &NativeProofConfig{MaxBytes: 1 << 20, MaxHeaders: 16, MaxWindowBlocks: 8, MaxTransactions: 256, MaxLookahead: 16}
	policy := OPOriginConfig{BeaconFinalityConfig: BeaconFinalityConfig{CheckpointRoot: "0x" + strings.Repeat("12", 32), ProofFile: "l1.json", MaxCheckpointAgeMS: 86400000}, L1RPCURL: "https://example.org/rpc"}
	for _, name := range []string{"valid", "genesis", "tag", "wrong chain", "file", "missing endpoint"} {
		t.Run(name, func(t *testing.T) {
			c, p := chain, policy
			switch name {
			case "genesis":
				c.Genesis = p.CheckpointRoot
			case "tag":
				c.Confirmation = "safe"
			case "wrong chain":
				c.Key = "evm:1"
			case "file":
				p.ProofFile = ""
			case "missing endpoint":
				p.L1RPCURL = ""
			}
			c.OPOrigin = &p
			cfg.Config.Chains[0] = c
			if err := cfg.Config.Validate("chain-parser-go"); (err == nil) != (name == "valid") {
				t.Fatal(err)
			}
		})
	}
}
