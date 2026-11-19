package provenance

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

// BeaconRelease describes the root and freshness policy supplied by an owner release.
type BeaconRelease struct {
	Root, ProofFile                 string
	CheckpointAgeMS, FinalizedAgeMS uint64
}

// PrepareBeaconRotation publishes one proof for a separately configured successor
// release. Its bootstrap must commit to exactly the predecessor's signed finalized root.
func PrepareBeaconRotation(ctx context.Context, old, next BeaconRelease, bootstrapFile string) (FinalityBinding, error) {
	return prepareBeaconRotation(ctx, old, next, bootstrapFile, time.Now)
}
func prepareBeaconRotation(ctx context.Context, old, next BeaconRelease, bootstrapFile string, now func() time.Time) (FinalityBinding, error) {
	var result FinalityBinding
	if err := ctx.Err(); err != nil {
		return result, err
	}
	if strings.EqualFold(old.Root, next.Root) || old.ProofFile == next.ProofFile {
		return result, invalid("BEACON_ROTATION_RELEASE")
	}
	// Refresh uses the same lock. Pin its observation until the candidate is durable.
	lock, err := os.OpenFile(old.ProofFile+".lock", os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return result, err
	}
	defer lock.Close()
	if err = syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		return result, invalid("BEACON_REFRESH_BUSY")
	}
	observation, err := readBeaconObservation(old.ProofFile)
	if err != nil {
		return result, err
	}
	prior, err := VerifyExecutionFinality(old.Root, observation.Bootstrap, observation.Finality, observation.Updates)
	if err != nil {
		return result, err
	}
	if err = prior.CheckOnlineAge(now(), old.CheckpointAgeMS, old.FinalizedAgeMS); err != nil {
		return result, err
	}
	if !strings.EqualFold(prior.finality.FinalizedRoot.Hex(), next.Root) || prior.finality.FinalizedSlot <= prior.checkpoint.Slot {
		return result, invalid("BEACON_ROTATION_FINALIZED_ROOT")
	}
	bootstrap, err := readBeaconFile(bootstrapFile, MaxBootstrapBytes)
	if err != nil {
		return result, err
	}
	candidate, err := VerifyExecutionFinality(next.Root, bootstrap, observation.Finality, nil)
	if err != nil {
		return result, err
	}
	if candidate.finality != prior.finality {
		return result, invalid("BEACON_ROTATION_FINALITY")
	}
	data, err := json.Marshal(OnlineBeaconProof{Bootstrap: bootstrap, Finality: observation.Finality})
	if err != nil {
		return result, err
	}
	dir, err := os.Open(filepath.Dir(next.ProofFile))
	if err != nil {
		return result, err
	}
	defer dir.Close()
	tmp, err := os.CreateTemp(dir.Name(), ".beacon-rotation-")
	if err != nil {
		return result, err
	}
	defer os.Remove(tmp.Name())
	if _, err = tmp.Write(data); err == nil {
		err = tmp.Sync()
	}
	closeErr := tmp.Close()
	if err != nil {
		return result, err
	}
	if closeErr != nil {
		return result, closeErr
	}
	if err = ctx.Err(); err != nil {
		return result, err
	}
	wall := now()
	if err = prior.CheckOnlineAge(wall, old.CheckpointAgeMS, old.FinalizedAgeMS); err != nil {
		return result, err
	}
	if err = candidate.CheckOnlineAge(wall, next.CheckpointAgeMS, next.FinalizedAgeMS); err != nil {
		return result, err
	}
	// Never replace a concurrently published or already refreshed candidate.
	if err = os.Link(tmp.Name(), next.ProofFile); err != nil {
		return result, err
	}
	if err = dir.Sync(); err != nil {
		return result, err
	}
	return candidate.finality, nil
}
