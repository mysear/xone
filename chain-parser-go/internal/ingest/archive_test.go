package ingest

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestArchiveDurableOwnershipDLQAndCapacity(t *testing.T) {
	dir := t.TempDir()
	limits := ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 5, MaxRecordBytes: 4096}
	a, err := OpenArchive(dir, limits)
	if err != nil {
		t.Fatal(err)
	}
	if other, e := OpenArchive(dir, limits); e == nil {
		other.Close()
		t.Fatal("concurrent writer accepted")
	}
	owner := Owner{Chain: "evm:1", Height: 1, Hash: "h", ProviderID: "p"}
	ref, err := a.Put(owner, []byte("raw"))
	if err != nil {
		t.Fatal(err)
	}
	again, err := a.Put(owner, []byte("raw"))
	if err != nil || again != ref {
		t.Fatal(again, err)
	}
	if _, err := a.Put(owner, []byte("different")); !errors.Is(err, ErrOwnership) {
		t.Fatal(err)
	}
	otherOwner := owner
	otherOwner.ProviderID = "other"
	other, err := a.Put(otherOwner, []byte("raw"))
	if err != nil || other.ID == ref.ID || other.Digest != ref.Digest {
		t.Fatal(other, err)
	}
	entry := DeadLetter{Raw: ref, Stage: "decode", Code: "UNKNOWN_EVENT", Scope: "pool-a"}
	if err := a.Quarantine(entry); err != nil {
		t.Fatal(err)
	}
	if err := a.Quarantine(entry); err != nil {
		t.Fatal(err)
	}
	a.Close()
	a, err = OpenArchive(dir, limits)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	raw, err := a.Read(ref)
	if err != nil || string(raw) != "raw" {
		t.Fatal(string(raw), err)
	}
	entries, err := a.DeadLetters()
	if err != nil || len(entries) != 1 || entries[0].Scope != "pool-a" {
		t.Fatal(entries, err)
	}
	wrong := ref
	wrong.Owner.Hash = "forged"
	if _, err := a.Read(wrong); err == nil {
		t.Fatal("forged ownership")
	}
	owner.Height = 2
	if _, err := a.Put(owner, make([]byte, 4097)); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
	owner.Height = 3
	if _, err := a.Put(owner, []byte("last")); err != nil {
		t.Fatal(err)
	}
	owner.Height = 4
	if _, err := a.Put(owner, []byte("overflow")); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
}

func TestArchiveRejectsCorruptionAndRestartOverBudget(t *testing.T) {
	for _, mode := range []string{"corrupt", "capacity", "temporary"} {
		t.Run(mode, func(t *testing.T) {
			dir := t.TempDir()
			lim := ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 10, MaxRecordBytes: 4096}
			a, err := OpenArchive(dir, lim)
			if err != nil {
				t.Fatal(err)
			}
			ref, err := a.Put(Owner{Chain: "c", Hash: "h", ProviderID: "p"}, []byte("evidence"))
			if err != nil {
				t.Fatal(err)
			}
			a.Close()
			switch mode {
			case "corrupt":
				if err := os.WriteFile(filepath.Join(dir, ref.ID+".raw"), []byte("broken"), 0600); err != nil {
					t.Fatal(err)
				}
			case "capacity":
				lim.MaxBytes = 1
			case "temporary":
				os.WriteFile(filepath.Join(dir, ".pending-crash"), []byte("uncommitted"), 0600)
			}
			reopened, err := OpenArchive(dir, lim)
			if mode == "temporary" {
				if err != nil {
					t.Fatal(err)
				}
				defer reopened.Close()
				if _, err := reopened.Read(ref); err != nil {
					t.Fatal(err)
				}
			} else if err == nil {
				reopened.Close()
				t.Fatal("unsafe restart accepted")
			}
		})
	}
}

