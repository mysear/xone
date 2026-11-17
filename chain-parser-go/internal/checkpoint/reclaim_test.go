package checkpoint

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

func controlledReclaimAuthority() error { return nil }

type failingReclaimTransport struct{}

func (failingReclaimTransport) RoundTrip(*http.Request) (*http.Response, error) {
	return nil, errors.New("controlled object store unavailable")
}
func reclaimPolicy() config.CheckpointRetention {
	policy := retentionPolicy()
	policy.ReclaimEnabled = true
	policy.PlanTTLMS = 300000
	return policy
}
func reclaimPlan(t *testing.T, remote *Remote, dir string, boot journal.Bootstrap, policy config.CheckpointRetention, now time.Time) RetentionPlan {
	t.Helper()
	plan, e := remote.PlanRetention(dir, boot, policy, now, nil)
	if e != nil {
		t.Fatal(e)
	}
	return plan
}
func applyRequest(plan RetentionPlan) RetentionApplyRequest {
	return RetentionApplyRequest{PlanID: plan.PlanID, ObservedAtMS: plan.ObservedAtMS}
}
func selectedIDs(plan RetentionPlan) map[string]bool {
	selected := map[string]bool{}
	for _, row := range plan.Objects {
		if row.Selected {
			selected[row.ObjectSHA256] = true
		}
	}
	return selected
}
func TestCheckpointReclaimExecutesOnlySelectedConfirmedOldCopiesAndIsDurablyIdempotent(t *testing.T) {
	remote, dir, pubs, boots, now := retentionFixture(t)
	policy := reclaimPolicy()
	plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
	if plan.SelectedObjects != 2 {
		t.Fatal(plan.SelectedObjects)
	}
	result, e := remote.ApplyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, controlledReclaimAuthority)
	if e != nil || !result.Applied || result.Status != "COMPLETE" || result.CompletedObjects != 2 || result.CompletedBytes != plan.SelectedBytes || result.TotalObjects != 2 {
		t.Fatal(result, e)
	}
	selected := selectedIDs(plan)
	for _, p := range pubs {
		if selected[p.Manifest.SHA256] {
			if _, e = os.Stat(filepath.Dir(p.ManifestPath)); !errors.Is(e, os.ErrNotExist) {
				t.Fatal("selected original directory not reclaimed", e)
			}
			restored, e := remote.Fetch(context.Background(), p.Manifest.SHA256, p.ManifestSHA256)
			if e != nil || restored.Snapshot.Reference != p.Manifest.Reference {
				t.Fatal("remote original cannot restore deleted local copy", e)
			}
		} else if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal("protected checkpoint removed", e)
		}
	}
	reopened := remoteFixture(t, remote.base, 5*time.Second)
	again, e := reopened.ApplyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now.Add(time.Hour), controlledReclaimAuthority)
	if e != nil || again.CompletedObjects != 2 || again.TotalObjects != 2 || !again.Applied {
		t.Fatal("duplicate apply changed durable totals", again, e)
	}
	status, e := reopened.ReclaimStatus(dir)
	if e != nil || status.TotalBytes != result.TotalBytes || status.TotalObjects != 2 {
		t.Fatal(status, e)
	}
	// The control ledger is bounded to one operation and a cumulative receipt.
	files, e := os.ReadDir(filepath.Join(dir, "retirements"))
	if e != nil || len(files) != 1 || files[0].Name() != "state.json" {
		t.Fatal(files, e)
	}
	if output := os.Getenv("XONE_CHECKPOINT_RECLAIM_OUTPUT"); output != "" {
		os.MkdirAll(output, 0700)
		raw, e := json.MarshalIndent(map[string]any{"plan": plan, "result": result, "reopened_status": status, "checks": []string{"selected_only", "newest_current_candidates_preserved", "remote_bytes_restore_deleted_copy", "idempotent_duplicate_after_reopen", "bounded_single_control_ledger"}}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(output, "reclaim-actual.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
}
func TestCheckpointReclaimRejectsDisabledExpiredStaleSourceNamespaceAndUnavailableBackup(t *testing.T) {
	for _, mode := range []string{"disabled", "expired", "future-time", "wrong-plan", "changed-source", "changed-namespace", "remote-unavailable", "cancelled"} {
		t.Run(mode, func(t *testing.T) {
			remote, dir, pubs, boots, now := retentionFixture(t)
			policy := reclaimPolicy()
			plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
			request := applyRequest(plan)
			boot := boots[5]
			ctx := context.Background()
			at := now
			switch mode {
			case "disabled":
				policy.ReclaimEnabled = false
				policy.PlanTTLMS = 0
			case "expired":
				at = now.Add(time.Duration(policy.PlanTTLMS+1) * time.Millisecond)
			case "future-time":
				request.ObservedAtMS = now.Add(time.Second).UnixMilli()
			case "wrong-plan":
				request.PlanID = strings.Repeat("a", 64)
			case "changed-source":
				boot.Snapshot.Reference.OnlineGeneration++
				boot.Snapshot.Reference.JournalSeq++
				boot.Cursor.Offset += 4
			case "changed-namespace":
				if e := os.WriteFile(filepath.Join(dir, "public-v2", "forensic-note"), []byte("keep"), 0600); e != nil {
					t.Fatal(e)
				}
			case "remote-unavailable":
				remote.client.CloseIdleConnections()
				remote.client.Transport = failingReclaimTransport{}
			case "cancelled":
				cancelled, cancel := context.WithCancel(ctx)
				cancel()
				ctx = cancelled
			}
			_, e := remote.ApplyRetention(ctx, dir, boot, policy, request, at, controlledReclaimAuthority)

			if e == nil {
				t.Fatal("invalid operation accepted", mode)
			}
			for _, p := range pubs {
				if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
					t.Fatal("failed precondition removed data", mode, e)
				}
			}
		})
	}
}
func TestCheckpointReclaimFaultedPhasesResumeWithoutReselectingOrDoubleCounting(t *testing.T) {
	for _, event := range []string{"intent_persisted", "before_rename", "after_rename", "quarantine_persisted", "after_remove:bootstrap.pb", "directory_removed", "object_committed", "before_complete_commit", "complete_persisted"} {
		t.Run(event, func(t *testing.T) {
			remote, dir, pubs, boots, now := retentionFixture(t)
			policy := reclaimPolicy()
			plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
			injected := errors.New("SIMULATED_PROCESS_INTERRUPTION")
			hit := false
			interruptedResult, e := remote.applyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, nil, func(point string) error {
				if point == event && !hit {
					hit = true
					return injected
				}
				return nil
			})
			if !hit || !errors.Is(e, injected) {
				t.Fatal("fault point was not executed", event, hit, e)
			}
			if event == "before_complete_commit" && (interruptedResult.Applied || interruptedResult.ReceiptConfirmed || interruptedResult.Status != "COMPLETION_UNCONFIRMED") {
				t.Fatal("uncommitted completion was qualified", interruptedResult)
			}
			if event == "complete_persisted" && (!interruptedResult.Applied || !interruptedResult.ReceiptConfirmed) {
				t.Fatal("durable but undelivered completion was lost", interruptedResult)
			}
			status, e := remote.ReclaimStatus(dir)
			if e != nil || status.OperationID != plan.PlanID {
				t.Fatal(status, e)
			}
			reopened := remoteFixture(t, remote.base, 5*time.Second)
			fresh := boots[5]
			fresh.Snapshot.Reference.OnlineGeneration++
			fresh.Snapshot.Reference.JournalSeq++
			fresh.Snapshot.Reference.WriterEpoch++
			fresh.Cursor.Offset += 4
			result, e := reopened.ResumeRetention(context.Background(), dir, fresh, policy, RetentionResumeRequest{OperationID: plan.PlanID}, now.Add(time.Second), controlledReclaimAuthority)
			if e != nil || !result.Applied || result.TotalObjects != 2 || result.CompletedBytes != plan.SelectedBytes {
				t.Fatal(event, result, e)
			}
			for _, p := range pubs {
				if !selectedIDs(plan)[p.Manifest.SHA256] {
					if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
						t.Fatal("resume removed unselected data", e)
					}
				}
			}
			again, e := reopened.ResumeRetention(context.Background(), dir, fresh, policy, RetentionResumeRequest{OperationID: plan.PlanID}, now.Add(time.Second), controlledReclaimAuthority)
			if e != nil || again.TotalObjects != 2 {
				t.Fatal("resume double counted", again, e)
			}
		})
	}
}

