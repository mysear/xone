//go:build integration

package journal

import (
	"context"
	"testing"
	"time"
)

func TestReadOnlyAnalyticsReaderDoesNotFenceOwner(t *testing.T) {
	if *broker == "" {
		t.Fatal("dedicated broker required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	owner, e := OpenKafka(ctx, []string{*broker}, "test.analytics", "test.analytics-owner", 1024, 8<<20, 4096, 5*time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer owner.Close()
	s := fixture(t)
	cursor, e := owner.Publish(ctx, Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, nil)
	if e != nil {
		t.Fatal(e)
	}
	reader, e := OpenReadOnly(ctx, []string{*broker}, "test.analytics", 8<<20, 4096)
	if e != nil {
		t.Fatal(e)
	}
	defer reader.Close()
	count := 0
	if e = reader.Consume(ctx, Cursor{Topic: "test.analytics"}, func(u Unit, c Cursor) error {
		count++
		if c != cursor {
			t.Fatal(c, cursor)
		}
		return nil
	}); e != nil || count != 1 {
		t.Fatal(count, e)
	}
	s.Reference.OnlineGeneration++
	s.Reference.JournalSeq++
	if _, e = owner.Publish(ctx, Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: s}, nil); e != nil {
		t.Fatal("reader fenced owner", e)
	}
	count = 0
	if e = reader.Consume(ctx, cursor, func(u Unit, c Cursor) error { count++; return nil }); e != nil || count != 1 {
		t.Fatal(count, e)
	}
}
