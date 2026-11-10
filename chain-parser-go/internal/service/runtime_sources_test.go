package service

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

func advancingProvider(t *testing.T, hooks ...func(*http.Request, string, []json.RawMessage)) (*rpc.Client, *atomic.Int32, *atomic.Int32) {
	t.Helper()
	var ignored atomic.Bool
	history := &atomic.Int32{}
	base := providerFixture(t, &ignored, history, hooks...)
	endpoint, _ := url.Parse(base.URLs[0])
	proxy := httputil.NewSingleHostReverseProxy(endpoint)
	heads := &atomic.Int32{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		r.Body = io.NopCloser(bytes.NewReader(raw))
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.Unmarshal(raw, &q)
		if q.Method == "eth_getBlockByNumber" && len(q.Params) > 0 && string(q.Params[0]) == `"0xb"` {
			heads.Add(1)
			_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": map[string]any{"number": "0xb", "hash": runtimeHash("e"), "parentHash": runtimeHash("c")}})
			return
		}
		proxy.ServeHTTP(w, r)
	}))
	t.Cleanup(server.Close)
	return rpc.New([]string{server.URL}, time.Second, 1<<20), history, heads
}
func TestRuntimeSourcesFastHeadAdvanceDoesNotProbeHistoryOrExtendCapabilities(t *testing.T) {
	client, history, heads := advancingProvider(t)
	cfg := probeConfig()
	cfg.IntervalMS = 30000
	cfg.MaxBackoffMS = 60000
	cfg.MaxAgeMS = 90000
	p, err := NewProviderRuntime(client, "evm:1", runtimeHash("a"), cfg)
	if err != nil {
		t.Fatal(err)
	}
	if err = p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	before := history.Load()
	checked := p.Snapshot().Providers[0].CheckedAt
	a := ingest.CatalogAnchor{Chain: "evm:1", Height: 11, Hash: runtimeHash("e"), Generation: 1}
	if p.Gate(a) == nil {
		t.Fatal("fixture did not reproduce moving-head gate")
	}
	if err = p.VerifyAnchor(context.Background(), a); err != nil {
		t.Fatal(err)
	}
	if err = p.Gate(a); err != nil {
		t.Fatal(err)
	}
	if err = p.VerifyAnchor(context.Background(), a); err != nil {
		t.Fatal(err)
	}
	if history.Load() != before || heads.Load() != 1 {
		t.Fatal("fast head multiplied probes", history.Load()-before, heads.Load())
	}
	if !p.Snapshot().Providers[0].CheckedAt.Equal(checked) {
		t.Fatal("head update extended capability lifetime")
	}
	p.mu.Lock()
	p.snapshot.Providers[0].CheckedAt = time.Now().Add(-time.Hour)
	p.mu.Unlock()
	if p.VerifyAnchor(context.Background(), a) == nil || heads.Load() != 1 {
		t.Fatal("expired capability revived by HTTP head")
	}
}
func TestRuntimeSourcesFastHeadRejectsWrongIdentityAndCancellation(t *testing.T) {
	client, history, _ := advancingProvider(t)
	p, _ := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err := p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	before := history.Load()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if p.VerifyAnchor(ctx, ingest.CatalogAnchor{Chain: "evm:1", Height: 11, Hash: runtimeHash("e"), Generation: 1}) == nil {
		t.Fatal("canceled head verification")
	}
	if history.Load() != before {
		t.Fatal("canceled verification did IO")
	}
	if p.VerifyAnchor(context.Background(), ingest.CatalogAnchor{Chain: "evm:1", Height: 11, Hash: runtimeHash("f"), Generation: 1}) == nil {
		t.Fatal("wrong head hash accepted")
	}
}

