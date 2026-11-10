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

type pancakeCase struct {
	Index    int               `json:"index"`
	Mode     string            `json:"mode"`
	Pool     domain.Pool       `json:"pool"`
	Config   config.PoolConfig `json:"config"`
	Original struct {
		SourceBefore struct {
			Reads []struct {
				To     string `json:"to"`
				Data   string `json:"data"`
				Result string `json:"result"`
			} `json:"reads"`
		} `json:"source_before"`
	} `json:"original"`
}
type pancakeFile struct {
	Deployment map[string]struct {
		Address string `json:"address"`
		Code    string `json:"code"`
	} `json:"deployment"`
	Cases []pancakeCase `json:"cases"`
}

func pancakeOriginal(t *testing.T) pancakeFile {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/pancake-lm-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data pancakeFile
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	if len(data.Cases) != 104 {
		t.Fatal("original complete/invalid banks missing")
	}
	return data
}
func pancakeProvider(t *testing.T, data pancakeFile, c pancakeCase, bad string) (*httptest.Server, config.PoolConfig) {
	t.Helper()
	reads := map[string]string{}
	for _, r := range c.Original.SourceBefore.Reads {
		key := r.To + ":" + r.Data
		if old, ok := reads[key]; ok && old != r.Result {
			t.Fatal("conflicting original getter")
		}
		reads[key] = r.Result
	}
	if len(reads) < 45 {
		t.Fatal("original read set incomplete")
	}
	codes := map[string]string{}
	for _, entry := range data.Deployment {
		codes[entry.Address] = entry.Code
	}
	cfg := c.Config
	if strings.HasSuffix(bad, "-code") {
		role := strings.TrimSuffix(bad, "-code")
		entry := data.Deployment[role]
		raw, _ := hex.DecodeString(entry.Code[2:])
		raw[0] ^= 1
		codes[entry.Address] = "0x" + hex.EncodeToString(raw)
		digest := sha256.Sum256(raw)
		copy := *cfg.PancakeLm
		cfg.PancakeLm = &copy
		switch role {
		case "pool":
			copy.ExpectedCodeSHA256 = hex.EncodeToString(digest[:])
		case "lm":
			copy.ExpectedLmPoolCodeSHA256 = hex.EncodeToString(digest[:])
		case "masterChef":
			copy.ExpectedMasterChefCodeSHA256 = hex.EncodeToString(digest[:])
		case "factory":
			copy.ExpectedFactoryCodeSHA256 = hex.EncodeToString(digest[:])
		}
	}
	s := c.Pool.PancakeLm
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
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
			result = map[string]any{"number": fmt.Sprintf("0x%x", s.BlockNumber), "hash": s.BlockHash, "parentHash": "0x" + strings.Repeat("c6", 32), "timestamp": fmt.Sprintf("0x%x", s.Timestamp), "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
		case "eth_call", "eth_getCode":
			var anchor map[string]any
			if e := json.Unmarshal(q.Params[1], &anchor); e != nil || anchor["blockHash"] != s.BlockHash || anchor["requireCanonical"] != true {
				t.Error("unbound LM source read")
				return
			}
			if q.Method == "eth_getCode" {
				var addr string
				json.Unmarshal(q.Params[0], &addr)
				value, ok := codes[addr]
				if !ok {
					t.Error("unknown original code", addr)
				}
				result = value
			} else {
				var call map[string]string
				json.Unmarshal(q.Params[0], &call)
				key := call["to"] + ":" + call["data"]
				value, ok := reads[key]
				selector := call["data"][:10]
				if bad == "missing-lm-tick" && call["to"] == s.Lm.Address && selector == singletonSelector("lmTicks(int24)") {
					ok = false
				}
				if !ok {
					if bad == "" {
						t.Error("unrecorded original getter", key)
					}
					failure = map[string]any{"code": -32000, "message": "missing original getter"}
					break
				}
				result = value
				if bad == "unregistered" && selector == singletonSelector("getPool(address,address,uint24)") {
					result = "0x" + strings.Repeat("0", 64)
				}
				if bad == "lm-pointer" && selector == singletonSelector("lmPool()") {
					result = "0x" + strings.Repeat("0", 64)
				}
				if bad == "oracle-high-bits" && selector == singletonSelector("observations(uint256)") {
					result = "0x01" + value[4:]
				}
				if bad == "reward-view" && selector == singletonSelector("getLatestPeriodInfo(address)") {
					raw, _ := hex.DecodeString(value[2:])
					raw[31] ^= 1
					result = "0x" + hex.EncodeToString(raw)
				}
			}
		default:
			t.Error("unexpected LM RPC", q.Method)
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
	return server, cfg
}
func TestPancakeLmOriginalGetterBanksPreserveFullLedgerAndRewards(t *testing.T) {
	data := pancakeOriginal(t)
	var accepted, rejected int
	for _, c := range data.Cases {
		t.Run(fmt.Sprint(c.Index), func(t *testing.T) {
			server, cfg := pancakeProvider(t, data, c, "")
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{cfg}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if c.Mode == "invalid-lm-curve" {
				if e == nil || len(pools) != 0 {
					t.Fatal("invalid source bank published")
				}
				rejected++
				return
			}
			if e != nil || len(pools) != 1 {
				t.Fatal(e)
			}
			if !reflect.DeepEqual(pools[0], c.Pool) {
				got, _ := json.Marshal(pools[0])
				want, _ := json.Marshal(c.Pool)
				t.Fatalf("LM source mismatch\n%s\n%s", got, want)
			}
			accepted++
		})
	}
	if accepted != 100 || rejected != 4 {
		t.Fatal(accepted, rejected)
	}
}
func TestPancakeLmMissingSourcesAndRehashedUnknownCodeNeverPublish(t *testing.T) {
	data := pancakeOriginal(t)
	c := data.Cases[2]
	for _, bad := range []string{"pool-code", "lm-code", "masterChef-code", "factory-code", "unregistered", "lm-pointer", "oracle-high-bits", "reward-view", "missing-lm-tick"} {
		t.Run(bad, func(t *testing.T) {
			server, cfg := pancakeProvider(t, data, c, bad)
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{cfg}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if e == nil || len(pools) != 0 {
				t.Fatal("bad source published")
			}
		})
	}
}
