package reducer

import (
	"encoding/json"
	"errors"
	"github.com/cockroachdb/pebble"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func (s *Store) ReorgPending() bool { s.mu.RLock(); defer s.mu.RUnlock(); return s.reorg != nil }

// UndoParent only returns the exact parent block's state. Callers walk this
// backwards before staging replacement blocks; it never changes the read view.
func (s *Store) UndoParent(child domain.Snapshot) (domain.Snapshot, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	b, c, e := s.db.Get([]byte("undo/" + child.Reference.Block.ParentHash))
	if e != nil {
		return domain.Snapshot{}, errors.New("REORG_UNDO_UNAVAILABLE")
	}
	defer c.Close()
	var parent domain.Snapshot
	if e = json.Unmarshal(b, &parent); e != nil {
		return parent, e
	}
	if e = parent.Validate(s.owner, s.assignment); e != nil {
		return parent, e
	}
	p, r := parent.Reference, child.Reference
	if p.Block.ChainKey != r.Block.ChainKey || p.Block.Hash != r.Block.ParentHash || p.Block.Number >= r.Block.Number || r.Block.Number-p.Block.Number != 1 || p.ScopeHash != r.ScopeHash || p.AdapterSetHash != r.AdapterSetHash {
		return domain.Snapshot{}, errors.New("REORG_UNDO_IDENTITY")
	}
	return clone(parent), nil
}

// StageReorg persists a bounded, complete replacement branch without publishing
// any intermediate state. The final KV commit retains these states as undo.
func (s *Store) StageReorg(branch []domain.Snapshot, maxBytes int) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.reorg == nil || s.reorgKind != "REORG_BEGIN" {
		return errors.New("REORG_BEGIN_REQUIRED")
	}
	if len(branch) == 0 || uint64(len(branch)-1) > s.window || maxBytes <= 0 {
		return errors.New("REORG_STAGE_LIMIT")
	}
	begin := s.reorg.Reference
	for i, snapshot := range branch {
		if e := snapshot.Validate(s.owner, s.assignment); e != nil {
			return e
		}
		r := snapshot.Reference
		expected := begin
		expected.JournalSeq++
		expected.Block = r.Block
		if r != expected {
			return errors.New("REORG_STAGE_VERSION")
		}
		if i == 0 {
			if r.Block.Number > begin.Block.Number || begin.Block.Number-r.Block.Number > s.window {
				return errors.New("REORG_STAGE_WINDOW")
			}
		} else {
			p := branch[i-1].Reference.Block
			if r.Block.Number != p.Number+1 || r.Block.ParentHash != p.Hash {
				return errors.New("REORG_STAGE_CONTINUITY")
			}
		}
	}
	b, e := json.Marshal(branch)
	if e != nil {
		return e
	}
	if len(b) > maxBytes {
		return errors.New("REORG_STAGE_LIMIT")
	}
	if old, c, e := s.db.Get([]byte("online/reorg-stage")); e == nil {
		same := string(old) == string(b)
		c.Close()
		if !same {
			return errors.New("REORG_STAGE_CONFLICT")
		}
		return nil
	} else if !errors.Is(e, pebble.ErrNotFound) {
		return e
	}
	return s.db.Set([]byte("online/reorg-stage"), b, pebble.Sync)
}

// ApplyLocal is restricted to the explicit HTTP development transport. Kafka
// sessions always call ApplyPrepared with the actual committed Kafka cursor.
func (s *Store) ApplyLocal(u journal.Unit) error {
	if e := s.Prepare(u); e != nil {
		return e
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	var undo *domain.Snapshot
	if u.Kind == "BLOCK_COMMIT" && s.current != nil {
		previous := clone(*s.current)
		undo = &previous
	}
	return s.persist(u.Snapshot, u.Kind, undo, nil)
}

func (s *Store) ReorgRevoked() bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.reorgKind == "ONLINE_SCOPE_REVOKED"
}
