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

func TestCurrentScopeStateSharesOneRetainedPoolBudget(t *testing.T) {
	token0, token1 := "0x0000000000000000000000000000000000000001", "0x0000000000000000000000000000000000000002"
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			Method string
			Params []json.RawMessage
		}
		if json.NewDecoder(r.Body).Decode(&q) != nil {
			t.Error("request decode")
			return
		}
		var result any
		switch q.Method {
		case "eth_getBlockByNumber":
			result = map[string]any{"number": "0x64", "hash": "0xabc", "parentHash": "0xdef", "timestamp": "0x64", "transactions": []string{}}
		case "eth_call":
			var call struct{ Data string }
			_ = json.Unmarshal(q.Params[0], &call)
			switch call.Data {
			case "0x0dfe1681":
				result = fmt.Sprintf("0x%064x", 1)
			case "0xd21220a7":
				result = fmt.Sprintf("0x%064x", 2)
			default:
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			}
		default:
			t.Error("unexpected RPC method", q.Method)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	pools := []config.PoolConfig{}
	for i := 0; i < 64; i++ {
		pools = append(pools, config.PoolConfig{ID: fmt.Sprintf("0x%040x", i+3), Protocol: "uniswap-v2", Token0: token0, Token1: token1, FeeNumerator: 3, FeeDenominator: 1000})
	}
	adapter := Adapter{Client: rpc.New([]string{server.URL}, 5*time.Second, 1024), Chain: config.Chain{Key: "evm:31337", Pools: pools}}
	_, states, err := adapter.ReadState(context.Background(), "0x64", false)
	if err == nil || len(states) != 0 {
		t.Fatalf("unbounded pool state accepted: pools=%d err=%v", len(states), err)
	}
	if err.Error() != "POOL_STATE_BUDGET" {
		t.Fatal("wrong rejection", err)
	}
}
