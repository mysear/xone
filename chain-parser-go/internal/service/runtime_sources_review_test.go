package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/ethereum/go-ethereum/core/types"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

func TestRuntimeSourcesReviewUnverifiedEndpointMustNotQualify(t *testing.T) {
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
			return
		case <-time.After(time.Second):
		}
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer slow.Close()
	var ignored atomic.Bool
	var calls, identityCalls atomic.Int32
	base := providerFixture(t, &ignored, &calls)
	endpoint, _ := url.Parse(base.URLs[0])
	proxy := httputil.NewSingleHostReverseProxy(endpoint)
	wrong := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		r.Body = io.NopCloser(bytes.NewReader(raw))
		var q struct {
			ID     any    `json:"id"`
			Method string `json:"method"`
		}
		_ = json.Unmarshal(raw, &q)
		if q.Method == "eth_chainId" {
			identityCalls.Add(1)
			_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": "0x2"})
			return
		}
		proxy.ServeHTTP(w, r)
	}))
	defer wrong.Close()
	client := rpc.New([]string{slow.URL, wrong.URL}, time.Second, 1<<20)
	cfg := probeConfig()
	cfg.TimeoutMS = 1000
	p, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), cfg)
	if err != nil {
		t.Fatal(err)
	}
	probeErr := p.Refresh(context.Background())
	gateErr := p.Gate(ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1})
	t.Logf("refresh=%v chainId_calls_to_wrong_chain=%d gate=%v providers=%+v", probeErr, identityCalls.Load(), gateErr, p.Snapshot().Providers)
	if gateErr == nil {
		t.Fatal("unverified wrong-chain endpoint qualified after identity budget timed out")
	}
}

func TestRuntimeSourcesReviewStaticFinalGateRejectsExpiredCapabilities(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	p := s.providerRuntime[ch.Key]
	p.mu.Lock()
	for i := range p.snapshot.Providers {
		p.snapshot.Providers[i].CheckedAt = time.Now().Add(-time.Hour)
	}
	p.mu.Unlock()
	if p.Gate(runtimeAnchor(snap.Reference)) == nil {
		t.Fatal("test needs expired provider evidence")
	}
	next := snap
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.Hash = runtimeHash("e")
	if err := s.runtimeScopeGate(context.Background(), nil, next)(); err == nil {
		t.Fatal("static publication gate accepted expired provider capabilities")
	}
}

func TestRuntimeSourcesReviewReorgStagingDoesNotExhaustNewBranch(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	ch.DynamicCatalog.MaxCandidates = 1
	s.cfg.Config.Startup.MaxTail = 1
	_ = runtimeCompleteRange(t, s, ch, snap, false)
	// Only the durable tip is known to be orphaned. Future verified blocks
	// belong to the superseded Source epoch and never reached the journal.
	if err := s.revokeRuntimeSources(context.Background(), ch, []string{snap.Reference.Block.Hash}); err != nil {
		t.Fatal(err)
	}
	ref := snap.Reference
	ref.CanonicalEpoch++
	s.providerRuntime[ch.Key].normal.owner.Store(&runtimeOwnerView{Ref: ref, Valid: true})
	proof := catalogHTTPProofFixture()
	proof.Header.Number = "0xb"
	proof.Header.Hash = runtimeHash("9")
	proof.Header.Transactions = []string{}
	proof.Header.ReceiptsRoot = types.EmptyReceiptsHash.Hex()
	err := s.captureRuntimeComplete(context.Background(), ch, proof.Header, []evm.Receipt{})
	progress, _ := s.runtimeDiscoveryProgress(ch.Key)
	t.Logf("new branch capture=%v queue=%v", err, progress)
	if err != nil {
		t.Fatalf("superseded uncommitted observations block the replacement branch: %v", err)
	}
}

