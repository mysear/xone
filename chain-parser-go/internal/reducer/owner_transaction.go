package reducer

import (
	"errors"
	"github.com/cockroachdb/pebble"
)

var ErrOwnerReopenRequired = errors.New("OWNER_STORE_REOPEN_REQUIRED")

// An I/O failure may occur after a durable batch became visible. Never reuse
// the cached generation/sequence floor after an uncertain write. Reopen first
// and reconcile the authoritative prepared/journal state.
func (s *Store) commitOwnerBatch(batch *pebble.Batch) error {
	if s.closed || s.failed {
		return ErrOwnerReopenRequired
	}
	var err error
	if s.commitFault != nil {
		err = s.commitFault(batch)
	} else {
		err = batch.Commit(pebble.Sync)
	}
	if err != nil {
		s.failed = true
		s.current = nil
	}
	return err
}
func (s *Store) setOwnerValue(key, value []byte) error {
	if s.closed || s.failed {
		return ErrOwnerReopenRequired
	}
	batch := s.db.NewBatch()
	defer batch.Close()
	if err := batch.Set(key, value, nil); err != nil {
		return err
	}
	return s.commitOwnerBatch(batch)
}
func (s *Store) NeedsReopen() bool { s.mu.RLock(); defer s.mu.RUnlock(); return s.failed }
