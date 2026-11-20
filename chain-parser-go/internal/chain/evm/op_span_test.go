package evm

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"testing"

	"github.com/andybalholm/brotli"
	"github.com/ethereum/go-ethereum/rlp"
)

func TestOPSpanMainnetBlockContents(t *testing.T) {
	var evidence OPBatchEvidence
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &evidence)
	var frames opFrames
	for _, blob := range evidence.Blobs[3:8] {
		p, err := unpackOPBlob(blob)
		if err != nil {
			t.Fatal(err)
		}
		if err = frames.appendPayload(p); err != nil {
			t.Fatal(err)
		}
	}
	raw, err := io.ReadAll(brotli.NewReader(bytes.NewReader(frames.compressed[1:])))
	if err != nil {
		t.Fatal(err)
	}
	span, _, err := rlp.SplitString(raw)
	if err != nil {
		t.Fatal(err)
	}
	var blocks []json.RawMessage
	readOriginFixture(t, "op-mainnet-batches/l2-159769779-159769799.json.gz", &blocks)
	ctx := context.Background()
	report, err := verifyOPSpan(ctx, span, blocks)
	if err != nil || report.FirstBlock != 159769779 || report.LastBlock != 159769799 || report.UserTransactions != 1082 {
		t.Fatal(report, err)
	}
	// Flip bytes in the actual batch, retaining its lengths and all 21 RPC responses.
	for _, tc := range []struct {
		name string
		at   int
	}{{"last origin hash", 32}, {"signature", 1024}, {"compact transaction data", 100000}} {
		t.Run(tc.name, func(t *testing.T) {
			changed := bytes.Clone(span)
			changed[tc.at] ^= 1
			if r, err := verifyOPSpan(ctx, changed, blocks); err == nil || err.Error() != "OP_SPAN_CONTENT_MISMATCH" || r != nil {
				t.Fatal(r, err)
			}
		})
	}
	for _, tc := range []struct {
		name   string
		blocks []json.RawMessage
		reason string
	}{{"missing last", blocks[:20], "OP_SPAN_CONTENT_MISMATCH"}, {"skipped block", []json.RawMessage{blocks[0], blocks[2]}, "OP_SPAN_BLOCK_LINK"}, {"reversed blocks", []json.RawMessage{blocks[1], blocks[0]}, "OP_SPAN_BLOCK_LINK"}} {
		t.Run(tc.name, func(t *testing.T) {
			if r, err := verifyOPSpan(ctx, span, tc.blocks); err == nil || err.Error() != tc.reason || r != nil {
				t.Fatal(r, err)
			}
		})
	}
}
