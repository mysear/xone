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
	"testing"
	"time"

	"google.golang.org/protobuf/proto"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestCurveMetaUnderlyingOriginalWire(t *testing.T) {
	file, err := os.Open("../../../test/contracts/curve-meta-underlying-source.json.gz")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	compressed, err := gzip.NewReader(file)
	if err != nil {
		t.Fatal(err)
	}
	defer compressed.Close()
	var source struct {
		Config    config.PoolConfig
		Codes     map[string]string
		Swaps     json.RawMessage
		Snapshots []struct {
			Header   map[string]any
			Calls    map[string]string
			Storage  map[string]string
			Observed struct {
				MetaBalances []domain.Amount `json:"meta_balances"`
				MetaAdmin    []domain.Amount `json:"meta_admin"`
				Stored       []domain.Amount
				BaseBalances []domain.Amount `json:"base_balances"`
				BaseAdmin    []domain.Amount `json:"base_admin"`
				Supply       domain.Amount
				VirtualPrice domain.Amount `json:"virtual_price"`
			}
		}
	}
	if err := json.NewDecoder(compressed).Decode(&source); err != nil {
		t.Fatal(err)
	}
	if len(source.Snapshots) != 4 {
		t.Fatalf("got %d snapshots, want four", len(source.Snapshots))
	}
	dir := "../../../test/fixtures/router-wire"
	write := os.Getenv("XONE_CURVE_META_WIRE_OUTPUT")
	checkFile := func(t *testing.T, name string, value []byte) {
		t.Helper()
		if write != "" {
			if err := os.WriteFile(filepath.Join(write, name), value, 0o600); err != nil {
				t.Fatal(err)
			}
			return
		}
		frozen, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil || !bytes.Equal(frozen, value) {
			t.Fatalf("fixture %s differs: %v", name, err)
		}
	}
	checkFile(t, "curve_meta_underlying_swaps.json", append(source.Swaps, '\n'))
	for i, raw := range source.Snapshots {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var request struct {
					ID     int
					Method string
					Params []json.RawMessage
				}
				err := json.NewDecoder(r.Body).Decode(&request)
				if err != nil || len(request.Params) < 2 {
					t.Errorf("invalid request: %v", err)
					w.WriteHeader(http.StatusBadRequest)
					return
				}
				params := request.Params
				var result any
				if request.Method == "eth_getBlockByNumber" {
					if string(params[0]) != fmt.Sprintf("%q", raw.Header["number"]) {
						t.Errorf("unexpected block tag: %s", params[0])
					}
					result = raw.Header
				} else {
					var anchor map[string]any
					if err := json.Unmarshal(params[len(params)-1], &anchor); err != nil ||
						anchor["blockHash"] != raw.Header["hash"] || anchor["requireCanonical"] != true {
						t.Errorf("unbound state call: %v, %v", anchor, err)
					}
					switch request.Method {
					case "eth_getCode", "eth_getStorageAt":
						var address string
						_ = json.Unmarshal(params[0], &address)
						if request.Method == "eth_getCode" {
							result = source.Codes[address]
						} else if len(params) == 3 {
							var slot string
							_ = json.Unmarshal(params[1], &slot)
							result = raw.Storage[address+slot]
						}
					case "eth_call":
						var call struct{ To, Data string }
						_ = json.Unmarshal(params[0], &call)
						result = raw.Calls[call.To+call.Data]
					}
					if result == nil || result == "" {
						t.Errorf("unrecorded RPC request %s %s", request.Method, params[0])
					}
				}
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": request.ID, "result": result})
			}))
			defer server.Close()
			a := Adapter{
				Client: rpc.New([]string{server.URL}, time.Second, 1<<20),
				Chain:  config.Chain{Key: "evm:1", Pools: []config.PoolConfig{source.Config}},
			}
			block, pools, err := a.ReadState(context.Background(), raw.Header["number"].(string), false)
			if err != nil || len(pools) != 1 {
				t.Fatalf("fixed-block parse: %v", err)
			}
			pool := pools[0]
			m := pool.MultiAsset.CurveMeta
			base := m.BasePoolState
			want := raw.Observed
			if base == nil || !reflect.DeepEqual(base.Balances, want.BaseBalances) ||
				!reflect.DeepEqual(base.AdminBalances, want.BaseAdmin) || base.TotalSupply != want.Supply ||
				m.BaseVirtualPrice != want.VirtualPrice || !reflect.DeepEqual(m.StoredBalances, want.Stored) ||
				!reflect.DeepEqual(m.AdminBalances, want.MetaAdmin) || pool.Reserve0 != want.MetaBalances[0] ||
				pool.Reserve1 != want.MetaBalances[1] {
				t.Fatalf("unexpected parsed curve state: %+v", pool)
			}
			baseline, err := domain.NewCurrentStateBaseline(block, pools, "chain-parser-go", 1, block.Number)
			if err != nil {
				t.Fatal(err)
			}
			baseline.Reference.JournalSeq = uint64(i) + 1
			baseline.Reference.WriterEpoch = 1
			baseline.Reference.OnlineGeneration = 1
			if err := baseline.Validate("chain-parser-go", 1); err != nil {
				t.Fatal(err)
			}
			wire, err := domain.SnapshotToProto(baseline)
			if err != nil {
				t.Fatal(err)
			}
			encoded, err := (proto.MarshalOptions{Deterministic: true}).Marshal(wire)
			if err != nil {
				t.Fatal(err)
			}
			checkFile(t, fmt.Sprintf("curve_meta_underlying_%d.pb", i), encoded)
		})
	}
}
