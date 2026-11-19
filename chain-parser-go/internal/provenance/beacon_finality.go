package provenance

import (
	"encoding/json"
	"fmt"
	"slices"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/beacon/merkle"
	"github.com/ethereum/go-ethereum/beacon/params"
	beacon "github.com/ethereum/go-ethereum/beacon/types"
	"github.com/ethereum/go-ethereum/common"
)

type FinalityBinding struct {
	ExecutionNumber uint64      `json:"execution_number"`
	FinalizedRoot   common.Hash `json:"finalized_root"`
	FinalizedSlot   uint64      `json:"finalized_slot"`
	AttestedRoot    common.Hash `json:"attested_root"`
	AttestedSlot    uint64      `json:"attested_slot"`
	SignatureSlot   uint64      `json:"signature_slot"`
	Signers         int         `json:"signers"`
	ExecutionHash   common.Hash `json:"execution_hash"`
}

// VerifyWithBeaconFinality checks a same-period finality update against the
// committee proven under an externally pinned checkpoint. This offline report
// does not assign freshness or trust to the checkpoint supplied by the caller.
func VerifyWithBeaconFinality(e Evidence, root string, bootstrap, raw []byte) (Report, error) {
	return verifyBeaconFinality(e, root, bootstrap, raw, nil)
}

func verifyBeaconFinality(e Evidence, root string, bootstrap, raw []byte, transitions []committeeUpdate) (Report, error) {
	if e.ChainID != "1" {
		return Report{}, invalid("BEACON_MAINNET_REQUIRED")
	}
	proof, err := verifyExecutionFinality(root, bootstrap, raw, transitions)
	if err != nil {
		return Report{}, err
	}
	if !strings.EqualFold(proof.finality.ExecutionHash.Hex(), e.ExpectedBlockHash) {
		return Report{}, invalid("BEACON_FINALIZED_EXECUTION_HASH")
	}
	report, err := Verify(e)
	if err != nil {
		return Report{}, err
	}
	if err := proof.bind(&report); err != nil {
		return Report{}, err
	}
	return report, nil
}

// VerifiedExecutionFinality can only be constructed by verifying the signed
// consensus proof. Callers can select the execution height before making RPCs.
type VerifiedExecutionFinality struct {
	checkpoint  *BeaconBinding
	finality    FinalityBinding
	transitions []CommitteeTransition
}

