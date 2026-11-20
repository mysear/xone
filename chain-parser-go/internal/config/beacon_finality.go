package config

import (
	"encoding/hex"
	"net/url"
	"path/filepath"
	"strings"
	"unicode"
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
	return b.validCheckpoint()
}

func (b *BeaconFinalityConfig) validCheckpoint() bool {
	// Keep the trusted root in the release and the updateable evidence in a separate file.
	if len(b.CheckpointRoot) != 66 || !strings.HasPrefix(b.CheckpointRoot, "0x") || strings.Trim(b.CheckpointRoot[2:], "0") == "" {
		return false
	}
	_, err := hex.DecodeString(b.CheckpointRoot[2:])
	return err == nil && strings.TrimSpace(b.ProofFile) != "" && filepath.Clean(b.ProofFile) == b.ProofFile && b.ProofFile != "." && b.MaxCheckpointAgeMS > 0 && b.MaxCheckpointAgeMS <= 16*8192*12000
}

type OPOriginConfig struct {
	BeaconFinalityConfig
	L1RPCURL       string `json:"l1_rpc_url"`
	L1ProofRPCURL  string `json:"l1_proof_rpc_url,omitempty"`
	BatchProofFile string `json:"batch_proof_file,omitempty"`
}

func (p *OPOriginConfig) valid(c Chain, service string) bool {
	const genesis = "0x7ca38a1916c42007829c55e69d3e9a73265554b586a499015373241b8a3fa48b"
	if service != "chain-parser-go" || c.Key != "evm:10" || c.Genesis != genesis || c.Confirmation != "finalized" || c.BlockIntegrity != "known-evm-native-commitments-v2" || !p.validCheckpoint() {
		return false
	}
	if path := p.BatchProofFile; path != "" && (strings.TrimSpace(path) == "" || path == "." || filepath.Clean(path) != path) {
		return false
	}
	return validOriginEndpoint(p.L1RPCURL) && (p.L1ProofRPCURL == "" || validOriginEndpoint(p.L1ProofRPCURL))
}

func validOriginEndpoint(endpoint string) bool {
	u, err := url.Parse(endpoint)
	return err == nil && u.Hostname() != "" && (u.Scheme == "https" || u.Scheme == "http") && u.User == nil && u.Fragment == "" && strings.IndexFunc(endpoint, unicode.IsSpace) < 0
}
