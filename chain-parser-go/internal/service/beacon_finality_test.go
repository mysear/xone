package service

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/provenance"
)

func TestBeaconShortTailPublicationGate(t *testing.T) {
	dir := os.Getenv("XONE_BEACON_REFRESH_FIXTURES")
	if dir == "" {
		t.Skip("requires retained mainnet finality proof")
	}
	const root = "0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7"
	proof, err := provenance.ReadOnlineBeaconProof(filepath.Join(dir, "beacon-refresh-192/proof.json"), root)
	if err != nil {
		t.Fatal(err)
	}
	height, hash := proof.ExecutionBlock()
	view := &beaconOwnerView{proof: proof, checkpointAge: ^uint64(0), finalizedAge: ^uint64(0), hashes: map[uint64]string{height: hash}}
	head := domain.BlockRef{ChainKey: "evm:1", Number: height, Hash: hash, ParentHash: "middle"}
	middle := domain.BlockRef{ChainKey: head.ChainKey, Number: height - 1, Hash: head.ParentHash, ParentHash: "base"}
	base := domain.BlockRef{ChainKey: head.ChainKey, Number: height - 2, Hash: middle.ParentHash}
	read := func(_ context.Context, tag string) (domain.BlockRef, error) {
		if tag != fmt.Sprintf("0x%x", middle.Number) {
			t.Fatalf("unexpected ancestor %s", tag)
		}
		return middle, nil
	}
	ctx := context.WithValue(context.Background(), beaconOwnerKey{}, view)
	s := Service{cfg: &config.Snapshot{Config: config.Config{Chains: []config.Chain{{Key: head.ChainKey, BeaconFinality: &config.BeaconFinalityConfig{}}}}}}
	gate := func(block domain.BlockRef) error {
		return s.runtimeScopeGate(ctx, nil, domain.Snapshot{Reference: domain.SnapshotRef{Block: block}})()
	}
	if err = view.admitTail(ctx, head, base, read); err != nil {
		t.Fatal(err)
	}
	if err = gate(middle); err != nil {
		t.Fatal(err)
	}
	middle.Hash = "changed"
	if err = gate(middle); err == nil || err.Error() != "BEACON_FINALITY_BLOCK_CONFLICT" {
		t.Fatalf("substituted publication: %v", err)
	}
	if err = view.admitTail(ctx, head, base, read); err == nil || err.Error() != "BEACON_FINALITY_PARENT_CONFLICT" {
		t.Fatalf("disconnected ancestry: %v", err)
	}
}
