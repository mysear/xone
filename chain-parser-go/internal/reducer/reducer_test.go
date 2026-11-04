package reducer

import (
	"encoding/json"
	"fmt"
	"github.com/cockroachdb/pebble"
	"os"
	"testing"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func baseline(t *testing.T) domain.Snapshot {
	t.Helper()
	b, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s domain.Snapshot
	if e = json.Unmarshal(b, &s); e != nil {
		t.Fatal(e)
	}
	return s
}
func TestAtomicReplayFencingAndHistoryIsolation(t *testing.T) {
	s := baseline(t)
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	next := s
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = s.Reference.Block.Hash
	next.Reference.Block.Hash = "block101"
	next.Pools = append([]domain.Pool(nil), s.Pools...)
	next.Pools[0].Reserve0 = "1100000"
	if e = store.Commit(next); e != nil {
		t.Fatal(e)
	}
	if e = store.Commit(next); e != nil {
		t.Fatal("duplicate", e)
	}
	bad := next
	bad.Reference.WriterEpoch = 0
	if e = store.Commit(bad); e == nil {
		t.Fatal("old writer accepted")
	}
	if e = store.WriteHistory(s); e != nil {
		t.Fatal(e)
	}
	got, _ := store.Current()
	if got.Reference.Block.Hash != "block101" {
		t.Fatal("history wrote through")
	}
	if e = store.RevertTo(s.Reference.Block.Hash, 2); e != nil {
		t.Fatal(e)
	}
	got, _ = store.Current()
	if got.Pools[0].Reserve0 != s.Pools[0].Reserve0 || got.Reference.CanonicalEpoch != 2 {
		t.Fatal("undo failed")
	}
}
func TestRecoveryAndOldBaselineRejected(t *testing.T) {
	dir := t.TempDir()
	s := baseline(t)
	store, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	store.Close()
	store, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if _, ok := store.Current(); ok {
		t.Fatal("old cursor made ready")
	}
	s.Reference.OnlineGeneration = 2
	s.Reference.WriterEpoch = 2
	s.Reference.JournalSeq = 2
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	s.Reference.OnlineGeneration = 1
	if e = store.InstallBaseline(s); e == nil {
		t.Fatal("old generation accepted")
	}
}
func TestPreparedIsDurableAndNotReady(t *testing.T) {
	dir := t.TempDir()
	s := baseline(t)
	store, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	u := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}
	if e = store.Prepare(u); e != nil {
		t.Fatal(e)
	}
	if _, ok := store.Current(); ok {
		t.Fatal("prepared became visible")
	}
	store.Close()
	store, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	pending, e := store.Prepared()
	if e != nil || pending == nil {
		t.Fatal(e)
	}
	cursor := journal.Cursor{Topic: "canonical.test", Partition: 0, Offset: 4}
	if e = store.ApplyPrepared(cursor); e != nil {
		t.Fatal(e)
	}
	boot, e := store.Bootstrap()
	if e != nil || boot.Cursor != cursor || boot.Snapshot.Reference != s.Reference {
		t.Fatal(boot, e)
	}
	pending, e = store.Prepared()
	if e != nil || pending != nil {
		t.Fatal("prepared not removed")
	}
}
func TestExpiredAuthorityCannotBecomeReadyAfterPause(t *testing.T) {
	s := baseline(t)
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	store.SetAuthorityDeadline(time.Now().Add(-time.Second))
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	if _, ready := store.Current(); ready {
		t.Fatal("expired writer advertised ready")
	}
}

func TestReorgBarrierDurableAndCommitBoundToBegin(t *testing.T) {
	dir := t.TempDir()
	s := baseline(t)
	store, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	begin := clone(s)
	begin.Reference.CanonicalEpoch++
	begin.Reference.JournalSeq++
	if e = store.Prepare(journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin}); e != nil {
		t.Fatal(e)
	}
	if _, ok := store.Current(); !ok {
		t.Fatal("uncommitted begin changed visibility")
	}
	if e = store.ApplyPrepared(journal.Cursor{Topic: "test", Offset: 4}); e != nil {
		t.Fatal(e)
	}
	if _, ok := store.Current(); ok {
		t.Fatal("begin remained ready")
	}
	if _, e = store.Bootstrap(); e == nil {
		t.Fatal("barrier bootstrap visible")
	}
	if store.DurableRef() != begin.Reference {
		t.Fatal("begin cursor not durable")
	}
	store.Close()
	store, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if _, ok := store.Current(); ok {
		t.Fatal("restarted barrier ready")
	}
	commit := clone(begin)
	commit.Reference.JournalSeq++
	commit.Reference.Block.Hash = "new-branch"
	commit.Pools[0].Reserve0 = "900000"
	bad := clone(commit)
	bad.Reference.CanonicalEpoch++
	if e = store.Prepare(journal.Unit{Kind: "REORG_COMMIT", Snapshot: bad}); e == nil {
		t.Fatal("wrong epoch accepted")
	}
	if e = store.Prepare(journal.Unit{Kind: "REORG_COMMIT", Snapshot: commit}); e != nil {
		t.Fatal(e)
	}
	if e = store.ApplyPrepared(journal.Cursor{Topic: "test", Offset: 8}); e != nil {
		t.Fatal(e)
	}
	got, ok := store.Current()
	if !ok || got.Reference != commit.Reference || got.Pools[0].Reserve0 != "900000" {
		t.Fatal("commit not atomic")
	}
}

