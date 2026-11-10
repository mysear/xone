package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/trie"
	"math/big"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
)

type rawMaintenanceRPCState struct {
	Head, Receipts    atomic.Int64
	Genesis, BaseHash string
}

func maintenanceFixture(t *testing.T, changes ...func(*config.RawMaintenanceConfig)) (*Service, *RawMaintenance, string, *rawMaintenanceRPCState) {
	t.Helper()
	return maintenanceFixtureWithChain(t, nil, changes...)
}

func maintenanceFixtureWithChain(t *testing.T, chainChange func(*config.Chain, *rawMaintenanceRPCState), changes ...func(*config.RawMaintenanceConfig)) (*Service, *RawMaintenance, string, *rawMaintenanceRPCState) {
	t.Helper()
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Journal.MaxBytes = 256 << 10
	cfg.Token = "raw-maintenance-auth-token-for-tests"
	cfg.Config.Journal.RawEvidence = &config.RawEvidenceConfig{MaxBytes: 64 << 20, MaxRecords: 10000, MaxRecordBytes: 4 << 20}
	mk := func(n int64, parent common.Hash) *types.Header {
		return &types.Header{Number: big.NewInt(n), ParentHash: parent, UncleHash: types.EmptyUncleHash, Root: common.HexToHash("0x1"), TxHash: types.EmptyTxsHash, ReceiptHash: types.EmptyReceiptsHash, Difficulty: big.NewInt(0), GasLimit: 30000000, Time: uint64(time.Now().Unix())}
	}
	genesis := mk(0, common.Hash{})
	base := mk(100, common.HexToHash("0x99"))
	next := mk(101, base.Hash())
	rpcState := &rawMaintenanceRPCState{Genesis: genesis.Hash().Hex(), BaseHash: base.Hash().Hex()}
	head := &rpcState.Head
	head.Store(100)
	signingKey, err := crypto.HexToECDSA(strings.Repeat("1", 64))
	if err != nil {
		t.Fatal(err)
	}
	recipient := common.HexToAddress("0x1234")
	tx, err := types.SignTx(types.NewTx(&types.LegacyTx{Nonce: 0, To: &recipient, Value: big.NewInt(0), Gas: 21000, GasPrice: big.NewInt(1)}), types.LatestSignerForChainID(big.NewInt(1)), signingKey)
	if err != nil {
		t.Fatal(err)
	}
	receipt := &types.Receipt{Type: 0, Status: 1, CumulativeGasUsed: 21000, Logs: []*types.Log{}}
	receipt.Bloom = types.CreateBloom(receipt)
	next.TxHash = types.DeriveSha(types.Transactions{tx}, trie.NewStackTrie(nil))
	next.ReceiptHash = types.DeriveSha(types.Receipts{receipt}, trie.NewStackTrie(nil))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			Method string
			Params []json.RawMessage
		}
		if json.NewDecoder(r.Body).Decode(&q) != nil {
			t.Error("bad RPC")
			return
		}
		var result any
		if q.Method == "eth_getCode" || q.Method == "eth_call" || q.Method == "eth_getStorageAt" || q.Method == "eth_getBalance" {
			index := 1
			if q.Method == "eth_getStorageAt" {
				index = 2
			}
			if len(q.Params) > index {
				var selector map[string]any
				if json.Unmarshal(q.Params[index], &selector) == nil {
					hash, _ := selector["blockHash"].(string)
					if hash != "" && hash != base.Hash().Hex() && hash != next.Hash().Hex() {
						_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "error": map[string]any{"code": -32001, "message": "unknown block"}})
						return
					}
				}
			}
		}
		switch q.Method {
		case "eth_getCode":
			result = "0x6000"
		case "eth_getStorageAt":
			result = "0x" + strings.Repeat("0", 64)
		case "eth_getBalance":
			result = "0x1"
		case "eth_blockNumber":
			result = "0x64"
		case "eth_chainId":
			result = "0x1"
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(q.Params[0], &tag)
			h := base
			if tag == "0x0" {
				h = genesis
			} else if tag == "0x65" || (tag != "0x64" && head.Load() == 101) {
				h = next
			}
			raw, _ := json.Marshal(h)
			var fields map[string]any
			_ = json.Unmarshal(raw, &fields)
			fields["transactions"] = []any{}
			if h == next {
				var full bool
				if len(q.Params) > 1 {
					_ = json.Unmarshal(q.Params[1], &full)
				}
				if full {
					raw, _ := tx.MarshalJSON()
					var transaction map[string]any
					_ = json.Unmarshal(raw, &transaction)
					transaction["blockHash"] = next.Hash().Hex()
					transaction["blockNumber"] = "0x65"
					transaction["transactionIndex"] = "0x0"
					fields["transactions"] = []any{transaction}
				} else {
					fields["transactions"] = []string{tx.Hash().Hex()}
				}
			}
			result = fields
		case "eth_getTransactionReceipt":
			rpcState.Receipts.Add(1)
			result = evm.Receipt{BlockHash: next.Hash().Hex(), TxHash: tx.Hash().Hex(), TxIndex: "0x0", Type: "0x0", Status: "0x1", CumulativeGasUsed: "0x5208", LogsBloom: "0x" + strings.Repeat("0", 512), Logs: []evm.Log{}}
		case "eth_call":
			var call struct{ Data string }
			_ = json.Unmarshal(q.Params[0], &call)
			switch call.Data {
			case "0x":
				result = "0x"
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("1", 40)
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("2", 40)
			default:
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			}
		default:
			t.Errorf("unexpected RPC %s", q.Method)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	t.Cleanup(server.Close)
	second := httptest.NewServer(server.Config.Handler)
	t.Cleanup(second.Close)
	ch := &cfg.Config.Chains[0]
	ch.Key = "evm:1"
	ch.Genesis = genesis.Hash().Hex()
	ch.RPCURLs = []string{server.URL, second.URL}
	ch.BlockIntegrity = "ethereum-header-transactions-v1"
	ch.Confirmation = "finalized"
	ch.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeNumerator: 3, FeeDenominator: 1000}}
	if chainChange != nil {
		chainChange(ch, rpcState)
	}
	c := config.RawMaintenanceConfig{SinkPath: t.TempDir(), SinkMaxBytes: 64 << 20, SinkMaxRecords: 10000, IntervalMS: 60000, TimeoutMS: 5000, ReplayTimeoutMS: 5000, KeepMS: 0, TargetBytes: 32 << 20, MaxScan: 32, MaxArchive: 4, MaxReclaims: 4, MaxRawBytes: 4 << 20, MaxDecodedBytes: 4 << 20, MaxPageRecords: 100, MaxRequestBytes: 8192, MaxPinsPerCapture: 64, DecoderVersion: RawStagedDecoderVersion}
	for _, change := range changes {
		change(&c)
	}
	cfg.Config.Journal.RawMaintenance = &c
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	m := s.rawMaintenance
	if m == nil {
		t.Fatal("normal New did not consume raw_maintenance configuration")
	}
	if err = s.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	return s, m, ch.Key, rpcState
}
func TestRawMaintenanceNormalNewStartArchivesWeakObservationsWithoutCanonicalPromotion(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	if err := m.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := m.Tick(context.Background()); err != nil {
		t.Fatal(err)
	}
	records, err := s.rawSources[key].archive.RawRecords()
	if err != nil || len(records) == 0 {
		t.Fatal(records, err)
	}
	found := false
	for _, r := range records {
		if r.Archived {
			manifest, err := s.rawSources[key].archive.ArchivedManifest(r.Raw)
			if err != nil || manifest.Metadata.DecoderVersion != RawObservationVersion {
				t.Fatal(manifest, err)
			}
			if manifest.Metadata.Namespace != ingest.NamespaceHistory || strings.Contains(manifest.Metadata.CanonicalProof, "canonical output") {
				t.Fatal("HTTP observation promoted to canonical state", manifest)
			}
			found = true
		}
	}
	if !found {
		t.Fatal("real local archive copy missing")
	}
	req := httptest.NewRequest("GET", "/internal/chains/"+key+"/raw/dlq", nil)
	w := httptest.NewRecorder()
	m.Handler().ServeHTTP(w, req)
	if w.Code != 401 {
		t.Fatal(w.Code)
	}
}
func TestRawMaintenanceStagedRepairUsesAtomicOwnerRecordAndReopensUnknown(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	current, ok := s.stores[key].Current()
	if !ok {
		t.Fatal("current")
	}
	reader := m.ProtectReader(key, current.Reference, s.adapters[key])
	block, pools, swaps, err := reader.ReadComplete(context.Background(), "0x65")
	if err != nil {
		t.Fatal(err)
	}
	ref, err := s.rawSources[key].archive.Put(ingest.Owner{Chain: key, Height: block.Number, Hash: block.Hash, ProviderID: "derived-state-from-recorded-rpc"}, mustRawJSON(t, stagedState{block, pools, swaps}))
	if err != nil {
		t.Fatal(err)
	}
	d := ingest.DeadLetter{Raw: ref, Stage: "validate", Code: "STAGED_REPAIR", Scope: current.Reference.ScopeHash, Message: "controlled staged failure"}
	incident, err := s.rawSources[key].archive.QuarantineIncident(d)
	if err != nil {
		t.Fatal(err)
	}
	request := RawReplayRequest{DeadLetterID: ingest.DeadLetterID(d), IncidentID: incident.ID, Namespace: ingest.NamespaceOnline, ExpectedGeneration: current.Reference.OnlineGeneration}
	result, err := m.Replay(context.Background(), key, request)
	if err != nil || result.Status != ingest.Resolved {
		t.Fatal(result, err)
	}
	record, err := m.lookupOwnerRecord(key, "online-replay/"+result.AttemptID)
	if err != nil || record.Kind != "BLOCK_COMMIT" || record.Reference.Block != block {
		t.Fatal(record, err)
	}
	if _, err := m.Replay(context.Background(), key, request); err == nil {
		t.Fatal("stale online request accepted as next-contiguous")
	}
	// Persisted owner identity remains authoritative even after current advances;
	// receipt confirmation must read the immutable reducer record, not echo intent.
	receipt, err := m.confirmOwner(key, result.Intent)
	if err != nil || receipt.State != ingest.CommitConfirmed || !strings.Contains(receipt.Proof, record.CommitID) {
		t.Fatal(receipt, err)
	}
}
func mustRawJSON(t *testing.T, v any) []byte {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func stagedMaintenanceFailure(t *testing.T, s *Service, m *RawMaintenance, key string) (RawReplayRequest, ingest.DeadLetter) {
	t.Helper()
	current, ok := s.stores[key].Current()
	if !ok {
		t.Fatal("current")
	}
	block, pools, swaps, err := m.ProtectReader(key, current.Reference, s.adapters[key]).ReadComplete(context.Background(), "0x65")
	if err != nil {
		t.Fatal(err)
	}
	ref, err := s.rawSources[key].archive.Put(ingest.Owner{Chain: key, Height: block.Number, Hash: block.Hash, ProviderID: "derived-state-from-recorded-rpc"}, mustRawJSON(t, stagedState{block, pools, swaps}))
	if err != nil {
		t.Fatal(err)
	}
	d := ingest.DeadLetter{Raw: ref, Stage: "validate", Code: "STAGED_REPAIR", Scope: current.Reference.ScopeHash, Message: "controlled staged failure"}
	i, err := s.rawSources[key].archive.QuarantineIncident(d)
	if err != nil {
		t.Fatal(err)
	}
	return RawReplayRequest{DeadLetterID: ingest.DeadLetterID(d), IncidentID: i.ID, Namespace: ingest.NamespaceOnline, ExpectedGeneration: current.Reference.OnlineGeneration}, d
}
func TestRawMaintenanceUnknownReopenReadsImmutableOwnerReceipt(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	request, d := stagedMaintenanceFailure(t, s, m, key)
	m.afterOwnerCommit = func() error { return errors.New("controlled owner ack lost after durable batch") }
	result, err := m.Replay(context.Background(), key, request)
	if !errors.Is(err, ingest.ErrCommitUnknown) || result.Status != ingest.Retry {
		t.Fatal(result, err)
	}
	if q, _ := s.rawSources[key].archive.IsQuarantined(d.Raw); !q {
		t.Fatal("unknown lost isolation")
	}
	record, err := s.stores[key].OwnerRecord("online-replay/" + result.AttemptID)
	if err != nil {
		t.Fatal("real owner batch did not commit", err)
	}
	oldArchive := s.rawSources[key].archive
	path, limits := oldArchive.Directory(), oldArchive.Limits()
	cfg := m.cfg
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	if err := oldArchive.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.stores[key].Close(); err != nil {
		t.Fatal(err)
	}
	chain := s.cfg.Config.Chains[0]
	store, err := reducer.Open(filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), strings.ReplaceAll(key, ":", "-")), chain.Owner, chain.Assignment, chain.ReorgWindow)
	if err != nil {
		t.Fatal(err)
	}
	s.stores[key] = store
	archive, err := ingest.OpenArchive(path, limits)
	if err != nil {
		t.Fatal(err)
	}
	s.rawSources[key].archive = archive
	restored, err := NewRawMaintenance(s, &cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Close()
	resolved, err := restored.Replay(context.Background(), key, RawReplayRequest{PendingAttemptID: result.AttemptID})
	if err != nil || resolved.Status != ingest.Resolved || resolved.Spec != result.Spec {
		t.Fatal(resolved, err)
	}
	// Mutating a returned owner payload cannot alter the reducer's immutable proof.
	record.Payload = []byte(`{}`)
	receipt, err := restored.confirmOwner(key, resolved.Intent)
	if err != nil || !strings.Contains(receipt.Proof, record.CommitID) {
		t.Fatal(receipt, err)
	}
	pending, err := archive.PendingReplays()
	if err != nil || len(pending) != 0 {
		t.Fatal(pending, err)
	}
}
func TestRawMaintenanceRejectsScriptsAndRawHTTPReplay(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	handler := m.Handler()
	for _, payload := range []string{`{"decoder_script":"evil"}`, `{"state":{"pools":[]}}`} {
		r := httptest.NewRequest("POST", "/internal/chains/"+key+"/raw/replay", strings.NewReader(payload))
		r.Header.Set("Authorization", "Bearer "+s.cfg.Token)
		w := httptest.NewRecorder()
		handler.ServeHTTP(w, r)
		if w.Code != 400 {
			t.Fatal(w.Code, w.Body.String())
		}
	}
	records, err := s.rawSources[key].archive.RawRecords()
	if err != nil || len(records) == 0 {
		t.Fatal(err)
	}
	d := ingest.DeadLetter{Raw: records[0].Raw, Stage: "decode", Code: "RPC", Scope: "chain", Message: "raw HTTP cannot reconstruct scope"}
	i, err := s.rawSources[key].archive.QuarantineIncident(d)
	if err != nil {
		t.Fatal(err)
	}
	_, err = m.Replay(context.Background(), key, RawReplayRequest{DeadLetterID: ingest.DeadLetterID(d), IncidentID: i.ID, Namespace: ingest.NamespaceOnline})
	if err == nil || !strings.Contains(err.Error(), "NOT_HTTP") {
		t.Fatal(err)
	}
}
func TestRawMaintenanceOldGenerationOnlyHistoryAndNoOnlineMutation(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, m, key)
	base, _ := s.stores[key].Current()
	advanced := base
	advanced.Reference.OnlineGeneration++
	advanced.Reference.WriterEpoch++
	advanced.Reference.JournalSeq++
	if err := s.stores[key].ApplyLocal(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: advanced}); err != nil {
		t.Fatal(err)
	}
	request.ExpectedGeneration = advanced.Reference.OnlineGeneration
	if _, err := m.Replay(context.Background(), key, request); err == nil {
		t.Fatal("old acquisition admitted online")
	}
	request.Namespace = ingest.NamespaceHistory
	result, err := m.Replay(context.Background(), key, request)
	if err != nil || result.Status != ingest.Resolved {
		t.Fatal(result, err)
	}
	now, _ := s.stores[key].Current()
	if now.Reference != advanced.Reference {
		t.Fatal("history changed online", now.Reference)
	}
	record, err := m.lookupOwnerRecord(key, "history-replay/"+result.AttemptID)
	if err != nil || record.Kind != "HISTORY_REPLAY_COMMIT" {
		t.Fatal(record, err)
	}
}

