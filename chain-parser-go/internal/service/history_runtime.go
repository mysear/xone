package service

import (
	"context"
	"errors"
	"math"
	"sync"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/reducer"
)

var ErrHistoryClosed = errors.New("HISTORY_CLOSED")
var ErrHistoryBusy = errors.New("HISTORY_BUSY")

// HistorySink is the worker's entire write capability. In particular it cannot
// install snapshots, commit online records, publish or change ready/generation.
type HistorySink interface {
	Claim(context.Context, time.Time) (reducer.HistoryClaim, error)
	Complete(context.Context, reducer.HistoryClaim, domain.FactBatch, []reducer.HistoryCandidate, ...reducer.HistoryRawGroup) error
	Retry(context.Context, reducer.HistoryClaim, time.Time, string) error
	Status(context.Context) (reducer.HistoryQueueStatus, error)
}
type HistoryReadFacts func(context.Context, uint64) (domain.FactBatch, []reducer.HistoryCandidate, error)
type HistoryRuntimeOptions struct {
	RawArchive *ingest.Archive
	MaxRawPins int
	Queue      *reducer.HistoryQueue
	Config     config.History
	ReadFacts  HistoryReadFacts
	// Share one Scheduler across every chain in a service. It serializes all
	// history sources and imposes a single aggregate duty-cycle budget.
	Scheduler                                 *HistoryScheduler
	Timeout, RetryMin, RetryMax, PollInterval time.Duration
}

// HistoryScheduler bounds duty cycle, not OS CPU, network bytes/sec or Pebble
// I/O latency. Each nonpreemptible step is <=one block with a source deadline.
// After work w, idle >=w*(1/share-1); a window can overshoot by at most one step
// plus timer/OS/fsync jitter. No hard realtime or physical I/O isolation claim.
type HistoryScheduler struct {
	mu   sync.Mutex
	busy bool
	next time.Time
}

func (s *HistoryScheduler) enter(now time.Time) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.busy || now.Before(s.next) {
		return false
	}
	s.busy = true
	return true
}
func (s *HistoryScheduler) leave(work time.Duration, share float64) time.Duration {
	idle := float64(work) * (1/share - 1)
	if idle > float64(math.MaxInt64) {
		idle = float64(math.MaxInt64)
	}
	cooldown := time.Duration(idle)
	if cooldown < 0 {
		cooldown = time.Duration(math.MaxInt64)
	}
	s.mu.Lock()
	s.busy = false
	s.next = time.Now().Add(cooldown)
	s.mu.Unlock()
	return cooldown
}

type HistoryRuntimeStatus struct {
	Enabled, Paused, Running, Closed bool
	MaxShare                         float64
	LastWork, Cooldown               time.Duration
	LastError                        string
	Queue                            reducer.HistoryQueueStatus
}
type HistoryRuntime struct {
	afterCommit             func(string)
	scopePending            bool
	updateScope             func(context.Context, config.Chain, domain.SnapshotRef) error
	recoverMu               sync.Mutex
	recoveryCursor          string
	closeSource             func()
	closeOnce               sync.Once
	mu                      sync.Mutex
	wg                      sync.WaitGroup
	options                 HistoryRuntimeOptions
	sink                    HistorySink
	scheduler               *HistoryScheduler
	cancel                  context.CancelFunc
	stepCancel              context.CancelFunc
	closed, paused, started bool
	lastWork, cooldown      time.Duration
	lastError               string
}

