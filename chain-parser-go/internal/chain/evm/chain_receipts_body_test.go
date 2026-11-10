package evm

import (
	"context"
	"encoding/json"
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

func TestChainBodyCommitmentsPublicBSC(t *testing.T) { testChainBodyCommitmentsPublic(t, "bsc") }
func TestChainBodyCommitmentsPublicOP(t *testing.T) {
	testChainBodyCommitmentsPublic(t, "optimism-regolith")
}
func TestChainBodyCommitmentsPublicNative(t *testing.T) {
	for _, name := range []string{"avalanche-current"} {
		t.Run(name, func(t *testing.T) { testChainBodyCommitmentsPublic(t, name) })
	}
}
func testChainBodyCommitmentsPublic(t *testing.T, name string) {
	for _, fault := range []string{"none", "coordinated-txhash", "raw-tamper", "drop-tail", "header-root", "raw-unsupported", "fingerprint", "budget", "aggregate-budget", "chain-id"} {
		t.Run(fault, func(t *testing.T) {
			if name != "bsc" && fault == "aggregate-budget" {
				t.Skip("aggregate exhaustion exercised by 79-transaction BSC vector")
			}
			if name == "avalanche-current" && fault == "coordinated-txhash" {
				t.Skip("receipt-only old profile is unsupported before SAE settlement proof")
			}
			v := loadChainReceiptVector(t, name)
			wantTransactions := len(v.Block.Transactions)
			raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, name, "vector.json"))
			if err != nil {
				t.Fatal(err)
			}
			var vector struct{ Block map[string]any }
			if err = json.Unmarshal(raw, &vector); err != nil {
				t.Fatal(err)
			}
			raw, err = os.ReadFile(filepath.Join(chainReceiptVectors, name, "body.json"))
			if err != nil {
				t.Fatal(err)
			}
			var source struct {
				GenesisHash     string `json:"genesis_hash"`
				RawTransactions []struct {
					Request  struct{ Params []string }
					Response struct{ Result string }
				} `json:"raw_transactions"`
			}
			if err = json.Unmarshal(raw, &source); err != nil {
				t.Fatal(err)
			}
			transactions := map[string]string{}
			for _, record := range source.RawTransactions {
				transactions[record.Request.Params[0]] = record.Response.Result
			}
			if len(transactions) != wantTransactions {
				t.Fatal("incomplete public raw transaction vector", len(transactions))
			}
			if fault == "coordinated-txhash" {
				// Receipt RLP does not commit transactionHash. Demonstrate the original
				// trust gap with this retained public block before exercising the body proof.
				fake := "0x" + strings.Repeat("ac", 32)
				original := v.Block.Transactions[0]
				v.Block.Transactions[0] = fake
				v.Receipts[0].TxHash = fake
				for i := range v.Receipts[0].Logs {
					v.Receipts[0].Logs[i].TxHash = fake
				}
				if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts); err != nil {
					t.Fatal("receipt-only attack demonstration failed", err)
				}
				vector.Block["transactions"] = v.Block.Transactions
				transactions[fake] = transactions[original]
			}
			if fault == "drop-tail" {
				vector.Block["transactions"] = v.Block.Transactions[:len(v.Block.Transactions)-1]
			}
			if fault == "header-root" {
				vector.Block["transactionsRoot"] = "0x" + strings.Repeat("01", 32)
			}
			var rawCalls atomic.Int32
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
					switch v.ChainKey {
					case "evm:56":
						result = "0x38"
					case "evm:10":
						result = "0xa"
					case "evm:42161":
						result = "0xa4b1"
					case "evm:43114":
						result = "0xa86a"
					}
					if fault == "chain-id" {
						result = "0x1"
					}
				case "eth_getBlockByNumber":
					var tag string
					json.Unmarshal(req.Params[0], &tag)
					if tag == "0x0" {
						result = map[string]any{"hash": source.GenesisHash}
					} else {
						result = vector.Block
					}
				case "eth_getRawTransactionByHash":
					rawCalls.Add(1)
					if fault == "raw-unsupported" {
						w.Write([]byte(`{"jsonrpc":"2.0","id":1,"error":{"code":-32601}}`))
						return
					}
					var tx string
					json.Unmarshal(req.Params[0], &tx)
					result = transactions[tx]
					if fault == "raw-tamper" {
						result = "0x01"
					}
				default:
					t.Error("unexpected body RPC", req.Method)
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
			}))
			defer server.Close()
			fingerprint := source.GenesisHash
			if fault == "fingerprint" {
				fingerprint = "0x" + strings.Repeat("01", 32)
			}
			budget := 8 << 20
			if fault == "budget" {
				budget = 2048
			}
			if fault == "aggregate-budget" {
				budget = 50000
			}
			a := Adapter{Client: rpc.New([]string{server.URL}, 5*time.Second, budget), Chain: config.Chain{Key: v.ChainKey, Genesis: fingerprint, BlockIntegrity: ChainBodyCommitmentsV1}}
			block, err := a.ReadChainCommittedBlock(context.Background(), v.Block.Number)
			if fault == "none" {
				if err != nil {
					t.Fatal(err)
				}
				if block.Hash != v.Block.Hash || len(block.Transactions) != wantTransactions {
					t.Fatal(block)
				}
				if int(rawCalls.Load()) != wantTransactions {
					t.Fatal(rawCalls.Load())
				}
			} else if err == nil {
				t.Fatal("accepted body proof fault", fault)
			}
			if fault == "aggregate-budget" && (rawCalls.Load() < 2 || int(rawCalls.Load()) >= wantTransactions) {
				t.Fatal("test must exhaust the aggregate, after several individually bounded raw responses", rawCalls.Load(), err)
			}
		})
	}
}
func TestChainBodyCommitmentsExplicitProfilesAndFingerprint(t *testing.T) {
	for _, key := range []string{"evm:42161", "evm:43114", "evm:9999"} {
		a := Adapter{Chain: config.Chain{Key: key, BlockIntegrity: ChainBodyCommitmentsV1}}
		if _, err := a.ReadChainCommittedBlock(context.Background(), "0x1"); err == nil {
			t.Fatal("missing fingerprint must fail closed", key)
		}
	}
	a := Adapter{Chain: config.Chain{Key: "evm:56"}}
	if _, err := a.ReadChainCommittedBlock(context.Background(), "0x1"); err == nil {
		t.Fatal("profile requires explicit opt-in")
	}
}
