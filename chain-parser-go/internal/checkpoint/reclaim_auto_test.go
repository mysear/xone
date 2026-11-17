package checkpoint

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestCheckpointAutoReclaimNeedsItsOwnOptInAndDoesNotJournalEmptyPlans(t *testing.T) {
	remote, dir, _, boots, now := retentionFixture(t)
	policy := reclaimPolicy()
	if _, e := remote.AutoReclaim(context.Background(), dir, boots[5], policy, now, controlledReclaimAuthority); e == nil || e.Error() != "CHECKPOINT_RECLAIM_AUTOMATIC_DISABLED" {
		t.Fatal(e)
	}
	policy.AutoReclaim = true
	policy.MinAgeMS = 31536000000
	result, e := remote.AutoReclaim(context.Background(), dir, boots[5], policy, now, controlledReclaimAuthority)
	if e != nil || result.Applied || result.Status != "NO_ELIGIBLE_OBJECTS" {
		t.Fatal(result, e)
	}
	if _, e = os.Stat(filepath.Join(dir, "retirements")); !errors.Is(e, os.ErrNotExist) {
		t.Fatal("empty automatic plan accumulated a control record", e)
	}
}
func TestCheckpointAutoReclaimResumesOnlyOneBoundedOperationAndNeverReselectsPendingData(t *testing.T) {
	remote, dir, pubs, boots, now := retentionFixture(t)
	policy := reclaimPolicy()
	// The persisted operation predates enabling automatic maintenance.
	plan := reclaimPlan(t, remote, dir, boots[5], policy, now)
	stop := errors.New("STOP_AFTER_INTENT")
	if _, e := remote.applyRetention(context.Background(), dir, boots[5], policy, applyRequest(plan), now, nil, func(point string) error {
		if point == "intent_persisted" {
			return stop
		}
		return nil
	}); !errors.Is(e, stop) {
		t.Fatal(e)
	}
	policy.AutoReclaim = true
	policy.MaxRemove = 1
	first, e := remote.AutoReclaim(context.Background(), dir, boots[5], policy, now.Add(time.Second), controlledReclaimAuthority)
	if e != nil || first.Applied || !first.Resumed || first.CompletedObjects != 1 || first.TotalObjects != 1 {
		t.Fatal("automatic resume exceeded its per-cycle limit", first, e)
	}
	second, e := remote.AutoReclaim(context.Background(), dir, boots[5], policy, now.Add(2*time.Second), controlledReclaimAuthority)
	if e != nil || !second.Applied || !second.Resumed || second.CompletedObjects != 2 || second.TotalObjects != 2 {
		t.Fatal("automatic resume started another selection", second, e)
	}
	for _, p := range pubs {
		if !selectedIDs(plan)[p.Manifest.SHA256] {
			if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
				t.Fatal(e)
			}
		}
	}
}
