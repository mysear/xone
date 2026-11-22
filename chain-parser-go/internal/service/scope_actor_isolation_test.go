package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

func scopeActorRequest(t *testing.T, s *Service, key, pool string, generation uint64) *httptest.ResponseRecorder {
	t.Helper()
	body, err := json.Marshal(map[string]any{"pool_ids": []string{pool}, "expected_online_generation": generation})
	if err != nil {
		t.Fatal(err)
	}
	req := httptest.NewRequest(http.MethodPost, "/internal/chains/"+key+"/ensure-pool-scope", bytes.NewReader(body))
	if s.cfg.Token != "" {
		req.Header.Set("Authorization", "Bearer "+s.cfg.Token)
	}
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, req)
	return response
}

func TestEnsurePoolScopeUsesOnlyRequestedChainActor(t *testing.T) {
	cfg := startupIsolationConfig(t)
	first, _ := startupRPCChain(t, cfg.Config.Chains[0], 31337, "")
	second, _ := startupRPCChain(t, cfg.Config.Chains[0], 31338, "")
	cfg.Config.Chains = []config.Chain{first, second}
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if err = s.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	var a, b domain.Snapshot
	readyDeadline := time.Now().Add(2 * time.Second)
	for {
		var firstReady, secondReady bool
		a, firstReady = s.stores[first.Key].Current()
		b, secondReady = s.stores[second.Key].Current()
		if firstReady && secondReady && len(a.Pools) != 0 && len(b.Pools) != 0 {
			// Current may become ready before the startup actor releases its lock.
			// Admit both scopes before beginning the deliberate contention checks.
			firstScope := scopeActorRequest(t, s, first.Key, a.Pools[0].PoolID, a.Reference.OnlineGeneration)
			secondScope := scopeActorRequest(t, s, second.Key, b.Pools[0].PoolID, b.Reference.OnlineGeneration)
			if firstScope.Code == http.StatusOK && secondScope.Code == http.StatusOK {
				break
			}
		}
		if time.Now().After(readyDeadline) {
			t.Fatal("both RPC chains did not admit their scopes", firstReady, secondReady, a.Reference, b.Reference)
		}
		time.Sleep(10 * time.Millisecond)
	}
	func() {
		actor := s.chainLock(first.Key)
		actor.Lock()
		defer actor.Unlock()
		if response := scopeActorRequest(t, s, second.Key, b.Pools[0].PoolID, b.Reference.OnlineGeneration); response.Code != http.StatusOK {
			t.Fatal("another chain's actor blocked a ready scope", response.Code, response.Body.String())
		}
		if response := scopeActorRequest(t, s, first.Key, a.Pools[0].PoolID, a.Reference.OnlineGeneration); response.Code != http.StatusTooManyRequests {
			t.Fatal("same-chain scope did not report a busy actor", response.Code, response.Body.String())
		}
		if response := scopeActorRequest(t, s, second.Key, b.Pools[0].PoolID, b.Reference.OnlineGeneration+1); response.Code != http.StatusConflict {
			t.Fatal("generation fence changed under another chain's lock", response.Code, response.Body.String())
		}
	}()
	for _, before := range []struct {
		key string
		ref domain.SnapshotRef
	}{{first.Key, a.Reference}, {second.Key, b.Reference}} {
		current, ready := s.stores[before.key].Current()
		if !ready || current.Reference != before.ref {
			t.Fatal("scope inspection changed an owner generation", before.key, current.Reference, before.ref)
		}
	}
	s.Close()
	if response := scopeActorRequest(t, s, second.Key, b.Pools[0].PoolID, b.Reference.OnlineGeneration); response.Code != http.StatusServiceUnavailable {
		t.Fatal("closed service admitted a scope operation", response.Code, response.Body.String())
	}
}

func TestRawReplayKeepsSameChainBusyAndGenerationFences(t *testing.T) {
	s, maintenance, key, _ := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, maintenance, key)
	before, ready := s.stores[key].Current()
	if !ready {
		t.Fatal("missing current scope")
	}
	func() {
		actor := s.chainLock(key)
		actor.Lock()
		defer actor.Unlock()
		if _, err := maintenance.Replay(context.Background(), key, request); !errors.Is(err, ingest.ErrReplayBusy) {
			t.Fatal("same-chain replay did not report a busy actor", err)
		}
	}()
	request.ExpectedGeneration++
	if _, err := maintenance.Replay(context.Background(), key, request); !errors.Is(err, ingest.ErrReplayFence) {
		t.Fatal("replay accepted a changed generation", err)
	}
	after, ready := s.stores[key].Current()
	if !ready || after.Reference != before.Reference {
		t.Fatal("rejected replay changed the owner scope", before.Reference, after.Reference)
	}
}

func TestRawReplayReacquiresChainActorAfterSourceReads(t *testing.T) {
	s, maintenance, key, _ := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, maintenance, key)
	before, ready := s.stores[key].Current()
	if !ready {
		t.Fatal("missing current scope")
	}
	client := s.adapters[key].Client
	upstream := client.HTTP.Transport
	if upstream == nil {
		upstream = http.DefaultTransport
	}
	entered, release := make(chan struct{}), make(chan struct{})
	var enteredOnce, releaseOnce sync.Once
	unblock := func() { releaseOnce.Do(func() { close(release) }) }
	defer unblock()
	client.HTTP.Transport = rawMaintenanceRoundTrip(func(r *http.Request) (*http.Response, error) {
		wait := false
		enteredOnce.Do(func() {
			wait = true
			close(entered)
		})
		if wait {
			select {
			case <-release:
			case <-r.Context().Done():
				return nil, r.Context().Err()
			}
		}
		return upstream.RoundTrip(r)
	})
	finished := make(chan error, 1)
	go func() {
		_, err := maintenance.Replay(context.Background(), key, request)
		finished <- err
	}()
	select {
	case <-entered:
	case err := <-finished:
		t.Fatal("replay failed before fresh source reads", err)
	case <-time.After(3 * time.Second):
		t.Fatal("replay did not start fresh source reads")
	}
	actor := s.chainLock(key)
	if !actor.TryLock() {
		t.Fatal("replay retained its actor during fresh source reads")
	}
	defer actor.Unlock()
	unblock()
	select {
	case err := <-finished:
		if !errors.Is(err, ingest.ErrReplayBusy) {
			t.Fatal("replay did not recheck the actor before owner commit", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("replay waited on a busy actor instead of rejecting its commit")
	}
	after, ready := s.stores[key].Current()
	if !ready || after.Reference != before.Reference {
		t.Fatal("busy commit published a replay scope", before.Reference, after.Reference)
	}
}
