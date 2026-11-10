package reducer

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/cockroachdb/pebble"
	"strings"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

const maxOwnerRecords = 4096
const maxOwnerRecordBytes = 16 << 20

// OwnerRecord is an immutable dedupe/receipt record made visible in the same
// local owner transaction as its exact state publication. It is never a proof
// of a still-pending external Kafka transaction.
type OwnerRecord struct {
	Key       string             `json:"key"`
	Kind      string             `json:"kind"`
	CommitID  string             `json:"commit_id"`
	Reference domain.SnapshotRef `json:"reference"`
	Payload   json.RawMessage    `json:"payload"`
}
type ownerPreparedRecords struct {
	Kind      string                     `json:"kind,omitempty"`
	UnitID    string                     `json:"unit_id"`
	Reference domain.SnapshotRef         `json:"reference"`
	Records   map[string]json.RawMessage `json:"records"`
}

func validOwnerRecordKey(key, namespace string) bool {
	prefix := namespace + "-replay/"
	return strings.HasPrefix(key, prefix) && validScopeJobKey(strings.TrimPrefix(key, prefix))
}
func validOwnerRecords(records map[string]json.RawMessage, namespace string) bool {
	if len(records) == 0 || len(records) > 64 {
		return false
	}
	var bytes int
	for key, value := range records {
		if !validOwnerRecordKey(key, namespace) || len(value) == 0 || len(value) > 65536 || !json.Valid(value) {
			return false
		}
		bytes += len(value) + len(key)
		if bytes > 4<<20 {
			return false
		}
	}
	return true
}
func (s *Store) ownerPrepared() (*ownerPreparedRecords, error) {
	if s.closed || s.failed {
		return nil, ErrOwnerReopenRequired
	}
	raw, c, e := s.db.Get([]byte("online/owner-records-prepared"))
	if errors.Is(e, pebble.ErrNotFound) {
		return nil, nil
	}
	if e != nil {
		return nil, e
	}
	defer c.Close()
	var p ownerPreparedRecords
	if len(raw) > 8<<20 || json.Unmarshal(raw, &p) != nil || len(p.UnitID) != 64 || !validOwnerRecords(p.Records, "online") {
		return nil, errors.New("OWNER_RECORDS_PREPARED_CORRUPT")
	}
	return &p, nil
}
func ownerRecordsEqual(a, b *ownerPreparedRecords) bool {
	if a == nil || b == nil {
		return a == nil && b == nil
	}
	x, _ := json.Marshal(a)
	y, _ := json.Marshal(b)
	return bytes.Equal(x, y)
}
func (s *Store) PrepareOwnerRecords(unit journal.Unit, records map[string]json.RawMessage, gate func() error) error {
	if (unit.Kind != "BLOCK_COMMIT" && unit.Kind != "ONLINE_BASELINE_COMMIT" && unit.Kind != "REORG_COMMIT") || !validOwnerRecords(records, "online") {
		return errors.New("OWNER_RECORDS_INVALID")
	}
	id, e := unit.CanonicalHash()
	if e != nil {
		return e
	}
	raw, e := json.Marshal(records)
	if e != nil {
		return e
	}
	var detached map[string]json.RawMessage
	if json.Unmarshal(raw, &detached) != nil {
		return errors.New("OWNER_RECORDS_INVALID")
	}
	return s.prepareOwner(unit, nil, &ownerPreparedRecords{Kind: unit.Kind, UnitID: id, Reference: unit.Snapshot.Reference, Records: detached}, gate)
}
func (s *Store) OwnerRecord(key string) (OwnerRecord, error) {
	if !validOwnerRecordKey(key, "online") && !validOwnerRecordKey(key, "history") {
		return OwnerRecord{}, errors.New("OWNER_RECORD_KEY_INVALID")
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed || s.failed {
		return OwnerRecord{}, ErrOwnerReopenRequired
	}
	raw, c, e := s.db.Get([]byte("owner/records/" + key))
	if e != nil {
		return OwnerRecord{}, e
	}
	defer c.Close()
	var proof OwnerRecord
	if len(raw) > 131072 || json.Unmarshal(raw, &proof) != nil || proof.Key != key || len(proof.CommitID) != 64 || !json.Valid(proof.Payload) {
		return OwnerRecord{}, errors.New("OWNER_RECORD_CORRUPT")
	}
	return proof, nil
}
func (s *Store) appendOwnerRecords(batch *pebble.Batch, kind, id string, reference domain.SnapshotRef, records map[string]json.RawMessage) error {
	return s.planOwnerRecords(batch, kind, id, reference, records)
}

// planOwnerRecords also runs without a batch during Prepare. Pending receipts
// are durable quota reservations; history cannot consume their count/bytes.
func (s *Store) planOwnerRecords(batch *pebble.Batch, kind, id string, reference domain.SnapshotRef, records map[string]json.RawMessage) error {
	iter, e := s.db.NewIter(&pebble.IterOptions{LowerBound: []byte("owner/records/"), UpperBound: []byte("owner/records0")})
	if e != nil {
		return e
	}
	defer iter.Close()
	count, used := 0, 0
	for iter.First(); iter.Valid(); iter.Next() {
		count++
		used += len(iter.Key()) + len(iter.Value()) + 256
		if count > maxOwnerRecords || used > maxOwnerRecordBytes {
			return errors.New("OWNER_RECORD_CAPACITY")
		}
	}
	if e = iter.Error(); e != nil {
		return e
	}
	pending, e := s.ownerPrepared()
	if e != nil {
		return e
	}
	reservations := []*ownerPreparedRecords{pending}
	scoped, e := s.scopePrepared()
	if e != nil {
		return e
	}
	if scoped != nil && len(scoped.OwnerRecords) > 0 {
		reservations = append(reservations, &ownerPreparedRecords{Kind: "ONLINE_BASELINE_COMMIT", UnitID: scoped.Commit.CommitID, Reference: scoped.Commit.Reference, Records: scoped.OwnerRecords})
	}
	reservedKeys := map[string]bool{}
	for _, pending := range reservations {
		if pending == nil || pending.UnitID == id {
			continue
		}
		pendingKind := pending.Kind
		if pendingKind == "" {
			unit, err := s.prepared()
			if err != nil {
				return err
			}
			if unit == nil {
				return errors.New("OWNER_RECORDS_WITHOUT_PREPARED_UNIT")
			}
			pendingKind = unit.Kind
		}
		for key, payload := range pending.Records {
			proof := OwnerRecord{Key: key, Kind: pendingKind, CommitID: pending.UnitID, Reference: pending.Reference, Payload: payload}
			raw, err := json.Marshal(proof)
			if err != nil {
				return err
			}
			storage := []byte("owner/records/" + key)
			previous, closer, err := s.db.Get(storage)
			if err == nil {
				same := bytes.Equal(previous, raw)
				closer.Close()
				if !same {
					return errors.New("OWNER_RECORD_RESERVATION_CONFLICT")
				}
				continue
			}
			if !errors.Is(err, pebble.ErrNotFound) {
				return err
			}
			if reservedKeys[key] {
				continue
			}
			reservedKeys[key] = true
			count++
			used += len(storage) + len(raw) + 256
			if count > maxOwnerRecords || used > maxOwnerRecordBytes {
				return errors.New("OWNER_RECORD_CAPACITY")
			}
		}
	}
	for key, payload := range records {
		proof := OwnerRecord{Key: key, Kind: kind, CommitID: id, Reference: reference, Payload: payload}
		raw, err := json.Marshal(proof)
		if err != nil {
			return err
		}
		storage := []byte("owner/records/" + key)
		old, c, err := s.db.Get(storage)
		if err == nil {
			same := bytes.Equal(old, raw)
			c.Close()
			if !same {
				return errors.New("OWNER_RECORD_CONFLICT")
			}
			continue
		}
		if !errors.Is(err, pebble.ErrNotFound) {
			return err
		}
		count++
		used += len(storage) + len(raw) + 256
		if count > maxOwnerRecords || used > maxOwnerRecordBytes {
			return errors.New("OWNER_RECORD_CAPACITY")
		}
		if batch != nil {
			if e = batch.Set(storage, raw, nil); e != nil {
				return e
			}
		}
	}
	return nil
}
func (s *Store) applyOwnerRecordsBatch(batch *pebble.Batch, snapshot domain.Snapshot, kind string) error {
	pending, e := s.ownerPrepared()
	if e != nil || pending == nil {
		return e
	}
	unit, e := s.prepared()
	if e != nil {
		return e
	}
	if unit == nil {
		return errors.New("OWNER_RECORDS_WITHOUT_PREPARED_UNIT")
	}
	id, e := unit.CanonicalHash()
	if e != nil {
		return e
	}
	if id != pending.UnitID || unit.Kind != kind || unit.Snapshot.Reference != snapshot.Reference || pending.Reference != snapshot.Reference {
		return errors.New("OWNER_RECORDS_COMMIT_CONFLICT")
	}
	expected, e := domain.Hash(unit.Snapshot)
	if e != nil {
		return e
	}
	actual, e := domain.Hash(snapshot)
	if e != nil {
		return e
	}
	if expected != actual {
		return errors.New("OWNER_RECORDS_STATE_CONFLICT")
	}
	if e = s.appendOwnerRecords(batch, kind, id, snapshot.Reference, pending.Records); e != nil {
		return e
	}
	return batch.Delete([]byte("online/owner-records-prepared"), nil)
}
func (s *Store) ApplyLocalOwnerRecords(unit journal.Unit, records map[string]json.RawMessage, gate func() error) error {
	if e := s.PrepareOwnerRecords(unit, records, gate); e != nil {
		return e
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	var undo *domain.Snapshot
	if unit.Kind == "BLOCK_COMMIT" && s.current != nil {
		copy := clone(*s.current)
		undo = &copy
	}
	return s.persistScope(unit.Snapshot, unit.Kind, undo, nil, nil, gate)
}
func HistoryOwnerCommitID(snapshot domain.Snapshot) (string, error) {
	return domain.Hash(map[string]any{"kind": "HISTORY_REPLAY_COMMIT", "snapshot": snapshot})
}
func (s *Store) CommitHistoryOwnerRecords(snapshot domain.Snapshot, records map[string]json.RawMessage, gate func() error) error {
	if !validOwnerRecords(records, "history") {
		return errors.New("HISTORY_OWNER_RECORDS_INVALID")
	}
	if e := snapshot.Validate(s.owner, s.assignment); e != nil {
		return e
	}
	id, e := HistoryOwnerCommitID(snapshot)
	if e != nil {
		return e
	}
	raw, e := json.Marshal(snapshot)
	if e != nil || len(raw) > maxScopeLedgerBytes {
		return errors.New("HISTORY_OWNER_RECORDS_BUDGET")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.failed {
		return ErrOwnerReopenRequired
	}
	batch := s.db.NewBatch()
	defer batch.Close()
	if e = batch.Set([]byte("history/"+snapshot.Reference.Block.Hash), raw, nil); e != nil {
		return e
	}
	if e = s.appendOwnerRecords(batch, "HISTORY_REPLAY_COMMIT", id, snapshot.Reference, records); e != nil {
		return e
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	return s.commitOwnerBatch(batch)
}
func (s *Store) stageOwnerRecords(batch *pebble.Batch, records *ownerPreparedRecords) error {
	if records == nil {
		return batch.Delete([]byte("online/owner-records-prepared"), nil)
	}
	raw, e := json.Marshal(records)
	if e != nil {
		return fmt.Errorf("OWNER_RECORDS_ENCODING: %w", e)
	}
	return batch.Set([]byte("online/owner-records-prepared"), raw, nil)
}

// RetireOwnerRecord is only for a service-owned durable acknowledgement/GC
// plan. The caller must prove pins have been transferred/released and any
// replay resolution is durable. An unknown external outcome must retain its
// receipt. Active prepared dedupe identities are never eligible.
func (s *Store) RetireOwnerRecord(key, expectedCommitID string, gate func() error) error {
	if (!validOwnerRecordKey(key, "online") && !validOwnerRecordKey(key, "history")) || len(expectedCommitID) != 64 {
		return errors.New("OWNER_RECORD_RETIRE_INVALID")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.failed {
		return ErrOwnerReopenRequired
	}
	pending, e := s.ownerPrepared()
	if e != nil {
		return e
	}
	if pending != nil {
		if _, active := pending.Records[key]; active {
			return errors.New("OWNER_RECORD_STILL_PREPARED")
		}
	}
	raw, c, e := s.db.Get([]byte("owner/records/" + key))
	if errors.Is(e, pebble.ErrNotFound) {
		return nil
	}
	if e != nil {
		return e
	}
	var proof OwnerRecord
	e = json.Unmarshal(raw, &proof)
	c.Close()
	if e != nil || proof.Key != key || proof.CommitID != expectedCommitID {
		return errors.New("OWNER_RECORD_RETIRE_CONFLICT")
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	batch := s.db.NewBatch()
	defer batch.Close()
	if e = batch.Delete([]byte("owner/records/"+key), nil); e != nil {
		return e
	}
	return s.commitOwnerBatch(batch)
}