func TestCheckpointReclaimResumeRechecksProtectedReferencesPolicyAndQuarantinedBytes(t *testing.T) {
	for _, mode := range []string{"new-candidate", "stricter-retention", "corrupt-payload", "unexpected-file", "remote-failure", "fenced-owner", "corrupt-journal", "quarantine-link"} {
		t.Run(mode, func(t *testing.T) {
			remote, dir, _, boots, now := retentionFixture(t)
			policy := reclaimPolicy()
			plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
			fault := errors.New("STOP_AFTER_RENAME")
			_, e := remote.applyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, nil, func(point string) error {
				if point == "after_rename" {
					return fault
				}
				return nil
			})
			if !errors.Is(e, fault) {
				t.Fatal(e)
			}
			state, _, e := remote.loadReclaim(dir, 1<<20)
			if e != nil {
				t.Fatal(e)
			}
			object := state.Objects[0]
			quarantine := filepath.Join(dir, "retirements", state.OperationID, object.Manifest.SHA256)
			before, e := os.ReadDir(quarantine)
			if e != nil {
				t.Fatal(e)
			}
			boot := boots[5]
			switch mode {
			case "new-candidate":
				p := remote.publication(Publication{ManifestSHA256: object.ManifestSHA256}, object.Manifest)
				raw, _ := json.Marshal(p)
				if e = os.WriteFile(filepath.Join(dir, "public-v2", "candidate-"+strings.Repeat("a", 64)+".json"), raw, 0600); e != nil {
					t.Fatal(e)
				}
			case "stricter-retention":
				policy.KeepLatest = 6
			case "corrupt-payload":
				if e = os.WriteFile(filepath.Join(quarantine, "bootstrap.pb"), []byte("different bytes"), 0600); e != nil {
					t.Fatal(e)
				}
			case "unexpected-file":
				if e = os.WriteFile(filepath.Join(quarantine, "FORENSIC_DO_NOT_DELETE"), []byte("unknown"), 0600); e != nil {
					t.Fatal(e)
				}
			case "remote-failure":
				remote.client.CloseIdleConnections()
				remote.client.Transport = failingReclaimTransport{}
			case "fenced-owner":
				boot.Snapshot.Reference.WriterEpoch = 0
			case "corrupt-journal":
				if e = os.WriteFile(filepath.Join(dir, "retirements", "state.json"), []byte("corrupt"), 0600); e != nil {
					t.Fatal(e)
				}
			case "quarantine-link":
				outside := t.TempDir()
				if e = os.WriteFile(filepath.Join(outside, "preserved"), []byte("external"), 0600); e != nil {
					t.Fatal(e)
				}
				original := quarantine + "-original"
				if e = os.Rename(quarantine, original); e != nil {
					t.Fatal(e)
				}
				if e = os.Symlink(outside, quarantine); e != nil {
					t.Fatal(e)
				}
			}
			_, e = remote.ResumeRetention(context.Background(), dir, boot, policy, RetentionResumeRequest{OperationID: plan.PlanID}, now.Add(time.Second), controlledReclaimAuthority)
			if e == nil {
				t.Fatal("changed pending operation resumed destructively", mode)
			}
			if mode == "quarantine-link" {
				raw, e := os.ReadFile(filepath.Join(quarantine, "preserved"))
				if e != nil || string(raw) != "external" {
					t.Fatal(e)
				}
				return
			}
			after, e := os.ReadDir(quarantine)
			if e != nil || len(after) < len(before) {
				t.Fatal("rejected resume removed more files", mode, len(before), len(after), e)
			}
		})
	}
}

