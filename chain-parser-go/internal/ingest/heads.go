package ingest

import (
	"context"
	"encoding/hex"
	"errors"
	"strings"
	"sync/atomic"
	"time"

	"xone/chain-parser-go/internal/rpc"
)

var ErrHeadCheckBudget = errors.New("INGEST_HEAD_CHECK_BUDGET")

type HTTPHead struct {
	Chain      string
	Height     uint64
	Hash       string
	ParentHash string
}
type HeadLimits struct {
	// Zero uses PollInterval. Positive values permit lower-latency WS wakeups
	// without allowing notification floods to exceed this HTTP check rate.
	MinCheckInterval time.Duration
	TailWindow       uint64
	Timeout          time.Duration
	PollInterval     time.Duration
	MaxChecks        int
}
type HeadAction string

const (
	HeadCaughtUp      HeadAction = "caught_up"
	HeadTailCommitted HeadAction = "tail_committed"
	HeadReanchor      HeadAction = "reanchor"
)

type HistoryGap struct {
	Chain  string
	From   uint64
	To     uint64
	Reason string
}
type HeadRecovery struct {
	Action     HeadAction
	Head       HTTPHead
	Cursor     Cursor
	HistoryGap *HistoryGap
	Reason     string
}

// HeadTracker always starts with the CURRENT HTTP head. WS heights/hashes are
// never passed to ReadHead, ReadHeader, Fetch or the commit cursor. VerifyHead
// supplies chain-specific provenance/trust checks; Pipeline.Validate must check
// the actual HTTP block/receipts. HEAD_FIRST reanchor never publishes a cursor.
// A returned gap describes block history still absent even if state is later
// bootstrapped at the new head. The service must persist it in history scope.
type HeadTracker struct {
	Chain      string
	Limits     HeadLimits
	Pipeline   *Pipeline
	ReadHead   func(context.Context) (HTTPHead, error)
	ReadHeader func(context.Context, uint64) (HTTPHead, error)
	VerifyHead func(context.Context, HTTPHead) error
	running    atomic.Bool
}

func validHeadHash(h string) bool {
	if len(h) != 66 || !strings.HasPrefix(h, "0x") {
		return false
	}
	_, err := hex.DecodeString(h[2:])
	return err == nil
}
func (t *HeadTracker) valid() bool {
	return t.Chain != "" && t.Limits.TailWindow > 0 && t.Limits.Timeout > 0 && t.Limits.PollInterval > 0 && t.Limits.MinCheckInterval >= 0 && t.Limits.MinCheckInterval <= t.Limits.PollInterval && t.Limits.MaxChecks > 0 && t.Pipeline != nil && t.Pipeline.Chain == t.Chain && t.ReadHead != nil && t.ReadHeader != nil && t.VerifyHead != nil
}
func (t *HeadTracker) CatchUp(ctx context.Context, cursor Cursor) (HeadRecovery, error) {
	if !t.running.CompareAndSwap(false, true) {
		return HeadRecovery{Cursor: cursor}, errors.New("INGEST_HEAD_ACTOR_RUNNING")
	}
	defer t.running.Store(false)
	return t.catchUp(ctx, cursor)
}
func (t *HeadTracker) catchUp(ctx context.Context, cursor Cursor) (HeadRecovery, error) {
	result := HeadRecovery{Cursor: cursor}
	if !t.valid() {
		return result, errors.New("INGEST_HEAD_LIMITS_OR_CALLBACKS_INVALID")
	}
	ctx, cancel := context.WithTimeout(ctx, t.Limits.Timeout)
	defer cancel()
	if ctx.Err() != nil {
		return result, ctx.Err()
	}
	head, err := t.ReadHead(ctx)
	if err != nil {
		return result, err
	}
	if head.Chain != t.Chain || !validHeadHash(head.Hash) || !validHeadHash(head.ParentHash) {
		return result, errors.New("INGEST_HTTP_HEAD_IDENTITY")
	}
	if err = t.VerifyHead(ctx, head); err != nil {
		return result, err
	}
	if ctx.Err() != nil {
		return result, ctx.Err()
	}
	result.Head = head
	reanchor := func(reason string) HeadRecovery {
		result.Action = HeadReanchor
		result.Reason = reason
		if cursor.Height < head.Height {
			result.HistoryGap = &HistoryGap{Chain: t.Chain, From: cursor.Height + 1, To: head.Height, Reason: reason}
		}
		return result
	}
	if cursor.Hash == "" {
		return reanchor("missing_baseline"), nil
	}
	if cursor.Height > head.Height {
		return reanchor("head_regressed"), nil
	}
	if head.Height-cursor.Height > t.Limits.TailWindow {
		return reanchor("tail_window_exceeded"), nil
	}
	var atCursor HTTPHead
	if cursor.Height == head.Height {
		atCursor = head
	} else {
		atCursor, err = t.ReadHeader(ctx, cursor.Height)
		if err != nil {
			return result, err
		}
		if atCursor.Chain != t.Chain || atCursor.Height != cursor.Height || !validHeadHash(atCursor.Hash) || !validHeadHash(atCursor.ParentHash) {
			return result, errors.New("INGEST_HTTP_CURSOR_HEADER")
		}
		if err = t.VerifyHead(ctx, atCursor); err != nil {
			return result, err
		}
	}
	if !strings.EqualFold(atCursor.Hash, cursor.Hash) {
		return reanchor("cursor_fork"), nil
	}
	if head.Height == cursor.Height {
		result.Action = HeadCaughtUp
		return result, nil
	}
	// Keep the existing pipeline's actor/fencing; pin its final staged block
	// to the independently verified current HTTP header.
	next, err := t.Pipeline.RunChecked(ctx, cursor, head.Height, func(ctx context.Context, b Block) error {
		if b.Height == head.Height && (!strings.EqualFold(b.Hash, head.Hash) || !strings.EqualFold(b.ParentHash, head.ParentHash)) {
			return errors.New("INGEST_HTTP_HEAD_BLOCK_MISMATCH")
		}
		return nil
	})
	result.Cursor = next
	if err != nil {
		return result, err
	}
	result.Action = HeadTailCommitted
	return result, nil
}

