package service

import (
	"context"
	"errors"
	"time"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
)

// PublishStaged is called by staging's Commit under the existing chain actor,
// after wrapping each window's reader with ProtectReader. Unit, owner records,
// current state and journal/cursor use the same reducer prepared/apply batches.
// Never release acquisition pins on a mere source read or cursor comparison.
func (m *RawMaintenance) PublishStaged(ctx context.Context, unit journal.Unit, selection *RuntimeScopeSelection) error {
	key := unit.Snapshot.Reference.Block.ChainKey
	store := m.s.stores[key]
	current, ok := store.Current()
	if !ok || unit.Kind != "BLOCK_COMMIT" || unit.Snapshot.Reference.Block.Number != current.Reference.Block.Number+1 || unit.Snapshot.Reference.Block.ParentHash != current.Reference.Block.Hash {
		return ingest.ErrReplayFence
	}
	records, e := m.OwnerRecordsForUnit(unit)
	if e != nil {
		return e
	}
	if len(records) == 0 {
		return errors.New("RAW_STAGING_ACQUISITION_REQUIRED")
	}
	writer := m.s.sessions[key]
	if m.s.cfg.Config.Journal.Transport == "kafka" && writer == nil {
		return errors.New("RAW_ACTIVE_LEASE_REQUIRED")
	}
	runtimeGate := m.s.runtimeScopeGate(ctx, selection, unit.Snapshot)
	gate := func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		ref := unit.Snapshot.Reference
		if m.s.sessions[key] != writer || ref.WriterEpoch != current.Reference.WriterEpoch || ref.OnlineGeneration != current.Reference.OnlineGeneration || ref.OwnerService != current.Reference.OwnerService || ref.OwnerAssignmentVersion != current.Reference.OwnerAssignmentVersion {
			return ingest.ErrReplayFence
		}
		if writer != nil {
			lease := writer.lease
			if lease.Chain != key || lease.Holder != m.s.cfg.Config.Control.InstanceID || lease.CanonicalEpoch != ref.CanonicalEpoch || lease.WriterEpoch != ref.WriterEpoch || lease.Generation != ref.OnlineGeneration || uint64(time.Now().UnixMilli())+m.s.cfg.Config.Control.MinCommitMS >= lease.ExpiresMS {
				return ingest.ErrReplayFence
			}
		}
		return runtimeGate()
	}
	if writer != nil {
		e = writer.publishOwnerRecords(ctx, unit, records, gate)
	} else {
		e = store.ApplyLocalOwnerRecords(unit, records, gate)
	}
	if e != nil {
		return e
	}
	return m.AcknowledgeOwnerRecords(unit, records)
}