func NewHistoryRuntime(o HistoryRuntimeOptions) (*HistoryRuntime, error) {
	if o.Queue == nil || o.Config.Namespace != "history" || o.Config.OnlineWrite || math.IsNaN(o.Config.MaxShare) || math.IsInf(o.Config.MaxShare, 0) || o.Config.MaxShare < 0 {
		return nil, errors.New("HISTORY_RUNTIME_CONFIG")
	}
	if o.Config.MaxShare > .15 {
		o.Config.MaxShare = .15
	}
	if o.Config.Enabled && o.Config.MaxShare > 0 && o.ReadFacts == nil {
		return nil, errors.New("HISTORY_SOURCE_REQUIRED")
	}
	if o.Timeout == 0 {
		o.Timeout = 10 * time.Second
	}
	if o.RetryMin == 0 {
		o.RetryMin = time.Second
	}
	if o.RetryMax == 0 {
		o.RetryMax = time.Minute
	}
	if o.PollInterval == 0 {
		o.PollInterval = 250 * time.Millisecond
	}
	if o.Timeout <= 0 || o.Timeout > time.Hour || o.RetryMin <= 0 || o.RetryMax < o.RetryMin || o.PollInterval < time.Millisecond {
		return nil, errors.New("HISTORY_RUNTIME_LIMITS")
	}
	if o.MaxRawPins == 0 {
		o.MaxRawPins = 128
	}
	if o.MaxRawPins < 1 || o.MaxRawPins > 512 {
		return nil, errors.New("HISTORY_RAW_PINS_LIMIT")
	}
	if o.Scheduler == nil {
		o.Scheduler = &HistoryScheduler{}
	}
	return &HistoryRuntime{options: o, sink: o.Queue, scheduler: o.Scheduler}, nil
}

// Start returns without RPC work or historical replay. Start after first scope
// publication; history startup errors must be reported on history status only.
func (r *HistoryRuntime) Start(ctx context.Context) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed {
		return ErrHistoryClosed
	}
	if r.started {
		return nil
	}
	if e := ctx.Err(); e != nil {
		return e
	}
	ctx, r.cancel = context.WithCancel(ctx)
	r.started = true
	r.wg.Add(1)
	go func() {
		defer r.wg.Done()
		timer := time.NewTicker(r.options.PollInterval)
		defer timer.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-timer.C:
				_, _ = r.Tick(ctx)
			}
		}
	}()
	return nil
}

