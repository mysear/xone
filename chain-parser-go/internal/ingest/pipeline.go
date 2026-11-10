package ingest

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"unicode/utf8"
)

type Cursor struct {
	Height uint64
	Hash   string
}
type Fetched struct {
	Height     uint64
	Hash       string
	ParentHash string
	ProviderID string
	Raw        []byte
}
type Block struct {
	Height     uint64
	Hash       string
	ParentHash string
	Raw        RawRef
	Decoded    []byte
}

// MaxBlockBytes covers retained Raw + Decoded bytes per task. Fetch must honor
// the supplied bound while reading (e.g. LimitReader); callbacks must honor ctx.
// MaxBytes reserves MaxBlockBytes per outstanding task, including tasks queued
// behind a slow earlier height. Workers never imply an unbounded reorder buffer.
type Limits struct {
	Workers       int
	MaxTasks      int
	MaxBytes      int64
	MaxBlockBytes int64
}

// Pipeline runs one commit actor per instance. All callbacks are required.
// Fetch succeeds only after ALL required shards have arrived. Validate owns
// chain-specific completeness, roots, canonical hashes and trusted-head checks.
// Commit must atomically write the WHOLE block and durable Cursor, conditional
// on expected Cursor (with fencing/idempotency in the existing adapter/journal).
// On ambiguous commit failure, reload the durable cursor before restarting Run;
// the returned cursor records only acknowledged commits, never an inferred tip.
// Success covers only the requested interval, not readiness/trust of the chain.
type Pipeline struct {
	Chain    string
	Limits   Limits
	Archive  *Archive
	Fetch    func(context.Context, uint64, int64) (Fetched, error)
	Decode   func(context.Context, Fetched) ([]byte, error)
	Validate func(context.Context, Fetched, []byte) error
	Commit   func(context.Context, Cursor, Block) error
	running  atomic.Bool
}

// DecodeError lets an adapter preserve protocol/pool identity in the durable
// DLQ. Error codes/scopes must fit DeadLetter bounds; an invalid entry stops Run.
type DecodeError struct {
	Code  string
	Scope string
	Err   error
}

func (e *DecodeError) Error() string {
	if e.Err == nil {
		return e.Code
	}
	return e.Err.Error()
}
func (e *DecodeError) Unwrap() error { return e.Err }

// StageError preserves the durable raw location even if the DLQ itself is full
// or its write fails. errors.Is still exposes the decode/validation/storage cause.
type StageError struct {
	Height uint64
	Raw    RawRef
	Stage  string
	Code   string
	Scope  string
	Err    error
}

func (e *StageError) Error() string {
	return fmt.Sprintf("INGEST_%s_HEIGHT_%d: %v", e.Stage, e.Height, e.Err)
}
func (e *StageError) Unwrap() error { return e.Err }

type staged struct {
	height uint64
	block  Block
	err    error
}

func (p *Pipeline) stage(ctx context.Context, height uint64) staged {
	result := staged{height: height}
	raw, err := p.Fetch(ctx, height, p.Limits.MaxBlockBytes)
	// Partial raw returned alongside a fetch error is archived for localization;
	// it can never be decoded or committed as a complete block.
	if ctx.Err() != nil {
		result.err = ctx.Err()
		return result
	}
	if raw.Height != height || raw.Hash == "" || raw.ParentHash == "" || raw.ProviderID == "" {
		if err != nil {
			result.err = err
		} else {
			result.err = errors.New("INGEST_FETCH_IDENTITY")
		}
		return result
	}
	if int64(len(raw.Raw)) > p.Limits.MaxBlockBytes {
		result.err = ErrBudget
		return result
	}
	ref, archiveErr := p.Archive.Put(Owner{Chain: p.Chain, Height: height, Hash: raw.Hash, ProviderID: raw.ProviderID}, raw.Raw)
	if archiveErr != nil {
		result.err = archiveErr
		return result
	}
	fail := func(stage, code string, cause error) staged {
		message := strings.ToValidUTF8(cause.Error(), "�")
		if len(message) > 2048 {
			message = message[:2048]
			for !utf8.ValidString(message) {
				message = message[:len(message)-1]
			}
		}
		scope := p.Chain
		var detail *DecodeError
		if errors.As(cause, &detail) {
			if detail.Scope != "" {
				scope = detail.Scope
			}
			if detail.Code != "" {
				code = detail.Code
			}
		}
		dlqErr := p.Archive.Quarantine(DeadLetter{Raw: ref, Stage: stage, Code: code, Scope: scope, Message: message})
		result.err = &StageError{Height: height, Raw: ref, Stage: stage, Code: code, Scope: scope, Err: errors.Join(cause, dlqErr)}
		return result
	}
	if err != nil {
		return fail("fetch", "FETCH_INCOMPLETE", err)
	}
	if ctx.Err() != nil {
		result.err = ctx.Err()
		return result
	}
	decoded, err := p.Decode(ctx, raw)
	if err != nil {
		return fail("decode", "DECODE_FAILED", err)
	}
	if int64(len(decoded)) > p.Limits.MaxBlockBytes-int64(len(raw.Raw)) {
		return fail("decode", "DECODE_BUDGET", ErrBudget)
	}
	if ctx.Err() != nil {
		result.err = ctx.Err()
		return result
	}
	if err = p.Validate(ctx, raw, decoded); err != nil {
		return fail("validate", "VALIDATION_FAILED", err)
	}
	result.block = Block{Height: height, Hash: raw.Hash, ParentHash: raw.ParentHash, Raw: ref, Decoded: decoded}
	return result
}

