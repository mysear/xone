package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"math/big"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

func TestNormalBaselineRecoveryCommitsScopeLedgerWithOwnerAndRestartReanchors(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	base := testSnapshotSource(t)
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		s, e := base(ctx, ch)
		s.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		return s, e
	}
	first, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	if e = first.Start(context.Background()); e != nil {
		first.Close()
		t.Fatal(e)
	}
	key := cfg.Config.Chains[0].Key
	current, ready := first.stores[key].Current()
	if !ready {
		first.Close()
		t.Fatal("baseline missing")
	}
	ledger, e := first.stores[key].ScopeState()
	if e != nil || len(ledger) == 0 {
		first.Close()
		t.Fatal("normal baseline bypassed embedded scope owner transaction", e)
	}
	var envelope struct {
		Payload string `json:"payload"`
	}
	if json.Unmarshal(ledger, &envelope) != nil {
		first.Close()
		t.Fatal("scope ledger corrupt")
	}
	var state struct {
		Jobs map[string]struct {
			Job struct {
				ID    string `json:"id"`
				Phase string `json:"phase"`
			} `json:"job"`
		} `json:"jobs"`
	}
	if json.Unmarshal([]byte(envelope.Payload), &state) != nil || len(state.Jobs) != 1 {
		first.Close()
		t.Fatal("missing scope job", envelope.Payload)
	}
	for id, entry := range state.Jobs {
		if entry.Job.Phase != "SCOPED_READY" {
			first.Close()
			t.Fatal("scope not committed", entry.Job.Phase)
		}
		proof, e := first.stores[key].ScopeCommit(id)
		if e != nil || proof.Reference != current.Reference {
			first.Close()
			t.Fatal("ledger and online state not bound", e, proof)
		}
	}
	old := current.Reference
	first.Close()
	second, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer second.Close()
	if _, ready = second.stores[key].Current(); ready {
		t.Fatal("restart restored old quote readiness")
	}
	if e = second.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	after, ready := second.stores[key].Current()
	if !ready || after.Reference.OnlineGeneration <= old.OnlineGeneration || after.Reference.JournalSeq <= old.JournalSeq {
		t.Fatal("restart did not publish a fresh qualified scope", after.Reference)
	}
}

func TestNormalStartupRestoresBoundedFirstScopeAndEnsureExpandsAdmittedPools(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	original, e := testSnapshotSource(t)(context.Background(), cfg.Config.Chains[0])
	if e != nil {
		t.Fatal(e)
	}
	prototype := original.Pools[0]
	for i := 0; i < 70; i++ {
		cfg.Config.Chains[0].Pools = append(cfg.Config.Chains[0].Pools, config.PoolConfig{ID: fmt.Sprintf("0x%040x", i+3), Protocol: "uniswap-v2", Token0: prototype.Token0, Token1: prototype.Token1, FeeNumerator: 3, FeeDenominator: 1000})
	}
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		snapshot := original
		snapshot.Pools = nil
		snapshot.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		for _, p := range ch.Pools {
			pool := prototype.Clone()
			pool.PoolID = p.ID
			snapshot.Pools = append(snapshot.Pools, pool)
		}
		snapshot.Reference.ScopeHash, _ = domain.ScopeHash(snapshot.Pools)
		return snapshot, ctx.Err()
	}
	service, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer service.Close()
	if e = service.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	key := cfg.Config.Chains[0].Key
	before, ready := service.stores[key].Current()
	if !ready || len(before.Pools) != 32 {
		t.Fatalf("startup did not qualify a bounded first scope: pools=%d ready=%v", len(before.Pools), ready)
	}
	history := service.historyRuntime[key]
	history.Pause(true)
	historyStatus, err := history.SourceStatus(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	expectedDataset, err := evm.HistoryDatasetID(cfg.Config.Chains[0])
	if err != nil || historyStatus.Queue.DatasetID != expectedDataset || historyStatus.Queue.ScopeStale {
		t.Fatal("online32 narrowed independently configured history70", historyStatus.Queue, err)
	}
	requested := cfg.Config.Chains[0].Pools[69].ID
	body, _ := json.Marshal(map[string]any{"pool_ids": []string{requested}, "expected_online_generation": before.Reference.OnlineGeneration})
	response := httptest.NewRecorder()
	service.Handler().ServeHTTP(response, httptest.NewRequest("POST", "/internal/chains/"+key+"/ensure-pool-scope", bytes.NewReader(body)))
	if response.Code != 200 {
		t.Fatal("admitted scope expansion failed", response.Code, response.Body.String())
	}
	after, ready := service.stores[key].Current()
	if !ready || after.Reference.OnlineGeneration <= before.Reference.OnlineGeneration || len(after.Pools) != 33 {
		t.Fatal("new scope not atomically published", after.Reference, len(after.Pools))
	}
	found := false
	for _, pool := range after.Pools {
		found = found || pool.PoolID == requested
	}
	if !found {
		t.Fatal("requested admitted pool missing")
	}
}

