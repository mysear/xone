package provenance

import (
	"bytes"
	"encoding/json"

	beacon "github.com/ethereum/go-ethereum/beacon/types"
	"github.com/ethereum/go-ethereum/common"
)

const maxCommitteeTransitions = 16
const MaxBeaconUpdatesBytes = maxCommitteeTransitions * MaxBootstrapBytes

type CommitteeTransition struct {
	Period            uint64      `json:"period"`
	SignatureSlot     uint64      `json:"signature_slot"`
	Signers           int         `json:"signers"`
	NextCommitteeRoot common.Hash `json:"next_committee_root"`
}

// VerifyWithBeaconUpdates accepts a bounded sequence in the standard beacon
// API updates response format. Each signature uses the previously proven keys.
func VerifyWithBeaconUpdates(e Evidence, root string, bootstrap, finality, updates []byte) (Report, error) {
	if len(updates) > MaxBeaconUpdatesBytes || !json.Valid(updates) {
		return Report{}, invalid("BEACON_UPDATES_ENCODING")
	}
	decoder := json.NewDecoder(bytes.NewReader(updates))
	if token, err := decoder.Token(); err != nil || token != json.Delim('[') {
		return Report{}, invalid("BEACON_UPDATES_ENCODING")
	}
	var chain []committeeUpdate
	for decoder.More() {
		if len(chain) == maxCommitteeTransitions {
			return Report{}, invalid("BEACON_UPDATES_LIMIT")
		}
		var raw json.RawMessage
		var update committeeUpdate
		if decoder.Decode(&raw) != nil || len(raw) > MaxBootstrapBytes || json.Unmarshal(raw, &update) != nil {
			return Report{}, invalid("BEACON_UPDATES_ENCODING")
		}
		chain = append(chain, update)
	}
	if len(chain) == 0 {
		return Report{}, invalid("BEACON_UPDATES_EMPTY")
	}
	return verifyBeaconFinality(e, root, bootstrap, finality, chain)
}

func verifyTransition(pin *BeaconBinding, period uint64, committee *beacon.SerializedSyncCommittee, update committeeUpdate) (CommitteeTransition, error) {
	u := update.Data
	attested, finalized := u.Attested.Beacon, u.Finalized.Beacon
	if attested == nil || finalized == nil || u.NextCommittee == nil || u.Signature == nil {
		return CommitteeTransition{}, invalid("BEACON_TRANSITION_FIELDS")
	}
	if update.Version != pin.Fork || mainnetHeaderFork(u.SignatureSlot) != pin.Fork {
		return CommitteeTransition{}, invalid("BEACON_TRANSITION_FORK")
	}
	if finalized.Slot < pin.Slot || finalized.Slot > attested.Slot || attested.Slot >= u.SignatureSlot || beacon.SyncPeriod(finalized.Slot) != period || beacon.SyncPeriod(u.SignatureSlot) != period {
		return CommitteeTransition{}, invalid("BEACON_TRANSITION_SLOTS")
	}
	if finalized.Slot == pin.Slot && finalized.Hash() != pin.Root {
		return CommitteeTransition{}, invalid("BEACON_CHECKPOINT_MISMATCH")
	}
	if _, err := u.Attested.verifiedExecution(update.Version); err != nil {
		return CommitteeTransition{}, err
	}
	if _, err := u.Finalized.verifiedExecution(update.Version); err != nil {
		return CommitteeTransition{}, err
	}
	nextRoot := u.NextCommittee.Root()
	proof := beacon.LightClientUpdate{Version: update.Version, AttestedHeader: beacon.SignedHeader{Header: *attested, SignatureSlot: u.SignatureSlot}, NextSyncCommitteeRoot: nextRoot, NextSyncCommitteeBranch: u.NextBranch, FinalizedHeader: finalized, FinalityBranch: u.Branch}
	if err := proof.Validate(); err != nil {
		return CommitteeTransition{}, invalid("BEACON_TRANSITION_PROOF")
	}
	if err := verifyCommitteeSignature(committee, attested, u.SignatureSlot, u.Signature); err != nil {
		return CommitteeTransition{}, err
	}
	return CommitteeTransition{Period: period, SignatureSlot: u.SignatureSlot, Signers: u.Signature.SignerCount(), NextCommitteeRoot: nextRoot}, nil
}
