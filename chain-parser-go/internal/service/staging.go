package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
)

type stateBlockReader interface {
	ReadComplete(context.Context, string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error)
}
type stagedState struct {
	Block domain.BlockRef   `json:"block"`
	Pools []domain.Pool     `json:"pools"`
	Swaps []domain.SwapFact `json:"swaps"`
}

// syncStaged runs under the chain actor. Source reads run concurrently; no
// worker touches lease/session state. Each two-block window gets one immutable
// source deadline with the existing commit/revocation reserve. Commits remain
// the existing prepared+journal transaction and renew the authority in order.
func (s *Service) syncStaged(ctx context.Context, ch config.Chain, current domain.Snapshot, target uint64, reader stateBlockReader) error {
	return s.syncStagedRuntime(ctx, ch, current, target, reader, nil)
}
func (s *Service) syncStagedRuntime(ctx context.Context, ch config.Chain, current domain.Snapshot, target uint64, reader stateBlockReader, selection *RuntimeScopeSelection) error {
	tracker := s.rawSources[ch.Key]
	if tracker == nil {
		return errors.New("STAGING_RAW_ARCHIVE_REQUIRED")
	}
	for current.Reference.Block.Number < target {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		source := ctx
		cancel := func() {}
		if writer := s.sessions[ch.Key]; writer != nil {
			if e := writer.heartbeat(ctx); e != nil {
				return e
			}
			source, cancel = context.WithDeadline(ctx, writer.renewed.Add(time.Duration(writer.cfg.Config.Control.LeaseTTLMS-writer.cfg.Config.Control.MinCommitMS)*time.Millisecond))
		}
		end := current.Reference.Block.Number + min(uint64(2), target-current.Reference.Block.Number)
		payload := int64(s.cfg.Config.Journal.MaxBytes)
		base := current
		windowReader := reader
		if s.rawMaintenance != nil {
			windowReader = s.rawMaintenance.ProtectReader(ch.Key, base.Reference, reader)
		}
		pipe := &ingest.Pipeline{Chain: ch.Key, Archive: tracker.archive, Limits: ingest.Limits{Workers: 2, MaxTasks: 2, MaxBlockBytes: payload * 2, MaxBytes: payload * 4},
			Fetch: func(source context.Context, height uint64, limit int64) (ingest.Fetched, error) {
				block, pools, swaps, e := windowReader.ReadComplete(source, fmt.Sprintf("0x%x", height))
				if e != nil {
					return ingest.Fetched{}, e
				}
				raw, e := json.Marshal(stagedState{block, pools, swaps})
				if e != nil {
					return ingest.Fetched{}, errors.New("STAGING_ENCODING")
				}
				if int64(len(raw))*2 > limit {
					return ingest.Fetched{}, ingest.ErrBudget
				}
				return ingest.Fetched{Height: block.Number, Hash: block.Hash, ParentHash: block.ParentHash, ProviderID: "derived-state-from-recorded-rpc", Raw: raw}, nil
			},
			Decode: func(_ context.Context, fetched ingest.Fetched) ([]byte, error) { return fetched.Raw, nil },
			Validate: func(ctx context.Context, fetched ingest.Fetched, raw []byte) error {
				var data stagedState
				if json.Unmarshal(raw, &data) != nil || data.Block.ChainKey != ch.Key || data.Block.Number != fetched.Height || data.Block.Hash != fetched.Hash || data.Block.ParentHash != fetched.ParentHash {
					return errors.New("STAGING_IDENTITY")
				}
				candidate := base
				candidate.Reference.Block = data.Block
				candidate.Pools = data.Pools
				if e := candidate.Validate(ch.Owner, ch.Assignment); e != nil {
					return e
				}
				return bootstrap.CheckFresh(candidate, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge)
			},
			Commit: func(ctx context.Context, expected ingest.Cursor, block ingest.Block) error {
				actual, ok := s.stores[ch.Key].Current()
				if !ok || actual.Reference != current.Reference || actual.Reference.Block.Number != expected.Height || actual.Reference.Block.Hash != expected.Hash {
					return errors.New("STAGING_COMMIT_FENCED")
				}
				var data stagedState
				if json.Unmarshal(block.Decoded, &data) != nil {
					return errors.New("STAGING_ENCODING")
				}
				next := current
				next.Reference.Block = data.Block
				next.Reference.JournalSeq++
				next.Pools = data.Pools
				unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next}
				if hasV2Scope(next.Pools) {
					unit.NormalizedFacts = &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: data.Block, Swaps: data.Swaps}}}
				}
				if s.rawMaintenance != nil {
					if e := s.rawMaintenance.PublishStaged(ctx, unit, selection); e != nil {
						return e
					}
				} else if writer := s.sessions[ch.Key]; writer != nil {
					if e := writer.publishOwner(ctx, unit, nil, s.runtimeScopeGate(ctx, selection, next)); e != nil {
						return e
					}
				} else if e := s.stores[ch.Key].ApplyLocalGuard(unit, s.runtimeScopeGate(ctx, selection, next)); e != nil {
					return e
				}
				current = next
				return nil
			},
		}
		_, e := pipe.Run(source, ingest.Cursor{Height: current.Reference.Block.Number, Hash: current.Reference.Block.Hash}, end)
		cancel()
		if e != nil {
			return e
		}
	}
	return nil
}
