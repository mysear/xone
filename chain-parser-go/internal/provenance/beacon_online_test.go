package provenance

import (
	"testing"
	"time"
)

func TestOwnerBeaconFreshness(t *testing.T) {
	p := VerifiedExecutionFinality{checkpoint: &BeaconBinding{Slot: 32}, finality: FinalityBinding{FinalizedSlot: 64, SignatureSlot: 66}}
	now := time.Unix(1606824023+66*12, 0)
	for _, tc := range []struct {
		name                   string
		now                    time.Time
		checkpointAge, headAge uint64
		want                   string
	}{
		{"inclusive", now, 34 * 12000, 2 * 12000, ""},
		{"future-signature", now.Add(-time.Millisecond), 34 * 12000, 2 * 12000, "PROVENANCE_BEACON_SIGNATURE_FUTURE"},
		{"expired-root", now, 34*12000 - 1, 2 * 12000, "PROVENANCE_BEACON_CHECKPOINT_AGE"},
		{"expired-finality", now, 34 * 12000, 2*12000 - 1, "PROVENANCE_BEACON_FINALITY_AGE"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			err := p.CheckOnlineAge(tc.now, tc.checkpointAge, tc.headAge)
			if tc.want == "" && err != nil || tc.want != "" && (err == nil || err.Error() != tc.want) {
				t.Fatalf("got %v, want %q", err, tc.want)
			}
		})
	}
}
