package ingest

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

var ErrFailureRecoveryRequired = errors.New("INGEST_FAILURE_RECOVERY_REQUIRED")

const failureSessionFile = ".failure-open"
const failureBarrierPrefix = ".failure-barrier-"

// A zero-byte, fixed-name safety marker is independent of the normal metadata
// budget. Its name binds raw identity, raw+scope digest and minimum failure cycle.
// At most one failed observation can remain per handle, since any error poisons
// it. Markers cannot be silently discarded as temporary/uncommitted files.
type failureBarrier struct {
	RawID string
	Key   string
	Cycle uint64
}

func (b failureBarrier) name() string {
	return fmt.Sprintf("%s%s-%s-%016x", failureBarrierPrefix, b.RawID, b.Key, b.Cycle)
}
func parseFailureBarrier(name string) (failureBarrier, error) {
	parts := strings.Split(strings.TrimPrefix(name, failureBarrierPrefix), "-")
	if len(parts) != 3 || !validDigest(parts[0]) || !validDigest(parts[1]) || len(parts[2]) != 16 {
		return failureBarrier{}, ErrCorrupt
	}
	cycle, err := strconv.ParseUint(parts[2], 16, 64)
	if err != nil {
		return failureBarrier{}, ErrCorrupt
	}
	b := failureBarrier{parts[0], parts[1], cycle}
	if b.name() != name {
		return failureBarrier{}, ErrCorrupt
	}
	return b, nil
}
func (a *Archive) createSafetyMarker(name string, sync func(string) error) error {
	path := filepath.Join(a.directory, name)
	f, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0600)
	if errors.Is(err, os.ErrExist) {
		f, err = openRegular(path)
	}
	if err != nil {
		return err
	}
	info, err := f.Stat()
	if err != nil {
		f.Close()
		return err
	}
	if info.Size() != 0 {
		f.Close()
		return ErrCorrupt
	}
	if err = f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	return sync(a.directory)
}
func (a *Archive) armFailureSession() error {
	// This pre-existing durable guard is the last resort if even the first scoped
	// marker cannot be recorded. A crash cannot safely prove an attempted failure
	// observation never occurred: such a restart retains raw and refuses cached
	// successes/GC. Pending original owner intents may still be confirmed.
	a.failureUncertain = true
	if err := a.createSafetyMarker(failureSessionFile, directorySync); err != nil {
		return err
	}
	a.failureSessionArmed = true
	a.failureUncertain = false
	return nil
}
func (a *Archive) beginFailureBarrier(b failureBarrier) error {
	a.failureUncertain = true
	if err := a.createSafetyMarker(b.name(), a.syncDir); err != nil {
		a.failed = true
		return err
	}
	a.failureBarriers[b.name()] = b
	a.failureUncertain = false
	return nil
}
func (a *Archive) finishFailureBarrier(b failureBarrier) error {
	if err := os.Remove(filepath.Join(a.directory, b.name())); err != nil {
		a.failed = true
		return err
	}
	if err := a.syncDir(a.directory); err != nil {
		a.failed = true
		return err
	}
	delete(a.failureBarriers, b.name())
	return nil
}
func (a *Archive) failureHeld(ref RawRef) bool {
	if a.restoring {
		return false
	}
	if a.failureRecoveryRequired {
		return true
	}
	for _, b := range a.failureBarriers {
		if b.RawID == ref.ID {
			return true
		}
	}
	return false
}
func (a *Archive) restoreFailureBarriers() error {
	for name, b := range a.failureBarriers {
		ref, exists := a.raw[b.RawID]
		if !exists {
			return ErrCorrupt
		}
		covered := false
		for _, i := range a.incidents {
			if i.Raw == ref && i.key() == b.Key && i.Cycle >= b.Cycle {
				covered = true
				break
			}
		}
		if !covered {
			if _, gone := a.reclaimed[ref.ID]; gone {
				return ErrCorrupt
			}
			continue
		}
		if err := os.Remove(filepath.Join(a.directory, name)); err != nil {
			return err
		}
		delete(a.failureBarriers, name)
	}
	return a.syncDir(a.directory)
}

// FailureRecoveryRequired reports an unclean session where a failure might not
// have reached its first durable scoped marker. Read/enumeration/Confirm remain
// available, but GC and cached successes stay blocked. Export/reconcile evidence
// under operator recovery; reopening or Close never silently acknowledges it.
func (a *Archive) FailureRecoveryRequired() bool {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.failureRecoveryRequired
}
func (a *Archive) closeFailureSession() error {
	if !a.failureSessionArmed || a.failureUncertain || a.failureRecoveryRequired {
		return nil
	}
	if err := os.Remove(filepath.Join(a.directory, failureSessionFile)); err != nil {
		return err
	}
	return directorySync(a.directory)
}
