package provenance

import (
	"bytes"
	"encoding/json"
	"io"
	"math"
	"os"
	"time"
)

// OnlineBeaconProof holds one provider observation. The checkpoint root comes
// from the owner's configuration, outside this replaceable document.
type OnlineBeaconProof struct {
	Bootstrap json.RawMessage `json:"bootstrap"`
	Finality  json.RawMessage `json:"finality"`
	Updates   json.RawMessage `json:"updates,omitempty"`
}

func ReadOnlineBeaconProof(path, root string) (*VerifiedExecutionFinality, error) {
	const limit = MaxBeaconUpdatesBytes + 2*MaxBootstrapBytes + 1024
	info, err := os.Lstat(path)
	if err != nil || !info.Mode().IsRegular() || info.Size() > limit {
		return nil, invalid("BEACON_ONLINE_FILE")
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, invalid("BEACON_ONLINE_FILE")
	}
	defer f.Close()
	opened, err := f.Stat()
	if err != nil || !os.SameFile(info, opened) {
		return nil, invalid("BEACON_ONLINE_FILE")
	}
	data, err := io.ReadAll(io.LimitReader(f, limit+1))
	if err != nil || len(data) > limit {
		return nil, invalid("BEACON_ONLINE_FILE")
	}
	var proof OnlineBeaconProof
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&proof) != nil || decoder.Decode(new(any)) != io.EOF {
		return nil, invalid("BEACON_ONLINE_ENCODING")
	}
	return VerifyExecutionFinality(root, proof.Bootstrap, proof.Finality, proof.Updates)
}

func (p *VerifiedExecutionFinality) ExecutionBlock() (uint64, string) {
	return p.finality.ExecutionNumber, p.finality.ExecutionHash.Hex()
}

// CheckOnlineAge applies operator freshness limits to authenticated slots. The
// signature may only come from a slot that has already begun on the local clock.
func (p *VerifiedExecutionFinality) CheckOnlineAge(now time.Time, maxCheckpointAgeMS, maxFinalizedAgeMS uint64) error {
	const genesisMS = 1606824023000
	const slotMS = 12000
	wall := now.UnixMilli()
	if p == nil || p.checkpoint == nil || wall < genesisMS || maxCheckpointAgeMS == 0 || maxFinalizedAgeMS == 0 {
		return invalid("BEACON_ONLINE_CLOCK")
	}
	elapsed := uint64(wall - genesisMS)
	for _, v := range []struct {
		slot, maxAge uint64
		reason       string
	}{
		{p.finality.SignatureSlot, math.MaxUint64, "BEACON_SIGNATURE_FUTURE"},
		{p.checkpoint.Slot, maxCheckpointAgeMS, "BEACON_CHECKPOINT_AGE"},
		{p.finality.FinalizedSlot, maxFinalizedAgeMS, "BEACON_FINALITY_AGE"},
	} {
		if v.slot > elapsed/slotMS || elapsed-v.slot*slotMS > v.maxAge {
			return invalid(v.reason)
		}
	}
	return nil
}