type rawMaintenanceRoundTrip func(*http.Request) (*http.Response, error)

func (f rawMaintenanceRoundTrip) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }
func TestRawMaintenanceFinalGateRejectsGenerationChangeOutsideNetworkLock(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	request, d := stagedMaintenanceFailure(t, s, m, key)
	client := s.adapters[key].Client
	upstream := client.HTTP.Transport
	if upstream == nil {
		upstream = http.DefaultTransport
	}
	var changed atomic.Bool
	client.HTTP.Transport = rawMaintenanceRoundTrip(func(r *http.Request) (*http.Response, error) {
		if !changed.Swap(true) {
			if !s.syncMu.TryLock() {
				t.Error("network read held chain actor")
			} else {
				base, _ := s.stores[key].Current()
				base.Reference.OnlineGeneration++
				base.Reference.WriterEpoch++
				base.Reference.JournalSeq++
				if err := s.stores[key].ApplyLocal(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: base}); err != nil {
					t.Error(err)
				}
				s.syncMu.Unlock()
			}
		}
		return upstream.RoundTrip(r)
	})
	result, err := m.Replay(context.Background(), key, request)
	if err == nil || result.Status != ingest.Retry {
		t.Fatal(result, err)
	}
	if _, err := s.stores[key].OwnerRecord("online-replay/" + result.AttemptID); err == nil {
		t.Fatal("fenced result published")
	}
	if q, _ := s.rawSources[key].archive.IsQuarantined(d.Raw); !q {
		t.Fatal("fenced result released raw")
	}
}
func TestRawMaintenanceNormalStagePreservesCallbackAndPinsUntilOwnerBatch(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	current, _ := s.stores[key].Current()
	var callbacks atomic.Int64
	s.adapters[key].OnComplete = func(ctx context.Context, b evm.RawBlock, receipts []evm.Receipt) error {
		if _, ok := ctx.Value(rawCaptureKey{}).(*rawCapture); !ok {
			t.Error("callback lost capture group")
		}
		callbacks.Add(1)
		return nil
	}
	b, pools, swaps, err := m.ProtectReader(key, current.Reference, s.adapters[key]).ReadComplete(context.Background(), "0x65")
	if err != nil || callbacks.Load() != 1 {
		t.Fatal(err, callbacks.Load())
	}
	next := current
	next.Reference.Block = b
	next.Reference.JournalSeq++
	next.Pools = pools
	unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: b, Swaps: swaps}}}}
	records, err := m.OwnerRecordsForUnit(unit)
	if err != nil || len(records) != 1 {
		t.Fatal(records, err)
	}
	rawRecords, err := s.rawSources[key].archive.RawRecords()
	if err != nil {
		t.Fatal(err)
	}
	held := false
	for _, r := range rawRecords {
		if r.Raw.Owner.ProviderID == "derived-state-from-recorded-rpc" {
			held = r.ActiveReferences > 0
		}
	}
	if !held {
		t.Fatal("stage unpinned before real owner batch")
	}
	if err := m.PublishStaged(context.Background(), unit, nil); err != nil {
		t.Fatal(err)
	}
	for id := range records {
		if _, err := s.stores[key].OwnerRecord(id); err == nil {
			t.Fatal("normal owner record retained per block after durable acknowledgement")
		}
		record, err := m.lookupOwnerRecord(key, id)
		if err != nil || record.Reference != next.Reference {
			t.Fatal(record, err)
		}
	}
	if err := m.Tick(context.Background()); err != nil {
		t.Fatal(err)
	}
}
func TestRawMaintenanceCloseCancelsInFlightReplay(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, m, key)
	client := s.adapters[key].Client
	entered := make(chan struct{})
	var once sync.Once
	client.HTTP.Transport = rawMaintenanceRoundTrip(func(r *http.Request) (*http.Response, error) {
		once.Do(func() { close(entered) })
		<-r.Context().Done()
		return nil, r.Context().Err()
	})
	finished := make(chan error, 1)
	go func() { _, err := m.Replay(context.Background(), key, request); finished <- err }()
	<-entered
	start := time.Now()
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	if time.Since(start) > time.Second {
		t.Error("Close did not cancel the replay promptly")
	}
	if err := <-finished; !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}

