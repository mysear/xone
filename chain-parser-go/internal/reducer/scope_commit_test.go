package reducer

import (
	"encoding/json"
	"errors"
	"github.com/cockroachdb/pebble"
	"testing"
	"xone/chain-parser-go/internal/journal"
)

func TestScopeOwnerPublicationIsAtomicWithPreparedJournalAndSurvivesCrash(t *testing.T) {
	directory := t.TempDir()
	store, e := Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	snapshot := baseline(t)
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot}
	ledger := []byte(`{"format":"test-scope-state","session":1}`)
	fence := json.RawMessage(`{"chain_key":"evm:31337","online_generation":1}`)
	if e = store.PrepareScope(unit, ledger, "scope-job", fence, nil); e != nil {
		t.Fatal(e)
	}
	if _, ready := store.Current(); ready {
		t.Fatal("prepared scope was visible")
	}
	if _, e = store.ScopeCommit("scope-job"); e == nil {
		t.Fatal("uncommitted scope proof visible")
	}
	state, e := store.ScopeState()
	if e != nil || state != nil {
		t.Fatal("prepared ledger was committed", e, string(state))
	}
	if e = store.Close(); e != nil {
		t.Fatal(e)
	}
	store, e = Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if e = store.ApplyPrepared(journal.Cursor{Topic: "scope-test", Partition: 0, Offset: 3}); e != nil {
		t.Fatal(e)
	}
	state, e = store.ScopeState()
	if e != nil || string(state) != string(ledger) {
		t.Fatal("missing committed ledger", e, string(state))
	}
	proof, e := store.ScopeCommit("scope-job")
	if e != nil || proof.Reference != snapshot.Reference || string(proof.Fence) != string(fence) {
		t.Fatal("missing owner commit proof", e, proof)
	}
	expected, _ := unit.CanonicalHash()
	if proof.CommitID != expected {
		t.Fatal("scope not bound to owner unit")
	}
	if _, ready := store.Current(); !ready {
		t.Fatal("complete online snapshot not published")
	}
}

func TestScopePreparedDiscardAndConflictsCannotPublishOrOverwriteLedger(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	ledger := []byte(`{"scope":1}`)
	fence := json.RawMessage(`{"generation":1}`)
	if e = store.PrepareScope(unit, ledger, "job", fence, nil); e != nil {
		t.Fatal(e)
	}
	if e = store.PrepareScope(unit, []byte(`{"scope":2}`), "job", fence, nil); e == nil {
		t.Fatal("same unit changed scope ledger")
	}
	if e = store.Prepare(unit); e == nil {
		t.Fatal("unscoped prepare hid pending scope publication")
	}
	if e = store.SaveScopeState([]byte(`{"scope":3}`), nil); e == nil {
		t.Fatal("checkpoint crossed pending owner commit")
	}
	if e = store.DiscardPrepared(); e != nil {
		t.Fatal(e)
	}
	state, e := store.ScopeState()
	if e != nil || state != nil {
		t.Fatal("discard published scope ledger", e)
	}
	if e = store.Prepare(unit); e != nil {
		t.Fatal("scope sidecar not discarded", e)
	}
}

func TestScopeLocalBaselineAndFailureGateShareTheSameOwnerTransaction(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	snapshot := baseline(t)
	ledger := []byte(`{"scope":1}`)
	fence := json.RawMessage(`{"generation":1}`)
	rejected := errors.New("expired-scope-lease")
	if e = store.InstallBaselineScope(snapshot, ledger, "job", fence, func() error { return rejected }); !errors.Is(e, rejected) {
		t.Fatal("gate not evaluated", e)
	}
	if store.DurableRef().JournalSeq != 0 {
		t.Fatal("failed gate wrote online state")
	}
	if _, e = store.ScopeCommit("job"); e == nil {
		t.Fatal("failed gate wrote scope proof")
	}
	if e = store.InstallBaselineScope(snapshot, ledger, "job", fence, nil); e != nil {
		t.Fatal(e)
	}
	proof, e := store.ScopeCommit("job")
	if e != nil || proof.Reference != snapshot.Reference {
		t.Fatal(e, proof)
	}
}

