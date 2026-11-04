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

func TestVersionedV2ReadsIdentityAndFixedFee(t *testing.T) {
	for _, name := range []string{"pancake-v1", "pancake-v2", "sushiswap-v2"} {
		for _, bad := range []string{"", "code", "factory", "token-padding", "reserve-time", "fee"} {
			t.Run(name+bad, func(t *testing.T) {
				profile, _ := domain.Protocol(name)
				code := []byte{0x60, 1, 0x60, 0, 0x55}
				hash := sha256.Sum256(code)
				p := config.PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: name, Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeNumerator: profile.FeeNumerator, FeeDenominator: profile.FeeDenominator, V2: &config.V2PoolConfig{ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedCodeSHA256: hex.EncodeToString(hash[:])}}
				if bad == "fee" {
					p.FeeNumerator++
				}
				reads := 0
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					var q struct {
						Method string            `json:"method"`
						Params []json.RawMessage `json:"params"`
					}
					json.NewDecoder(r.Body).Decode(&q)
					var out string
					if q.Method == "eth_getBlockByNumber" {
						json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": map[string]any{"number": "0x64", "hash": "0x" + strings.Repeat("a", 64), "parentHash": "0x" + strings.Repeat("b", 64), "timestamp": "0x64", "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}})
						return
					}
					var anchor map[string]any
					json.Unmarshal(q.Params[1], &anchor)
					if anchor["blockHash"] != "0x"+strings.Repeat("a", 64) || anchor["requireCanonical"] != true {
						t.Error("unpinned dependency")
					}
					reads++
					if q.Method == "eth_getCode" {
						out = "0x" + hex.EncodeToString(code)
						if bad == "code" {
							out = "0x00"
						}
					} else {
						var call map[string]string
						json.Unmarshal(q.Params[0], &call)
						switch call["data"] {
						case "0x0dfe1681":
							out = "0x" + strings.Repeat("0", 24) + p.Token0[2:]
							if bad == "token-padding" {
								out = "0x01" + out[4:]
							}
						case "0xd21220a7":
							out = "0x" + strings.Repeat("0", 24) + p.Token1[2:]
						case "0xc45a0155":
							out = "0x" + strings.Repeat("0", 24) + p.V2.ExpectedFactory[2:]
							if bad == "factory" {
								out = "0x" + strings.Repeat("0", 64)
							}
						case "0x0902f1ac":
							out = "0x" + fmt.Sprintf("%064x%064x", 1000000, 2000000) + abiInt(100)
							if bad == "reserve-time" {
								out = out[:130] + fmt.Sprintf("%064x", uint64(1)<<32)
							}
						default:
							t.Error("unexpected method")
						}
					}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": out})
				}))
				defer server.Close()
				adapter := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
				_, pools, e := adapter.ReadState(context.Background(), "latest", true)
				if bad != "" {
					if e == nil {
						t.Fatal("invalid versioned state accepted")
					}
					return
				}
				if e != nil || len(pools) != 1 || pools[0].AdapterVersion != profile.AdapterVersion || pools[0].V2 == nil || reads != 5 {
					t.Fatal(pools, e, reads)
				}
			})
		}
	}
}