func VerifyExecutionFinality(root string, bootstrap, raw, updates []byte) (*VerifiedExecutionFinality, error) {
	var transitions []committeeUpdate
	if updates != nil {
		var err error
		transitions, err = parseCommitteeUpdates(updates)
		if err != nil {
			return nil, err
		}
	}
	return verifyExecutionFinality(root, bootstrap, raw, transitions)
}
func (p *VerifiedExecutionFinality) BlockTag() string {
	return fmt.Sprintf("0x%x", p.finality.ExecutionNumber)
}
func (p *VerifiedExecutionFinality) bind(report *Report) error {
	if report.ChainID != "1" || report.BlockNumber != p.finality.ExecutionNumber || !strings.EqualFold(p.finality.ExecutionHash.Hex(), report.BlockHash) {
		return invalid("BEACON_FINALIZED_EXECUTION_HASH")
	}
	finality, checkpoint := p.finality, *p.checkpoint
	report.BeaconCheckpoint = &checkpoint
	report.CommitteeTransitions = slices.Clone(p.transitions)
	report.BeaconFinality = &finality
	return nil
}
func verifyExecutionFinality(root string, bootstrap, raw []byte, transitions []committeeUpdate) (*VerifiedExecutionFinality, error) {
	pin, committee, err := verifyCheckpoint(root, bootstrap)
	if err != nil {
		return nil, err
	}
	period := beacon.SyncPeriod(pin.Slot)
	var history []CommitteeTransition
	for _, next := range transitions {
		step, err := verifyTransition(pin, period, committee, next)
		if err != nil {
			return nil, err
		}
		history = append(history, step)
		committee = next.Data.NextCommittee
		period++
	}
	var update committeeUpdate
	if len(raw) > MaxBootstrapBytes || json.Unmarshal(raw, &update) != nil {
		return nil, invalid("BEACON_UPDATE_ENCODING")
	}
	u := update.Data
	if u.Attested.Beacon == nil || u.Finalized.Beacon == nil || u.Signature == nil {
		return nil, invalid("BEACON_UPDATE_FIELDS")
	}
	attested, finalized := u.Attested.Beacon, u.Finalized.Beacon
	if finalized.Slot < pin.Slot || finalized.Slot > attested.Slot || attested.Slot >= u.SignatureSlot || beacon.SyncPeriod(u.SignatureSlot) != period || len(history) > 0 && beacon.SyncPeriod(finalized.Slot) != period {
		return nil, invalid("BEACON_UPDATE_SLOTS")
	}
	// The state branch belongs to the attested fork, even if the trusted checkpoint predates it.
	if !update.matchesForkSchedule() {
		return nil, invalid("BEACON_UPDATE_FORK")
	}
	if _, err := u.Attested.verifiedExecution(update.Version); err != nil {
		return nil, err
	}
	payload, err := u.Finalized.verifiedExecution(mainnetHeaderFork(finalized.Slot))
	if err != nil {
		return nil, err
	}

	finalizedRoot := finalized.Hash()
	if finalized.Slot == pin.Slot && finalizedRoot != pin.Root {
		return nil, invalid("BEACON_CHECKPOINT_MISMATCH")
	}
	if err := merkle.VerifyProof(attested.StateRoot, params.StateIndexFinalBlock(update.Version), u.Branch, merkle.Value(finalizedRoot)); err != nil {
		return nil, invalid("BEACON_FINALITY_BRANCH")
	}
	if err := verifyCommitteeSignature(committee, attested, u.SignatureSlot, u.Signature); err != nil {
		return nil, err
	}
	var execution struct {
		BlockNumber string `json:"block_number"`
	}
	if err := json.Unmarshal(u.Finalized.Execution, &execution); err != nil {
		return nil, invalid("BEACON_PAYLOAD_ENCODING")
	}
	number, err := strconv.ParseUint(execution.BlockNumber, 10, 64)
	if err != nil {
		return nil, invalid("BEACON_PAYLOAD_ENCODING")
	}
	return &VerifiedExecutionFinality{checkpoint: pin, transitions: history, finality: FinalityBinding{ExecutionNumber: number, FinalizedRoot: finalizedRoot, FinalizedSlot: finalized.Slot, AttestedRoot: attested.Hash(), AttestedSlot: attested.Slot, SignatureSlot: u.SignatureSlot, Signers: u.Signature.SignerCount(), ExecutionHash: payload.BlockHash()}}, nil
}

type committeeUpdate struct {
	Version string `json:"version"`
	Data    struct {
		Attested      lightHeader                     `json:"attested_header"`
		Finalized     lightHeader                     `json:"finalized_header"`
		Branch        merkle.Values                   `json:"finality_branch"`
		Signature     *beacon.SyncAggregate           `json:"sync_aggregate"`
		NextCommittee *beacon.SerializedSyncCommittee `json:"next_sync_committee"`
		NextBranch    merkle.Values                   `json:"next_sync_committee_branch"`
		SignatureSlot uint64                          `json:"signature_slot,string"`
	} `json:"data"`
}

// The API version selects the attested state's proof indices. Each header's
// execution payload is verified using its own slot. BLS uses signatureSlot-1.
func (u *committeeUpdate) matchesForkSchedule() bool {
	fork := mainnetHeaderFork(u.Data.Attested.Beacon.Slot)
	return fork != "" && u.Version == fork && mainnetHeaderFork(u.Data.SignatureSlot) != ""
}

func verifyCommitteeSignature(committee *beacon.SerializedSyncCommittee, attested *beacon.Header, signatureSlot uint64, signature *beacon.SyncAggregate) error {
	count := signature.SignerCount()
	if count*3 < params.SyncCommitteeSize*2 {
		return invalid("BEACON_UPDATE_PARTICIPATION")
	}
	config := *params.MainnetLightConfig
	config.Forks = slices.Clone(config.Forks)
	config.AddFork("FULU", 411392, []byte{6, 0, 0, 0})
	signingRoot, err := config.Forks.SigningRoot((signatureSlot-1)/params.EpochLength, attested.Hash())
	if err != nil {
		return invalid("BEACON_SIGNING_DOMAIN")
	}
	keys, err := committee.Deserialize()
	if err != nil || !keys.VerifySignature(signingRoot, signature) {
		return invalid("BEACON_UPDATE_SIGNATURE")
	}
	return nil
}