func TestScopeOwnerProofRejectsNewGenerationEvenWhenOldLedgerStillExists(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		snapshot, e := testSnapshotSource(t)(ctx, ch)
		snapshot.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		return snapshot, e
	}
	service, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer service.Close()
	if e = service.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	key := cfg.Config.Chains[0].Key
	engine := service.recoveries[key]
	ledger, e := service.stores[key].ScopeState()
	if e != nil {
		t.Fatal(e)
	}
	var envelope struct {
		Payload string `json:"payload"`
	}
	_ = json.Unmarshal(ledger, &envelope)
	var disk struct {
		Jobs map[string]any `json:"jobs"`
	}
	_ = json.Unmarshal([]byte(envelope.Payload), &disk)
	var job string
	for id := range disk.Jobs {
		job = id
	}
	newer, ok := service.stores[key].Current()
	if !ok {
		t.Fatal("no current scope")
	}
	newer.Reference.OnlineGeneration++
	newer.Reference.WriterEpoch++
	newer.Reference.JournalSeq++
	if e = service.stores[key].InstallBaseline(newer); e != nil {
		t.Fatal(e)
	}
	if _, e = engine.OnlineCommit(context.Background(), job); e == nil {
		t.Fatal("old owner ledger bypassed actual online generation")
	}
}

func TestFailedExpansionRetainsUnchangedFreshQualifiedScope(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	original, e := testSnapshotSource(t)(context.Background(), cfg.Config.Chains[0])
	if e != nil {
		t.Fatal(e)
	}
	prototype := original.Pools[0]
	for i := 0; i < 33; i++ {
		cfg.Config.Chains[0].Pools = append(cfg.Config.Chains[0].Pools, config.PoolConfig{ID: fmt.Sprintf("0x%040x", i+3), Protocol: "uniswap-v2", Token0: prototype.Token0, Token1: prototype.Token1, FeeNumerator: 3, FeeDenominator: 1000})
	}
	requested := cfg.Config.Chains[0].Pools[32].ID
	calls := 0
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		calls++
		snapshot := original
		snapshot.Pools = nil
		snapshot.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		for _, config := range ch.Pools {
			if config.ID == requested {
				return snapshot, errors.New("NEW_POOL_DEPENDENCY_UNAVAILABLE")
			}
			pool := prototype.Clone()
			pool.PoolID = config.ID
			snapshot.Pools = append(snapshot.Pools, pool)
		}
		snapshot.Reference.ScopeHash, _ = domain.ScopeHash(snapshot.Pools)
		return snapshot, ctx.Err()
	}
	service, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer service.Close()
	if e = service.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	key := cfg.Config.Chains[0].Key
	before, ready := service.stores[key].Current()
	if !ready {
		t.Fatal("no first scope")
	}
	raw, _ := json.Marshal(map[string]any{"pool_ids": []string{requested}, "expected_online_generation": before.Reference.OnlineGeneration})
	response := httptest.NewRecorder()
	service.Handler().ServeHTTP(response, httptest.NewRequest("POST", "/internal/chains/"+key+"/ensure-pool-scope", bytes.NewReader(raw)))
	if response.Code != 503 || calls != 2 {
		t.Fatal("expansion not attempted/failed correctly", response.Code, calls, response.Body.String())
	}
	after, ready := service.stores[key].Current()
	if !ready || after.Reference != before.Reference || len(after.Pools) != 32 {
		t.Fatal("unrelated valid scope revoked by new pool failure", ready, after.Reference)
	}
}

