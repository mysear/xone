package service

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

func runtimeHistoryBatch(n uint64) domain.FactBatch {
	return domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: domain.BlockRef{ChainKey: "evm:31337", Number: n, Hash: "0x" + strings.Repeat("aa", 32), ParentHash: "0x" + strings.Repeat("aa", 32), TimestampMS: 1}}}}
}
func runtimeHistoryQueue(t *testing.T) *reducer.HistoryQueue {
	t.Helper()
	s, e := reducer.Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { s.Close() })
	q, e := s.OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("ed", 32)})
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 1000000}}); e != nil {
		t.Fatal(e)
	}
	return q
}
func TestHistoryRuntimeShareAndOneStep(t *testing.T) {
	q := runtimeHistoryQueue(t)
	var calls atomic.Int32
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .25}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		calls.Add(1)
		time.Sleep(10 * time.Millisecond)
		return runtimeHistoryBatch(n), nil, nil
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	worked, e := r.Tick(context.Background())
	if e != nil || !worked {
		t.Fatal(worked, e)
	}
	status, e := r.SourceStatus(context.Background())
	if e != nil || status.MaxShare != .15 || status.Queue.FactBlocks != 1 || status.Cooldown <= 0 {
		t.Fatal(status, e)
	}
	if worked, e = r.Tick(context.Background()); e != nil || worked || calls.Load() != 1 {
		t.Fatal("share not enforced", worked, e)
	}
	if status.Cooldown < time.Duration(float64(status.LastWork)*(1/.15-1)) {
		t.Fatal("cooldown smaller than measured work share", status)
	}
}
func TestHistoryRuntimePauseCancelRetryDoesNotChangeOnline(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	store := s.stores["evm:31337"]
	before, ok := store.Current()
	if !ok {
		t.Fatal("online not ready")
	}
	q, e := store.OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("ed", 32)})
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 30}}); e != nil {
		t.Fatal(e)
	}
	entered := make(chan struct{})
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		close(entered)
		<-ctx.Done()
		return runtimeHistoryBatch(n), nil, ctx.Err()
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	done := make(chan error, 1)
	go func() { _, e := r.Tick(context.Background()); done <- e }()
	<-entered
	r.Pause(true)
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("pause did not cancel source")
	}
	status, e := r.SourceStatus(context.Background())
	if e != nil || !status.Paused || status.Queue.Jobs[0].Attempts != 1 || status.Queue.FactBlocks != 0 {
		t.Fatal(status, e)
	}
	after, ready := store.Current()
	if !ready || before.Reference != after.Reference {
		t.Fatal("history affected online readiness")
	}
}
func TestHistoryRuntimeUnavailablePersistsAndClose(t *testing.T) {
	q := runtimeHistoryQueue(t)
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(context.Context, uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		return domain.FactBatch{}, nil, errors.New("unavailable")
	}})
	if e != nil {
		t.Fatal(e)
	}
	if ok, e := r.Tick(context.Background()); !ok || e == nil {
		t.Fatal(ok, e)
	}
	st, e := r.SourceStatus(context.Background())
	if e != nil || st.Queue.Jobs[0].LastError != "HISTORY_SOURCE_RETRY" || st.Queue.Jobs[0].RetryAt.IsZero() {
		t.Fatal(st, e)
	}
	if e = r.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	r.Close()
	r.Close()
	if _, e = r.Tick(context.Background()); !errors.Is(e, ErrHistoryClosed) {
		t.Fatal(e)
	}
}

