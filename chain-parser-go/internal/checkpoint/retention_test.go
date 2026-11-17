package checkpoint

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

func retentionFixture(t *testing.T) (*Remote, string, []Publication, []journal.Bootstrap, time.Time) {
	return retentionFixtureAt(t, "")
}
func retentionFixtureAt(t *testing.T, output string) (*Remote, string, []Publication, []journal.Bootstrap, time.Time) {
	t.Helper()
	_, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 5*time.Second)
	directory := t.TempDir()
	if output != "" {
		directory = filepath.Join(output, "objects")
		if e := os.MkdirAll(directory, 0700); e != nil {
			t.Fatal(e)
		}
	}
	now := time.Now().UTC().Truncate(time.Millisecond)
	old := now.Add(-48 * time.Hour)
	var publications []Publication
	var boots []journal.Bootstrap
	for i := 0; i < 6; i++ {
		boot := bootstrapFixture(t)
		boot.Snapshot.Reference.JournalSeq = uint64(i + 1)
		boot.Snapshot.Reference.OnlineGeneration = uint64(i + 1)
		boot.Snapshot.Reference.Block.Number += uint64(i)
		boot.Snapshot.Reference.Block.TimestampMS = uint64(old.UnixMilli())
		boot.Snapshot.Reference.Block.Hash = fmt.Sprintf("0x%064x", i+1)
		boot.Cursor.Offset += int64(4 * i)
		p, e := WriteBootstrap(directory, boot, 8<<20, nil)
		if e != nil {
			t.Fatal(e)
		}
		if _, e = remote.Publish(context.Background(), p, nil); e != nil {
			t.Fatal(e)
		}
		if e = os.Chtimes(p.ManifestPath, old, old); e != nil {
			t.Fatal(e)
		}
		publications = append(publications, p)
		boots = append(boots, boot)
	}
	return remote, directory, publications, boots, now
}
func retentionPolicy() config.CheckpointRetention {
	return config.CheckpointRetention{KeepLatest: 2, MinAgeMS: 3600000, MaxRemove: 2, MaxObjects: 32, MaxEntries: 256, MaxReadBytes: 1 << 20}
}
func TestCheckpointRetentionPlanProtectsCurrentCandidatesAndNewestBackupsWithoutMutation(t *testing.T) {
	output := os.Getenv("XONE_CHECKPOINT_RETENTION_VECTORS_OUTPUT")
	remote, directory, pubs, boots, now := retentionFixtureAt(t, output)
	policy := retentionPolicy()
	// A prior configured store still references the oldest object: protect every candidate.
	other := remoteFixture(t, remote.base+"/other", 5*time.Second)
	if _, e := other.Publish(context.Background(), pubs[0], nil); e != nil {
		t.Fatal(e)
	}
	plan, e := remote.PlanRetention(directory, boots[5], policy, now, nil)
	if e != nil {
		t.Fatal(e)
	}
	if plan.Applied || plan.PlanID == "" || plan.NamespaceSHA256 == "" || len(plan.Objects) != 6 || plan.SelectedObjects != 2 {
		t.Fatal(plan)
	}
	selected := map[uint64]bool{}
	reasons := map[uint64]string{}
	for _, object := range plan.Objects {
		if object.Reference == nil {
			t.Fatal(object)
		}
		reasons[object.Reference.JournalSeq] = object.Reason
		if object.Selected {
			selected[object.Reference.JournalSeq] = true
		}
	}
	if !selected[2] || !selected[3] || selected[1] || selected[5] || selected[6] || reasons[1] != "CANDIDATE_REFERENCE" || reasons[5] != "LATEST_BACKUP" || reasons[6] != "CURRENT_STATE" {
		t.Fatal(selected, reasons)
	}
	same, e := remote.PlanRetention(directory, boots[5], policy, now, nil)
	if e != nil || same.PlanID != plan.PlanID || same.NamespaceSHA256 != plan.NamespaceSHA256 {
		t.Fatal("read-only plan changed filesystem or encoding", e)
	}
	for _, p := range pubs {
		if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal("planning deleted checkpoint", e)
		}
	}

	if output != "" {
		raw, e := json.MarshalIndent(map[string]any{"base_url": remote.base, "bootstrap": boots[5], "policy": policy, "observed_at_ms": now.UnixMilli(), "plan": plan}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(output, "retention-vector.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
	if output := os.Getenv("XONE_CHECKPOINT_RETENTION_PLAN_OUTPUT"); output != "" {
		if e = os.MkdirAll(output, 0700); e != nil {
			t.Fatal(e)
		}
		raw, e := json.MarshalIndent(map[string]any{"plan": plan, "repeat_plan_id": same.PlanID, "all_original_checkpoints_still_decode": true, "verification_scope": "SIX_CURRENT_GENERATED_CHECKPOINTS_TWO_STORE_CANDIDATES_READ_ONLY_PLAN"}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(output, "six-object-retention-plan.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
}
func TestCheckpointRetentionPlanBindsOwnerAndNamespaceAndHonorsAgeAndLimits(t *testing.T) {
	remote, directory, pubs, boots, now := retentionFixture(t)
	policy := retentionPolicy()
	original, e := remote.PlanRetention(directory, boots[5], policy, now, nil)
	if e != nil {
		t.Fatal(e)
	}
	if e = os.Chtimes(pubs[0].ManifestPath, now, now); e != nil {
		t.Fatal(e)
	}
	changed, e := remote.PlanRetention(directory, boots[5], policy, now, nil)
	if e != nil || changed.NamespaceSHA256 == original.NamespaceSHA256 || changed.PlanID == original.PlanID {
		t.Fatal(e)
	}
	for _, row := range changed.Objects {
		if row.ObjectSHA256 == pubs[0].Manifest.SHA256 && (row.Selected || row.Reason != "AGE_PROTECTED") {
			t.Fatal(row)
		}
	}
	next := boots[5]
	next.Snapshot.Reference.JournalSeq++
	next.Snapshot.Reference.OnlineGeneration++
	next.Cursor.Offset += 4
	different, e := remote.PlanRetention(directory, next, policy, now, nil)
	if e != nil || different.PlanID == changed.PlanID {
		t.Fatal("plan lost source binding", e)
	}
	for _, mode := range []string{"read", "scan", "objects"} {
		limited := policy
		switch mode {
		case "read":
			limited.MaxReadBytes = 1
		case "scan":
			limited.MaxObjects = 6
			limited.MaxEntries = 6
		case "objects":
			limited.MaxObjects = 2
		}
		if _, e = remote.PlanRetention(directory, boots[5], limited, now, nil); e == nil {
			t.Fatal("unbounded plan", mode)
		}
	}
	denied := errors.New("OWNER_REVOKED")
	if _, e = remote.PlanRetention(directory, boots[5], policy, now, func() error { return denied }); !errors.Is(e, denied) {
		t.Fatal(e)
	}
	calls := 0
	if _, e = remote.PlanRetention(directory, boots[5], policy, now, func() error {
		calls++
		if calls >= 4 {
			return denied
		}
		return nil
	}); !errors.Is(e, denied) {
		t.Fatal("scan ignored cancellation", e)
	}
}
func TestCheckpointRetentionPlanRejectsCorruptCertificatesPayloadsAndCandidateLinks(t *testing.T) {
	for _, mode := range []string{"certificate", "payload", "candidate", "link", "invalid-name"} {
		t.Run(mode, func(t *testing.T) {
			remote, directory, pubs, boots, now := retentionFixture(t)
			switch mode {
			case "certificate":
				if e := os.WriteFile(filepath.Join(filepath.Dir(pubs[0].ManifestPath), remote.completionName()), []byte("bad"), 0600); e != nil {
					t.Fatal(e)
				}
			case "payload":
				if e := os.WriteFile(filepath.Join(filepath.Dir(pubs[0].ManifestPath), "bootstrap.pb"), []byte("corrupt"), 0600); e != nil {
					t.Fatal(e)
				}
			case "candidate":
				if e := os.WriteFile(filepath.Join(directory, "public-v2", remote.candidateName()), []byte("bad"), 0600); e != nil {
					t.Fatal(e)
				}
			case "invalid-name":
				if e := os.WriteFile(filepath.Join(directory, "public-v2", string([]byte{0xff})), nil, 0600); e != nil {
					if errors.Is(e, syscall.EILSEQ) {
						t.Skip("filesystem already rejects invalid UTF-8 before the planner can observe it")
					}
					t.Fatal(e)
				}
			case "link":
				path := filepath.Join(filepath.Dir(pubs[0].ManifestPath), "bootstrap.pb")
				outside := filepath.Join(t.TempDir(), "outside")
				raw, _ := os.ReadFile(path)
				os.WriteFile(outside, raw, 0600)
				os.Remove(path)
				if e := os.Symlink(outside, path); e != nil {
					t.Fatal(e)
				}
			}
			if _, e := remote.PlanRetention(directory, boots[5], retentionPolicy(), now, nil); e == nil {
				t.Fatal("corrupt plan qualified", mode)
			}
			if _, e := os.Stat(pubs[0].ManifestPath); e != nil {
				t.Fatal("failure deleted forensic object", e)
			}
		})
	}
}
