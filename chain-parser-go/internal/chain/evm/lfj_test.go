package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
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

type lfjReference struct {
	Deployment map[string]string  `json:"deployment"`
	BlockHash  string             `json:"block_hash"`
	ParentHash string             `json:"parent_hash"`
	Timestamp  uint64             `json:"timestamp"`
	Cases      []lfjReferenceCase `json:"cases"`
}
type lfjReferenceCase struct {
	Index       int               `json:"index"`
	Pool        domain.Pool       `json:"pool"`
	Config      config.PoolConfig `json:"config"`
	GetterReads []struct {
		To     string `json:"to"`
		Data   string `json:"data"`
		Result string `json:"result"`
	} `json:"getter_reads"`
	StorageReads map[string]string `json:"storage_reads"`
}

func lfjFixture(t *testing.T) lfjReference {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/lfj-v21-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data lfjReference
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	if len(data.Cases) != 96 {
		t.Fatal("incomplete LFJ original corpus")
	}
	return data
}
func lfjProvider(t *testing.T, data lfjReference, source lfjReferenceCase, bad string) *httptest.Server {
	t.Helper()
	observed := map[string]string{}
	for _, read := range source.GetterReads {
		key := read.To + ":" + read.Data
		if _, exists := observed[key]; exists {
			t.Fatal("duplicate native LFJ getter", key)
		}
		observed[key] = read.Result
	}
	if len(observed) < 30 {
		t.Fatal("incomplete original getter observations")
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
		var rpcErr any
		state := source.Pool.Lfj
		switch q.Method {
		case "eth_chainId":
			result = "0x7a69"
		case "eth_blockNumber":
			result = "0x64"
		case "eth_getBlockByNumber":
			result = map[string]any{"number": "0x64", "hash": data.BlockHash, "parentHash": data.ParentHash, "timestamp": fmt.Sprintf("0x%x", state.Timestamp), "transactions": []any{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
		case "eth_call", "eth_getCode", "eth_getStorageAt":
			at := 1
			if q.Method == "eth_getStorageAt" {
				at = 2
			}
			var anchor map[string]any
			if e := json.Unmarshal(q.Params[at], &anchor); e != nil || anchor["blockHash"] != data.BlockHash || anchor["requireCanonical"] != true {
				t.Error("unbound LFJ read", q.Method)
				rpcErr = map[string]any{"code": -32000, "message": "wrong anchor"}
				break
			}
			if q.Method == "eth_getCode" {
				var address string
				json.Unmarshal(q.Params[0], &address)
				switch address {
				case source.Pool.PoolID:
					result = data.Deployment["pool_code"]
				case state.Implementation:
					result = data.Deployment["implementation_code"]
				case state.Factory:
					result = data.Deployment["factory_code"]
				case source.Pool.Token0:
					result = data.Deployment["token0_code"]
				case source.Pool.Token1:
					result = data.Deployment["token1_code"]
				default:
					t.Error("unknown LFJ code", address)
					result = "0x"
				}
				if address == source.Pool.PoolID {
					if bad == "code" {
						result = "0x6000"
					}
					if bad == "clone-key" {
						raw, _ := hex.DecodeString(data.Deployment["pool_code"][2:])
						raw[len(raw)-3] ^= 1
						result = "0x" + hex.EncodeToString(raw)
					}
				}
			} else if q.Method == "eth_getStorageAt" {
				var target, index string
				json.Unmarshal(q.Params[0], &target)
				json.Unmarshal(q.Params[1], &index)
				if target != source.Pool.PoolID {
					t.Error("LFJ storage receiver")
					return
				}
				value, ok := source.StorageReads[index]
				if !ok {
					t.Error("unknown original LFJ storage", index)
					rpcErr = map[string]any{"code": -32000, "message": "missing storage"}
					break
				}
				result = value
				if bad == "locked" && index == "0x3" {
					result = fmt.Sprintf("0x%064x", 2)
				}
				if bad == "future-clock" && index == "0x4" {
					n, _ := new(big.Int).SetString(value[2:], 16)
					mask := new(big.Int).Lsh(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 40), big.NewInt(1)), 176)
					n.AndNot(n, mask)
					n.Or(n, new(big.Int).Lsh(new(big.Int).SetUint64(state.Timestamp+1), 176))
					result = fmt.Sprintf("0x%064x", n)
				}
			} else {
				var call map[string]string
				json.Unmarshal(q.Params[0], &call)
				if len(call["data"]) < 10 {
					t.Error("short LFJ calldata")
					return
				}
				selector := call["data"][:10]
				args := call["data"][10:]
				if bad == "missing-bin" && selector == singletonSelector("getNextNonEmptyBin(bool,uint24)") {
					side, _ := new(big.Int).SetString(args[:64], 16)
					id, _ := new(big.Int).SetString(args[64:], 16)
					var next uint32
					if side.Sign() != 0 {
						next = 0xffffff
					}
					for i, bin := range state.Bins {
						if i == 2 {
							continue
						}
						if side.Sign() == 0 && uint64(bin.ID) > id.Uint64() {
							next = bin.ID
							break
						}
						if side.Sign() != 0 && uint64(bin.ID) < id.Uint64() {
							next = bin.ID
						}
					}
					result = fmt.Sprintf("0x%064x", next)
				} else {
					raw, ok := observed[call["to"]+":"+call["data"]]
					if !ok {
						t.Error("unrecorded original LFJ call", call)
						rpcErr = map[string]any{"code": -32601, "message": "unrecorded getter"}
						break
					}
					result = raw
					if bad == "getter-high-bits" && selector == singletonSelector("getStaticFeeParameters()") {
						result = "0x01" + raw[4:]
					}
					if bad == "donation" && selector == singletonSelector("balanceOf(address)") {
						n, _ := new(big.Int).SetString(raw[2:], 16)
						result = fmt.Sprintf("0x%064x", n.Add(n, big.NewInt(1)))
					}
					if bad == "zero-supply" && selector == singletonSelector("totalSupply(uint256)") && args == fmt.Sprintf("%064x", state.Bins[2].ID) {
						result = "0x" + strings.Repeat("0", 64)
					}
				}
			}
		default:
			t.Error("unknown LFJ RPC", q.Method)
			rpcErr = map[string]any{"code": -32601, "message": "unknown method"}
		}
		response := map[string]any{"jsonrpc": "2.0", "id": q.ID}
		if rpcErr != nil {
			response["error"] = rpcErr
		} else {
			response["result"] = result
		}
		json.NewEncoder(w).Encode(response)
	}))
}
func TestLfjOriginalBanksReadEveryFeeBinOracleAtOneHash(t *testing.T) {
	data := lfjFixture(t)
	for _, c := range data.Cases {
		t.Run(fmt.Sprint(c.Index), func(t *testing.T) {
			server := lfjProvider(t, data, c, "")
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{c.Config}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if e != nil || len(pools) != 1 {
				t.Fatal(e)
			}
			if !reflect.DeepEqual(pools[0], c.Pool) {
				left, _ := json.Marshal(pools[0])
				right, _ := json.Marshal(c.Pool)
				t.Fatalf("LFJ state mismatch\n%s\n%s", left, right)
			}
		})
	}
}
func TestLfjMissingBinsUnsyncedTokensAndBadSourcesNeverPublish(t *testing.T) {
	data := lfjFixture(t)
	for _, bad := range []string{"code", "clone-key", "locked", "future-clock", "getter-high-bits", "donation", "missing-bin", "zero-supply"} {
		t.Run(bad, func(t *testing.T) {
			c := data.Cases[0]
			c.Config.Lfj = func() *config.LfjConfig { copy := *c.Config.Lfj; return &copy }()
			if bad == "clone-key" {
				raw, _ := hex.DecodeString(data.Deployment["pool_code"][2:])
				raw[len(raw)-3] ^= 1
				digest := sha256.Sum256(raw)
				c.Config.Lfj.ExpectedCodeSHA256 = hex.EncodeToString(digest[:])
			}
			server := lfjProvider(t, data, c, bad)
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{c.Config}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if e == nil || len(pools) != 0 {
				t.Fatal("unsafe LFJ source published", bad, e)
			}
		})
	}
}
