package provenance

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"syscall"
	"testing"
	"time"
)

func TestBeaconRotationPublication(t *testing.T) {
	var oldObservation OnlineBeaconProof
	readForkCapture(t, "electra-fulu.json.gz", &oldObservation)
	var bootstrap json.RawMessage
	readForkCapture(t, "fulu-rotation-bootstrap.json.gz", &bootstrap)
	var slot committeeUpdate
	if err := json.Unmarshal(oldObservation.Finality, &slot); err != nil {
		t.Fatal(err)
	}
	atSignature := time.Unix(1606824023+int64(slot.Data.SignatureSlot)*12, 0)
	for _, scenario := range []string{"success", "wrong-root", "wrong-bootstrap", "expired-predecessor", "expired-successor", "canceled", "busy", "exists"} {
		t.Run(scenario, func(t *testing.T) {
			dir := t.TempDir()
			old := BeaconRelease{Root: "0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c", ProofFile: filepath.Join(dir, "old"), CheckpointAgeMS: 7 * 86400000, FinalizedAgeMS: 86400000}
			next := old
			next.Root = "0xaa34231b2a668decf756fab0a13b61a96d7d3108afebfc269f2b350e49dc5a38"
			next.ProofFile = filepath.Join(dir, "next")
			input := filepath.Join(dir, "bootstrap")
			data, err := json.Marshal(oldObservation)
			if err != nil {
				t.Fatal(err)
			}
			write := func(path string, raw []byte) {
				t.Helper()
				if err := os.WriteFile(path, raw, 0600); err != nil {
					t.Fatal(err)
				}
			}
			write(old.ProofFile, data)
			write(input, bootstrap)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			now := func() time.Time { return atSignature }
			var want string
			switch scenario {
			case "wrong-root":
				next.Root = "0x" + string(bytes.Repeat([]byte("1"), 64))
				want = "PROVENANCE_BEACON_ROTATION_FINALIZED_ROOT"
			case "wrong-bootstrap":
				write(input, oldObservation.Bootstrap)
				want = "PROVENANCE_BEACON_CHECKPOINT_MISMATCH"
			case "expired-predecessor":
				old.CheckpointAgeMS = 1
				want = "PROVENANCE_BEACON_CHECKPOINT_AGE"
			case "expired-successor":
				next.FinalizedAgeMS = 1
				want = "PROVENANCE_BEACON_FINALITY_AGE"
			case "canceled":
				cancel()
				want = context.Canceled.Error()
			case "busy":
				f, err := os.OpenFile(old.ProofFile+".lock", os.O_CREATE|os.O_RDWR, 0600)
				if err != nil {
					t.Fatal(err)
				}
				defer f.Close()
				if err = syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
					t.Fatal(err)
				}
				want = "PROVENANCE_BEACON_REFRESH_BUSY"
			case "exists":
				write(next.ProofFile, []byte("prior"))
			}
			result, err := prepareBeaconRotation(ctx, old, next, input, now)
			previous, readErr := os.ReadFile(old.ProofFile)
			if readErr != nil || !bytes.Equal(previous, data) {
				t.Fatal("predecessor changed", readErr)
			}
			if scenario == "exists" {
				existing, readErr := os.ReadFile(next.ProofFile)
				if !errors.Is(err, os.ErrExist) || readErr != nil || string(existing) != "prior" {
					t.Fatal(err, readErr)
				}
				return
			}
			if want != "" {
				if err == nil || err.Error() != want {
					t.Fatal(err, want)
				}
				if _, err = os.Stat(next.ProofFile); !errors.Is(err, os.ErrNotExist) {
					t.Fatal("unexpected publication", err)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			proof, err := ReadOnlineBeaconProof(next.ProofFile, next.Root)
			if err != nil || proof.finality != result || len(proof.transitions) != 0 {
				t.Fatal("published proof", err)
			}
		})
	}
}
