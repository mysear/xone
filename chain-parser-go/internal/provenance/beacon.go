package provenance

import (
	"encoding/json"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/beacon/merkle"
	beacon "github.com/ethereum/go-ethereum/beacon/types"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
)

const MaxBootstrapBytes = 128 << 10

// BeaconBinding records what was checked against the caller's external pin.
// The origin and finality of that pin are outside the proof.
type BeaconBinding struct {
	Root          common.Hash `json:"root"`
	Slot          uint64      `json:"slot"`
	Fork          string      `json:"fork"`
	CommitteeRoot common.Hash `json:"committee_root"`
	ExecutionHash common.Hash `json:"execution_hash"`
}

type lightHeader struct {
	Beacon    *beacon.Header  `json:"beacon"`
	Execution json.RawMessage `json:"execution"`
	Branch    merkle.Values   `json:"execution_branch"`
}

func mainnetHeaderFork(slot uint64) string {
	switch epoch := slot / 32; {
	case epoch >= 411392:
		return "fulu"
	case epoch >= 364032:
		return "electra"
	case epoch >= 269568:
		return "deneb"
	case epoch >= 194048:
		return "capella"
	default:
		return ""
	}
}

func (h lightHeader) verifiedExecution(fork string) (*beacon.ExecutionHeader, error) {
	if h.Beacon == nil || fork == "" || mainnetHeaderFork(h.Beacon.Slot) != fork || len(h.Branch) != 4 {
		return nil, invalid("BEACON_FORK_OR_HEADER")
	}
	// Consensus REST fields are required even when the encoded value is zero.
	fields := strings.Fields("parent_hash fee_recipient state_root receipts_root logs_bloom prev_randao block_number gas_limit gas_used timestamp extra_data base_fee_per_gas block_hash transactions_root withdrawals_root")
	layout := fork
	if fork != "capella" {
		fields = append(fields, "blob_gas_used", "excess_blob_gas")
		layout = "deneb"
	} // Electra and Fulu retain Deneb's payload header layout.
	var values map[string]string
	if json.Unmarshal(h.Execution, &values) != nil || len(values) != len(fields) {
		return nil, invalid("BEACON_PAYLOAD_FIELDS")
	}
	for _, key := range fields {
		if values[key] == "" {
			return nil, invalid("BEACON_PAYLOAD_FIELDS")
		}
	}
	extra, err := hexutil.Decode(values["extra_data"])
	if err != nil || len(extra) > 32 {
		return nil, invalid("BEACON_EXTRA_DATA")
	}
	timestamp, err := strconv.ParseUint(values["timestamp"], 10, 64)
	if err != nil || timestamp < 1606824023 || (timestamp-1606824023)%12 != 0 || (timestamp-1606824023)/12 != h.Beacon.Slot {
		return nil, invalid("BEACON_SLOT_TIME")
	}
	payload, err := beacon.ExecutionHeaderFromJSON(layout, h.Execution)
	if err != nil {
		return nil, invalid("BEACON_PAYLOAD_ENCODING")
	}
	proof := beacon.HeaderWithExecProof{Header: *h.Beacon, PayloadHeader: payload, PayloadBranch: h.Branch}
	if err := proof.Validate(); err != nil {
		return nil, invalid("BEACON_PAYLOAD_BRANCH")
	}
	return payload, nil
}

// VerifyWithBeaconCheckpoint binds the normal deployment proof to the execution
// payload of one explicitly pinned mainnet beacon block and its current committee.
func VerifyWithBeaconCheckpoint(e Evidence, rootText string, raw []byte) (Report, error) {
	if e.ChainID != "1" {
		return Report{}, invalid("BEACON_MAINNET_REQUIRED")
	}
	var root common.Hash
	if root.UnmarshalText([]byte(rootText)) != nil || root == (common.Hash{}) {
		return Report{}, invalid("BEACON_CHECKPOINT_ROOT")
	}
	var response struct {
		Version string `json:"version"`
		Data    struct {
			Header    lightHeader                     `json:"header"`
			Committee *beacon.SerializedSyncCommittee `json:"current_sync_committee"`
			Branch    merkle.Values                   `json:"current_sync_committee_branch"`
		} `json:"data"`
	}
	if len(raw) > MaxBootstrapBytes || json.Unmarshal(raw, &response) != nil {
		return Report{}, invalid("BEACON_BOOTSTRAP_ENCODING")
	}
	h := response.Data.Header
	if h.Beacon == nil || h.Beacon.Hash() != root {
		return Report{}, invalid("BEACON_CHECKPOINT_MISMATCH")
	}
	payload, err := h.verifiedExecution(response.Version)
	if err != nil {
		return Report{}, err
	}
	if !strings.EqualFold(payload.BlockHash().Hex(), e.ExpectedBlockHash) {
		return Report{}, invalid("BEACON_EXECUTION_HASH")
	}
	committee := response.Data.Committee
	if committee == nil {
		return Report{}, invalid("BEACON_COMMITTEE_MISSING")
	}
	bootstrap := beacon.BootstrapData{Version: response.Version, Header: *h.Beacon, Committee: committee, CommitteeRoot: committee.Root(), CommitteeBranch: response.Data.Branch}
	if err := bootstrap.Validate(); err != nil {
		return Report{}, invalid("BEACON_COMMITTEE_BRANCH")
	}
	report, err := Verify(e)
	if err != nil {
		return Report{}, err
	}
	report.BeaconCheckpoint = &BeaconBinding{Root: root, Slot: h.Beacon.Slot, Fork: response.Version, CommitteeRoot: bootstrap.CommitteeRoot, ExecutionHash: payload.BlockHash()}
	return report, nil
}
