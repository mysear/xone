package service

import (
	"context"
	"errors"
	"sync"
	"time"

	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/config"
)

// One owner operation at a time may mutate a chain's writer, recovery or scope.
// Its worker and one buffered wake bound scheduling independently of other chains.
type chainActor struct {
	mu        sync.Mutex
	wake      chan context.Context
	nextHTTP  time.Time
	probeDone chan struct{}
}

type startupAttempt struct {
	done chan struct{}
	err  error
}

type chainStartResult struct {
	index  int
	active bool
	err    error
}

func (s *Service) actorLocked(key string) *chainActor {
	if s.actors == nil {
		s.actors = map[string]*chainActor{}
	}
	a := s.actors[key]
	if a == nil {
		a = &chainActor{wake: make(chan context.Context, 1)}
		s.actors[key] = a
	}
	return a
}

func (s *Service) actor(key string) *chainActor {
	s.lifecycleMu.Lock()
	defer s.lifecycleMu.Unlock()
	return s.actorLocked(key)
}

func (s *Service) chainLock(key string) *sync.Mutex { return &s.actor(key).mu }

// A session pointer may be looked up under a reducer gate. This helper never
// takes the chain actor or performs I/O; the caller already owns that actor.
func (s *Service) session(key string) *writerSession {
	s.sessionsMu.RLock()
	defer s.sessionsMu.RUnlock()
	return s.sessions[key]
}

func (s *Service) setSession(key string, writer *writerSession) {
	s.sessionsMu.Lock()
	defer s.sessionsMu.Unlock()
	if s.sessions == nil {
		s.sessions = map[string]*writerSession{}
	}
	s.sessions[key] = writer
}

func (s *Service) takeSession(key string) *writerSession {
	s.sessionsMu.Lock()
	defer s.sessionsMu.Unlock()
	writer := s.sessions[key]
	delete(s.sessions, key)
	return writer
}

func (s *Service) recovery(key string) *bootstrap.EmbeddedScopeStore {
	s.recoveryMu.Lock()
	defer s.recoveryMu.Unlock()
	return s.recoveries[key]
}

func (s *Service) setRecovery(key string, recovery *bootstrap.EmbeddedScopeStore) {
	s.recoveryMu.Lock()
	defer s.recoveryMu.Unlock()
	if s.recoveries == nil {
		s.recoveries = map[string]*bootstrap.EmbeddedScopeStore{}
	}
	s.recoveries[key] = recovery
}

func (s *Service) takeRecovery(key string) *bootstrap.EmbeddedScopeStore {
	s.recoveryMu.Lock()
	defer s.recoveryMu.Unlock()
	recovery := s.recoveries[key]
	delete(s.recoveries, key)
	return recovery
}

// Registration and the closed check share one short lock. Close cancels this
// context before waiting; no mutex is held while operations or RPCs unwind.
func (s *Service) beginOperation(parent context.Context) (context.Context, func(), error) {
	s.lifecycleMu.Lock()
	if s.closed.Load() {
		s.lifecycleMu.Unlock()
		return nil, nil, errors.New("SERVICE_CLOSED")
	}
	s.operationWG.Add(1)
	run := s.activeContext
	if run == nil {
		run = s.runContext
	}
	s.lifecycleMu.Unlock()
	ctx, cancel := context.WithCancel(parent)
	stop := func() bool { return false }
	if run != nil {
		stop = context.AfterFunc(run, cancel)
		if run.Err() != nil {
			cancel()
		}
	}
	var once sync.Once
	finish := func() {
		once.Do(func() {
			stop()
			cancel()
			s.operationWG.Done()
		})
	}
	return ctx, finish, nil
}

// ScheduleTick is the production polling/WS entry. It never waits for RPC or
// another chain, and repeated hints retain at most one pending wake per chain.
func (s *Service) ScheduleTick(ctx context.Context) {
	s.lifecycleMu.Lock()
	defer s.lifecycleMu.Unlock()
	if s.closed.Load() || !s.started.Load() {
		return
	}
	for _, chain := range s.cfg.Config.Chains {
		a := s.actorLocked(chain.Key)
		select {
		case a.wake <- ctx:
		default:
		}
	}
}

func (s *Service) runChainActor(ctx context.Context, chain config.Chain, index int, results chan<- chainStartResult) {
	defer s.actorWG.Done()
	a := s.actor(chain.Key)
	defer func() {
		s.started.Store(false)
		a.mu.Lock()
		defer a.mu.Unlock()
		// A lost source lifetime also withdraws the published owner. A fresh,
		// bounded cleanup window may publish its barrier while authority lasts;
		// local readiness and the runtime owner view fail closed regardless.
		cleanup, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		if p := s.providerRuntime[chain.Key]; p != nil && p.normal != nil {
			p.normal.owner.Store(&runtimeOwnerView{})
		}
		if current, ready := s.stores[chain.Key].Current(); ready {
			if err := s.revokeScope(cleanup, chain, current); err != nil {
				s.log.Event("warn", "scope_stop_revocation_failed", map[string]any{"chain_key": chain.Key, "error_code": err.Error()})
			}
		}
		s.stores[chain.Key].Suspend()
		_ = s.revokeRuntimeSources(cleanup, chain, nil)
		if w := s.takeSession(chain.Key); w != nil {
			w.close()
		}
	}()
	a.mu.Lock()
	active, err := s.initializeChain(ctx, chain)
	if err != nil {
		s.failChain(ctx, chain, err, "chain_start_failed")
	}
	a.mu.Unlock()
	results <- chainStartResult{index: index, active: active, err: err}
	var renewal <-chan time.Time
	if s.control != nil {
		ticker := time.NewTicker(time.Duration(s.cfg.Config.Control.RenewalMS) * time.Millisecond)
		defer ticker.Stop()
		renewal = ticker.C
	}
	for {
		select {
		case <-ctx.Done():
			return
		case parent := <-a.wake:
			operation, finish, err := s.beginOperation(parent)
			if err != nil {
				return
			}
			a.mu.Lock()
			s.tickChain(operation, chain, a)
			a.mu.Unlock()
			finish()
		case <-renewal:
			a.mu.Lock()
			if writer := s.session(chain.Key); writer != nil {
				if err := writer.heartbeat(ctx); err != nil {
					s.failChain(ctx, chain, err, "writer_lease_lost")
				}
			}
			a.mu.Unlock()
		}
	}
}

// Failed startup attempts cancel only their registered run. Persistent stores,
// catalogues and the shared PG connection remain available for a later Start.
func (s *Service) resetStartupRun() {
	s.actorWG.Wait()
	if s.providerCancel != nil {
		s.providerCancel()
		s.providerWG.Wait()
	}
	if s.wsCancel != nil {
		s.wsCancel()
		s.wsWG.Wait()
	}
	s.closeHistoryLifecycle()
	s.historyMu.Lock()
	s.historyCancel = nil
	s.historyContext = nil
	s.historyRuntime = nil
	s.historyScheduler = nil
	s.historyInitErrors = nil
	s.historyMu.Unlock()
	if m := s.rawMaintenance; m != nil {
		m.mu.Lock()
		cancel := m.cancel
		m.mu.Unlock()
		if cancel != nil {
			cancel()
			m.wg.Wait()
			m.mu.Lock()
			m.started = false
			m.cancel = nil
			m.mu.Unlock()
		}
	}
	s.lifecycleMu.Lock()
	s.providerCancel = nil
	s.wsCancel = nil
	for _, actor := range s.actors {
		actor.probeDone = nil
	}
	s.lifecycleMu.Unlock()
}
