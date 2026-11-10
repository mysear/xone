package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
)

type writerAuthority interface {
	Renew(context.Context, control.Lease, uint64) (control.Lease, error)
	Check(context.Context, control.Lease, uint64) error
	AllocateBaseline(context.Context, control.Lease, domain.SnapshotRef) (control.Lease, error)
	Committed(context.Context, control.Lease, domain.SnapshotRef) error
	Release(context.Context, control.Lease) error
}
type writerSession struct {
	pg              writerAuthority
	lease           control.Lease
	kafka           *journal.Kafka
	store           *reducer.Store
	cfg             *config.Snapshot
	chain           config.Chain
	floor           domain.SnapshotRef
	takeover        bool
	reorgUnresolved string
	renewed         time.Time
}

func (s *Service) openSession(ctx context.Context, ch config.Chain) (*writerSession, error) {
	pg := s.control
	timeout, cancel := context.WithTimeout(ctx, time.Duration(s.cfg.Config.Control.RenewalMS)*time.Millisecond)
	defer cancel()
	if e := pg.Project(timeout, ch.Key, ch.Owner, ch.Assignment, s.cfg.PublicHash, s.cfg.Release); e != nil {
		return nil, e
	}
	acquireStarted := time.Now()
	lease, e := pg.Acquire(timeout, ch.Key, ch.Owner, ch.Assignment, s.cfg.Config.Control.InstanceID, s.cfg.Config.Control.LeaseTTLMS)
	if e != nil {
		return nil, e
	}
	kcfg := s.cfg.Config.Journal.Kafka
	kctx, kcancel := context.WithTimeout(ctx, time.Duration(kcfg.TransactionTimeoutMS)*time.Millisecond)
	defer kcancel()
	producer, e := journal.OpenKafka(kctx, kcfg.Brokers, kcfg.Topics[ch.Key], fmt.Sprintf("xone.canonical.%s", ch.Key), kcfg.ChunkBytes, s.cfg.Config.Journal.MaxBytes, kcfg.MaxTailRecords, time.Duration(kcfg.TransactionTimeoutMS)*time.Millisecond)
	if e != nil {
		_ = pg.Release(timeout, lease)
		return nil, e
	}
	session := &writerSession{pg: pg, lease: lease, kafka: producer, store: s.stores[ch.Key], cfg: s.cfg, chain: ch, takeover: true, renewed: acquireStarted}
	latest, cursor, e := producer.Tail(kctx)
	if e != nil {
		producer.Close()
		_ = pg.Release(timeout, lease)
		return nil, e
	}
	session.store.SetAuthorityDeadline(acquireStarted.Add(time.Duration(s.cfg.Config.Control.LeaseTTLMS) * time.Millisecond))
	session.floor = session.store.DurableRef()
	if latest != nil {
		ref := latest.Snapshot.Reference
		if ref.OwnerService != ch.Owner || ref.OwnerAssignmentVersion != ch.Assignment {
			producer.Close()
			return nil, errors.New("JOURNAL_OWNER_MISMATCH")
		}
		session.floor = ref
		if latest.Kind == "REORG_BEGIN" || latest.Kind == "ONLINE_SCOPE_REVOKED" {
			session.reorgUnresolved = latest.Kind
		}
	}
	pending, e := session.store.Prepared()
	if e != nil {
		producer.Close()
		return nil, e
	}
	if pending != nil {
		if latest != nil && pending.Snapshot.Reference == latest.Snapshot.Reference {
			a, ae := pending.CanonicalHash()
			b, be := latest.CanonicalHash()
			if ae != nil || be != nil || a != b {
				producer.Close()
				return nil, errors.New("PREPARED_JOURNAL_CONFLICT")
			}
			if e = session.store.ApplyPrepared(cursor); e != nil {
				producer.Close()
				return nil, e
			}
		} else if e = session.store.DiscardPrepared(); e != nil {
			producer.Close()
			return nil, e
		}
		session.store.Suspend()
	}
	return session, nil
}
func (w *writerSession) baseline(ctx context.Context, snapshot *domain.Snapshot, fork bool) error {
	floor := w.floor
	local := w.store.DurableRef()
	if local.JournalSeq > floor.JournalSeq {
		floor = local
	}
	if snapshot.Reference.CanonicalEpoch > floor.CanonicalEpoch {
		floor.CanonicalEpoch = snapshot.Reference.CanonicalEpoch
	}
	if fork {
		floor.CanonicalEpoch++
	}
	if !w.takeover {
		floor.WriterEpoch = 0
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(w.cfg.Config.Control.RenewalMS)*time.Millisecond)
	defer cancel()
	next, e := w.pg.AllocateBaseline(ctx, w.lease, floor)
	if e != nil {
		return e
	}
	w.lease = next
	w.takeover = false
	snapshot.Reference.WriterEpoch = next.WriterEpoch
	snapshot.Reference.OnlineGeneration = next.Generation
	snapshot.Reference.CanonicalEpoch = next.CanonicalEpoch
	snapshot.Reference.JournalSeq = next.JournalSeq + 1
	return nil
}
func (w *writerSession) heartbeat(ctx context.Context) error {
	if time.Since(w.renewed) < time.Duration(w.cfg.Config.Control.RenewalMS)*time.Millisecond {
		return nil
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(w.cfg.Config.Control.RenewalMS)*time.Millisecond)
	defer cancel()
	renewStarted := time.Now()
	next, e := w.pg.Renew(ctx, w.lease, w.cfg.Config.Control.LeaseTTLMS)
	if e != nil {
		return e
	}
	w.lease = next
	w.renewed = renewStarted
	w.store.SetAuthorityDeadline(renewStarted.Add(time.Duration(w.cfg.Config.Control.LeaseTTLMS) * time.Millisecond))
	return nil
}
func (w *writerSession) publish(ctx context.Context, unit journal.Unit) error {
	return w.publishOwner(ctx, unit, nil, nil)
}
func (w *writerSession) publishScope(ctx context.Context, unit journal.Unit, ledger []byte, job string, fence []byte, gate func() error) error {
	return w.publishOwner(ctx, unit, func() error { return w.store.PrepareScope(unit, ledger, job, fence, gate) }, gate)
}
func (w *writerSession) publishScopeCAS(ctx context.Context, unit journal.Unit, ledger []byte, job string, fence []byte, expected string, gate func() error) error {
	return w.publishOwner(ctx, unit, func() error { return w.store.PrepareScopeCAS(unit, ledger, job, fence, expected, gate) }, gate)
}
func (w *writerSession) publishScopeRecordsCAS(ctx context.Context, unit journal.Unit, ledger []byte, job string, fence []byte, expected string, records map[string]json.RawMessage, gate func() error) error {
	return w.publishOwner(ctx, unit, func() error { return w.store.PrepareScopeRecordsCAS(unit, ledger, job, fence, expected, records, gate) }, gate)
}
func (w *writerSession) publishOwnerRecords(ctx context.Context, unit journal.Unit, records map[string]json.RawMessage, gate func() error) error {
	return w.publishOwner(ctx, unit, func() error { return w.store.PrepareOwnerRecords(unit, records, gate) }, gate)
}
func (w *writerSession) publishOwner(ctx context.Context, unit journal.Unit, prepare func() error, gate func() error) error {
	if e := w.heartbeat(ctx); e != nil {
		return e
	}
	if prepare == nil {
		prepare = func() error { return w.store.PrepareGuard(unit, gate) }
	}
	if e := prepare(); e != nil {
		return e
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(w.cfg.Config.Journal.Kafka.TransactionTimeoutMS)*time.Millisecond)
	defer cancel()
	cursor, e := w.kafka.Publish(ctx, unit, func(ctx context.Context) error { return w.pg.Check(ctx, w.lease, w.cfg.Config.Control.MinCommitMS) })
	if e != nil {
		return e
	}
	if e = w.pg.Check(ctx, w.lease, 0); e != nil {
		return e
	}
	if e = w.pg.Committed(ctx, w.lease, unit.Snapshot.Reference); e != nil {
		return e
	}
	if e = w.store.ApplyPreparedScope(cursor, gate); e != nil {
		return e
	}
	w.observeOwnerCommit(unit)
	return nil
}
func (w *writerSession) observeOwnerCommit(unit journal.Unit) {
	w.floor = unit.Snapshot.Reference
	// PG.Committed has already accepted this projection and the matching
	// prepared owner batch is applied. Preserve the observed lease deadline;
	// local catalogue gates must not wait for a later periodic PG renewal.
	w.lease.Generation = max(w.lease.Generation, w.floor.OnlineGeneration)
	w.lease.CanonicalEpoch = max(w.lease.CanonicalEpoch, w.floor.CanonicalEpoch)
	w.lease.JournalSeq = max(w.lease.JournalSeq, w.floor.JournalSeq)
	w.reorgUnresolved = ""
	if unit.Kind == "REORG_BEGIN" || unit.Kind == "ONLINE_SCOPE_REVOKED" {
		w.reorgUnresolved = unit.Kind
	}
}
func (w *writerSession) close() {
	w.kafka.Close()
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_ = w.pg.Release(ctx, w.lease)
	w.store.Suspend()
}

