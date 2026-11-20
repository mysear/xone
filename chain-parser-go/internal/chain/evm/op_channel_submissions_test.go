package evm

import (
	"context"
	"testing"
)

func TestOPChannelSubmissionBoundaries(t *testing.T) {
	var evidence OPBatchEvidence
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &evidence)
	blobs := evidence.Blobs[3:8]
	ctx := context.Background()
	for split := 1; split < len(blobs); split++ {
		var frames opFrames
		if err := frames.appendBlobs(ctx, blobs[:split]); err != nil {
			t.Fatal(err)
		}
		if report, err := frames.decode(ctx, nil); err == nil || report != nil {
			t.Fatal("accepted incomplete channel")
		}
		if err := frames.appendBlobs(ctx, blobs[split:]); err != nil {
			t.Fatal(err)
		}
		report, err := frames.decode(ctx, nil)
		if err != nil || report.DecodedSHA256 != "d2d00254efa34182369af0b770589a2c08e63e48f8af3cb885d111144236bdf5" {
			t.Fatal(report, err)
		}
	}
}

func TestOPChannelSubmissionOrderingAndTimeout(t *testing.T) {
	first := OPBatchReport{inclusionTime: 1795138335, L1Number: 100, FirstBlobIndex: 3}
	// The previous transaction occupies five blob positions, including index 7.
	first.VersionedHashes = append(first.VersionedHashes, [32]byte{}, [32]byte{}, [32]byte{}, [32]byte{}, [32]byte{})
	for _, tc := range []struct {
		height uint64
		index  int
		ok     bool
	}{{99, 8, false}, {100, 3, false}, {100, 7, false}, {100, 8, true}, {150, 0, true}, {151, 0, false}} {
		next := OPBatchReport{inclusionTime: first.inclusionTime, L1Number: tc.height, FirstBlobIndex: tc.index}
		err := checkOPChannelPosition(first, first, next)
		if (err == nil) != tc.ok {
			t.Fatal(tc, err)
		}
	}
}
