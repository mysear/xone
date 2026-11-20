package evm

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"

	"github.com/ethereum/go-ethereum/crypto/kzg4844"
)

// Recovery is limited to transaction boundaries: the retained observation must
// still pass the strict verifier, which consumes every blob of each submission.
// A transaction mixing orphan frames and a new channel remains unsupported.
type opForwardChannelFrames struct {
	frames opFrames
	first  uint64
}

const opForwardHoloceneTime = uint64(1736445601)

// Channel recovery clears only retained output. The full scan keeps its byte
// and candidate counters, including work spent on discarded transactions.
type opForwardScan struct {
	channel          opForwardChannelFrames
	submissions      []OPChannelSubmission
	between          []OPChannelIntervalBlock
	remaining        int
	candidates       int
	encodedLimit     int
	encodedBudget    int
	selectedAtHeight bool
}

func newOPForwardScan(budget int) opForwardScan {
	limit := min(budget, 32<<20)
	return opForwardScan{remaining: budget, encodedLimit: limit, encodedBudget: limit}
}

func (s *opForwardScan) charge(bytes int) error {
	if bytes < 0 || bytes > s.remaining {
		return errors.New("OP_BATCH_SEARCH_BUDGET")
	}
	s.remaining -= bytes
	return nil
}

func (s *opForwardScan) candidate() error {
	if s.candidates >= 128 {
		return errors.New("OP_CHANNEL_SUBMISSION_COUNT")
	}
	s.candidates++
	return nil
}

func (s *opForwardScan) clearObservation() {
	s.submissions, s.between = nil, nil
	s.encodedBudget = s.encodedLimit
	s.selectedAtHeight = false
}

func (s *opForwardScan) beginHeight(height, timestamp uint64) error {
	s.selectedAtHeight = false
	if s.channel.expire(height) {
		if timestamp < opForwardHoloceneTime {
			return errors.New("OP_CHANNEL_TIMEOUT")
		}
		s.clearObservation()
	}
	return nil
}

func (s *opForwardScan) append(ctx context.Context, height, timestamp uint64, blobs []*kzg4844.Blob, item OPChannelSubmission) (bool, error) {
	accepted, restarted, err := s.channel.append(ctx, height, timestamp, blobs)
	if err != nil || !accepted {
		return false, err
	}
	if restarted {
		s.clearObservation()
	}
	item.Intervening = s.between
	// The output may repeat block and blob evidence at a shared height. Bound
	// that expansion separately from the cumulative input byte allowance.
	wire, err := json.Marshal(item)
	if err != nil {
		return false, err
	}
	if len(wire) > s.encodedBudget {
		return false, errors.New("OP_BATCH_OBSERVATION_FILE")
	}
	s.encodedBudget -= len(wire)
	s.submissions = append(s.submissions, item)
	s.between = nil
	s.selectedAtHeight = true
	return s.channel.frames.closed, nil
}

func (s *opForwardScan) endHeight(raw json.RawMessage, authority *OPBatchSenderProof) {
	if len(s.submissions) > 0 && !s.selectedAtHeight {
		s.between = append(s.between, OPChannelIntervalBlock{Block: raw, Authority: authority})
	}
}

func (c *opForwardChannelFrames) expire(height uint64) bool {
	if c.frames.count == 0 || height < c.first || height-c.first <= 50 {
		return false
	}
	*c = opForwardChannelFrames{}
	return true
}

// Inspect orphan-only transactions before skipping them. A frame zero starts
// a replacement channel, including when it repeats the abandoned channel ID.
func opForwardStartsChannel(ctx context.Context, blobs []*kzg4844.Blob) (bool, error) {
	seen := false
	for _, blob := range blobs {
		if err := ctx.Err(); err != nil {
			return false, err
		}
		payload, err := unpackOPBlob(blob)
		if err != nil {
			return false, err
		}
		if len(payload) < 2 || payload[0] != 0 {
			return false, errors.New("OP_FRAME_VERSION")
		}
		for data := payload[1:]; len(data) > 0; {
			if len(data) < 23 {
				return false, errors.New("OP_FRAME_TRUNCATED")
			}
			size := binary.BigEndian.Uint32(data[18:22])
			if size > 1_000_000 || uint64(size)+23 > uint64(len(data)) {
				return false, errors.New("OP_FRAME_LENGTH")
			}
			if data[22+int(size)] > 1 {
				return false, errors.New("OP_FRAME_ORDER")
			}
			if binary.BigEndian.Uint16(data[16:18]) == 0 {
				if seen {
					return false, errors.New("OP_FRAME_ORDER")
				}
				// The strict append checks all remaining frames in this case.
				return true, nil
			}
			seen = true
			data = data[23+int(size):]
		}
	}
	if !seen {
		return false, errors.New("OP_CHANNEL_INCOMPLETE")
	}
	return false, nil
}

func (c *opForwardChannelFrames) append(ctx context.Context, height, timestamp uint64, blobs []*kzg4844.Blob) (accepted, restarted bool, err error) {
	// Before Holocene the upstream stage is ChannelBank, not ChannelAssembler.
	// Preserve the existing strict behavior rather than applying reset rules
	// from a later fork to a historical channel.
	if timestamp < opForwardHoloceneTime {
		if c.frames.count == 0 {
			c.first = height
		}
		if err := c.frames.appendBlobs(ctx, blobs); err != nil {
			return false, false, err
		}
		return true, false, nil
	}
	starts, err := opForwardStartsChannel(ctx, blobs)
	if err != nil {
		return false, false, err
	}
	if starts {
		var next opFrames
		if err := next.appendBlobs(ctx, blobs); err != nil {
			return false, false, err
		}
		restarted = c.frames.count > 0
		c.frames, c.first = next, height
		return true, restarted, nil
	}
	if c.frames.count == 0 {
		return false, false, nil
	}
	if err := c.frames.appendBlobs(ctx, blobs); err != nil {
		return false, false, err
	}
	return true, false, nil
}
