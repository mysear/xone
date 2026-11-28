package service

import (
	"context"
	"encoding/json"
	"net/http"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

func TestRuntimeSourcesObserverOwnsStandaloneReadsAndInheritsBaselineCapture(t *testing.T) {
	s, m, key, state := maintenanceFixtureWithChain(t, func(chain *config.Chain, state *rawMaintenanceRPCState) {
		probe := probeConfig()
		probe.HistoricalHeight, probe.HistoricalHash = 100, state.BaseHash
		probe.TimeoutMS, probe.IntervalMS, probe.MaxBackoffMS, probe.MaxAgeMS = 8000, 60000, 60000, 120000
		chain.ProviderProbe = &probe
	}, func(c *config.RawMaintenanceConfig) {
		c.TimeoutMS = 10000
	})
	s.providerCancel()
	s.providerWG.Wait()
	p := s.providerRuntime[key]
	if p == nil {
		t.Fatal("normal New did not construct provider runtime")
	}
	upstream := p.client.HTTP.Transport
	var reads, identities atomic.Int64
	var inherited atomic.Pointer[rawCapture]
	p.client.HTTP.Transport = runtimeTestTransport(func(r *http.Request) (*http.Response, error) {
		group, _ := r.Context().Value(rawCaptureKey{}).(*rawCapture)
		if group == nil || group.Chain != key {
			t.Error("provider read escaped raw responsibility")
		}
		if expected := inherited.Load(); expected != nil && group != expected {
			t.Error("nested provider observation replaced baseline capture")
		}
		reads.Add(1)
		return upstream.RoundTrip(r)
	})
	p.client.SetRecorder(func(ctx context.Context, provider, method string, params json.RawMessage, response []byte) error {
		if method == "eth_chainId" {
			identities.Add(1)
		}
		return s.rawSources[key].capture(ctx, provider, method, params, response)
	})
	p.mu.Lock()
	p.next = time.Time{}
	p.mu.Unlock()
	if err := p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if reads.Load() == 0 {
		t.Fatal("probe test made no observed RPC reads")
	}
	providers := p.Snapshot().Providers
	if len(providers) != 2 || identities.Load() != 2 {
		t.Fatalf("expected one identity handshake per endpoint, got %d for %d endpoints", identities.Load(), len(providers))
	}
	for _, provider := range providers {
		if !providerQualified(provider) {
			t.Fatalf("missing current capability evidence: %+v", provider)
		}
	}
	before := len(m.observationEntries(key))
	ctx, finish, err := m.BeginBaselineCapture(context.Background(), key)
	if err != nil {
		t.Fatal(err)
	}
	defer finish()
	group, _ := ctx.Value(rawCaptureKey{}).(*rawCapture)
	inherited.Store(group)
	p.mu.Lock()
	p.anchorChecks = nil
	p.mu.Unlock()
	beforeReads := reads.Load()
	if err = p.VerifyAnchor(ctx, ingest.CatalogAnchor{Chain: key, Height: 100, Hash: state.BaseHash, Generation: 1}); err != nil {
		t.Fatal(err)
	}
	if reads.Load() <= beforeReads {
		t.Fatal("anchor test did not exercise HTTP identity read")
	}
	if len(m.observationEntries(key)) != before {
		t.Fatal("baseline inheritance minted independent release authority")
	}
}
