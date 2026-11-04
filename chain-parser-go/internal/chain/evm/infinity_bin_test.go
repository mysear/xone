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
	"strconv"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func binOriginalFixture(t *testing.T) map[string]any {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/infinity-bin-service.json")
	if e != nil {
		t.Fatal(e)
	}
	var d map[string]any
	if e = json.Unmarshal(raw, &d); e != nil {
		t.Fatal(e)
	}
	return d
}
func binOriginalConfig(t *testing.T, d map[string]any, bank map[string]any, index int) config.PoolConfig {
	t.Helper()
	manager := d["manager"].(string)
	vault := d["vault"].(string)
	pool := d["pools"].([]any)[index].(map[string]any)
	key := pool["key"].(map[string]any)
	code := bank["code"].(map[string]any)
	hash := func(addr string) string {
		b, _ := hex.DecodeString(code[addr].(string)[2:])
		h := sha256.Sum256(b)
		return hex.EncodeToString(h[:])
	}
	doc := map[string]any{"id": manager + "/" + pool["id"].(string), "protocol": "pancake-infinity-bin", "token0": key["currency0"], "token1": key["currency1"], "fee_numerator": pool["lp_fee"], "fee_denominator": 1000000, "infinity_bin": map[string]any{"manager": manager, "expected_manager_code_sha256": hash(manager), "vault": vault, "expected_vault_code_sha256": hash(vault), "pool_id": pool["id"], "bin_step": pool["bin_step"], "lp_fee": pool["lp_fee"], "expected_token0_code_sha256": hash(key["currency0"].(string)), "expected_token1_code_sha256": hash(key["currency1"].(string)), "token_policy": "standard", "max_bins": 64, "max_tree_words": 64, "max_steps": 64}}
	b, _ := json.Marshal(doc)
	var p config.PoolConfig
	if e := json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestBinOriginalManagerFullTreeAnchoredReadAndRejectsCorruption(t *testing.T) {
	for _, bad := range []string{"", "lowered-max-step", "manager-template", "vault-code", "key", "paused", "registered", "tree-zero-child", "tree-bins-budget", "tree-words-budget", "bin-width", "next", "share-zero", "source-reanchor", "receipt-root"} {
		t.Run(bad, func(t *testing.T) {
			d := binOriginalFixture(t)
			bank := d["RPCbanks"].([]any)[0].(map[string]any)
			index := 0
			if bad == "lowered-max-step" {
				index = 1
			}
			p := binOriginalConfig(t, d, bank, index)
			manager := d["manager"].(string)
			vault := d["vault"].(string)
			pool := d["pools"].([]any)[index].(map[string]any)
			hash := bank["block"].(map[string]any)["hash"].(string)
			n := uint64(bank["block"].(map[string]any)["num"].(float64))
			timestamp := uint64(bank["block"].(map[string]any)["timestamp"].(float64))
			heads, reads := 0, 0
			if bad == "tree-bins-budget" || bad == "tree-words-budget" {
				raw, _ := json.Marshal(p)
				var doc map[string]any
				json.Unmarshal(raw, &doc)
				v := doc["infinity_bin"].(map[string]any)
				if bad == "tree-bins-budget" {
					v["max_bins"] = 1
				} else {
					v["max_tree_words"] = 1
				}
				raw, _ = json.Marshal(doc)
				json.Unmarshal(raw, &p)
			}
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var q struct {
					ID     any               `json:"id"`
					Method string            `json:"method"`
					Params []json.RawMessage `json:"params"`
				}
				json.NewDecoder(r.Body).Decode(&q)
				var out any
				if q.Method == "eth_getBlockByNumber" {
					heads++
					h := hash
					if bad == "source-reanchor" && heads > 1 {
						h = "0x" + strings.Repeat("b", 64)
					}
					root := "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
					if bad == "receipt-root" {
						root = "0x" + strings.Repeat("f", 64)
					}
					out = map[string]any{"number": fmt.Sprintf("0x%x", n), "hash": h, "parentHash": "0x" + strings.Repeat("c", 64), "timestamp": fmt.Sprintf("0x%x", timestamp), "transactions": []any{}, "receiptsRoot": root}
				} else {
					reads++
					var anchor map[string]any
					param := 1
					if q.Method == "eth_getStorageAt" {
						param = 2
					}
					json.Unmarshal(q.Params[param], &anchor)
					if anchor["blockHash"] != hash || anchor["requireCanonical"] != true {
						t.Error("unanchored bin dependency", q.Method, anchor)
					}
					switch q.Method {
					case "eth_getCode":
						var target string
						json.Unmarshal(q.Params[0], &target)
						out = bank["code"].(map[string]any)[target]
						if out == nil {
							t.Error("unmodeled code", target)
							out = "0x"
						}
						if bad == "vault-code" && target == vault {
							out = "0x6000"
						}
						if bad == "manager-template" && target == manager {
							b, _ := hex.DecodeString(out.(string)[2:])
							b[0] ^= 1
							out = "0x" + hex.EncodeToString(b)
						}
					case "eth_call":
						var c map[string]string
						json.Unmarshal(q.Params[0], &c)
						target, data := c["to"], c["data"]
						value, ok := bank["callmap"].(map[string]any)[target].(map[string]any)[data]
						if !ok {
							t.Error("unmodeled Bin call", target, data)
							value = "0x" + strings.Repeat("0", 64)
						}
						out = value
						selector := data[:10]
						if bad == "lowered-max-step" && selector == "0x615188b7" {
							out = fmt.Sprintf("0x%064x", 1)
						}
						if bad == "paused" && selector == "0x5c975abb" {
							out = fmt.Sprintf("0x%064x", 1)
						}
						if bad == "registered" && selector == "0x8403be91" {
							out = "0x" + strings.Repeat("0", 64)
						}
						if bad == "key" && selector == "0x0e2d484a" {
							b, _ := hex.DecodeString(value.(string)[2:])
							b[31] ^= 1
							out = "0x" + hex.EncodeToString(b)
						}
						if bad == "next" && selector == "0xbf7b7286" {
							out = fmt.Sprintf("0x%064x", 8388608)
						}
						if bad == "bin-width" && selector == "0x4580c3c5" {
							b, _ := hex.DecodeString(value.(string)[2:])
							b[0] = 1
							out = "0x" + hex.EncodeToString(b)
						}
						if bad == "share-zero" && selector == "0x4580c3c5" {
							b, _ := hex.DecodeString(value.(string)[2:])
							for i := 96; i < 128; i++ {
								b[i] = 0
							}
							out = "0x" + hex.EncodeToString(b)
						}
						if bad == "tree-zero-child" && selector == "0x1e2eaeaf" {
							raw, _ := hex.DecodeString(data[10:])
							slot := new(big.Int).SetBytes(raw)
							base, _ := new(big.Int).SetString(pool["pool_base"].(string)[2:], 16)
							rootSlot := new(big.Int).Add(base, big.NewInt(4))
							if slot.Cmp(rootSlot) != 0 {
								out = "0x" + strings.Repeat("0", 64)
							}
						}
					case "eth_getStorageAt":
						var target, key string
						json.Unmarshal(q.Params[0], &target)
						json.Unmarshal(q.Params[1], &key)
						v, _ := new(big.Int).SetString(strings.TrimPrefix(key, "0x"), 16)
						key = fmt.Sprintf("0x%064x", v)
						out = bank["storage"].(map[string]any)[target].(map[string]any)[key]
						if out == nil {
							out = "0x" + strings.Repeat("0", 64)
						}
					default:
						t.Error("unexpected Bin RPC", q.Method)
					}
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
			}))
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, 3*time.Second, 2<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
			block, pools, e := a.ReadState(context.Background(), strconv.FormatUint(n, 10), true)
			if bad != "" && bad != "lowered-max-step" {
				if e == nil {
					t.Fatal("bad Bin source admitted", bad, pools)
				}
				if bad == "source-reanchor" && (!strings.Contains(e.Error(), "ANCHOR_REORGED") || heads != 2) {
					t.Fatal("source reanchor not reached", e, heads)
				}
				return
			}
			if e != nil || len(pools) != 1 {
				t.Fatal("Bin full tree missing", e, pools)
			}
			s := pools[0].InfinityBin
			if s == nil || len(s.Bins) != 7 || len(s.TreeLevel1) != 4 || len(s.TreeLevel2) != 6 || s.BlockHash != block.Hash || reads < 30 || heads != 2 {
				t.Fatal("incomplete Bin tree source", s, reads, heads)
			}
		})
	}
}