func TestHistoryRuntimeNormalSourceIsolatedAndIdentityChecked(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": "0x1"})
	}))
	defer server.Close()
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.History = config.History{Enabled: true, Namespace: "history", MaxShare: .15}
	cfg.Config.Chains[0].RPCURLs = []string{server.URL}
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	ch := cfg.Config.Chains[0]
	online := rpc.New([]string{server.URL}, time.Second, 1<<20)
	var onlineRecorded atomic.Int32
	online.SetRecorder(func(context.Context, string, string, json.RawMessage, []byte) error {
		onlineRecorded.Add(1)
		return nil
	})
	s.adapters[ch.Key] = &evm.Adapter{Client: online, Chain: ch}
	history, e := s.NewChainHistoryRuntime(ch.Key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	defer history.Close()
	if e = history.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 2}}); e != nil {
		t.Fatal(e)
	}
	if ok, e := history.Tick(context.Background()); !ok || e == nil {
		t.Fatal("wrong chain accepted", ok, e)
	}
	if calls.Load() == 0 || onlineRecorded.Load() != 0 {
		t.Fatal("online client recorder was used")
	}
	var out string
	if e = online.Call(context.Background(), "eth_chainId", []any{}, &out); e != nil || onlineRecorded.Load() != 1 {
		t.Fatal("online recorder overwritten", e)
	}
	status, e := history.SourceStatus(context.Background())
	if e != nil || status.Queue.FactBlocks != 0 || status.Queue.Jobs[0].Attempts != 1 {
		t.Fatal(status, e)
	}
}

func TestHistoryRuntimeSharedSchedulerPreventsOverlappingSources(t *testing.T) {
	scheduler := &HistoryScheduler{}
	entered := make(chan struct{})
	release := make(chan struct{})
	first, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: runtimeHistoryQueue(t), Scheduler: scheduler, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		close(entered)
		select {
		case <-release:
			return runtimeHistoryBatch(n), nil, nil
		case <-ctx.Done():
			return domain.FactBatch{}, nil, ctx.Err()
		}
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer first.Close()
	var secondCalls atomic.Int32
	second, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: runtimeHistoryQueue(t), Scheduler: scheduler, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		secondCalls.Add(1)
		return runtimeHistoryBatch(n), nil, nil
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer second.Close()
	done := make(chan error, 1)
	go func() { _, e := first.Tick(context.Background()); done <- e }()
	<-entered
	if worked, e := second.Tick(context.Background()); e != nil || worked || secondCalls.Load() != 0 {
		t.Fatal("parallel history source", worked, e)
	}
	close(release)
	if e = <-done; e != nil {
		t.Fatal(e)
	}
}
func TestHistoryRuntimeTimeoutZeroShareAndCapacity(t *testing.T) {
	q := runtimeHistoryQueue(t)
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Config: config.History{Enabled: true, Namespace: "history", MaxShare: 0}})
	if e != nil {
		t.Fatal(e)
	}
	if worked, e := r.Tick(context.Background()); e != nil || worked {
		t.Fatal(worked, e)
	}
	r.Close()
	r, e = NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Timeout: 10 * time.Millisecond, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		<-ctx.Done()
		return domain.FactBatch{}, nil, ctx.Err()
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	if _, e = r.Tick(context.Background()); !errors.Is(e, context.DeadlineExceeded) {
		t.Fatal(e)
	}
	st, _ := r.SourceStatus(context.Background())
	if st.Queue.Jobs[0].Attempts != 1 || st.Queue.Jobs[0].Token != "" || st.Queue.FactBlocks != 0 {
		t.Fatal(st)
	}
}
func TestHistoryRuntimeNormalPublicSourceSuccessBudgetAndPrivateStateForbidden(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.History = config.History{Enabled: true, Namespace: "history", MaxShare: .15}
	ch := cfg.Config.Chains[0]
	var forbidden atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result any
		switch req.Method {
		case "eth_chainId":
			result = "0x7a69"
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(req.Params[0], &tag)
			if tag == "0x0" {
				result = map[string]any{"hash": ch.Genesis}
			} else {
				result = map[string]any{"number": tag, "hash": "0x" + strings.Repeat("aa", 32), "parentHash": "0x" + strings.Repeat("aa", 32), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
			}
		default:
			forbidden.Add(1)
			w.WriteHeader(400)
			return
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	cfg.Config.Chains[0].RPCURLs = []string{server.URL}
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	history, e := s.NewChainHistoryRuntime(ch.Key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	defer history.Close()
	if e = history.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 2}}); e != nil {
		t.Fatal(e)
	}
	if worked, e := history.Tick(context.Background()); e != nil || !worked {
		t.Fatal(worked, e)
	}
	status, e := history.SourceStatus(context.Background())
	if e != nil || status.Queue.FactBlocks != 1 || forbidden.Load() != 0 {
		t.Fatal(status, e)
	}
	// The actual independent source is also callable under a caller-owned deadline.
	// Exhaust its aggregate response budget while every response still fits RPC's
	// individual body cap, then check it never delivers an incomplete FactBatch.
	source := &historyPublicSource{adapter: evm.Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: cfg.Config.Chains[0]}, recorder: newRawRecorder(ch.Key, historyArchive(t)), maxBytes: 256}
	source.adapter.Client.SetRecorder(source.record)
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	batch, _, e := source.read(ctx, 2)
	if e == nil || len(batch.Blocks) != 0 {
		t.Fatal("aggregate budget ignored", batch, e)
	}
}
func historyArchive(t *testing.T) *ingest.Archive {
	t.Helper()
	a, e := ingest.OpenArchive(t.TempDir(), ingest.ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 64, MaxRecordBytes: 1 << 16})
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { a.Close() })
	return a
}

