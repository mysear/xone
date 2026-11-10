package service

import (
	"context"
	"testing"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestNormalHistoryRuntimeInstallsNativeEvidenceResponsibilitySink(t *testing.T) {
	f := nativeServicePublicFixture(t, "avalanche-current")
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	f.chain.Owner = "chain-parser-go"
	f.chain.Assignment = 1
	f.chain.Confirmation = "finalized"
	f.chain.ReorgWindow = 128
	f.chain.RPCURLs = append([]string(nil), f.client.URLs...)
	cfg.Config.Chains = []config.Chain{f.chain}
	service, err := NewWithSource(cfg, func(context.Context, config.Chain) (domain.Snapshot, error) {
		t.Fatal("history constructor called online baseline")
		return domain.Snapshot{}, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	defer service.Close()
	runtime, err := service.NewChainHistoryRuntime(f.chain.Key, &HistoryScheduler{})
	if err != nil {
		t.Fatal(err)
	}
	defer runtime.Close()
	group := &rawCapture{ID: "normal-history-native-construction", Chain: f.chain.Key, Max: 512, Kind: "history"}
	ctx := context.WithValue(context.Background(), rawCaptureKey{}, group)
	batch, _, err := runtime.options.ReadFacts(ctx, 99642059)
	if err != nil {
		t.Fatal("normal source construction omitted native evidence hook:", err)
	}
	if len(batch.Blocks) != 1 || batch.Blocks[0].Block.Hash != f.block.Hash {
		t.Fatal("historical anchor changed", batch)
	}
	proofs := 0
	for _, pin := range group.Pins {
		if pin.Raw.Owner.ProviderID == "native-history-proof/v2" {
			proofs++
			replayed, _, err := ReplayNativeHistoryEvidence(ctx, runtime.options.RawArchive, pin.Raw, f.chain)
			if err != nil || len(replayed.Blocks) != 1 || replayed.Blocks[0].Block != batch.Blocks[0].Block {
				t.Fatal("normal history lost replayable native proof", err, replayed)
			}
		}
	}
	if proofs != 1 {
		t.Fatal("normal history did not own exactly one native proof", proofs)
	}
}
