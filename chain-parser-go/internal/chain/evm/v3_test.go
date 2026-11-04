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

func abiInt(n int64) string {
	v := big.NewInt(n)
	if n < 0 {
		v.Add(v, new(big.Int).Lsh(big.NewInt(1), 256))
	}
	return fmt.Sprintf("%064x", v)
}
func TestV3HashPinnedReadChecksCodeBitmapAndEveryTick(t *testing.T) {
	for _, mode := range []string{"ok", "code", "missing-tick", "bad-net", "locked", "anchor", "word-budget", "minimum-price", "pancake-ok", "pancake-lm", "pancake-bad-protocol-fee", "uniswap-wide-protocol-fee", "slipstream-ok", "slipstream-module", "slipstream-unstaked-module", "slipstream-clone-unpinned", "factory-code-ok", "factory-code-mismatch"} {
		t.Run(mode, func(t *testing.T) {
			fixture := "../../../test/contracts/v3-state.json"
			if mode == "minimum-price" {
				fixture = "../../../test/contracts/v3-minimum-state.json"
			}
			raw, e := os.ReadFile(fixture)
			if e != nil {
				t.Fatal(e)
			}
			var pool domain.Pool
			if e = json.Unmarshal(raw, &pool); e != nil {
				t.Fatal(e)
			}
			if strings.HasPrefix(mode, "slipstream-") {
				pool.Protocol = "aerodrome-slipstream"
			}
			if strings.HasPrefix(mode, "pancake-") {
				pool.Protocol = "pancake-v3"
			}
			code := []byte{0x60, 1, 0x60, 0, 0x55}
			if mode == "slipstream-clone-unpinned" {
				code, _ = hex.DecodeString("363d3d373d3d3d363d73" + strings.Repeat("6", 40) + "5af43d82803e903d91602b57fd5bf3")
			}
			digest := sha256.Sum256(code)
			cfg := config.PoolConfig{ID: pool.PoolID, Protocol: pool.Protocol, Token0: pool.Token0, Token1: pool.Token1, FeeNumerator: pool.FeeNumerator, FeeDenominator: 1000000, V3: &config.V3PoolConfig{ExpectedFactory: pool.V3.Factory, ExpectedCodeSHA256: hex.EncodeToString(digest[:]), TickWordRadius: 1, MaxInitializedTicks: 32, MaxSteps: 512}}
			if strings.HasPrefix(mode, "slipstream-") {
				cfg.V3.ExpectedFactoryCodeSHA256 = cfg.V3.ExpectedCodeSHA256
			}
			if strings.HasPrefix(mode, "factory-code-") {
				cfg.V3.ExpectedFactoryCodeSHA256 = cfg.V3.ExpectedCodeSHA256
				if mode == "factory-code-mismatch" {
					cfg.V3.ExpectedFactoryCodeSHA256 = strings.Repeat("b", 64)
				}
			}
			if mode == "word-budget" {
				cfg.V3.MaxInitializedTicks = 1
			}
			pinned := 0
			headers := 0
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var q struct {
					Method string            `json:"method"`
					Params []json.RawMessage `json:"params"`
				}
				if json.NewDecoder(r.Body).Decode(&q) != nil {
					t.Error("request")
					return
				}
				var out any
				switch q.Method {
				case "eth_getBlockByNumber":
					headers++
					hash := "0x" + strings.Repeat("1", 64)
					if mode == "anchor" && headers > 1 {
						hash = "0x" + strings.Repeat("2", 64)
					}
					out = map[string]any{"number": "0x64", "hash": hash, "parentHash": "0x" + strings.Repeat("0", 64), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
				case "eth_getCode", "eth_call":
					var anchor map[string]any
					json.Unmarshal(q.Params[1], &anchor)
					if anchor["blockHash"] != "0x"+strings.Repeat("1", 64) || anchor["requireCanonical"] != true {
						t.Error("unpinned V3 dependency")
					}
					pinned++
					if q.Method == "eth_getCode" {
						out = "0x" + hex.EncodeToString(code)
						if mode == "code" {
							out = "0x00"
						}
						break
					}
					var call map[string]string
					json.Unmarshal(q.Params[0], &call)
					selector := call["data"][:10]
					switch selector {
					case "0x0dfe1681":
						out = "0x" + strings.Repeat("0", 24) + pool.Token0[2:]
					case "0xd21220a7":
						out = "0x" + strings.Repeat("0", 24) + pool.Token1[2:]
					case "0xc45a0155":
						out = "0x" + strings.Repeat("0", 24) + pool.V3.Factory[2:]
					case selectorUnstakedFeeModule:
						out = "0x" + abiInt(0)
						if mode == "slipstream-unstaked-module" {
							out = "0x" + abiInt(1)
						}
					case selectorSwapFeeModule:
						out = "0x" + abiInt(0)
						if mode == "slipstream-module" {
							out = "0x" + abiInt(1)
						}
					case "0x540d4918":
						out = "0x" + abiInt(0)
						if mode == "pancake-lm" {
							out = "0x" + abiInt(1)
						}
					case "0xddca3f43":
						out = "0x" + abiInt(int64(pool.FeeNumerator))
					case "0xd0c93a7c":
						out = "0x" + abiInt(int64(pool.V3.TickSpacing))
					case "0x1a686502":
						out = "0x" + fmt.Sprintf("%064x", pool.V3.Liquidity.Big())
					case "0x3850c7bd":
						unlocked := int64(1)
						if mode == "locked" {
							unlocked = 0
						}
						feeProtocol := int64(0)
						if strings.HasPrefix(mode, "pancake-") || mode == "uniswap-wide-protocol-fee" {
							feeProtocol = 209718400
						}
						if mode == "pancake-bad-protocol-fee" {
							feeProtocol = 65535
						}
						out = "0x" + fmt.Sprintf("%064x", pool.V3.SqrtPriceX96.Big()) + abiInt(int64(pool.V3.Tick)) + abiInt(0) + abiInt(1) + abiInt(1) + abiInt(feeProtocol) + abiInt(unlocked)
						if strings.HasPrefix(mode, "slipstream-") {
							out = "0x" + fmt.Sprintf("%064x", pool.V3.SqrtPriceX96.Big()) + abiInt(int64(pool.V3.Tick)) + abiInt(0) + abiInt(1) + abiInt(1) + abiInt(unlocked)
						}
					case "0x5339c296":
						arg, _ := new(big.Int).SetString(call["data"][10:], 16)
						if arg.Bit(255) != 0 {
							arg.Sub(arg, new(big.Int).Lsh(big.NewInt(1), 256))
						}
						value := big.NewInt(0)
						for _, word := range pool.V3.TickWords {
							if int64(word.WordPos) == arg.Int64() {
								value = word.Bitmap.Big()
							}
						}
						out = "0x" + fmt.Sprintf("%064x", value)
					case "0xf30dba93":
						arg, _ := new(big.Int).SetString(call["data"][10:], 16)
						if arg.Bit(255) != 0 {
							arg.Sub(arg, new(big.Int).Lsh(big.NewInt(1), 256))
						}
						found := false
						for _, tick := range pool.V3.Ticks {
							if int64(tick.Tick) == arg.Int64() {
								net, _ := domain.ParseLiquidityNet(tick.LiquidityNet)
								if mode == "bad-net" {
									net.SetString("99999999999999999999999999999999999999", 10)
								}
								if net.Sign() < 0 {
									net.Add(net, new(big.Int).Lsh(big.NewInt(1), 256))
								}
								initialized := int64(1)
								if mode == "missing-tick" {
									initialized = 0
								}
								out = "0x" + fmt.Sprintf("%064x%064x", tick.LiquidityGross.Big(), net) + strings.Repeat("0", 64*5) + abiInt(initialized)
								if strings.HasPrefix(mode, "slipstream-") {
									out = "0x" + fmt.Sprintf("%064x%064x", tick.LiquidityGross.Big(), net) + strings.Repeat("0", 64*7) + abiInt(initialized)
								}
								found = true
							}
						}
						if !found {
							t.Error("unproven tick requested")
							out = "0x"
						}
					default:
						t.Error("unexpected selector", selector)
						out = "0x"
					}
				default:
					t.Error("unexpected RPC", q.Method)
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": out})
			}))
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: pool.ChainKey, Pools: []config.PoolConfig{cfg}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if mode != "ok" && mode != "minimum-price" && mode != "pancake-ok" && mode != "slipstream-ok" && mode != "factory-code-ok" {
				if e == nil {
					t.Fatal("bad V3 state accepted", mode)
				}
				return
			}
			if e != nil || len(pools) != 1 || pools[0].V3 == nil {
				t.Fatal(pools, e)
			}
			if mode == "pancake-ok" && (pools[0].Protocol != "pancake-v3" || pools[0].V3.FeeProtocol != 209718400) {
				t.Fatal("wrong version/layout")
			}
			wantWords := 3
			if mode == "minimum-price" {
				wantWords = 2
			}
			if pinned < 8+wantWords+len(pool.V3.Ticks) || len(pools[0].V3.TickWords) != wantWords || len(pools[0].V3.Ticks) != len(pool.V3.Ticks) || pools[0].V3.Ticks[len(pool.V3.Ticks)-1].LiquidityNet != pool.V3.Ticks[len(pool.V3.Ticks)-1].LiquidityNet {
				t.Fatal("dependencies incomplete", pinned)
			}
		})
	}
}