func TestHistoryRuntimePausedBeforeOnlineFirstScope(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q, e := s.stores["evm:31337"].OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("ed", 32)})
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 1000000000}}); e != nil {
		t.Fatal(e)
	}
	var called atomic.Int32
	history, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(context.Context, uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		called.Add(1)
		return domain.FactBatch{}, nil, errors.New("unavailable")
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer history.Close()
	history.Pause(true)
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if e = history.Start(ctx); e != nil {
		t.Fatal(e)
	}
	if e = s.Start(ctx); e != nil {
		t.Fatal(e)
	}
	if _, ready := s.stores["evm:31337"].Current(); !ready || called.Load() != 0 {
		t.Fatal("paused history blocked online first scope")
	}
}
func TestHistoryRuntimeCapacityIsPersistentRetry(t *testing.T) {
	store, e := reducer.Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	q, e := store.OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("ed", 32), MaxBlockBytes: 32})
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		return runtimeHistoryBatch(n), nil, nil
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	if _, e = r.Tick(context.Background()); !errors.Is(e, reducer.ErrHistoryCapacity) {
		t.Fatal(e)
	}
	st, e := r.SourceStatus(context.Background())
	if e != nil || st.Queue.Jobs[0].Attempts != 1 || st.Queue.Jobs[0].LastError != "HISTORY_CAPACITY_RETRY" || st.Queue.FactBlocks != 0 {
		t.Fatal(st, e)
	}
}