func TestHeadFirstRecoveryWorkDoesNotScaleWithOneHourOneDayOrThirtyDaysGap(t *testing.T) {
	for _, hours := range []uint64{1, 24, 720} {
		t.Run(fmt.Sprintf("offline-%dh", hours), func(t *testing.T) {
			cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
			if e != nil {
				t.Fatal(e)
			}
			cfg.Config.State.Path = t.TempDir()
			cfg.Config.Startup.MaxAge = 120000
			cfg.Config.History.Enabled = false
			initial, e := testSnapshotSource(t)(context.Background(), cfg.Config.Chains[0])
			if e != nil {
				t.Fatal(e)
			}
			// Controlled persistent old state models elapsed downtime without sleeping.
			initial.Reference.Block.TimestampMS = uint64(time.Now().Add(-time.Duration(hours) * time.Hour).UnixMilli())
			path := filepath.Join(cfg.Config.State.Path, "evm-31337")
			old, e := reducer.Open(path, "chain-parser-go", 1, 128)
			if e != nil {
				t.Fatal(e)
			}
			if e = old.InstallBaseline(initial); e != nil {
				old.Close()
				t.Fatal(e)
			}
			old.Close()
			calls := 0
			source := func(ctx context.Context, _ config.Chain) (domain.Snapshot, error) {
				calls++
				next := initial
				next.Reference.Block.Number += hours * 300
				next.Reference.Block.Hash = fmt.Sprintf("0x%064x", next.Reference.Block.Number)
				next.Reference.Block.ParentHash = fmt.Sprintf("0x%064x", next.Reference.Block.Number-1)
				next.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
				return next, ctx.Err()
			}
			service, e := NewWithSource(cfg, source)
			if e != nil {
				t.Fatal(e)
			}
			defer service.Close()
			if _, ready := service.stores[cfg.Config.Chains[0].Key].Current(); ready {
				t.Fatal("old stored state auto-ready")
			}
			started := time.Now()
			if e = service.Start(context.Background()); e != nil {
				t.Fatal(e)
			}
			current, ready := service.stores[cfg.Config.Chains[0].Key].Current()
			if !ready || calls != 1 || current.Reference.OnlineGeneration <= initial.Reference.OnlineGeneration {
				t.Fatal("recovery replayed old blocks or reused generation", ready, calls, current.Reference)
			}
			if len(current.HistoryGaps) == 0 {
				t.Fatal("history gap hidden")
			}
			t.Logf("controlled offline=%dh current-source-reads=%d first-qualified=%s pools=%d history-wait=false", hours, calls, time.Since(started), len(current.Pools))
		})
	}
}
func TestScopeRecoveryCapacityFailureNeverPublishesPartialReady(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Journal.MaxBytes = 1024
	cfg.Config.History.Enabled = false
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		snapshot, e := testSnapshotSource(t)(ctx, ch)
		snapshot.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		prototype := snapshot.Pools[0]
		snapshot.Pools = nil
		for index := 0; index < 16; index++ {
			pool := prototype.Clone()
			pool.PoolID = fmt.Sprintf("capacity-pool-%02d", index)
			snapshot.Pools = append(snapshot.Pools, pool)
		}
		snapshot.Reference.ScopeHash, _ = domain.ScopeHash(snapshot.Pools)
		return snapshot, e
	}
	service, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer service.Close()
	if e = service.Start(context.Background()); e == nil {
		t.Fatal("over-budget scope published")
	}
	if _, ready := service.stores[cfg.Config.Chains[0].Key].Current(); ready {
		t.Fatal("partial scope ready after budget failure")
	}
	if service.stores[cfg.Config.Chains[0].Key].DurableRef().OnlineGeneration != 0 {
		t.Fatal("partial generation committed")
	}
}