func TestArchiveFsyncFailureRequiresReopenAndRecoversRenamedRecord(t *testing.T) {
	dir := t.TempDir()
	lim := ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 10, MaxRecordBytes: 4096}
	a, err := OpenArchive(dir, lim)
	if err != nil {
		t.Fatal(err)
	}
	a.syncDir = func(string) error { return errors.New("disk fsync failure") }
	owner := Owner{Chain: "c", Hash: "h", ProviderID: "p"}
	if _, err := a.Put(owner, []byte("raw")); err == nil {
		t.Fatal("durability failure accepted")
	}
	if _, err := a.Put(owner, []byte("raw")); err == nil {
		t.Fatal("uncertain handle reused")
	}
	a.Close()
	a, err = OpenArchive(dir, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if _, err := a.Put(owner, []byte("raw")); err != nil {
		t.Fatal(err)
	}
}

func TestArchiveDLQCanRestartWithEscapedMetadata(t *testing.T) {
	dir := t.TempDir()
	lim := ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 10, MaxRecordBytes: 1}
	a, err := OpenArchive(dir, lim)
	if err != nil {
		t.Fatal(err)
	}
	owner := Owner{Chain: strings.Repeat("\x01", 512), Hash: strings.Repeat("\x01", 512), ProviderID: strings.Repeat("\x01", 512)}
	ref, err := a.Put(owner, []byte("r"))
	if err != nil {
		t.Fatal(err)
	}
	if err = a.Quarantine(DeadLetter{Raw: ref, Stage: "decode", Code: "failure", Message: strings.Repeat("\x01", 2048), Scope: strings.Repeat("\x01", 512)}); err != nil {
		t.Fatal(err)
	}
	a.Close()
	a, err = OpenArchive(dir, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if entries, err := a.DeadLetters(); err != nil || len(entries) != 1 {
		t.Fatal(entries, err)
	}
}

func TestArchiveRejectsUnroundtrippableOwnership(t *testing.T) {
	a, err := OpenArchive(t.TempDir(), ArchiveLimits{MaxBytes: 4096, MaxRecords: 5, MaxRecordBytes: 512})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if _, err := a.Put(Owner{Chain: "c", Hash: string([]byte{0xff}), ProviderID: "p"}, []byte("raw")); !errors.Is(err, ErrOwnership) {
		t.Fatal(err)
	}
}

func TestBoundedLocalArchiveSinkCapacitySurvivesReopen(t *testing.T) {
	a, err := OpenArchive(t.TempDir(), ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 100, MaxRecordBytes: 4096})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	ref, err := a.Put(Owner{Chain: "c", Hash: "h", ProviderID: "p"}, make([]byte, 2048))
	if err != nil {
		t.Fatal(err)
	}
	path := t.TempDir()
	sink, err := OpenBoundedLocalArchiveSink(path, LocalArchiveLimits{MaxBytes: 4096, MaxRecords: 2})
	if err != nil {
		t.Fatal(err)
	}
	m, err := a.ArchiveRaw(context.Background(), ref, ArchiveMetadata{Namespace: NamespaceHistory, DecoderVersion: "observation/1", CanonicalProof: "observation-only"}, sink)
	if err != nil {
		t.Fatal(err)
	}
	if err := sink.Close(); err != nil {
		t.Fatal(err)
	}
	sink, err = OpenBoundedLocalArchiveSink(path, LocalArchiveLimits{MaxBytes: 4096, MaxRecords: 2})
	if err != nil {
		t.Fatal(err)
	}
	defer sink.Close()
	if err := sink.Verify(context.Background(), m); err != nil {
		t.Fatal(err)
	}
	usage, err := sink.Status()
	if err != nil || usage.Records != 2 || usage.Bytes < 2048 {
		t.Fatal(usage, err)
	}
	other := m
	other.Raw.Owner.Height++
	other.Raw.ID = jsonID(other.Raw.Owner)
	if err := sink.Store(context.Background(), other, make([]byte, 2048)); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
	usage, _ = sink.Status()
	if !usage.Failed || usage.Records != 2 {
		t.Fatal(usage)
	}
}