func runtimeBridgeFixture(t *testing.T) (*Service, *catalogFixtureState, domain.Snapshot) {
	t.Helper()
	f := catalogFixture(t)
	f.httpProofEnabled.Store(true)
	endpoint, _ := url.Parse(f.options.Client.URLs[0])
	proxy := httputil.NewSingleHostReverseProxy(endpoint)
	timestamp := fmt.Sprintf("0x%x", time.Now().Unix())
	proxy.ModifyResponse = func(response *http.Response) error {
		raw, err := io.ReadAll(response.Body)
		if err != nil {
			return err
		}
		var body map[string]any
		if json.Unmarshal(raw, &body) == nil {
			if result, ok := body["result"].(map[string]any); ok && result["number"] == "0xa" {
				result["receiptsRoot"] = types.EmptyReceiptsHash.Hex()
				result["timestamp"] = timestamp
				raw, _ = json.Marshal(body)
			}
		}
		response.Body = io.NopCloser(bytes.NewReader(raw))
		response.ContentLength = int64(len(raw))
		response.Header.Set("Content-Length", fmt.Sprint(len(raw)))
		return nil
	}
	server := httptest.NewServer(proxy)
	t.Cleanup(server.Close)
	client := rpc.New([]string{server.URL}, time.Second, 1<<20)
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	ch := f.options.Chain
	ch.Owner = "chain-parser-go"
	ch.Assignment = 1
	ch.ReorgWindow = 128
	ch.Confirmation = "latest"
	ch.RPCURLs = []string{server.URL}
	probe := probeConfig()
	ch.ProviderProbe = &probe
	catalog := f.options.Config
	ch.DynamicCatalog = &catalog
	pin := f.options.Config.Sources[0].PoolCodeSHA256
	ch.Pools = []config.PoolConfig{{ID: catalogAddr(30), Protocol: "uniswap-v2", Token0: catalogAddr(1), Token1: catalogAddr(2), FeeNumerator: 3, FeeDenominator: 1000, V2: &config.V2PoolConfig{ExpectedFactory: catalogAddr(10), ExpectedCodeSHA256: pin}}}
	cfg.Config.Chains = []config.Chain{ch}
	store, err := reducer.Open(filepath.Join(t.TempDir(), "state"), ch.Owner, 1, 128)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = store.Close() })
	static := domain.Pool{ChainKey: ch.Key, PoolID: catalogAddr(30), Protocol: "uniswap-v2", Token0: catalogAddr(1), Token1: catalogAddr(2), Reserve0: "1000", Reserve1: "2000", FeeNumerator: 3, FeeDenominator: 1000, AdapterVersion: "uniswap-v2/1", TokenBehavior: "standard", V2: &domain.V2State{Factory: catalogAddr(10), CodeSHA256: pin}}
	adapterHash, _ := domain.Hash([]string{static.AdapterVersion})
	scopeHash, _ := domain.ScopeHash([]domain.Pool{static})
	snap := domain.Snapshot{Reference: domain.SnapshotRef{Block: domain.BlockRef{ChainKey: ch.Key, Number: 10, Hash: runtimeHash("c"), ParentHash: runtimeHash("d"), TimestampMS: uint64(time.Now().UnixMilli())}, WriterEpoch: 1, CanonicalEpoch: 1, OnlineGeneration: 1, JournalSeq: 1, SchemaVersion: domain.SchemaVersion, AdapterSetHash: adapterHash, ScopeHash: scopeHash, OwnerService: ch.Owner, OwnerAssignmentVersion: 1}, Pools: []domain.Pool{static}, HistoryGaps: []domain.Gap{}}
	if err = store.InstallBaseline(snap); err != nil {
		t.Fatal(err)
	}
	s := &Service{cfg: cfg, stores: map[string]*reducer.Store{ch.Key: store}, adapters: map[string]*evm.Adapter{ch.Key: {Client: client, Chain: ch}}, rawSources: map[string]*rawRecorder{ch.Key: newRawRecorder(ch.Key, f.options.Archive)}}
	if err = s.initRuntimeSources(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.closeRuntimeSources)
	return s, f, snap
}
func TestRuntimeSourcesLifecycleDiscoveryAndHandler(t *testing.T) {
	s, _, snap := runtimeBridgeFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	s.startRuntimeSources(ctx)
	ch := s.cfg.Config.Chains[0]
	if err := s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	catalog := s.catalogRuntime[ch.Key]
	c, err := catalog.DiscoverHeight(ctx, ingest.CatalogOnline, 7)
	if err != nil {
		t.Fatal(err)
	}
	if err = catalog.Admit(ctx, ingest.CatalogOnline, c); err != nil {
		t.Fatal(err)
	}
	mixed := s.runtimeCatalogChain(ch)
	if len(mixed.Pools) != 2 {
		t.Fatal("admitted dynamic identity missing", mixed.Pools)
	}
	recorder := httptest.NewRecorder()
	request := httptest.NewRequest("GET", "/internal/chains/evm:1/source-evidence", nil)
	request.SetPathValue("chain", ch.Key)
	s.runtimeSourceStatus(recorder, request)
	if recorder.Code != 200 || !bytes.Contains(recorder.Body.Bytes(), []byte(`"pool_count":1`)) {
		t.Fatal(recorder.Code, recorder.Body.String())
	}
	if current, ok := s.stores[ch.Key].Current(); !ok || current.Reference != snap.Reference {
		t.Fatal("catalog qualification changed static scope")
	}
	s.closeRuntimeSources()
	if catalog.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("closed catalog still ready")
	}
}
func TestRuntimeSourcesFreezeCrossGenerationAndCurrentReader(t *testing.T) {
	s, _, snap := runtimeBridgeFixture(t)
	ctx := context.Background()
	s.startRuntimeSources(ctx)
	ch := s.cfg.Config.Chains[0]
	if err := s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	catalog := s.catalogRuntime[ch.Key]
	c, err := catalog.DiscoverHeight(ctx, ingest.CatalogOnline, 7)
	if err != nil {
		t.Fatal(err)
	}
	if err = catalog.Admit(ctx, ingest.CatalogOnline, c); err != nil {
		t.Fatal(err)
	}
	selected := s.runtimeCatalogChain(ch)
	frozen, err := s.freezeRuntimeScope(ctx, selected)
	if err != nil {
		t.Fatal(err)
	}
	dynamic := catalog.CatalogSnapshot(ingest.CatalogOnline).Entries[0].Pool
	next := snap
	next.Pools = append(append([]domain.Pool(nil), snap.Pools...), dynamic)
	next.Reference.OnlineGeneration++
	next.Reference.WriterEpoch++
	next.Reference.JournalSeq++
	next.Reference.ScopeHash, _ = domain.ScopeHash(next.Pools)
	gate := s.runtimeScopeGate(ctx, frozen, next)
	if err = gate(); err != nil {
		t.Fatal("cross generation selection rejected", err)
	}
	// Execute the final gate under the real owner lock: it must never reread Store.
	done := make(chan error, 1)
	go func() {
		done <- s.stores[ch.Key].InstallBaselineScope(next, []byte("{}"), "runtime-gate", json.RawMessage("{}"), gate)
	}()
	select {
	case err = <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("scope gate deadlocked under owner lock")
	}
	if err = s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	reader, selection, err := s.runtimeScopeAdapter(ctx, ch, next)
	if err != nil || reader == nil || len(reader.Chain.Pools) != 2 || selection == nil {
		t.Fatal("current reader lost dynamic pool", err)
	}
	if err = s.revokeRuntimeSources(ctx, ch, []string{runtimeHash("b")}); err != nil {
		t.Fatal(err)
	}
	if gate() == nil {
		t.Fatal("revoked frozen scope published")
	}
}