func TestCurrentScopeAdapterPreservesCompleteObserver(t *testing.T) {
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	s, err := NewWithSource(cfg, testSnapshotSource(t))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	ch := cfg.Config.Chains[0]
	ch.Pools = []config.PoolConfig{{ID: "observer-pool"}}
	base := &evm.Adapter{Chain: ch}
	calls := 0
	base.OnComplete = func(context.Context, evm.RawBlock, []evm.Receipt) error { calls++; return nil }
	s.adapters[ch.Key] = base
	current := domain.Snapshot{Pools: []domain.Pool{{PoolID: ch.Pools[0].ID}}}
	copy := s.adapterForCurrentScope(ch, current)
	if copy.OnComplete == nil {
		t.Fatal("current-scope adapter lost verified-block completion observer")
	}
	if err = copy.OnComplete(context.Background(), evm.RawBlock{}, nil); err != nil || calls != 1 {
		t.Fatal("current-scope completion observer not invoked", calls, err)
	}
}

type scopeFinalAuthority struct {
	catchupAuthority
	checks int
}

func (a *scopeFinalAuthority) Check(context.Context, control.Lease, uint64) error {
	a.checks++
	return nil
}
func TestScopeOwnerFinalGateUsesLocalAuthorityOnly(t *testing.T) {
	authority := &scopeFinalAuthority{}
	fence := bootstrap.Fence{ChainKey: "evm:1", WriterEpoch: 2, Generation: 3, CanonicalEpoch: 4}
	lease := control.Lease{Chain: fence.ChainKey, WriterEpoch: fence.WriterEpoch, Generation: fence.Generation, CanonicalEpoch: fence.CanonicalEpoch, ExpiresMS: uint64(time.Now().Add(time.Minute).UnixMilli())}
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	backend := &ownerScopeBackend{fence: fence, session: &writerSession{pg: authority, lease: lease, cfg: cfg}}
	if err = backend.authorityGate(nil)(); err != nil {
		t.Fatal(err)
	}
	if authority.checks != 0 {
		t.Fatal("owner final gate performed network authority check under owner mutex", authority.checks)
	}
	backend.session.lease.ExpiresMS = uint64(time.Now().Add(-time.Second).UnixMilli())
	if err = backend.authorityGate(nil)(); !errors.Is(err, bootstrap.ErrFenceChanged) {
		t.Fatal("expired local authority admitted", err)
	}
}