// Tick is a nonqueued, bounded step. It may also be used without Start from a
// separate history loop, never synchronously on the online scope/commit path.
func (r *HistoryRuntime) Tick(ctx context.Context) (bool, error) {
	if e := ctx.Err(); e != nil {
		return false, e
	}
	r.mu.Lock()
	if r.closed {
		r.mu.Unlock()
		return false, ErrHistoryClosed
	}
	if r.paused || r.scopePending || !r.options.Config.Enabled || r.options.Config.MaxShare == 0 {
		r.mu.Unlock()
		return false, nil
	}
	if r.stepCancel != nil {
		r.mu.Unlock()
		return false, ErrHistoryBusy
	}
	if !r.scheduler.enter(time.Now()) {
		r.mu.Unlock()
		return false, nil
	}
	started := time.Now()
	step, cancel := context.WithTimeout(ctx, r.options.Timeout)
	r.stepCancel = cancel
	r.wg.Add(1)
	r.mu.Unlock()
	defer func() {
		cancel()
		work := time.Since(started)
		cooldown := r.scheduler.leave(work, r.options.Config.MaxShare)
		r.mu.Lock()
		r.stepCancel = nil
		r.lastWork = work
		r.cooldown = cooldown
		r.mu.Unlock()
		r.wg.Done()
	}()
	return r.work(step)
}
func (r *HistoryRuntime) work(ctx context.Context) (bool, error) {
	if more, e := r.RecoverRawPins(ctx, 16); e != nil {
		r.setError("HISTORY_RAW_RECOVERY_RETRY")
		return false, e
	} else if more {
		return false, nil
	}
	claim, e := r.sink.Claim(ctx, time.Now())
	if errors.Is(e, reducer.ErrHistoryIdle) {
		return false, nil
	}
	if e != nil {
		r.setError("HISTORY_QUEUE_RETRY")
		return false, e
	}
	group := &rawCapture{ID: rawGroup(), Chain: r.options.Queue.Chain(), Max: r.options.MaxRawPins}
	ctx = context.WithValue(ctx, rawCaptureKey{}, group)
	ctx = context.WithValue(ctx, historyClaimContextKey{}, claim)
	evidence := &historyStepEvidence{}
	ctx = context.WithValue(ctx, historyEvidenceContextKey{}, evidence)
	facts, candidates, e := r.options.ReadFacts(ctx, claim.Height)
	group.mu.Lock()
	pins := append([]RawPin(nil), group.Pins...)
	group.mu.Unlock()
	var rawGroups []reducer.HistoryRawGroup
	if len(pins) > 0 {
		if r.options.RawArchive == nil {
			e = errors.Join(e, errors.New("HISTORY_RAW_ARCHIVE_REQUIRED"))
		} else {
			g := reducer.HistoryRawGroup{ID: group.ID, Parents: evidence.Parents}
			for _, pin := range pins {
				g.Pins = append(g.Pins, reducer.HistoryRawPin{Raw: pin.Raw, Reference: pin.Reference})
			}
			rawGroups = append(rawGroups, g)
		}
	}
	// Pause/Close revoke the in-flight context. Hold this small mutex through the
	// final sink call so a completed Pause can never be followed by an old commit.
	if e == nil {
		r.mu.Lock()
		e = ctx.Err()
		if e == nil && !r.closed && !r.paused {
			e = r.sink.Complete(ctx, claim, facts, candidates, rawGroups...)
		} else if e == nil {
			e = context.Canceled
		}
		r.mu.Unlock()
	}
	if e == nil && len(rawGroups) > 0 {
		e = r.releaseRawGroup(ctx, rawGroups[0].ID)
		if e != nil {
			r.setError("HISTORY_RAW_RELEASE_RETRY")
			return true, e
		}
	}
	if e == nil && len(rawGroups) > 0 && r.afterCommit != nil {
		r.afterCommit(rawGroups[0].ID)
	}
	if e == nil {
		r.setError("")
		return true, nil
	}
	code := "HISTORY_SOURCE_RETRY"
	if errors.Is(e, reducer.ErrHistoryCapacity) {
		code = "HISTORY_CAPACITY_RETRY"
	}
	if ctx.Err() != nil {
		code = "HISTORY_CANCELED_RETRY"
	}
	// Releasing a claim is local durable cleanup, independent of source cancel.
	// No source request, private-data escalation or online callback occurs here.
	cleanup, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	retry := r.options.RetryMin
	status, statusErr := r.sink.Status(cleanup)
	if statusErr == nil {
		for _, j := range status.Jobs {
			if j.From == claim.From {
				for n := uint64(0); n < j.Attempts && retry < r.options.RetryMax; n++ {
					if retry > r.options.RetryMax/2 {
						retry = r.options.RetryMax
						break
					}
					retry *= 2
				}
				break
			}
		}
	}
	if retry > r.options.RetryMax {
		retry = r.options.RetryMax
	}
	releaseErr := r.sink.Retry(cleanup, claim, time.Now().Add(retry), code)
	r.setError(code)
	return true, errors.Join(e, releaseErr)
}
func (r *HistoryRuntime) setError(code string) { r.mu.Lock(); r.lastError = code; r.mu.Unlock() }
func (r *HistoryRuntime) Pause(paused bool) {
	r.mu.Lock()
	r.paused = paused
	if paused && r.stepCancel != nil {
		r.stepCancel()
	}
	r.mu.Unlock()
}
func (r *HistoryRuntime) EnqueueBaseline(ctx context.Context, s domain.Snapshot) error {
	return r.options.Queue.EnqueueBaseline(ctx, s)
}
func (r *HistoryRuntime) Enqueue(ctx context.Context, gaps []domain.Gap) error {
	return r.options.Queue.Enqueue(ctx, gaps)
}