func TestHistoryReviewNormalFactoryScopeReopenInvalidatesEmptyFacts(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Chains[0].Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("aa", 20), Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("11", 20), Token1: "0x" + strings.Repeat("22", 20), FeeNumerator: 3, FeeDenominator: 1000}}
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	key := cfg.Config.Chains[0].Key
	h, e := s.NewChainHistoryRuntime(key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	ctx := context.Background()
	if e = h.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	c, e := h.options.Queue.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = h.options.Queue.Complete(ctx, c, runtimeHistoryBatch(1), nil); e != nil {
		t.Fatal(e)
	}
	h.Close()
	s.Close()
	added := config.PoolConfig{ID: "0x" + strings.Repeat("aa", 20), Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("11", 20), Token1: "0x" + strings.Repeat("22", 20), FeeNumerator: 3, FeeDenominator: 1000}
	added.ID = "0x" + strings.Repeat("bb", 20)
	cfg.Config.Chains[0].Pools = append(cfg.Config.Chains[0].Pools, added)
	s, e = NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	h, e = s.NewChainHistoryRuntime(key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	defer h.Close()
	st, e := h.SourceStatus(ctx)
	if e != nil || st.Queue.Complete || len(st.Queue.Covered) > 0 || st.Queue.Jobs[0].Next != 1 {
		t.Fatal("A-only empty result still complete after adding B", st, e)
	}
}
func TestHistoryReviewCompletedStepReleasesOnlyItsPins(t *testing.T) {
	q := runtimeHistoryQueue(t)
	a := historyArchive(t)
	recorder := newRawRecorder("evm:31337", a)
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, RawArchive: a, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		if e := recorder.capture(ctx, "history-provider", "eth_getBlockByNumber", json.RawMessage(`[]`), []byte(`{"result":"block"}`)); e != nil {
			return domain.FactBatch{}, nil, e
		}
		return runtimeHistoryBatch(n), nil, nil
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	if _, e = r.Tick(context.Background()); e != nil {
		t.Fatal(e)
	}
	records, e := a.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	for _, rec := range records {
		if rec.ActiveReferences != 0 {
			t.Fatal("successful history completion leaked HTTP pin", rec.ActiveReferences)
		}
	}
}

func TestHistoryReview32RecordNormalSourceSixStepsHaveNoActivePins(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.History = config.History{Enabled: true, Namespace: "history", MaxShare: .15}
	cfg.Config.Journal.RawEvidence = &config.RawEvidenceConfig{MaxBytes: 64 << 20, MaxRecords: 32, MaxRecordBytes: int64(cfg.Config.Journal.MaxBytes)*4 + 16384}
	ch := cfg.Config.Chains[0]
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result any
		switch req.Method {
		case "eth_chainId":
			result = "0x7a69"
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(req.Params[0], &tag)
			if tag == "0x0" {
				result = map[string]any{"hash": ch.Genesis}
			} else {
				result = map[string]any{"number": tag, "hash": "0x" + strings.Repeat("aa", 32), "parentHash": "0x" + strings.Repeat("aa", 32), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
			}
		default:
			t.Error("private RPC", req.Method)
			w.WriteHeader(400)
			return
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	cfg.Config.Chains[0].RPCURLs = []string{server.URL}
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	h, e := s.NewChainHistoryRuntime(ch.Key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	defer h.Close()
	if e = h.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 6}}); e != nil {
		t.Fatal(e)
	}
	for n := 1; n <= 6; n++ {
		h.scheduler.mu.Lock()
		h.scheduler.next = time.Time{}
		h.scheduler.mu.Unlock()
		if worked, e := h.Tick(context.Background()); e != nil || !worked {
			t.Fatalf("block %d: worked=%v err=%v", n, worked, e)
		}
		records, e := h.HistoryArchive().RawRecords()
		if e != nil {
			t.Fatal(e)
		}
		for _, r := range records {
			if r.ActiveReferences != 0 {
				t.Fatalf("block %d leaked %d active pins", n, r.ActiveReferences)
			}
		}
	}
	st, e := h.SourceStatus(context.Background())
	if e != nil || st.Queue.FactBlocks != 6 || !st.Queue.Complete {
		t.Fatal(st, e)
	}
}

func TestHistoryReviewReopenRecoversOnlyProvenRawGroups(t *testing.T) {
	dir := t.TempDir()
	rawDir := t.TempDir()
	ctx := context.Background()
	store, e := reducer.Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	queue, e := store.OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("ed", 32)})
	if e != nil {
		t.Fatal(e)
	}
	if e = queue.Enqueue(ctx, []domain.Gap{{From: 1, To: 2}}); e != nil {
		t.Fatal(e)
	}
	limits := ingest.ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 32, MaxRecordBytes: 1 << 16}
	a, e := ingest.OpenArchive(rawDir, limits)
	if e != nil {
		t.Fatal(e)
	}
	committed, e := a.PutPinned(ingest.Owner{Chain: "evm:31337", ProviderID: "public", Hash: "committed"}, []byte(`{"result":1}`), "committed-step")
	if e != nil {
		t.Fatal(e)
	}
	failed, e := a.PutPinned(ingest.Owner{Chain: "evm:31337", ProviderID: "public", Hash: "failed"}, []byte(`{"result":2}`), "failed-step")
	if e != nil {
		t.Fatal(e)
	}
	c, e := queue.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	group, _ := domain.Hash("committed-group")
	if e = queue.Complete(ctx, c, runtimeHistoryBatch(1), nil, reducer.HistoryRawGroup{ID: group, Pins: []reducer.HistoryRawPin{{Raw: committed, Reference: "committed-step"}}}); e != nil {
		t.Fatal(e)
	}
	// Crash/unknown owner outcome between durable receipt and releasing raw pins.
	store.Close()
	a.Close()
	store, e = reducer.Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer store.Close()
	queue, e = store.OpenHistoryQueue(reducer.HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("ed", 32)})
	if e != nil {
		t.Fatal(e)
	}
	a, e = ingest.OpenArchive(rawDir, limits)
	if e != nil {
		t.Fatal(e)
	}
	defer a.Close()
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: queue, RawArchive: a, Config: config.History{Enabled: false, Namespace: "history"}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	if _, e = r.RecoverRawPins(ctx, 16); e != nil {
		t.Fatal(e)
	}
	if _, e = r.RecoverRawPins(ctx, 16); e != nil {
		t.Fatal("release must be idempotent", e)
	}
	records, e := a.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	for _, v := range records {
		if v.Raw == committed && v.ActiveReferences != 0 {
			t.Fatal("committed pin held", v)
		}
		if v.Raw == failed && v.ActiveReferences != 1 {
			t.Fatal("unproven failed pin discarded", v)
		}
	}
}
func TestHistoryReviewUpdateScopeCancelsOldSourceWithoutOwnerNetworkLock(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.History = config.History{Enabled: true, Namespace: "history", MaxShare: .15}
	entered := make(chan struct{})
	releaseServer := make(chan struct{})
	var once sync.Once
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		once.Do(func() { close(entered) })
		select {
		case <-r.Context().Done():
		case <-releaseServer:
		}
	}))
	defer server.Close()
	defer close(releaseServer)
	cfg.Config.Chains[0].RPCURLs = []string{server.URL}
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	ch := cfg.Config.Chains[0]
	h, e := s.NewChainHistoryRuntime(ch.Key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	defer h.Close()
	ctx := context.Background()
	if e = h.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	old, _ := h.SourceStatus(ctx)
	done := make(chan error, 1)
	go func() { _, e := h.Tick(ctx); done <- e }()
	<-entered
	scope := ch
	scope.Pools = append([]config.PoolConfig(nil), ch.Pools...)
	scope.Pools = append(scope.Pools, config.PoolConfig{ID: "new-dynamic-pool", Protocol: "uniswap-v2", Token0: "A", Token1: "B"})
	expected := s.stores[ch.Key].DurableRef()
	updateCtx, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	if e = h.UpdateScope(updateCtx, scope, expected); e != nil {
		t.Fatal(e)
	}
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("scope update did not cancel old history source")
	}
	next, e := h.SourceStatus(ctx)
	if e != nil || old.Queue.DatasetID == next.Queue.DatasetID || next.Queue.Complete || len(next.Queue.Covered) != 0 {
		t.Fatal(next, e)
	}
	// Caller mutation after UpdateScope cannot mutate its frozen decoder scope.
	scope.Pools[len(scope.Pools)-1].Token0 = "tampered"
	nextAgain, _ := h.SourceStatus(ctx)
	if nextAgain.Queue.DatasetID != next.Queue.DatasetID {
		t.Fatal("mutable scope alias")
	}
}

