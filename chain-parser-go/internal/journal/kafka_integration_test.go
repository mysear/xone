//go:build integration

package journal

import (
	"context"
	"errors"
	"flag"
	"testing"
	"time"
)

var broker = flag.String("kafka-test-broker", "", "isolated Kafka broker")

func TestRealKafkaTransactionsAbortAndFencing(t *testing.T) {
	if *broker == "" {
		t.Fatal("dedicated test requires --kafka-test-broker")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 40*time.Second)
	defer cancel()
	open := func(topic, id string) *Kafka {
		k, e := OpenKafka(ctx, []string{*broker}, topic, id, 1024, 8<<20, 4096, 10*time.Second)
		if e != nil {
			t.Fatal(e)
		}
		t.Cleanup(k.Close)
		return k
	}
	old := open("test.fencing", "test.same-writer")
	s := fixture(t)
	cursor, e := old.Publish(ctx, Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, nil)
	if e != nil {
		t.Fatal(e)
	}
	if cursor.Offset < 1 {
		t.Fatal(cursor)
	}
	next := open("test.fencing", "test.same-writer")
	s.Reference.JournalSeq++
	if _, e = old.Publish(ctx, Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, nil); e == nil {
		t.Fatal("old producer was not fenced")
	}
	s.Reference.OnlineGeneration++
	s.Reference.WriterEpoch++
	if _, e = next.Publish(ctx, Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, nil); e != nil {
		t.Fatal(e)
	}
	latest, _, e := next.Tail(ctx)
	if e != nil || latest == nil || latest.Snapshot.Reference != s.Reference {
		t.Fatal(latest, e)
	}
	aborted := open("test.abort", "test.abort-writer")
	first := fixture(t)
	if _, e = aborted.Publish(ctx, Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: first}, nil); e != nil {
		t.Fatal(e)
	}
	second := first
	second.Reference.JournalSeq++
	calls := 0
	_, e = aborted.Publish(ctx, Unit{Kind: "BLOCK_COMMIT", Snapshot: second}, func(context.Context) error {
		calls++
		if calls == 2 {
			return errors.New("LEASE_EXPIRED")
		}
		return nil
	})
	if e == nil {
		t.Fatal("lease failure ignored")
	}
	replacement := open("test.abort", "test.abort-writer")
	latest, _, e = replacement.Tail(ctx)
	if e != nil || latest == nil || latest.Snapshot.Reference != first.Reference {
		t.Fatal("aborted transaction visible", latest, e)
	}
	if k, e := OpenKafka(ctx, []string{*broker}, "test.invalid-partitions", "reject", 1024, 8<<20, 4096, 10*time.Second); e == nil {
		k.Close()
		t.Fatal("partition expansion accepted")
	}
}
