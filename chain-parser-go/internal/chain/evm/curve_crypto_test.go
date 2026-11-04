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

func TestCryptoAnchoredOriginalRuntimeLPAndCompleteState(t *testing.T) {
	raw, e := os.ReadFile("../../../test/contracts/curve-crypto-runtime-template.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Template string `json:"runtime_template_hex"`
		LP       string `json:"lp_runtime_hex"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	template, _ := hex.DecodeString(f.Template)
	lpcode, _ := hex.DecodeString(f.LP)
	for _, bad := range []string{"", "template", "immutable-lp", "immutable-coin", "precision", "decimals", "lp-code", "minter", "coin-order", "token-getter", "killed", "ramp", "boolean", "time-width", "raw-shortfall", "zero-supply", "fee", "after-anchor", "receipt-root", "storage-abi", "ramp-online", "ramp-online-down", "ramp-online-end", "ramp-online-stop", "ramp-origin-future", "ramp-origin-width", "ramp-origin-domain", "ramp-initial-after-end"} {
		t.Run(bad, func(t *testing.T) {
			addr := func(i int) string { return fmt.Sprintf("0x%040x", i) }
			word := func(v *big.Int) string { return fmt.Sprintf("%064x", v) }
			u := func(v int64) *big.Int { return big.NewInt(v) }
			one := new(big.Int).Exp(u(10), u(18), nil)
			balance := new(big.Int).Exp(u(10), u(24), nil)
			tail := make([]byte, 160)
			for i, v := range []int64{99, 1, 2, 1, 1} {
				u(v).FillBytes(tail[i*32 : (i+1)*32])
			}
			switch bad {
			case "immutable-lp":
				u(77).FillBytes(tail[:32])
			case "immutable-coin":
				u(7).FillBytes(tail[32:64])
			case "precision":
				u(2).FillBytes(tail[96:128])
			}
			code := append(append([]byte{}, template...), tail...)
			if bad == "template" {
				code[0] ^= 1
			}
			poolhash := sha256.Sum256(code)
			tokenCode := []byte{0x60, 2, 0x55}
			tokenhash := sha256.Sum256(tokenCode)
			v := &config.MultiAssetConfig{ExpectedCodeSHA256: hex.EncodeToString(poolhash[:]), ExpectedLPToken: addr(99), ExpectedLPTokenCodeSHA256: domain.CurveCryptoLPCodeSHA256, TokenPolicy: "standard-no-bpt", Tokens: []config.MultiTokenConfig{{Token: addr(1), ExpectedCodeSHA256: hex.EncodeToString(tokenhash[:])}, {Token: addr(2), ExpectedCodeSHA256: hex.EncodeToString(tokenhash[:])}}}
			p := config.PoolConfig{ID: addr(50), Protocol: "curve-crypto2-full", Token0: addr(1), Token1: addr(2), FeeNumerator: 3000000, FeeDenominator: 10000000000, MultiAsset: v}
			if strings.HasPrefix(bad, "ramp-") && bad != "ramp" {
				p.Protocol = "curve-crypto2-ramp"
			}
			packed := new(big.Int).Lsh(u(400000), 128)
			packed.Or(packed, u(145000000000000))
			slots := map[uint64]*big.Int{1: one, 2: one, 3: one, 4: u(100), 6: packed, 8: u(0), 9: one, 11: u(230000000000000), 13: u(146000000000000), 15: u(600), 17: u(3000000), 18: u(30000000), 19: u(5000000000), 23: balance, 24: balance, 25: new(big.Int).Mul(balance, u(2)), 28: one, 29: one, 30: one, 31: u(0), 32: u(0), 36: u(98)}
			if p.Protocol == "curve-crypto2-ramp" {
				slots[5] = new(big.Int).Or(new(big.Int).Lsh(u(200000), 128), u(100000000000000))
				slots[7] = u(50)
				slots[8] = u(200)
				switch bad {
				case "ramp-online-down":
					slots[5] = new(big.Int).Or(new(big.Int).Lsh(u(800000), 128), u(300000000000000))
				case "ramp-online-end":
					slots[8] = u(100)
				case "ramp-online-stop":
					slots[7] = u(100)
					slots[8] = u(100)
					slots[5] = packed
				case "ramp-origin-future":
					slots[7] = u(101)
				case "ramp-origin-width":
					slots[7] = new(big.Int).Lsh(u(1), 65)
				case "ramp-origin-domain":
					slots[5] = u(1)
				case "ramp-initial-after-end":
					slots[7] = u(99)
					slots[8] = u(98)
				}
			}
			switch bad {
			case "killed":
				slots[32] = u(1)
			case "ramp":
				slots[8] = u(200)
			case "boolean":
				slots[31] = u(2)
			case "time-width":
				slots[4] = new(big.Int).Lsh(u(1), 65)
			case "fee":
				slots[17] = u(4000000)
			}
			heads, reads := 0, 0
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
					hash := "0x" + strings.Repeat("a", 64)
					if bad == "after-anchor" && heads > 1 {
						hash = "0x" + strings.Repeat("b", 64)
					}
					root := "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
					if bad == "receipt-root" {
						root = "0x" + strings.Repeat("f", 64)
					}
					out = map[string]any{"number": "0x64", "hash": hash, "parentHash": "0x" + strings.Repeat("c", 64), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": root}
				} else {
					reads++
					var anchor map[string]any
					at := 1
					if q.Method == "eth_getStorageAt" {
						at = 2
					}
					json.Unmarshal(q.Params[at], &anchor)
					if anchor["blockHash"] != "0x"+strings.Repeat("a", 64) || anchor["requireCanonical"] != true {
						t.Error("unanchored Crypto dependency", q.Method)
					}
					switch q.Method {
					case "eth_getCode":
						var to string
						json.Unmarshal(q.Params[0], &to)
						b := tokenCode
						if to == p.ID {
							b = code
						}
						if to == addr(99) {
							b = lpcode
							if bad == "lp-code" {
								b = tokenCode
							}
						}
						out = "0x" + hex.EncodeToString(b)
					case "eth_getStorageAt":
						var key string
						json.Unmarshal(q.Params[1], &key)
						var slot uint64
						fmt.Sscanf(key, "0x%x", &slot)
						value, ok := slots[slot]
						if !ok {
							t.Error("unmodeled Crypto slot", slot)
							value = u(0)
						}
						out = "0x" + word(value)
						if bad == "storage-abi" {
							out = "0x01"
						}
					case "eth_call":
						var c map[string]string
						json.Unmarshal(q.Params[0], &c)
						selector := c["data"][:10]
						value := u(0)
						switch selector {
						case "0xfc0c546a":
							value = u(99)
							if bad == "token-getter" {
								value = u(77)
							}
						case "0x07546172":
							value = u(50)
							if bad == "minter" {
								value = u(51)
							}
						case selectorCoins:
							var i int
							fmt.Sscanf(c["data"][10:], "%x", &i)
							value = u(int64(i + 1))
							if bad == "coin-order" {
								value = u(7)
							}
						case selectorDecimalsWeighted:
							value = u(18)
							if bad == "decimals" {
								value = u(19)
							}
						case "0x18160ddd":
							value = balance
							if bad == "zero-supply" {
								value = u(0)
							}
						case "0x70a08231":
							if c["to"] != addr(99) {
								value = balance
								if bad == "raw-shortfall" {
									value = u(0)
								}
							}
						default:
							t.Error("unmodeled Crypto selector", selector)
						}
						out = "0x" + word(value)
					default:
						t.Error("unmodeled Crypto RPC", q.Method)
					}
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
			}))
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second*3, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
			block, pools, err := a.ReadState(context.Background(), "latest", true)
			if bad != "" && !strings.HasPrefix(bad, "ramp-online") {
				if err == nil {
					t.Fatal("invalid Crypto source admitted", bad, pools)
				}
				if bad == "after-anchor" && (!strings.Contains(err.Error(), "ANCHOR_REORGED") || heads != 2) {
					t.Fatal("did not reach read-after anchor", err, heads)
				}
				return
			}
			if err != nil || len(pools) != 1 {
				t.Fatal("missing Crypto parser", err, pools)
			}
			c := pools[0].MultiAsset.CurveCrypto
			if p.Protocol == "curve-crypto2-ramp" {
				if c == nil || c.Ramp == nil || c.Ramp.InitialTime != slots[7].Uint64() || c.FutureAGammaTime != slots[8].Uint64() {
					t.Fatal("missing anchored ramp schedule", c)
				}
				packedOrigin := new(big.Int).Or(new(big.Int).Lsh(c.Ramp.InitialAmp.Big(), 128), c.Ramp.InitialGamma.Big())
				if packedOrigin.Cmp(slots[5]) != 0 {
					t.Fatal("ramp origin lost packed source", c.Ramp)
				}
			} else if c != nil && c.Ramp != nil {
				t.Fatal("settled profile acquired ramp origin")
			}
			if c == nil || c.BlockHash != block.Hash || c.Timestamp*1000 != block.TimestampMS || c.RawBalances[0].String() != balance.String() || c.TotalSupply.String() != balance.String() || reads < 30 || heads != 2 {
				t.Fatal("incomplete Crypto state", c, reads, heads)
			}
		})
	}
}