func TestRawMaintenanceSinkFullBackpressuresNormalRPC(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t, func(c *config.RawMaintenanceConfig) { c.SinkMaxRecords = 2 })
	err := m.Tick(context.Background())
	if !errors.Is(err, ingest.ErrBudget) {
		t.Fatal("sink did not fail closed at capacity", err)
	}
	status, err := m.chains[key].sink.Status()
	if err != nil || !status.Failed || status.Records > 2 {
		t.Fatal(status, err)
	}
	var chainID string
	if err := s.adapters[key].Client.Call(context.Background(), "eth_chainId", []any{}, &chainID); err == nil {
		t.Fatal("normal source ignored archive backpressure")
	}
	if m.Status()["error"] == "" {
		t.Fatal("maintenance health hid capacity failure")
	}
}

func TestRawMaintenanceOwnerRecordRejectsDifferentStateAtSameHeightHash(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	current, _ := s.stores[key].Current()
	b, pools, _, err := m.ProtectReader(key, current.Reference, s.adapters[key]).ReadComplete(context.Background(), "0x65")
	if err != nil {
		t.Fatal(err)
	}
	next := current
	next.Reference.Block = b
	next.Reference.JournalSeq++
	next.Pools = append([]domain.Pool(nil), pools...)
	next.Pools[0].Reserve0 = domain.Amount("123")
	if _, err := m.OwnerRecordsForUnit(journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next}); err == nil {
		t.Fatal("capture group bound to another state at same block")
	}
}

