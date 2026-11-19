package provenance

import (
	"encoding/json"
	"slices"
	"strings"

	"github.com/ethereum/go-ethereum/beacon/merkle"
	"github.com/ethereum/go-ethereum/beacon/params"
	beacon "github.com/ethereum/go-ethereum/beacon/types"
	"github.com/ethereum/go-ethereum/common"
)

type FinalityBinding struct {
	FinalizedRoot common.Hash `json:"finalized_root"`
	FinalizedSlot uint64      `json:"finalized_slot"`
	AttestedRoot  common.Hash `json:"attested_root"`
	AttestedSlot  uint64      `json:"attested_slot"`
	SignatureSlot uint64      `json:"signature_slot"`
	Signers       int         `json:"signers"`
	ExecutionHash common.Hash `json:"execution_hash"`
}

// VerifyWithBeaconFinality checks a same-period finality update against the
// committee proven under an externally pinned checkpoint. This offline report
// does not assign freshness or trust to the checkpoint supplied by the caller.
func VerifyWithBeaconFinality(e Evidence, root string, bootstrap, raw []byte) (Report, error) {
	if e.ChainID != "1" {
		return Report{}, invalid("BEACON_MAINNET_REQUIRED")
	}
	pin, committee, err := verifyCheckpoint(root, bootstrap)
	if err != nil {
		return Report{}, err
	}
	var update struct {
		Version string `json:"version"`
		Data    struct {
			Attested      lightHeader           `json:"attested_header"`
			Finalized     lightHeader           `json:"finalized_header"`
			Branch        merkle.Values         `json:"finality_branch"`
			Signature     *beacon.SyncAggregate `json:"sync_aggregate"`
			SignatureSlot uint64                `json:"signature_slot,string"`
		} `json:"data"`
	}
	if len(raw) > MaxBootstrapBytes || json.Unmarshal(raw, &update) != nil {
		return Report{}, invalid("BEACON_UPDATE_ENCODING")
	}
	u := update.Data
	if u.Attested.Beacon == nil || u.Finalized.Beacon == nil || u.Signature == nil {
		return Report{}, invalid("BEACON_UPDATE_FIELDS")
	}
	attested, finalized := u.Attested.Beacon, u.Finalized.Beacon
	if finalized.Slot < pin.Slot || finalized.Slot > attested.Slot || attested.Slot >= u.SignatureSlot || beacon.SyncPeriod(u.SignatureSlot) != beacon.SyncPeriod(pin.Slot) {
		return Report{}, invalid("BEACON_UPDATE_SLOTS")
	}
	// Same-fork, same-period updates only. A transition needs its own proof.
	if update.Version != pin.Fork || mainnetHeaderFork(u.SignatureSlot) != pin.Fork {
		return Report{}, invalid("BEACON_UPDATE_FORK")
	}
	if _, err := u.Attested.verifiedExecution(update.Version); err != nil {
		return Report{}, err
	}
	payload, err := u.Finalized.verifiedExecution(update.Version)
	if err != nil {
		return Report{}, err
	}
	if !strings.EqualFold(payload.BlockHash().Hex(), e.ExpectedBlockHash) {
		return Report{}, invalid("BEACON_FINALIZED_EXECUTION_HASH")
	}
	finalizedRoot := finalized.Hash()
	if finalized.Slot == pin.Slot && finalizedRoot != pin.Root {
		return Report{}, invalid("BEACON_CHECKPOINT_MISMATCH")
	}
	if err := merkle.VerifyProof(attested.StateRoot, params.StateIndexFinalBlock(update.Version), u.Branch, merkle.Value(finalizedRoot)); err != nil {
		return Report{}, invalid("BEACON_FINALITY_BRANCH")
	}
	count := u.Signature.SignerCount()
	if count*3 < params.SyncCommitteeSize*2 {
		return Report{}, invalid("BEACON_UPDATE_PARTICIPATION")
	}
	config := *params.MainnetLightConfig
	config.Forks = slices.Clone(config.Forks)
	config.AddFork("FULU", 411392, []byte{6, 0, 0, 0})
	signingRoot, err := config.Forks.SigningRoot((u.SignatureSlot-1)/params.EpochLength, attested.Hash())
	if err != nil {
		return Report{}, invalid("BEACON_SIGNING_DOMAIN")
	}
	keys, err := committee.Deserialize()
	if err != nil || !keys.VerifySignature(signingRoot, u.Signature) {
		return Report{}, invalid("BEACON_UPDATE_SIGNATURE")
	}
	report, err := Verify(e)
	if err != nil {
		return Report{}, err
	}
	report.BeaconCheckpoint = pin
	report.BeaconFinality = &FinalityBinding{FinalizedRoot: finalizedRoot, FinalizedSlot: finalized.Slot, AttestedRoot: attested.Hash(), AttestedSlot: attested.Slot, SignatureSlot: u.SignatureSlot, Signers: count, ExecutionHash: payload.BlockHash()}
	return report, nil
}