func TestScopeInputBufferIsDurableBoundedAndReleaseCannotEraseAnotherAttempt(t *testing.T) {
	directory := t.TempDir()
	store, e := Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	first, e := store.BeginScopeBuffer([]byte(`{"source":"public-rpc","attempt":1}`))
	if e != nil {
		t.Fatal(e)
	}
	store.Close()
	store, e = Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if _, e = store.ScopeBuffer(first); e != nil {
		t.Fatal("buffer marker not durable", e)
	}
	second, e := store.BeginScopeBuffer([]byte(`{"source":"public-rpc","attempt":2}`))
	if e != nil {
		t.Fatal(e)
	}
	if e = store.ReleaseScopeBuffer(first); e != nil {
		t.Fatal(e)
	}
	if _, e = store.ScopeBuffer(second); e != nil {
		t.Fatal("old release erased active buffer", e)
	}
	if _, e = store.BeginScopeBuffer(make([]byte, 65537)); e == nil {
		t.Fatal("oversized marker accepted")
	}
}

func TestScopeFinalGateIsRecheckedAfterExternalJournalCommit(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	active := true
	expired := errors.New("scope-authority-expired")
	gate := func() error {
		if !active {
			return expired
		}
		return nil
	}
	if e = store.PrepareScope(unit, []byte(`{"scope":1}`), "job", json.RawMessage(`{"generation":1}`), gate); e != nil {
		t.Fatal(e)
	}
	active = false
	cursor := journal.Cursor{Topic: "committed-scope", Partition: 0, Offset: 2}
	if e = store.ApplyPreparedScope(cursor, gate); !errors.Is(e, expired) {
		t.Fatal("final publication did not recheck authority", e)
	}
	if store.DurableRef().JournalSeq != 0 {
		t.Fatal("late commit bypassed gate")
	}
	if _, e = store.ScopeCommit("job"); e == nil {
		t.Fatal("late scope proof published")
	}
	pending, e := store.Prepared()
	if e != nil || pending == nil {
		t.Fatal("unknown externally committed outcome lost durable prepared state", e)
	}
	active = true
	if e = store.ApplyPreparedScope(cursor, gate); e != nil {
		t.Fatal(e)
	}
}

func TestScopeLedgerCASRejectsAnotherOwnerAdvancingTheLedger(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	first := []byte(`{"fence":1}`)
	next := []byte(`{"fence":2}`)
	if e = store.CompareScopeState("", first, nil); e != nil {
		t.Fatal(e)
	}
	if e = store.CompareScopeState(ScopeLedgerDigest(first), next, nil); e != nil {
		t.Fatal(e)
	}
	if e = store.CompareScopeState(ScopeLedgerDigest(first), []byte(`{"fence":1,"stale":true}`), nil); e == nil {
		t.Fatal("old owner overwrote newer ledger")
	}
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	if e = store.PrepareScopeCAS(unit, []byte(`{"ready":true}`), "job", json.RawMessage(`{"generation":1}`), ScopeLedgerDigest(first), nil); e == nil {
		t.Fatal("old scope prepared against a superseded owner ledger")
	}
	current, e := store.ScopeState()
	if e != nil || string(current) != string(next) {
		t.Fatal("CAS failure changed ledger", e, string(current))
	}
}

func TestScopeAmbiguousLocalCommitRequiresReopenBeforeAnyRetry(t *testing.T) {
	directory := t.TempDir()
	store, e := Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	snapshot := baseline(t)
	uncertain := errors.New("fsync-result-unknown")
	store.commitFault = func(batch *pebble.Batch) error {
		if e := batch.Commit(pebble.Sync); e != nil {
			return e
		}
		return uncertain
	}
	e = store.InstallBaselineScope(snapshot, []byte(`{"scope":1}`), "job", json.RawMessage(`{"generation":1}`), nil)
	if !errors.Is(e, uncertain) {
		store.Close()
		t.Fatal("did not reproduce ambiguous commit", e)
	}
	if _, ready := store.Current(); ready {
		store.Close()
		t.Fatal("unknown commit exposed readiness")
	}
	if _, e = store.BeginScopeBuffer([]byte(`{"attempt":2}`)); !errors.Is(e, ErrOwnerReopenRequired) {
		store.Close()
		t.Fatal("unknown outcome was blindly retried", e)
	}
	store.Close()
	store, e = Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if store.DurableRef() != snapshot.Reference {
		t.Fatal("reopen lost actually committed owner state")
	}
	if _, ready := store.Current(); ready {
		t.Fatal("reopen restored old readiness")
	}
	next := snapshot
	next.Reference.OnlineGeneration++
	next.Reference.WriterEpoch++
	next.Reference.JournalSeq++
	if e = store.InstallBaseline(next); e != nil {
		t.Fatal("fresh generation could not recover after reopen", e)
	}
}