func TestRawMaintenanceRepairRereadsEveryReceiptAtOriginalHeight(t *testing.T) {
	s, m, key, rpcState := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, m, key)
	before := rpcState.Receipts.Load()
	if before == 0 {
		t.Fatal("capture bypassed receipt verification")
	}
	result, err := m.Replay(context.Background(), key, request)
	if err != nil || result.Status != ingest.Resolved {
		t.Fatal(result, err)
	}
	if rpcState.Receipts.Load() <= before {
		t.Fatal("repair reused old HTTP/staged state instead of rereading receipts")
	}
}

func TestRawMaintenancePinGroupsKeepOwnerPayloadBelow64KiB(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	m.cfg.MaxPinsPerCapture = 20000
	records, err := s.rawSources[key].archive.RawRecords()
	if err != nil || len(records) == 0 {
		t.Fatal(err)
	}
	pins := make([]RawPin, 5000)
	for i := range pins {
		pins[i] = RawPin{Raw: records[0].Raw, Reference: fmt.Sprintf("bounded-test-%d", i)}
	}
	group, err := m.savePinGroup(context.Background(), key, "bounded-test", 101, pins)
	if err != nil {
		t.Fatal(err)
	}
	p := rawOwnerProof{Kind: "raw-capture-owner-v1", Groups: []rawPinGroup{group}}
	raw := mustRawJSON(t, p)
	if len(raw) >= 65536 {
		t.Fatal("owner payload expanded with RPC count", len(raw))
	}
	if len(p.Pins) != 0 {
		t.Fatal("flat pin array retained in owner proof")
	}
	loaded, err := m.expandProofPins(context.Background(), key, p)
	if err != nil || len(loaded) != 5001 {
		t.Fatal(len(loaded), err)
	}
}