func TestRuntimeSourcesFastHeadDoesNotWaitForHistoricalActor(t *testing.T) {
	client, _, _ := advancingProvider(t)
	p, _ := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err := p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	p.busy.Store(true)
	defer p.busy.Store(false)
	if err := p.VerifyAnchor(context.Background(), ingest.CatalogAnchor{Chain: "evm:1", Height: 11, Hash: runtimeHash("e"), Generation: 1}); err != nil {
		t.Fatal("history actor blocked current head", err)
	}
}

func TestRuntimeSourcesCatalogFailureDoesNotSuspendStaticAndRetryIsBounded(t *testing.T) {
	s, _, snap := runtimeBridgeFixture(t)
	ctx := context.Background()
	s.startRuntimeSources(ctx)
	ch := s.cfg.Config.Chains[0]
	catalog := s.catalogRuntime[ch.Key]
	catalog.mu.Lock()
	catalog.blocked = true
	catalog.mu.Unlock()
	if s.tickRuntimeSources(ctx, ch) == nil {
		t.Fatal("catalog failure not recorded")
	}
	checked := s.providerRuntime[ch.Key].Snapshot().Providers[0].Observation.Attempts
	// An immediate same-fence tick performs no more HTTP/catalog attempts.
	if err := s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	if s.providerRuntime[ch.Key].Snapshot().Providers[0].Observation.Attempts != checked {
		t.Fatal("failed catalog retried outside budget")
	}
	if current, ready := s.stores[ch.Key].Current(); !ready || current.Reference != snap.Reference {
		t.Fatal("catalog failure revoked static scope")
	}
	if len(s.runtimeCatalogChain(ch).Pools) != 1 {
		t.Fatal("failed catalog supplied candidate configs")
	}
}
func TestRuntimeSourcesCandidateCannotBeSelectedAndHistoryStaysOut(t *testing.T) {
	s, _, _ := runtimeBridgeFixture(t)
	ctx := context.Background()
	s.startRuntimeSources(ctx)
	ch := s.cfg.Config.Chains[0]
	if err := s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	catalog := s.catalogRuntime[ch.Key]
	c, err := catalog.DiscoverHeight(ctx, ingest.CatalogHistory, 7)
	if err != nil {
		t.Fatal(err)
	}
	if err = catalog.Admit(ctx, ingest.CatalogHistory, c); err != nil {
		t.Fatal(err)
	}
	if len(s.runtimeCatalogChain(ch).Pools) != 1 {
		t.Fatal("history mixed into target catalog")
	}
	fake := ch
	fake.Pools = append(append([]config.PoolConfig(nil), ch.Pools...), config.PoolConfig{ID: catalogAddr(20), Protocol: "uniswap-v2", Token0: catalogAddr(1), Token1: catalogAddr(2)})
	if _, err = s.freezeRuntimeScope(ctx, fake); err == nil {
		t.Fatal("candidate entered scope without online admission")
	}
}
func TestRuntimeSourcesV3StrictCodePinCannotCoverTwoImmutableInstances(t *testing.T) {
	// A minimal runtime template with constructor-patched PUSH32 operands. Both
	// instances have the same instruction/template bytes and different token/self
	// identities. This is a counterexample for whole-runtime digest reuse only;
	// it does not claim to verify an official artifact or template provenance.
	runtime := func(token, self int) string {
		return "0x7f" + fmt.Sprintf("%064x", token) + "507f" + fmt.Sprintf("%064x", self) + "5000"
	}
	first, second := runtime(1, 20), runtime(2, 21)
	raw, _ := hex.DecodeString(first[2:])
	sum := sha256.Sum256(raw)
	pin := hex.EncodeToString(sum[:])
	if err := catalogCheckCode(first, pin); err != nil {
		t.Fatal(err)
	}
	if err := catalogCheckCode(second, pin); err == nil {
		t.Fatal("strict whole-runtime pin accepted different immutable instance")
	}
}

func TestRuntimeSourcesGateAndStatusRejectExpiredOwnerWithoutStoreReentry(t *testing.T) {
	s, _, snap := runtimeBridgeFixture(t)
	ctx := context.Background()
	s.startRuntimeSources(ctx)
	ch := s.cfg.Config.Chains[0]
	if err := s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	catalog := s.catalogRuntime[ch.Key]
	c, err := catalog.DiscoverHeight(ctx, ingest.CatalogOnline, 7)
	if err != nil {
		t.Fatal(err)
	}
	if err = catalog.Admit(ctx, ingest.CatalogOnline, c); err != nil {
		t.Fatal(err)
	}
	selected := s.runtimeCatalogChain(ch)
	frozen, err := s.freezeRuntimeScope(ctx, selected)
	if err != nil {
		t.Fatal(err)
	}
	target := snap
	target.Reference.JournalSeq++
	target.Pools = append(append([]domain.Pool(nil), snap.Pools...), catalog.CatalogSnapshot(ingest.CatalogOnline).Entries[0].Pool)
	s.providerRuntime[ch.Key].normal.owner.Store(&runtimeOwnerView{Ref: snap.Reference, DeadlineMS: uint64(time.Now().Add(-time.Second).UnixMilli()), Valid: true})
	if s.runtimeScopeGate(ctx, frozen, target)() == nil {
		t.Fatal("expired owner authorized scope")
	}
	recorder := httptest.NewRecorder()
	request := httptest.NewRequest("GET", "/internal/chains/evm:1/source-evidence", nil)
	request.SetPathValue("chain", ch.Key)
	s.runtimeSourceStatus(recorder, request)
	var status struct {
		Catalog struct {
			Ready bool `json:"ready"`
		} `json:"catalog"`
	}
	if json.Unmarshal(recorder.Body.Bytes(), &status) != nil || status.Catalog.Ready {
		t.Fatal("status claimed expired owner ready", recorder.Body.String())
	}
}

