package evm

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

// RefreshOwnerOPSpan uses the owner's release, freshness limits and endpoint
// identities. The file lock serializes writers and rename publishes one span.
func RefreshOwnerOPSpan(ctx context.Context, snapshot *config.Snapshot, submissions []OPChannelSubmission) (*OPChannelSubmissionsReport, error) {
	return refreshOPSpan(ctx, snapshot, submissions, "", nil)
}

func RefreshLatestOPSpan(ctx context.Context, snapshot *config.Snapshot, beaconAPI string) (*OPChannelSubmissionsReport, error) {
	return refreshOPSpan(ctx, snapshot, nil, beaconAPI, nil)
}

// RefreshForwardOPSpan installs the first complete channel at an inclusive L1 cursor.
func RefreshForwardOPSpan(ctx context.Context, snapshot *config.Snapshot, beaconAPI, start string) (*OPChannelSubmissionsReport, error) {
	cursor, err := parseOPScanCursor(start)
	if err != nil {
		return nil, err
	}
	return refreshOPSpan(ctx, snapshot, nil, beaconAPI, &cursor)
}

func refreshOPSpan(ctx context.Context, snapshot *config.Snapshot, submissions []OPChannelSubmission, beaconAPI string, cursor *opScanCursor) (*OPChannelSubmissionsReport, error) {
	for _, chain := range snapshot.Config.Chains {
		policy := chain.OPOrigin
		if policy == nil || policy.BatchProofFile == "" {
			continue
		}
		timeout := time.Duration(snapshot.Config.Journal.TimeoutMS) * time.Millisecond
		l2 := rpc.New(chain.RPCURLs, timeout, snapshot.Config.Journal.MaxBytes)
		defer l2.HTTP.CloseIdleConnections()
		l1 := rpc.New([]string{policy.L1RPCURL}, timeout, snapshot.Config.Journal.MaxBytes)
		defer l1.HTTP.CloseIdleConnections()
		proofFile := snapshot.Resolve(policy.ProofFile)
		load := func() (*provenance.VerifiedExecutionFinality, error) {
			proof, err := provenance.ReadOnlineBeaconProof(proofFile, policy.CheckpointRoot)
			if err != nil {
				return nil, err
			}
			return proof, proof.CheckOnlineAge(time.Now(), policy.MaxCheckpointAgeMS, snapshot.Config.Startup.MaxAge)
		}
		proof, err := load()
		if err != nil {
			return nil, err
		}
		head, hash := proof.ExecutionBlock()
		genesis, err := knownBodyGenesis("evm:1")
		if err != nil {
			return nil, err
		}
		if err = l1.VerifyEVM(ctx, "evm:1", genesis); err != nil {
			return nil, err
		}
		if beaconAPI != "" {
			blobsAt, err := opBeaconBlobs(beaconAPI)
			if err != nil {
				return nil, err
			}
			state := l1
			if policy.L1ProofRPCURL != "" && policy.L1ProofRPCURL != policy.L1RPCURL {
				state = rpc.New([]string{policy.L1ProofRPCURL}, timeout, 2<<20)
				defer state.HTTP.CloseIdleConnections()
				if err = state.VerifyEVM(ctx, "evm:1", genesis); err != nil {
					return nil, err
				}
			}
			window := min(129, chain.NativeProof.MaxHeaders)
			if cursor != nil {
				submissions, err = collectOPForward(ctx, proof, l1, state, window, chain.NativeProof.MaxBytes, *cursor, blobsAt)
			} else {
				submissions, err = collectOPSubmission(ctx, proof, l1, state, window, chain.NativeProof.MaxBytes, blobsAt)
			}
			if err != nil {
				return nil, err
			}
		}
		observation, report, err := CaptureOPSpan(ctx, proof, submissions, l2, 128)
		if err != nil {
			return nil, err
		}
		span := report.Channel.Span
		err = verifyOriginRange(ctx, head, hash, span.origins, func(h uint64) (json.RawMessage, error) {
			var raw json.RawMessage
			err := l1.Call(ctx, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", h), false}, &raw)
			return raw, err
		})
		if err != nil {
			return nil, err
		}
		span.VerifiedL1Origins = len(span.origins)
		data, err := json.Marshal(observation)
		if err != nil {
			return nil, err
		}
		if len(data) > min(chain.NativeProof.MaxBytes, 32<<20) {
			return nil, errors.New("OP_BATCH_OBSERVATION_FILE")
		}
		gate := func() error {
			if err := ctx.Err(); err != nil {
				return err
			}
			current, err := load()
			if err != nil {
				return err
			}
			n, h := current.ExecutionBlock()
			if n != head || h != hash {
				return errors.New("OP_SPAN_REFRESH_HEAD_CHANGED")
			}
			return nil
		}
		if err = installOPSpan(ctx, proofFile, opObservationPath(proofFile, policy.BatchProofFile), data, span, gate, cursor != nil); err != nil {
			return nil, err
		}
		return report, nil
	}
	return nil, errors.New("CONFIG_OP_SPAN_REQUIRED")
}

func installOPSpan(ctx context.Context, proofFile, path string, data []byte, next *OPSpanReport, gate func() error, contiguous bool) error {
	lock, err := os.OpenFile(path+".lock", os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return err
	}
	defer lock.Close()
	if err = syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		return errors.New("OP_SPAN_REFRESH_BUSY")
	}
	if old, _, err := readOPChannelObservation(path, 32<<20); err == nil {
		// Full RPC bodies contain transaction objects. Read just the old watermark.
		var watermark struct{ Number, Hash string }
		if err = json.Unmarshal(old.Blocks[len(old.Blocks)-1], &watermark); err != nil {
			return err
		}
		h, err := canonicalIndex(watermark.Number)
		if err != nil {
			return err
		}
		if contiguous && (next.FirstBlock == 0 || h != next.FirstBlock-1 || watermark.Hash != next.firstParent) {
			return errors.New("OP_SPAN_REFRESH_DISCONTINUITY")
		}
		if contiguous {
			// The old tip's body must match the parent hash committed by the new span.
			body, err := VerifyChainBlockJSON(ctx, "evm:10", old.Blocks[len(old.Blocks)-1], 32<<20)
			if err != nil {
				return err
			}
			if len(body.RawTransactions) == 0 || len(next.origins) == 0 {
				return errors.New("OP_SPAN_ATTRIBUTES")
			}
			timestamp, err := canonicalIndex(body.Block.Timestamp)
			if err != nil {
				return err
			}
			previous, err := opBlockOrigin(&body, timestamp)
			if err != nil {
				return err
			}
			if err = checkOPOriginTransition(previous, next.origins[0]); err != nil {
				return err
			}
		}
		if h > next.LastBlock || (h >= next.FirstBlock && watermark.Hash != next.blockHashes[h-next.FirstBlock]) {
			return errors.New("OP_SPAN_REFRESH_CONFLICT")
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	dir := filepath.Dir(path)
	file, err := os.CreateTemp(dir, ".op-span-")
	if err != nil {
		return err
	}
	defer os.Remove(file.Name())
	if _, err = file.Write(data); err == nil {
		err = file.Sync()
	}
	closeErr := file.Close()
	if err != nil {
		return err
	}
	if closeErr != nil {
		return closeErr
	}
	// Pin the signed head across the final check, rename and directory sync.
	// Acquisition and staging above can run concurrently with beacon refresh.
	beaconLock, err := os.OpenFile(proofFile+".lock", os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return err
	}
	defer beaconLock.Close()
	if err = syscall.Flock(int(beaconLock.Fd()), syscall.LOCK_SH|syscall.LOCK_NB); err != nil {
		if errors.Is(err, syscall.EWOULDBLOCK) {
			return errors.New("OP_SPAN_BEACON_REFRESH_BUSY")
		}
		return err
	}
	if err = gate(); err != nil {
		return err
	}
	if err = os.Rename(file.Name(), path); err != nil {
		return err
	}
	parent, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer parent.Close()
	return parent.Sync()
}
