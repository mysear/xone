package main

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"time"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
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
	Facts       *domain.FactBatch  `json:"normalized_facts,omitempty"`
}

type historyObservation struct {
	Block             domain.BlockRef `json:"block"`
	SwapCount         int             `json:"swap_count"`
	ScopeDatasetID    string          `json:"scope_dataset_id"`
	DatasetID         string          `json:"dataset_id"`
	NativeSemanticsID string          `json:"native_semantics_id"`
	Digest            string          `json:"evidence_sha256"`
}

type numberedWindow struct {
	Initial       domain.Snapshot       `json:"initial_snapshot"`
	InitialRecord domain.Record         `json:"initial_record"`
	Rows          []numberedObservation `json:"rows"`
	Final         domain.Snapshot       `json:"final_snapshot"`
	History       *historyObservation   `json:"history,omitempty"`
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
		facts := evm.V2JournalFacts(chain, pools, []domain.FactBlock{{Block: block, Swaps: swaps}}, nil)
		if err = store.ApplyLocal(journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: current, NormalizedFacts: facts}); err != nil {
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
			PayloadHash: record.PayloadHash, Digest: digest, SwapCount: len(swaps), Facts: record.NormalizedFacts,
		})
		if chain.Key == "evm:56" && len(swaps) != 0 && report.History == nil {
			report.History, err = replayHistoricalSwaps(ctx, adapter, block, record.NormalizedFacts)
			if err != nil {
				return nil, fmt.Errorf("decode history %d: %w", height, err)
			}
		}
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

// Re-read a nonempty block through the history adapter against the sealed
// responses. Verify its serialized native receipt evidence independently of
// state storage, then compare the result to the persisted online fact batch.
func replayHistoricalSwaps(ctx context.Context, adapter *evm.Adapter, block domain.BlockRef, published *domain.FactBatch) (*historyObservation, error) {
	if published == nil || len(published.RevertedBlocks) != 0 {
		return nil, errors.New("REPLAY_HISTORY_FACTS_MISMATCH")
	}
	history := adapter.CopyWithChain(adapter.Chain)
	var raw []byte
	history.OnHistoryEvidence = func(_ context.Context, value evm.NativeHistoryEvidence) error {
		var err error
		raw, err = json.Marshal(value)
		return err
	}
	batch, err := history.ReadFacts(ctx, fmt.Sprintf("0x%x", block.Number))
	if err != nil {
		return nil, err
	}
	var evidence evm.NativeHistoryEvidence
	if err = json.Unmarshal(raw, &evidence); err != nil {
		return nil, err
	}
	facts, _, err := evm.VerifyNativeHistoryEvidence(ctx, history.Chain, evidence)
	if err != nil {
		return nil, err
	}
	// The journal record materializes empty protobuf arrays. Historical
	// acquisition uses nil for its empty list of reverted blocks.
	normalized := *published
	normalized.RevertedBlocks = nil
	live, err := domain.Hash(normalized)
	if err != nil {
		return nil, err
	}
	calculated, err := domain.Hash(batch)
	if err != nil {
		return nil, err
	}
	verified, err := domain.Hash(facts)
	if err != nil {
		return nil, err
	}
	if live != calculated || live != verified || len(facts.Blocks) != 1 || facts.Blocks[0].Block != block {
		return nil, errors.New("REPLAY_HISTORY_FACTS_MISMATCH")
	}
	return &historyObservation{
		Block: block, SwapCount: len(facts.Blocks[0].Swaps), ScopeDatasetID: evidence.ScopeDatasetID,
		DatasetID: evidence.DatasetID, NativeSemanticsID: evidence.NativeSemanticsID,
		Digest: fmt.Sprintf("%x", sha256.Sum256(raw)),
	}, nil
}