func TestRawMaintenanceUnknownCannotRetireOwnerProof(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, m, key)
	m.afterOwnerCommit = func() error { return errors.New("owner ack lost") }
	result, err := m.Replay(context.Background(), key, request)
	if !errors.Is(err, ingest.ErrCommitUnknown) {
		t.Fatal(result, err)
	}
	record, err := s.stores[key].OwnerRecord("online-replay/" + result.AttemptID)
	if err != nil {
		t.Fatal(err)
	}
	entry, err := m.persistOwnerLedger(context.Background(), key, record)
	if err != nil {
		t.Fatal(err)
	}
	if err := m.finishRetirement(context.Background(), key, entry); !errors.Is(err, ingest.ErrCommitUnknown) {
		t.Fatal("unknown proof retired", err)
	}
	if _, err := s.stores[key].OwnerRecord(record.Key); err != nil {
		t.Fatal("unknown dedupe identity deleted", err)
	}
}
func TestRawMaintenanceResolvedProofRetiresOnlyAfterLedgerHandoffAndReopensCold(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	request, _ := stagedMaintenanceFailure(t, s, m, key)
	result, err := m.Replay(context.Background(), key, request)
	if err != nil || result.Status != ingest.Resolved {
		t.Fatal(result, err)
	}
	sqlKey := "online-replay/" + result.AttemptID
	if _, err := s.stores[key].OwnerRecord(sqlKey); err == nil {
		t.Fatal("resolved dedupe proof was never retired")
	}
	record, err := m.lookupOwnerRecord(key, sqlKey)
	if err != nil || record.CommitID == "" {
		t.Fatal(record, err)
	}
	// A few bounded passes hand released proof/group files to the verified sink.
	drainRawMaintenance(t, m, func(records []ingest.RawRecordInfo) bool {
		m.mu.Lock()
		entry, ok := m.chains[key].ledgers[sqlKey]
		m.mu.Unlock()
		if !ok {
			return false
		}
		for _, r := range records {
			if r.Raw == entry.Raw {
				return r.Archived && r.ActiveReferences == 0
			}
		}
		return false
	})
	m.mu.Lock()
	entry, ok := m.chains[key].ledgers[sqlKey]
	m.mu.Unlock()
	if !ok || !entry.Retired {
		t.Fatal("ledger handoff missing")
	}
	manifest, err := s.rawSources[key].archive.ArchivedManifest(entry.Raw)
	if err != nil {
		t.Fatal(err)
	}
	if err := m.chains[key].sink.Verify(context.Background(), manifest); err != nil {
		t.Fatal(err)
	}
	cfg := m.cfg
	old := s.rawSources[key].archive
	path, limits := old.Directory(), old.Limits()
	m.Close()
	old.Close()
	s.stores[key].Close()
	chain := s.cfg.Config.Chains[0]
	store, err := reducer.Open(filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), strings.ReplaceAll(key, ":", "-")), chain.Owner, chain.Assignment, chain.ReorgWindow)
	if err != nil {
		t.Fatal(err)
	}
	s.stores[key] = store
	archive, err := ingest.OpenArchive(path, limits)
	if err != nil {
		t.Fatal(err)
	}
	s.rawSources[key].archive = archive
	restored, err := NewRawMaintenance(s, &cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Close()
	receipt, err := restored.confirmOwner(key, result.Intent)
	if err != nil || receipt.State != ingest.CommitConfirmed || !strings.Contains(receipt.Proof, record.CommitID) {
		t.Fatal(receipt, err)
	}
	if _, err := store.OwnerRecord(sqlKey); err == nil {
		t.Fatal("cold proof confirmation recreated a retired dedupe record")
	}
}
