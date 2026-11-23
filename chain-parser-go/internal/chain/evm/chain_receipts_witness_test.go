package evm

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"slices"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestNativeWitnessReprovesCurrentSAEInsteadOfOwnHeader(t *testing.T) {
	target, proof := retainedSAEProof(t)
	expected, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	full := retainedFullChainBlock(t, "avalanche-current")
	headers := map[string]json.RawMessage{}
	for _, h := range proof.Headers {
		var b RawBlock
		json.Unmarshal(h, &b)
		headers[b.Number] = h
	}
	var settledReads sync.Map
	var mutateAfterReceipt, switchChain atomic.Bool
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result any
		switch req.Method {
		case "eth_chainId":
			result = "0xa86a"
		case "eth_getBlockByNumber":
			var tag string
			var fullBody bool
			json.Unmarshal(req.Params[0], &tag)
			json.Unmarshal(req.Params[1], &fullBody)
			if tag == "0x0" {
				result = map[string]any{"hash": proof.GenesisHash}
				if _, read := settledReads.Load(r.Host); read && switchChain.Load() {
					result = map[string]any{"hash": target.Hash}
				}
			} else if tag == target.Number && fullBody {
				result = full
			} else {
				result = headers[tag]
				if _, orphan := settledReads.Load(r.Host); orphan && tag == "0x5f06ad2" {
					var fields map[string]json.RawMessage
					json.Unmarshal(headers[tag], &fields)
					fields["stateRoot"] = json.RawMessage(`"0x1111111111111111111111111111111111111111111111111111111111111111"`)
					result = fields
				}
			}
		case "eth_getBlockReceipts":
			result = proof.Window[0].Receipts
			if mutateAfterReceipt.Load() {
				settledReads.Store(r.Host, true)
			}
		default:
			t.Error(req.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	})
	left := httptest.NewServer(handler)
	defer left.Close()
	right := httptest.NewServer(handler)
	defer right.Close()
	a := Adapter{Client: rpc.New([]string{left.URL, right.URL}, 10*time.Second, 16<<20), Chain: config.Chain{Key: "evm:43114", Genesis: proof.GenesisHash, BlockIntegrity: ChainNativeCommitmentsV2}}
	if err := a.VerifyNativeReceiptWitnesses(context.Background(), expected, "bulk-auto", nativeProofTestLimits()); err != nil {
		t.Fatal(err)
	}
	mutateAfterReceipt.Store(true)
	if err := a.VerifyNativeReceiptWitnesses(context.Background(), expected, "bulk-auto", nativeProofTestLimits()); err == nil {
		t.Fatal("witness final check ignored orphaned settling header")
	}
	settledReads.Clear()
	switchChain.Store(true)
	err = a.VerifyNativeReceiptWitnesses(context.Background(), expected, "bulk-auto", nativeProofTestLimits())
	var detail *NativeWitnessError
	if !errors.As(err, &detail) || detail.Phase != "body_recheck" {
		t.Fatalf("genesis changed after receipts: %#v", err)
	}

}

func TestEthereumWitnessReusesOnlyItsOwnBody(t *testing.T) {
	header, err := json.Marshal(newV2ProofBank(t, "").header)
	if err != nil {
		t.Fatal(err)
	}
	var block RawBlock
	if err := json.Unmarshal(header, &block); err != nil {
		t.Fatal(err)
	}
	genesis, err := knownBodyGenesis("evm:1")
	if err != nil {
		t.Fatal(err)
	}
	fixture := func() *normalNativeFixture {
		return &normalNativeFixture{chain: config.Chain{Key: "evm:1", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2}, target: block, full: map[string]json.RawMessage{block.Number: header}, receipts: map[string][]Receipt{block.Hash: {}}}
	}
	left, right := fixture(), fixture()
	leftURL, rightURL := left.serve(t).URL, right.serve(t).URL
	ctx := context.Background()
	a := Adapter{Client: rpc.New([]string{leftURL}, 10*time.Second, 16<<20), Chain: left.chain}
	limits := nativeProofTestLimits()
	expected, err := a.ReadNativeReceiptCoverage(ctx, left.target, "bulk-auto", limits)
	if err != nil {
		t.Fatal(err)
	}
	a.Client = rpc.New([]string{leftURL, rightURL}, 10*time.Second, 16<<20)
	before := left.fullReads.Load()
	if err = a.VerifyNativeReceiptWitnesses(ctx, expected, "bulk-auto", limits); err != nil {
		t.Fatal(err)
	}
	if left.fullReads.Load()-before != 2 || right.fullReads.Load() != 2 {
		t.Fatalf("expected one body and one recheck per witness: %d, %d", left.fullReads.Load()-before, right.fullReads.Load())
	}
	right.receipts[right.target.Hash] = nil
	err = a.VerifyNativeReceiptWitnesses(ctx, expected, "bulk-auto", limits)
	var detail *NativeWitnessError
	if !errors.As(err, &detail) || detail.Error() != "RECEIPT_WITNESS_INVALID" || detail.Phase != "receipts" || detail.WitnessIndex != 1 || detail.HeightTag != block.Number || detail.Class != rpc.InvalidResponse && detail.Class != rpc.Unavailable {
		t.Fatalf("unexpected missing-receipts diagnostic: %#v", err)
	}
}

