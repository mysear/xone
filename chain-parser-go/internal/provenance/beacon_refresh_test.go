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

// Replay immutable captures through the same fetch, verify, compare, and
// atomic replacement path that the maintenance command uses.
func TestBeaconRefreshPublication(t *testing.T) {
	dir := os.Getenv("XONE_BEACON_REFRESH_FIXTURES")
	if dir == "" {
		t.Skip("requires retained Fulu bootstrap and finality responses")
	}
	read := func(name string) json.RawMessage {
		raw, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		return raw
	}
	boot := read("beacon-transition-189/two-bootstrap.json")
	updates := read("beacon-transition-189/two-updates.json")
	older := read("beacon-finality-188/finality.json")
	newer := read("beacon-capture-190/finality.json")
	root := "0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7"
	now := time.Unix(1606824023+15688200*12, 0)
	ctx := context.Background()
	target := filepath.Join(t.TempDir(), "proof.json")
	finality := older
	fetch := func(route string, limit int) (json.RawMessage, error) {
		switch route {
		case "bootstrap/" + root:
			return boot, nil
		case "finality_update":
			return finality, nil
		case "updates?start_period=1913&count=2":
			return updates, nil
		}
		return nil, errors.New("unexpected route")
	}
	refresh := func() (FinalityBinding, error) {
		return refreshBeaconProof(ctx, root, target, 7*86400000, 86400000, fetch, func() time.Time { return now })
	}
	for _, input := range []json.RawMessage{older, newer, newer} {
		finality = input
		if _, err := refresh(); err != nil {
			t.Fatal(err)
		}
	}
	expected, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct{ name, want string }{{"rollback", "PROVENANCE_BEACON_REFRESH_CONFLICT"}, {"invalid", "PROVENANCE_BEACON_UPDATES_LIMIT"}, {"concurrent", "PROVENANCE_BEACON_REFRESH_BUSY"}, {"expired", "PROVENANCE_BEACON_CHECKPOINT_AGE"}, {"cancelled", "context canceled"}} {
		t.Run(tc.name, func(t *testing.T) {
			finality = newer
			switch tc.name {
			case "rollback":
				finality = older
			case "invalid":
				finality = []byte(`{}`)
			case "concurrent":
				lock, err := os.OpenFile(target+".lock", os.O_RDWR, 0600)
				if err != nil {
					t.Fatal(err)
				}
				defer lock.Close()
				if err := syscall.Flock(int(lock.Fd()), syscall.LOCK_EX); err != nil {
					t.Fatal(err)
				}
			case "expired":
				saved := now
				now = now.Add(7 * 24 * time.Hour)
				defer func() { now = saved }()
			case "cancelled":
				var cancel context.CancelFunc
				ctx, cancel = context.WithCancel(ctx)
				cancel()
			}
			if _, err := refresh(); err == nil || err.Error() != tc.want {
				t.Fatalf("got %v, want %s", err, tc.want)
			}
			data, err := os.ReadFile(target)
			if err != nil || !bytes.Equal(data, expected) {
				t.Fatal("changed previously installed proof", err)
			}
		})
	}
}
