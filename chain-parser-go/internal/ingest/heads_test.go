package ingest

import (
	"context"
	"errors"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/rpc"
)

func headHash(s string) string { return "0x" + strings.Repeat(s, 32) }
func trackerFixture(t *testing.T) (*HeadTracker, *atomic.Int32) {
	p, _ := fixture(t)
	var commits atomic.Int32
	p.Fetch = func(ctx context.Context, h uint64, max int64) (Fetched, error) {
		return Fetched{Height: h, Hash: headHash("ab"), ParentHash: headHash("ab"), ProviderID: "http-source", Raw: []byte("raw")}, nil
	}
	p.Commit = func(context.Context, Cursor, Block) error { commits.Add(1); return nil }
	return &HeadTracker{Chain: "evm:1", Pipeline: p, Limits: HeadLimits{TailWindow: 3, Timeout: time.Second, PollInterval: 20 * time.Millisecond, MaxChecks: 4},
		ReadHead: func(context.Context) (HTTPHead, error) {
			return HTTPHead{Chain: "evm:1", Height: 2, Hash: headHash("ab"), ParentHash: headHash("ab")}, nil
		},
		ReadHeader: func(ctx context.Context, h uint64) (HTTPHead, error) {
			return HTTPHead{Chain: "evm:1", Height: h, Hash: headHash("ab"), ParentHash: headHash("ab")}, nil
		},
		VerifyHead: func(context.Context, HTTPHead) error { return nil },
	}, &commits
}
func TestHEADFirstShortTailAndBeyondWindowHistoryGap(t *testing.T) {
	tr, commits := trackerFixture(t)
	result, err := tr.CatchUp(context.Background(), Cursor{Height: 1, Hash: headHash("ab")})
	if err != nil || result.Action != HeadTailCommitted || result.Cursor.Height != 2 || commits.Load() != 1 {
		t.Fatal(result, err, commits.Load())
	}
	tr.ReadHead = func(context.Context) (HTTPHead, error) {
		return HTTPHead{Chain: "evm:1", Height: 100, Hash: headHash("ab"), ParentHash: headHash("ab")}, nil
	}
	tr.ReadHeader = func(context.Context, uint64) (HTTPHead, error) {
		t.Error("old cursor read before HEAD_FIRST decision")
		return HTTPHead{}, nil
	}
	result, err = tr.CatchUp(context.Background(), Cursor{Height: 2, Hash: headHash("ab")})
	if err != nil || result.Action != HeadReanchor || result.Head.Height != 100 || result.Cursor.Height != 2 || result.HistoryGap == nil || result.HistoryGap.From != 3 || result.HistoryGap.To != 100 || commits.Load() != 1 {
		t.Fatal(result, err)
	}
}
func TestWSFakeHeadCannotAdvanceCursorAndPollingSurvivesClosedHints(t *testing.T) {
	tr, commits := trackerFixture(t)
	hints := make(chan rpc.HeadSignal, 1)
	hints <- rpc.HeadSignal{Kind: rpc.WSHeadAvailable, Height: 999999, Hash: headHash("ff")}
	close(hints)
	result, err := tr.Follow(context.Background(), Cursor{Height: 1, Hash: headHash("ab")}, hints, nil)
	if !errors.Is(err, ErrHeadCheckBudget) || result.Cursor.Height != 2 || commits.Load() != 1 {
		t.Fatal(result, err, commits.Load())
	}
}
func TestHeadHTTPProofPinnedBlockAndForkBarriers(t *testing.T) {
	for _, mode := range []string{"proof", "fork", "pinned-block", "canceled"} {
		t.Run(mode, func(t *testing.T) {
			tr, commits := trackerFixture(t)
			ctx := context.Background()
			switch mode {
			case "proof":
				tr.VerifyHead = func(context.Context, HTTPHead) error { return errors.New("untrusted HTTP header") }
			case "fork":
				tr.ReadHeader = func(context.Context, uint64) (HTTPHead, error) {
					return HTTPHead{Chain: "evm:1", Height: 1, Hash: headHash("ee"), ParentHash: headHash("ab")}, nil
				}
			case "pinned-block":
				tr.Pipeline.Fetch = func(context.Context, uint64, int64) (Fetched, error) {
					return Fetched{Height: 2, Hash: headHash("ee"), ParentHash: headHash("ab"), ProviderID: "p", Raw: []byte("wrong")}, nil
				}
			case "canceled":
				c, cancel := context.WithCancel(ctx)
				cancel()
				ctx = c
			}
			result, err := tr.CatchUp(ctx, Cursor{Height: 1, Hash: headHash("ab")})
			if commits.Load() != 0 || result.Cursor.Height != 1 {
				t.Fatal(result, err)
			}
			if mode == "fork" {
				if err != nil || result.Action != HeadReanchor {
					t.Fatal(result, err)
				}
			} else if err == nil {
				t.Fatal("HTTP/pin barrier bypassed", result)
			}
		})
	}
}

func TestHeadTrackerDoesNotBypassPipelineCommitActor(t *testing.T) {
	tr, commits := trackerFixture(t)
	tr.Pipeline.running.Store(true)
	defer tr.Pipeline.running.Store(false)
	result, err := tr.CatchUp(context.Background(), Cursor{Height: 1, Hash: headHash("ab")})
	if err == nil || result.Cursor.Height != 1 || commits.Load() != 0 {
		t.Fatal("shared pipeline actor bypassed", result, err, commits.Load())
	}
}

func TestWSHintTriggersHTTPBeforeFallbackPollWithoutUsingHintHeight(t *testing.T) {
	tr, commits := trackerFixture(t)
	tr.Limits.PollInterval = time.Second
	tr.Limits.MinCheckInterval = 20 * time.Millisecond
	var height atomic.Uint64
	height.Store(1)
	first := make(chan struct{})
	var calls atomic.Int32
	tr.ReadHead = func(context.Context) (HTTPHead, error) {
		h := height.Load()
		if calls.Add(1) == 1 {
			close(first)
		}
		return HTTPHead{Chain: "evm:1", Height: h, Hash: headHash("ab"), ParentHash: headHash("ab")}, nil
	}
	hints := make(chan rpc.HeadSignal, 1)
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()
	go func() {
		<-first
		height.Store(2)
		hints <- rpc.HeadSignal{Kind: rpc.WSHeadAvailable, Height: 999999, Hash: headHash("ff")}
	}()
	result, err := tr.Follow(ctx, Cursor{Height: 1, Hash: headHash("ab")}, hints, func(ctx context.Context, r HeadRecovery) error {
		if r.Cursor.Height == 2 {
			cancel()
		}
		return nil
	})
	if !errors.Is(err, context.Canceled) || result.Cursor.Height != 2 || commits.Load() != 1 || calls.Load() != 2 {
		t.Fatal(result, err, commits.Load(), calls.Load())
	}
}