func TestNativeWitnessCallOrderAndSharedBudget(t *testing.T) {
	for _, name := range []string{"arbitrum", "avalanche-current", "polygon-synthetic"} {
		t.Run(name, func(t *testing.T) {
			left, right := newNormalNativeFixture(t, name), newNormalNativeFixture(t, name)
			leftURL, rightURL := left.serve(t).URL, right.serve(t).URL
			ctx, limits := context.Background(), nativeProofTestLimits()
			limits.MaxBytes = 64 << 20
			a := Adapter{Client: rpc.New([]string{leftURL}, 30*time.Second, 16<<20), Chain: left.chain}
			expected, err := a.ReadNativeReceiptCoverage(ctx, left.target, "bulk-auto", limits)
			if err != nil {
				t.Fatal(err)
			}
			a.Client = rpc.New([]string{leftURL, rightURL}, 30*time.Second, 16<<20)
			calls := make(map[string][]string)
			a.Client.SetRecorder(func(_ context.Context, source, method string, request json.RawMessage, _ []byte) error {
				if method == "eth_getBlockByNumber" {
					var params []json.RawMessage
					if err := json.Unmarshal(request, &params); err != nil {
						return err
					}
					switch {
					case string(params[0]) == `"0x0"`:
						method = "genesis"
					case string(params[1]) == "true":
						method = "body"
					default:
						method = "header"
					}
				}
				calls[source] = append(calls[source], method)
				return nil
			})
			used, err := a.VerifyNativeReceiptWitnessesUsage(ctx, expected, "bulk-auto", limits)
			if err != nil || used <= 0 {
				t.Fatal(used, err)
			}
			want := []string{"eth_chainId", "genesis", "body"}
			if expected.Evidence.SAE != nil {
				for range expected.Evidence.SAE.Headers[1:] {
					want = append(want, "header")
				}
			}
			want = append(want, "eth_getBlockReceipts", "eth_chainId", "genesis", "body")
			if name == "avalanche-current" {
				want = append(want, "header")
			}
			if len(calls) != 2 {
				t.Fatal("distinct recorded witnesses", calls)
			}
			for source, actual := range calls {
				if !slices.Equal(actual, want) {
					t.Fatalf("source %s: %v, want %v", source, actual, want)
				}
			}
			limits.MaxBytes = used - 1
			if _, err = a.VerifyNativeReceiptWitnessesUsage(ctx, expected, "bulk-auto", limits); err == nil {
				t.Fatal("witnesses reused the allowance consumed by an earlier phase")
			}
		})
	}
}

func TestNativeWitnessFailureClassification(t *testing.T) {
	for _, tc := range []struct {
		err   error
		class rpc.ErrorClass
	}{
		{errors.New("RECEIPT_PROOF_BUDGET"), rpc.BudgetExceeded},
		{errors.New("RPC_HTTP_429"), rpc.RateLimited},
		{context.DeadlineExceeded, rpc.Deadline},
		{context.Canceled, rpc.Canceled},
		{errors.New("RECEIPTS_ROOT_MISMATCH"), rpc.InvalidResponse},
	} {
		got := witnessFailure("RECEIPT_WITNESS_INVALID", "receipts", 1, "0x01", tc.err)
		if got.Class != tc.class || got.Error() != "RECEIPT_WITNESS_INVALID" {
			t.Fatalf("%v: %#v", tc.err, got)
		}
	}
}
