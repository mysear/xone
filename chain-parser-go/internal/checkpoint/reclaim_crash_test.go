package checkpoint

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

type reclaimCrashInput struct {
	Directory string                     `json:"directory"`
	Base      string                     `json:"base"`
	Bootstrap journal.Bootstrap          `json:"bootstrap"`
	Policy    config.CheckpointRetention `json:"policy"`
	Request   RetentionApplyRequest      `json:"request"`
	Now       time.Time                  `json:"now"`
	Point     string                     `json:"point"`
}

func TestCheckpointReclaimProcessCrashChild(t *testing.T) {
	input := os.Getenv("XONE_CHECKPOINT_RECLAIM_CRASH_INPUT")
	if input == "" {
		t.Skip("only invoked by the real-process parent crash test")
	}
	raw, e := os.ReadFile(input)
	if e != nil {
		t.Fatal(e)
	}
	var value reclaimCrashInput
	if e = json.Unmarshal(raw, &value); e != nil {
		t.Fatal(e)
	}
	remote := remoteFixture(t, value.Base, 5*time.Second)
	_, e = remote.applyRetention(context.Background(), value.Directory, value.Bootstrap, value.Policy, value.Request, value.Now, nil, func(point string) error {
		if point == value.Point {
			fmt.Fprintln(os.Stdout, "XONE_RECLAIM_CRASH_POINT="+point)
			os.Exit(77)
		}
		return nil
	})
	t.Fatalf("controlled process crash point not reached: %s: %v", value.Point, e)
}
func TestCheckpointReclaimRealProcessCrashAndFreshOwnerResume(t *testing.T) {
	var evidence []map[string]any
	for _, point := range []string{"after_rename", "after_remove:bootstrap.pb", "directory_removed", "before_complete_commit"} {
		t.Run(point, func(t *testing.T) {
			remote, dir, pubs, boots, now := retentionFixture(t)
			policy := reclaimPolicy()
			plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
			input := reclaimCrashInput{Directory: dir, Base: remote.base, Bootstrap: boots[5], Policy: policy, Request: applyRequest(plan), Now: now, Point: point}
			raw, e := json.Marshal(input)
			if e != nil {
				t.Fatal(e)
			}
			path := filepath.Join(dir, "crash-input.json")
			if e = os.WriteFile(path, raw, 0600); e != nil {
				t.Fatal(e)
			}
			executable, e := os.Executable()
			if e != nil {
				t.Fatal(e)
			}
			childContext, stopChild := context.WithTimeout(context.Background(), 15*time.Second)
			defer stopChild()
			command := exec.CommandContext(childContext, executable, "-test.run=^TestCheckpointReclaimProcessCrashChild$", "-test.v")
			command.Env = append(os.Environ(), "XONE_CHECKPOINT_RECLAIM_CRASH_INPUT="+path)
			output, e := command.CombinedOutput()
			exit, ok := e.(*exec.ExitError)
			if !ok || exit.ExitCode() != 77 || !strings.Contains(string(output), "XONE_RECLAIM_CRASH_POINT="+point) {
				t.Fatalf("not an observed process crash: %s: %v %s", point, e, output)
			}
			pending, e := remote.ReclaimStatus(dir)
			if e != nil || pending.Applied || pending.OperationID != plan.PlanID {
				t.Fatal(pending, e)
			}
			// A restarted HEAD_FIRST owner has a new writer/generation; do not require
			// reconstructing the vanished process's deadline or memory to finish its journal.
			fresh := boots[5]
			fresh.Snapshot.Reference.WriterEpoch++
			fresh.Snapshot.Reference.OnlineGeneration++
			fresh.Snapshot.Reference.JournalSeq++
			fresh.Cursor.Offset += 4
			reopened := remoteFixture(t, remote.base, 5*time.Second)
			result, e := reopened.ResumeRetention(context.Background(), dir, fresh, policy, RetentionResumeRequest{OperationID: plan.PlanID}, now.Add(time.Second), controlledReclaimAuthority)
			if e != nil || !result.Applied || result.CompletedObjects != 2 || result.TotalObjects != 2 {
				t.Fatal(point, result, e)
			}
			for _, p := range pubs {
				if !selectedIDs(plan)[p.Manifest.SHA256] {
					if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
						t.Fatal(e)
					}
				}
			}
			evidence = append(evidence, map[string]any{"point": point, "observed_child_exit": 77, "child_output_sha256": rawSHA(output), "pending": pending, "fresh_owner_result": result, "protected_checkpoints_still_decode": true})
		})
	}
	if out := os.Getenv("XONE_CHECKPOINT_RECLAIM_OUTPUT"); out != "" {
		if e := os.MkdirAll(out, 0700); e != nil {
			t.Fatal(e)
		}
		raw, e := json.MarshalIndent(map[string]any{"crashes": evidence, "verification_scope": "ACTUAL_CHILD_OS_EXIT_WITHOUT_DEFERS_DURABLE_JOURNAL_AND_FRESH_OWNER_RESUME"}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(out, "reclaim-real-process-crashes.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
}
