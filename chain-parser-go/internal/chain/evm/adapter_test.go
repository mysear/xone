package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestHashPinnedBaselineAndIncompleteReceipts(t *testing.T) {
	var pinned bool
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		json.NewDecoder(r.Body).Decode(&q)
		var out any
		switch q.Method {
		case "eth_getBlockByNumber":
			out = map[string]any{"number": "0x64", "hash": "0xabc", "parentHash": "0xdef", "timestamp": "0x64", "transactions": []string{}}
		case "eth_call":
			var anchor map[string]any
			json.Unmarshal(q.Params[1], &anchor)
			pinned = anchor["blockHash"] == "0xabc" && anchor["requireCanonical"] == true
			var call map[string]any
			json.Unmarshal(q.Params[0], &call)
			switch call["data"] {
			case "0x0dfe1681":
				out = "0x" + fmt.Sprintf("%064x", 1)
			case "0xd21220a7":
				out = "0x" + fmt.Sprintf("%064x", 2)
			default:
				out = "0x" + fmt.Sprintf("%064x%064x%064x", 1000000, 1000000, 0)
			}
		default:
			t.Errorf("unexpected %s", q.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": out})
	}))
	defer server.Close()
	a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{{ID: "0x0000000000000000000000000000000000000003", Protocol: "uniswap-v2", Token0: "0x0000000000000000000000000000000000000001", Token1: "0x0000000000000000000000000000000000000002", FeeNumerator: 3, FeeDenominator: 1000}}}}
	b, p, e := a.ReadState(context.Background(), "latest", false)
	if e != nil || !pinned || len(p) != 1 || b.Number != 100 {
		t.Fatal(b, p, e, pinned)
	}
	if e = ValidateReceipts(RawBlock{Hash: "h", Transactions: []string{"tx"}}, nil); e == nil {
		t.Fatal("partial block accepted")
	}
}
