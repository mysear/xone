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
	"xone/chain-parser-go/internal/rpc"
)

func TestCurveNGAnchoredImmutableTypeBalancesAndOracleState(t *testing.T) {
	raw, err := os.ReadFile("../../../test/contracts/curve-ng-runtime-template.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Template string `json:"runtime_template_hex"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	template, err := hex.DecodeString(fixture.Template)
	if err != nil {
		t.Fatal(err)
	}

	for _, n := range []int{2, 3, 8} {
		for _, bad := range []string{"", "asset-type", "oracle-provider", "rebasing", "coin-order", "coin-count", "rate", "decimals", "storage-length", "stored-admin", "future-ma", "time-width", "bad-price", "fee-domain", "proxy", "template", "after-anchor", "receipt-root"} {
			t.Run(fmt.Sprint(n)+"/"+bad, func(t *testing.T) {
				addr := func(i int) string { return fmt.Sprintf("0x%040x", i) }
				one := new(big.Int).Exp(big.NewInt(10), big.NewInt(18), nil)
				balance := new(big.Int).Exp(big.NewInt(10), big.NewInt(24), nil)
				pack := func(a, b *big.Int) *big.Int {
					return new(big.Int).Or(new(big.Int).Set(a), new(big.Int).Lsh(new(big.Int).Set(b), 128))
				}
				word := func(v *big.Int) string { return fmt.Sprintf("%064x", v) }
				immutable := make([]byte, curveNGImmutableBytes)
				set := func(offset int, value *big.Int) { value.FillBytes(immutable[offset : offset+32]) }
				for _, offset := range []int{0, 32, 96, 384, 704, 992, 1280, 1568} {
					set(offset, big.NewInt(int64(n)))
				}
				set(64, big.NewInt(90))
				for i := 0; i < n; i++ {
					set(128+32*i, big.NewInt(int64(i+1)))
					set(736+32*i, one)
				}
				if bad == "asset-type" {
					set(416, big.NewInt(3))
				}
				if bad == "oracle-provider" {
					set(1024, big.NewInt(1))
				}
				if bad == "rebasing" {
					set(672, big.NewInt(1))
				}
				if bad == "coin-order" {
					set(128, big.NewInt(5))
				}
				if bad == "coin-count" {
					set(0, big.NewInt(9))
				}
				if bad == "rate" {
					set(736, big.NewInt(1))
				}
				code := append(append([]byte{}, template...), immutable...)
				if bad == "template" {
					code[0] ^= 1
				}
				if bad == "proxy" {
					code, _ = hex.DecodeString("363d3d373d3d3d363d73" + strings.Repeat("6", 40) + "5af43d82803e903d91602b57fd5bf3")
				}
				hash := sha256.Sum256(code)
				tokenCode := []byte{0x60, 2, 0x55}
				tokenHash := sha256.Sum256(tokenCode)
				v := &config.MultiAssetConfig{ExpectedCodeSHA256: hex.EncodeToString(hash[:]), TokenPolicy: "standard-no-bpt"}
				for i := 0; i < n; i++ {
					v.Tokens = append(v.Tokens, config.MultiTokenConfig{Token: addr(i + 1), ExpectedCodeSHA256: hex.EncodeToString(tokenHash[:])})
				}
				p := config.PoolConfig{ID: addr(50), Protocol: "curve-stableswap-ng", Token0: addr(1), Token1: addr(2), FeeNumerator: 4000000, FeeDenominator: 10000000000, MultiAsset: v}
				slots := map[uint64]*big.Int{1: big.NewInt(int64(n)), 16: big.NewInt(int64(n)), 25: big.NewInt(int64(n - 1)), 34: pack(new(big.Int).Mul(balance, big.NewInt(int64(n))), new(big.Int).Mul(balance, big.NewInt(int64(n)))), 37: pack(big.NewInt(90), big.NewInt(90))}
				for i := 0; i < n; i++ {
					slots[uint64(2+i)] = new(big.Int).Add(balance, big.NewInt(int64(100+i)))
				}
				for i := 0; i < n-1; i++ {
					slots[uint64(26+i)] = pack(one, one)
				}
				if bad == "storage-length" {
					slots[25] = big.NewInt(8)
				}
				if bad == "stored-admin" {
					slots[2] = new(big.Int).Set(balance)
				}
				if bad == "future-ma" {
					slots[37] = pack(big.NewInt(101), big.NewInt(90))
				}
				if bad == "bad-price" {
					slots[26] = pack(new(big.Int).Mul(one, big.NewInt(3)), one)
				}
				reads := 0
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
						anchorParam := 1
						if q.Method == "eth_getStorageAt" {
							anchorParam = 2
						}
						json.Unmarshal(q.Params[anchorParam], &anchor)
						if anchor["blockHash"] != "0x"+strings.Repeat("a", 64) || anchor["requireCanonical"] != true {
							t.Error("unanchored dependency", q.Method, anchor)
						}
						if q.Method == "eth_getCode" {
							var to string
							json.Unmarshal(q.Params[0], &to)
							if to == p.ID {
								out = "0x" + hex.EncodeToString(code)
							} else {
								out = "0x" + hex.EncodeToString(tokenCode)
							}
						} else if q.Method == "eth_getStorageAt" {
							var key string
							json.Unmarshal(q.Params[1], &key)
							var slot uint64
							fmt.Sscanf(key, "0x%x", &slot)
							value, ok := slots[slot]
							if !ok {
								t.Error("unmodeled storage slot", key)
								value = big.NewInt(0)
							}
							out = "0x" + word(value)
						} else if q.Method == "eth_call" {
							var call map[string]string
							json.Unmarshal(q.Params[0], &call)
							sel := call["data"][:10]
							switch sel {
							case "0x29357750":
								out = "0x" + word(big.NewInt(int64(n)))
							case "0xfd0684b1", "0x14f05979":
								out = "0x" + word(big.NewInt(32)) + word(big.NewInt(int64(n)))
								for i := 0; i < n; i++ {
									if sel == "0xfd0684b1" {
										out = out.(string) + word(one)
									} else {
										out = out.(string) + word(balance)
									}
								}
							case selectorCoins:
								var index int
								fmt.Sscanf(call["data"][10:], "%x", &index)
								out = "0x" + word(big.NewInt(int64(index+1)))
							case selectorDecimalsWeighted:
								d := int64(18)
								if bad == "decimals" {
									d = 19
								}
								out = "0x" + word(big.NewInt(d))
							case "0xe2e7d264":
								var index int
								fmt.Sscanf(call["data"][10:], "%x", &index)
								out = "0x" + word(big.NewInt(int64(100+index)))
							case curveNGSelectorMAExpTime, curveNGSelectorDMaTime:
								value := big.NewInt(600)
								if bad == "time-width" {
									value = new(big.Int).Lsh(big.NewInt(1), 65)
								}
								out = "0x" + word(value)
							case selectorAPrecise:
								out = "0x" + word(big.NewInt(10000))
							case selectorFee:
								fee := int64(4000000)
								if bad == "fee-domain" {
									fee = 5000000001
								}
								out = "0x" + word(big.NewInt(fee))
							case selectorAdminFee:
								out = "0x" + word(big.NewInt(5000000000))
							case "0x8edfdd5f":
								out = "0x" + word(big.NewInt(20000000000))
							default:
								t.Error("unmodeled selector", sel)
								out = "0x"
							}
						} else {
							t.Error("unexpected RPC", q.Method)
						}
					}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
				}))
				defer server.Close()
				a := Adapter{Client: rpc.New([]string{server.URL}, time.Second*3, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
				block, pools, e := a.ReadState(context.Background(), "latest", true)
				if bad != "" {
					if e == nil {
						t.Fatal("unsupported NG state admitted", bad, pools)
					}
					return
				}
				if e != nil || len(pools) != 1 {
					t.Fatal(pools, e)
				}
				s := pools[0].MultiAsset
				if s.CurveNG == nil || len(s.Assets) != n || s.CurveNG.BlockHash != block.Hash || s.CurveNG.Timestamp*1000 != block.TimestampMS || s.CurveNG.AdminBalances[n-1].String() != fmt.Sprint(99+n) || reads < 20 || heads != 2 {
					t.Fatal("incomplete NG snapshot", s, reads, heads)
				}
			})
		}
	}
}
