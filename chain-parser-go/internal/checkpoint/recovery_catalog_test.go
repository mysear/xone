package checkpoint

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func recoveryHead(boot journal.Bootstrap) (domain.SnapshotRef, journal.Cursor) {
	ref := boot.Snapshot.Reference
	ref.JournalSeq++
	ref.Block.Number++
	ref.Block.ParentHash = ref.Block.Hash
	ref.Block.Hash = "0x" + strings.Repeat("3", 64)
	ref.Block.TimestampMS++
	cursor := boot.Cursor
	cursor.Offset += 4
	return ref, cursor
}
func TestRecoveryCatalogQualifiesCompletedOlderCheckpointAndBindsWideHead(t *testing.T) {
	_, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 5*time.Second)
	boot := bootstrapFixture(t)
	dir := t.TempDir()
	p, e := WriteBootstrap(dir, boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = remote.RecoveryCatalog(dir, boot.Snapshot.Reference, boot.Cursor, 100); e == nil {
		t.Fatal("unuploaded snapshot qualified")
	}
	if _, e = remote.Publish(context.Background(), p, nil); e != nil {
		t.Fatal(e)
	}
	head, cursor := recoveryHead(boot)
	catalog, e := remote.RecoveryCatalog(dir, head, cursor, 100)
	if e != nil {
		t.Fatal(e)
	}
	if catalog.Checkpoint.ManifestSHA256 != p.ManifestSHA256 || catalog.HeadReference != head || catalog.HeadCursor.NextOffset != "9007199254741013" {
		t.Fatal(catalog)
	}
	if _, e = remote.Catalog(dir, journal.Bootstrap{Snapshot: domain.Snapshot{Reference: head, Pools: boot.Snapshot.Pools, Source: boot.Snapshot.Source, HistoryGaps: boot.Snapshot.HistoryGaps}, Cursor: cursor}); e == nil {
		t.Fatal("exact-current catalog lied about old checkpoint")
	}
	if _, e = remote.RecoveryCatalog(dir, head, cursor, 3); e == nil {
		t.Fatal("tail over budget qualified")
	}
}
func TestRecoveryCatalogRejectsEraCursorConflictCorruptionAndMissingObject(t *testing.T) {
	_, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 5*time.Second)
	boot := bootstrapFixture(t)
	dir := t.TempDir()
	p, e := WriteBootstrap(dir, boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = remote.Publish(context.Background(), p, nil); e != nil {
		t.Fatal(e)
	}
	for _, mode := range []string{"owner", "writer", "canonical", "generation", "scope", "adapter", "assignment", "topic", "offset", "seq", "same-seq-conflict", "head-hash", "same-height", "zero-budget"} {
		t.Run(mode, func(t *testing.T) {
			head, cursor := recoveryHead(boot)
			limit := 100
			switch mode {
			case "owner":
				head.OwnerService = "chain-parser-rust"
			case "writer":
				head.WriterEpoch++
			case "canonical":
				head.CanonicalEpoch++
			case "generation":
				head.OnlineGeneration++
			case "scope":
				head.ScopeHash = strings.Repeat("a", 64)
			case "adapter":
				head.AdapterSetHash = strings.Repeat("a", 64)
			case "assignment":
				head.OwnerAssignmentVersion++
			case "topic":
				cursor.Topic = "another.topic"
			case "offset":
				cursor.Offset = boot.Cursor.Offset
			case "seq":
				head.JournalSeq = boot.Snapshot.Reference.JournalSeq - 1
			case "same-seq-conflict":
				head.JournalSeq = boot.Snapshot.Reference.JournalSeq
			case "head-hash":
				head.Block.Hash = "not-a-hash"
			case "same-height":
				head.Block.Number = boot.Snapshot.Reference.Block.Number
			case "zero-budget":
				limit = 0
			}
			if _, e := remote.RecoveryCatalog(dir, head, cursor, limit); e == nil {
				t.Fatal(mode)
			}
		})
	}
	hint := filepath.Join(dir, "public-v2", remote.candidateName())
	original, e := os.ReadFile(hint)
	if e != nil {
		t.Fatal(e)
	}
	var changed RemotePublication
	if e = json.Unmarshal(original, &changed); e != nil {
		t.Fatal(e)
	}
	changed.ManifestURL = "https://other.example/manifest.json"
	raw, _ := json.Marshal(changed)
	if e = os.WriteFile(hint, raw, 0600); e != nil {
		t.Fatal(e)
	}
	if _, e = remote.RecoveryCatalog(dir, boot.Snapshot.Reference, boot.Cursor, 100); e == nil {
		t.Fatal("unconfigured hint location trusted")
	}
	if e = os.WriteFile(hint, original, 0600); e != nil {
		t.Fatal(e)
	}
	if e = os.Remove(filepath.Join(filepath.Dir(p.ManifestPath), "bootstrap.pb")); e != nil {
		t.Fatal(e)
	}
	if _, e = remote.RecoveryCatalog(dir, boot.Snapshot.Reference, boot.Cursor, 100); e == nil {
		t.Fatal("missing payload qualified")
	}
}