func TestDescendingUndoAndReplacementBranchRetention(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 4)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	s := baseline(t)
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	tip := clone(s)
	tip.Reference.JournalSeq++
	tip.Reference.Block.Number++
	tip.Reference.Block.ParentHash = s.Reference.Block.Hash
	tip.Reference.Block.Hash = "old101"
	if e = store.Commit(tip); e != nil {
		t.Fatal(e)
	}
	parent, e := store.UndoParent(tip)
	if e != nil || parent.Reference.Block != s.Reference.Block {
		t.Fatal(parent, e)
	}
	corrupt := clone(tip)
	corrupt.Reference.Block.Number++
	if _, e = store.UndoParent(corrupt); e == nil {
		t.Fatal("non-parent undo accepted")
	}
	begin := clone(tip)
	begin.Reference.JournalSeq++
	begin.Reference.CanonicalEpoch++
	if e = store.ApplyLocal(journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin}); e != nil {
		t.Fatal(e)
	}
	ancestor := clone(s)
	ancestor.Reference = begin.Reference
	ancestor.Reference.JournalSeq++
	ancestor.Reference.Block = s.Reference.Block
	next := clone(ancestor)
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = s.Reference.Block.Hash
	next.Reference.Block.Hash = "new101"
	if e = store.StageReorg([]domain.Snapshot{ancestor, next}, 1<<20); e != nil {
		t.Fatal(e)
	}
	if _, ok := store.Current(); ok {
		t.Fatal("staged branch visible")
	}
	if e = store.ApplyLocal(journal.Unit{Kind: "REORG_COMMIT", Snapshot: next}); e != nil {
		t.Fatal(e)
	}
	parent, e = store.UndoParent(next)
	if e != nil || parent.Reference.Block != s.Reference.Block {
		t.Fatal("replacement undo missing", e)
	}
}

func TestAdvancingReorgPrunesUndoWithinWindow(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 3)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	s := baseline(t)
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	tip := clone(s)
	tip.Reference.JournalSeq++
	tip.Reference.Block.Number++
	tip.Reference.Block.ParentHash = s.Reference.Block.Hash
	tip.Reference.Block.Hash = "initial101"
	if e = store.Commit(tip); e != nil {
		t.Fatal(e)
	}
	for fork := 0; fork < 8; fork++ {
		parent, e := store.UndoParent(tip)
		if e != nil {
			t.Fatal(e)
		}
		begin := clone(tip)
		begin.Reference.CanonicalEpoch++
		begin.Reference.JournalSeq++
		if e = store.ApplyLocal(journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin}); e != nil {
			t.Fatal(e)
		}
		ancestor := clone(parent)
		ancestor.Reference = begin.Reference
		ancestor.Reference.JournalSeq++
		ancestor.Reference.Block = parent.Reference.Block
		first := clone(ancestor)
		first.Reference.Block.Number++
		first.Reference.Block.ParentHash = ancestor.Reference.Block.Hash
		first.Reference.Block.Hash = fmt.Sprintf("fork%d-first", fork)
		last := clone(first)
		last.Reference.Block.Number++
		last.Reference.Block.ParentHash = first.Reference.Block.Hash
		last.Reference.Block.Hash = fmt.Sprintf("fork%d-last", fork)
		if e = store.StageReorg([]domain.Snapshot{ancestor, first, last}, 1<<20); e != nil {
			t.Fatal(e)
		}
		if e = store.ApplyLocal(journal.Unit{Kind: "REORG_COMMIT", Snapshot: last}); e != nil {
			t.Fatal(e)
		}
		tip = last
	}
	iter, e := store.db.NewIter(&pebble.IterOptions{LowerBound: []byte("undo/"), UpperBound: []byte("undo0")})
	if e != nil {
		t.Fatal(e)
	}
	defer iter.Close()
	count := 0
	for iter.First(); iter.Valid(); iter.Next() {
		var state domain.Snapshot
		if e = json.Unmarshal(iter.Value(), &state); e != nil {
			t.Fatal(e)
		}
		if state.Reference.Block.Number < tip.Reference.Block.Number-store.window {
			t.Fatal("undo below retention", state.Reference.Block.Number)
		}
		count++
	}
	if count > 4 {
		t.Fatal("unbounded retained undo", count)
	}
}

func TestEmptyHistoryGapListKeepsCrossLanguageArrayShape(t *testing.T) {
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	s := baseline(t)
	s.HistoryGaps = []domain.Gap{}
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	current, _ := store.Current()
	raw, e := json.Marshal(current)
	if e != nil {
		t.Fatal(e)
	}
	var body map[string]json.RawMessage
	if e = json.Unmarshal(raw, &body); e != nil {
		t.Fatal(e)
	}
	if string(body["history_gaps"]) != "[]" {
		t.Fatal("empty history gaps must be array", string(body["history_gaps"]))
	}
}

func TestWeightedCurrentSnapshotCannotMutateStoredState(t *testing.T) {
	s := baseline(t)
	raw, e := os.ReadFile("../../test/contracts/weighted-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Pools []domain.Pool `json:"pools"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	s.Pools = data.Pools
	s.Reference.ScopeHash, e = domain.ScopeHash(s.Pools)
	if e != nil {
		t.Fatal(e)
	}
	versions := []string{s.Pools[0].AdapterVersion}
	s.Reference.AdapterSetHash, e = domain.Hash(versions)
	if e != nil {
		t.Fatal(e)
	}
	store, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	if e = store.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	before, _ := domain.Hash(s)
	got, ready := store.Current()
	if !ready {
		t.Fatal("missing baseline")
	}
	got.Pools[0].Weighted.Weight0 = "1"
	got.Pools[0].Weighted.Scaling0 = "1"
	got.Pools[0].Weighted.Vault = "mutated"
	after, ready := store.Current()
	hash, _ := domain.Hash(after)
	if !ready || hash != before {
		t.Fatal("returned snapshot mutated durable state")
	}
}
