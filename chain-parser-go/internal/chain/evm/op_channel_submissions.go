package evm

import (
	"context"
	"encoding/json"
	"errors"

	"github.com/ethereum/go-ethereum/common"
	"xone/chain-parser-go/internal/provenance"
)

type OPChannelSubmission struct {
	Evidence  OPBatchEvidence    `json:"evidence"`
	Authority OPBatchSenderProof `json:"authority"`
}

type OPChannelSubmissionsReport struct {
	Submissions []OPBatchReport  `json:"submissions"`
	Channel     *OPChannelReport `json:"channel"`
}

// Ordered frames after Granite have an inclusive 50 L1 block timeout. Compare
// the block-wide blob index to order blob transactions within the same block.
func checkOPChannelPosition(first, previous, current OPBatchReport) error {
	const graniteTime = 1726070401
	if current.inclusionTime < graniteTime {
		return errors.New("OP_CHANNEL_GRANITE_REQUIRED")
	}
	if current.L1Number < previous.L1Number || (previous.L1Number == current.L1Number && current.FirstBlobIndex < previous.FirstBlobIndex+len(previous.VersionedHashes)) {
		return errors.New("OP_CHANNEL_SUBMISSION_ORDER")
	}
	if current.L1Number-first.L1Number > 50 {
		return errors.New("OP_CHANNEL_TIMEOUT")
	}
	return nil
}

// VerifyOPChannelSubmissions authenticates every selected transaction and its
// sender against the same finalized chain before joining its frames.
func VerifyOPChannelSubmissions(ctx context.Context, finality *provenance.VerifiedExecutionFinality, submissions []OPChannelSubmission, blocks []json.RawMessage) (*OPChannelSubmissionsReport, error) {
	if len(submissions) == 0 || len(submissions) > 128 {
		return nil, errors.New("OP_CHANNEL_SUBMISSION_COUNT")
	}
	var frames opFrames
	var result OPChannelSubmissionsReport
	for _, submission := range submissions {
		evidence := submission.Evidence
		evidence.SenderProof = &submission.Authority
		report, err := VerifyOPBatchBlobs(ctx, finality, common.Address{}, evidence)
		if err != nil {
			return nil, err
		}
		var first, previous OPBatchReport
		if len(result.Submissions) > 0 {
			first = result.Submissions[0]
			previous = result.Submissions[len(result.Submissions)-1]
		} else {
			first = report
		}
		if err = checkOPChannelPosition(first, previous, report); err != nil {
			return nil, err
		}
		start := report.FirstBlobIndex
		if err = frames.appendBlobs(ctx, evidence.Blobs[start:start+len(report.VersionedHashes)]); err != nil {
			return nil, err
		}
		result.Submissions = append(result.Submissions, report)
	}
	var err error
	result.Channel, err = frames.decode(ctx, blocks)
	if err != nil {
		return nil, err
	}
	return &result, nil
}
