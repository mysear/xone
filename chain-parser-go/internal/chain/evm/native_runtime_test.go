package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

type nativeDutyKey struct{}
type normalNativeFixture struct {
	chain           config.Chain
	target          RawBlock
	headers         map[string]json.RawMessage
	full            map[string]json.RawMessage
	receipts        map[string][]Receipt
	observed        string
	corruptSettling atomic.Bool
	fullReads       atomic.Int32
	stateReads      atomic.Int32
}

func newNormalNativeFixture(t *testing.T, name string) *normalNativeFixture {
	t.Helper()
	v := loadChainReceiptVector(t, name)
	genesis, _ := knownBodyGenesis(v.ChainKey)
	encoded, _ := json.Marshal(map[string]any{"key": v.ChainKey, "genesis_hash": genesis, "block_integrity": ChainNativeCommitmentsV2, "receipts_mode": "bulk-auto", "native_proof": map[string]any{"max_bytes": 16 << 20, "max_headers": 64, "max_window_blocks": 16, "max_transactions": 4096, "max_lookahead": 32}})
	var chain config.Chain
	if err := json.Unmarshal(encoded, &chain); err != nil {
		t.Fatal(err)
	}
	f := &normalNativeFixture{chain: chain, target: v.Block, headers: map[string]json.RawMessage{v.Block.Number: retainedChainHeader(t, name)}, full: map[string]json.RawMessage{v.Block.Number: retainedFullChainBlock(t, name)}, receipts: map[string][]Receipt{v.Block.Hash: v.Receipts}, observed: v.Block.Number}
	if name == "avalanche-current" {
		_, proof := retainedSAEProof(t)
		for _, raw := range proof.Headers {
			var b RawBlock
			json.Unmarshal(raw, &b)
			f.headers[b.Number] = raw
		}
		f.observed = "0x5f06ad2"
	}
	return f
}
func (f *normalNativeFixture) serve(t *testing.T) *httptest.Server {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Error(err)
			return
		}
		var result any
		switch req.Method {
		case "eth_chainId":
			ids := map[string]string{"evm:43114": "0xa86a", "evm:42161": "0xa4b1", "evm:137": "0x89"}
			result = ids[f.chain.Key]
		case "eth_getBlockByNumber":
			var tag string
			var full bool
			json.Unmarshal(req.Params[0], &tag)
			json.Unmarshal(req.Params[1], &full)
			if tag == "0x0" {
				result = map[string]any{"hash": f.chain.Genesis}
			} else {
				if tag == "latest" || tag == "safe" || tag == "finalized" {
					tag = f.observed
				}
				if full {
					f.fullReads.Add(1)
					result = f.full[tag]
				} else {
					raw := f.headers[tag]
					if f.corruptSettling.Load() && tag == f.observed {
						var h map[string]json.RawMessage
						json.Unmarshal(raw, &h)
						h["stateRoot"], _ = json.Marshal("0x" + strings.Repeat("11", 32))
						raw, _ = json.Marshal(h)
					}
					result = raw
				}
			}
		case "eth_getBlockReceipts":
			var hash string
			json.Unmarshal(req.Params[0], &hash)
			result = f.receipts[hash]
		case "eth_getTransactionReceipt":
			var hash string
			json.Unmarshal(req.Params[0], &hash)
			for _, rs := range f.receipts {
				for _, receipt := range rs {
					if receipt.TxHash == hash {
						result = receipt
					}
				}
			}
		case "eth_call":
			f.stateReads.Add(1)
			var call struct{ Data string }
			json.Unmarshal(req.Params[0], &call)
			switch call.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("a", 40)
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("b", 40)
			default:
				result = "0x" + fmt.Sprintf("%064x%064x%064x", 100, 200, 0)
			}
		default:
			t.Error("unexpected normal native RPC", req.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	t.Cleanup(server.Close)
	return server
}
func (f *normalNativeFixture) adapter(t *testing.T) *Adapter {
	server := f.serve(t)
	a := &Adapter{Client: rpc.New([]string{server.URL}, 15*time.Second, 16<<20), Chain: f.chain}
	a.Client.SetRecorder(func(ctx context.Context, _, _ string, _ json.RawMessage, _ []byte) error {
		if ctx.Value(nativeDutyKey{}) == nil {
			return fmt.Errorf("native duty context lost")
		}
		return nil
	})
	return a
}
func TestNormalNativeReadCompleteUsesActualPublicCommitments(t *testing.T) {
	for _, name := range []string{"avalanche-current", "arbitrum", "polygon-synthetic"} {
		t.Run(name, func(t *testing.T) {
			f := newNormalNativeFixture(t, name)
			a := f.adapter(t)
			called := false
			a.OnComplete = func(ctx context.Context, b RawBlock, receipts []Receipt) error {
				called = true
				if ctx.Value(nativeDutyKey{}) != "baseline" {
					t.Fatal("callback responsibility context changed")
				}
				if b.NativeCoverage == nil || b.NativeCoverage.Evidence == nil || b.CurrentStateAnchor == nil {
					t.Fatal("normal callback lost native authority")
				}
				if name == "avalanche-current" && (b.ObservedHead == nil || b.ObservedHead.Number != f.observed || b.CurrentStateAnchor.Height != 99642059 || b.CurrentStateAnchor.StateRoot == "0x6161d53cfdeb11a266d4f3fb2f31ed97e6ceefce0bbe088f3ecf5d1233c5a3a9") {
					t.Fatal("SAE normal anchor used head/stale state root", b)
				}
				if name == "polygon-synthetic" && (len(receipts) != 389 || len(b.NativeCoverage.SystemObservations) != 1) {
					t.Fatal("system observations leaked into canonical receipts")
				}
				return nil
			}
			ctx := context.WithValue(context.Background(), nativeDutyKey{}, "baseline")
			ref, pools, facts, err := a.ReadComplete(ctx, "latest")
			if err != nil {
				t.Fatal(err)
			}
			if ref.Hash != f.target.Hash || len(pools) != 0 || len(facts) != 0 || !called {
				t.Fatal("normal pipeline did not publish proved execution", ref, called)
			}
			if f.fullReads.Load() != 2 {
				t.Fatal("body proof cache did not avoid target refetch", f.fullReads.Load())
			}
		})
	}
}
func TestNormalNativeRejectsWrongSettlingAndAffectedSystemBeforePoolIO(t *testing.T) {
	for _, fault := range []string{"settling", "affected-system"} {
		t.Run(fault, func(t *testing.T) {
			name := "avalanche-current"
			if fault == "affected-system" {
				name = "polygon-synthetic"
			}
			f := newNormalNativeFixture(t, name)
			f.corruptSettling.Store(fault == "settling")
			a := f.adapter(t)
			if fault == "affected-system" {
				a.Chain.Pools = []config.PoolConfig{{ID: "0x1fdeaf938267ca43388ed1fdb879eaf91e920c7a", Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("a", 40), Token1: "0x" + strings.Repeat("b", 40), FeeNumerator: 997, FeeDenominator: 1000}}
			}
			callbacks := 0
			a.OnComplete = func(context.Context, RawBlock, []Receipt) error { callbacks++; return nil }
			_, _, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "reorg"), "latest")
			if err == nil {
				t.Fatal("normal pipeline admitted bad native proof")
			}
			if callbacks != 0 || f.stateReads.Load() != 0 {
				t.Fatal("unqualified proof reached state/callback")
			}
		})
	}
}
func TestNormalNativeUnrelatedSystemDoesNotGloballyStopProvedScope(t *testing.T) {
	f := newNormalNativeFixture(t, "polygon-synthetic")
	a := f.adapter(t)
	a.Chain.Pools = []config.PoolConfig{{ID: "0x28c96a719edc7c1ba3fb1bafcadf470ace9dfd75", Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("a", 40), Token1: "0x" + strings.Repeat("b", 40), FeeNumerator: 997, FeeDenominator: 1000}}
	_, pools, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "baseline"), "latest")
	if err != nil {
		t.Fatal(err)
	}
	if len(pools) != 1 || f.stateReads.Load() != 3 {
		t.Fatal(pools, f.stateReads.Load())
	}
}
func TestNormalNativeHistoryExactHeightKeepsSuccessorEvidence(t *testing.T) {
	f := newNormalNativeFixture(t, "avalanche-current")
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "avalanche-current", "settlement-multi.json"))
	if err != nil {
		t.Fatal(err)
	}
	var proof SAEExecutionProof
	if err = json.Unmarshal(raw, &proof); err != nil {
		t.Fatal(err)
	}
	for _, h := range proof.Headers {
		var block RawBlock
		json.Unmarshal(h, &block)
		f.headers[block.Number] = h
	}
	for _, window := range proof.Window {
		var fields map[string]json.RawMessage
		json.Unmarshal(window.Header, &fields)
		fields["transactions"], _ = json.Marshal(window.TransactionsJSON)
		full, _ := json.Marshal(fields)
		var block RawBlock
		json.Unmarshal(window.Header, &block)
		f.full[block.Number] = full
		f.receipts[block.Hash] = window.Receipts
	}
	a := f.adapter(t)
	captured := 0
	a.OnHistoryEvidence = func(ctx context.Context, e NativeHistoryEvidence) error {
		captured++
		if ctx.Value(nativeDutyKey{}) != "history" || e.NativeSemanticsID == "" || e.Evidence.SAE == nil || e.Block.Number != "0x5f06acc" {
			t.Fatal("history lost exact native proof", e)
		}
		_, _, err := VerifyNativeHistoryEvidence(ctx, a.Chain, e)
		return err
	}
	batch, _, err := a.ReadFactsAndCatalog(context.WithValue(context.Background(), nativeDutyKey{}, "history"), "0x5f06acc")
	if err != nil {
		t.Fatal(err)
	}
	if len(batch.Blocks) != 1 || batch.Blocks[0].Block.Number != 99642060 || batch.DecoderVersion != "uniswap-v2/1" || captured != 1 {
		t.Fatal("history silently reanchored or lost evidence", batch, captured)
	}
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "baseline")
	if _, _, err := a.ReadState(ctx, "0x5f06acc", false); err == nil || !strings.Contains(err.Error(), "EXECUTION_STATE_TARGET_NOT_DIRECTLY_SETTLED") {
		t.Fatal("state read attributed multiwindow end state to the earlier exact height", err)
	}
	// Named head selects the execution window end, where the successor state
	// root applies, while the exact historical request above stays unchanged.
	f.observed = "0x5f06ad4"
	ref, _, err := a.ReadState(ctx, "latest", false)
	if err != nil || ref.Number != 99642061 {
		t.Fatal("named head failed to select the directly settled window end", ref, err)
	}
}

