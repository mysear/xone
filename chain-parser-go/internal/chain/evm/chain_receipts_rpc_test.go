package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestNativeCoveragePublicRPCReplay(t *testing.T) {
	for _, name := range []string{"avalanche-current", "arbitrum", "polygon-synthetic"} {
		t.Run(name, func(t *testing.T) {
			v := loadChainReceiptVector(t, name)
			full := retainedFullChainBlock(t, name)
			genesis, _ := knownBodyGenesis(v.ChainKey)
			headers := map[string]json.RawMessage{v.Block.Number: retainedChainHeader(t, name)}
			if name == "avalanche-current" {
				_, proof := retainedSAEProof(t)
				for _, raw := range proof.Headers {
					var b RawBlock
					json.Unmarshal(raw, &b)
					headers[b.Number] = raw
				}
			}
			var calls atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var request struct {
					Method string
					Params []json.RawMessage
				}
				json.NewDecoder(r.Body).Decode(&request)
				calls.Add(1)
				var result any
				switch request.Method {
				case "eth_chainId":
					id := map[string]string{"evm:43114": "0xa86a", "evm:42161": "0xa4b1", "evm:137": "0x89"}
					result = id[v.ChainKey]
				case "eth_getBlockByNumber":
					var tag string
					var body bool
					json.Unmarshal(request.Params[0], &tag)
					json.Unmarshal(request.Params[1], &body)
					if tag == "latest" && name == "avalanche-current" {
						result = headers["0x5f06ad2"]
					} else if tag == "0x0" {
						result = map[string]any{"hash": genesis}
					} else if body && tag == v.Block.Number {
						result = full
					} else {
						result = headers[tag]
						if result == nil {
							t.Error("unbounded/unexpected header", tag)
						}
					}
				case "eth_getBlockReceipts":
					result = v.Receipts
				default:
					t.Error("unexpected native RPC", request.Method)
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
			}))
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, 15*time.Second, 16<<20), Chain: config.Chain{Key: v.ChainKey, Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto"}}
			if name == "avalanche-current" {
				settled, e := a.ReadNativeSettledBlockProof(context.Background(), "latest", nativeProofTestLimits())
				if e != nil {
					t.Fatal(e)
				}
				if settled.Block.Number != v.Block.Number || settled.ObservedHead == nil || settled.ObservedHead.Number != "0x5f06ad2" {
					t.Fatal("normal SAE head did not explicitly select settled execution", settled)
				}
			}
			bodyProof, err := a.ReadNativeBlockProof(context.Background(), v.Block.Number, nativeProofTestLimits())
			if err != nil {
				t.Fatal(err)
			}
			if bodyProof.Block.Hash != v.Block.Hash || len(bodyProof.RawTransactions) == 0 {
				t.Fatal(bodyProof)
			}
			if name == "polygon-synthetic" && (len(bodyProof.UncommittedSystemTransactionIDs) != 1 || len(bodyProof.CanonicalBlock.Transactions) != 389) {
				t.Fatal("lost explicit system body isolation", bodyProof)
			}
			beforeCoverage := calls.Load()
			coverage, err := a.ReadNativeReceiptCoverage(context.Background(), v.Block, "bulk-auto", nativeProofTestLimits())
			if err != nil {
				t.Fatal(err)
			}
			if !coverage.CanonicalBodyProven || !coverage.CanonicalReceiptsProven {
				t.Fatal(coverage)
			}
			if name == "polygon-synthetic" {
				if coverage.WholeBlockReceiptEventsProven || len(coverage.SystemObservations) != 1 {
					t.Fatal(coverage)
				}
			} else if err := coverage.RequireWholeBlockEvents(); err != nil {
				t.Fatal(err)
			}
			if calls.Load()-beforeCoverage > 16 {
				t.Fatal("unexpected RPC amplification", calls.Load())
			}
		})
	}
}
func TestNativeCoverageRPCBoundedLookahead(t *testing.T) {
	v := loadChainReceiptVector(t, "avalanche-current")
	_, proof := retainedSAEProof(t)
	byNumber := map[string]json.RawMessage{}
	for _, raw := range proof.Headers {
		var block RawBlock
		json.Unmarshal(raw, &block)
		byNumber[block.Number] = raw
	}
	genesis, _ := knownBodyGenesis(v.ChainKey)
	var calls atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var request struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&request)
		calls.Add(1)
		var result any
		switch request.Method {
		case "eth_chainId":
			result = "0xa86a"
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(request.Params[0], &tag)
			if tag == "0x0" {
				result = map[string]any{"hash": genesis}
			} else if tag == v.Block.Number {
				result = retainedFullChainBlock(t, "avalanche-current")
			} else {
				result = byNumber[tag]
			}
		default:
			t.Error(request.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 16<<20), Chain: config.Chain{Key: v.ChainKey, Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2}}
	limits := nativeProofTestLimits()
	limits.MaxLookahead = 2
	if _, err := a.ReadNativeReceiptCoverage(context.Background(), v.Block, "bulk-auto", limits); err == nil {
		t.Fatal("accepted absent settling proof")
	}
	if calls.Load() > 5 {
		t.Fatal(fmt.Sprintf("lookahead exceeded budget: %d", calls.Load()))
	}
}

func TestNativePublicAccountProofRPCReplay(t *testing.T) {
	target, proof, state := retainedFreshSAEProof(t)
	coverage, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	genesis, _ := knownBodyGenesis("evm:43114")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var request struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&request)
		var result any
		switch request.Method {
		case "eth_chainId":
			result = "0xa86a"
		case "eth_getBlockByNumber":
			result = map[string]any{"hash": genesis}
		case "eth_getProof":
			var anchor struct {
				BlockHash        string
				RequireCanonical bool
			}
			json.Unmarshal(request.Params[2], &anchor)
			if anchor.BlockHash != target.Hash || !anchor.RequireCanonical {
				t.Error("state query not pinned to proven execution", anchor)
			}
			result = state
		default:
			t.Error(request.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 16<<20), Chain: config.Chain{Key: "evm:43114", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2}}
	account, err := a.ReadNativeAccountProof(context.Background(), coverage, "0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7", []string{}, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if !account.Exists || account.HeldXChainAsset == nil {
		t.Fatal(account)
	}
}

func TestNativeReaderChargesUnsupportedResponseBytes(t *testing.T) {
	genesis, _ := knownBodyGenesis("evm:43114")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct{ Method string }
		json.NewDecoder(r.Body).Decode(&req)
		var response map[string]any
		switch req.Method {
		case "eth_chainId":
			response = map[string]any{"result": "0xa86a"}
		case "eth_getBlockByNumber":
			response = map[string]any{"result": map[string]any{"hash": genesis}}
		default:
			response = map[string]any{"error": map[string]any{"code": -32601, "message": strings.Repeat("x", 8192)}}
		}
		response["jsonrpc"] = "2.0"
		response["id"] = 1
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()
	a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:43114", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2}}
	limits := nativeProofTestLimits()
	limits.MaxBytes = 128 << 10
	reader, err := newNativeProofReader(context.Background(), &a, limits)
	if err != nil {
		t.Fatal(err)
	}
	defer reader.cancel()
	before := reader.budget.remaining
	if _, err = reader.call("eth_getBlockReceipts", []any{"0x" + strings.Repeat("11", 32)}); err == nil {
		t.Fatal("missing unsupported error")
	}
	if before-reader.budget.remaining < 8192 {
		t.Fatal("error response bypassed total budget", before-reader.budget.remaining)
	}
}
