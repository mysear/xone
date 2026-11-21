package service

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
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
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := v.check(head); err != nil {
		return err
	}
	if base.ChainKey != head.ChainKey || base.Number > head.Number || base.Hash == "" {
		return errors.New("BEACON_FINALITY_PARENT_CONFLICT")
	}
	var ancestors []domain.BlockRef
	if head.Number > base.Number {
		var err error
		ancestors, err = prefetchBeaconAncestors(ctx, head.Number, head.Number-base.Number-1, read)
		if err != nil {
			return err
		}
		for _, prev := range ancestors {
			if prev.Number != head.Number-1 || prev.ChainKey != head.ChainKey || prev.Hash == "" || !strings.EqualFold(head.ParentHash, prev.Hash) {
				return errors.New("BEACON_FINALITY_PARENT_CONFLICT")
			}
			head = prev
		}
		if !strings.EqualFold(head.ParentHash, base.Hash) {
			return errors.New("BEACON_FINALITY_PARENT_CONFLICT")
		}
	} else if !strings.EqualFold(head.Hash, base.Hash) {
		return errors.New("BEACON_FINALITY_PARENT_CONFLICT")
	}
	// Keep the signed head and previously admitted hashes unchanged on every
	// failed read, ancestry check, cancellation or freshness check.
	hashes := make(map[uint64]string, len(v.hashes)+len(ancestors))
	for height, hash := range v.hashes {
		hashes[height] = hash
	}
	for _, ancestor := range ancestors {
		hashes[ancestor.Number] = ancestor.Hash
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := v.fresh(); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	v.hashes = hashes
	return nil
}

// Fetch independently verified headers concurrently, retaining their requested
// descending order. Admission still follows the signed head's parent links.
func prefetchBeaconAncestors(ctx context.Context, head, count uint64, read func(context.Context, string) (domain.BlockRef, error)) ([]domain.BlockRef, error) {
	if count == 0 {
		return nil, ctx.Err()
	}
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	ancestors := make([]domain.BlockRef, count)
	jobs := make(chan int)
	var workers sync.WaitGroup
	var failed sync.Once
	var firstErr error
	for range min(4, len(ancestors)) {
		workers.Add(1)
		go func() {
			defer workers.Done()
			for index := range jobs {
				if ctx.Err() != nil {
					return
				}
				block, err := read(ctx, fmt.Sprintf("0x%x", head-uint64(index)-1))
				if err != nil {
					failed.Do(func() {
						firstErr = err
						cancel()
					})
					return
				}
				ancestors[index] = block
			}
		}()
	}
dispatch:
	for index := range ancestors {
		select {
		case jobs <- index:
		case <-ctx.Done():
			break dispatch
		}
	}
	close(jobs)
	workers.Wait()
	if firstErr != nil {
		return nil, firstErr
	}
	return ancestors, ctx.Err()
}
