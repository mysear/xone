package analysis

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func snapshot(t *testing.T) domain.Snapshot {
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
func applyUnit(t *testing.T, s *JournalState, u journal.Unit) {
	t.Helper()
	if e := s.Apply(u, journal.Cursor{Topic: "test", Offset: int64(u.Snapshot.Reference.JournalSeq) * 4}); e != nil {
		t.Fatal(e)
	}
}
func swap(s domain.Snapshot, price string) domain.FactBlock {
	b := s.Reference.Block
	return domain.FactBlock{Block: b, Swaps: []domain.SwapFact{{EventID: domain.SwapEventID(b, 0, 0), TxHash: "0x" + strings.Repeat("a", 64), PoolID: s.Pools[0].PoolID, Token0: s.Pools[0].Token0, Token1: s.Pools[0].Token1, Amount0In: "1", Amount1In: "0", Amount0Out: "0", Amount1Out: domain.Amount(price)}}}
}
func TestJournalReorgRecomputesOHLCAndPublishesDeletedWindow(t *testing.T) {
	b := snapshot(t)
	state := NewJournalState("evm:31337", "chain-parser-go", 1, 60000)
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b})
	next := b
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = b.Reference.Block.Hash
	next.Reference.Block.Hash = "0x" + strings.Repeat("2", 64)
	f := swap(next, "100")
	u := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{f}}}
	applyUnit(t, state, u)
	before := state.Release()
	if !before.Publishable || len(before.Candles) != 1 || before.Candles[0].High != "100" {
		t.Fatal(before)
	}
	applyUnit(t, state, u)
	if state.Release().Candles[0].Revision != before.Candles[0].Revision {
		t.Fatal("duplicate changed revision")
	}
	begin := next
	begin.Reference.CanonicalEpoch++
	begin.Reference.JournalSeq++
	applyUnit(t, state, journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin})
	if state.Release().Publishable || len(state.Release().Candles) != 0 {
		t.Fatal("half reorg published")
	}
	commit := begin
	commit.Reference.JournalSeq++
	commit.Reference.Block.Hash = "0x" + strings.Repeat("3", 64)
	facts := &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: commit.Reference.Block, Swaps: []domain.SwapFact{}}}, RevertedBlocks: []domain.BlockRef{next.Reference.Block}}
	applyUnit(t, state, journal.Unit{Kind: "REORG_COMMIT", Snapshot: commit, NormalizedFacts: facts})
	release := state.Release()
	if !release.Publishable || len(release.Candles) != 1 || !release.Candles[0].Deleted || release.Candles[0].Count != 0 || release.Candles[0].Revision <= before.Candles[0].Revision {
		t.Fatal(release)
	}
	file := t.TempDir() + "/checkpoint.json"
	if e := state.Save(file, 1<<20); e != nil {
		t.Fatal(e)
	}
	restored, e := LoadJournalState(file, 1<<20, "evm:31337", "chain-parser-go", 1, 60000)
	if e != nil {
		t.Fatal(e)
	}
	if restored.Cursor != state.Cursor || restored.Release().Candles[0].Revision != release.Candles[0].Revision {
		t.Fatal("checkpoint mismatch")
	}
	bad := commit
	bad.Reference.JournalSeq++
	bad.Reference.Block.Number++
	bad.Reference.Block.ParentHash = commit.Reference.Block.Hash
	if e = restored.Apply(journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: bad}, journal.Cursor{Topic: "test", Offset: 999}); e == nil {
		t.Fatal("missing facts treated complete")
	}
	if restored.Cursor != state.Cursor {
		t.Fatal("failed unit moved checkpoint")
	}
}

