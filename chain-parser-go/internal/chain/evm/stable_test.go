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
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestStableHashPinnedModelsAndDependencyRejections(t *testing.T) {
	for _, protocol := range []string{"velodrome-v2-volatile", "velodrome-v2-stable", "aerodrome-v2-volatile", "aerodrome-v2-stable", "curve-plain2-basic", "balancer-v2-stable2"} {
		for _, bad := range []string{"", "code", "identity", "model", "abi-high-bits", "rate", "vault-paused", "array-offset-high", "clone-unpinned"} {
			t.Run(protocol+bad, func(t *testing.T) {
				profile, _ := domain.Protocol(protocol)
				code := []byte{0x60, 1, 0x60, 0, 0x55}
				if bad == "clone-unpinned" {
					code, _ = hex.DecodeString("363d3d373d3d3d363d73" + strings.Repeat("6", 40) + "5af43d82803e903d91602b57fd5bf3")
				}
				hash := sha256.Sum256(code)
				digest := hex.EncodeToString(hash[:])
				address := func(n int) string { return fmt.Sprintf("0x%040x", n) }
				state := &config.StablePoolConfig{ExpectedCodeSHA256: digest}
				if profile.Family == "aero" {
					state.ExpectedFactory = address(4)
					state.ExpectedFactoryCodeSHA256 = digest
				}
				if profile.Family == "balancer" {
					state.ExpectedVault = address(5)
					state.ExpectedVaultCodeSHA256 = digest
					state.PoolID = address(3) + strings.Repeat("0", 24)
				}
				p := config.PoolConfig{ID: address(3), Protocol: protocol, Token0: address(1), Token1: address(2), FeeDenominator: profile.FeeDenominator, Stable: state}
				pinned := 0
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					var q struct {
						Method string            `json:"method"`
						Params []json.RawMessage `json:"params"`
					}
					json.NewDecoder(r.Body).Decode(&q)
					var out any
					if q.Method == "eth_getBlockByNumber" {
						out = map[string]any{"number": "0x64", "hash": "0x" + strings.Repeat("a", 64), "parentHash": "0x" + strings.Repeat("b", 64), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
					} else {
						var anchor map[string]any
						index := 1
						if q.Method == "eth_getStorageAt" {
							index = 2
						}
						json.Unmarshal(q.Params[index], &anchor)
						if anchor["blockHash"] != "0x"+strings.Repeat("a", 64) || anchor["requireCanonical"] != true {
							t.Error("unpinned stable dependency")
						}
						pinned++
						if q.Method == "eth_getCode" {
							out = "0x" + hex.EncodeToString(code)
							if bad == "code" {
								out = "0x00"
							}
						} else if q.Method == "eth_getStorageAt" {
							out = "0x" + abiInt(1000000000000000000)
							if bad == "rate" {
								out = "0x" + abiInt(0)
							}
						} else {
							var call map[string]string
							json.Unmarshal(q.Params[0], &call)
							selector := call["data"][:10]
							pad := func(a string) string { return strings.Repeat("0", 24) + a[2:] }
							switch selector {
							case selectorIsPaused:
								out = "0x" + abiInt(0)
							case "0xc45a0155":
								out = "0x" + pad(state.ExpectedFactory)
								if bad == "identity" {
									out = "0x" + pad(address(6))
								}
							case selectorMetadata:
								flag := int64(0)
								if protocol == "aerodrome-v2-stable" || protocol == "velodrome-v2-stable" {
									flag = 1
								}
								if bad == "model" {
									flag = 1 - flag
								}
								out = "0x" + abiInt(1000000000000000000) + abiInt(1000000000000000000) + abiInt(1000000) + abiInt(1000000) + abiInt(flag) + pad(p.Token0) + pad(p.Token1)
								if bad == "rate" {
									out = "0x" + abiInt(0) + out.(string)[66:]
								}
							case selectorGetFee:
								out = "0x" + abiInt(4)
							case selectorCoins:
								n := 0
								fmt.Sscanf(call["data"][10:], "%x", &n)
								token := p.Token0
								if strings.HasSuffix(call["data"], "1") {
									token = p.Token1
								}
								if bad == "identity" {
									token = address(6)
								}
								out = "0x" + pad(token)
							case selectorBalances:
								out = "0x" + abiInt(1000000000000000000)
							case selectorFee:
								out = "0x" + abiInt(4000000)
							case selectorAPrecise:
								out = "0x" + abiInt(10000)
								if bad == "model" {
									out = "0x" + abiInt(0)
								}
							case selectorAdminFee:
								out = "0x" + abiInt(5000000000)
							case selectorGetVault:
								out = "0x" + pad(state.ExpectedVault)
								if bad == "identity" {
									out = "0x" + pad(address(6))
								}
							case selectorGetPoolID:
								out = state.PoolID
							case selectorGetPool:
								out = "0x" + pad(p.ID) + abiInt(2)
							case selectorPausedState:
								paused := int64(0)
								if bad == "vault-paused" && call["to"] == state.ExpectedVault {
									paused = 1
								}
								out = "0x" + abiInt(paused) + abiInt(0) + abiInt(0)
								if bad == "model" {
									out = "0x" + abiInt(1) + abiInt(0) + abiInt(0)
								}
							case selectorAmplification:
								out = "0x" + abiInt(100000) + abiInt(0) + abiInt(1000)
							case selectorScaling:
								out = "0x" + abiInt(32) + abiInt(2) + abiInt(1000000000000000000) + abiInt(1000000000000000000)
								if bad == "array-offset-high" {
									out = "0x01" + out.(string)[4:]
								}
								if bad == "rate" {
									out = "0x" + abiInt(32) + abiInt(2) + abiInt(0) + abiInt(1000000000000000000)
								}
							case selectorPoolTokens:
								out = "0x" + abiInt(96) + abiInt(192) + abiInt(100) + abiInt(2) + pad(p.Token0) + pad(p.Token1) + abiInt(2) + abiInt(1000000) + abiInt(1000000)
							case selectorSwapFee:
								out = "0x" + abiInt(1000000000000000)
							default:
								t.Error("unexpected stable RPC", selector)
								out = "0x"
							}
							if bad == "abi-high-bits" {
								v := out.(string)
								out = "0x01" + v[4:]
							}
						}
					}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": out})
				}))
				defer server.Close()
				a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
				_, pools, e := a.ReadState(context.Background(), "latest", true)
				if (bad == "vault-paused" || bad == "array-offset-high") && profile.Family != "balancer" {
					return
				}
				if bad != "" {
					if e == nil {
						t.Fatal("invalid stable state accepted", bad)
					}
					return
				}
				if e != nil || len(pools) != 1 || pools[0].Stable == nil || pools[0].AdapterVersion != profile.AdapterVersion || pinned < 4 {
					t.Fatal(pools, e, pinned)
				}
			})
		}
	}
}

func TestStandardCloneRequiresPinnedImplementationAndPublishesNoGuess(t *testing.T) {
	implementation := "0x" + strings.Repeat("6", 40)
	runtime, _ := hex.DecodeString("363d3d373d3d3d363d73" + implementation[2:] + "5af43d82803e903d91602b57fd5bf3")
	body := []byte{0x60, 1, 0x55}
	digest := sha256.Sum256(body)
	reads := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		reads++
		var q struct {
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		json.NewDecoder(r.Body).Decode(&q)
		var address string
		json.Unmarshal(q.Params[0], &address)
		if q.Method != "eth_getCode" || address != implementation {
			t.Error("wrong implementation dependency")
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": "0x" + hex.EncodeToString(body)})
	}))
	defer server.Close()
	a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20)}
	if e := a.verifyClone(context.Background(), runtime, "", "", map[string]any{}); e == nil {
		t.Fatal("unidentified clone accepted")
	}
	if e := a.verifyClone(context.Background(), runtime, implementation, hex.EncodeToString(digest[:]), map[string]any{"blockHash": "anchored", "requireCanonical": true}); e != nil {
		t.Fatal(e)
	}
	if reads != 1 {
		t.Fatal("implementation not read exactly once")
	}
}