func TestHistoryReviewDisabledNormalScopeCanBeUpdatedWithoutRPC(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.History.Enabled = false
	cfg.Config.History.MaxShare = 0
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	ch := cfg.Config.Chains[0]
	h, e := s.NewChainHistoryRuntime(ch.Key, &HistoryScheduler{})
	if e != nil {
		t.Fatal(e)
	}
	defer h.Close()
	old, _ := h.SourceStatus(context.Background())
	ch.Pools = append(ch.Pools, config.PoolConfig{ID: "new-admitted-pool", Protocol: "uniswap-v2", Token0: "A", Token1: "B"})
	if e = h.UpdateScope(context.Background(), ch, s.stores[ch.Key].DurableRef()); e != nil {
		t.Fatal(e)
	}
	next, _ := h.SourceStatus(context.Background())
	if next.Queue.DatasetID == old.Queue.DatasetID || h.HistoryArchive() != nil {
		t.Fatal("disabled scope refresh opened RPC/archive or retained old scope")
	}
}
func TestHistoryReviewCloseReconcilesProvenPinsButNotFailedPins(t *testing.T) {
	q := runtimeHistoryQueue(t)
	a := historyArchive(t)
	ctx := context.Background()
	ref, e := a.PutPinned(ingest.Owner{Chain: q.Chain(), ProviderID: "public", Hash: "close-proof"}, []byte(`{"result":"raw"}`), "proven-close")
	if e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	id, _ := domain.Hash("close-group")
	if e = q.Complete(ctx, c, runtimeHistoryBatch(c.Height), nil, reducer.HistoryRawGroup{ID: id, Pins: []reducer.HistoryRawPin{{Raw: ref, Reference: "proven-close"}}}); e != nil {
		t.Fatal(e)
	}
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, RawArchive: a, Config: config.History{Namespace: "history"}})
	if e != nil {
		t.Fatal(e)
	}
	r.Close()
	records, e := a.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	if records[0].ActiveReferences != 0 {
		t.Fatal("Close left proven completed pins held", records[0])
	}
}
func TestHistoryReviewCaptureContextDoesNotReleaseOuterOnlinePins(t *testing.T) {
	q := runtimeHistoryQueue(t)
	a := historyArchive(t)
	online, e := a.PutPinned(ingest.Owner{Chain: q.Chain(), ProviderID: "online", Hash: "online-pending"}, []byte(`{"result":"online"}`), "online-uncommitted")
	if e != nil {
		t.Fatal(e)
	}
	outer := &rawCapture{ID: "outer-online-capture", Chain: q.Chain(), Max: 1, Pins: []RawPin{{Raw: online, Reference: "online-uncommitted"}}}
	recorder := newRawRecorder(q.Chain(), a)
	r, e := NewHistoryRuntime(HistoryRuntimeOptions{Queue: q, RawArchive: a, Config: config.History{Enabled: true, Namespace: "history", MaxShare: .15}, ReadFacts: func(ctx context.Context, n uint64) (domain.FactBatch, []reducer.HistoryCandidate, error) {
		if e := recorder.capture(ctx, "history", "eth_getBlockByNumber", json.RawMessage(`[]`), []byte(`{"result":"history"}`)); e != nil {
			return domain.FactBatch{}, nil, e
		}
		return runtimeHistoryBatch(n), nil, nil
	}})
	if e != nil {
		t.Fatal(e)
	}
	defer r.Close()
	if _, e = r.Tick(context.WithValue(context.Background(), rawCaptureKey{}, outer)); e != nil {
		t.Fatal(e)
	}
	if len(outer.Pins) != 1 {
		t.Fatal("history modified outer capture")
	}
	records, _ := a.RawRecords()
	for _, v := range records {
		if v.Raw == online && v.ActiveReferences != 1 {
			t.Fatal("history released uncommitted online pin", v)
		}
		if v.Raw != online && v.ActiveReferences != 0 {
			t.Fatal("history pin leaked", v)
		}
	}
}

