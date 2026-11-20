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
		observation, report, err := CaptureOPSpan(ctx, proof, submissions, l2, 128)
		if err != nil {
			return nil, err
		}
		genesis, err := knownBodyGenesis("evm:1")
		if err != nil {
			return nil, err
		}
		if err = l1.VerifyEVM(ctx, "evm:1", genesis); err != nil {
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
		if err = installOPSpan(opObservationPath(proofFile, policy.BatchProofFile), data, span, gate); err != nil {
			return nil, err
		}
		return report, nil
	}
	return nil, errors.New("CONFIG_OP_SPAN_REQUIRED")
}

func installOPSpan(path string, data []byte, next *OPSpanReport, gate func() error) error {
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