func TestRuntimeSourcesFastHeadOverlapsRealHistoricalProbe(t *testing.T) {
	var block atomic.Bool
	entered := make(chan struct{}, 1)
	release := make(chan struct{})
	client, _, heads := advancingProvider(t, func(r *http.Request, method string, params []json.RawMessage) {
		if block.Load() && method == "eth_getBlockByNumber" && string(params[0]) == `"0x7"` {
			select {
			case entered <- struct{}{}:
			default:
			}
			select {
			case <-release:
			case <-r.Context().Done():
			}
		}
	})
	p, _ := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err := p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	p.mu.Lock()
	p.next = time.Time{}
	p.mu.Unlock()
	block.Store(true)
	done := make(chan error, 1)
	go func() { done <- p.Refresh(context.Background()) }()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("historical probe did not start")
	}
	err := p.VerifyAnchor(context.Background(), ingest.CatalogAnchor{Chain: "evm:1", Height: 11, Hash: runtimeHash("e"), Generation: 1})
	close(release)
	if err != nil || heads.Load() != 1 {
		t.Fatal("full history probe starved current anchor", err, heads.Load())
	}
	if err = <-done; err != nil {
		t.Fatal(err)
	}
}

func TestRuntimeSourcesStateReadsUseQualifiedEndpointWithoutRemovingWitnesses(t *testing.T) {
	var ignored, good atomic.Bool
	ignored.Store(true)
	var badCalls, goodCalls atomic.Int32
	bad := providerFixture(t, &ignored, &badCalls)
	healthy := providerFixture(t, &good, &goodCalls)
	client := rpc.New([]string{bad.URLs[0], healthy.URLs[0]}, time.Second, 1<<20)
	probe := probeConfig()
	probe.IntervalMS = 30000
	probe.MaxBackoffMS = 60000
	probe.MaxAgeMS = 90000
	ch := config.Chain{Key: "evm:1", Genesis: runtimeHash("a"), ProviderProbe: &probe}
	s := &Service{cfg: &config.Snapshot{Config: config.Config{Chains: []config.Chain{ch}}}, adapters: map[string]*evm.Adapter{ch.Key: {Client: client, Chain: ch}}}
	if err := s.initRuntimeSources(); err != nil {
		t.Fatal(err)
	}
	defer s.closeRuntimeSources()
	s.startRuntimeSources(context.Background())
	badBefore, goodBefore := badCalls.Load(), goodCalls.Load()
	var balance string
	if err := client.Call(context.Background(), "eth_getBalance", []any{probe.Account, map[string]any{"blockHash": runtimeHash("c"), "requireCanonical": true}}, &balance); err != nil {
		t.Fatal(err)
	}
	if badCalls.Load() != badBefore || goodCalls.Load() != goodBefore+1 {
		t.Fatal("normal state call used an unqualified provider", badCalls.Load()-badBefore, goodCalls.Load()-goodBefore)
	}
	configured, witnesses := client.ProviderClients()
	if configured != 2 || len(witnesses) != 2 {
		t.Fatal("capability routing silently reduced receipt witnesses", configured, len(witnesses))
	}
}

func TestRuntimeSourcesBalanceControlDoesNotCertifyIgnoredEthCallSelector(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	base := providerFixture(t, &ignored, &calls)
	endpoint, _ := url.Parse(base.URLs[0])
	proxy := httputil.NewSingleHostReverseProxy(endpoint)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		r.Body = io.NopCloser(bytes.NewReader(raw))
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.Unmarshal(raw, &q)
		if q.Method == "eth_getCode" || q.Method == "eth_call" {
			var selector map[string]any
			_ = json.Unmarshal(q.Params[1], &selector)
			if q.Method == "eth_getCode" && selector["blockHash"] != runtimeHash("c") {
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32001, "message": "missing"}})
				return
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": "0x"})
			return
		}
		proxy.ServeHTTP(w, r)
	}))
	defer server.Close()
	p, _ := NewProviderRuntime(rpc.New([]string{server.URL}, time.Second, 1<<20), "evm:1", runtimeHash("a"), probeConfig())
	if err := p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if p.Snapshot().Providers[0].CurrentHashPinned.Status != rpc.Supported {
		t.Fatal("balance control fixture failed")
	}
	if p.Gate(ingest.CatalogAnchor{Chain: "evm:1", Height: 10, Hash: runtimeHash("c"), Generation: 1}) == nil {
		t.Fatal("balance capability authorized ignored eth_call selector")
	}
}

