package evm

import (
	"context"
	"encoding/json"
	"testing"
)

// These are the original five blobs from one signed mainnet transaction. Their
// split/replacement placement is synthetic; it does not claim a captured
// multi-submission channel. The normal signed collector is tested separately.
func TestOPForwardChannelBoundaryRecovery(t *testing.T) {
	var evidence OPBatchEvidence
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &evidence)
	blobs := evidence.Blobs[3:8]
	ctx := context.Background()
	checkComplete := func(t *testing.T, channel *opForwardChannelFrames, first uint64) {
		t.Helper()
		report, err := channel.frames.decode(ctx, nil)
		if err != nil || channel.first != first || report.Frames != 5 || report.DecodedSHA256 != "d2d00254efa34182369af0b770589a2c08e63e48f8af3cb885d111144236bdf5" {
			t.Fatal(report, channel.first, err)
		}
	}
	t.Run("continuation", func(t *testing.T) {
		var channel opForwardChannelFrames
		if accepted, restarted, err := channel.append(ctx, 100, opForwardHoloceneTime, blobs[:2]); err != nil || !accepted || restarted || channel.frames.closed {
			t.Fatal(accepted, restarted, err)
		}
		if accepted, restarted, err := channel.append(ctx, 101, opForwardHoloceneTime, blobs[2:]); err != nil || !accepted || restarted {
			t.Fatal(accepted, restarted, err)
		}
		checkComplete(t, &channel, 100)
	})
	t.Run("first frame replaces abandoned channel", func(t *testing.T) {
		var channel opForwardChannelFrames
		if _, _, err := channel.append(ctx, 100, opForwardHoloceneTime, blobs[:2]); err != nil {
			t.Fatal(err)
		}
		if accepted, restarted, err := channel.append(ctx, 101, opForwardHoloceneTime, blobs); err != nil || !accepted || !restarted {
			t.Fatal(accepted, restarted, err)
		}
		checkComplete(t, &channel, 101)
	})
	t.Run("orphan frames do not prevent later channel", func(t *testing.T) {
		var channel opForwardChannelFrames
		if accepted, restarted, err := channel.append(ctx, 100, opForwardHoloceneTime, blobs[2:]); err != nil || accepted || restarted || channel.frames.count != 0 {
			t.Fatal(accepted, restarted, err)
		}
		if accepted, restarted, err := channel.append(ctx, 101, opForwardHoloceneTime, blobs); err != nil || !accepted || restarted {
			t.Fatal(accepted, restarted, err)
		}
		checkComplete(t, &channel, 101)
	})
	t.Run("timeout resets before next transaction", func(t *testing.T) {
		var channel opForwardChannelFrames
		if _, _, err := channel.append(ctx, 100, opForwardHoloceneTime, blobs[:2]); err != nil {
			t.Fatal(err)
		}
		if channel.expire(150) || !channel.expire(151) || channel.frames.count != 0 || channel.first != 0 {
			t.Fatal("incorrect inclusive channel timeout")
		}
		if accepted, _, err := channel.append(ctx, 151, opForwardHoloceneTime, blobs[2:]); err != nil || accepted {
			t.Fatal("accepted timed-out continuation", accepted, err)
		}
		if accepted, restarted, err := channel.append(ctx, 152, opForwardHoloceneTime, blobs); err != nil || !accepted || restarted {
			t.Fatal(accepted, restarted, err)
		}
		checkComplete(t, &channel, 152)
	})
	t.Run("mixed orphan and first frames remain unsupported", func(t *testing.T) {
		var channel opForwardChannelFrames
		mixed := append(evidence.Blobs[4:5:5], evidence.Blobs[3])
		if accepted, _, err := channel.append(ctx, 100, opForwardHoloceneTime, mixed); err == nil || err.Error() != "OP_FRAME_ORDER" || accepted || channel.frames.count != 0 {
			t.Fatal(accepted, err)
		}
	})
	t.Run("wrong active continuation remains rejected", func(t *testing.T) {
		var channel opForwardChannelFrames
		if _, _, err := channel.append(ctx, 100, opForwardHoloceneTime, blobs[:2]); err != nil {
			t.Fatal(err)
		}
		if accepted, _, err := channel.append(ctx, 101, opForwardHoloceneTime, blobs[3:]); err == nil || err.Error() != "OP_FRAME_ORDER" || accepted {
			t.Fatal(accepted, err)
		}
	})
	t.Run("pre-Holocene retains strict channel behavior", func(t *testing.T) {
		var channel opForwardChannelFrames
		if _, _, err := channel.append(ctx, 100, opForwardHoloceneTime-1, blobs[:2]); err != nil {
			t.Fatal(err)
		}
		if accepted, restarted, err := channel.append(ctx, 101, opForwardHoloceneTime-1, blobs); err == nil || err.Error() != "OP_FRAME_ORDER" || accepted || restarted {
			t.Fatal(accepted, restarted, err)
		}
		var orphan opForwardChannelFrames
		if accepted, _, err := orphan.append(ctx, 100, opForwardHoloceneTime-1, blobs[2:]); err == nil || err.Error() != "OP_FRAME_ORDER" || accepted {
			t.Fatal(accepted, err)
		}
	})
	t.Run("cancellation", func(t *testing.T) {
		canceled, cancel := context.WithCancel(ctx)
		cancel()
		var channel opForwardChannelFrames
		if accepted, _, err := channel.append(canceled, 100, opForwardHoloceneTime, blobs); err != context.Canceled || accepted || channel.frames.count != 0 {
			t.Fatal(accepted, err)
		}
	})
}

