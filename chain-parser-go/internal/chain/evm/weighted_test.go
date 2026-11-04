package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestWeightedFixedBlockAndUnsupportedDependencies(t *testing.T) {
	for _, protocol := range []string{"balancer-v2-weighted2-equal", "balancer-v2-weighted2-static"} {
		for _, bad := range []string{"", "pool-code", "vault-code", "token-code", "clone", "clone-pinned", "weight-min", "vault", "pool-id", "registration", "specialization0", "specialization1", "weight", "weight-count", "scale", "decimals", "vault-pause", "pool-pause", "token-order", "array-offset", "manager", "managed", "cash", "cash-width", "change-block", "fee-low", "fee-high", "after-anchor", "receipts-root"} {
			t.Run(protocol+"/"+bad, func(t *testing.T) {
				addr := func(n int) string { return fmt.Sprintf("0x%040x", n) }
				word := func(n int64) string { return fmt.Sprintf("%064x", n) }
				pad := func(s string) string { return strings.Repeat("0", 24) + s[2:] }
				code := []byte{0x60, 1, 0x60, 0, 0x55}
				if bad == "clone-pinned" {
					code, _ = hex.DecodeString("363d3d373d3d3d363d73" + strings.Repeat("6", 40) + "5af43d82803e903d91602b57fd5bf3")
				}
				sum := sha256.Sum256(code)
				digest := hex.EncodeToString(sum[:])
				id := addr(3) + "0002" + strings.Repeat("0", 19) + "1"
				v := &config.WeightedPoolConfig{ExpectedCodeSHA256: digest, ExpectedVault: addr(5), ExpectedVaultCodeSHA256: digest, PoolID: id, ExpectedToken0CodeSHA256: digest, ExpectedToken1CodeSHA256: digest, TokenPolicy: "standard-no-bpt"}
				p := config.PoolConfig{ID: addr(3), Protocol: protocol, Token0: addr(1), Token1: addr(2), FeeNumerator: 1000000000000000, FeeDenominator: 1000000000000000000, Weighted: v}
				pinned, heads := 0, 0
				seen := map[string]bool{}
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
					var out any
					if q.Method == "eth_getBlockByNumber" {
						heads++
						hash := "0x" + strings.Repeat("a", 64)
						if bad == "after-anchor" && heads > 1 {
							hash = "0x" + strings.Repeat("c", 64)
						}
						root := "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
						if bad == "receipts-root" {
							root = "0x" + strings.Repeat("f", 64)
						}
						out = map[string]any{"number": "0x64", "hash": hash, "parentHash": "0x" + strings.Repeat("b", 64), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": root}
					} else {
						var anchor map[string]any
						json.Unmarshal(q.Params[1], &anchor)
						if anchor["blockHash"] != "0x"+strings.Repeat("a", 64) || anchor["requireCanonical"] != true {
							t.Error("not hash-pinned", anchor)
						}
						pinned++
						if q.Method == "eth_getCode" {
							var target string
							json.Unmarshal(q.Params[0], &target)
							out = "0x" + hex.EncodeToString(code)
							if bad == "pool-code" && target == p.ID || bad == "vault-code" && target == v.ExpectedVault || bad == "token-code" && target == p.Token0 {
								out = "0x6002"
							}
							if bad == "clone" && target == p.ID {
								out = "0x363d3d373d3d3d363d73" + strings.Repeat("6", 40) + "5af43d82803e903d91602b57fd5bf3"
							}
						} else if q.Method == "eth_call" {
							var c map[string]string
							json.Unmarshal(q.Params[0], &c)
							data := c["data"]
							sel := data[:10]
							seen[c["to"]+sel] = true
							switch sel {
							case selectorGetVault:
								out = "0x" + pad(v.ExpectedVault)
								if bad == "vault" {
									out = "0x" + pad(addr(6))
								}
							case selectorGetPoolID:
								out = id
								if bad == "pool-id" {
									out = "0x" + strings.Repeat("0", 64)
								}
							case selectorGetPool:
								pool := p.ID
								spec := int64(2)
								if bad == "registration" {
									pool = addr(8)
								}
								if bad == "specialization0" {
									spec = 0
								}
								if bad == "specialization1" {
									spec = 1
								}
								out = "0x" + pad(pool) + word(spec)
							case selectorPausedState:
								flag := int64(0)
								if bad == "vault-pause" && c["to"] == v.ExpectedVault || bad == "pool-pause" && c["to"] == p.ID {
									flag = 1
								}
								out = "0x" + word(flag) + word(0) + word(0)
							case selectorWeights:
								weight, count := int64(500000000000000000), int64(2)
								weight1 := int64(500000000000000000)
								if protocol == "balancer-v2-weighted2-static" {
									weight = 800000000000000000
									weight1 = 200000000000000000
								}
								if bad == "weight-min" {
									weight = 9999999999999999
									weight1 = 1000000000000000000 - weight
								}
								if bad == "weight" {
									weight--
								}
								if bad == "weight-count" {
									count = 3
								}
								out = "0x" + word(32) + word(count) + word(weight) + word(weight1)
							case selectorScaling:
								scale := int64(1000000000000000000)
								if bad == "scale" {
									scale--
								}
								out = "0x" + word(32) + word(2) + word(scale) + word(scale)
							case selectorDecimalsWeighted:
								d := int64(18)
								if bad == "decimals" {
									d = 19
								}
								out = "0x" + word(d)
							case selectorPoolTokens:
								t0, t1 := p.Token0, p.Token1
								if bad == "token-order" {
									t0, t1 = t1, t0
								}
								offset := word(96)
								if bad == "array-offset" {
									offset = "01" + offset[2:]
								}
								balance := word(1000000000000000000)
								if bad == "cash-width" {
									balance = "0000000000000000000000000000000000010000000000000000000000000000"
								}
								out = "0x" + offset + word(192) + word(100) + word(2) + pad(t0) + pad(t1) + word(2) + balance + balance
							case selectorTokenInfo:
								cash, managed, last := int64(1000000000000000000), int64(0), int64(100)
								manager := word(0)
								if bad == "cash" {
									cash--
								}
								if bad == "manager" {
									manager = pad(addr(9))
								}
								if bad == "managed" {
									managed = 1
								}
								if bad == "change-block" {
									last = 101
								}
								out = "0x" + word(cash) + word(managed) + word(last) + manager
							case selectorSwapFee:
								f := int64(1000000000000000)
								if bad == "fee-low" {
									f = 999999999999
								}
								if bad == "fee-high" {
									f = 100000000000000001
								}
								out = "0x" + word(f)
							default:
								t.Error("unexpected selector", sel)
								out = "0x"
							}
						} else {
							t.Error("unexpected method", q.Method)
						}
					}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
				}))
				defer server.Close()
				client := rpc.New([]string{server.URL}, time.Second, 1<<20)
				a := &Adapter{Client: client, Chain: config.Chain{Key: "evm:31337", Confirmation: "latest", Pools: []config.PoolConfig{p}}}
				block, pools, e := a.ReadState(context.Background(), "latest", true)
				if bad != "" {
					if e == nil {
						t.Fatal("unsupported dependency accepted", bad, pools)
					}
					return
				}
				if e != nil || block.Number != 100 || len(pools) != 1 || pools[0].Weighted == nil || pinned < 17 {
					t.Fatal(block, pools, pinned, e)
				}
				for _, target := range []string{p.Token0, p.Token1} {
					if !seen[target+selectorDecimalsWeighted] {
						t.Fatal("missing decimals", target)
					}
				}
				if heads < 2 {
					t.Fatal("missing after-anchor")
				}
			})
		}
	}
}
