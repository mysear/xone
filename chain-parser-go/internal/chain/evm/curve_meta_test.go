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
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestMetaFixedBlockReadsOriginalCloneFullBalancesAndBaseDependency(t *testing.T) {
	raw, e := os.ReadFile("../../../test/contracts/curve-meta-service.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Cases []struct {
			Name   string            `json:"name"`
			Pool   domain.Pool       `json:"pool"`
			Config config.PoolConfig `json:"config"`
		}
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	for _, c := range f.Cases {
		if c.Pool.MultiAsset.CurveMeta.Timestamp != 1767225600 {
			continue
		}
		for _, bad := range []string{"", "implementation", "base-code", "vp", "stored", "admin", "token-order", "lp-decimals", "meta-decimals", "rate", "clone", "template", "after-anchor", "root"} {
			t.Run(c.Name+"/"+bad, func(t *testing.T) {
				p := c.Pool.Clone()
				var cfg config.PoolConfig
				cfgBytes, err := json.Marshal(c.Config)
				if err != nil {
					t.Fatal(err)
				}
				if err = json.Unmarshal(cfgBytes, &cfg); err != nil {
					t.Fatal(err)
				}
				s := p.MultiAsset
				m := s.CurveMeta
				corpus, e := os.ReadFile("../../../test/contracts/curve-meta-runtime.json")
				if e != nil {
					t.Fatal(e)
				}
				var runtimes map[string]struct {
					Implementation string `json:"implementation"`
					Clone          string `json:"clone"`
					Token0         string `json:"token0"`
					Token1         string `json:"token1"`
					Base           string `json:"base"`
				}
				if e = json.Unmarshal(corpus, &runtimes); e != nil {
					t.Fatal(e)
				}
				code := runtimes[c.Name]
				impl, _ := hex.DecodeString(strings.TrimPrefix(code.Implementation, "0x"))
				if bad == "template" {
					impl[0] ^= 1
					hash := sha256.Sum256(impl)
					cfg.MultiAsset.ExpectedImplementationCodeSHA256 = hex.EncodeToString(hash[:])
				}
				word := func(v string) string {
					n, ok := new(big.Int).SetString(v, 10)
					if !ok {
						t.Fatal(v)
					}
					return fmt.Sprintf("%064x", n)
				}
				pad := func(v string) string { return strings.Repeat("0", 24) + v[2:] }
				reads := map[string]bool{}
				heads := 0
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					var q struct {
						ID     json.RawMessage   `json:"id"`
						Method string            `json:"method"`
						Params []json.RawMessage `json:"params"`
					}
					json.NewDecoder(r.Body).Decode(&q)
					var out any
					if q.Method == "eth_getBlockByNumber" {
						heads++
						hash := m.BlockHash
						if bad == "after-anchor" && heads > 1 {
							hash = "0x" + strings.Repeat("b", 64)
						}
						root := "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
						if bad == "root" {
							root = "0x" + strings.Repeat("f", 64)
						}
						out = map[string]any{"number": "0x64", "hash": hash, "parentHash": "0x" + strings.Repeat("c", 64), "timestamp": fmt.Sprintf("0x%x", m.Timestamp), "transactions": []string{}, "receiptsRoot": root}
					} else {
						anchorIndex := 1
						if q.Method == "eth_getStorageAt" {
							anchorIndex = 2
						}
						var anchor map[string]any
						json.Unmarshal(q.Params[anchorIndex], &anchor)
						if anchor["blockHash"] != m.BlockHash || anchor["requireCanonical"] != true {
							t.Error("unanchored Meta dependency", anchor)
						}
						if q.Method == "eth_getCode" {
							var target string
							json.Unmarshal(q.Params[0], &target)
							reads[target+"/code"] = true
							switch target {
							case p.PoolID:
								out = code.Clone
								if bad == "clone" {
									out = "0x6001"
								}
							case m.Implementation:
								out = "0x" + hex.EncodeToString(impl)
								if bad == "implementation" {
									out = "0x6001"
								}
							case s.Assets[0].Token:
								out = code.Token0
							case s.Assets[1].Token:
								out = code.Token1
							case m.BasePool:
								out = code.Base
								if bad == "base-code" {
									out = "0x6001"
								}
							default:
								t.Error("unknown code target", target)
							}
						} else if q.Method == "eth_getStorageAt" {
							var slot string
							json.Unmarshal(q.Params[1], &slot)
							if slot != "0xb" {
								t.Error("unexpected Meta slot", slot)
							}
							value := string(s.Assets[0].Scaling)
							if bad == "rate" {
								value = "1"
							}
							out = "0x" + word(value)
						} else if q.Method == "eth_call" {
							var call map[string]string
							json.Unmarshal(q.Params[0], &call)
							data := call["data"]
							sel := data[:10]
							reads[call["to"]+data] = true
							switch sel {
							case "0xbb7b8b80":
								v := string(m.BaseVirtualPrice)
								if bad == "vp" {
									v = "0"
								}
								out = "0x" + word(v)
							case "0xc6610657":
								index := 0
								if strings.HasSuffix(data, "1") {
									index = 1
								}
								token := s.Assets[index].Token
								if bad == "token-order" {
									token = s.Assets[1-index].Token
								}
								out = "0x" + pad(token)
							case "0x4903b0d1", "0xe2e7d264":
								index := 0
								if strings.HasSuffix(data, "1") {
									index = 1
								}
								value := string(s.Assets[index].Balance)
								if sel == "0xe2e7d264" {
									value = string(m.AdminBalances[index])
									if bad == "admin" {
										value = "1"
									}
								}
								out = "0x" + word(value)
							case "0x313ce567":
								d := 18
								if call["to"] == s.Assets[0].Token {
									d = 36 - (len(string(s.Assets[0].Scaling)) - 1)
									if bad == "meta-decimals" {
										d = (d + 1) % 19
									}
								} else if bad == "lp-decimals" {
									d = 6
								}
								out = "0x" + word(fmt.Sprint(d))
							case "0x70a08231":
								i := 0
								if call["to"] == s.Assets[1].Token {
									i = 1
								}
								value := string(m.StoredBalances[i])
								if bad == "stored" {
									value = "0"
								}
								out = "0x" + word(value)
							case "0x76a2f0f0":
								out = "0x" + word(string(s.Amplification))
							case "0xddca3f43":
								out = "0x" + word(fmt.Sprint(p.FeeNumerator))
							case "0xfee3f7f9":
								out = "0x" + word(fmt.Sprint(s.AdminFee))
							default:
								t.Error("unknown Meta selector", sel)
								out = "0x"
							}
						} else {
							t.Error("unexpected RPC", q.Method)
						}
					}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
				}))
				defer server.Close()
				a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<22), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{cfg}}}
				_, pools, e := a.ReadState(context.Background(), "latest", true)
				if bad == "after-anchor" && (e == nil || e.Error() != "ANCHOR_REORGED" || heads < 2) {
					t.Fatalf("reanchor guard not exercised: heads=%d error=%v", heads, e)
				}
				if bad != "" {
					if e == nil {
						t.Fatal("unsupported Meta dependency admitted", bad)
					}
					return
				}
				if e != nil || len(pools) != 1 {
					t.Fatal(pools, e)
				}
				x, _ := domain.Hash(pools[0])
				y, _ := domain.Hash(p)
				if x != y {
					t.Fatal("Meta complete pool changed", pools[0], p)
				}
				for _, target := range []string{p.PoolID, m.Implementation, m.BasePool, s.Assets[0].Token, s.Assets[1].Token} {
					if !reads[target+"/code"] {
						t.Fatal("missing Meta code proof", target)
					}
				}
			})
		}
	}
}
