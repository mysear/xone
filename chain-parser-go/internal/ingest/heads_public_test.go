package ingest_test

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

const publicHeadChain = "evm:1"

type publicHeadSource struct {
	ws  string
	rpc *rpc.Client
	id  string
}

type publicFixedBlock struct {
	Body     json.RawMessage
	Receipts json.RawMessage
}

// TestPublicEthereumHeadRecovery exercises real endpoints with a local archive
// and a commit observer. Run with XONE_PUBLIC_EVM_HEADS=1.
func TestPublicEthereumHeadRecovery(t *testing.T) {
	if os.Getenv("XONE_PUBLIC_EVM_HEADS") != "1" {
		t.Skip("requires live Ethereum mainnet RPC")
	}
	const (
		genesis     = "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
		window      = 8
		maxResponse = 8 << 20
	)
	ctx, cancel := context.WithTimeout(context.Background(), 4*time.Minute)
	defer cancel()
	sources := [2]publicHeadSource{}
	for i, host := range []string{"ethereum-rpc.publicnode.com", "eth.drpc.org"} {
		client := rpc.New([]string{"https://" + host}, 12*time.Second, maxResponse)
		if err := client.VerifyEVM(ctx, publicHeadChain, genesis); err != nil {
			t.Fatalf("verify %s genesis: %v", host, err)
		}
		sources[i] = publicHeadSource{ws: "wss://" + host, rpc: client, id: client.Providers()[0].ID}
		t.Logf("verified chain=%s genesis=%s source=%s http_id=%s", publicHeadChain, genesis, host, sources[i].id)
	}

	baselineHint := commonPublicWSHead(t, ctx, sources, 1)
	readHeader := func(ctx context.Context, height uint64) (ingest.HTTPHead, error) {
		return sources[0].header(ctx, fmt.Sprintf("0x%x", height))
	}
	verifyHead := func(ctx context.Context, head ingest.HTTPHead) error {
		other, err := sources[1].header(ctx, fmt.Sprintf("0x%x", head.Height))
		if err != nil {
			return err
		}
		if head != other {
			return fmt.Errorf("provider disagreement at height %d: %+v / %+v", head.Height, head, other)
		}
		return nil
	}
	baseline, err := readHeader(ctx, baselineHint.Height)
	if err != nil {
		t.Fatal(err)
	}
	if err := verifyHead(ctx, baseline); err != nil {
		t.Fatal(err)
	}
	assertPublicHint(t, baselineHint, baseline)
	cursor := ingest.Cursor{Height: baseline.Height, Hash: baseline.Hash}

	archive, err := ingest.OpenArchive(t.TempDir(), ingest.ArchiveLimits{
		MaxBytes: 32 * maxResponse, MaxRecords: 2 * window, MaxRecordBytes: 4 * maxResponse,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer func() {
		if err := archive.Close(); err != nil {
			t.Error(err)
		}
	}()
	var commits uint64
	pipe := &ingest.Pipeline{
		Chain: publicHeadChain, Archive: archive,
		Limits: ingest.Limits{Workers: 2, MaxTasks: 2, MaxBytes: 8 * maxResponse, MaxBlockBytes: 4 * maxResponse},
		Fetch: func(ctx context.Context, height uint64, limit int64) (ingest.Fetched, error) {
			first, identity, err := sources[0].block(ctx, height)
			if err != nil {
				return ingest.Fetched{}, err
			}
			raw, err := json.Marshal(first)
			if err != nil {
				return ingest.Fetched{}, err
			}
			if 2*int64(len(raw)) > limit {
				return ingest.Fetched{}, ingest.ErrBudget
			}
			return ingest.Fetched{Height: height, Hash: identity.Hash, ParentHash: identity.ParentHash, ProviderID: sources[0].id, Raw: raw}, nil
		},
		Decode: func(_ context.Context, block ingest.Fetched) ([]byte, error) { return block.Raw, nil },
		Validate: func(ctx context.Context, block ingest.Fetched, decoded []byte) error {
			var raw publicFixedBlock
			if err := json.Unmarshal(decoded, &raw); err != nil {
				return err
			}
			header, err := verifyPublicFixedBlock(ctx, raw, 8*maxResponse)
			if err != nil {
				return fmt.Errorf("%s: %w", sources[0].id, err)
			}
			if header.Block.Number != fmt.Sprintf("0x%x", block.Height) ||
				header.Block.Hash != block.Hash || header.Block.ParentHash != block.ParentHash {
				return errors.New("staged block identity mismatch")
			}
			second, _, err := sources[1].block(ctx, block.Height)
			if err != nil {
				return err
			}
			witness, err := verifyPublicFixedBlock(ctx, second, 8*maxResponse)
			if err != nil {
				return fmt.Errorf("%s: %w", sources[1].id, err)
			}
			if header.Block.Number != witness.Block.Number || header.Block.Hash != witness.Block.Hash {
				return fmt.Errorf("block commitment disagreement at %d", block.Height)
			}
			return nil
		},
		Commit: func(_ context.Context, expected ingest.Cursor, block ingest.Block) error {
			if block.Height != expected.Height+1 || block.ParentHash != expected.Hash {
				return errors.New("staged block committed out of parent order")
			}
			if _, err := archive.Read(block.Raw); err != nil {
				return err
			}
			commits++
			t.Logf("commit height=%d hash=%s parent=%s raw_digest=%s", block.Height, block.Hash, block.ParentHash, block.Raw.Digest)
			return nil
		},
	}
	tracker := &ingest.HeadTracker{
		Chain: publicHeadChain, Pipeline: pipe, ReadHeader: readHeader, VerifyHead: verifyHead,
		Limits: ingest.HeadLimits{TailWindow: window, Timeout: 80 * time.Second, PollInterval: 4 * time.Second, MaxChecks: 1},
		ReadHead: func(ctx context.Context) (ingest.HTTPHead, error) {
			first, err := sources[0].header(ctx, "latest")
			if err != nil {
				return ingest.HTTPHead{}, err
			}
			second, err := sources[1].header(ctx, "latest")
			if err != nil {
				return ingest.HTTPHead{}, err
			}
			if second.Height < first.Height {
				return readHeader(ctx, second.Height)
			}
			return first, nil
		},
	}

	// Both subscriptions above are closed before the two-block interval begins.
	ticker := time.NewTicker(4 * time.Second)
	defer ticker.Stop()
	for {
		head, err := tracker.ReadHead(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if head.Height >= cursor.Height+2 {
			break
		}
		select {
		case <-ticker.C:
		case <-ctx.Done():
			t.Fatal(ctx.Err())
		}
	}
	reconnectedHint := commonPublicWSHead(t, ctx, sources, cursor.Height+2)
	reconnected, err := readHeader(ctx, reconnectedHint.Height)
	if err != nil {
		t.Fatal(err)
	}
	if err := verifyHead(ctx, reconnected); err != nil {
		t.Fatal(err)
	}
	assertPublicHint(t, reconnectedHint, reconnected)
	recovery, err := tracker.CatchUp(ctx, cursor)
	if err != nil {
		t.Fatal(err)
	}
	if recovery.Action != ingest.HeadTailCommitted || recovery.HistoryGap != nil ||
		recovery.Head.Height < reconnected.Height || recovery.Cursor.Height != recovery.Head.Height ||
		recovery.Cursor.Hash != recovery.Head.Hash || commits != recovery.Cursor.Height-cursor.Height {
		t.Fatalf("unexpected recovery: %+v, commits=%d; websocket height=%d", recovery, commits, reconnected.Height)
	}

	// Tightening the tail budget must surface the missing interval before staging.
	tracker.Limits.TailWindow = 1
	limited, err := tracker.CatchUp(ctx, cursor)
	if err != nil {
		t.Fatal(err)
	}
	if limited.Action != ingest.HeadReanchor || limited.Cursor != cursor || limited.HistoryGap == nil ||
		limited.HistoryGap.From != cursor.Height+1 || limited.HistoryGap.To != limited.Head.Height ||
		limited.HistoryGap.Reason != "tail_window_exceeded" || commits != recovery.Cursor.Height-cursor.Height {
		t.Fatalf("unexpected tail-limit result: %+v, commits=%d", limited, commits)
	}
	t.Logf("recovered %d..%d, over-limit gap %d..%d", cursor.Height+1, recovery.Cursor.Height, limited.HistoryGap.From, limited.HistoryGap.To)
}

func commonPublicWSHead(t *testing.T, parent context.Context, sources [2]publicHeadSource, minimum uint64) rpc.HeadSignal {
	t.Helper()
	ctx, cancel := context.WithTimeout(parent, 65*time.Second)
	defer cancel()
	var streams [2]*rpc.HeadStream
	for i, source := range sources {
		stream, err := rpc.WatchHeads(ctx, source.ws, rpc.WSLimits{
			DialTimeout: 8 * time.Second, IdleTimeout: 24 * time.Second, SessionTimeout: time.Minute,
			MaxMessageBytes: 64 << 10, Queue: 16, MaxMessages: 16, MaxConnections: 2,
			ReconnectBackoff: time.Second, MaxBackoff: 2 * time.Second,
		})
		if err != nil {
			t.Fatal(err)
		}
		streams[i] = stream
		defer func() {
			cancel()
			<-stream.Done
		}()
	}
	seen := [2]map[uint64]rpc.HeadSignal{{}, {}}
	for {
		var hint rpc.HeadSignal
		var i int
		var ok bool
		select {
		case hint, ok = <-streams[0].Hints:
		case hint, ok = <-streams[1].Hints:
			i = 1
		case <-ctx.Done():
			t.Fatal(ctx.Err())
		}
		if !ok {
			t.Fatalf("stream %s ended: %v (%+v)", sources[i].id, <-streams[i].Done, streams[i].Snapshot())
		}
		if hint.ProviderID != streams[i].ProviderID {
			t.Fatalf("wrong websocket provider: %+v", hint)
		}
		if hint.Kind != rpc.WSHeadAvailable || hint.Height < minimum {
			continue
		}
		other, matches := seen[1-i][hint.Height]
		if matches && hint.Hash == other.Hash && hint.ParentHash == other.ParentHash {
			t.Logf("websocket ids=%s,%s height=%d hash=%s parent=%s", hint.ProviderID, other.ProviderID, hint.Height, hint.Hash, hint.ParentHash)
			return hint
		}
		seen[i][hint.Height] = hint
	}
}

func assertPublicHint(t *testing.T, hint rpc.HeadSignal, head ingest.HTTPHead) {
	t.Helper()
	if head.Height != hint.Height || head.Hash != hint.Hash || head.ParentHash != hint.ParentHash {
		t.Fatalf("websocket and HTTP disagree: %+v / %+v", hint, head)
	}
}

func (s publicHeadSource) header(ctx context.Context, tag string) (ingest.HTTPHead, error) {
	var raw json.RawMessage
	if err := s.rpc.Call(ctx, "eth_getBlockByNumber", []any{tag, false}, &raw); err != nil {
		return ingest.HTTPHead{}, fmt.Errorf("%s header %s: %w", s.id, tag, err)
	}
	header, err := evm.VerifyChainHeaderJSON(ctx, publicHeadChain, raw)
	if err != nil {
		return ingest.HTTPHead{}, fmt.Errorf("%s header %s: %w", s.id, tag, err)
	}
	block := header.Block
	height, err := strconv.ParseUint(strings.TrimPrefix(block.Number, "0x"), 16, 64)
	return ingest.HTTPHead{Chain: publicHeadChain, Height: height, Hash: block.Hash, ParentHash: block.ParentHash}, err
}

func (s publicHeadSource) block(ctx context.Context, height uint64) (publicFixedBlock, evm.RawBlock, error) {
	var sample publicFixedBlock
	var identity evm.RawBlock
	tag := fmt.Sprintf("0x%x", height)
	if err := s.rpc.Call(ctx, "eth_getBlockByNumber", []any{tag, true}, &sample.Body); err != nil {
		return sample, identity, fmt.Errorf("%s block %s: %w", s.id, tag, err)
	}
	// Extract only the header identity before validating the complete body.
	var header struct {
		Number, Hash, ParentHash string
	}
	if err := json.Unmarshal(sample.Body, &header); err != nil {
		return sample, identity, err
	}
	if header.Number != tag || header.Hash == "" || header.ParentHash == "" {
		return sample, identity, fmt.Errorf("%s returned an invalid block identity for %s", s.id, tag)
	}
	identity = evm.RawBlock{Number: header.Number, Hash: header.Hash, ParentHash: header.ParentHash}
	if err := s.rpc.Call(ctx, "eth_getBlockReceipts", []any{identity.Hash}, &sample.Receipts); err != nil {
		return sample, identity, fmt.Errorf("%s receipts %s: %w", s.id, tag, err)
	}
	return sample, identity, nil
}

func verifyPublicFixedBlock(ctx context.Context, sample publicFixedBlock, budget int) (evm.ChainHeaderCommitment, error) {
	proof, err := evm.VerifyChainBlockJSON(ctx, publicHeadChain, sample.Body, budget)
	if err != nil {
		return proof.HeaderCommitment, err
	}
	var receipts []evm.Receipt
	if err := json.Unmarshal(sample.Receipts, &receipts); err != nil {
		return proof.HeaderCommitment, err
	}
	if receipts == nil {
		return proof.HeaderCommitment, errors.New("missing block receipt list")
	}
	return proof.HeaderCommitment, evm.VerifyChainReceiptRoot(ctx, publicHeadChain, proof.Block, receipts)
}