// Each complete block read gets a fresh, bounded source window. Preserve the
// configured commit reserve so a failed read can publish ONLINE_SCOPE_REVOKED
// while the last admitted bank is still authoritative. A long batch renews
// between reads; no goroutine may race updates to the writer lease.
func readCompleteWithLease(ctx context.Context, reader stateBlockReader, w *writerSession, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	if w == nil {
		return reader.ReadComplete(ctx, tag)
	}
	if e := w.heartbeat(ctx); e != nil {
		return domain.BlockRef{}, nil, nil, e
	}
	deadline := w.renewed.Add(time.Duration(w.cfg.Config.Control.LeaseTTLMS-w.cfg.Config.Control.MinCommitMS) * time.Millisecond)
	source, cancel := context.WithDeadline(ctx, deadline)
	block, pools, swaps, e := reader.ReadComplete(source, tag)
	cancel()
	if e != nil {
		return domain.BlockRef{}, nil, nil, e
	}
	if e = w.heartbeat(ctx); e != nil {
		return domain.BlockRef{}, nil, nil, e
	}
	return block, pools, swaps, nil
}

// A failed complete read may have exhausted its source window. The fork
// diagnostic must use that same window, leaving the reserve for revocation.
// Reanchoring can confirm the fork later under a fresh writer admission.
func readHeaderWithLease(ctx context.Context, reader interface {
	Header(context.Context, string) (domain.BlockRef, error)
}, w *writerSession, tag string) (domain.BlockRef, error) {
	if w == nil {
		return reader.Header(ctx, tag)
	}
	deadline := w.renewed.Add(time.Duration(w.cfg.Config.Control.LeaseTTLMS-w.cfg.Config.Control.MinCommitMS) * time.Millisecond)
	probe, cancel := context.WithDeadline(ctx, deadline)
	defer cancel()
	return reader.Header(probe, tag)
}
