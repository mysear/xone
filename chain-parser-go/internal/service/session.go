package service

import (
	"context"
	"errors"
	"fmt"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
)

type writerSession struct {
	pg              *control.Postgres
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
	if e := w.store.Prepare(unit); e != nil {
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
	if e = w.store.ApplyPrepared(cursor); e != nil {
		return e
	}
	w.floor = unit.Snapshot.Reference
	w.reorgUnresolved = ""
	if unit.Kind == "REORG_BEGIN" || unit.Kind == "ONLINE_SCOPE_REVOKED" {
		w.reorgUnresolved = unit.Kind
	}
	return nil
}
func (w *writerSession) close() {
	w.kafka.Close()
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_ = w.pg.Release(ctx, w.lease)
	w.store.Suspend()
}
