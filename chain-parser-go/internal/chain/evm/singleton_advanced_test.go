package evm

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestSingletonAdvancedReadFrozenDynamicNativeSnapshot(t *testing.T) {
	raw, err := os.ReadFile("../../../test/contracts/singleton-advanced-quotes.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Cases []struct {
			Snapshot domain.SingletonAdvancedState `json:"snapshot"`
		} `json:"cases"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	for _, protocol := range []string{"uniswap-v4", "pancake-infinity-cl"} {
		t.Run(protocol, func(t *testing.T) {
			var state domain.SingletonAdvancedState
			for _, c := range fixture.Cases {
				if c.Snapshot.Protocol == protocol && c.Snapshot.Currency0 == domain.SingletonZeroAddress && c.Snapshot.KeyFee == domain.SingletonDynamicFeeFlag && c.Snapshot.Snapshot.LPFee == 3000 {
					state = c.Snapshot
					break
				}
			}
			// Translate an existing independently encoded RPC layout to the freshly frozen PoolKey.
			f := singletonFixture(protocol)
			oldID := f.config.SingletonCL.PoolID
			newID := state.Snapshot.PoolID
			next := map[string]string{}
			if protocol == "uniswap-v4" {
				oldBase := singletonTestMapping(oldID[2:], singletonTestInt(6))
				newBase := singletonTestMapping(newID[2:], singletonTestInt(6))
				replacements := map[string]string{oldBase: newBase, singletonTestOffset(oldBase, 3): singletonTestOffset(newBase, 3)}
				for _, pos := range []int64{-2, -1, 0} {
					replacements[singletonTestMapping(singletonTestInt(pos), singletonTestOffset(oldBase, 5))] = singletonTestMapping(singletonTestInt(pos), singletonTestOffset(newBase, 5))
				}
				for _, tick := range []int64{-60, 60} {
					replacements[singletonTestMapping(singletonTestInt(tick), singletonTestOffset(oldBase, 4))] = singletonTestMapping(singletonTestInt(tick), singletonTestOffset(newBase, 4))
				}
				for k, v := range f.results {
					for old, new := range replacements {
						if strings.HasSuffix(k, old) {
							next[k[:len(k)-len(old)]+new] = v
						}
					}
				}
			} else {
				for k, v := range f.results {
					next[strings.ReplaceAll(k, oldID[2:], newID[2:])] = v
				}
				word := func(a string) string { return strings.Repeat("0", 24) + a[2:] }
				next[singletonTestSelector("poolIdToPoolKey(bytes32)")+newID[2:]] = "0x" + word(state.Currency0) + word(state.Currency1) + word(state.Snapshot.Hooks) + word(state.Snapshot.Manager) + singletonTestInt(int64(state.KeyFee)) + state.Snapshot.Parameters[2:]
				next[singletonTestSelector("getHooksRegistrationBitmap()")] = "0x" + singletonTestInt(0)
			}
			f.results = next
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var req struct {
					ID     json.RawMessage   `json:"id"`
					Method string            `json:"method"`
					Params []json.RawMessage `json:"params"`
				}
				_ = json.NewDecoder(r.Body).Decode(&req)
				if req.Method == "eth_getBlockByNumber" {
					result := map[string]any{"number": "0x64", "hash": state.SnapshotBlockHash, "parentHash": "0x" + strings.Repeat("bc", 32), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": req.ID, "result": result})
					return
				}

				var anchor map[string]any
				_ = json.Unmarshal(req.Params[1], &anchor)
				if anchor["blockHash"] != state.SnapshotBlockHash || anchor["requireCanonical"] != true {
					t.Error("unanchored read")
				}
				result := f.code
				if req.Method == "eth_call" {
					var call map[string]string
					_ = json.Unmarshal(req.Params[0], &call)
					var ok bool
					result, ok = f.results[call["data"]]
					if !ok {
						t.Errorf("unknown call %s", call["data"])
					}
				}
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": req.ID, "result": result})
			}))
			defer server.Close()
			client := rpc.New([]string{server.URL}, time.Second, 1<<20)
			adapter := Adapter{Client: client, Chain: config.Chain{Key: "evm:31337"}}
			state.Snapshot.ManagerCodeSHA256 = f.config.SingletonCL.ExpectedManagerCodeSHA256
			state.Snapshot.LPFee = 0
			state.Snapshot.ProtocolFee0 = 0
			state.Snapshot.ProtocolFee1 = 0
			state.Snapshot.Core.SqrtPriceX96 = "0"
			state.Snapshot.Core.Liquidity = "0"
			state.Snapshot.Core.TickWords = nil
			state.Snapshot.Core.Ticks = nil
			req := SingletonAdvancedReadConfig{State: state, TickWordRadius: 1, MaxInitializedTicks: 8, MaxSteps: 512}
			got, err := adapter.ReadSingletonAdvanced(context.Background(), req)
			if err != nil {
				t.Fatal(err)
			}
			if got.KeyFee != domain.SingletonDynamicFeeFlag || got.Snapshot.LPFee != 3000 || got.Currency0 != domain.SingletonZeroAddress {
				t.Fatal("key/live/native identity lost")
			}
			if err = got.Validate(); err != nil {
				t.Fatal(err)
			}
			advancedConfig := &config.SingletonAdvancedPoolConfig{Protocol: state.Protocol, Manager: state.Snapshot.Manager, ExpectedManagerCodeSHA256: state.Snapshot.ManagerCodeSHA256, PoolID: state.Snapshot.PoolID, Currency0: state.Currency0, Currency1: state.Currency1, KeyFee: state.KeyFee, TickSpacing: state.Snapshot.Core.TickSpacing, Hooks: state.Snapshot.Hooks, Parameters: state.Snapshot.Parameters, TickWordRadius: 1, MaxInitializedTicks: 8, MaxSteps: 512}
			adapter.Chain.Pools = []config.PoolConfig{{ID: state.Snapshot.Manager + "/" + state.Snapshot.PoolID, Protocol: state.Protocol + "-advanced", Token0: state.Currency0, Token1: state.Currency1, FeeNumerator: uint64(state.KeyFee), FeeDenominator: 1000000, SingletonAdvanced: advancedConfig}}
			block, pools, e := adapter.ReadState(context.Background(), "latest", true)
			if e != nil || len(pools) != 1 || pools[0].SingletonAdvanced == nil || pools[0].SingletonCL != nil || pools[0].SingletonAdvanced.SnapshotBlockHash != block.Hash || pools[0].FeeNumerator != 3000 {
				t.Fatal("normal adapter path lost advanced snapshot", pools, e)
			}
			if protocol == "pancake-infinity-cl" {
				bitmapCall := singletonTestSelector("getHooksRegistrationBitmap()")
				f.results[bitmapCall] = "0x" + singletonTestInt(1)
				if _, err = adapter.ReadSingletonAdvanced(context.Background(), req); err == nil || !strings.Contains(err.Error(), "SINGLETON_HOOK_BITMAP") {
					t.Fatalf("mismatched original registration bitmap: %v", err)
				}
				f.results[bitmapCall] = "0x" + singletonTestInt(0)
			}
			req.State.Snapshot.Hooks = "0x0000000000000000000000000000000000010080"
			if _, err = adapter.ReadSingletonAdvanced(context.Background(), req); err == nil {
				t.Fatal("unbound callback accepted")
			}
		})
	}
}
