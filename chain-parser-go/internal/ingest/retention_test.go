package ingest

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestRetentionRequiresVerifiedCopyResolutionAndDurablePins(t *testing.T) {
	a, d, s := repairFixture(t)
	ctx := context.Background()
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	meta := ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "decoder-v2", CanonicalProof: "journal:7"}
	if err := a.ReclaimRaw(ctx, d.Raw, sink); !errors.Is(err, ErrRetentionBlocked) {
		t.Fatal(err)
	}
	m, err := a.ArchiveRaw(ctx, d.Raw, meta, sink)
	if err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(ctx, d.Raw, sink); !errors.Is(err, ErrRetentionBlocked) {
		t.Fatal(err)
	}
	if _, err := a.Replay(ctx, s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}); err != nil {
		t.Fatal(err)
	}
	if err := a.PinRaw(d.Raw, "checkpoint-1"); err != nil {
		t.Fatal(err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if err := a.ReclaimRaw(ctx, d.Raw, sink); !errors.Is(err, ErrRetentionBlocked) {
		t.Fatal(err)
	}
	if err := a.ReleaseRaw(d.Raw, "checkpoint-1"); err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(ctx, d.Raw, sink); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Read(d.Raw); !errors.Is(err, ErrRawReclaimed) {
		t.Fatal(err)
	}
	if err := sink.Verify(ctx, m); err != nil {
		t.Fatal(err)
	}
	entries, err := a.DeadLetters()
	if err != nil || len(entries) != 1 || entries[0] != d {
		t.Fatal(entries, err)
	}
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	raws, err := a.RawRecords()
	if err != nil || len(raws) != 1 || !raws[0].Reclaimed {
		t.Fatal(raws, err)
	}
	if _, err := a.Put(d.Raw.Owner, []byte("changed")); !errors.Is(err, ErrOwnership) {
		t.Fatal(err)
	}
}
func TestRetentionTamperedExternalContentPoisonsHandle(t *testing.T) {
	a, _, _ := repairFixture(t)
	ref, err := a.Put(Owner{Chain: "c", Hash: "h", ProviderID: "p"}, []byte("clean"))
	if err != nil {
		t.Fatal(err)
	}
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	m, err := a.ArchiveRaw(context.Background(), ref, ArchiveMetadata{Namespace: NamespaceHistory, DecoderVersion: "v1", CanonicalProof: "proof"}, sink)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sink.Directory(), m.ObjectKey()), []byte("tampered"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(context.Background(), ref, sink); err == nil {
		t.Fatal("tampered copy accepted")
	}
	if _, err := a.Read(ref); err == nil {
		t.Fatal("failed verification did not poison")
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if b, err := a.Read(ref); err != nil || string(b) != "clean" {
		t.Fatal(string(b), err)
	}
}

func TestRetentionCrashTombstoneIsRecovered(t *testing.T) {
	for _, syncFailure := range []int{1, 2} {
		t.Run(fmt.Sprint(syncFailure), func(t *testing.T) {
			a, _, _ := repairFixture(t)
			ref, err := a.Put(Owner{Chain: "clean", Hash: "h", ProviderID: "p"}, []byte("content"))
			if err != nil {
				t.Fatal(err)
			}
			sink, err := OpenLocalArchiveSink(t.TempDir())
			if err != nil {
				t.Fatal(err)
			}
			if _, err := a.ArchiveRaw(context.Background(), ref, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v1", CanonicalProof: "journal"}, sink); err != nil {
				t.Fatal(err)
			}
			count := 0
			a.syncDir = func(path string) error {
				count++
				if count == syncFailure {
					return errors.New("crash")
				}
				return directorySync(path)
			}
			if err := a.ReclaimRaw(context.Background(), ref, sink); err == nil {
				t.Fatal("injected crash ignored")
			}
			path, lim := a.Directory(), a.Limits()
			a.Close()
			a, err = OpenArchive(path, lim)
			if err != nil {
				t.Fatal(err)
			}
			defer a.Close()
			if _, err := a.Read(ref); !errors.Is(err, ErrRawReclaimed) {
				t.Fatal(err)
			}
			if _, err := os.Stat(filepath.Join(path, ref.ID+".raw")); !errors.Is(err, os.ErrNotExist) {
				t.Fatal(err)
			}
		})
	}
}
func TestRetentionContinuousBudgetAndUnknownCommit(t *testing.T) {
	a, d, s := repairFixture(t)
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	meta := ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v2", CanonicalProof: "journal"}
	if _, err := a.ArchiveRaw(context.Background(), d.Raw, meta, sink); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		return CommitReceipt{}, errors.New("unknown")
	}}); !errors.Is(err, ErrCommitUnknown) {
		t.Fatal(err)
	}
	for i := 0; i < 3; i++ {
		ref, err := a.Put(Owner{Chain: "clean", Height: uint64(i), Hash: "h", ProviderID: "p"}, make([]byte, 2048))
		if err != nil {
			t.Fatal(err)
		}
		if _, err := a.ArchiveRaw(context.Background(), ref, meta, sink); err != nil {
			t.Fatal(err)
		}
	}
	p := RetentionPolicy{TargetBytes: 1, MaxScan: 10, MaxReclaims: 1, Timeout: time.Second}
	ctx, cancel := context.WithCancel(context.Background())
	passes, total := 0, 0
	err = a.RunRetention(ctx, time.Millisecond, p, sink, func(_ context.Context, r RetentionReport) error {
		passes++
		total += r.Reclaimed
		if r.Reclaimed > 1 {
			t.Fatal("pass budget exceeded")
		}
		if total == 3 {
			cancel()
		}
		return nil
	})
	if !errors.Is(err, context.Canceled) || total != 3 || passes < 3 {
		t.Fatal(total, passes, err)
	}
	if _, err := a.Read(d.Raw); err != nil {
		t.Fatal("unknown commit raw deleted", err)
	}
}
func TestRetentionExternalManifestTamperAndSinkCrash(t *testing.T) {
	for _, mode := range []string{"manifest", "sink-sync"} {
		t.Run(mode, func(t *testing.T) {
			a, _, _ := repairFixture(t)
			ref, err := a.Put(Owner{Chain: "clean", Hash: "h", ProviderID: "p"}, []byte("raw"))
			if err != nil {
				t.Fatal(err)
			}
			sink, err := OpenLocalArchiveSink(t.TempDir())
			if err != nil {
				t.Fatal(err)
			}
			if mode == "sink-sync" {
				sink.syncDir = func(string) error { return errors.New("sync failure") }
			}
			m, err := a.ArchiveRaw(context.Background(), ref, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v1", CanonicalProof: "journal"}, sink)
			if mode == "sink-sync" {
				if err == nil {
					t.Fatal("sink sync accepted")
				}
			} else {
				if err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(sink.Directory(), m.ManifestKey()), []byte("{}"), 0600); err != nil {
					t.Fatal(err)
				}
				if err := a.ReclaimRaw(context.Background(), ref, sink); err == nil {
					t.Fatal("bad manifest accepted")
				}
			}
			if _, _, err := a.Usage(); err == nil {
				t.Fatal("failed archive/verification did not poison")
			}
			path, lim := a.Directory(), a.Limits()
			a.Close()
			a, err = OpenArchive(path, lim)
			if err != nil {
				t.Fatal(err)
			}
			defer a.Close()
			if _, err := a.Read(ref); err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestRetentionBoundedPassDoesNotStarveBehindUnresolvedDLQ(t *testing.T) {
	a, _, _ := repairFixture(t)
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 3; i++ {
		ref, err := a.Put(Owner{Chain: "clean", Height: uint64(i), Hash: "h", ProviderID: "p"}, make([]byte, 2048))
		if err != nil {
			t.Fatal(err)
		}
		if _, err := a.ArchiveRaw(context.Background(), ref, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v1", CanonicalProof: "journal"}, sink); err != nil {
			t.Fatal(err)
		}
	}
	reclaimed := 0
	for i := 0; i < 8; i++ {
		r, err := a.RetainOnce(context.Background(), RetentionPolicy{TargetBytes: 1, MaxScan: 1, MaxReclaims: 1, Timeout: time.Second}, sink)
		if err != nil {
			t.Fatal(err)
		}
		if r.Scanned > 1 {
			t.Fatal("scan budget", r)
		}
		reclaimed += r.Reclaimed
	}
	if reclaimed != 3 {
		t.Fatal("unresolved oldest DLQ starved maintenance", reclaimed)
	}
}

func TestRetentionAtomicPutPinAndArchivedReadAfterRestart(t *testing.T) {
	a, _, _ := repairFixture(t)
	ctx := context.Background()
	ref, err := a.PutPinned(Owner{Chain: "clean", Hash: "h", ProviderID: "p"}, []byte("public raw"), "pipeline/block-h")
	if err != nil {
		t.Fatal(err)
	}
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	m, err := a.ArchiveRaw(ctx, ref, ArchiveMetadata{Namespace: NamespaceHistory, DecoderVersion: "decoder-v1", CanonicalProof: "public header"}, sink)
	if err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(ctx, ref, sink); !errors.Is(err, ErrRetentionBlocked) {
		t.Fatal(err)
	}
	if err := a.ReleaseRaw(ref, "pipeline/block-h"); err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(ctx, ref, sink); err != nil {
		t.Fatal(err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	restored, err := a.ArchivedManifest(ref)
	if err != nil || restored != m {
		t.Fatal(restored, err)
	}
	b, err := sink.Read(ctx, restored, 64)
	if err != nil || string(b) != "public raw" {
		t.Fatal(string(b), err)
	}
	if _, err := sink.Read(ctx, restored, 2); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
	if _, err := a.PutPinned(ref.Owner, []byte("public raw"), "old-task"); !errors.Is(err, ErrRawReclaimed) {
		t.Fatal(err)
	}
}
func TestRetentionReservedMaintenanceCapacity(t *testing.T) {
	a, err := OpenArchive(t.TempDir(), ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 5, MaxRecordBytes: 4096, ReservedBytes: 16 << 10, ReservedRecords: 2})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	var first RawRef
	for i := 0; i < 3; i++ {
		ref, err := a.Put(Owner{Chain: "c", Height: uint64(i), Hash: "h", ProviderID: "p"}, make([]byte, 2048))
		if err != nil {
			t.Fatal(err)
		}
		if i == 0 {
			first = ref
		}
	}
	if _, err := a.Put(Owner{Chain: "c", Height: 4, Hash: "h", ProviderID: "p"}, []byte("x")); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := a.ArchiveRaw(context.Background(), first, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v1", CanonicalProof: "journal"}, sink); err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(context.Background(), first, sink); err != nil {
		t.Fatal(err)
	}
}

func TestRetentionRejectsTamperedResolutionBeforeDelete(t *testing.T) {
	a, d, s := repairFixture(t)
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := a.ArchiveRaw(context.Background(), d.Raw, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v2", CanonicalProof: "journal"}, sink); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}); err != nil {
		t.Fatal(err)
	}
	for _, event := range a.events {
		if event.Replay != nil && event.Replay.Status == Resolved {
			if err := os.WriteFile(filepath.Join(a.directory, jsonID(event)+".event"), []byte("tampered"), 0600); err != nil {
				t.Fatal(err)
			}
		}
	}
	if err := a.ReclaimRaw(context.Background(), d.Raw, sink); !errors.Is(err, ErrCorrupt) {
		t.Fatal("corrupt resolution permitted GC", err)
	}
	if _, err := os.Stat(filepath.Join(a.directory, d.Raw.ID+".raw")); err != nil {
		t.Fatal("raw removed", err)
	}
}

func TestLocalArchiveSinkExistingCopyStillRequiresDirectorySync(t *testing.T) {
	a, _, _ := repairFixture(t)
	ref, err := a.Put(Owner{Chain: "clean", Hash: "h", ProviderID: "p"}, []byte("raw"))
	if err != nil {
		t.Fatal(err)
	}
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	m, err := a.ArchiveRaw(context.Background(), ref, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v1", CanonicalProof: "journal"}, sink)
	if err != nil {
		t.Fatal(err)
	}
	injected := errors.New("directory sync failed on existing copy")
	sink.syncDir = func(string) error { return injected }
	if err := sink.Store(context.Background(), m, []byte("raw")); !errors.Is(err, injected) {
		t.Fatal("existing file bypassed durability barrier", err)
	}
	if err := sink.Verify(context.Background(), m); err == nil {
		t.Fatal("failed sink reused")
	}
}

func TestRetentionP1RepeatedFailureBlocksPreviouslyResolvedRaw(t *testing.T) {
	a, d, s := repairFixture(t)
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := a.ArchiveRaw(context.Background(), d.Raw, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v2", CanonicalProof: "journal"}, sink); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}); err != nil {
		t.Fatal(err)
	}
	if err := a.Quarantine(d); err != nil {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(context.Background(), d.Raw, sink); !errors.Is(err, ErrRetentionBlocked) {
		t.Fatal("old resolution allowed raw GC", err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if err := a.ReclaimRaw(context.Background(), d.Raw, sink); !errors.Is(err, ErrRetentionBlocked) {
		t.Fatal("restart allowed raw GC", err)
	}
	if _, err := a.Read(d.Raw); err != nil {
		t.Fatal(err)
	}
}