// Exercise the same state transitions used by the collector, including retained
// proof metadata and both budgets. Only frame placement and labels are synthetic.
func TestOPForwardScanRecoveryDiscardsEvidence(t *testing.T) {
	var evidence OPBatchEvidence
	readOriginFixture(t, "op-mainnet-batches/batch-26453043.json.gz", &evidence)
	blobs := evidence.Blobs[3:8]
	ctx := context.Background()
	for _, mode := range []string{"replacement", "timeout"} {
		t.Run(mode, func(t *testing.T) {
			scan := newOPForwardScan(32 << 20)
			if err := scan.beginHeight(100, opForwardHoloceneTime); err != nil {
				t.Fatal(err)
			}
			if err := scan.charge(123); err != nil {
				t.Fatal(err)
			}
			if err := scan.candidate(); err != nil {
				t.Fatal(err)
			}
			old := OPChannelSubmission{Evidence: OPBatchEvidence{TxHash: "abandoned", Block: json.RawMessage(`{"old":true}`)}}
			if closed, err := scan.append(ctx, 100, opForwardHoloceneTime, blobs[:2], old); err != nil || closed {
				t.Fatal(closed, err)
			}
			// A selected transaction means this height is already represented.
			scan.endHeight(json.RawMessage(`{"same_height":true}`), nil)
			if len(scan.between) != 0 {
				t.Fatal("retained selected height as intervening")
			}
			if err := scan.beginHeight(101, opForwardHoloceneTime); err != nil {
				t.Fatal(err)
			}
			oldAuthority := &OPBatchSenderProof{Proxy: json.RawMessage(`{"old_proof":true}`)}
			scan.endHeight(json.RawMessage(`{"intervening":true}`), oldAuthority)
			if len(scan.between) != 1 || scan.between[0].Authority != oldAuthority || len(scan.submissions) != 1 || scan.encodedBudget >= scan.encodedLimit {
				t.Fatal("missing staged metadata")
			}
			height := uint64(102)
			if mode == "timeout" {
				if err := scan.beginHeight(150, opForwardHoloceneTime); err != nil || len(scan.submissions) != 1 {
					t.Fatal("expired at inclusive boundary", err)
				}
				if err := scan.beginHeight(151, opForwardHoloceneTime); err != nil {
					t.Fatal(err)
				}
				if len(scan.submissions) != 0 || len(scan.between) != 0 || scan.encodedBudget != scan.encodedLimit {
					t.Fatal("expiry retained prior evidence or output charge")
				}
				if closed, err := scan.append(ctx, 151, opForwardHoloceneTime, blobs[2:], old); err != nil || closed || len(scan.submissions) != 0 {
					t.Fatal("retained orphan evidence", closed, err)
				}
				height = 152
			}
			if err := scan.beginHeight(height, opForwardHoloceneTime); err != nil {
				t.Fatal(err)
			}
			if err := scan.charge(321); err != nil {
				t.Fatal(err)
			}
			if err := scan.candidate(); err != nil {
				t.Fatal(err)
			}
			next := OPChannelSubmission{Evidence: OPBatchEvidence{TxHash: "replacement", Block: json.RawMessage(`{"next":true}`)}}
			if closed, err := scan.append(ctx, height, opForwardHoloceneTime, blobs, next); err != nil || !closed {
				t.Fatal(closed, err)
			}
			if len(scan.submissions) != 1 || scan.submissions[0].Evidence.TxHash != "replacement" || len(scan.submissions[0].Intervening) != 0 || len(scan.between) != 0 || scan.channel.first != height {
				t.Fatal("replacement retained discarded evidence", scan.submissions)
			}
			wire, err := json.Marshal(next)
			if err != nil || scan.encodedBudget != scan.encodedLimit-len(wire) {
				t.Fatal("output allowance was not reset", err)
			}
			if scan.remaining != (32<<20)-444 || scan.candidates != 2 {
				t.Fatal("recovery reset cumulative work", scan.remaining, scan.candidates)
			}
			for scan.candidates < 128 {
				if err := scan.candidate(); err != nil {
					t.Fatal(err)
				}
			}
			if err := scan.charge(scan.remaining); err != nil {
				t.Fatal(err)
			}
			if err := scan.beginHeight(height+51, opForwardHoloceneTime); err != nil {
				t.Fatal(err)
			}
			if err := scan.candidate(); err == nil || err.Error() != "OP_CHANNEL_SUBMISSION_COUNT" {
				t.Fatal("recovery replenished candidate allowance", err)
			}
			if err := scan.charge(1); err == nil || err.Error() != "OP_BATCH_SEARCH_BUDGET" {
				t.Fatal("recovery replenished read allowance", err)
			}
		})
	}
}
