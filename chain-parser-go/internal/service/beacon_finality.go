package service

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/provenance"
)

type beaconOwnerKey struct{}
type beaconOwnerView struct {
	proof                       *provenance.VerifiedExecutionFinality
	checkpointAge, finalizedAge uint64
	hashes                      map[uint64]string
}

func (s *Service) beaconContext(ctx context.Context, ch config.Chain) (context.Context, *beaconOwnerView, error) {
	policy := ch.BeaconFinality
	if policy == nil {
		return ctx, nil, nil
	}
	if err := ctx.Err(); err != nil {
		return ctx, nil, err
	}
	proof, err := provenance.ReadOnlineBeaconProof(s.cfg.Resolve(policy.ProofFile), policy.CheckpointRoot)
	if err != nil {
		return ctx, nil, err
	}
	view := &beaconOwnerView{proof: proof, checkpointAge: policy.MaxCheckpointAgeMS, finalizedAge: s.cfg.Config.Startup.MaxAge}
	if err := view.fresh(); err != nil {
		return ctx, nil, err
	}
	height, hash := proof.ExecutionBlock()
	view.hashes = map[uint64]string{height: hash}
	floor := s.stores[ch.Key].DurableRef().Block
	if floor.ChainKey != "" && (floor.Number > height || floor.Number == height && !strings.EqualFold(hash, floor.Hash)) {
		return ctx, nil, errors.New("BEACON_FINALITY_DURABLE_CONFLICT")
	}
	return context.WithValue(ctx, beaconOwnerKey{}, view), view, ctx.Err()
}
func (v *beaconOwnerView) fresh() error {
	return v.proof.CheckOnlineAge(time.Now(), v.checkpointAge, v.finalizedAge)
}
func (v *beaconOwnerView) check(block domain.BlockRef) error {
	if block.ChainKey != "evm:1" || !strings.EqualFold(v.hashes[block.Number], block.Hash) || block.Hash == "" {
		return errors.New("BEACON_FINALITY_BLOCK_CONFLICT")
	}
	return v.fresh()
}

// Walk backwards through verified execution headers before any short-tail
// publication. This binds every intermediate height to the signed head hash.
func (v *beaconOwnerView) admitTail(ctx context.Context, head, base domain.BlockRef, read func(context.Context, string) (domain.BlockRef, error)) error {
	if err := v.check(head); err != nil {
		return err
	}
	for head.Number > base.Number {
		parent := head.Number - 1
		if parent == base.Number {
			if !strings.EqualFold(head.ParentHash, base.Hash) {
				return errors.New("BEACON_FINALITY_PARENT_CONFLICT")
			}
			break
		}
		prev, err := read(ctx, fmt.Sprintf("0x%x", parent))
		if err != nil {
			return err
		}
		if prev.Number != parent || prev.ChainKey != head.ChainKey || !strings.EqualFold(head.ParentHash, prev.Hash) {
			return errors.New("BEACON_FINALITY_PARENT_CONFLICT")
		}
		v.hashes[parent] = prev.Hash
		head = prev
	}
	return ctx.Err()
}