// Follow coalesces wakeups and polls HTTP even after Hints closes. A session has
// both a total timeout and MaxChecks; hint floods cannot increase HTTP call rate
// beyond MinCheckInterval (or PollInterval when the minimum is zero). Reconnect signals trigger the same HEAD_FIRST check.
func (t *HeadTracker) Follow(ctx context.Context, cursor Cursor, hints <-chan rpc.HeadSignal, onProgress func(context.Context, HeadRecovery) error) (HeadRecovery, error) {
	result := HeadRecovery{Cursor: cursor}
	if !t.running.CompareAndSwap(false, true) {
		return result, errors.New("INGEST_HEAD_ACTOR_RUNNING")
	}
	defer t.running.Store(false)
	if !t.valid() {
		return result, errors.New("INGEST_HEAD_LIMITS_OR_CALLBACKS_INVALID")
	}
	ctx, cancel := context.WithTimeout(ctx, t.Limits.Timeout)
	defer cancel()
	timer := time.NewTimer(0)
	defer timer.Stop()
	checks := 0
	minCheck := t.Limits.MinCheckInterval
	if minCheck == 0 {
		minCheck = t.Limits.PollInterval
	}
	nextCheck := time.Now()
	var lastCheck time.Time
	for {
		if ctx.Err() != nil {
			return result, ctx.Err()
		}
		select {
		case <-ctx.Done():
			return result, ctx.Err()
		case hint, ok := <-hints:
			if !ok {
				hints = nil
				continue
			}
			if hint.Kind != rpc.WSHeadAvailable && hint.Kind != rpc.WSConnected {
				continue
			}
			// Only the wakeup kind matters. Height/hash never enter the HTTP path.
			desired := lastCheck.Add(minCheck)
			if desired.Before(nextCheck) {
				if !timer.Stop() {
					select {
					case <-timer.C:
					default:
					}
				}
				timer.Reset(max(time.Until(desired), 0))
				nextCheck = desired
			}
		case <-timer.C:
			if checks >= t.Limits.MaxChecks {
				return result, ErrHeadCheckBudget
			}
			checks++
			next, err := t.catchUp(ctx, cursor)
			result = next
			cursor = next.Cursor
			if err != nil {
				return result, err
			}
			if onProgress != nil {
				if err = onProgress(ctx, next); err != nil {
					return result, err
				}
			}
			if next.Action == HeadReanchor {
				return result, nil
			}
			lastCheck = time.Now()
			nextCheck = lastCheck.Add(t.Limits.PollInterval)
			timer.Reset(t.Limits.PollInterval)
		}
	}
}
