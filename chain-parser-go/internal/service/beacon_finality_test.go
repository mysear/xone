package service

import (
	"context"
	"fmt"
	"testing"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestBeaconShortTailPublicationGate(t *testing.T) {
	view, head, base, refs := retainedBeaconTail(t, 2)
	middle := refs[head.Number-1]
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
	if err := view.admitTail(ctx, head, base, read); err != nil {
		t.Fatal(err)
	}
	if err := gate(middle); err != nil {
		t.Fatal(err)
	}
	middle.Hash = "changed"
	if err := gate(middle); err == nil || err.Error() != "BEACON_FINALITY_BLOCK_CONFLICT" {
		t.Fatalf("substituted publication: %v", err)
	}
	if err := view.admitTail(ctx, head, base, read); err == nil || err.Error() != "BEACON_FINALITY_PARENT_CONFLICT" {
		t.Fatalf("disconnected ancestry: %v", err)
	}
}