func (p *Pipeline) Run(ctx context.Context, cursor Cursor, target uint64) (Cursor, error) {
	return p.run(ctx, cursor, target, nil)
}

// RunChecked uses the SAME commit actor as Run and adds an HTTP-head/provenance
// barrier after staging, immediately before a whole-block commit. A failed
// barrier is quarantined; callers never mutate shared pipeline callbacks.
func (p *Pipeline) RunChecked(ctx context.Context, cursor Cursor, target uint64, check func(context.Context, Block) error) (Cursor, error) {
	if check == nil {
		return cursor, errors.New("INGEST_COMMIT_CHECK_REQUIRED")
	}
	return p.run(ctx, cursor, target, check)
}
func (p *Pipeline) run(ctx context.Context, cursor Cursor, target uint64, check func(context.Context, Block) error) (Cursor, error) {
	if !p.running.CompareAndSwap(false, true) {
		return cursor, errors.New("INGEST_ACTOR_RUNNING")
	}
	defer p.running.Store(false)
	if ctx.Err() != nil {
		return cursor, ctx.Err()
	}
	l := p.Limits
	if l.Workers <= 0 || l.MaxTasks <= 0 || l.MaxBlockBytes <= 0 || l.MaxBytes < l.MaxBlockBytes {
		return cursor, ErrBudget
	}
	if p.Chain == "" || cursor.Hash == "" || p.Archive == nil || p.Fetch == nil || p.Decode == nil || p.Validate == nil || p.Commit == nil {
		return cursor, errors.New("INGEST_PIPELINE_INVALID")
	}
	if target < cursor.Height {
		return cursor, errors.New("INGEST_TARGET_BEHIND_CURSOR")
	}
	if target == cursor.Height {
		return cursor, nil
	}
	window := int64(l.MaxTasks)
	if bytesWindow := l.MaxBytes / l.MaxBlockBytes; bytesWindow < window {
		window = bytesWindow
	}
	if remaining := target - cursor.Height; uint64(window) > remaining {
		window = int64(remaining)
	}
	workers := l.Workers
	if int64(workers) > window {
		workers = int(window)
	}
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	jobs := make(chan uint64, int(window))
	results := make(chan staged, int(window))
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				select {
				case <-ctx.Done():
					return
				case height, ok := <-jobs:
					if !ok {
						return
					}
					result := p.stage(ctx, height)
					select {
					case results <- result:
					case <-ctx.Done():
						return
					}
				}
			}
		}()
	}
	defer func() { cancel(); close(jobs); wg.Wait() }()
	// A slot is released only after a successful ordered commit. Therefore both
	// running work and completed out-of-order staging stay inside this window.
	scheduled := cursor.Height
	schedule := func() { scheduled++; jobs <- scheduled }
	for i := int64(0); i < window; i++ {
		schedule()
	}
	pending := make(map[uint64]staged, int(window))
	for cursor.Height < target {
		select {
		case <-ctx.Done():
			return cursor, ctx.Err()
		case result := <-results:
			if result.err != nil {
				return cursor, fmt.Errorf("INGEST_STAGE_HEIGHT_%d: %w", result.height, result.err)
			}
			pending[result.height] = result
			for {
				next, ok := pending[cursor.Height+1]
				if !ok {
					break
				}
				if ctx.Err() != nil {
					return cursor, ctx.Err()
				}
				if next.block.ParentHash != cursor.Hash {
					cause := errors.New("INGEST_PARENT_HASH_MISMATCH")
					dlqErr := p.Archive.Quarantine(DeadLetter{Raw: next.block.Raw, Stage: "commit", Code: "PARENT_HASH_MISMATCH", Scope: p.Chain})
					return cursor, errors.Join(cause, dlqErr)
				}
				if check != nil {
					if err := check(ctx, next.block); err != nil {
						dlqErr := p.Archive.Quarantine(DeadLetter{Raw: next.block.Raw, Stage: "commit", Code: "COMMIT_PRECONDITION", Scope: p.Chain})
						return cursor, errors.Join(err, dlqErr)
					}
				}
				if ctx.Err() != nil {
					return cursor, ctx.Err()
				}
				if err := p.Commit(ctx, cursor, next.block); err != nil {
					return cursor, fmt.Errorf("INGEST_COMMIT_HEIGHT_%d: %w", next.height, err)
				}
				cursor = Cursor{Height: next.block.Height, Hash: next.block.Hash}
				delete(pending, next.height)
				if scheduled < target {
					schedule()
				}
			}
		}
	}
	return cursor, nil
}
