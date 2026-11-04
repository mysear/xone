//go:build integration

package control

import (
	"context"
	"flag"
	"os"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/domain"
)

var dsnFile = flag.String("control-test-dsn-file", "", "permission-restricted test DSN file")

func TestPostgresLeaseFencingAndAudit(t *testing.T) {
	if *dsnFile == "" {
		t.Fatal("dedicated integration test requires --control-test-dsn-file")
	}
	b, e := os.ReadFile(*dsnFile)
	if e != nil {
		t.Fatal("test DSN unavailable")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	p, e := Open(ctx, strings.TrimSpace(string(b)))
	if e != nil {
		t.Fatal(e)
	}
	defer p.Close()
	chain := "evm:90001"
	if e = p.Project(ctx, chain, "chain-parser-go", 1, "hash", "test"); e != nil {
		t.Fatal(e)
	}
	if e = p.Project(ctx, chain, "chain-parser-rust", 1, "hash", "test"); e == nil {
		t.Fatal("owner handoff silently accepted")
	}
	old, e := p.Acquire(ctx, chain, "chain-parser-go", 1, "old", 2000)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = p.Acquire(ctx, chain, "chain-parser-go", 1, "new", 2000); e == nil {
		t.Fatal("simultaneous leader")
	}
	if e = p.Check(ctx, old, 100); e != nil {
		t.Fatal(e)
	}
	old, e = p.AllocateBaseline(ctx, old, domain.SnapshotRef{WriterEpoch: old.WriterEpoch + 100, OnlineGeneration: 50, CanonicalEpoch: 7, JournalSeq: 99})
	if e != nil || old.Generation != 51 || old.JournalSeq != 99 {
		t.Fatal(old, e)
	}
	p.Release(ctx, old)
	if _, e = p.Renew(ctx, old, 2000); e == nil {
		t.Fatal("expired lease resurrected")
	}
	next, e := p.Acquire(ctx, chain, "chain-parser-go", 1, "new", 2000)
	if e != nil || next.WriterEpoch <= old.WriterEpoch {
		t.Fatal(next, e)
	}
	if e = p.Check(ctx, old, 0); e == nil {
		t.Fatal("old holder accepted")
	}
	p.Release(ctx, next)
}
