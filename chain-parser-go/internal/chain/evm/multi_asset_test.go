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

func TestMultiTokenWeightedReadsAllAnchoredDependenciesAndRejectsIncompleteVectors(t *testing.T) {
	for _, bad := range []string{"", "weights-length", "weights-sum", "third-code", "third-token", "third-scale", "third-decimals", "third-cash", "third-managed", "third-manager", "specialization", "offset-high", "after-anchor", "root"} {
		t.Run(bad, func(t *testing.T) {
			addr := func(n int) string { return fmt.Sprintf("0x%040x", n) }
			word := func(n int64) string { return fmt.Sprintf("%064x", n) }
			pad := func(a string) string { return strings.Repeat("0", 24) + a[2:] }
			code := []byte{0x60, 1, 0x60, 0, 0x55}
			sum := sha256.Sum256(code)
			digest := hex.EncodeToString(sum[:])
			id := addr(3) + "0001" + strings.Repeat("0", 19) + "1"
			v := &config.MultiAssetConfig{ExpectedCodeSHA256: digest, ExpectedVault: addr(5), ExpectedVaultCodeSHA256: digest, PoolID: id, TokenPolicy: "standard-no-bpt"}
			for _, n := range []int{1, 2, 4} {
				v.Tokens = append(v.Tokens, config.MultiTokenConfig{Token: addr(n), ExpectedCodeSHA256: digest})
			}
			p := config.PoolConfig{ID: addr(3), Protocol: "balancer-v2-weighted-multi", Token0: addr(1), Token1: addr(2), FeeNumerator: 1000000000000000, FeeDenominator: 1000000000000000000, MultiAsset: v}
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
					hash := "0x" + strings.Repeat("a", 64)
					if bad == "after-anchor" && heads > 1 {
						hash = "0x" + strings.Repeat("b", 64)
					}
					root := "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
					if bad == "root" {
						root = "0x" + strings.Repeat("f", 64)
					}
					out = map[string]any{"number": "0x64", "hash": hash, "parentHash": "0x" + strings.Repeat("c", 64), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": root}
				} else {
					var anchor map[string]any
					json.Unmarshal(q.Params[1], &anchor)
					if anchor["blockHash"] != "0x"+strings.Repeat("a", 64) || anchor["requireCanonical"] != true {
						t.Error("not hash pinned", anchor)
					}
					if q.Method == "eth_getCode" {
						var target string
						json.Unmarshal(q.Params[0], &target)
						reads[target+"/code"] = true
						out = "0x" + hex.EncodeToString(code)
						if bad == "third-code" && target == addr(4) {
							out = "0x6002"
						}
					} else if q.Method == "eth_call" {
						var c map[string]string
						json.Unmarshal(q.Params[0], &c)
						data := c["data"]
						sel := data[:10]
						reads[c["to"]+sel] = true
						switch sel {
						case selectorGetVault:
							out = "0x" + pad(v.ExpectedVault)
						case selectorGetPoolID:
							out = id
						case selectorGetPool:
							spec := int64(1)
							if bad == "specialization" {
								spec = 2
							}
							out = "0x" + pad(p.ID) + word(spec)
						case selectorPausedState:
							out = "0x" + word(0) + word(0) + word(0)
						case selectorWeights:
							n := int64(3)
							weight := int64(400000000000000000)
							if bad == "weights-length" {
								n = 2
							}
							if bad == "weights-sum" {
								weight--
							}
							out = "0x" + word(32) + word(n) + word(weight) + word(300000000000000000) + word(300000000000000000)
						case selectorScaling:
							scale := int64(1000000000000000000)
							if bad == "third-scale" {
								scale--
							}
							out = "0x" + word(32) + word(3) + word(1000000000000000000) + word(1000000000000000000) + word(scale)
						case selectorDecimalsWeighted:
							d := int64(18)
							if bad == "third-decimals" && c["to"] == addr(4) {
								d = 19
							}
							out = "0x" + word(d)
						case selectorPoolTokens:
							third := addr(4)
							if bad == "third-token" {
								third = addr(6)
							}
							offset := word(224)
							if bad == "offset-high" {
								offset = "01" + offset[2:]
							}
							out = "0x" + word(96) + offset + word(100) + word(3) + pad(addr(1)) + pad(addr(2)) + pad(third) + word(3) + word(1000000000000000000) + word(1000000000000000000) + word(1000000000000000000)
						case selectorTokenInfo:
							third := strings.HasSuffix(data, addr(4)[2:])
							cash := int64(1000000000000000000)
							managed, manager := int64(0), int64(0)
							if third && bad == "third-cash" {
								cash--
							}
							if third && bad == "third-managed" {
								managed = 1
							}
							if third && bad == "third-manager" {
								manager = 1
							}
							out = "0x" + word(cash) + word(managed) + word(100) + word(manager)
							reads[data] = true
						case selectorSwapFee:
							out = "0x" + word(1000000000000000)
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
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
			_, pools, e := a.ReadState(context.Background(), "latest", true)
			if bad != "" {
				if e == nil {
					t.Fatal("bad vector admitted", bad, pools)
				}
				return
			}
			if e != nil || len(pools) != 1 || pools[0].MultiAsset == nil || len(pools[0].MultiAsset.Assets) != 3 {
				t.Fatal(pools, e)
			}
			for _, token := range []string{addr(1), addr(2), addr(4)} {
				if !reads[token+"/code"] || !reads[token+selectorDecimalsWeighted] || !reads[selectorTokenInfo+id[2:]+strings.Repeat("0", 24)+token[2:]] {
					t.Fatal("missing token dependency", token)
				}
			}
		})
	}
}
