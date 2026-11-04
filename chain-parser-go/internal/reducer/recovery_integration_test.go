//go:build integration

package reducer

import (
	"context"
	"flag"
	"testing"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

var recoveryBroker = flag.String("reducer-kafka-broker", "", "isolated Kafka broker")

func TestRealJournalCrashBoundaries(t *testing.T) {
	if *recoveryBroker == "" {
		t.Fatal("dedicated integration requires --reducer-kafka-broker")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	open := func() *journal.Kafka {
		k, e := journal.OpenKafka(ctx, []string{*recoveryBroker}, "test.crash", "test.crash-writer", 1024, 8<<20, 4096, 5*time.Second)
		if e != nil {
			t.Fatal(e)
		}
		t.Cleanup(k.Close)
		return k
	}
	dir := t.TempDir()
	state, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline(t)}
	if e = state.Prepare(unit); e != nil {
		t.Fatal(e)
	}
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer func() { state.Close() }()
	if _, ready := state.Current(); ready {
		t.Fatal("prepared cursor made ready")
	}
	producer := open()
	cursor, e := producer.Publish(ctx, unit, nil)
	if e != nil {
		t.Fatal(e)
	}
	producer.Close()
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	replacement := open()
	committed, proof, e := replacement.Tail(ctx)
	if e != nil || committed == nil || committed.Snapshot.Reference != unit.Snapshot.Reference {
		t.Fatal(committed, e)
	}
	if proof != cursor {
		t.Fatal("commit proof cursor", proof, cursor)
	}
	if e = state.ApplyPrepared(proof); e != nil {
		t.Fatal(e)
	}
	boot, e := state.Bootstrap()
	if e != nil || boot.Snapshot.Reference != unit.Snapshot.Reference {
		t.Fatal(e)
	}
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if _, ready := state.Current(); ready {
		t.Fatal("durable old online state automatically ready")
	}
	pending, e := state.Prepared()
	if e != nil || pending != nil {
		t.Fatal("applied commit still prepared")
	}
	if state.DurableRef().JournalSeq != unit.Snapshot.Reference.JournalSeq {
		t.Fatal("applied seq lost")
	}
	next := unit
	next.Snapshot.Reference.OnlineGeneration++
	next.Snapshot.Reference.WriterEpoch++
	next.Snapshot.Reference.JournalSeq++
	if e = state.Prepare(next); e != nil {
		t.Fatal(e)
	}
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if _, ready := state.Current(); ready {
		t.Fatal("uncommitted next generation exposed")
	}
	if e = state.DiscardPrepared(); e != nil {
		t.Fatal(e)
	}
	if state.DurableRef() != unit.Snapshot.Reference {
		t.Fatal("discard changed applied state")
	}
}

func TestRealJournalReorgCommitCrashRetainsBarrierAndUndo(t *testing.T) {
	if *recoveryBroker == "" {
		t.Fatal("dedicated Kafka broker required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	producer, e := journal.OpenKafka(ctx, []string{*recoveryBroker}, "test.reorg", "test.reorg-writer", 1024, 8<<20, 4096, 5*time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer producer.Close()
	dir := t.TempDir()
	state, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer func() { state.Close() }()
	apply := func(u journal.Unit) {
		t.Helper()
		if e := state.Prepare(u); e != nil {
			t.Fatal(e)
		}
		cursor, e := producer.Publish(ctx, u, nil)
		if e != nil {
			t.Fatal(e)
		}
		if e = state.ApplyPrepared(cursor); e != nil {
			t.Fatal(e)
		}
	}
	s := baseline(t)
	apply(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s})
	tip := clone(s)
	tip.Reference.Block.Number++
	tip.Reference.Block.ParentHash = s.Reference.Block.Hash
	tip.Reference.Block.Hash = "0x2222222222222222222222222222222222222222222222222222222222222222"
	tip.Reference.JournalSeq++
	apply(journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: tip})
	revoked := clone(tip)
	revoked.Reference.JournalSeq++
	apply(journal.Unit{Kind: "ONLINE_SCOPE_REVOKED", Snapshot: revoked})
	begin := clone(revoked)
	begin.Reference.CanonicalEpoch++
	begin.Reference.JournalSeq++
	apply(journal.Unit{Kind: "REORG_BEGIN", Snapshot: begin})
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if !state.ReorgPending() {
		t.Fatal("barrier not durable")
	}
	if _, e = state.Bootstrap(); e == nil {
		t.Fatal("barrier bootstrap visible")
	}
	ancestor := clone(s)
	ancestor.Reference = begin.Reference
	ancestor.Reference.JournalSeq++
	ancestor.Reference.Block = s.Reference.Block
	replacement := clone(ancestor)
	replacement.Reference.Block = tip.Reference.Block
	replacement.Reference.Block.Hash = "0x3333333333333333333333333333333333333333333333333333333333333333"
	replacement.Pools[0].Reserve0 = "900000"
	if e = state.StageReorg([]domain.Snapshot{ancestor, replacement}, 8<<20); e != nil {
		t.Fatal(e)
	}
	unit := journal.Unit{Kind: "REORG_COMMIT", Snapshot: replacement}
	if e = state.Prepare(unit); e != nil {
		t.Fatal(e)
	}
	cursor, e := producer.Publish(ctx, unit, nil)
	if e != nil {
		t.Fatal(e)
	}
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	if _, ready := state.Current(); ready {
		t.Fatal("prepared reorg made ready")
	}
	committed, proof, e := producer.Tail(ctx)
	if e != nil || committed == nil || committed.Kind != "REORG_COMMIT" || proof != cursor {
		t.Fatal(committed, proof, e)
	}
	if e = state.ApplyPrepared(proof); e != nil {
		t.Fatal(e)
	}
	boot, e := state.Bootstrap()
	if e != nil || boot.Cursor != cursor || boot.Snapshot.Reference != replacement.Reference {
		t.Fatal(boot, e)
	}
	if _, e = state.UndoParent(replacement); e != nil {
		t.Fatal("replacement undo lost on crash", e)
	}
}

func TestRealNormalizedFactPreparedWireRecovery(t *testing.T) {
	if *recoveryBroker == "" {
		t.Fatal("dedicated broker required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	producer, e := journal.OpenKafka(ctx, []string{*recoveryBroker}, "test.fact-crash", "test.fact-crash-writer", 1024, 8<<20, 4096, 5*time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer producer.Close()
	dir := t.TempDir()
	state, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer func() { state.Close() }()
	s := baseline(t)
	if e = state.InstallBaseline(s); e != nil {
		t.Fatal(e)
	}
	next := clone(s)
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = s.Reference.Block.Hash
	next.Reference.Block.Hash = "0x2222222222222222222222222222222222222222222222222222222222222222"
	unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: next.Reference.Block}}}}
	if e = state.Prepare(unit); e != nil {
		t.Fatal(e)
	}
	cursor, e := producer.Publish(ctx, unit, nil)
	if e != nil {
		t.Fatal(e)
	}
	state.Close()
	state, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	pending, e := state.Prepared()
	if e != nil || pending == nil {
		t.Fatal(e)
	}
	committed, proof, e := producer.Tail(ctx)
	if e != nil || committed == nil || proof != cursor {
		t.Fatal(e)
	}
	a, e := pending.CanonicalHash()
	if e != nil {
		t.Fatal(e)
	}
	b, e := committed.CanonicalHash()
	if e != nil || a != b {
		t.Fatal("protobuf nil-list roundtrip falsely conflicts", e)
	}
	if e = state.ApplyPrepared(proof); e != nil {
		t.Fatal(e)
	}
	boot, e := state.Bootstrap()
	if e != nil || boot.Snapshot.Reference != next.Reference {
		t.Fatal(e)
	}
}
