package config

import (
	"encoding/hex"
	"path/filepath"
	"strings"
)

type BeaconFinalityConfig struct {
	CheckpointRoot     string `json:"checkpoint_root"`
	ProofFile          string `json:"proof_file"`
	MaxCheckpointAgeMS uint64 `json:"max_checkpoint_age_ms"`
}

func (b *BeaconFinalityConfig) valid(c Chain, service string) bool {
	if service != "chain-parser-go" || c.Key != "evm:1" || c.Genesis != "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3" || c.Confirmation != "finalized" || c.BlockIntegrity != "known-evm-native-commitments-v2" {
		return false
	}
	// Keep the trusted root in the release and the updateable evidence in a separate file.
	if len(b.CheckpointRoot) != 66 || !strings.HasPrefix(b.CheckpointRoot, "0x") || strings.Trim(b.CheckpointRoot[2:], "0") == "" {
		return false
	}
	_, err := hex.DecodeString(b.CheckpointRoot[2:])
	return err == nil && strings.TrimSpace(b.ProofFile) != "" && filepath.Clean(b.ProofFile) == b.ProofFile && b.ProofFile != "." && b.MaxCheckpointAgeMS > 0 && b.MaxCheckpointAgeMS <= 16*8192*12000
}
