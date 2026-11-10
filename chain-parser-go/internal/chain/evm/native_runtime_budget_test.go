package evm

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func nativeBudgetPool() config.PoolConfig {
	return config.PoolConfig{ID: "0x28c96a719edc7c1ba3fb1bafcadf470ace9dfd75", Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("a", 40), Token1: "0x" + strings.Repeat("b", 40), FeeNumerator: 997, FeeDenominator: 1000}
}
func TestNativeWitnessBusinessScopeCheckedOnEveryProvider(t *testing.T) {
	for _, affected := range []bool{true, false} {
		t.Run(map[bool]string{true: "affected", false: "unrelated"}[affected], func(t *testing.T) {
			primary := newNormalNativeFixture(t, "polygon-synthetic")
			secondary := newNormalNativeFixture(t, "polygon-synthetic")
			// Only the primary omits the uncommitted RPC synthetic suffix. The witness
			// retains every original public system log; canonical commitments agree.
			primary.target.Transactions = primary.target.Transactions[:len(primary.target.Transactions)-1]
			primary.receipts[primary.target.Hash] = primary.receipts[primary.target.Hash][:len(primary.receipts[primary.target.Hash])-1]
			var fields map[string]json.RawMessage
			json.Unmarshal(primary.headers[primary.target.Number], &fields)
			fields["transactions"], _ = json.Marshal(primary.target.Transactions)
			primary.headers[primary.target.Number], _ = json.Marshal(fields)
			json.Unmarshal(primary.full[primary.target.Number], &fields)
			var txs []json.RawMessage
			json.Unmarshal(fields["transactions"], &txs)
			fields["transactions"], _ = json.Marshal(txs[:len(txs)-1])
			primary.full[primary.target.Number], _ = json.Marshal(fields)
			chain := primary.chain
			limits := *chain.NativeProof
			limits.MaxBytes = 64 << 20
			chain.NativeProof = &limits
			pool := nativeBudgetPool()
			if affected {
				pool.ID = "0x1fdeaf938267ca43388ed1fdb879eaf91e920c7a"
			}
			chain.Pools = []config.PoolConfig{pool}
			left, right := primary.serve(t), secondary.serve(t)
			a := Adapter{Client: rpc.New([]string{left.URL, right.URL}, 30*time.Second, 16<<20), Chain: chain}
			ctx := context.WithValue(context.Background(), nativeDutyKey{}, "witness-business")
			b, err := a.Block(ctx, primary.target.Number)
			if err != nil {
				t.Fatal(err)
			}
			_, _, err = a.ReadQualifiedReceipts(ctx, b, false)
			if affected {
				if err == nil || !strings.Contains(err.Error(), "RECEIPT_BUSINESS_SYSTEM_EVENTS_UNPROVEN") {
					t.Fatalf("witness business scope was not enforced: %v", err)
				}
			} else if err != nil {
				t.Fatal("unrelated system observations stopped canonical business scope", err)
			}
		})
	}
}
func TestNativeOwnHeaderReplayChargesEvidenceAndFinalAnchor(t *testing.T) {
	f := newNormalNativeFixture(t, "arbitrum")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "replay-budget")
	b, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	b, _, err = a.ReadQualifiedReceipts(ctx, b, false)
	if err != nil {
		t.Fatal(err)
	}
	verified, err := VerifyNativeReceiptEvidence(ctx, a.Chain.Key, b, *b.NativeCoverage.Evidence, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if verified.ProofBytesUsed <= 0 {
		t.Fatal("own-header replay dropped local verification cost")
	}
	limits := *a.Chain.NativeProof
	limits.MaxBytes = 2*b.NativeBodyProof.ProofBytesUsed + 1
	a.Chain.NativeProof = &limits
	fresh, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	if err = a.RecheckNativeReceiptEvidence(ctx, fresh, *verified.Evidence); err == nil {
		t.Fatal("archive verification bypassed aggregate native budget")
	}
}
func TestNativeNormalStateSharesProofBudgetAndBackpressuresCompletion(t *testing.T) {
	f := newNormalNativeFixture(t, "arbitrum")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "state-budget")
	proofCost := 0
	a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error {
		proofCost = a.Chain.NativeProof.MaxBytes - b.nativeSession.remaining
		return nil
	}
	if _, _, _, err := a.ReadComplete(ctx, f.target.Number); err != nil {
		t.Fatal(err)
	}
	limits := *a.Chain.NativeProof
	limits.MaxBytes = proofCost + 1
	a.Chain.NativeProof = &limits
	a.Chain.Pools = []config.PoolConfig{nativeBudgetPool()}
	completions := 0
	a.OnComplete = func(context.Context, RawBlock, []Receipt) error { completions++; return nil }
	if _, _, _, err := a.ReadComplete(ctx, f.target.Number); err == nil || !strings.Contains(err.Error(), "BUDGET") {
		t.Fatal("normal pool state did not fail for the native budget", err)
	}
	if completions != 0 {
		t.Fatal("budget-exhausted state reached OnComplete")
	}
}

