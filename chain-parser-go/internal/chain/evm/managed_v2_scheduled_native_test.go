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
	"reflect"
	"sort"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

type scheduledOriginalState struct {
	ID       string
	Header   map[string]any
	Getters  []managedBankGetter
	Accounts []struct {
		Address    string
		CodeSHA256 string `json:"code_sha256"`
	}
	Storage []struct{ Address, Key, Value string }
}
type scheduledOriginalBranch struct {
	States        []scheduledOriginalState
	NativeCode    map[string]string `json:"native_code"`
	NativeRuntime map[string]string `json:"native_runtime"`
}
type scheduledOriginalBank struct {
	Branches map[string]scheduledOriginalBranch
}
type scheduledThin struct {
	OriginalRaw struct {
		SHA256 string
		Bytes  int
	} `json:"original_raw"`
	States  map[string]domain.Pool
	Configs map[string]config.PoolConfig
}

func scheduledOriginals(t *testing.T) (scheduledOriginalBank, scheduledThin) {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/managed-v2-scheduled.json")
	if e != nil {
		t.Fatal(e)
	}
	data, e := os.ReadFile("../../../test/contracts/managed-v2-scheduled-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var b scheduledOriginalBank
	var thin scheduledThin
	if e = json.Unmarshal(raw, &b); e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(data, &thin); e != nil {
		t.Fatal(e)
	}
	digest := sha256.Sum256(raw)
	if thin.OriginalRaw.SHA256 != hex.EncodeToString(digest[:]) || thin.OriginalRaw.Bytes != len(raw) {
		t.Fatal("projection detached from original bytes")
	}
	return b, thin
}
func scheduledOriginalAdapter(t *testing.T, branch scheduledOriginalBranch, state scheduledOriginalState, pool config.PoolConfig) Adapter {
	t.Helper()
	codes := branch.NativeRuntime
	if codes == nil {
		codes = branch.NativeCode
	}
	calls := map[string]string{}
	for _, g := range state.Getters {
		calls[g.To+g.Data] = g.Result
	}
	storage := map[string]string{}
	for _, row := range state.Storage {
		// The in-process original VM omits leading zero bytes. JSON-RPC returns
		// 32-byte DATA; padding here preserves the exact observed storage integer.
		word := strings.TrimPrefix(row.Value, "0x")
		if len(word) > 64 {
			t.Fatal("original storage width")
		}
		storage[row.Address+row.Key] = "0x" + strings.Repeat("0", 64-len(word)) + word
	}
	sourceCodes := map[string]string{}
	for _, a := range state.Accounts {
		code, ok := codes[a.CodeSHA256]
		if !ok {
			t.Fatal("missing original runtime", a.Address)
		}
		sourceCodes[a.Address] = code
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     int
			Method string
			Params []json.RawMessage
		}
		if e := json.NewDecoder(r.Body).Decode(&q); e != nil {
			t.Error(e)
			return
		}
		var result any
		found := true
		if q.Method == "eth_getBlockByNumber" {
			result = state.Header
		} else {
			anchorIndex := 1
			if q.Method == "eth_getStorageAt" {
				anchorIndex = 2
			}
			if len(q.Params) <= anchorIndex {
				t.Error("RPC arity", q.Method)
				return
			}
			var anchor map[string]any
			if e := json.Unmarshal(q.Params[anchorIndex], &anchor); e != nil {
				t.Error(e)
				return
			}
			if len(anchor) != 2 || anchor["blockHash"] != state.Header["hash"] || anchor["requireCanonical"] != true {
				t.Error("original source H lost", anchor)
			}
			var address string
			switch q.Method {
			case "eth_call":
				var call map[string]string
				_ = json.Unmarshal(q.Params[0], &call)
				result, found = calls[call["to"]+call["data"]]
			case "eth_getCode":
				_ = json.Unmarshal(q.Params[0], &address)
				result, found = sourceCodes[address]
			case "eth_getStorageAt":
				var slot string
				_ = json.Unmarshal(q.Params[0], &address)
				_ = json.Unmarshal(q.Params[1], &slot)
				result, found = storage[address+slot]
			default:
				t.Error("unexpected RPC", q.Method)
				found = false
			}
		}
		if !found {
			_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32000, "message": "unobserved source"}})
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	return Adapter{Client: rpc.New([]string{server.URL}, 10*time.Second, 4<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{pool}}}
}
func TestManagedScheduledEveryOriginalSnapshot(t *testing.T) {
	raw, thin := scheduledOriginals(t)
	names := make([]string, 0, len(raw.Branches))
	for name := range raw.Branches {
		names = append(names, name)
	}
	sort.Strings(names)
	count := 0
	for _, name := range names {
		branch := raw.Branches[name]
		for _, state := range branch.States {
			key := name + "/" + state.ID
			t.Run(key, func(t *testing.T) {
				expected, ok := thin.States[key]
				if !ok {
					t.Fatal("missing original projection", key)
				}
				a := scheduledOriginalAdapter(t, branch, state, thin.Configs[key])
				ref, pools, e := a.ReadState(context.Background(), state.Header["number"].(string), false)
				if e != nil {
					t.Fatal(e)
				}
				if len(pools) != 1 {
					t.Fatal("source pool cardinality")
				}
				if e = pools[0].Validate(); e != nil {
					t.Fatal(e)
				}
				if !reflect.DeepEqual(pools[0], expected) {
					got, _ := json.MarshalIndent(pools[0], "", "  ")
					want, _ := json.MarshalIndent(expected, "", "  ")
					t.Fatalf("original %s full state differs\ngot=%s\nwant=%s", key, got, want)
				}
				if ref.Hash != state.Header["hash"] || ref.TimestampMS != expected.MultiAsset.ManagedV2.Timestamp*1000 {
					t.Fatal("original header binding")
				}
			})
			count++
		}
	}
	if count != 234 || len(thin.States) != 234 {
		t.Fatalf("incomplete original snapshot inventory: %d", count)
	}
}

