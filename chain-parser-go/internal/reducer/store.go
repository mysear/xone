// Package reducer owns the online namespace. History has a separate write path.
package reducer

import (
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"sync"
	"time"

	"github.com/cockroachdb/pebble"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

type Store struct {
	closed            bool
	failed            bool
	commitFault       func(*pebble.Batch) error // test-only ambiguous I/O injection
	mu                sync.RWMutex
	db                *pebble.DB
	owner             string
	assignment        uint64
	window            uint64
	current           *domain.Snapshot
	reorg             *domain.Snapshot
	reorgKind         string
	durable           domain.SnapshotRef
	cursor            *journal.Cursor
	authorityDeadline time.Time
}

func Open(path, owner string, assignment, window uint64) (*Store, error) {
	return OpenWithLogger(path, owner, assignment, window, nil)
}
func OpenWithLogger(path, owner string, assignment, window uint64, logger pebble.Logger) (*Store, error) {
	db, e := pebble.Open(filepath.Clean(path), &pebble.Options{Logger: logger})
	if e != nil {
		return nil, e
	}
	s := &Store{db: db, owner: owner, assignment: assignment, window: window}
	b, closer, e := db.Get([]byte("online/current"))
	if e == nil {
		defer closer.Close()
		var previous domain.Snapshot
		if e = json.Unmarshal(b, &previous); e != nil {
			db.Close()
			return nil, e
		}
		s.durable = previous.Reference
	} else if !errors.Is(e, pebble.ErrNotFound) {
		db.Close()
		return nil, e
	}
	if b, c, e := db.Get([]byte("online/cursor")); e == nil {
		var cursor journal.Cursor
		if e = json.Unmarshal(b, &cursor); e != nil {
			c.Close()
			db.Close()
			return nil, e
		}
		c.Close()
		s.cursor = &cursor
	} else if !errors.Is(e, pebble.ErrNotFound) {
		db.Close()
		return nil, e
	}
	if b, c, e := db.Get([]byte("online/reorg")); e == nil {
		var pending domain.Snapshot
		e = json.Unmarshal(b, &pending)
		c.Close()
		if e != nil || pending.Reference != s.durable {
			db.Close()
			return nil, errors.New("REORG_STATE_CORRUPT")
		}
		s.reorg = &pending
		kind, closer, err := db.Get([]byte("online/reorg-kind"))
		if err != nil {
			db.Close()
			return nil, errors.New("REORG_STATE_CORRUPT")
		}
		s.reorgKind = string(kind)
		closer.Close()
		if s.reorgKind != "REORG_BEGIN" && s.reorgKind != "ONLINE_SCOPE_REVOKED" {
			db.Close()
			return nil, errors.New("REORG_STATE_CORRUPT")
		}
	} else if !errors.Is(e, pebble.ErrNotFound) {
		db.Close()
		return nil, e
	}
	return s, nil
}
func (s *Store) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil
	}
	s.closed = true
	s.current = nil
	return s.db.Close()
}
func clone(snapshot domain.Snapshot) domain.Snapshot {
	snapshot.Pools = append([]domain.Pool(nil), snapshot.Pools...)
	for i := range snapshot.Pools {
		snapshot.Pools[i] = snapshot.Pools[i].Clone()
	}
	snapshot.HistoryGaps = append(make([]domain.Gap, 0, len(snapshot.HistoryGaps)), snapshot.HistoryGaps...)
	return snapshot
}
func (s *Store) Current() (domain.Snapshot, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed || s.failed || s.current == nil || !s.authorityDeadline.IsZero() && time.Now().After(s.authorityDeadline) {
		return domain.Snapshot{}, false
	}
	return clone(*s.current), true
}
func (s *Store) DurableRef() domain.SnapshotRef { s.mu.RLock(); defer s.mu.RUnlock(); return s.durable }
func (s *Store) Suspend()                       { s.mu.Lock(); defer s.mu.Unlock(); s.current = nil }
func (s *Store) InstallBaseline(snapshot domain.Snapshot) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if e := snapshot.Validate(s.owner, s.assignment); e != nil {
		return e
	}
	if snapshot.Reference.OnlineGeneration <= s.durable.OnlineGeneration || snapshot.Reference.WriterEpoch < s.durable.WriterEpoch || snapshot.Reference.JournalSeq <= s.durable.JournalSeq || snapshot.Reference.CanonicalEpoch < s.durable.CanonicalEpoch {
		return errors.New("STALE_BASELINE")
	}
	return s.persist(snapshot, "ONLINE_BASELINE_COMMIT", nil, nil)
}
func (s *Store) Commit(snapshot domain.Snapshot) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.current == nil {
		return errors.New("BASELINE_REQUIRED")
	}
	if e := snapshot.Validate(s.owner, s.assignment); e != nil {
		return e
	}
	r, old := snapshot.Reference, s.current.Reference
	if r.WriterEpoch < old.WriterEpoch {
		return errors.New("WRITER_FENCED")
	}
	if r.CanonicalEpoch != old.CanonicalEpoch || r.OnlineGeneration != old.OnlineGeneration {
		return errors.New("EPOCH_MISMATCH")
	}
	if r.JournalSeq == old.JournalSeq {
		a, _ := domain.Hash(snapshot)
		b, _ := domain.Hash(*s.current)
		if a == b {
			return nil
		}
		return errors.New("DUPLICATE_CONTENT_CONFLICT")
	}
	if r.JournalSeq != old.JournalSeq+1 || r.Block.Number != old.Block.Number+1 || r.Block.ParentHash != old.Block.Hash {
		return errors.New("NON_CONTIGUOUS_BLOCK")
	}
	if r.ScopeHash != old.ScopeHash {
		return errors.New("SCOPE_CHANGE_REQUIRES_BASELINE")
	}
	previous := clone(*s.current)
	return s.persist(snapshot, "BLOCK_COMMIT", &previous, nil)
}
func (s *Store) persist(snapshot domain.Snapshot, kind string, undo *domain.Snapshot, cursor *journal.Cursor) error {
	return s.persistScope(snapshot, kind, undo, cursor, nil, nil)
}
func (s *Store) persistScope(snapshot domain.Snapshot, kind string, undo *domain.Snapshot, cursor *journal.Cursor, scope *preparedScope, gate func() error) error {
	if s.closed || s.failed {
		return ErrOwnerReopenRequired
	}
	if scope == nil {
		var err error
		scope, err = s.scopePrepared()
		if err != nil {
			return err
		}
	}
	record, e := domain.NewRecord(snapshot, kind)
	if e != nil {
		return e
	}
	b, e := json.Marshal(snapshot)
	if e != nil {
		return e
	}
	raw, e := json.Marshal(record)
	if e != nil {
		return e
	}
	batch := s.db.NewBatch()
	defer batch.Close()
	if e = batch.Set([]byte("online/current"), b, nil); e != nil {
		return e
	}
	if e = batch.Set([]byte(fmt.Sprintf("journal/%020d", snapshot.Reference.JournalSeq)), raw, nil); e != nil {
		return e
	}
	if undo != nil {
		u, e := json.Marshal(undo)
		if e != nil {
			return e
		}
		if e = batch.Set([]byte("undo/"+undo.Reference.Block.Hash), u, nil); e != nil {
			return e
		}
	}
	if undo != nil || kind == "REORG_COMMIT" {
		if snapshot.Reference.Block.Number > s.window { // Bound undo retention by the configured number of blocks.
			iter, e := s.db.NewIter(&pebble.IterOptions{LowerBound: []byte("undo/"), UpperBound: []byte("undo0")})
			if e != nil {
				return e
			}
			for iter.First(); iter.Valid(); iter.Next() {
				var old domain.Snapshot
				if json.Unmarshal(iter.Value(), &old) == nil && old.Reference.Block.Number < snapshot.Reference.Block.Number-s.window {
					if e = batch.Delete(append([]byte(nil), iter.Key()...), nil); e != nil {
						iter.Close()
						return e
					}
				}
			}
			e = iter.Error()
			iter.Close()
			if e != nil {
				return e
			}
		}
	}
	if cursor != nil {
		b, e := json.Marshal(cursor)
		if e != nil {
			return e
		}
		if e = batch.Set([]byte("online/cursor"), b, nil); e != nil {
			return e
		}
		if e = batch.Delete([]byte("online/prepared"), nil); e != nil {
			return e
		}
	}
	if e = batch.Delete([]byte("online/prepared"), nil); e != nil {
		return e
	}
	if kind == "REORG_COMMIT" {
		if raw, c, err := s.db.Get([]byte("online/reorg-stage")); err == nil {
			var branch []domain.Snapshot
			err = json.Unmarshal(raw, &branch)
			c.Close()
			if err != nil {
				return err
			}
			if len(branch) == 0 {
				return errors.New("REORG_STAGE_EMPTY")
			}
			old := s.reorg
			if old == nil {
				return errors.New("REORG_BEGIN_REQUIRED")
			}
			// Remove only the replaced branch's exact hashes. Orphaned states
			// at the same height must not accumulate through repeated forks.
			for block := old.Reference.Block; block.Number > branch[0].Reference.Block.Number; {
				if e = batch.Delete([]byte("undo/"+block.Hash), nil); e != nil {
					return e
				}
				parent, closer, err := s.db.Get([]byte("undo/" + block.ParentHash))
				if err != nil {
					return errors.New("REORG_UNDO_UNAVAILABLE")
				}
				var previous domain.Snapshot
				err = json.Unmarshal(parent, &previous)
				closer.Close()
				if err != nil || previous.Reference.Block.Hash != block.ParentHash || previous.Reference.Block.Number >= block.Number || block.Number-previous.Reference.Block.Number != 1 {
					return errors.New("REORG_UNDO_IDENTITY")
				}
				block = previous.Reference.Block
			}
			for _, state := range branch {
				encoded, err := json.Marshal(state)
				if err != nil {
					return err
				}
				if e = batch.Set([]byte("undo/"+state.Reference.Block.Hash), encoded, nil); e != nil {
					return e
				}
			}
		} else if !errors.Is(err, pebble.ErrNotFound) {
			return err
		}
	}
	if kind == "ONLINE_BASELINE_COMMIT" {
		if e = batch.DeleteRange([]byte("undo/"), []byte("undo0"), nil); e != nil {
			return e
		}
	}
	if kind == "REORG_BEGIN" || kind == "ONLINE_SCOPE_REVOKED" || kind == "REORG_COMMIT" || kind == "ONLINE_BASELINE_COMMIT" {
		if e = batch.Delete([]byte("online/reorg-stage"), nil); e != nil {
			return e
		}
	}
	if kind == "REORG_BEGIN" || kind == "ONLINE_SCOPE_REVOKED" {
		if e = batch.Set([]byte("online/reorg-kind"), []byte(kind), nil); e != nil {
			return e
		}
		if e = batch.Set([]byte("online/reorg"), b, nil); e != nil {
			return e
		}
	} else if kind == "REORG_COMMIT" || kind == "ONLINE_BASELINE_COMMIT" {
		if e = batch.Delete([]byte("online/reorg-kind"), nil); e != nil {
			return e
		}
		if e = batch.Delete([]byte("online/reorg"), nil); e != nil {
			return e
		}
	}
	if e = s.applyScopeBatch(batch, snapshot, kind, scope); e != nil {
		return e
	}
	if e = s.applyOwnerRecordsBatch(batch, snapshot, kind); e != nil {
		return e
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	if e = s.commitOwnerBatch(batch); e != nil {
		return e
	}
	next := clone(snapshot)
	s.current = &next
	if kind == "REORG_BEGIN" || kind == "ONLINE_SCOPE_REVOKED" {
		s.reorgKind = kind
		s.reorg = &next
		s.current = nil
	} else {
		s.reorg = nil
		s.reorgKind = ""
	}
	s.durable = next.Reference
	if cursor != nil {
		copy := *cursor
		s.cursor = &copy
	}
	return nil
}
func (s *Store) RevertTo(hash string, newEpoch uint64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.current == nil {
		return errors.New("BASELINE_REQUIRED")
	}
	if newEpoch <= s.current.Reference.CanonicalEpoch {
		return errors.New("STALE_CANONICAL_EPOCH")
	}
	b, closer, e := s.db.Get([]byte("undo/" + hash))
	if e != nil {
		s.current = nil
		return errors.New("DEEP_REORG_REQUIRES_CURRENT_BASELINE")
	}
	defer closer.Close()
	var target domain.Snapshot
	if e = json.Unmarshal(b, &target); e != nil {
		return e
	}
	if s.current.Reference.Block.Number-target.Reference.Block.Number > s.window {
		s.current = nil
		return errors.New("DEEP_REORG_REQUIRES_CURRENT_BASELINE")
	}
	target.Reference.CanonicalEpoch = newEpoch
	target.Reference.OnlineGeneration = s.current.Reference.OnlineGeneration
	target.Reference.WriterEpoch = s.current.Reference.WriterEpoch
	target.Reference.JournalSeq = s.current.Reference.JournalSeq + 1
	return s.persist(target, "REORG_COMMIT", nil, nil)
}
func (s *Store) WriteHistory(snapshot domain.Snapshot) error {
	b, e := json.Marshal(snapshot)
	if e != nil {
		return e
	}
	return s.db.Set([]byte("history/"+snapshot.Reference.Block.Hash), b, pebble.Sync)
}
func (s *Store) Records(after uint64, limit int) ([]domain.Record, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed || s.failed {
		return nil, ErrOwnerReopenRequired
	}
	if limit < 1 || limit > 1000 {
		return nil, errors.New("INVALID_LIMIT")
	}
	iter, e := s.db.NewIter(&pebble.IterOptions{LowerBound: []byte(fmt.Sprintf("journal/%020d", after+1)), UpperBound: []byte("journal0")})
	if e != nil {
		return nil, e
	}
	defer iter.Close()
	records := make([]domain.Record, 0)
	for iter.First(); iter.Valid() && len(records) < limit; iter.Next() {
		var r domain.Record
		if e = json.Unmarshal(iter.Value(), &r); e != nil {
			return nil, e
		}
		records = append(records, r)
	}
	return records, iter.Error()
}

func (s *Store) Prepare(u journal.Unit) error {
	return s.prepare(u, nil, nil)
}
func (s *Store) prepare(u journal.Unit, scope *preparedScope, gate func() error) error {
	return s.prepareOwner(u, scope, nil, gate)
}
func (s *Store) prepareOwner(u journal.Unit, scope *preparedScope, records *ownerPreparedRecords, gate func() error) error {
	if e := u.ValidateFacts(); e != nil {
		return e
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.failed {
		return ErrOwnerReopenRequired
	}
	if e := s.checkScopeBase(scope); e != nil {
		return e
	}
	if e := u.Snapshot.Validate(s.owner, s.assignment); e != nil {
		return e
	}
	r := u.Snapshot.Reference
	if u.Kind == "ONLINE_BASELINE_COMMIT" {
		if r.OnlineGeneration <= s.durable.OnlineGeneration || r.WriterEpoch < s.durable.WriterEpoch || r.JournalSeq <= s.durable.JournalSeq || r.CanonicalEpoch < s.durable.CanonicalEpoch {
			return errors.New("STALE_BASELINE")
		}
	} else if u.Kind == "BLOCK_COMMIT" {
		if s.current == nil {
			return errors.New("BASELINE_REQUIRED")
		}
		old := s.current.Reference
		if r.JournalSeq != old.JournalSeq+1 || r.Block.Number != old.Block.Number+1 || r.Block.ParentHash != old.Block.Hash || r.OnlineGeneration != old.OnlineGeneration || r.CanonicalEpoch != old.CanonicalEpoch || r.WriterEpoch < old.WriterEpoch || r.ScopeHash != old.ScopeHash {
			return errors.New("NON_CONTIGUOUS_BLOCK")
		}
	} else if u.Kind == "REORG_BEGIN" || u.Kind == "ONLINE_SCOPE_REVOKED" {
		if s.reorg != nil && (s.reorgKind != "ONLINE_SCOPE_REVOKED" || u.Kind != "REORG_BEGIN") {
			return errors.New("REORG_ALREADY_PENDING")
		}
		b, c, e := s.db.Get([]byte("online/current"))
		if e != nil {
			return errors.New("BASELINE_REQUIRED")
		}
		var old domain.Snapshot
		e = json.Unmarshal(b, &old)
		c.Close()
		if e != nil {
			return e
		}
		expected := clone(old)
		if u.Kind == "REORG_BEGIN" {
			expected.Reference.CanonicalEpoch++
		}
		expected.Reference.JournalSeq++
		a, _ := domain.Hash(expected)
		h, _ := domain.Hash(u.Snapshot)
		if a != h || expected.Reference.CanonicalEpoch == 0 || expected.Reference.JournalSeq == 0 {
			return errors.New("REORG_BEGIN_MISMATCH")
		}
	} else if u.Kind == "REORG_COMMIT" {
		if s.reorg == nil || s.reorgKind != "REORG_BEGIN" {
			return errors.New("REORG_BEGIN_REQUIRED")
		}
		if raw, closer, err := s.db.Get([]byte("online/reorg-stage")); err == nil {
			var branch []domain.Snapshot
			err = json.Unmarshal(raw, &branch)
			closer.Close()
			if err != nil || len(branch) == 0 {
				return errors.New("REORG_STAGE_CORRUPT")
			}
			expected, _ := domain.Hash(branch[len(branch)-1])
			actual, _ := domain.Hash(u.Snapshot)
			if expected != actual {
				return errors.New("REORG_STAGE_COMMIT_CONFLICT")
			}
		} else if !errors.Is(err, pebble.ErrNotFound) {
			return err
		}
		old := s.reorg.Reference
		if r.JournalSeq != old.JournalSeq+1 || r.CanonicalEpoch != old.CanonicalEpoch || r.OnlineGeneration != old.OnlineGeneration || r.WriterEpoch != old.WriterEpoch || r.ScopeHash != old.ScopeHash || r.AdapterSetHash != old.AdapterSetHash {
			return errors.New("REORG_COMMIT_MISMATCH")
		}
	} else {
		return errors.New("PREPARED_KIND_UNSUPPORTED")
	}
	b, e := json.Marshal(u)
	if e != nil {
		return e
	}
	previous, c, e := s.db.Get([]byte("online/prepared"))
	if e == nil {
		same := string(previous) == string(b)
		c.Close()
		if !same {
			return errors.New("PREPARED_CONFLICT")
		}
		oldScope, err := s.scopePrepared()
		if err != nil {
			return err
		}
		if !scopePreparedEqual(oldScope, scope) {
			return errors.New("SCOPE_PREPARED_CONFLICT")
		}
		oldRecords, err := s.ownerPrepared()
		if err != nil {
			return err
		}
		if !ownerRecordsEqual(oldRecords, records) {
			return errors.New("OWNER_RECORDS_PREPARED_CONFLICT")
		}
		if gate != nil {
			return gate()
		}
		return nil
	}
	if !errors.Is(e, pebble.ErrNotFound) {
		return e
	}
	batch := s.db.NewBatch()
	defer batch.Close()
	if e = batch.Set([]byte("online/prepared"), b, nil); e != nil {
		return e
	}
	if scope != nil {
		raw, err := json.Marshal(scope)
		if err != nil {
			return err
		}
		if e = batch.Set([]byte("online/scope-prepared"), raw, nil); e != nil {
			return e
		}
	} else if e = batch.Delete([]byte("online/scope-prepared"), nil); e != nil {
		return e
	}
	if scope != nil && len(scope.OwnerRecords) > 0 {
		if e = s.planOwnerRecords(nil, u.Kind, scope.Commit.CommitID, u.Snapshot.Reference, scope.OwnerRecords); e != nil {
			return e
		}
	}
	if records != nil {
		if e = s.planOwnerRecords(nil, u.Kind, records.UnitID, u.Snapshot.Reference, records.Records); e != nil {
			return e
		}
	}
	if e = s.stageOwnerRecords(batch, records); e != nil {
		return e
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	return s.commitOwnerBatch(batch)
}
func (s *Store) Prepared() (*journal.Unit, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.prepared()
}
func (s *Store) prepared() (*journal.Unit, error) {
	if s.closed || s.failed {
		return nil, ErrOwnerReopenRequired
	}
	b, c, e := s.db.Get([]byte("online/prepared"))
	if errors.Is(e, pebble.ErrNotFound) {
		return nil, nil
	}
	if e != nil {
		return nil, e
	}
	defer c.Close()
	var u journal.Unit
	if e = json.Unmarshal(b, &u); e != nil {
		return nil, e
	}
	return &u, nil
}
func (s *Store) DiscardPrepared() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	batch := s.db.NewBatch()
	defer batch.Close()
	if e := batch.Delete([]byte("online/prepared"), nil); e != nil {
		return e
	}
	if e := batch.Delete([]byte("online/scope-prepared"), nil); e != nil {
		return e
	}
	if e := batch.Delete([]byte("online/owner-records-prepared"), nil); e != nil {
		return e
	}
	return s.commitOwnerBatch(batch)
}
func (s *Store) ApplyPrepared(cursor journal.Cursor) error {
	return s.ApplyPreparedScope(cursor, nil)
}
func (s *Store) ApplyPreparedScope(cursor journal.Cursor, gate func() error) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	u, e := s.prepared()
	if e != nil {
		return e
	}
	if u == nil {
		return errors.New("PREPARED_REQUIRED")
	}
	if cursor.Topic == "" || cursor.Partition != 0 || cursor.Offset < 1 {
		return errors.New("JOURNAL_CURSOR_INVALID")
	}
	var undo *domain.Snapshot
	if u.Kind == "BLOCK_COMMIT" && s.current != nil {
		copy := clone(*s.current)
		undo = &copy
	}
	return s.persistScope(u.Snapshot, u.Kind, undo, &cursor, nil, gate)
}
func (s *Store) Bootstrap() (journal.Bootstrap, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed || s.failed || s.current == nil || s.cursor == nil || !s.authorityDeadline.IsZero() && time.Now().After(s.authorityDeadline) {
		return journal.Bootstrap{}, errors.New("BOOTSTRAP_NOT_READY")
	}
	return journal.Bootstrap{Snapshot: clone(*s.current), Cursor: *s.cursor}, nil
}

func (s *Store) SetAuthorityDeadline(deadline time.Time) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.authorityDeadline = deadline
}