func TestRuntimeSourcesCompleteHookPreservesCloneAndVerifierBarrier(t *testing.T) {
	f := catalogFixture(t)
	f.httpProofEnabled.Store(true)
	a := &evm.Adapter{Client: f.options.Client, Chain: f.options.Chain}
	var calls atomic.Int32
	a.OnComplete = func(ctx context.Context, b evm.RawBlock, receipts []evm.Receipt) error {
		calls.Add(1)
		if b.Hash != runtimeHash("b") || len(receipts) != 1 {
			return errors.New("wrong complete proof")
		}
		return ctx.Err()
	}
	copy := a.CopyWithChain(a.Chain)
	if _, _, _, err := copy.ReadComplete(context.Background(), "0x7"); err != nil {
		t.Fatal(err)
	}
	if calls.Load() != 1 {
		t.Fatal("clone lost callback")
	}
	f.truncatedReceipts.Store(true)
	if _, _, _, err := copy.ReadComplete(context.Background(), "0x7"); err == nil {
		t.Fatal("truncated receipt accepted")
	}
	if calls.Load() != 1 {
		t.Fatal("hook ran before receipt root verifier")
	}
	f.truncatedReceipts.Store(false)
	copy.OnComplete = func(context.Context, evm.RawBlock, []evm.Receipt) error { return ingest.ErrBudget }
	if _, _, _, err := copy.ReadComplete(context.Background(), "0x7"); !errors.Is(err, ingest.ErrBudget) {
		t.Fatal("complete hook did not backpressure reader", err)
	}
}

type runtimeTestTransport func(*http.Request) (*http.Response, error)

