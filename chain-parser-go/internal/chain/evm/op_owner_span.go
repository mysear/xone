package evm

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"

	"xone/chain-parser-go/internal/provenance"
)

// The publisher can atomically replace this observation as the finalized L1
// head advances. Only verified submissions and bodies supply membership.
type OPChannelObservation struct {
	Submissions []OPChannelSubmission `json:"submissions"`
	Blocks      []json.RawMessage     `json:"blocks"`
}

func readOwnerOPSpan(ctx context.Context, checkpointFile, path string, proof *provenance.VerifiedExecutionFinality, block RawBlock, budget int) (*OPSpanReport, int, error) {
	observation, used, err := readOPChannelObservation(opObservationPath(checkpointFile, path), budget)
	if err != nil {
		return nil, 0, err
	}
	report, err := VerifyOPChannelSubmissions(ctx, proof, observation.Submissions, observation.Blocks)
	if err != nil {
		return nil, 0, err
	}
	span := report.Channel.Span
	height, err := canonicalIndex(block.Number)
	if err != nil || height < span.FirstBlock || height > span.LastBlock || span.blockHashes[height-span.FirstBlock] != block.Hash {
		return nil, 0, errors.New("OP_SPAN_OWNER_BLOCK_MISMATCH")
	}
	if span.origins[len(span.origins)-1].Number > report.Submissions[len(report.Submissions)-1].L1Number {
		return nil, 0, errors.New("OP_SPAN_ORIGIN_AFTER_SUBMISSION")
	}
	return span, used, nil
}

func opObservationPath(checkpointFile, path string) string {
	if !filepath.IsAbs(path) {
		return filepath.Join(filepath.Dir(checkpointFile), path)
	}
	return path
}

func readOPChannelObservation(path string, budget int) (OPChannelObservation, int, error) {
	limit := int64(min(budget, 32<<20))
	info, err := os.Lstat(path)
	if err != nil {
		return OPChannelObservation{}, 0, err
	}
	if !info.Mode().IsRegular() || info.Size() <= 0 || info.Size() > limit {
		return OPChannelObservation{}, 0, errors.New("OP_BATCH_OBSERVATION_FILE")
	}
	f, err := os.Open(path)
	if err != nil {
		return OPChannelObservation{}, 0, err
	}
	defer f.Close()
	stat, err := f.Stat()
	if err != nil || !os.SameFile(info, stat) {
		return OPChannelObservation{}, 0, errors.New("OP_BATCH_OBSERVATION_FILE")
	}
	raw, err := io.ReadAll(io.LimitReader(f, limit+1))
	if err != nil {
		return OPChannelObservation{}, 0, err
	}
	if int64(len(raw)) != info.Size() {
		return OPChannelObservation{}, 0, errors.New("OP_BATCH_OBSERVATION_FILE")
	}
	var observation OPChannelObservation
	if err := json.Unmarshal(raw, &observation); err != nil {
		return OPChannelObservation{}, 0, err
	}
	if len(observation.Blocks) == 0 {
		return OPChannelObservation{}, 0, errors.New("OP_SPAN_BLOCKS_REQUIRED")
	}
	return observation, len(raw), nil
}
