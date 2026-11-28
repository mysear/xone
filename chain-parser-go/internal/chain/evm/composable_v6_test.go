package evm

import (
	"bytes"
	"compress/gzip"
	"context"
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

	"google.golang.org/protobuf/proto"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestComposableV6OriginalVaultSnapshotWire(t *testing.T) {
	file, err := os.Open("../../../test/contracts/composable-v6-wire-source.json.gz")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	zipped, err := gzip.NewReader(file)
	if err != nil {
		t.Fatal(err)
	}
	defer zipped.Close()
	var source struct {
		Codes     map[string]string
		Snapshots []struct {
			Header       map[string]any
			Calls        map[string]string
			Config       config.PoolConfig
			CollectorBPT domain.Amount `json:"collector_bpt"`
			Amounts      struct {
				Balances        []domain.Amount
				Scales          []domain.Amount
				TotalSupply     domain.Amount `json:"total_supply"`
				LastJoinExitAmp domain.Amount `json:"last_join_exit_amp"`
				LastInvariant   domain.Amount `json:"last_post_join_exit_invariant"`
				SwapFee         domain.Amount `json:"swap_fee"`
				YieldFee        domain.Amount `json:"yield_fee"`
				Rates           []*struct {
					Current domain.Amount `json:"current"`
					Old     domain.Amount `json:"old"`
				}
			}
		}
	}
	if err = json.NewDecoder(zipped).Decode(&source); err != nil {
		t.Fatal(err)
	}
	if len(source.Snapshots) != 14 {
		t.Fatal("expected ten BPT snapshots and four rate refresh snapshots")
	}
	for index, snapshot := range source.Snapshots {
		t.Run(fmt.Sprint(index), func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var req struct {
					ID     int
					Method string
					Params []json.RawMessage
				}
				if err := json.NewDecoder(r.Body).Decode(&req); err != nil || len(req.Params) != 2 {
					t.Error("invalid RPC request", err)
					w.WriteHeader(http.StatusBadRequest)
					return
				}
				var result any
				if req.Method == "eth_getBlockByNumber" {
					var tag string
					if err := json.Unmarshal(req.Params[0], &tag); err != nil || tag != snapshot.Header["number"] {
						t.Errorf("unexpected block tag %s: %v", tag, err)
					}
					result = snapshot.Header
				} else {
					var anchor map[string]any
					if err := json.Unmarshal(req.Params[1], &anchor); err != nil || anchor["blockHash"] != snapshot.Header["hash"] || anchor["requireCanonical"] != true {
						t.Errorf("unbound state call %v: %v", anchor, err)
					}
					switch req.Method {
					case "eth_getCode":
						var address string
						if err := json.Unmarshal(req.Params[0], &address); err != nil {
							t.Error(err)
						}
						result = source.Codes[address]
					case "eth_call":
						var call struct{ To, Data string }
						if err := json.Unmarshal(req.Params[0], &call); err != nil {
							t.Error(err)
						}
						result = snapshot.Calls[call.To+call.Data]
					}
					if result == nil || result == "" {
						t.Errorf("unrecorded RPC request %s %s", req.Method, req.Params[0])
					}
				}
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": req.ID, "result": result})
			}))
			defer server.Close()
			adapter := Adapter{
				Client: rpc.New([]string{server.URL}, time.Second, 1<<20),
				Chain:  config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{snapshot.Config}},
			}
			block, pools, err := adapter.ReadState(context.Background(), snapshot.Header["number"].(string), false)
			if err != nil || len(pools) != 1 {
				t.Fatalf("fixed-block parse: %v", err)
			}
			state := pools[0].MultiAsset
			v6 := state.ComposableV6
			if v6 == nil || v6.TotalSupply != snapshot.Amounts.TotalSupply || v6.LastJoinExitAmp != snapshot.Amounts.LastJoinExitAmp ||
				v6.LastJoinExitInvariant != snapshot.Amounts.LastInvariant || v6.ProtocolSwapFee != snapshot.Amounts.SwapFee ||
				v6.ProtocolYieldFee != snapshot.Amounts.YieldFee || v6.CollectorBPT != snapshot.CollectorBPT {
				t.Fatalf("unexpected fee state: %+v", v6)
			}
			for i, asset := range state.Assets {
				if asset.Balance != snapshot.Amounts.Balances[i] || asset.Scaling != snapshot.Amounts.Scales[i] {
					t.Fatalf("unexpected registered asset %d: %+v", i, asset)
				}
			}
			for _, cache := range v6.RateCaches {
				observed := snapshot.Amounts.Rates[cache.TokenIndex]
				if observed == nil || cache.Current != observed.Current || cache.Old != observed.Old {
					t.Fatalf("unexpected rate cache: %+v", cache)
				}
				wantRefresh := index == 10 || index == 12
				if (cache.ObservedRefreshRate != "") != wantRefresh {
					t.Fatalf("unexpected refresh observation: %+v", cache)
				}
			}
			if (index >= 5 && len(v6.RateCaches) != 2) || (index < 5 && len(v6.RateCaches) != 0) {
				t.Fatalf("unexpected rate cache count: %d", len(v6.RateCaches))
			}
			baseline, err := domain.NewCurrentStateBaseline(block, pools, "chain-parser-go", 1, block.Number)
			if err != nil {
				t.Fatal(err)
			}
			baseline.Source = "LOCAL_COMPOSABLE_V6_VAULT_REPLAY"
			if index < 10 {
				baseline.Reference.JournalSeq = uint64(index%5) + 1
			} else {
				baseline.Reference.JournalSeq = uint64(index%2) + 1
			}
			baseline.Reference.WriterEpoch = 1
			baseline.Reference.OnlineGeneration = 1
			wire, err := domain.SnapshotToProto(baseline)
			if err != nil {
				t.Fatal(err)
			}
			encoded, err := (proto.MarshalOptions{Deterministic: true}).Marshal(wire)
			if err != nil {
				t.Fatal(err)
			}
			name := fmt.Sprintf("composable_v6_snapshot_%d.pb", index)
			if dest := os.Getenv("XONE_COMPOSABLE_V6_WIRE_OUTPUT"); dest != "" {
				if err := os.WriteFile(filepath.Join(dest, name), encoded, 0o600); err != nil {
					t.Fatal(err)
				}
			} else if expected, err := os.ReadFile(filepath.Join("../../../test/fixtures/router-wire", name)); err != nil || !bytes.Equal(encoded, expected) {
				t.Fatalf("Go snapshot differs from the Rust fixture: %v", err)
			}
			decoded, err := domain.MultiAssetFromProto(wire.Pools[0].MultiAsset)
			if err != nil || !reflect.DeepEqual(state, decoded) {
				t.Fatalf("Go round trip: %v", err)
			}
			wire.Pools[0].MultiAsset.ComposableV6.YieldExempt = nil
			if _, err := domain.MultiAssetFromProto(wire.Pools[0].MultiAsset); err == nil {
				t.Fatal("accepted missing yield exemption observation")
			}
			if index >= 5 {
				invalid := pools[0].Clone()
				stale := &invalid.MultiAsset.ComposableV6.RateCaches[0]
				stale.Expires = 1
				stale.ObservedRefreshRate = ""
				if invalid.Validate() == nil || pools[0].Validate() != nil {
					t.Fatal("expired cache admission or aliased clone")
				}
				baseline.Reference.Block.TimestampMS += 1000
				if err := baseline.Validate("chain-parser-go", 1); err == nil || err.Error() != "COMPOSABLE_V6_RATE_SNAPSHOT_CLOCK" {
					t.Fatalf("unbound cache snapshot: %v", err)
				}
				token := &snapshot.Config.MultiAsset.Tokens[v6.RateCaches[0].TokenIndex]
				if index == 10 || index == 12 {
					token.RateProviderContextIndependent = false
					if _, _, err := adapter.ReadState(context.Background(), snapshot.Header["number"].(string), false); err == nil ||
						!strings.Contains(err.Error(), "COMPOSABLE_V6_RATE_CACHE_EXPIRED") {
						t.Fatalf("expired provider without context-independent approval: %v", err)
					}
					token.RateProviderContextIndependent = true
				}
				provider := token.RateProvider
				token.RateProvider = snapshot.Config.ID
				if _, _, err := adapter.ReadState(context.Background(), snapshot.Header["number"].(string), false); err == nil {
					t.Fatal("accepted a mismatched provider address")
				}
				token.RateProvider = provider
				token.RateProviderCodeSHA256 = strings.Repeat("f", 64)
				if _, _, err := adapter.ReadState(context.Background(), snapshot.Header["number"].(string), false); err == nil {
					t.Fatal("accepted a mismatched provider runtime digest")
				}
			}
		})
	}
}
