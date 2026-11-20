package evm

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"golang.org/x/sync/errgroup"
	"math"
	"sync/atomic"

	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

const (
	opBedrockTime   = 1686068903
	opBedrockHeight = 105235063
	opBlockSeconds  = 2
)

type opSpanLoader func(ctx context.Context, first, count uint64) ([]json.RawMessage, error)

// Read only the range needed to request execution bodies. verifyOPSpan then
// reproduces and compares the entire batch including its canonical prefix.
func opSpanHeightRange(batch []byte) (uint64, uint64, error) {
	if len(batch) == 0 || batch[0] != 1 {
		return 0, 0, errors.New("OP_CHANNEL_SINGLE_SPAN_REQUIRED")
	}
	rel, n := binary.Uvarint(batch[1:])
	if n <= 0 || rel%opBlockSeconds != 0 || rel > math.MaxUint64-opBedrockTime {
		return 0, 0, errors.New("OP_SPAN_BLOCK_TIME")
	}
	prefix := batch[1+n:]
	_, n = binary.Uvarint(prefix)
	if n <= 0 || len(prefix)-n <= 40 {
		return 0, 0, errors.New("OP_SPAN_PREFIX")
	}
	count, n := binary.Uvarint(prefix[n+40:])
	if n <= 0 || count == 0 || count > 1000000 {
		return 0, 0, errors.New("OP_SPAN_BLOCK_COUNT")
	}
	return opBedrockHeight + rel/opBlockSeconds, count, nil
}

// CaptureOPSpan fetches only the heights indicated by an authenticated channel.
// No observation is returned until the full span matches the execution bodies.
func CaptureOPSpan(ctx context.Context, finality *provenance.VerifiedExecutionFinality, submissions []OPChannelSubmission, client *rpc.Client, maxBlocks int) (OPChannelObservation, *OPChannelSubmissionsReport, error) {
	var empty OPChannelObservation
	var bodies []json.RawMessage
	load := func(ctx context.Context, first, count uint64) ([]json.RawMessage, error) {
		if maxBlocks < 1 || count > uint64(maxBlocks) {
			return nil, errors.New("OP_SPAN_CAPTURE_WINDOW")
		}
		genesis, err := knownBodyGenesis("evm:10")
		if err != nil {
			return nil, err
		}
		if err = client.VerifyEVM(ctx, "evm:10", genesis); err != nil {
			return nil, err
		}
		var retained atomic.Int64
		bodies = make([]json.RawMessage, count)
		group, requests := errgroup.WithContext(ctx)
		group.SetLimit(4)
		for offset := range count {
			if requests.Err() != nil {
				break
			}
			group.Go(func() error {
				var raw json.RawMessage
				if err := client.Call(requests, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", first+offset), true}, &raw); err != nil {
					return err
				}
				if retained.Add(int64(len(raw))) > 32<<20 {
					return errors.New("OP_SPAN_BYTE_LIMIT")
				}
				bodies[offset] = raw
				return nil
			})
		}
		if err := group.Wait(); err != nil {
			return nil, err
		}
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		return bodies, nil
	}
	report, err := verifyOPSubmissions(ctx, finality, submissions, nil, load)
	if err != nil {
		return empty, nil, err
	}
	if report.Channel.Span.origins[len(report.Channel.Span.origins)-1].Number > report.Submissions[len(report.Submissions)-1].L1Number {
		return empty, nil, errors.New("OP_SPAN_ORIGIN_AFTER_SUBMISSION")
	}
	return OPChannelObservation{Submissions: submissions, Blocks: bodies}, report, nil
}