func TestCheckpointReclaimBoundsResourcesCleansOnlyEmptyOrphansAndKeepsCumulativeReceipt(t *testing.T) {
	remote, dir, pubs, boots, now := retentionFixture(t)
	policy := reclaimPolicy()
	// A crashed pre-intent writer may leave an empty generated job or temp journal.
	ledger := filepath.Join(dir, "retirements")
	if e := os.MkdirAll(filepath.Join(ledger, strings.Repeat("b", 64)), 0700); e != nil {
		t.Fatal(e)
	}
	if e := os.WriteFile(filepath.Join(ledger, ".reclaim-staging-"+strings.Repeat("a", 32)), []byte("uncommitted journal bytes"), 0600); e != nil {
		t.Fatal(e)
	}
	for i := 0; i < 2; i++ {
		at := now.Add(time.Duration(i) * time.Second)
		plan := reclaimPlan(t, remote, dir, boots[5], policy, at)
		result, e := remote.ApplyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), at, controlledReclaimAuthority)
		if e != nil || !result.Applied || result.TotalObjects != uint64(2*(i+1)) {
			t.Fatal(result, e)
		}
	}
	entries, e := os.ReadDir(ledger)
	if e != nil || len(entries) != 1 || entries[0].Name() != "state.json" {
		t.Fatal("unbounded control ledger", entries, e)
	}
	for _, p := range pubs[4:] {
		if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal(e)
		}
	}
	// Unknown nonempty job directories never become recursive deletion targets.
	unrelated := filepath.Join(ledger, strings.Repeat("c", 64))
	if e = os.Mkdir(unrelated, 0700); e != nil {
		t.Fatal(e)
	}
	keep := filepath.Join(unrelated, "keep")
	if e = os.WriteFile(keep, []byte("operator evidence"), 0600); e != nil {
		t.Fatal(e)
	}
	plan := reclaimPlan(t, remote, dir, boots[5], policy, now.Add(3*time.Second))
	if _, e = remote.ApplyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now.Add(3*time.Second), controlledReclaimAuthority); e == nil {
		t.Fatal("unknown nonempty job was accepted")
	}
	if raw, e := os.ReadFile(keep); e != nil || string(raw) != "operator evidence" {
		t.Fatal("unknown evidence removed", e)
	}
}
func TestCheckpointReclaimPendingBudgetsAndSourceCancellationPreserveJournalAndCopies(t *testing.T) {
	for _, mode := range []string{"read-limit", "scan-limit", "source-cancel", "wrong-operation"} {
		t.Run(mode, func(t *testing.T) {
			remote, dir, _, boots, now := retentionFixture(t)
			policy := reclaimPolicy()
			plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
			pause := errors.New("INTERRUPT_AFTER_QUARANTINE")
			if _, e := remote.applyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, nil, func(point string) error {
				if point == "quarantine_persisted" {
					return pause
				}
				return nil
			}); !errors.Is(e, pause) {
				t.Fatal(e)
			}
			state, _, e := remote.loadReclaim(dir, 1<<20)
			if e != nil {
				t.Fatal(e)
			}
			path := filepath.Join(dir, "retirements", plan.PlanID, state.Objects[0].Manifest.SHA256)
			original, e := os.ReadDir(path)
			if e != nil {
				t.Fatal(e)
			}
			guard := func() error { return nil }
			operation := plan.PlanID
			switch mode {
			case "read-limit":
				policy.MaxReadBytes = 1
			case "scan-limit":
				policy.MaxObjects = 2
				policy.MaxEntries = 2
			case "source-cancel":
				guard = func() error { return context.Canceled }
			case "wrong-operation":
				operation = strings.Repeat("d", 64)
			}
			if _, e = remote.ResumeRetention(context.Background(), dir, boots[5], policy, RetentionResumeRequest{OperationID: operation}, now, guard); e == nil {
				t.Fatal(mode)
			}
			after, e := os.ReadDir(path)
			if e != nil || len(after) != len(original) {
				t.Fatal("rejected resume consumed more copies", mode, e)
			}
			status, e := remote.ReclaimStatus(dir)
			if e != nil || status.Applied {
				t.Fatal(status, e)
			}
		})
	}
}

