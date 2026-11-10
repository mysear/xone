package reducer

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func TestReplayReceiptAndOnlineStateCommitInTheSamePreparedBatch(t *testing.T) {
	dir := t.TempDir()
	store, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	key := "online-replay/attempt1"
	payload := json.RawMessage(`{"attempt":"attempt1","state":"confirmed"}`)
	if e = store.PrepareOwnerRecords(unit, map[string]json.RawMessage{key: payload}, nil); e != nil {
		t.Fatal(e)
	}
	if _, e = store.OwnerRecord(key); e == nil {
		t.Fatal("prepared replay receipt visible")
	}
	store.Close()
	store, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if e = store.ApplyPrepared(journal.Cursor{Topic: "replay-owner", Partition: 0, Offset: 2}); e != nil {
		t.Fatal(e)
	}
	proof, e := store.OwnerRecord(key)
	if e != nil || string(proof.Payload) != string(payload) || proof.Reference != unit.Snapshot.Reference {
		t.Fatal("receipt not atomically bound", proof, e)
	}
	id, _ := unit.CanonicalHash()
	if proof.CommitID != id {
		t.Fatal("receipt not bound to exact canonical unit")
	}
	if _, ready := store.Current(); !ready {
		t.Fatal("committed receipt without online state")
	}
}
func TestOwnerRecordsCannotChangePendingIdentityOrCrossHistoryNamespace(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	if e = store.PrepareOwnerRecords(unit, map[string]json.RawMessage{"history-replay/x": json.RawMessage(`{}`)}, nil); e == nil {
		t.Fatal("history capability used online path")
	}
	records := map[string]json.RawMessage{"online-replay/attempt1": json.RawMessage(`{"output":"one"}`)}
	if e = store.PrepareOwnerRecords(unit, records, nil); e != nil {
		t.Fatal(e)
	}
	if e = store.PrepareOwnerRecords(unit, map[string]json.RawMessage{"online-replay/attempt1": json.RawMessage(`{"output":"two"}`)}, nil); e == nil {
		t.Fatal("pending replay identity changed")
	}
	if e = store.Prepare(unit); e == nil {
		t.Fatal("plain writer hid replay dedupe metadata")
	}
	if e = store.DiscardPrepared(); e != nil {
		t.Fatal(e)
	}
	if _, e = store.OwnerRecord("online-replay/attempt1"); e == nil {
		t.Fatal("discard published receipt")
	}
}
func TestHistoryReplayReceiptHasNoOnlinePublicationCapability(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	snap := baseline(t)
	records := map[string]json.RawMessage{"history-replay/attempt": json.RawMessage(`{"history":true}`)}
	if e = store.CommitHistoryOwnerRecords(snap, records, nil); e != nil {
		t.Fatal(e)
	}
	if _, ready := store.Current(); ready || store.DurableRef().JournalSeq != 0 {
		t.Fatal("history wrote online state")
	}
	proof, e := store.OwnerRecord("history-replay/attempt")
	if e != nil || proof.Kind != "HISTORY_REPLAY_COMMIT" {
		t.Fatal(proof, e)
	}
	if e = store.CommitHistoryOwnerRecords(snap, map[string]json.RawMessage{"online-replay/attempt": json.RawMessage(`{}`)}, nil); e == nil {
		t.Fatal("history wrote online receipt namespace")
	}
}

