package main

import (
	"context"
	"crypto/sha256"
	"fmt"
	"testing"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestReplayProviderWindowRejectsMismatchedProfileBeforeReading(t *testing.T) {
	valid := config.Chain{
		Key: "evm:56", Confirmation: "finalized", BlockIntegrity: evm.ChainNativeCommitmentsV2,
		RPCURLs: []string{"http://127.0.0.1:18091", "http://127.0.0.1:18092"},
		Pools:   []config.PoolConfig{{ID: "pool"}},
	}
	for _, tc := range []struct {
		name        string
		change      func(*config.Chain)
		first, last uint64
		want        string
	}{
		{"confirmation", func(c *config.Chain) { c.Confirmation = "latest" }, 10, 11, "REPLAY_PROVIDER_PROFILE"},
		{"single endpoint", func(c *config.Chain) { c.RPCURLs[1] = c.RPCURLs[0] }, 10, 11, "REPLAY_PROVIDER_PROFILE"},
		{"remote proof", func(c *config.Chain) { c.ProofRPCURL = "https://example.org" }, 10, 11, "REPLAY_LOOPBACK_REQUIRED"},
		{"same port alias", func(c *config.Chain) { c.RPCURLs[1] = "http://127.0.0.1:018091" }, 10, 11, "REPLAY_LOOPBACK_REQUIRED"},
		{"baseline only", func(*config.Chain) {}, 10, 10, "REPLAY_WINDOW"},
		{"overlong", func(*config.Chain) {}, 10, 15, "REPLAY_WINDOW"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			chain := valid
			chain.RPCURLs = append([]string(nil), chain.RPCURLs...)
			tc.change(&chain)
			snapshot := &config.Snapshot{Config: config.Config{
				Startup: config.Startup{MaxTail: 4}, Chains: []config.Chain{chain},
				Control: config.Control{Backend: "disabled"}, Journal: config.Journal{Transport: "http-development"},
			}}
			report, err := replayProviderWindow(context.Background(), snapshot, tc.first, tc.last)
			if report != nil || err == nil || err.Error() != tc.want {
				t.Fatalf("report = %v, error = %v, want %s", report, err, tc.want)
			}
		})
	}
}

func TestStagedDigestBindsTheSerializedStateAndFacts(t *testing.T) {
	raw := `{"block":{"chain_key":"evm:56","number_or_slot":2,"hash":"block","parent_hash":"parent","timestamp_ms":12000},"pools":[],"swaps":[]}`
	block := domain.BlockRef{ChainKey: "evm:56", Number: 2, Hash: "block", ParentHash: "parent", TimestampMS: 12000}
	digest, err := stagedDigest(block, []domain.Pool{}, []domain.SwapFact{})
	if want := fmt.Sprintf("%x", sha256.Sum256([]byte(raw))); err != nil || digest != want {
		t.Fatalf("digest = %s, error = %v, want %s", digest, err, want)
	}
}