func TestCheckpointReclaimPublicEntryCannotRunWithoutAuthorityGuard(t *testing.T) {
	remote, dir, _, boots, now := retentionFixture(t)
	policy := reclaimPolicy()
	plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
	if _, e := remote.ApplyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, nil); e == nil || e.Error() != "CHECKPOINT_RECLAIM_AUTHORITY_REQUIRED" {
		t.Fatal(e)
	}
	if _, e := remote.ResumeRetention(context.Background(), dir, boots[5], policy, RetentionResumeRequest{OperationID: plan.PlanID}, now, nil); e == nil || e.Error() != "CHECKPOINT_RECLAIM_AUTHORITY_REQUIRED" {
		t.Fatal(e)
	}
	if _, e := os.Stat(filepath.Join(dir, "retirements")); !errors.Is(e, os.ErrNotExist) {
		t.Fatal("missing guard touched control ledger", e)
	}
}

func TestCheckpointReclaimStoreRotationCannotResumeOldPendingDataButCanStartAfterCompletion(t *testing.T) {
	remote, dir, _, boots, now := retentionFixture(t)
	policy := reclaimPolicy()
	plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
	stop := errors.New("STOP_PENDING")
	if _, e := remote.applyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, nil, func(point string) error {
		if point == "after_rename" {
			return stop
		}
		return nil
	}); !errors.Is(e, stop) {
		t.Fatal(e)
	}
	if pending, e := HasPendingReclaim(dir); e != nil || !pending {
		t.Fatal(pending, e)
	}
	changed := remoteFixture(t, remote.base+"/new-store", 5*time.Second)
	if _, e := changed.ResumeRetention(context.Background(), dir, boots[5], policy, RetentionResumeRequest{OperationID: plan.PlanID}, now, controlledReclaimAuthority); e == nil || e.Error() != "CHECKPOINT_RECLAIM_STORE_CHANGED" {
		t.Fatal(e)
	}
	result, e := remote.ResumeRetention(context.Background(), dir, boots[5], policy, RetentionResumeRequest{OperationID: plan.PlanID}, now, controlledReclaimAuthority)
	if e != nil || !result.Applied {
		t.Fatal(result, e)
	}
	if pending, e := HasPendingReclaim(dir); e != nil || pending {
		t.Fatal(pending, e)
	}
	next := reclaimPlan(t, changed, dir, boots[5], policy, now.Add(time.Second))
	if next.SelectedObjects != 0 {
		t.Fatal("old certificate qualified the new store")
	}
	migrated, e := changed.ApplyRetention(context.Background(), dir, boots[5], policy, applyRequest(next), now.Add(time.Second), controlledReclaimAuthority)
	if e != nil || !migrated.Applied || migrated.CompletedObjects != 0 || migrated.TotalObjects != 2 {
		t.Fatal("completed old receipt prevents controlled store rotation", migrated, e)
	}
}