func TestRuntimeSourcesReviewFailedQueueSaveDoesNotLeakPin(t *testing.T) {
	s, ch, _ := runtimeQueueFixture(t)
	for height := uint64(20); height < 32; height++ {
		header := catalogHTTPProofFixture().Header
		header.Number = fmt.Sprintf("0x%x", height)
		header.Hash = fmt.Sprintf("0x%064x", height)
		header.Transactions = []string{}
		header.ReceiptsRoot = types.EmptyReceiptsHash.Hex()
		if err := s.captureRuntimeComplete(context.Background(), ch, header, []evm.Receipt{}); err != nil {
			t.Fatal(err)
		}
	}
	c := s.catalogRuntime[ch.Key]
	c.mu.Lock()
	previous, closer, err := c.db.Get(runtimeDiscoveryKey)
	if err != nil {
		c.mu.Unlock()
		t.Fatal(err)
	}
	budget := len(previous) + 32
	_ = closer.Close()
	c.options.Config.MaxBytes = int64(budget)
	ch.DynamicCatalog.MaxBytes = int64(budget)
	c.mu.Unlock()
	selected := ch
	selected.Pools = nil
	_, _, _, err = s.adapters[ch.Key].CopyWithChain(selected).ReadComplete(context.Background(), "0xb")
	if !errors.Is(err, ingest.ErrBudget) {
		t.Fatalf("need queue save budget error, got %v", err)
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil {
		t.Fatal(err)
	}
	records, err := c.options.Archive.RawRecords()
	if err != nil {
		t.Fatal(err)
	}
	for _, record := range records {
		if record.Raw.Owner.ProviderID == "verified-normal-complete" && record.ActiveReferences > 0 {
			t.Fatalf("queue save failed with no durable release owner: queue=%v pin_count=%d raw=%s", progress, record.ActiveReferences, record.Raw.ID)
		}
	}
}

func TestRuntimeSourcesReviewReorgMustKeepCommittedCanonicalAncestorCreation(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	next := runtimeCompleteRange(t, s, ch, snap, true)
	if err := s.revokeRuntimeSources(context.Background(), ch, []string{next.Reference.Block.Hash}); err != nil {
		t.Fatal(err)
	}
	client := s.adapters[ch.Key].Client
	upstream := client.HTTP.Transport
	client.HTTP.Transport = runtimeTestTransport(func(r *http.Request) (*http.Response, error) {
		body, _ := r.GetBody()
		raw, _ := io.ReadAll(body)
		_ = body.Close()
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.Unmarshal(raw, &q)
		if q.Method == "eth_getBlockByNumber" && len(q.Params) > 0 && string(q.Params[0]) == `"0xc"` {
			header := catalogHTTPProofFixture().Header
			header.Number = "0xc"
			header.Hash = runtimeHash("9")
			header.ParentHash = runtimeHash("e")
			header.Transactions = []string{}
			header.ReceiptsRoot = types.EmptyReceiptsHash.Hex()
			header.Timestamp = fmt.Sprintf("0x%x", time.Now().Unix())
			encoded, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": header})
			return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(bytes.NewReader(encoded)), Request: r}, nil
		}
		return upstream.RoundTrip(r)
	})
	next.Reference.CanonicalEpoch++
	next.Reference.OnlineGeneration++
	next.Reference.WriterEpoch++
	next.Reference.JournalSeq++
	next.Reference.Block.Hash = runtimeHash("9")
	next.Reference.Block.ParentHash = runtimeHash("e")
	if err := s.stores[ch.Key].InstallBaseline(next); err != nil {
		t.Fatal(err)
	}
	if err := s.tickRuntimeSources(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	progress, _ := s.runtimeDiscoveryProgress(ch.Key)
	catalog := s.catalogRuntime[ch.Key].CatalogSnapshot(ingest.CatalogOnline)
	if len(catalog.Entries) != 1 {
		t.Fatalf("H11 creation is still canonical under replacement H12 but was consumed without admission: progress=%v entries=%d", progress, len(catalog.Entries))
	}
}

func TestRuntimeSourcesReviewStaticGateChecksLiveEvidenceAtInvocation(t *testing.T) {
	for _, mode := range []string{"expired", "call-refuted", "identity-refuted"} {
		t.Run(mode, func(t *testing.T) {
			s, ch, snap := runtimeQueueFixture(t)
			next := snap
			next.Reference.JournalSeq++
			next.Reference.Block.Number++
			next.Reference.Block.Hash = runtimeHash("e")
			gate := s.runtimeScopeGate(context.Background(), nil, next)
			if err := gate(); err != nil {
				t.Fatal("fresh static selection rejected", err)
			}
			p := s.providerRuntime[ch.Key]
			p.mu.Lock()
			for i := range p.snapshot.Providers {
				switch mode {
				case "expired":
					p.snapshot.Providers[i].CheckedAt = time.Now().Add(-time.Hour)
				case "call-refuted":
					p.snapshot.Providers[i].CurrentCallPinned.Status = rpc.NotSupported
				case "identity-refuted":
					p.snapshot.Providers[i].Identity.Status = rpc.NotSupported
				}
			}
			p.mu.Unlock()
			if err := gate(); err == nil {
				t.Fatal("final gate cached capability approval across refresh/refutation")
			}
		})
	}
}

func TestRuntimeSourcesReviewCanonicalAncestorsSurviveUndoPruning(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	next := runtimeCompleteRange(t, s, ch, snap, true)
	next.Reference.CanonicalEpoch += 2
	next.Reference.OnlineGeneration++
	next.Reference.WriterEpoch++
	next.Reference.JournalSeq++
	next.Reference.Block.ParentHash = next.Reference.Block.Hash
	next.Reference.Block.Number++
	next.Reference.Block.Hash = runtimeHash("9")
	if err := s.stores[ch.Key].InstallBaseline(next); err != nil {
		t.Fatal(err)
	}
	view, err := s.runtimeCanonicalView(ch)
	if err != nil {
		t.Fatal(err)
	}
	if ok, err := view.contains(context.Background(), 11, runtimeHash("e")); err != nil || !ok {
		t.Fatal("durable journal ancestry lost after baseline removed undo", ok, err)
	}
	if ok, err := view.contains(context.Background(), 11, runtimeHash("7")); err != nil || ok {
		t.Fatal("orphan accepted as durable ancestor", ok, err)
	}
	if err = s.reconcileRuntimeDiscovery(context.Background(), ch, next.Reference); err != nil {
		t.Fatal(err)
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["queued_complete_blocks"] != 2 {
		t.Fatal("canonical observations discarded while changing source fence", progress, err)
	}
}