func (f runtimeTestTransport) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }
func runtimeQueueFixture(t *testing.T) (*Service, config.Chain, domain.Snapshot) {
	t.Helper()
	s, _, snap := runtimeBridgeFixture(t)
	ch := s.cfg.Config.Chains[0]
	ch.ProviderProbe.IntervalMS = 30000
	ch.ProviderProbe.MaxBackoffMS = 60000
	ch.ProviderProbe.MaxAgeMS = 90000
	s.cfg.Config.Chains[0] = ch
	p := s.providerRuntime[ch.Key]
	p.cfg = *ch.ProviderProbe
	client := s.adapters[ch.Key].Client
	upstream := client.HTTP.Transport
	proof := catalogHTTPProofFixture()
	proof.Header.Number = "0xb"
	proof.Header.Hash = runtimeHash("e")
	proof.Header.ParentHash = runtimeHash("c")
	proof.Header.Timestamp = fmt.Sprintf("0x%x", time.Now().Unix())
	proof.Receipts[0].BlockHash = proof.Header.Hash
	for i := range proof.Receipts[0].Logs {
		proof.Receipts[0].Logs[i].BlockHash = proof.Header.Hash
	}
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
		var result any
		intercept := false
		if q.Method == "eth_getBlockByNumber" {
			var tag string
			_ = json.Unmarshal(q.Params[0], &tag)
			switch tag {
			case "0xb":
				result = proof.Header
				intercept = true
			case "0xc", "0xd":
				header := proof.Header
				header.Number = tag
				header.Hash = runtimeHash("f")
				header.ParentHash = runtimeHash("e")
				if tag == "0xd" {
					header.Hash = runtimeHash("9")
					header.ParentHash = runtimeHash("f")
				}
				header.Transactions = []string{}
				header.ReceiptsRoot = types.EmptyReceiptsHash.Hex()
				result = header
				intercept = true
			}
		}
		if q.Method == "eth_getTransactionReceipt" {
			result = proof.Receipts[0]
			intercept = true
		}
		if intercept {
			encoded, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
			return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(bytes.NewReader(encoded)), Request: r}, nil
		}
		if q.Method == "eth_getCode" || q.Method == "eth_call" || q.Method == "eth_getBalance" {
			var selector map[string]any
			if len(q.Params) > 1 && json.Unmarshal(q.Params[1], &selector) == nil && (selector["blockHash"] == runtimeHash("e") || selector["blockHash"] == runtimeHash("f") || selector["blockHash"] == runtimeHash("9")) {
				selector["blockHash"] = runtimeHash("c")
				q.Params[1], _ = json.Marshal(selector)
				var whole map[string]json.RawMessage
				_ = json.Unmarshal(raw, &whole)
				whole["params"], _ = json.Marshal(q.Params)
				raw, _ = json.Marshal(whole)
				clone := r.Clone(r.Context())
				clone.Body = io.NopCloser(bytes.NewReader(raw))
				clone.GetBody = func() (io.ReadCloser, error) { return io.NopCloser(bytes.NewReader(raw)), nil }
				clone.ContentLength = int64(len(raw))
				r = clone
			}
		}
		return upstream.RoundTrip(r)
	})
	s.startRuntimeSources(context.Background())
	if err := s.tickRuntimeSources(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	return s, ch, snap
}
func runtimeCompleteRange(t *testing.T, s *Service, ch config.Chain, snap domain.Snapshot, commit bool) domain.Snapshot {
	t.Helper()
	selected := ch
	selected.Pools = nil
	a := s.adapters[ch.Key].CopyWithChain(selected)
	for _, height := range []uint64{11, 12} {
		block, _, _, err := a.ReadComplete(context.Background(), fmt.Sprintf("0x%x", height))
		if err != nil {
			t.Fatal(err)
		}
		if commit {
			next := snap
			next.Reference.Block = block
			next.Reference.JournalSeq++
			if err = s.stores[ch.Key].Commit(next); err != nil {
				t.Fatal(err)
			}
			snap = next
		}
	}
	return snap
}
func TestRuntimeSourcesContinuousQueueCoversEveryCommittedBlockInsideProbeInterval(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	before := s.catalogRuntime[ch.Key].revision
	// H+1 contains a creation; H+2 does not. Neither is the baseline/probe head.
	next := runtimeCompleteRange(t, s, ch, snap, true)
	if s.catalogRuntime[ch.Key].revision != before {
		t.Fatal("completion hook refreshed a staging capsule")
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil {
		t.Fatal(err)
	}
	if progress["discovery_journal_seq"] != snap.Reference.JournalSeq || progress["queued_complete_blocks"] != 2 {
		t.Fatal("committed cursor was presented as discovered", progress)
	}
	if err = s.tickRuntimeSources(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	progress, err = s.runtimeDiscoveryProgress(ch.Key)
	if err != nil {
		t.Fatal(err)
	}
	if progress["discovery_journal_seq"] != next.Reference.JournalSeq || progress["queued_complete_blocks"] != 0 {
		t.Fatal("online interval silently skipped a block", progress)
	}
	catalog := s.catalogRuntime[ch.Key].CatalogSnapshot(ingest.CatalogOnline)
	if !catalog.Ready || len(catalog.Entries) != 1 || catalog.Entries[0].Admission.Candidate.Height != 11 || catalog.Anchor.Height != 12 {
		t.Fatal("H+1 creation was missed", catalog)
	}
	if current, ready := s.stores[ch.Key].Current(); !ready || len(current.Pools) != 1 {
		t.Fatal("discovery inserted a pool directly into scope")
	}
}
func TestRuntimeSourcesQueueSurvivesRestartAndDoesNotConsumeUncommittedProof(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	_ = runtimeCompleteRange(t, s, ch, snap, false)
	if err := s.tickRuntimeSources(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	if s.catalogRuntime[ch.Key].CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("uncommitted complete event was admitted")
	}
	original := s.catalogRuntime[ch.Key]
	options := original.options
	s.closeRuntimeSources()
	restored, err := OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Close()
	restored.mu.Lock()
	queue, err := loadRuntimeDiscoveryLocked(restored)
	restored.mu.Unlock()
	if err != nil || len(queue.Observed) != 2 || queue.Cursor != snap.Reference.JournalSeq {
		t.Fatal("pending proof lost on restart", queue, err)
	}
	if restored.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("restart trusted queued receipts as admission")
	}
	records, err := restored.options.Archive.RawRecords()
	if err != nil {
		t.Fatal(err)
	}
	pins := 0
	for _, record := range records {
		pins += record.ActiveReferences
	}
	if pins < 1 {
		t.Fatal("queued creation proof was reclaimed")
	}
}
func TestRuntimeSourcesQueueBudgetBackpressuresCompleteAndPreservesPending(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	ch.DynamicCatalog.MaxCandidates = 1
	s.cfg.Config.Startup.MaxTail = 1
	_ = runtimeCompleteRange(t, s, ch, snap, false)
	selected := ch
	selected.Pools = nil
	if _, _, _, err := s.adapters[ch.Key].CopyWithChain(selected).ReadComplete(context.Background(), "0xd"); !errors.Is(err, ingest.ErrBudget) {
		t.Fatal("full outbox did not backpressure verified read", err)
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["queued_complete_blocks"] != 2 {
		t.Fatal(progress, err)
	}
}

func TestRuntimeSourcesStorageCapabilityIsIndependentOfCallAndBalance(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	client := providerFixture(t, &ignored, &calls)
	p, _ := NewProviderRuntime(client, "evm:1", runtimeHash("a"), probeConfig())
	if err := p.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if p.Snapshot().Providers[0].CurrentStoragePinned.Status != rpc.Supported {
		t.Fatal("storage selector did not get its own positive/negative control")
	}
}

func TestRuntimeSourcesMissingObservationUsesNormalCompleteVerifierWithoutSkipping(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	selected := ch
	selected.Pools = nil
	// Simulate a legacy reader/cache clone which omitted the new callback.
	plain := &evm.Adapter{Client: s.adapters[ch.Key].Client, Chain: selected}
	for _, height := range []uint64{11, 12} {
		block, _, _, err := plain.ReadComplete(context.Background(), fmt.Sprintf("0x%x", height))
		if err != nil {
			t.Fatal(err)
		}
		next := snap
		next.Reference.Block = block
		next.Reference.JournalSeq++
		if err = s.stores[ch.Key].Commit(next); err != nil {
			t.Fatal(err)
		}
		snap = next
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["queued_complete_blocks"] != 0 {
		t.Fatal(progress, err)
	}
	if err = s.tickRuntimeSources(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	progress, err = s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["discovery_journal_seq"] != snap.Reference.JournalSeq {
		t.Fatal("missing observation was not repaired", progress, err)
	}
	if !s.catalogRuntime[ch.Key].CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("fallback dropped H+1 creation")
	}
}
func TestRuntimeSourcesQueueTimeoutRetainsWorkAndPinUntilNextBudget(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	next := runtimeCompleteRange(t, s, ch, snap, true)
	c := s.catalogRuntime[ch.Key]
	c.options.VerifyBlock = func(ctx context.Context, _ ingest.FactoryBlock) error { <-ctx.Done(); return ctx.Err() }
	if s.tickRuntimeSources(context.Background(), ch) == nil {
		t.Fatal("exhausted budget was ignored")
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["discovery_journal_seq"] != snap.Reference.JournalSeq || progress["queued_complete_blocks"] != 2 {
		t.Fatal("failed work advanced discovery cursor", progress, err)
	}
	if current, ok := s.stores[ch.Key].Current(); !ok || current.Reference != next.Reference {
		t.Fatal("sidecar budget revoked current static source")
	}
	c.options.VerifyBlock = nil
	p := s.providerRuntime[ch.Key]
	p.normal.mu.Lock()
	p.normal.next = time.Time{}
	p.normal.mu.Unlock()
	if err = s.tickRuntimeSources(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	progress, err = s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["discovery_journal_seq"] != next.Reference.JournalSeq || progress["pending_pin_releases"] != 0 {
		t.Fatal("pending work did not resume", progress, err)
	}
}

func TestRuntimeSourcesRenewedActorLeaseKeepsFrozenScopeWindowValid(t *testing.T) {
	s, _, snap := runtimeBridgeFixture(t)
	ctx := context.Background()
	s.startRuntimeSources(ctx)
	ch := s.cfg.Config.Chains[0]
	if err := s.tickRuntimeSources(ctx, ch); err != nil {
		t.Fatal(err)
	}
	c := s.catalogRuntime[ch.Key]
	candidates, err := c.DiscoverHeight(ctx, ingest.CatalogOnline, 7)
	if err != nil {
		t.Fatal(err)
	}
	if err = c.Admit(ctx, ingest.CatalogOnline, candidates); err != nil {
		t.Fatal(err)
	}
	selected := s.runtimeCatalogChain(ch)
	frozen, err := s.freezeRuntimeScope(ctx, selected)
	if err != nil {
		t.Fatal(err)
	}
	next := snap
	next.Reference.OnlineGeneration++
	next.Reference.WriterEpoch++
	next.Reference.JournalSeq++
	next.Pools = append(append([]domain.Pool(nil), snap.Pools...), c.CatalogSnapshot(ingest.CatalogOnline).Entries[0].Pool)
	// The actor renewed/allocated its authoritative lease after selection. The
	// final gate must consume that view without rereading the reducer lock.
	s.providerRuntime[ch.Key].normal.owner.Store(&runtimeOwnerView{Ref: snap.Reference, DeadlineMS: uint64(time.Now().Add(-time.Second).UnixMilli()), Valid: true})
	s.sessions = map[string]*writerSession{ch.Key: {chain: ch, lease: control.Lease{Chain: ch.Key, WriterEpoch: next.Reference.WriterEpoch, Generation: next.Reference.OnlineGeneration, CanonicalEpoch: next.Reference.CanonicalEpoch, ExpiresMS: uint64(time.Now().Add(time.Minute).UnixMilli())}}}
	if err = s.runtimeScopeGate(ctx, frozen, next)(); err != nil {
		t.Fatal("renewal spuriously invalidated the staging capsule", err)
	}
}

func TestRuntimeSourcesReorgRemovesQueuedCreationBeforeReleasingItsPin(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	_ = runtimeCompleteRange(t, s, ch, snap, false)
	if err := s.revokeRuntimeSources(context.Background(), ch, []string{runtimeHash("e")}); err != nil {
		t.Fatal(err)
	}
	progress, err := s.runtimeDiscoveryProgress(ch.Key)
	if err != nil || progress["queued_complete_blocks"] != 1 || progress["pending_pin_releases"] != 1 {
		t.Fatal("orphan work was not durably removed", progress, err)
	}
	if s.catalogRuntime[ch.Key].CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("reorg candidate remained eligible")
	}
	if err = s.flushRuntimeDiscoveryReleases(context.Background(), ch); err != nil {
		t.Fatal(err)
	}
	records, err := s.catalogRuntime[ch.Key].options.Archive.RawRecords()
	if err != nil {
		t.Fatal(err)
	}
	for _, record := range records {
		if record.Raw.Owner.ProviderID == "verified-normal-complete" && record.ActiveReferences != 0 {
			t.Fatal("orphan pin was not released after durable removal", record)
		}
	}
}

func TestRuntimeSourcesPublicationBackpressuresMissingCallbackBacklog(t *testing.T) {
	s, ch, snap := runtimeQueueFixture(t)
	ch.DynamicCatalog.MaxCandidates = 1
	s.cfg.Config.Startup.MaxTail = 1
	selected := ch
	selected.Pools = nil
	legacy := &evm.Adapter{Client: s.adapters[ch.Key].Client, Chain: selected}
	for _, height := range []uint64{11, 12} {
		block, _, _, err := legacy.ReadComplete(context.Background(), fmt.Sprintf("0x%x", height))
		if err != nil {
			t.Fatal(err)
		}
		next := snap
		next.Reference.Block = block
		next.Reference.JournalSeq++
		if err = s.stores[ch.Key].Commit(next); err != nil {
			t.Fatal(err)
		}
		snap = next
	}
	block, _, _, err := legacy.ReadComplete(context.Background(), "0xd")
	if err != nil {
		t.Fatal(err)
	}
	target := snap
	target.Reference.Block = block
	target.Reference.JournalSeq++
	if err = s.runtimeScopeGate(context.Background(), nil, target)(); !errors.Is(err, ingest.ErrBudget) {
		t.Fatal("missing callback bypassed discovery backlog capacity", err)
	}
}

func TestRuntimeSourcesCloseRejectsNewRPCWithoutNetwork(t *testing.T) {
	var ignored atomic.Bool
	var calls atomic.Int32
	client := providerFixture(t, &ignored, &calls)
	cfg := probeConfig()
	ch := config.Chain{Key: "evm:1", Genesis: runtimeHash("a"), ProviderProbe: &cfg}
	s := &Service{cfg: &config.Snapshot{Config: config.Config{Chains: []config.Chain{ch}}}, adapters: map[string]*evm.Adapter{ch.Key: {Client: client, Chain: ch}}}
	if err := s.initRuntimeSources(); err != nil {
		t.Fatal(err)
	}
	s.startRuntimeSources(context.Background())
	s.closeRuntimeSources()
	before := calls.Load()
	var balance string
	if err := client.Call(context.Background(), "eth_getBalance", []any{cfg.Account, map[string]any{"blockHash": runtimeHash("c"), "requireCanonical": true}}, &balance); err == nil {
		t.Fatal("closed normal source still issued state RPC")
	}
	if calls.Load() != before {
		t.Fatal("close allowed network traffic")
	}
}

type runtimeEncodedReceipts [][]byte

func (r runtimeEncodedReceipts) Len() int                           { return len(r) }
func (r runtimeEncodedReceipts) EncodeIndex(i int, w *bytes.Buffer) { w.Write(r[i]) }
func TestRuntimeSourcesCachedFactoryProofUsesNormalChainReceiptRules(t *testing.T) {
	f := catalogFixture(t)
	proof := catalogHTTPProofFixture()
	proof.Receipts[0].Type = "0x7e"
	rawData, _ := hex.DecodeString(proof.Receipts[0].Logs[0].Data[2:])
	topics := []common.Hash{}
	for _, topic := range proof.Receipts[0].Logs[0].Topics {
		topics = append(topics, common.HexToHash(topic))
	}
	log := &types.Log{Address: common.HexToAddress(proof.Receipts[0].Logs[0].Address), Topics: topics, Data: rawData}
	receipt := &types.Receipt{Status: 1, CumulativeGasUsed: 21000, Logs: []*types.Log{log}}
	receipt.Bloom = types.CreateBloom(receipt)
	payload, err := rlp.EncodeToBytes([]any{[]byte{1}, uint64(21000), receipt.Bloom, receipt.Logs})
	if err != nil {
		t.Fatal(err)
	}
	proof.Header.ReceiptsRoot = types.DeriveSha(runtimeEncodedReceipts{append([]byte{0x7e}, payload...)}, trie.NewStackTrie(nil)).Hex()
	if evm.VerifyReceiptRoot(context.Background(), proof.Header, proof.Receipts) == nil {
		t.Fatal("counterexample unexpectedly passed Ethereum-only rules")
	}
	ch := f.options.Chain
	ch.Key = "evm:10"
	client := rpc.New([]string{"http://normal-proof.invalid"}, time.Second, 1<<20)
	client.HTTP.Transport = runtimeTestTransport(func(r *http.Request) (*http.Response, error) {
		raw, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "result": proof.Header})
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(bytes.NewReader(raw)), Request: r}, nil
	})
	c := &CatalogRuntime{options: CatalogRuntimeOptions{Chain: ch, Config: f.options.Config, Archive: f.options.Archive}}
	s := &Service{catalogRuntime: map[string]*CatalogRuntime{ch.Key: c}, adapters: map[string]*evm.Adapter{ch.Key: {Client: client, Chain: ch}}}
	raw, _ := json.Marshal(proof)
	ref, err := f.options.Archive.Put(ingest.Owner{Chain: ch.Key, Height: 7, Hash: proof.Header.Hash, ProviderID: "normal-native-proof"}, raw)
	if err != nil {
		t.Fatal(err)
	}
	block, err := c.projectHTTPProof(proof)
	if err != nil {
		t.Fatal(err)
	}
	block.Raw = ref
	if err = s.verifyRuntimeFactoryProof(context.Background(), ch, block); err != nil {
		t.Fatal("normal cached native proof was rejected", err)
	}
}

func TestRuntimeSourcesStatusMergesRecoveryAndHistoryWithoutQualifyingR02(t *testing.T) {
	s, _, snapshot := runtimeBridgeFixture(t)
	key := snapshot.Reference.Block.ChainKey
	s.constructedAt = time.Now().Add(-time.Second)
	s.scopeTimings = map[string]*scopeRestoreStats{key: {}}
	s.setScopeTarget(key, 32)
	finish := s.startScopeMetrics(key)
	finish(nil)
	s.observeQualifiedScope(snapshot)
	before := s.providerRuntime[key].client.Providers()[0].Attempts
	request := httptest.NewRequest("GET", "/internal/chains/"+key+"/source-evidence", nil)
	request.SetPathValue("chain", key)
	response := httptest.NewRecorder()
	s.runtimeSourceStatus(response, request)
	var status struct {
		Recovery struct {
			Observed  bool   `json:"observed"`
			Successes uint64 `json:"restore_successes"`
			Target    int    `json:"target_scope_pool_count"`
			Pools     int    `json:"last_qualified_pool_count"`
		} `json:"recovery"`
		History     json.RawMessage `json:"history"`
		R02Complete *bool           `json:"r02_complete"`
	}
	if err := json.Unmarshal(response.Body.Bytes(), &status); err != nil {
		t.Fatal(err)
	}
	if !status.Recovery.Observed || status.Recovery.Successes != 1 || status.Recovery.Target != 32 || status.Recovery.Pools != 1 {
		t.Fatal("recovery observations missing from source status", response.Body.String())
	}
	if len(status.History) == 0 || status.R02Complete == nil || *status.R02Complete {
		t.Fatal("history or live qualification semantics changed", response.Body.String())
	}
	if s.providerRuntime[key].client.Providers()[0].Attempts != before {
		t.Fatal("status handler performed network work")
	}
}
