package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"

	"google.golang.org/protobuf/proto"
)

func TestBinSettlementHookRuntimeAtCanonicalBlock(t *testing.T) {
	path := os.Getenv("XONE_INFINITY_HOOK_QUOTES")
	if path == "" {
		t.Skip("requires original Cancun Bin Manager and Vault replay")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var cases []struct {
		Manager, Vault, Hook, PoolID, Currency0, Currency1 string
		StateAfter                                         string
		AppReserve0After, AppReserve1After                 domain.Amount
		BinsAfter                                          []domain.InfinityBinEntry
		RPCBankAfter                                       *struct {
			Code  map[string]string
			Calls map[string]map[string]string
		}
	}
	if err = json.Unmarshal(raw, &cases); err != nil {
		t.Fatal(err)
	}
	block := domain.BlockRef{Hash: "0x" + strings.Repeat("ab", 32), Number: 100, TimestampMS: 1700000000000}
	checked := 0
	for _, item := range cases {
		bank := item.RPCBankAfter
		if bank == nil {
			continue
		}
		checked++
		digest := func(address string) string {
			if address == domain.SingletonZeroAddress {
				return ""
			}
			code, err := hex.DecodeString(strings.TrimPrefix(bank.Code[address], "0x"))
			if err != nil || len(code) == 0 {
				t.Fatal("missing replay code", address, err)
			}
			return fmt.Sprintf("%x", sha256.Sum256(code))
		}
		v := config.InfinityBinConfig{
			Manager: item.Manager, Vault: item.Vault, PoolID: item.PoolID, BinStep: 25,
			Hooks: item.Hook, HookModel: domain.InfinitySettlementHookModel,
			ExpectedManagerCodeSHA256: digest(item.Manager), ExpectedVaultCodeSHA256: digest(item.Vault),
			ExpectedToken0CodeSHA256: digest(item.Currency0), ExpectedToken1CodeSHA256: digest(item.Currency1),
			TokenPolicy: "standard", MaxBins: 8, MaxTreeWords: 8, MaxSteps: 8,
		}
		for _, variant := range []struct{ name, rejection string }{
			{"valid", ""},
			{"bitmap", "INFINITY_BIN_HOOK_BITMAP"},
			{"runtime", "SINGLETON_HOOK_CODE_IDENTITY"},
			{"unknown-model", "INFINITY_BIN_KEY_UNSUPPORTED"},
		} {
			t.Run(fmt.Sprintf("%d/%s", checked, variant.name), func(t *testing.T) {
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					var req struct {
						ID     json.RawMessage
						Method string
						Params []json.RawMessage
					}
					if err := json.NewDecoder(r.Body).Decode(&req); err != nil || len(req.Params) != 2 {
						t.Error("invalid replay request", err)
						w.WriteHeader(http.StatusBadRequest)
						return
					}
					var anchor map[string]any
					if err := json.Unmarshal(req.Params[1], &anchor); err != nil || anchor["blockHash"] != block.Hash || anchor["requireCanonical"] != true {
						t.Error("unanchored Bin dependency", err)
					}
					var result string
					switch req.Method {
					case "eth_getCode":
						var address string
						if err := json.Unmarshal(req.Params[0], &address); err != nil {
							t.Error(err)
						}
						result = bank.Code[address]
						if address == item.Hook && variant.name == "runtime" {
							result = "0x00"
						}
					case "eth_call":
						var call map[string]string
						if err := json.Unmarshal(req.Params[0], &call); err != nil {
							t.Error(err)
						}
						result = bank.Calls[call["to"]][call["data"]]
						if call["to"] == item.Hook && variant.name == "bitmap" {
							result = "0x" + strings.Repeat("0", 64)
						}
					}
					if result == "" {
						t.Errorf("unrecorded Bin dependency %s %s", req.Method, req.Params[0])
					}
					if err := json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": req.ID, "result": result}); err != nil {
						t.Error(err)
					}
				}))
				defer server.Close()
				poolConfig := v
				if variant.name == "unknown-model" {
					poolConfig.HookModel = "unknown"
				}
				p := config.PoolConfig{ID: item.Manager + "/" + item.PoolID, Protocol: "pancake-infinity-bin",
					Token0: item.Currency0, Token1: item.Currency1, FeeDenominator: 1000000, InfinityBin: &poolConfig}
				adapter := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337"}}
				pool, err := adapter.readInfinityBin(context.Background(), p, map[string]any{"blockHash": block.Hash, "requireCanonical": true}, block)
				if variant.rejection != "" {
					if err == nil || err.Error() != variant.rejection {
						t.Fatalf("got %v, want %s", err, variant.rejection)
					}
					return
				}
				if err != nil {
					t.Fatal(err)
				}
				s := pool.InfinityBin
				if fmt.Sprint(s.ActiveID) != item.StateAfter || !reflect.DeepEqual(s.Bins, item.BinsAfter) || s.AppReserve0 != item.AppReserve0After || s.AppReserve1 != item.AppReserve1After {
					t.Fatalf("collector disagrees with original poststate: %+v", s)
				}
				message, err := domain.InfinityBinToProto(s)
				if err != nil {
					t.Fatal(err)
				}
				decoded, err := domain.InfinityBinFromProto(message)
				if err != nil || !reflect.DeepEqual(decoded, s) {
					t.Fatalf("Go wire round trip disagrees: %v", err)
				}
				if dir := os.Getenv("XONE_INFINITY_BIN_WIRE_DIR"); dir != "" {
					bytes, err := proto.Marshal(message)
					if err != nil {
						t.Fatal(err)
					}
					if err = os.WriteFile(filepath.Join(dir, item.PoolID+".pb"), bytes, 0600); err != nil {
						t.Fatal(err)
					}
				}
			})
		}
	}
	if checked != 2 {
		t.Fatalf("expected ERC-20 and native Bin bank replays, got %d", checked)
	}
}
