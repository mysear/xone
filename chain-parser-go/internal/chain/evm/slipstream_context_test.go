package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

type slipOriginalCase struct {
	Index  int               `json:"index"`
	Pool   domain.Pool       `json:"pool"`
	Config config.PoolConfig `json:"config"`
	Reads  []struct {
		To     string `json:"to"`
		Data   string `json:"data"`
		From   string `json:"from"`
		Result string `json:"result"`
	} `json:"getter_reads"`
}
type slipOriginalFile struct {
	Deployment map[string]string  `json:"deployment"`
	Cases      []slipOriginalCase `json:"cases"`
}

func slipOriginal(t *testing.T) slipOriginalFile {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/slipstream-context-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data slipOriginalFile
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	if len(data.Cases) != 108 {
		t.Fatal("original Slipstream cases missing")
	}
	return data
}
func slipProvider(t *testing.T, data slipOriginalFile, c slipOriginalCase, bad string) *httptest.Server {
	t.Helper()
	reads := map[string]string{}
	for _, r := range c.Reads {
		key := r.To + ":" + r.Data + ":" + r.From
		if old, ok := reads[key]; ok && old != r.Result {
			t.Fatal("conflicting original getter bytes")
		}
		reads[key] = r.Result
	}
	if len(reads) < 50 {
		t.Fatal("native getter readset incomplete")
	}
	state := c.Pool.SlipstreamContext
	codes := map[string]string{c.Pool.PoolID: data.Deployment["pool_code"], state.Core.Factory: data.Deployment["factory_code"], state.Core.Implementation: data.Deployment["implementation_code"], state.FactoryVoter: data.Deployment["voter_code"], c.Pool.Token0: data.Deployment["token0_code"], c.Pool.Token1: data.Deployment["token1_code"], state.Origins[0].Origin: "0x"}
	for _, m := range []domain.SlipstreamModule{state.SwapFeeModule, state.UnstakedFeeModule} {
		if m.Kind == "none" {
			continue
		}
		key := "unstaked_custom_code"
		if m.Address == state.SwapFeeModule.Address {
			key = "swap_custom_code"
			if m.Kind == "dynamic" {
				key = "swap_dynamic_code"
			}
		}
		codes[m.Address] = data.Deployment[key]
	}
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     json.RawMessage   `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		if e := json.NewDecoder(r.Body).Decode(&q); e != nil {
			t.Error(e)
			return
		}
		var result any
		var failure any
		switch q.Method {
		case "eth_chainId":
			result = "0x7a69"
		case "eth_getBlockByNumber":
			result = map[string]any{"number": "0x64", "hash": state.BlockHash, "parentHash": "0x" + strings.Repeat("c6", 32), "timestamp": fmt.Sprintf("0x%x", state.Timestamp), "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
		case "eth_call", "eth_getCode":
			var anchor map[string]any
			if e := json.Unmarshal(q.Params[1], &anchor); e != nil || anchor["blockHash"] != state.BlockHash || anchor["requireCanonical"] != true {
				t.Error("Slipstream unbound read")
				return
			}
			if q.Method == "eth_getCode" {
				var address string
				json.Unmarshal(q.Params[0], &address)
				raw, ok := codes[address]
				if !ok {
					t.Error("unrecorded Slipstream account", address)
					failure = map[string]any{"code": -32000, "message": "unrecorded account"}
				} else {
					result = raw
					if bad == "implementation-template" && address == state.Core.Implementation || bad == "module-template" && address == state.SwapFeeModule.Address {
						bytes, _ := hex.DecodeString(raw[2:])
						bytes[0] ^= 1
						result = "0x" + hex.EncodeToString(bytes)
					}
					if bad == "origin-code" && address == state.Origins[0].Origin {
						result = "0x6000"
					}
				}
			} else {
				var call map[string]string
				json.Unmarshal(q.Params[0], &call)
				from := call["from"]
				if from == "" {
					from = domain.SingletonZeroAddress
				}
				key := call["to"] + ":" + call["data"] + ":" + from
				value, ok := reads[key]
				if !ok {
					t.Error("unrecorded original Slipstream call", key)
					failure = map[string]any{"code": -32601, "message": "unrecorded getter"}
					break
				}
				result = value
				selector := call["data"][:10]
				if bad == "unregistered" && selector == singletonSelector("isPool(address)") {
					result = "0x" + strings.Repeat("0", 64)
				}
				if bad == "pool-key" && selector == singletonSelector("getPool(address,address,int24)") {
					result = "0x" + strings.Repeat("0", 24) + strings.Repeat("1", 40)
				}
				if bad == "oracle-high-bits" && selector == singletonSelector("observations(uint256)") {
					result = "0x01" + value[4:]
				}
				if bad == "module-pointer" && selector == selectorSwapFeeModule {
					result = "0x" + strings.Repeat("0", 64)
				}
			}
		default:
			t.Error("unexpected Slipstream RPC", q.Method)
			failure = map[string]any{"code": -32601, "message": "unexpected method"}
		}
		response := map[string]any{"jsonrpc": "2.0", "id": q.ID}
		if failure != nil {
			response["error"] = failure
		} else {
			response["result"] = result
		}
		json.NewEncoder(w).Encode(response)
	}))
}
func TestSlipstreamContextOriginalBanksRetainEveryLedgerOracleAndCallerInput(t *testing.T) {
	data := slipOriginal(t)
	for _, c := range data.Cases {
		t.Run(fmt.Sprint(c.Index), func(t *testing.T) {
			server := slipProvider(t, data, c, "")
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{c.Config}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if e != nil || len(pools) != 1 {
				t.Fatal(e)
			}
			if !reflect.DeepEqual(pools[0], c.Pool) {
				got, _ := json.Marshal(pools[0])
				want, _ := json.Marshal(c.Pool)
				t.Fatalf("Slipstream complete state mismatch\n%s\n%s", got, want)
			}
		})
	}
}
func TestSlipstreamContextBadSourcesAndRehashedTemplatesNeverPublish(t *testing.T) {
	data := slipOriginal(t)
	var original slipOriginalCase
	for _, c := range data.Cases {
		if c.Pool.SlipstreamContext.SwapFeeModule.Kind == "dynamic" {
			original = c
			break
		}
	}
	for _, bad := range []string{"implementation-template", "module-template", "origin-code", "unregistered", "pool-key", "oracle-high-bits", "module-pointer"} {
		t.Run(bad, func(t *testing.T) {
			raw, _ := json.Marshal(original)
			var c slipOriginalCase
			json.Unmarshal(raw, &c)
			for _, spec := range []struct {
				test, key string
				target    *string
			}{{"implementation-template", "implementation_code", &c.Config.SlipstreamContext.ExpectedImplementationCodeSHA256}, {"module-template", "swap_dynamic_code", &c.Config.SlipstreamContext.SwapFeeModule.ExpectedCodeSHA256}} {
				if bad == spec.test {
					bytes, _ := hex.DecodeString(data.Deployment[spec.key][2:])
					bytes[0] ^= 1
					hash := sha256.Sum256(bytes)
					*spec.target = hex.EncodeToString(hash[:])
				}
			}
			server := slipProvider(t, data, c, bad)
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{c.Config}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if e == nil || len(pools) != 0 {
				t.Fatal("unsafe contextual source published", bad, e)
			}
		})
	}
}