// Exercise the normal RPC adapters and two-worker pipeline before switching
// the same providers to a complete three-block replacement branch.
func TestNormalRPCReorgCommitsAllRawInputsWithOwnerAndRefreshesHistory(t *testing.T) {
	now := uint64(time.Now().Unix())
	makeHeader := func(height uint64, parent common.Hash, state uint64) *types.Header {
		return &types.Header{Number: new(big.Int).SetUint64(height), ParentHash: parent, UncleHash: types.EmptyUncleHash, Root: common.BigToHash(new(big.Int).SetUint64(state)), TxHash: types.EmptyTxsHash, ReceiptHash: types.EmptyReceiptsHash, Difficulty: big.NewInt(0), GasLimit: 30000000, Time: now}
	}
	genesis := makeHeader(0, common.Hash{}, 1)
	base := makeHeader(100, common.HexToHash("0x99"), 1)
	old := map[uint64]*types.Header{100: base}
	replacement := map[uint64]*types.Header{100: base}
	for height := uint64(101); height <= 103; height++ {
		replacement[height] = makeHeader(height, replacement[height-1].Hash(), 2)
		if height <= 102 {
			old[height] = makeHeader(height, old[height-1].Hash(), 1)
		}
	}
	var head atomic.Uint64
	var fork atomic.Bool
	head.Store(100)
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var request struct {
			ID     json.RawMessage
			Method string
			Params []json.RawMessage
		}
		if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
			t.Error(err)
			return
		}
		var result any
		switch request.Method {
		case "eth_chainId":
			result = "0x1"
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(request.Params[0], &tag)
			height := head.Load()
			if strings.HasPrefix(tag, "0x") {
				height, _ = strconv.ParseUint(tag[2:], 16, 64)
			}
			selected := old
			if fork.Load() {
				selected = replacement
			}
			block := selected[height]
			if height == 0 {
				block = genesis
			}
			if block == nil {
				t.Errorf("unexpected header height %d", height)
				return
			}
			encoded, _ := json.Marshal(block)
			var fields map[string]any
			_ = json.Unmarshal(encoded, &fields)
			fields["transactions"] = []any{}
			result = fields
		case "eth_getCode":
			result = "0x6000"
		case "eth_call":
			var call struct{ Data string }
			_ = json.Unmarshal(request.Params[0], &call)
			switch call.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("1", 40)
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("2", 40)
			default:
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			}
		default:
			t.Errorf("unexpected RPC %s", request.Method)
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": request.ID, "result": result})
	})
	first := httptest.NewServer(handler)
	defer first.Close()
	second := httptest.NewServer(handler)
	defer second.Close()
	s, maintenance, key, _ := maintenanceFixtureWithChain(t, func(chain *config.Chain, _ *rawMaintenanceRPCState) {
		chain.Genesis = genesis.Hash().Hex()
		chain.RPCURLs = []string{first.URL, second.URL}
	})
	// This scenario captures dozens of durable RPC inputs across head/ancestor reads,
	// three complete replacement reads and both endpoint confirmations. Give
	// the aggregate reorg the fixture's existing 5s maintenance budget; each
	// adapter read keeps the original 2s source deadline. The default 2s total
	// intentionally exhausted during fsync under race (not an admission bug).
	s.cfg.Config.Journal.TimeoutMS = maintenance.cfg.TimeoutMS
	s.adapters[key].Client.SetRecorder(func(ctx context.Context, provider, method string, params json.RawMessage, response []byte) error {
		err := s.rawSources[key].capture(ctx, provider, method, params, response)
		if err != nil {
			t.Errorf("responsible RPC capture %s(%s): %v", method, params, err)
		}
		return err
	})
	s.historyRuntime[key].Pause(true)
	head.Store(102)
	s.Tick(context.Background())
	before, ready := s.stores[key].Current()
	if !ready || before.Reference.Block.Number != 102 {
		t.Fatal("normal staged collection failed", before.Reference, ready)
	}
	s.historyRuntime[key].Pause(true)
	fork.Store(true)
	head.Store(103)
	s.Tick(context.Background())
	after, ready := s.stores[key].Current()
	if !ready || after.Reference.Block.Hash != replacement[103].Hash().Hex() || after.Reference.CanonicalEpoch != before.Reference.CanonicalEpoch+1 {
		t.Fatal("normal reorg owner commit missing", after.Reference, ready)
	}
	units, err := s.stores[key].Records(before.Reference.JournalSeq, 10)
	if err != nil || len(units) != 3 || units[2].Kind != "REORG_COMMIT" {
		t.Fatal("replacement facts not atomic with owner commit", units, err)
	}
	maintenance.mu.Lock()
	var ledger rawOwnerLedgerEntry
	for _, entry := range maintenance.chains[key].ledgers {
		if entry.Ledger.Record.Kind == "REORG_COMMIT" {
			ledger = entry
			break
		}
	}
	maintenance.mu.Unlock()
	var proof rawOwnerProof
	if ledger.Raw.ID == "" || json.Unmarshal(ledger.Ledger.Record.Payload, &proof) != nil || proof.Kind != RawReorgOwnerKind || proof.Reference != after.Reference || !ledger.Retired {
		t.Fatal("reorg did not hand off actual same-batch owner receipt", ledger, proof)
	}
	bytes, err := maintenance.readEvidence(context.Background(), key, proof.Raw)
	var captured journal.Unit
	if err != nil || json.Unmarshal(bytes, &captured) != nil || len(captured.NormalizedFacts.Blocks) != 3 || len(captured.NormalizedFacts.RevertedBlocks) != 2 {
		t.Fatal("raw owner unit lost replacement or reverted blocks", err, captured)
	}
	pins, err := maintenance.expandProofPins(context.Background(), key, proof)
	if err != nil {
		t.Fatal(err)
	}
	tags := map[string]bool{}
	for _, pin := range pins {
		data, err := maintenance.readEvidence(context.Background(), key, pin.Raw)
		if err != nil {
			t.Fatal(err)
		}
		var rpc struct {
			Method string
			Params []json.RawMessage
		}
		if json.Unmarshal(data, &rpc) == nil && rpc.Method == "eth_getBlockByNumber" && len(rpc.Params) > 0 {
			var tag string
			_ = json.Unmarshal(rpc.Params[0], &tag)
			tags[tag] = true
		}
	}
	for _, tag := range []string{"0x64", "0x65", "0x66", "0x67", "finalized"} {
		if !tags[tag] {
			t.Fatal("reorg source read escaped responsibility group", tag, tags)
		}
	}
	history, err := s.historyRuntime[key].SourceStatus(context.Background())
	if err != nil || history.Queue.ScopeStale || history.Queue.CanonicalEpoch != after.Reference.CanonicalEpoch {
		t.Fatal("history canonical scope not refreshed", history, err)
	}
}

