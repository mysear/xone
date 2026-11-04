package evm

import (
	"context"
	"encoding/json"
	"fmt"
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

func TestManagedFeeLedgerUsesOneBlockHashAndCoalescesRecipients(t *testing.T) {
	for _, bad := range []string{"", "same-recipient", "recovery", "delegation", "collector", "owner-width", "supply", "protocol-fee", "owner-balance", "short-word", "time"} {
		t.Run(bad, func(t *testing.T) {
			raw, e := os.ReadFile("../../../test/contracts/managed-fees-wire.json")
			if e != nil {
				t.Fatal(e)
			}
			var f struct {
				Snapshot domain.Snapshot `json:"snapshot"`
			}
			if e = json.Unmarshal(raw, &f); e != nil {
				t.Fatal(e)
			}
			model := f.Snapshot.Pools[0]
			owner := fmt.Sprintf("0x%040x", 90)
			collector := fmt.Sprintf("0x%040x", 91)
			if bad == "same-recipient" {
				collector = owner
			}
			hash := "0x" + strings.Repeat("a", 64)
			balanceReads := 0
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var q struct {
					ID     json.RawMessage   `json:"id"`
					Method string            `json:"method"`
					Params []json.RawMessage `json:"params"`
				}
				json.NewDecoder(r.Body).Decode(&q)
				if q.Method != "eth_call" {
					t.Error("unexpected method", q.Method)
				}
				var anchor map[string]any
				json.Unmarshal(q.Params[1], &anchor)
				if anchor["blockHash"] != hash || anchor["requireCanonical"] != true {
					t.Error("unanchored BPT ledger read", anchor)
				}
				var call map[string]string
				json.Unmarshal(q.Params[0], &call)
				sel := call["data"][:10]
				word := func(v uint64) string { return fmt.Sprintf("0x%064x", v) }
				address := func(v string) string { return "0x" + strings.Repeat("0", 24) + v[2:] }
				var out string
				switch sel {
				case "0xb35056b8":
					v := uint64(0)
					if bad == "recovery" {
						v = 1
					}
					out = word(v)
				case "0x619a6f2b":
					v := uint64(0)
					if bad == "delegation" {
						v = 2
					}
					out = word(v)
				case "0x893d20e8":
					out = address(owner)
					if bad == "owner-width" {
						out = "0x1" + strings.Repeat("0", 63)
					}
				case "0xd2946c2b":
					out = address(collector)
					if bad == "collector" && call["to"] == model.MultiAsset.Vault {
						out = address(owner)
					}
				case "0x18160ddd":
					v := uint64(1000)
					if bad == "supply" {
						v = 0
					}
					out = word(v)
				case "0x77eef903":
					v := uint64(300000000000000000)
					if bad == "protocol-fee" {
						v = 500000000000000001
					}
					out = word(v)
				case "0x90605f3a":
					out = word(100000000000000000)
				case "0x70a08231":
					balanceReads++
					v := uint64(200)
					if strings.HasSuffix(call["data"], owner[2:]) {
						v = 700
					}
					if bad == "owner-balance" {
						v = 1100
					}
					out = word(v)
				default:
					t.Error("unknown BPT getter", sel)
					out = "0x"
				}
				if bad == "short-word" && sel == "0x18160ddd" {
					out = "0x01"
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
			}))
			defer server.Close()
			a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20)}
			pc := config.PoolConfig{ID: model.PoolID, MultiAsset: &config.MultiAssetConfig{ExpectedVault: model.MultiAsset.Vault}}
			block := domain.BlockRef{Number: 100, Hash: hash, TimestampMS: 100000}
			if bad == "time" {
				block.TimestampMS++
			}
			observed, e := a.readManagedFeeLedger(context.Background(), pc, map[string]any{"blockHash": hash, "requireCanonical": true}, block)
			want := bad == "" || bad == "same-recipient"
			if e == nil {
				model.MultiAsset.ManagedFees = observed.State
				e = model.Validate()
			}
			if want != (e == nil) {
				t.Fatal("bad ledger admission", bad, e)
			}
			if want {
				m := observed.State
				if m.TotalSupply != "1000" || m.OwnerBalance != "700" || m.BlockNumber != 100 || m.Timestamp != 100 || m.BlockHash != hash {
					t.Fatal("incomplete BPT ledger", m)
				}
				expected := 2
				if bad == "same-recipient" {
					expected = 1
					if m.OwnerBalance != m.ProtocolCollectorBalance {
						t.Fatal("same BPT recipient was not coalesced")
					}
				}
				if balanceReads != expected {
					t.Fatal("BPT balance dependency count", balanceReads, expected)
				}
			}
		})
	}
}

func TestManagedReferenceRuntimeCannotEnterNormalParser(t *testing.T) {
	// Even direct Adapter use cannot bypass the normal configuration rejection.
	a := Adapter{}
	_, e := a.readMultiAsset(context.Background(), config.PoolConfig{Protocol: "balancer-v2-managed-fees"}, nil, 100)
	if e == nil || e.Error() != "MANAGED_FEE_RUNTIME_UNAVAILABLE" {
		t.Fatal("reference-only runtime entered online parser", e)
	}
}