func TestNormalNativeFinalAnchorRechecksSettlingAuthority(t *testing.T) {
	f := newNormalNativeFixture(t, "avalanche-current")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "reorg")
	b, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	b, _, err = a.ReadQualifiedReceipts(ctx, b, true)
	if err != nil {
		t.Fatal(err)
	}
	// Change only the successor after successful receipt verification; a stable
	// execution target does not make an orphaned settling header canonical.
	f.corruptSettling.Store(true)
	if err = a.RecheckQualifiedBlock(ctx, b); err == nil {
		t.Fatal("final anchor ignored orphaned/tampered settling authority")
	}
}
func TestNormalNativeBudgetCannotResetBetweenBodyReceiptsAndFinalAnchor(t *testing.T) {
	f := newNormalNativeFixture(t, "arbitrum")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "baseline")
	b, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	bodyCost := b.NativeBodyProof.ProofBytesUsed
	b, _, err = a.ReadQualifiedReceipts(ctx, b, true)
	if err != nil {
		t.Fatal(err)
	}
	if b.nativeSession.remaining != a.Chain.NativeProof.MaxBytes-bodyCost-b.NativeCoverage.ProofBytesUsed {
		t.Fatal("native stages did not share an outer budget")
	}
	// Leave enough for the complete initial body but insufficient for receipts.
	limits := *a.Chain.NativeProof
	limits.MaxBytes = bodyCost + 1
	a.Chain.NativeProof = &limits
	called := false
	a.OnComplete = func(context.Context, RawBlock, []Receipt) error { called = true; return nil }
	if _, _, _, err = a.ReadComplete(ctx, f.target.Number); err == nil || called {
		t.Fatal("native budget reset at receipt boundary", err, called)
	}
	b.nativeSession.remaining = 1
	if err = a.RecheckQualifiedBlock(ctx, b); err == nil {
		t.Fatal("final anchor bypassed the outer native budget")
	}
}

func TestNativeArchivedProofRechecksSuccessorCanonicality(t *testing.T) {
	f := newNormalNativeFixture(t, "avalanche-current")
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "replay")
	b, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	b, _, err = a.ReadQualifiedReceipts(ctx, b, false)
	if err != nil {
		t.Fatal(err)
	}
	evidence := b.NativeCoverage.Evidence
	f.corruptSettling.Store(true)
	current, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	if err = a.RecheckNativeReceiptEvidence(ctx, current, *evidence); err == nil {
		t.Fatal("archived successor accepted after only the execution target was reanchored")
	}
}
