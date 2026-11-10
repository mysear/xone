package reducer

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"

	"github.com/cockroachdb/pebble"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

const maxScopeLedgerBytes = 64 << 20

// ScopeOwnerCommit is persisted in the same Pebble batch as the complete online
// snapshot, local journal, cursor and scope ledger. Kafka remains the external
// commit authority; preparing these bytes alone grants no readiness.
type ScopeOwnerCommit struct {
	JobID     string             `json:"job_id"`
	CommitID  string             `json:"commit_id"`
	Reference domain.SnapshotRef `json:"reference"`
	Fence     json.RawMessage    `json:"fence"`
}
type preparedScope struct {
	OwnerRecords map[string]json.RawMessage `json:"owner_records,omitempty"`
	BaseDigest   *string                    `json:"base_digest,omitempty"`
	Ledger       []byte                     `json:"ledger"`
	Commit       ScopeOwnerCommit           `json:"commit"`
}

func validScopeJobKey(id string) bool {
	if len(id) == 0 || len(id) > 128 {
		return false
	}
	for _, r := range id {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '-' || r == '_') {
			return false
		}
	}
	return true
}
func scopeLedgerValid(raw []byte) bool {
	return len(raw) > 0 && len(raw) <= maxScopeLedgerBytes && json.Valid(raw) && bytes.HasPrefix(bytes.TrimSpace(raw), []byte("{"))
}
func makePreparedScope(unit journal.Unit, ledger []byte, job string, fence json.RawMessage) (*preparedScope, error) {
	if unit.Kind != "ONLINE_BASELINE_COMMIT" || !scopeLedgerValid(ledger) || !validScopeJobKey(job) || len(fence) == 0 || len(fence) > 16384 || !json.Valid(fence) || !bytes.HasPrefix(bytes.TrimSpace(fence), []byte("{")) {
		return nil, errors.New("SCOPE_OWNER_PUBLICATION_INVALID")
	}
	id, e := unit.CanonicalHash()
	if e != nil {
		return nil, e
	}
	return &preparedScope{Ledger: append([]byte(nil), ledger...), Commit: ScopeOwnerCommit{JobID: job, CommitID: id, Reference: unit.Snapshot.Reference, Fence: append(json.RawMessage(nil), fence...)}}, nil
}
func (s *Store) scopePrepared() (*preparedScope, error) {
	b, c, e := s.db.Get([]byte("online/scope-prepared"))
	if errors.Is(e, pebble.ErrNotFound) {
		return nil, nil
	}
	if e != nil {
		return nil, e
	}
	defer c.Close()
	var p preparedScope
	if json.Unmarshal(b, &p) != nil || !scopeLedgerValid(p.Ledger) || !validScopeJobKey(p.Commit.JobID) || len(p.Commit.CommitID) != 64 || !json.Valid(p.Commit.Fence) {
		return nil, errors.New("SCOPE_PREPARED_CORRUPT")
	}
	return &p, nil
}
func (s *Store) ScopeState() ([]byte, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.scopeState()
}
func (s *Store) scopeState() ([]byte, error) {
	if s.closed || s.failed {
		return nil, ErrOwnerReopenRequired
	}
	b, c, e := s.db.Get([]byte("scope/state"))
	if errors.Is(e, pebble.ErrNotFound) {
		return nil, nil
	}
	if e != nil {
		return nil, e
	}
	defer c.Close()
	if !scopeLedgerValid(b) {
		return nil, errors.New("SCOPE_OWNER_LEDGER_CORRUPT")
	}
	return append([]byte(nil), b...), nil
}
func (s *Store) SaveScopeState(raw []byte, gate func() error) error {
	if !scopeLedgerValid(raw) {
		return errors.New("SCOPE_OWNER_LEDGER_INVALID")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	pending, e := s.prepared()
	if e != nil {
		return e
	}
	if pending != nil {
		return errors.New("SCOPE_OWNER_COMMIT_PENDING")
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	return s.setOwnerValue([]byte("scope/state"), raw)
}
func (s *Store) ScopeCommit(job string) (ScopeOwnerCommit, error) {
	if !validScopeJobKey(job) {
		return ScopeOwnerCommit{}, errors.New("SCOPE_OWNER_JOB_INVALID")
	}
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed || s.failed {
		return ScopeOwnerCommit{}, ErrOwnerReopenRequired
	}
	b, c, e := s.db.Get([]byte("scope/commits/" + job))
	if e != nil {
		return ScopeOwnerCommit{}, e
	}
	defer c.Close()
	var proof ScopeOwnerCommit
	if json.Unmarshal(b, &proof) != nil || proof.JobID != job || len(proof.CommitID) != 64 || !json.Valid(proof.Fence) || proof.Reference.JournalSeq == 0 {
		return ScopeOwnerCommit{}, errors.New("SCOPE_OWNER_COMMIT_CORRUPT")
	}
	return proof, nil
}
func (s *Store) PrepareScope(unit journal.Unit, ledger []byte, job string, fence json.RawMessage, gate func() error) error {
	scope, e := makePreparedScope(unit, ledger, job, fence)
	if e != nil {
		return e
	}
	return s.prepare(unit, scope, gate)
}
func (s *Store) InstallBaselineScope(snapshot domain.Snapshot, ledger []byte, job string, fence json.RawMessage, gate func() error) error {
	scope, e := makePreparedScope(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot}, ledger, job, fence)
	if e != nil {
		return e
	}
	return s.installBaselineScope(snapshot, scope, gate)
}
func (s *Store) installBaselineScope(snapshot domain.Snapshot, scope *preparedScope, gate func() error) error {
	var e error
	s.mu.Lock()
	defer s.mu.Unlock()
	if e = snapshot.Validate(s.owner, s.assignment); e != nil {
		return e
	}
	r := snapshot.Reference
	if r.OnlineGeneration <= s.durable.OnlineGeneration || r.WriterEpoch < s.durable.WriterEpoch || r.JournalSeq <= s.durable.JournalSeq || r.CanonicalEpoch < s.durable.CanonicalEpoch {
		return errors.New("STALE_BASELINE")
	}
	if pending, e := s.prepared(); e != nil {
		return e
	} else if pending != nil {
		return errors.New("SCOPE_OWNER_COMMIT_PENDING")
	}
	return s.persistScope(snapshot, "ONLINE_BASELINE_COMMIT", nil, nil, scope, gate)
}
func (s *Store) applyScopeBatch(batch *pebble.Batch, snapshot domain.Snapshot, kind string, scope *preparedScope) error {
	if scope == nil {
		return nil
	}
	if e := s.checkScopeBase(scope); e != nil {
		return e
	}
	expected, e := (journal.Unit{Kind: kind, Snapshot: snapshot}).CanonicalHash()
	if e != nil {
		return e
	}
	if kind != "ONLINE_BASELINE_COMMIT" || expected != scope.Commit.CommitID || scope.Commit.Reference != snapshot.Reference {
		return errors.New("SCOPE_PREPARED_OWNER_CONFLICT")
	}
	if !scopeLedgerValid(scope.Ledger) || !validScopeJobKey(scope.Commit.JobID) {
		return errors.New("SCOPE_PREPARED_CORRUPT")
	}
	if len(scope.OwnerRecords) > 0 {
		if !validOwnerRecords(scope.OwnerRecords, "online") {
			return errors.New("OWNER_RECORDS_INVALID")
		}
		if e = s.appendOwnerRecords(batch, kind, scope.Commit.CommitID, snapshot.Reference, scope.OwnerRecords); e != nil {
			return e
		}
	}
	proof, e := json.Marshal(scope.Commit)
	if e != nil {
		return e
	}
	for key, value := range map[string][]byte{"scope/state": scope.Ledger, "scope/commits/" + scope.Commit.JobID: proof} {
		if e = batch.Set([]byte(key), value, nil); e != nil {
			return e
		}
	}
	// Commit proofs from superseded generations are audit metadata, never ready
	// handles. Keep one generation bounded by its number of admitted jobs.
	iter, e := s.db.NewIter(&pebble.IterOptions{LowerBound: []byte("scope/commits/"), UpperBound: []byte("scope/commits0")})
	if e != nil {
		return e
	}
	defer iter.Close()
	for iter.First(); iter.Valid(); iter.Next() {
		var old ScopeOwnerCommit
		if json.Unmarshal(iter.Value(), &old) != nil {
			return errors.New("SCOPE_OWNER_COMMIT_CORRUPT")
		}
		if old.Reference.OnlineGeneration < snapshot.Reference.OnlineGeneration {
			if e = batch.Delete(append([]byte(nil), iter.Key()...), nil); e != nil {
				return e
			}
		}
	}
	if e = iter.Error(); e != nil {
		return fmt.Errorf("SCOPE_OWNER_ITERATOR: %w", e)
	}
	return batch.Delete([]byte("online/scope-prepared"), nil)
}
func scopePreparedEqual(a, b *preparedScope) bool {
	if a == nil || b == nil {
		return a == nil && b == nil
	}
	x, _ := json.Marshal(a)
	y, _ := json.Marshal(b)
	return bytes.Equal(x, y)
}

// Only the current recovery actor's acquisition marker is retained here. The
// actual source responses remain in their independent raw archive/WAL; releasing
// this marker never deletes source evidence or a prepared publication.
type scopeInputBuffer struct {
	ID      string          `json:"id"`
	Payload json.RawMessage `json:"payload"`
}

func (s *Store) BeginScopeBuffer(raw []byte) (string, error) {
	if len(raw) > 65536 || !scopeLedgerValid(raw) {
		return "", errors.New("SCOPE_BUFFER_INVALID")
	}
	id, e := domain.Hash(json.RawMessage(raw))
	if e != nil {
		return "", e
	}
	encoded, e := json.Marshal(scopeInputBuffer{ID: id, Payload: append(json.RawMessage(nil), raw...)})
	if e != nil {
		return "", e
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed || s.failed {
		return "", ErrOwnerReopenRequired
	}
	pending, e := s.prepared()
	if e != nil {
		return "", e
	}
	if pending != nil {
		return "", errors.New("SCOPE_OWNER_COMMIT_PENDING")
	}
	if e = s.setOwnerValue([]byte("scope/input-buffer"), encoded); e != nil {
		return "", e
	}
	return id, nil
}
func (s *Store) ScopeBuffer(id string) ([]byte, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.scopeBuffer(id)
}
func (s *Store) scopeBuffer(id string) ([]byte, error) {
	if s.closed || s.failed {
		return nil, ErrOwnerReopenRequired
	}
	raw, c, e := s.db.Get([]byte("scope/input-buffer"))
	if e != nil {
		return nil, e
	}
	defer c.Close()
	var record scopeInputBuffer
	if json.Unmarshal(raw, &record) != nil || len(record.Payload) > 65536 || !scopeLedgerValid(record.Payload) {
		return nil, errors.New("SCOPE_BUFFER_CORRUPT")
	}
	digest, e := domain.Hash(record.Payload)
	if e != nil || digest != record.ID {
		return nil, errors.New("SCOPE_BUFFER_CORRUPT")
	}
	if record.ID != id {
		return nil, errors.New("SCOPE_BUFFER_REPLACED")
	}
	return append([]byte(nil), record.Payload...), nil
}
func (s *Store) ReleaseScopeBuffer(id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, e := s.scopeBuffer(id); errors.Is(e, pebble.ErrNotFound) || e != nil && e.Error() == "SCOPE_BUFFER_REPLACED" {
		return nil
	} else if e != nil {
		return e
	}
	return s.db.Delete([]byte("scope/input-buffer"), pebble.Sync)
}

// PriorPoolIDs may reuse catalog identities after an empty-runtime restart. It
// deliberately returns no balances, prices, cursor or ready flag; every returned
// identity must be re-admitted by the current catalog and reread at a fresh H.
func (s *Store) PriorPoolIDs() ([]string, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.closed || s.failed {
		return nil, ErrOwnerReopenRequired
	}
	raw, c, e := s.db.Get([]byte("online/current"))
	if errors.Is(e, pebble.ErrNotFound) {
		return nil, nil
	}
	if e != nil {
		return nil, e
	}
	defer c.Close()
	if len(raw) > maxScopeLedgerBytes {
		return nil, errors.New("SCOPE_PRIOR_CATALOG_BUDGET")
	}
	var snapshot struct {
		Pools []struct {
			PoolID string `json:"pool_id"`
		} `json:"pools"`
	}
	if json.Unmarshal(raw, &snapshot) != nil {
		return nil, errors.New("SCOPE_PRIOR_CATALOG_CORRUPT")
	}
	ids := make([]string, 0, len(snapshot.Pools))
	seen := map[string]bool{}
	for _, p := range snapshot.Pools {
		if p.PoolID == "" || seen[p.PoolID] {
			return nil, errors.New("SCOPE_PRIOR_CATALOG_CORRUPT")
		}
		seen[p.PoolID] = true
		ids = append(ids, p.PoolID)
	}
	return ids, nil
}

func ScopeLedgerDigest(raw []byte) string {
	if raw == nil {
		return ""
	}
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}
func (s *Store) CompareScopeState(expected string, raw []byte, gate func() error) error {
	if !scopeLedgerValid(raw) {
		return errors.New("SCOPE_OWNER_LEDGER_INVALID")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	current, e := s.scopeState()
	if e != nil {
		return e
	}
	if ScopeLedgerDigest(current) != expected {
		return errors.New("SCOPE_OWNER_LEDGER_CHANGED")
	}
	pending, e := s.prepared()
	if e != nil {
		return e
	}
	if pending != nil {
		return errors.New("SCOPE_OWNER_COMMIT_PENDING")
	}
	if gate != nil {
		if e = gate(); e != nil {
			return e
		}
	}
	return s.setOwnerValue([]byte("scope/state"), raw)
}
func (s *Store) checkScopeBase(scope *preparedScope) error {
	if scope == nil || scope.BaseDigest == nil {
		return nil
	}
	raw, e := s.scopeState()
	if e != nil {
		return e
	}
	if ScopeLedgerDigest(raw) != *scope.BaseDigest {
		return errors.New("SCOPE_OWNER_LEDGER_CHANGED")
	}
	return nil
}
func (s *Store) PrepareScopeCAS(unit journal.Unit, ledger []byte, job string, fence json.RawMessage, expected string, gate func() error) error {
	scope, e := makePreparedScope(unit, ledger, job, fence)
	if e != nil {
		return e
	}
	scope.BaseDigest = &expected
	return s.prepare(unit, scope, gate)
}
func (s *Store) InstallBaselineScopeCAS(snapshot domain.Snapshot, ledger []byte, job string, fence json.RawMessage, expected string, gate func() error) error {
	scope, e := makePreparedScope(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot}, ledger, job, fence)
	if e != nil {
		return e
	}
	scope.BaseDigest = &expected
	return s.installBaselineScope(snapshot, scope, gate)
}

func (s *Store) PrepareGuard(unit journal.Unit, gate func() error) error {
	return s.prepare(unit, nil, gate)
}

func (s *Store) PrepareScopeRecordsCAS(unit journal.Unit, ledger []byte, job string, fence json.RawMessage, expected string, records map[string]json.RawMessage, gate func() error) error {
	if !validOwnerRecords(records, "online") {
		return errors.New("OWNER_RECORDS_INVALID")
	}
	scope, e := makePreparedScope(unit, ledger, job, fence)
	if e != nil {
		return e
	}
	scope.BaseDigest = &expected
	raw, e := json.Marshal(records)
	if e != nil {
		return e
	}
	if e = json.Unmarshal(raw, &scope.OwnerRecords); e != nil {
		return e
	}
	return s.prepare(unit, scope, gate)
}
func (s *Store) InstallBaselineScopeRecordsCAS(snapshot domain.Snapshot, ledger []byte, job string, fence json.RawMessage, expected string, records map[string]json.RawMessage, gate func() error) error {
	if !validOwnerRecords(records, "online") {
		return errors.New("OWNER_RECORDS_INVALID")
	}
	scope, e := makePreparedScope(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot}, ledger, job, fence)
	if e != nil {
		return e
	}
	scope.BaseDigest = &expected
	raw, e := json.Marshal(records)
	if e != nil {
		return e
	}
	if e = json.Unmarshal(raw, &scope.OwnerRecords); e != nil {
		return e
	}
	return s.installBaselineScope(snapshot, scope, gate)
}
