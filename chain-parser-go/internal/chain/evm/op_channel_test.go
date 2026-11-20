package evm

import (
	"context"
	"testing"

	"github.com/ethereum/go-ethereum/crypto/kzg4844"
)

func TestOPChannelFiveCommittedFrames(t *testing.T) {
	var evidence OPBatchEvidence
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &evidence)
	blobs := evidence.Blobs[3:8]
	ctx := context.Background()
	report, err := decodeOPChannel(ctx, blobs)
	if err != nil || report.Frames != 5 || report.ID != "912c873f4ac241003e7e83ea8738dc67" || report.CompressedBytes != 614073 || report.DecodedBytes != 1202848 || report.DecodedSHA256 != "d2d00254efa34182369af0b770589a2c08e63e48f8af3cb885d111144236bdf5" || report.SpanBatches != 1 || report.SingularBatches != 0 {
		t.Fatal(report, err)
	}
	for _, tc := range []struct {
		name  string
		blobs []*kzg4844.Blob
	}{
		{"missing last", blobs[:4]}, {"missing middle", []*kzg4844.Blob{blobs[0], blobs[2]}}, {"reversed first two", []*kzg4844.Blob{blobs[1], blobs[0]}}, {"duplicate", []*kzg4844.Blob{blobs[0], blobs[0]}}, {"after close", append(append([]*kzg4844.Blob{}, blobs...), blobs[0])},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if out, err := decodeOPChannel(ctx, tc.blobs); err == nil || out != nil {
				t.Fatal(out, err)
			}
		})
	}
	for _, at := range []int{0, 32, len(*blobs[4]) - 1} {
		bad := *blobs[4]
		bad[at] ^= 0x80
		if _, err := unpackOPBlob(&bad); err == nil {
			t.Fatal("bad packing", at)
		}
	}
	payload, err := unpackOPBlob(blobs[0])
	if err != nil {
		t.Fatal(err)
	}
	for _, n := range []int{0, 1, 4, 22, len(payload) - 1} {
		var frames opFrames
		if err := frames.appendPayload(payload[:n]); err == nil {
			t.Fatal("short frame", n)
		}
	}
}