func TestAllOriginalBinBanksMatchCompleteProjectedState(t *testing.T) {
	d := binOriginalFixture(t)
	raw, e := os.ReadFile("../../../test/contracts/infinity-bin-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Banks []struct {
			Items []struct {
				Pool domain.Pool `json:"pool"`
			} `json:"items"`
		} `json:"banks"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	for phase, item := range d["RPCbanks"].([]any) {
		bank := item.(map[string]any)
		hash := bank["block"].(map[string]any)["hash"].(string)
		n := uint64(bank["block"].(map[string]any)["num"].(float64))
		timestamp := uint64(bank["block"].(map[string]any)["timestamp"].(float64))
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var q struct {
				ID     any               `json:"id"`
				Method string            `json:"method"`
				Params []json.RawMessage `json:"params"`
			}
			json.NewDecoder(r.Body).Decode(&q)
			var out any
			switch q.Method {
			case "eth_getCode":
				var target string
				json.Unmarshal(q.Params[0], &target)
				out = bank["code"].(map[string]any)[target]
			case "eth_call":
				var call map[string]string
				json.Unmarshal(q.Params[0], &call)
				out = bank["callmap"].(map[string]any)[call["to"]].(map[string]any)[call["data"]]
			default:
				t.Error("unexpected original Bin RPC", q.Method)
			}
			var anchor map[string]any
			json.Unmarshal(q.Params[1], &anchor)
			if anchor["blockHash"] != hash || anchor["requireCanonical"] != true {
				t.Error("bank mixing")
			}
			json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
		}))
		a := Adapter{Client: rpc.New([]string{server.URL}, 3*time.Second, 2<<20), Chain: config.Chain{Key: "evm:31337"}}
		for index := 0; index < 2; index++ {
			p, e := a.readInfinityBin(context.Background(), binOriginalConfig(t, d, bank, index), map[string]any{"blockHash": hash, "requireCanonical": true}, domain.BlockRef{ChainKey: "evm:31337", Number: n, Hash: hash, TimestampMS: timestamp * 1000})
			if e != nil {
				t.Fatal(phase, index, e)
			}
			if !reflect.DeepEqual(p, f.Banks[phase].Items[index].Pool) {
				t.Fatal("full original manager bank differs", phase, index)
			}
		}
		server.Close()
	}
}

func TestBinDetachedActiveDustIsReadSeparatelyFromActualTree(t *testing.T) {
	d := binOriginalFixture(t)
	bank := d["RPCbanks"].([]any)[0].(map[string]any)
	p := binOriginalConfig(t, d, bank, 0)
	manager := d["manager"].(string)
	definition := d["pools"].([]any)[0].(map[string]any)
	id := definition["id"].(string)[2:]
	base, _ := hex.DecodeString(definition["pool_base"].(string)[2:])
	active := uint64(8388608)
	word := func(n uint64) string { return fmt.Sprintf("%064x", n) }
	aWord := new(big.Int).SetUint64(active).FillBytes(make([]byte, 32))
	reserveSlot := "0x" + hex.EncodeToString(singletonMappingSlot(aWord, singletonSlotOffset(base, 1)))
	shareSlot := "0x" + hex.EncodeToString(singletonMappingSlot(aWord, singletonSlotOffset(base, 2)))
	leafSlot := "0x" + hex.EncodeToString(singletonMappingSlot(new(big.Int).SetUint64(active>>8).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 6)))
	getBin := singletonSelector("getBin(bytes32,uint24)") + id + word(active)
	calls := bank["callmap"].(map[string]any)[manager].(map[string]any)
	ext := singletonSelector("extsload(bytes32)")
	calls[ext+leafSlot[2:]] = "0x" + word(2)
	dust := new(big.Int).Or(big.NewInt(1), new(big.Int).Lsh(big.NewInt(1), 128))
	calls[ext+reserveSlot[2:]] = fmt.Sprintf("0x%064x", dust)
	calls[ext+shareSlot[2:]] = "0x" + word(1000)
	calls[getBin] = "0x" + word(1) + word(1) + word(1) + word(1000)
	ids := []uint64{8323071, 8388351, 8388607, 8388609, 8388865, 8454145}
	for i, bid := range ids {
		left := uint64(0xffffff)
		right := uint64(0)
		if i > 0 {
			left = ids[i-1]
		}
		if i+1 < len(ids) {
			right = ids[i+1]
		}
		calls[singletonSelector("getNextNonEmptyBin(bytes32,bool,uint24)")+id+word(1)+word(bid)] = "0x" + word(left)
		calls[singletonSelector("getNextNonEmptyBin(bytes32,bool,uint24)")+id+word(0)+word(bid)] = "0x" + word(right)
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		json.NewDecoder(r.Body).Decode(&q)
		var out any
		if q.Method == "eth_getCode" {
			var addr string
			json.Unmarshal(q.Params[0], &addr)
			out = bank["code"].(map[string]any)[addr]
		} else {
			var c map[string]string
			json.Unmarshal(q.Params[0], &c)
			out = bank["callmap"].(map[string]any)[c["to"]].(map[string]any)[c["data"]]
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
	}))
	defer server.Close()
	hash := bank["block"].(map[string]any)["hash"].(string)
	a := Adapter{Client: rpc.New([]string{server.URL}, 3*time.Second, 2<<20), Chain: config.Chain{Key: "evm:31337"}}
	pool, e := a.readInfinityBin(context.Background(), p, map[string]any{"blockHash": hash, "requireCanonical": true}, domain.BlockRef{ChainKey: "evm:31337", Number: 100, Hash: hash, TimestampMS: 1793577600000})
	if e != nil {
		t.Fatal(e)
	}
	s := pool.InfinityBin
	if s.DetachedActive == nil || s.DetachedActive.ID != uint32(active) || s.DetachedActive.Reserve0 != "1" || s.DetachedActive.TotalShares != "1000" || len(s.Bins) != 6 {
		t.Fatal("lost detached active or changed actual tree", s)
	}
}