func TestNormalLifecycleDuplicateStartAndClosedTickDoNotReenterSources(t *testing.T) {
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	base := testSnapshotSource(t)
	calls := 0
	s, err := NewWithSource(cfg, func(ctx context.Context, chain config.Chain) (domain.Snapshot, error) {
		calls++
		return base(ctx, chain)
	})
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err = s.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	before := s.stores[cfg.Config.Chains[0].Key].DurableRef()
	if err = s.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	if calls != 1 || s.stores[cfg.Config.Chains[0].Key].DurableRef() != before {
		t.Fatal("duplicate Start reacquired online generation", calls)
	}
	s.Close()
	s.Close()
	s.Tick(context.Background())
	if err = s.Start(context.Background()); err == nil || err.Error() != "SERVICE_CLOSED" {
		t.Fatal("closed service restarted", err)
	}
	if calls != 1 {
		t.Fatal("closed lifecycle reentered source", calls)
	}
}

func TestReorgOwnerProjectionKeepsProviderFenceCurrentBeforeNextRenewal(t *testing.T) {
	s, chain, previous, reader := forkService(t)
	if err := s.recoverFork(context.Background(), chain, previous, reader); err != nil {
		t.Fatal(err)
	}
	current, ready := s.stores[chain.Key].Current()
	if !ready {
		t.Fatal("replacement branch not committed")
	}
	// PG.Committed projects the new canonical epoch immediately. The chain
	// actor must mirror that committed projection before the next PG renewal.
	s.cfg.Config.Journal.Transport = "kafka"
	writer := &writerSession{chain: chain, floor: previous.Reference, lease: control.Lease{Chain: chain.Key, WriterEpoch: current.Reference.WriterEpoch, Generation: previous.Reference.OnlineGeneration, CanonicalEpoch: previous.Reference.CanonicalEpoch, JournalSeq: previous.Reference.JournalSeq, ExpiresMS: uint64(time.Now().Add(time.Minute).UnixMilli())}}
	s.sessions = map[string]*writerSession{chain.Key: writer}
	defer delete(s.sessions, chain.Key)
	writer.observeOwnerCommit(journal.Unit{Kind: "REORG_COMMIT", Snapshot: current})
	view := s.captureRuntimeOwner(chain)
	if !view.Valid || view.Ref != current.Reference || writer.lease.JournalSeq != current.Reference.JournalSeq {
		t.Fatal("committed owner projection became stale until periodic renewal", view, writer.lease)
	}
}

func TestNormalReorgCannotReadRevokedDynamicScopeAsStatic(t *testing.T) {
	s, chain, previous, _ := forkService(t)
	s.source = nil
	selected := chain
	for _, pool := range previous.Pools {
		selected.Pools = append(selected.Pools, config.PoolConfig{ID: pool.PoolID, Protocol: pool.Protocol})
	}
	var reads atomic.Int64
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		reads.Add(1)
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "error": map[string]any{"code": -32000, "message": "revoked dynamic scope must requalify first"}})
	}))
	defer server.Close()
	adapter := &evm.Adapter{Chain: selected, Client: rpc.New([]string{server.URL}, time.Second, 1<<20)}
	err := s.recoverFork(context.Background(), chain, previous, adapter)
	if err == nil || reads.Load() != 0 {
		t.Fatal("revoked adapter identities bypassed dynamic admission", reads.Load(), err)
	}
	if _, ready := s.stores[chain.Key].Current(); ready {
		t.Fatal("revoked dynamic scope remained online")
	}
}