func TestManagedScheduledOriginalFaultsNeverPublish(t *testing.T) {
	raw, thin := scheduledOriginals(t)
	branch := raw.Branches["reschedule"]
	var source scheduledOriginalState
	for _, s := range branch.States {
		expected := thin.States["reschedule/"+s.ID]
		m := expected.MultiAsset.ManagedV2
		if m.GradualWeightEnd > m.Timestamp {
			source = s
			break
		}
	}
	if source.ID == "" {
		t.Fatal("active original schedule missing")
	}
	pool := thin.Configs["reschedule/"+source.ID]
	type fault func(*scheduledOriginalState, *config.PoolConfig)
	dropGetter := func(method string) fault {
		return func(s *scheduledOriginalState, p *config.PoolConfig) {
			found := false
			kept := s.Getters[:0]
			for _, g := range s.Getters {
				if g.To == p.ID && g.Method == method {
					found = true
					continue
				}
				kept = append(kept, g)
			}
			s.Getters = kept
			if !found {
				t.Fatal("fault did not remove original getter", method)
			}
		}
	}
	faults := map[string]fault{
		"missing-identity":      dropGetter("getVault"),
		"missing-join-flag":     dropGetter("getJoinExitEnabled"),
		"missing-actual-supply": dropGetter("getActualSupply"),
		"missing-pool-word": func(s *scheduledOriginalState, p *config.PoolConfig) {
			kept := s.Storage[:0]
			for _, r := range s.Storage {
				if r.Address != p.ID || r.Key != "0x9" {
					kept = append(kept, r)
				}
			}
			s.Storage = kept
		},
		"missing-token-word": func(s *scheduledOriginalState, p *config.PoolConfig) {
			key := managedV2ScheduledTokenSlot(p.Token0)
			kept := s.Storage[:0]
			for _, r := range s.Storage {
				if r.Address != p.ID || r.Key != key {
					kept = append(kept, r)
				}
			}
			s.Storage = kept
		},
		"getter-fee-poison": func(s *scheduledOriginalState, p *config.PoolConfig) {
			for i := range s.Getters {
				g := &s.Getters[i]
				if g.To == p.ID && g.Method == "getSwapFeePercentage" {
					value := new(big.Int)
					value.SetString(g.Result[2:], 16)
					value.Add(value, big.NewInt(1))
					g.Result = fmt.Sprintf("0x%064x", value)
				}
			}
		},
		"token-reserved-bit": func(s *scheduledOriginalState, p *config.PoolConfig) {
			key := managedV2ScheduledTokenSlot(p.Token0)
			for i := range s.Storage {
				r := &s.Storage[i]
				if r.Address == p.ID && r.Key == key {
					value := new(big.Int)
					value.SetString(r.Value[2:], 16)
					value.SetBit(value, 133, 1)
					r.Value = fmt.Sprintf("0x%064x", value)
				}
			}
		},
		"legacy-active-promotion": func(s *scheduledOriginalState, p *config.PoolConfig) { p.Protocol = domain.ManagedV2Protocol },
	}
	for name, mutate := range faults {
		t.Run(name, func(t *testing.T) {
			bytes, e := json.Marshal(source)
			if e != nil {
				t.Fatal(e)
			}
			var state scheduledOriginalState
			if e = json.Unmarshal(bytes, &state); e != nil {
				t.Fatal(e)
			}
			bytes, e = json.Marshal(pool)
			if e != nil {
				t.Fatal(e)
			}
			var configured config.PoolConfig
			if e = json.Unmarshal(bytes, &configured); e != nil {
				t.Fatal(e)
			}
			mutate(&state, &configured)
			a := scheduledOriginalAdapter(t, branch, state, configured)
			_, pools, e := a.ReadState(context.Background(), state.Header["number"].(string), false)
			if e == nil || pools != nil {
				t.Fatal("faulted original source published a pool", e)
			}
		})
	}
}