func TestBaselineTakeoverDoesNotLoseEarlierReorgFacts(t *testing.T) {
	b := snapshot(t)
	state := NewJournalState("evm:31337", "chain-parser-go", 1, 60000)
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b})
	next := b
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = b.Reference.Block.Hash
	next.Reference.Block.Hash = "0x" + strings.Repeat("2", 64)
	applyUnit(t, state, journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{swap(next, "100")}}})
	takeover := next
	takeover.Reference.JournalSeq++
	takeover.Reference.OnlineGeneration++
	takeover.Reference.WriterEpoch++
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: takeover})
	if len(state.Release().Candles) != 1 {
		t.Fatal("takeover lost valid prior candle")
	}
	recovered := takeover
	recovered.Reference.JournalSeq++
	recovered.Reference.OnlineGeneration++
	recovered.Reference.WriterEpoch++
	recovered.Reference.CanonicalEpoch++
	recovered.Reference.Block.Hash = "0x" + strings.Repeat("3", 64)
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: recovered})
	if !state.Release().Candles[0].Deleted {
		t.Fatal("old generation facts survived unknown reorg")
	}
}
func TestCheckpointLimitCannotAdvanceInMemoryOrPublishedCursor(t *testing.T) {
	b := snapshot(t)
	state := NewJournalState("evm:31337", "chain-parser-go", 1, 60000)
	file := t.TempDir() + "/journal.json"
	if e := state.ApplyAndSave(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b}, journal.Cursor{Topic: "test", Offset: 4}, file, 1<<20); e != nil {
		t.Fatal(e)
	}
	before, _ := os.ReadFile(file)
	next := b
	next.Reference.JournalSeq++
	next.Reference.OnlineGeneration++
	if e := state.ApplyAndSave(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: next}, journal.Cursor{Topic: "test", Offset: 8}, file, 100); e == nil {
		t.Fatal("unbounded checkpoint")
	}
	after, _ := os.ReadFile(file)
	if string(before) != string(after) || state.Reference.JournalSeq != b.Reference.JournalSeq {
		t.Fatal("failed save advanced state")
	}
}

func TestSameEpochBaselineAfterBeginRetractsUnreconciledTrades(t *testing.T) {
	b := snapshot(t)
	state := NewJournalState("evm:31337", "chain-parser-go", 1, 60000)
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b})
	next := b
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = b.Reference.Block.Hash
	next.Reference.Block.Hash = "0x" + strings.Repeat("2", 64)
	applyUnit(t, state, journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{swap(next, "100")}}})
	begin := next
	begin.Reference.JournalSeq++
	begin.Reference.CanonicalEpoch++
	applyUnit(t, state, journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin})
	recovered := begin
	recovered.Reference.OnlineGeneration++
	recovered.Reference.WriterEpoch++
	recovered.Reference.JournalSeq++
	recovered.Reference.Block.Hash = "0x" + strings.Repeat("3", 64)
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: recovered})
	release := state.Release()
	if !release.Publishable || !release.Candles[0].Deleted || len(state.Trades) != 0 {
		t.Fatal("unreconciled orphan facts republished", release)
	}
}

func TestCandidateExhaustionCanPersistUnpublishableDurableRelease(t *testing.T) {
	b := snapshot(t)
	state := NewJournalState("evm:31337", "chain-parser-go", 1, 60000)
	path := t.TempDir() + "/checkpoint.json"
	if e := state.ApplyAndSave(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b}, journal.Cursor{Topic: "test", Offset: 4}, path, 1<<20); e != nil {
		t.Fatal(e)
	}
	raw, _ := os.ReadFile(path)
	budget := len(raw) + 512
	if e := state.Save(path, budget); e != nil {
		t.Fatal("reserved initial state", e)
	}
	next := b
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = b.Reference.Block.Hash
	next.Reference.Block.Hash = "0x" + strings.Repeat("2", 64)
	unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{swap(next, "100")}}}
	if e := state.ApplyAndSave(unit, journal.Cursor{Topic: "test", Offset: 8}, path, budget); e == nil {
		t.Fatal("candidate did not exceed budget")
	}
	state.Ready = false
	state.SuspendedReason = "ANALYTICS_CHECKPOINT_LIMIT"
	if e := state.Save(path, budget); e != nil {
		t.Fatal("exhaustion prevented suspension", e)
	}
	restored, e := LoadJournalState(path, budget, "evm:31337", "chain-parser-go", 1, 60000)
	if e != nil {
		t.Fatal(e)
	}
	if restored.Release().Publishable || restored.Cursor.Offset != 4 {
		t.Fatal("oversized candidate published")
	}
}

func TestSuccessfulRecheckClearsTransientFailureWithoutInventingEvents(t *testing.T) {
	b := snapshot(t)
	state := NewJournalState("evm:31337", "chain-parser-go", 1, 60000)
	applyUnit(t, state, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: b})
	old := state.Cursor
	state.Ready = false
	state.SuspendedReason = "ANALYTICS_FETCH"
	state.ConfirmReadComplete()
	if !state.Release().Publishable || state.Cursor != old {
		t.Fatal("validated unchanged journal could not recover transient failure")
	}
	state.Pending = "REORG_BEGIN"
	state.Ready = false
	state.ConfirmReadComplete()
	if state.Release().Publishable {
		t.Fatal("empty poll cleared pending reorg")
	}
}