// SourceStatus is suitable for the authenticated normal source status handler.
// It reads bounded local metadata and makes no RPC calls; never use it for ready.
func (r *HistoryRuntime) SourceStatus(ctx context.Context) (HistoryRuntimeStatus, error) {
	r.mu.Lock()
	pending := r.scopePending
	st := HistoryRuntimeStatus{Enabled: r.options.Config.Enabled, Paused: r.paused || r.options.Config.MaxShare == 0, Running: r.stepCancel != nil, Closed: r.closed, MaxShare: r.options.Config.MaxShare, LastWork: r.lastWork, Cooldown: r.cooldown, LastError: r.lastError}
	r.mu.Unlock()
	var e error
	st.Queue, e = r.sink.Status(ctx)
	if pending {
		st.LastError = "HISTORY_SCOPE_UPDATE_RETRY"
		st.Queue.ScopeStale = true
		st.Queue.Complete = false
		st.Queue.Covered = nil
	}
	return st, e
}
func (r *HistoryRuntime) Close() {
	r.mu.Lock()
	r.closed = true
	if r.cancel != nil {
		r.cancel()
	}
	if r.stepCancel != nil {
		r.stepCancel()
	}
	r.mu.Unlock()
	r.wg.Wait()
	r.closeOnce.Do(func() {
		if r.options.RawArchive != nil {
			cleanup, cancel := context.WithTimeout(context.Background(), time.Second)
			_, e := r.RecoverRawPins(cleanup, 16)
			cancel()
			if e != nil {
				r.setError("HISTORY_RAW_CLOSE_RECOVERY_RETRY")
			}
		}
		if r.closeSource != nil {
			r.closeSource()
		}
	})
}

// UpdateScope accepts a frozen, admitted scope from the chain actor (for example
// runtimeCatalogChain after reanchor/Expand). It never edits root configuration.
// Canonical/scope CAS and dataset reset are one owner transaction. Old network
// work is canceled and cannot commit after this method's successful return.
func (r *HistoryRuntime) UpdateScope(ctx context.Context, scope config.Chain, expected domain.SnapshotRef) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed {
		return ErrHistoryClosed
	}
	if r.updateScope == nil {
		return errors.New("HISTORY_NORMAL_SCOPE_REQUIRED")
	}
	if r.stepCancel != nil {
		r.stepCancel()
	}
	r.scopePending = true
	e := r.updateScope(ctx, scope, expected)
	if e == nil {
		r.scopePending = false
	}
	return e
}
func (r *HistoryRuntime) HistoryArchive() *ingest.Archive { return r.options.RawArchive }
func (r *HistoryRuntime) releaseRawGroup(ctx context.Context, group string) error {
	proof, e := r.options.Queue.RawReceipt(ctx, group)
	if e != nil {
		return e
	}
	if r.options.RawArchive == nil {
		return errors.New("HISTORY_RAW_ARCHIVE_REQUIRED")
	}
	for _, pin := range proof.Pins {
		if e := ctx.Err(); e != nil {
			return e
		}
		if e = r.options.RawArchive.ReleaseRaw(pin.Raw, pin.Reference); e != nil {
			return e
		}
	}
	return r.options.Queue.AcknowledgeRaw(ctx, proof)
}

// RecoverRawPins releases only groups proven by the same owner DB's immutable
// facts+receipt transaction. Failed/uncommitted/orphan pins remain held. It also
// reconciles unknown commit results after the owner Store has been reopened.
// Each call scans at most limit receipts, and performs no source RPC calls.
func (r *HistoryRuntime) RecoverRawPins(ctx context.Context, limit int) (bool, error) {
	if r.options.RawArchive == nil {
		return false, nil
	}
	r.recoverMu.Lock()
	defer r.recoverMu.Unlock()
	pending, next, e := r.options.Queue.RawReceiptPage(ctx, r.recoveryCursor, limit)
	if e != nil {
		return false, e
	}
	for _, p := range pending {
		if e = r.releaseRawGroup(ctx, p.Group); e != nil {
			return false, e
		}
	}
	r.recoveryCursor = next
	return next != "", nil
}
