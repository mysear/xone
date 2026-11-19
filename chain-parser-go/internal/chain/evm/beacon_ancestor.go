package evm

import (
	"context"
	"encoding/json"
	"errors"

	"xone/chain-parser-go/internal/domain"
)

// BeaconAncestorReader verifies the execution headers linking a finalized head
// to the owner cursor. State and receipt reads still use the normal adapter.
type BeaconAncestorReader struct{ Adapter *Adapter }

func (r BeaconAncestorReader) Header(ctx context.Context, tag string) (domain.BlockRef, error) {
	a := r.Adapter
	if a == nil || a.Client == nil || a.Chain.Key != "evm:1" || a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 || a.Chain.BeaconFinality == nil {
		return domain.BlockRef{}, errors.New("BEACON_ANCESTRY_PROFILE")
	}
	height, err := canonicalIndex(tag)
	if err != nil {
		return domain.BlockRef{}, err
	}
	var raw json.RawMessage
	if err := a.Client.Call(ctx, "eth_getBlockByNumber", []any{tag, false}, &raw); err != nil {
		return domain.BlockRef{}, err
	}
	header, err := VerifyChainHeaderJSON(ctx, a.Chain.Key, raw)
	if err != nil {
		return domain.BlockRef{}, err
	}
	ref, err := blockReference(a.Chain.Key, header.Block)
	if err != nil {
		return domain.BlockRef{}, err
	}
	if ref.Number != height {
		return domain.BlockRef{}, errors.New("BLOCK_INTEGRITY_HEIGHT")
	}
	return ref, nil
}
