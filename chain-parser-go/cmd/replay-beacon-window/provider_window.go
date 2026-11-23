package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"time"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

type numberedObservation struct {
	Block       domain.BlockRef    `json:"block"`
	Reference   domain.SnapshotRef `json:"reference"`
	Kind        string             `json:"kind"`
	RecordID    string             `json:"record_id"`
	PayloadHash string             `json:"payload_hash"`
	Digest      string             `json:"staged_sha256"`
	SwapCount   int                `json:"swap_count"`
}

type numberedWindow struct {
	Initial       domain.Snapshot       `json:"initial_snapshot"`
	InitialRecord domain.Record         `json:"initial_record"`
	Rows          []numberedObservation `json:"rows"`
	Final         domain.Snapshot       `json:"final_snapshot"`
}

func replayProviderWindow(ctx context.Context, cfg *config.Snapshot, first, last uint64) (result *numberedWindow, err error) {
	if len(cfg.Config.Chains) != 1 || cfg.Config.Journal.Transport != "http-development" || cfg.Config.Control.Backend != "disabled" {
		return nil, errors.New("REPLAY_PROVIDER_PROFILE")
	}
	chain := cfg.Config.Chains[0]
	if chain.Confirmation != "finalized" || chain.BlockIntegrity != evm.ChainNativeCommitmentsV2 ||
		chain.BeaconFinality != nil || chain.OPOrigin != nil || len(chain.Pools) == 0 ||
		len(chain.RPCURLs) != 2 || chain.RPCURLs[0] == chain.RPCURLs[1] {
		return nil, errors.New("REPLAY_PROVIDER_PROFILE")
	}
	if err := checkReplayEndpoints(chain); err != nil {
		return nil, err
	}
	if first == 0 || last <= first || last-first > 128 || last-first > cfg.Config.Startup.MaxTail {
		return nil, errors.New("REPLAY_WINDOW")
	}
	client := rpc.New(chain.RPCURLs, time.Duration(cfg.Config.Journal.TimeoutMS)*time.Millisecond, cfg.Config.Journal.MaxBytes)
	defer client.HTTP.CloseIdleConnections()
	if err := client.VerifyEVM(ctx, chain.Key, chain.Genesis); err != nil {
		return nil, err
	}
	adapter := &evm.Adapter{Chain: chain, Client: client}
	// The archive verifier binds the initial and advancing finalized heads
	// to the captured endpoints. Every state read here uses an explicit height.
	block, pools, err := adapter.ReadState(ctx, fmt.Sprintf("0x%x", first), false)
	if err != nil {
		return nil, fmt.Errorf("decode baseline %d: %w", first, err)
	}
	if block.Number != first || block.ChainKey != chain.Key {
		return nil, errors.New("REPLAY_BLOCK")
	}
	initial, err := domain.NewCurrentStateBaseline(block, pools, chain.Owner, chain.Assignment, 0)
	if err != nil {
		return nil, err
	}
	initial.Reference.OnlineGeneration = 1
	initial.Reference.WriterEpoch = 1
	initial.Reference.JournalSeq = 1
	directory, err := os.MkdirTemp("", "xone-replay-journal-")
	if err != nil {
		return nil, err
	}
	defer func() { err = errors.Join(err, os.RemoveAll(directory)) }()
	store, err := reducer.Open(directory, chain.Owner, chain.Assignment, chain.ReorgWindow)
	if err != nil {
		return nil, err
	}
	defer func() { err = errors.Join(err, store.Close()) }()
	if err = store.InstallBaseline(initial); err != nil {
		return nil, err
	}
	records, err := store.Records(0, 1)
	if err != nil {
		return nil, err
	}
	if len(records) != 1 || records[0].Reference != initial.Reference {
		return nil, errors.New("REPLAY_JOURNAL_BASELINE")
	}
	report := &numberedWindow{
		Initial:       initial,
		InitialRecord: records[0],
		Rows:          make([]numberedObservation, 0, last-first),
	}
	current := initial
	for height := first + 1; ; height++ {
		block, pools, swaps, err := adapter.ReadComplete(ctx, fmt.Sprintf("0x%x", height))
		if err != nil {
			return nil, fmt.Errorf("decode %d: %w", height, err)
		}
		if block.Number != height || block.ChainKey != chain.Key || block.ParentHash != current.Reference.Block.Hash {
			return nil, errors.New("REPLAY_ANCESTRY")
		}
		digest, err := stagedDigest(block, pools, swaps)
		if err != nil {
			return nil, err
		}
		after := current.Reference.JournalSeq
		current.Reference.Block = block
		current.Reference.JournalSeq++
		current.Pools = pools
		if err = store.Commit(current); err != nil {
			return nil, err
		}
		records, err := store.Records(after, 1)
		if err != nil {
			return nil, err
		}
		if len(records) != 1 || records[0].Reference != current.Reference {
			return nil, errors.New("REPLAY_JOURNAL_RECORD")
		}
		record := records[0]
		report.Rows = append(report.Rows, numberedObservation{
			Block: block, Reference: record.Reference, Kind: record.Kind, RecordID: record.RecordID,
			PayloadHash: record.PayloadHash, Digest: digest, SwapCount: len(swaps),
		})
		if height == last {
			snapshot, ready := store.Current()
			if !ready || snapshot.Reference != current.Reference {
				return nil, errors.New("REPLAY_JOURNAL_SNAPSHOT")
			}
			report.Final = snapshot
			return report, nil
		}
	}
}