func TestHistoryReviewScopedRebuildPreservesExactDataset(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	scope := cfg.Config.Chains[0]
	scope.Pools = append([]config.PoolConfig(nil), scope.Pools...)
	scope.Pools = append(scope.Pools, config.PoolConfig{ID: "dynamic-B", Protocol: "uniswap-v2", Token0: "A", Token1: "B"})
	expected := s.stores[scope.Key].DurableRef()
	h, e := s.NewChainHistoryRuntimeForScope(scope.Key, &HistoryScheduler{}, scope, expected)
	if e != nil {
		t.Fatal(e)
	}
	ctx := context.Background()
	if e = h.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	c, e := h.options.Queue.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = h.options.Queue.Complete(ctx, c, runtimeHistoryBatch(1), nil); e != nil {
		t.Fatal(e)
	}
	before, _ := h.SourceStatus(ctx)
	h.Close()
	h, e = s.NewChainHistoryRuntimeForScope(scope.Key, &HistoryScheduler{}, scope, expected)
	if e != nil {
		t.Fatal(e)
	}
	defer h.Close()
	after, e := h.SourceStatus(ctx)
	if e != nil || !after.Queue.Complete || after.Queue.DatasetID != before.Queue.DatasetID || after.Queue.FactBlocks != 1 {
		t.Fatal("scoped rebuild temporarily rebound root config", after, e)
	}
}
