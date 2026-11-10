package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestReceiptBulkModesPublicBlock(t *testing.T) {
	for _, mode := range []string{"", "per-tx", "bulk-auto"} {
		t.Run(mode, func(t *testing.T) {
			v := loadChainReceiptVector(t, "polygon")
			// Encode the 564-receipt fixture before the RPC deadline starts. This
			// checks bulk selection and hash pinning, not race-instrumented JSON
			// encoding throughput on the server goroutine.
			encode := func(result any) []byte {
				body, err := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
				if err != nil {
					t.Fatal(err)
				}
				return body
			}
			bulkBody := encode(v.Receipts)
			perTxBodies := make(map[string][]byte, len(v.Receipts))
			for _, receipt := range v.Receipts {
				perTxBodies[receipt.TxHash] = encode(receipt)
			}
			var bulk, perTx atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var req struct {
					Method string
					Params []json.RawMessage
				}
				json.NewDecoder(r.Body).Decode(&req)
				var body []byte
				switch req.Method {
				case "eth_getBlockReceipts":
					bulk.Add(1)
					var hash string
					json.Unmarshal(req.Params[0], &hash)
					if hash != v.Block.Hash {
						t.Error("bulk must pin block hash", hash)
					}
					body = bulkBody
				case "eth_getTransactionReceipt":
					perTx.Add(1)
					var hash string
					json.Unmarshal(req.Params[0], &hash)
					body = perTxBodies[hash]
					if body == nil {
						t.Error("unknown transaction", hash)
					}
				default:
					t.Error("unexpected RPC", req.Method)
				}
				_, _ = w.Write(body)
			}))
			defer server.Close()
			// A full public block is a multi-megabyte response under -race.
			// Deadline and cancellation behavior have separate focused cases.
			a := Adapter{Client: rpc.New([]string{server.URL}, 10*time.Second, 8<<20), Chain: config.Chain{Key: v.ChainKey}}
			rs, err := a.ReadReceiptsWithMode(context.Background(), v.Block, mode)
			if err != nil {
				t.Fatal(err)
			}
			if len(rs) != 564 {
				t.Fatal("public fixture changed", len(rs))
			}
			if mode == "bulk-auto" {
				if bulk.Load() != 1 || perTx.Load() != 0 {
					t.Fatal(bulk.Load(), perTx.Load())
				}
			} else if bulk.Load() != 0 || perTx.Load() != 564 {
				t.Fatal(bulk.Load(), perTx.Load())
			}
		})
	}
}
func TestReceiptBulkFallbackOnlyUnsupported(t *testing.T) {
	for _, fault := range []string{"unsupported", "invalid-params", "server-error", "http-error", "null", "empty", "identity", "root", "duplicate", "missing", "budget", "non-array"} {
		t.Run(fault, func(t *testing.T) {
			v := loadChainReceiptVector(t, "base-canyon")
			var perTx atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var req struct {
					Method string
					Params []json.RawMessage
				}
				json.NewDecoder(r.Body).Decode(&req)
				if req.Method == "eth_getTransactionReceipt" {
					perTx.Add(1)
					var hash string
					json.Unmarshal(req.Params[0], &hash)
					for _, receipt := range v.Receipts {
						if receipt.TxHash == hash {
							json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": receipt})
							return
						}
					}
					t.Error("unknown tx")
					return
				}
				if req.Method != "eth_getBlockReceipts" {
					t.Error(req.Method)
					return
				}
				switch fault {
				case "unsupported", "invalid-params", "server-error":
					code := map[string]int{"unsupported": -32601, "invalid-params": -32602, "server-error": -32603}[fault]
					fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"error":{"code":%d}}`, code)
					return
				case "http-error":
					w.WriteHeader(503)
					return
				}
				receipts := append([]Receipt(nil), v.Receipts...)
				var result any = receipts
				switch fault {
				case "null":
					result = nil
				case "empty":
					result = []Receipt{}
				case "identity":
					receipts[0].BlockHash = "other-block"
				case "root":
					receipts[0].Status = "0x0"
				case "duplicate":
					receipts[1] = receipts[0]
				case "missing":
					result = receipts[:len(receipts)-1]
				case "non-array":
					result = map[string]any{}
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
			}))
			defer server.Close()
			budget := 8 << 20
			if fault == "budget" {
				budget = 2048
			}
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, budget), Chain: config.Chain{Key: v.ChainKey}}
			_, err := a.ReadReceiptsWithMode(context.Background(), v.Block, "bulk-auto")
			if fault == "unsupported" {
				if err != nil || int(perTx.Load()) != len(v.Receipts) {
					t.Fatal("unsupported fallback", err, perTx.Load())
				}
			} else if err == nil || perTx.Load() != 0 {
				t.Fatal("must not fallback from invalid response", err, perTx.Load())
			}
		})
	}
}
func TestReceiptBulkInvalidModeDoesNoIO(t *testing.T) {
	a := Adapter{}
	if _, err := a.ReadReceiptsWithMode(context.Background(), RawBlock{}, "auto"); err == nil {
		t.Fatal("invalid mode")
	}
}

func TestChainReceiptsWitnessUsesNativeProfile(t *testing.T) {
	v := loadChainReceiptVector(t, "base-canyon")
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
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
		case "eth_getBlockByNumber":
			result = v.Block
		case "eth_getTransactionReceipt":
			var tx string
			json.Unmarshal(req.Params[0], &tx)
			for _, receipt := range v.Receipts {
				if receipt.TxHash == tx {
					result = receipt
					break
				}
			}
		default:
			t.Error("unexpected RPC", req.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	})
	left := httptest.NewServer(handler)
	defer left.Close()
	right := httptest.NewServer(handler)
	defer right.Close()
	a := Adapter{Client: rpc.New([]string{left.URL, right.URL}, time.Second, 8<<20), Chain: config.Chain{Key: v.ChainKey}}
	if err := a.verifyReceiptWitnesses(context.Background(), v.Block, v.Receipts); err != nil {
		t.Fatal(err)
	}
}
