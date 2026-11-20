package evm

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"strconv"
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

// The first and last epochs can both match while an intermediate deposit names
// the wrong hash. A single authenticated traversal must check all four epochs.
func TestOPSpanOriginRange(t *testing.T) {
	var headers []json.RawMessage
	readOriginFixture(t, "op-mainnet-batches/origins-26453043-26453032.json.gz", &headers)
	var origins []OPL1Origin
	for i := len(headers) - 1; i >= 8; i-- {
		var h RawBlock
		if err := json.Unmarshal(headers[i], &h); err != nil {
			t.Fatal(err)
		}
		n, _ := canonicalIndex(h.Number)
		ts, _ := canonicalIndex(h.Timestamp)
		var fee struct {
			Value string `json:"baseFeePerGas"`
		}
		if err := json.Unmarshal(headers[i], &fee); err != nil {
			t.Fatal(err)
		}
		value, err := canonicalIndex(fee.Value)
		if err != nil {
			t.Fatal(err)
		}
		origins = append(origins, OPL1Origin{Number: n, Hash: h.Hash, Timestamp: ts, BaseFeeWei: strconv.FormatUint(value, 10)})
	}
	ctx := context.Background()
	const head = 26453043
	const hash = "0x52fd70e5648c3caee20f1a2b51566ebaca6c5caba35880f668c3fa8cb9bc5239"
	read := func(h uint64) (json.RawMessage, error) { return headers[head-h], nil }
	if err := verifyOriginRange(ctx, head, hash, origins, read); err != nil {
		t.Fatal(err)
	}
	saved := origins[1].BaseFeeWei
	origins[1].BaseFeeWei = "18446744073709551616"
	if err := verifyOriginRange(ctx, head, hash, origins, read); err == nil || err.Error() != "OP_ORIGIN_L1_BASE_FEE_MISMATCH" {
		t.Fatal(err)
	}
	origins[1].BaseFeeWei = saved
	origins[1].Hash = origins[0].Hash
	if err := verifyOriginRange(ctx, head, hash, origins, read); err == nil || err.Error() != "OP_ORIGIN_L1_MISMATCH" {
		t.Fatal(err)
	}
}
