package evm

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func completeReceiptFixture() (RawBlock, []Receipt) {
	block := RawBlock{Number: "0x64", Hash: "0x" + strings.Repeat("a", 64), ParentHash: "0x" + strings.Repeat("b", 64), Timestamp: "0x64", Transactions: []string{"tx0", "tx1", "tx2"}}
	log := func(tx, index string) Log {
		return Log{BlockHash: block.Hash, TxHash: block.Transactions[0], TxIndex: tx, Index: index, Address: "0x" + strings.Repeat("a", 40), Topics: []string{}, Data: "0x"}
	}
	first := log("0x0", "0x0")
	second := log("0x0", "0x1")
	third := log("0x2", "0x2")
	third.TxHash = "tx2"
	return block, []Receipt{{BlockHash: block.Hash, TxHash: "tx0", TxIndex: "0x0", Logs: []Log{first, second}}, {BlockHash: block.Hash, TxHash: "tx1", TxIndex: "0x1", Logs: []Log{}}, {BlockHash: block.Hash, TxHash: "tx2", TxIndex: "0x2", Logs: []Log{third}}}
}

func TestReceiptIntegrityUsesCanonicalTransactionOrder(t *testing.T) {
	for _, order := range [][]int{{0, 1, 2}, {2, 0, 1}, {1, 2, 0}} {
		block, receipts := completeReceiptFixture()
		input := []Receipt{receipts[order[0]], receipts[order[1]], receipts[order[2]]}
		before, _ := json.Marshal(input)
		if err := ValidateReceipts(block, input); err != nil {
			t.Fatal(order, err)
		}
		after, _ := json.Marshal(input)
		if string(before) != string(after) {
			t.Fatal("validation mutated caller receipts")
		}
	}
	if err := ValidateReceipts(RawBlock{Transactions: []string{}}, []Receipt{}); err != nil {
		t.Fatal(err)
	}
}

func TestReceiptIntegrityRejectsIncompleteOrAmbiguousPositions(t *testing.T) {
	cases := []struct {
		name, want string
		mutate     func(*RawBlock, *[]Receipt)
	}{
		{"missing transaction list", "BLOCK_TRANSACTIONS_MISSING", func(b *RawBlock, r *[]Receipt) { b.Transactions = nil; *r = nil }},
		{"duplicate block transaction", "BLOCK_TRANSACTION_IDENTITY", func(b *RawBlock, r *[]Receipt) { b.Transactions[1] = "tx0" }},
		{"empty transaction hash", "BLOCK_TRANSACTION_IDENTITY", func(b *RawBlock, r *[]Receipt) { b.Transactions[1] = "" }},
		{"missing receipt logs", "RECEIPT_LOGS_MISSING", func(b *RawBlock, r *[]Receipt) { (*r)[1].Logs = nil }},
		{"first log not zero", "LOG_INDEX_GAP", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs[0].Index = "0x3" }},
		{"internal log gap", "LOG_INDEX_GAP", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs[1].Index = "0x3" }},
		{"cross transaction gap", "LOG_INDEX_GAP", func(b *RawBlock, r *[]Receipt) { (*r)[2].Logs[0].Index = "0x3" }},
		{"out of order logs", "LOG_INDEX_GAP", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs[0], (*r)[0].Logs[1] = (*r)[0].Logs[1], (*r)[0].Logs[0] }},
		{"transaction positions reversed", "LOG_INDEX_GAP", func(b *RawBlock, r *[]Receipt) {
			(*r)[0].Logs[0].Index = "0x1"
			(*r)[0].Logs[1].Index = "0x2"
			(*r)[2].Logs[0].Index = "0x0"
		}},
		{"leading zero receipt index", "RECEIPT_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[1].TxIndex = "0x01" }},
		{"leading zero log transaction index", "LOG_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[2].Logs[0].TxIndex = "0x02" }},
		{"leading zero log index", "LOG_INDEX", func(b *RawBlock, r *[]Receipt) { (*r)[2].Logs[0].Index = "0x02" }},
		{"wide log index", "LOG_INDEX", func(b *RawBlock, r *[]Receipt) { (*r)[2].Logs[0].Index = "0x10000000000000000" }},
		{"duplicate log", "DUPLICATE_LOG", func(b *RawBlock, r *[]Receipt) { (*r)[2].Logs[0].Index = "0x1" }},
		{"wrong receipt block", "RECEIPT_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[1].BlockHash = "other" }},
		{"wrong receipt transaction", "RECEIPT_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[1].TxHash = "other" }},
		{"duplicate receipt", "RECEIPT_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[1] = (*r)[0] }},
		{"wrong log block", "LOG_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs[0].BlockHash = "other" }},
		{"removed log", "LOG_IDENTITY", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs[0].Removed = true }},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			block, receipts := completeReceiptFixture()
			tc.mutate(&block, &receipts)
			before, _ := json.Marshal(receipts)
			err := ValidateReceipts(block, receipts)
			if err == nil || err.Error() != tc.want {
				t.Fatalf("got %v, want %s", err, tc.want)
			}
			after, _ := json.Marshal(receipts)
			if string(before) != string(after) {
				t.Fatal("mutated rejected input")
			}
		})
	}
}

func TestReadCompleteRejectsReceiptGapBeforeReadingPoolState(t *testing.T) {
	block, receipts := completeReceiptFixture()
	receipts[2].Logs[0].Index = "0x3"
	stateCalls := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		if json.NewDecoder(r.Body).Decode(&req) != nil {
			t.Error("bad request")
		}
		var result any
		switch req.Method {
		case "eth_getBlockByNumber":
			result = block
		case "eth_getTransactionReceipt":
			var tx string
			json.Unmarshal(req.Params[0], &tx)
			for _, receipt := range receipts {
				if receipt.TxHash == tx {
					result = receipt
				}
			}
		default:
			stateCalls++
			result = "0x0"
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	adapter := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{{Protocol: "uniswap-v3"}}}}
	_, pools, facts, err := adapter.ReadComplete(context.Background(), "0x64")
	if err == nil || err.Error() != "LOG_INDEX_GAP" {
		t.Fatal(err)
	}
	if len(pools) != 0 || len(facts) != 0 || stateCalls != 0 {
		t.Fatal("bad receipts reached state parsing", pools, facts, stateCalls)
	}
}

func TestRPCBlockRequiresExplicitTransactionArray(t *testing.T) {
	for _, field := range []string{"", `,"transactions":null`} {
		t.Run(field, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Write([]byte(`{"jsonrpc":"2.0","id":1,"result":{"number":"0x64","hash":"h","parentHash":"p","timestamp":"0x64"` + field + `}}`))
			}))
			defer server.Close()
			adapter := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20)}
			_, err := adapter.Block(context.Background(), "0x64")
			if err == nil || err.Error() != "BLOCK_TRANSACTIONS_MISSING" {
				t.Fatal(err)
			}
		})
	}
}