func TestOwnerRecordConflictsAreRejectedBeforeExternalPublication(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	records := map[string]json.RawMessage{"online-replay/reused": json.RawMessage(`{"same":true}`)}
	if e = store.ApplyLocalOwnerRecords(unit, records, nil); e != nil {
		t.Fatal(e)
	}
	next := unit
	next.Snapshot.Reference.OnlineGeneration++
	next.Snapshot.Reference.WriterEpoch++
	next.Snapshot.Reference.JournalSeq++
	if e = store.PrepareOwnerRecords(next, records, nil); e == nil {
		t.Fatal("immutable receipt conflict was deferred until after Kafka publication")
	}
	if pending, e := store.Prepared(); e != nil || pending != nil {
		t.Fatal("rejected receipt left a publishable pending unit", e)
	}
}
func fillHistoryOwnerRecords(t *testing.T, store *Store, count int) {
	t.Helper()
	snapshot := baseline(t)
	for start := 0; start < count; start += 64 {
		records := map[string]json.RawMessage{}
		for n := start; n < count && n < start+64; n++ {
			records[fmt.Sprintf("history-replay/record-%04d", n)] = json.RawMessage(`{"history":true}`)
		}
		if e := store.CommitHistoryOwnerRecords(snapshot, records, nil); e != nil {
			t.Fatal("fill history receipt capacity", e)
		}
	}
}
func TestOwnerRecordCapacityIsReservedDurablyBeforeKafkaAndCannotBeStolenByHistory(t *testing.T) {
	directory := t.TempDir()
	store, e := Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	fillHistoryOwnerRecords(t, store, maxOwnerRecords-1)
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	if e = store.PrepareOwnerRecords(unit, map[string]json.RawMessage{"online-replay/last-slot": json.RawMessage(`{"online":true}`)}, nil); e != nil {
		store.Close()
		t.Fatal(e)
	}
	store.Close()
	store, e = Open(directory, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if e = store.CommitHistoryOwnerRecords(baseline(t), map[string]json.RawMessage{"history-replay/steal-slot": json.RawMessage(`{"history":true}`)}, nil); e == nil {
		t.Fatal("history stole a durable pending online receipt reservation")
	}
	if e = store.ApplyPrepared(journal.Cursor{Topic: "owner-record-reservation", Partition: 0, Offset: 2}); e != nil {
		t.Fatal("externally committed unit cannot finish after reopen", e)
	}
}
func TestOwnerRecordFullCapacityRejectsPrepareWithoutPending(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	fillHistoryOwnerRecords(t, store, maxOwnerRecords)
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	if e = store.PrepareOwnerRecords(unit, map[string]json.RawMessage{"online-replay/overflow": json.RawMessage(`{}`)}, nil); e == nil {
		t.Fatal("full capacity passed pre-publication reservation")
	}
	if p, e := store.Prepared(); e != nil || p != nil {
		t.Fatal("full capacity left a pending publication", e)
	}
}

func TestOwnerReceiptRetirementRequiresExactCommitAndAcknowledgement(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	key := "online-replay/acknowledged-capture"
	if e = store.ApplyLocalOwnerRecords(unit, map[string]json.RawMessage{key: json.RawMessage(`{"capture":"done"}`)}, nil); e != nil {
		t.Fatal(e)
	}
	proof, e := store.OwnerRecord(key)
	if e != nil {
		t.Fatal(e)
	}
	if e = store.RetireOwnerRecord(key, "wrong-commit", nil); e == nil {
		t.Fatal("wrong owner commit retired receipt")
	}
	denied := errors.New("pins-or-resolution-not-acknowledged")
	if e = store.RetireOwnerRecord(key, proof.CommitID, func() error { return denied }); !errors.Is(e, denied) {
		t.Fatal("acknowledgement gate bypassed", e)
	}
	if _, e = store.OwnerRecord(key); e != nil {
		t.Fatal("failed retirement erased receipt", e)
	}
	if e = store.RetireOwnerRecord(key, proof.CommitID, nil); e != nil {
		t.Fatal(e)
	}
	if e = store.RetireOwnerRecord(key, proof.CommitID, nil); e != nil {
		t.Fatal("retirement is not idempotent", e)
	}
	if _, e = store.OwnerRecord(key); e == nil {
		t.Fatal("acknowledged receipt not retired")
	}
}

func TestHistoryOwnerRecordsAfterCloseReturnReopenRequiredWithoutPanic(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if e = store.Close(); e != nil {
		t.Fatal(e)
	}
	if e = store.CommitHistoryOwnerRecords(baseline(t), map[string]json.RawMessage{"history-replay/late-worker": json.RawMessage(`{}`)}, nil); !errors.Is(e, ErrOwnerReopenRequired) {
		t.Fatal("closed store did not reject history receipt", e)
	}
}

func TestBaselineScopeAndRawOwnershipRecordsShareOneAtomicCommit(t *testing.T) {
	dir := t.TempDir()
	store, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	records := map[string]json.RawMessage{"online-replay/baseline-capture": json.RawMessage(`{"pins":"root-reference"}`)}
	if e = store.PrepareScopeRecordsCAS(unit, []byte(`{"scope":1}`), "job", json.RawMessage(`{"generation":1}`), "", records, nil); e != nil {
		store.Close()
		t.Fatal(e)
	}
	if _, e = store.OwnerRecord("online-replay/baseline-capture"); e == nil {
		store.Close()
		t.Fatal("raw owner proof visible before scope commit")
	}
	store.Close()
	store, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if e = store.ApplyPrepared(journal.Cursor{Topic: "scope-and-raw", Partition: 0, Offset: 2}); e != nil {
		t.Fatal(e)
	}
	scope, e := store.ScopeCommit("job")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := store.OwnerRecord("online-replay/baseline-capture")
	if e != nil || raw.CommitID != scope.CommitID || raw.Reference != scope.Reference {
		t.Fatal("baseline raw and scope proof not bound", e, scope, raw)
	}
}

func TestOwnerRecordsCanBindOnlyValidatedCompleteReorgCommit(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	base := baseline(t)
	if e = store.InstallBaseline(base); e != nil {
		t.Fatal(e)
	}
	tip := clone(base)
	tip.Reference.JournalSeq++
	tip.Reference.Block.Number++
	tip.Reference.Block.ParentHash = base.Reference.Block.Hash
	tip.Reference.Block.Hash = "0x" + strings.Repeat("a", 64)
	if e = store.Commit(tip); e != nil {
		t.Fatal(e)
	}
	begin := clone(tip)
	begin.Reference.JournalSeq++
	begin.Reference.CanonicalEpoch++
	if e = store.ApplyLocal(journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin}); e != nil {
		t.Fatal(e)
	}
	ancestor := clone(base)
	ancestor.Reference = begin.Reference
	ancestor.Reference.JournalSeq++
	ancestor.Reference.Block = base.Reference.Block
	next := clone(ancestor)
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = base.Reference.Block.Hash
	next.Reference.Block.Hash = "0x" + strings.Repeat("b", 64)
	if e = store.StageReorg([]domain.Snapshot{ancestor, next}, 1<<20); e != nil {
		t.Fatal(e)
	}
	unit := journal.Unit{Kind: "REORG_COMMIT", Snapshot: next}
	key := "online-replay/reorg-source-group"
	if e = store.ApplyLocalOwnerRecords(unit, map[string]json.RawMessage{key: json.RawMessage(`{"complete_branch":true}`)}, nil); e != nil {
		t.Fatal(e)
	}
	record, e := store.OwnerRecord(key)
	if e != nil || record.Kind != "REORG_COMMIT" || record.Reference != next.Reference {
		t.Fatal("raw record not same reorg owner batch", e, record)
	}
	if current, ready := store.Current(); !ready || current.Reference.Block.Hash != "0x"+strings.Repeat("b", 64) {
		t.Fatal("replacement not visible", ready, current.Reference)
	}
}