func TestNativeNormalStateChargesWireAndRetainedPoolsWithoutReplacingSource(t *testing.T) {
	f := newNormalNativeFixture(t, "arbitrum")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "state-cost")
	originalClient, originalHTTP := a.Client, a.Client.HTTP
	proofCost, withStateCost, wireBytes, completions := 0, 0, 0, 0
	a.Client.SetRecorder(func(observe context.Context, _ string, method string, params json.RawMessage, response []byte) error {
		if observe.Value(nativeDutyKey{}) != "state-cost" {
			t.Fatal("state RPC lost responsibility context")
		}
		if method == "eth_call" {
			request, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
			wireBytes += len(request) + len(response)
		}
		return nil
	})
	a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error {
		proofCost = a.Chain.NativeProof.MaxBytes - b.nativeSession.remaining
		return nil
	}
	if _, _, _, err := a.ReadComplete(ctx, f.target.Number); err != nil {
		t.Fatal(err)
	}
	a.Chain.Pools = []config.PoolConfig{nativeBudgetPool()}
	a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error {
		withStateCost = a.Chain.NativeProof.MaxBytes - b.nativeSession.remaining
		completions++
		return nil
	}
	_, pools, _, err := a.ReadComplete(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	if len(pools) != 1 || completions != 1 || f.stateReads.Load() != 3 {
		t.Fatal("normal state source flow changed", pools, completions, f.stateReads.Load())
	}
	encoded, _ := json.Marshal(pools[0])
	want := proofCost + wireBytes + len(encoded) + 256
	if withStateCost != want {
		t.Fatalf("state wire/retained cost escaped native session: used=%d want=%d proof=%d wire=%d pool=%d", withStateCost, want, proofCost, wireBytes, len(encoded))
	}
	if a.Client != originalClient || a.Client.HTTP != originalHTTP {
		t.Fatal("state budget mutated the shared source client")
	}
}

func TestNativeStateBudgetChargesFailedProviderAndFailover(t *testing.T) {
	for _, allow := range []bool{true, false} {
		t.Run(map[bool]string{true: "enough", false: "exhausted-on-failover"}[allow], func(t *testing.T) {
			var requests, responses, recorded atomic.Int32
			failed := `{"jsonrpc":"2.0","id":1,"error":{"code":-32000}}`
			success := `{"jsonrpc":"2.0","id":1,"result":"0x1"}`
			serve := func(status int, body string) *httptest.Server {
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					raw, _ := io.ReadAll(r.Body)
					requests.Add(int32(len(raw)))
					responses.Add(int32(len(body)))
					w.WriteHeader(status)
					io.WriteString(w, body)
				}))
				t.Cleanup(server.Close)
				return server
			}
			left, right := serve(503, failed), serve(200, success)
			client := rpc.New([]string{left.URL, right.URL}, time.Second, 4096)
			if err := client.SetPolicy(rpc.Policy{MaxAttempts: 2}); err != nil {
				t.Fatal(err)
			}
			ctx := context.WithValue(context.Background(), nativeDutyKey{}, "state-failover")
			payload, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": "eth_call", "params": []any{}})
			limit := 2*len(payload) + len(failed) + len(success) + 1
			if !allow {
				limit = 2*len(payload) + len(failed) + 1
			}
			session := &nativeReadSession{ctx: ctx, limits: nativeProofTestLimits(), remaining: limit}
			a := Adapter{Client: client, Chain: config.Chain{BlockIntegrity: ChainNativeCommitmentsV2}}
			scope, err := a.nativeStateReader(ctx, RawBlock{nativeSession: session})
			if err != nil {
				t.Fatal(err)
			}
			// Replacing the recorder after copying must still affect the scoped client.
			client.SetRecorder(func(ctx context.Context, _ string, _ string, _ json.RawMessage, _ []byte) error {
				if ctx.Value(nativeDutyKey{}) != "state-failover" {
					t.Fatal("failover lost duty context")
				}
				recorded.Add(1)
				return nil
			})
			var result string
			err = scope.Client.Call(ctx, "eth_call", []any{}, &result)
			if allow {
				if err != nil || result != "0x1" || session.remaining != limit-int(requests.Load()+responses.Load()) || recorded.Load() != 2 {
					t.Fatal("state attempts escaped shared IO budget", err, session.remaining, requests.Load(), responses.Load(), recorded.Load())
				}
			} else if err == nil || session.remaining != 0 || recorded.Load() != 1 {
				t.Fatal("partial over-budget success was published/recorded", err, session.remaining, recorded.Load())
			}
		})
	}
}

