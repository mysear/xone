package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

type forkReader interface {
	ReadState(context.Context, string, bool) (domain.BlockRef, []domain.Pool, error)
	ReadComplete(context.Context, string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error)
	ConfirmBlock(context.Context, domain.BlockRef) error
}

// recoverFork publishes a revocation barrier before doing bounded rollback and
// replacement reads. Intermediate branch states never become Current/Bootstrap.
func (s *Service) recoverFork(ctx context.Context, ch config.Chain, old domain.Snapshot, a forkReader) error {
	if err := s.revokeRuntimeSources(ctx, ch, []string{old.Reference.Block.Hash}); err != nil {
		return err
	}
	store := s.stores[ch.Key]
	store.Suspend()
	if e := s.revokeScope(ctx, ch, old); e != nil {
		return e
	}
	revoked := old
	revoked.Reference.JournalSeq++
	publish := func(u journal.Unit) error {
		if w := s.sessions[ch.Key]; w != nil {
			return w.publish(ctx, u)
		}
		return store.ApplyLocal(u)
	}
	// The total recovery budget is bounded as well as each RPC. Expiration leaves
	// the durable barrier in place for qualified HEAD_FIRST recovery.
	sidecarContext := ctx
	ctx, cancel := context.WithTimeout(ctx, time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
	defer cancel()
	endCapture := func() {}
	if s.rawMaintenance != nil {
		captured, finish, err := s.rawMaintenance.BeginOwnerCapture(ctx, ch.Key, "reorg")
		if err != nil {
			return err
		}
		ctx, endCapture = captured, finish
		defer finish()
	}
	selected := ch
	if adapter, ok := a.(*evm.Adapter); ok {
		// Validate the actual reader's identities. A root static config omits
		// discovered pools; treating that root as the replacement scope could
		// bypass requalification after the catalogue revocation above.
		selected = adapter.Chain
	}
	selection, e := s.freezeRuntimeScope(ctx, selected)
	if e != nil {
		return e
	}
	head, _, e := a.ReadState(ctx, ch.Confirmation, false)
	if e != nil {
		return e
	}
	if e = a.ConfirmBlock(ctx, head); e != nil {
		return e
	}
	ancestor := old
	reverted := []domain.BlockRef{}
	for depth := uint64(0); ; depth++ {
		if ctx.Err() != nil {
			return errors.New("REORG_DEADLINE")
		}
		if w := s.sessions[ch.Key]; w != nil {
			if e = w.heartbeat(ctx); e != nil {
				return e
			}
		}
		if ancestor.Reference.Block.Number <= head.Number {
			block, pools, err := a.ReadState(ctx, fmt.Sprintf("0x%x", ancestor.Reference.Block.Number), false)
			if err != nil {
				return err
			}
			if block.Hash == ancestor.Reference.Block.Hash {
				before, _ := domain.Hash(ancestor.Pools)
				after, _ := domain.Hash(pools)
				if block != ancestor.Reference.Block || before != after {
					return errors.New("REORG_ANCESTOR_CONTENT_CONFLICT")
				}
				break
			}
		}
		if depth >= ch.ReorgWindow || ancestor.Reference.Block.Number == 0 {
			return errors.New("DEEP_REORG_REQUIRES_CURRENT_BASELINE")
		}
		reverted = append(reverted, ancestor.Reference.Block)
		ancestor, e = store.UndoParent(ancestor)
		if e != nil {
			return e
		}
	}
	if head.Number-ancestor.Reference.Block.Number > ch.ReorgWindow || head.Number-ancestor.Reference.Block.Number > s.cfg.Config.Startup.MaxTail {
		return errors.New("REORG_REPLACEMENT_LIMIT")
	}
	begin := revoked
	begin.Reference.CanonicalEpoch++
	begin.Reference.JournalSeq++
	if begin.Reference.CanonicalEpoch == 0 || begin.Reference.JournalSeq == 0 {
		return errors.New("REORG_VERSION_OVERFLOW")
	}
	if e := publish(journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin}); e != nil {
		return e
	}
	s.log.Event("warn", "reorg_begin", map[string]any{"chain_key": ch.Key, "canonical_epoch": begin.Reference.CanonicalEpoch, "journal_seq": begin.Reference.JournalSeq})
	block := ancestor.Reference.Block
	ancestor.Reference = begin.Reference
	ancestor.Reference.JournalSeq++
	ancestor.Reference.Block = block
	branch := []domain.Snapshot{ancestor}
	factBlocks := []domain.FactBlock{}
	raw, e := json.Marshal(ancestor)
	if e != nil {
		return e
	}
	size := len(raw) + 2
	for h := block.Number + 1; h <= head.Number; h++ {
		if ctx.Err() != nil {
			return errors.New("REORG_DEADLINE")
		}
		nextBlock, pools, swaps, err := readCompleteWithLease(ctx, a, s.sessions[ch.Key], fmt.Sprintf("0x%x", h))
		if err != nil {
			return err
		}
		previous := branch[len(branch)-1]
		if nextBlock.Number != h || nextBlock.ParentHash != previous.Reference.Block.Hash {
			return errors.New("REORG_BRANCH_CONTINUITY")
		}
		next := previous
		next.Reference.Block = nextBlock
		next.Pools = pools
		if e = next.Validate(ch.Owner, ch.Assignment); e != nil {
			return e
		}
		raw, e = json.Marshal(next)
		if e != nil {
			return e
		}
		size += len(raw) + 1
		if size > s.cfg.Config.Journal.MaxBytes {
			return errors.New("REORG_STAGE_LIMIT")
		}
		factBlocks = append(factBlocks, domain.FactBlock{Block: nextBlock, Swaps: swaps})
		branch = append(branch, next)
	}
	next := branch[len(branch)-1]
	if next.Reference.Block != head {
		return errors.New("REORG_HEAD_CHANGED")
	}
	if e = a.ConfirmBlock(ctx, head); e != nil {
		return e
	}
	if e = bootstrap.CheckFresh(next, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge); e != nil {
		return e
	}
	if e = store.StageReorg(branch, s.cfg.Config.Journal.MaxBytes); e != nil {
		return e
	}
	unit := journal.Unit{Kind: "REORG_COMMIT", Snapshot: next}
	if hasV2Scope(next.Pools) {
		unit.NormalizedFacts = &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: factBlocks, RevertedBlocks: reverted}
	}
	gate := s.runtimeScopeGate(ctx, selection, next)
	var records map[string]json.RawMessage
	if s.rawMaintenance != nil {
		records, e = s.rawMaintenance.OwnerUnitRecords(ctx, unit)
		if e != nil {
			return e
		}
	}
	if w := s.sessions[ch.Key]; w != nil {
		if len(records) > 0 {
			e = w.publishOwnerRecords(ctx, unit, records, gate)
		} else {
			e = w.publishOwner(ctx, unit, nil, gate)
		}
	} else if len(records) > 0 {
		e = store.ApplyLocalOwnerRecords(unit, records, gate)
	} else {
		e = store.ApplyLocalGuard(unit, gate)
	}
	if e != nil {
		return e
	}
	if s.rawMaintenance != nil && len(records) > 0 {
		if e = s.rawMaintenance.AcknowledgeOwnerRecords(unit, records); e != nil {
			return e
		}
	}
	endCapture()
	s.recordHistoryBaseline(sidecarContext, ch, next)
	s.log.Event("info", "reorg_committed", map[string]any{"chain_key": ch.Key, "canonical_epoch": next.Reference.CanonicalEpoch, "journal_seq": next.Reference.JournalSeq, "ancestor_height": block.Number, "replacement_block_count": len(branch) - 1})
	return nil
}

func (s *Service) revokeScope(ctx context.Context, ch config.Chain, old domain.Snapshot) error {
	revoked := old
	revoked.Reference.JournalSeq++
	if revoked.Reference.JournalSeq == 0 {
		return errors.New("REORG_VERSION_OVERFLOW")
	}
	unit := journal.Unit{Kind: "ONLINE_SCOPE_REVOKED", Snapshot: revoked}
	if w := s.sessions[ch.Key]; w != nil {
		return w.publish(ctx, unit)
	}
	return s.stores[ch.Key].ApplyLocal(unit)
}
