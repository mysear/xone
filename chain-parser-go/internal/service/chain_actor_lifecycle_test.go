package service

import (
	"context"
	"errors"
	"net/http"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
)

func TestStartupCanceledSuccessfulRunCannotReportSuccessAgain(t *testing.T) {
	cfg := startupIsolationConfig(t)
	chain, _ := startupRPCChain(t, cfg.Config.Chains[0], 31502, "")
	cfg.Config.Chains = []config.Chain{chain}
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	if err := s.Start(ctx); err != nil {
		t.Fatal(err)
	}
	assertStartupBaselineHTTP(t, s, chain.Key, http.StatusOK)
	cancel()
	drained := make(chan struct{})
	go func() { s.actorWG.Wait(); close(drained) }()
	select {
	case <-drained:
	case <-time.After(5 * time.Second):
		t.Fatal("canceled source workers did not stop")
	}
	if s.started.Load() {
		t.Fatal("canceled source lifetime remains started")
	}
	if err := s.Start(context.Background()); !errors.Is(err, context.Canceled) {
		t.Fatal("completed startup hid the canceled source lifetime", err)
	}
	assertStartupBaselineHTTP(t, s, chain.Key, http.StatusServiceUnavailable)
}

func TestStartupFailedAttemptCanRetryWithNewSourceLifetime(t *testing.T) {
	cfg := startupIsolationConfig(t)
	chain, node := startupRPCChain(t, cfg.Config.Chains[0], 31501, "wrong-chain")
	cfg.Config.Chains = []config.Chain{chain}
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	first, cancelFirst := context.WithCancel(context.Background())
	defer cancelFirst()
	if err = s.Start(first); err == nil {
		t.Fatal("wrong identity was admitted")
	}
	assertStartupBaselineHTTP(t, s, chain.Key, http.StatusServiceUnavailable)
	node.broken.Store(false)
	if err = s.Start(context.Background()); err != nil {
		t.Fatal("failed startup permanently closed a reusable service", err)
	}
	before, ready := s.stores[chain.Key].Current()
	if !ready || before.Reference.OnlineGeneration != 1 {
		t.Fatal("retry failed to publish its first valid generation", before.Reference)
	}
	// Canceling the obsolete startup context must not cancel the repaired run.
	cancelFirst()
	s.Tick(context.Background())
	assertStartupBaselineHTTP(t, s, chain.Key, http.StatusOK)
	after, ready := s.stores[chain.Key].Current()
	if !ready || after.Reference != before.Reference {
		t.Fatal("obsolete startup lifetime changed the repaired bank", before.Reference, after.Reference)
	}
}