func TestNativeNormalExactFinalBudgetBoundary(t *testing.T) {
	for _, tc := range []struct {
		name  string
		limit int
		want  bool
		left  int
	}{
		{"short-one", 90318, false, 0}, {"exact", 90319, true, 0}, {"extra-one", 90320, true, 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			f := newNormalNativeFixture(t, "arbitrum")
			a := f.adapter(t)
			a.Chain.Pools = []config.PoolConfig{nativeBudgetPool()}
			limits := *a.Chain.NativeProof
			limits.MaxBytes = tc.limit
			a.Chain.NativeProof = &limits
			ctx := context.WithValue(context.Background(), nativeDutyKey{}, "exact-state-budget")
			callbacks, left := 0, -1
			a.OnComplete = func(ctx context.Context, b RawBlock, _ []Receipt) error {
				callbacks++
				left = b.nativeSession.remaining
				if ctx.Value(nativeDutyKey{}) != "exact-state-budget" {
					t.Fatal("budget boundary lost responsibility context")
				}
				return nil
			}
			_, pools, _, err := a.ReadComplete(ctx, f.target.Number)
			if !tc.want {
				if err == nil || !strings.Contains(err.Error(), "BUDGET") || callbacks != 0 || len(pools) != 0 {
					t.Fatal("one-byte deficit qualified", err, callbacks, len(pools))
				}
				return
			}
			if err != nil || callbacks != 1 || len(pools) != 1 || left != tc.left {
				t.Fatal("sufficient/exact budget rejected or mischarged", err, callbacks, len(pools), left)
			}
		})
	}
}

func TestNativeSAEFinalRecheckStillRequiresPositiveRemainingBudget(t *testing.T) {
	f := newNormalNativeFixture(t, "avalanche-current")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "sae-exact-budget")
	block, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	block, _, err = a.ReadQualifiedReceipts(ctx, block, true)
	if err != nil {
		t.Fatal(err)
	}
	if block.NativeCoverage.Evidence.SAE == nil {
		t.Fatal("fixture lost successor authority")
	}
	// Allow exactly the mandatory final body proof. SAE still has a real
	// successor read after it, so zero remaining must stop that next operation.
	block.nativeSession.mu.Lock()
	block.nativeSession.remaining = block.NativeBodyProof.ProofBytesUsed
	block.nativeSession.mu.Unlock()
	if err = a.RecheckQualifiedBlock(ctx, block); err == nil || !strings.Contains(err.Error(), "NATIVE_RUNTIME_PROOF_BUDGET") || block.nativeSession.remaining != 0 {
		t.Fatal("zero budget admitted SAE successor work", err, block.nativeSession.remaining)
	}
	// A complete SAE read may itself finish exactly at zero only after the
	// successor work has also been paid, never by skipping its cost.
	total := 0
	a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error {
		total = a.Chain.NativeProof.MaxBytes - b.nativeSession.remaining
		return nil
	}
	if _, _, _, err = a.ReadComplete(ctx, f.target.Number); err != nil {
		t.Fatal(err)
	}
	for _, slack := range []int{-1, 0, 1} {
		limits := *a.Chain.NativeProof
		limits.MaxBytes = total + slack
		a.Chain.NativeProof = &limits
		callbacks, left := 0, -1
		a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error {
			callbacks++
			left = b.nativeSession.remaining
			return nil
		}
		_, _, _, err = a.ReadComplete(ctx, f.target.Number)
		if slack < 0 {
			if err == nil || !strings.Contains(err.Error(), "BUDGET") || callbacks != 0 {
				t.Fatal("SAE deficit was not rejected", err, callbacks)
			}
		} else if err != nil || callbacks != 1 || left != slack {
			t.Fatal("SAE successor cost was not fully charged", slack, err, callbacks, left)
		}
	}
}

func TestNativeArchiveExactFinalBudgetBoundary(t *testing.T) {
	f := newNormalNativeFixture(t, "arbitrum")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "archive-exact-budget")
	block, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	block, _, err = a.ReadQualifiedReceipts(ctx, block, false)
	if err != nil {
		t.Fatal(err)
	}
	evidence := *block.NativeCoverage.Evidence
	verified, err := VerifyNativeReceiptEvidence(ctx, a.Chain.Key, block, evidence, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	total := 2*block.NativeBodyProof.ProofBytesUsed + verified.ProofBytesUsed
	for _, slack := range []int{-1, 0, 1} {
		limits := *a.Chain.NativeProof
		limits.MaxBytes = total + slack
		a.Chain.NativeProof = &limits
		fresh, err := a.Block(ctx, f.target.Number)
		if err != nil {
			t.Fatal(err)
		}
		err = a.RecheckNativeReceiptEvidence(ctx, fresh, evidence)
		if slack < 0 {
			if err == nil || !strings.Contains(err.Error(), "BUDGET") {
				t.Fatal("archived proof deficit was not rejected", err)
			}
		} else if err != nil || fresh.nativeSession.remaining != slack {
			t.Fatal("exact archived proof budget was not honored", slack, err, fresh.nativeSession.remaining)
		}
	}
}
