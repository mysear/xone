package evm

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"os"
	"sort"
	"strings"
	"sync"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

type managedBankObject struct {
	Kind  string
	Value json.RawMessage
}
type managedBankRef struct{ Ref string }
type managedBank struct {
	Objects   map[string]managedBankObject
	Cases     []managedV2OriginalCase
	Scenarios []struct {
		Name          string
		Source        managedBankRef `json:"source_h"`
		Pool          string
		Underlying    []string
		Configuration struct {
			Params struct{ Name string }
			Owner  string
		}
	}
	Reference struct {
		Identities     []managedBankRef
		FactoryStorage struct {
			Getters   []managedBankRef
			Contracts []struct {
				Address string
				Code    managedBankRef
			}
		} `json:"factory_code_storage"`
	}
}
type managedBankGetter struct {
	To, Method, Data, Result string
	Args                     []json.RawMessage
}
type managedBankIdentity struct {
	Label, Address string
	Code           managedBankRef
}
type managedBankSnapshot struct {
	Header managedBankRef
	State  struct {
		Pool       string
		PoolID     string `json:"pool_id"`
		Roles      map[string]string
		Annual     string `json:"annual_aum_fee"`
		ProviderID string `json:"provider_fee_id"`
	}
	Getters, Accounts []managedBankRef
}

func (b managedBank) decode(t *testing.T, r managedBankRef, kind string, out any) {
	t.Helper()
	v, ok := b.Objects[r.Ref]
	if !ok || v.Kind != kind {
		t.Fatalf("missing %s %s", kind, r.Ref)
	}
	if e := json.Unmarshal(v.Value, out); e != nil {
		t.Fatal(e)
	}
}
func managedDigest(hexCode string) string {
	b, _ := hex.DecodeString(strings.TrimPrefix(hexCode, "0x"))
	h := sha256.Sum256(b)
	return hex.EncodeToString(h[:])
}

var managedBankCache struct {
	sync.Once
	bank managedBank
	err  error
}

func managedCanonical(raw []byte) ([]byte, error) {
	var value any
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	if e := dec.Decode(&value); e != nil {
		return nil, e
	}
	// encoding/json sorts map keys and UseNumber preserves the literal decimal
	// integers. Disable HTML escaping to match the bank's canonical JSON API.
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if e := enc.Encode(value); e != nil {
		return nil, e
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil
}
func managedLoadBank(t *testing.T) managedBank {
	t.Helper()
	managedBankCache.Do(func() {
		raw, e := os.ReadFile("../../../test/contracts/managed-v2-aum.json")
		if e != nil {
			managedBankCache.err = e
			return
		}
		h := sha256.Sum256(raw)
		if len(raw) != 7905624 || hex.EncodeToString(h[:]) != "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66" {
			managedBankCache.err = fmt.Errorf("frozen Managed V2 bank digest/size changed")
			return
		}
		var objects struct{ Objects map[string]json.RawMessage }
		if e = json.Unmarshal(raw, &objects); e != nil {
			managedBankCache.err = e
			return
		}
		for id, obj := range objects.Objects {
			canonical, e := managedCanonical(obj)
			if e != nil {
				managedBankCache.err = e
				return
			}
			h := sha256.Sum256(canonical)
			if hex.EncodeToString(h[:]) != id {
				managedBankCache.err = fmt.Errorf("content address mismatch %s", id)
				return
			}
		}
		managedBankCache.err = json.Unmarshal(raw, &managedBankCache.bank)
	})
	if managedBankCache.err != nil {
		t.Fatal(managedBankCache.err)
	}
	return managedBankCache.bank
}

type managedTestState struct {
	Reorg        bool
	RPCFault     string
	Codes, Calls map[string]string
	Header       map[string]any
	Pool         config.PoolConfig
}

func managedNativeFixture(t *testing.T, index int, mutate func(*managedTestState)) (Adapter, *managedTestState) {
	return managedNativeFixtureAt(t, index, managedBankRef{}, mutate)
}
func managedNativeFixtureAt(t *testing.T, index int, source managedBankRef, mutate func(*managedTestState)) (Adapter, *managedTestState) {
	t.Helper()
	b := managedLoadBank(t)
	scenario := b.Scenarios[index]
	var snap managedBankSnapshot
	if source.Ref == "" {
		source = scenario.Source
	}
	b.decode(t, source, "snapshot", &snap)
	codes := map[string]string{}
	for _, r := range snap.Accounts {
		var a struct {
			Address string
			Code    managedBankRef
		}
		b.decode(t, r, "account", &a)
		var c struct{ Hex string }
		b.decode(t, a.Code, "code", &c)
		codes[a.Address] = c.Hex
	}
	ids := map[string]string{}
	for _, r := range b.Reference.Identities {
		var i managedBankIdentity
		b.decode(t, r, "identity", &i)
		ids[i.Label] = i.Address
		var code struct{ Hex string }
		b.decode(t, i.Code, "code", &code)
		if _, ok := codes[i.Address]; !ok {
			codes[i.Address] = code.Hex
		}
	}
	calls := map[string]string{}
	for _, r := range snap.Getters {
		var g managedBankGetter
		b.decode(t, r, "getter", &g)
		calls[g.To+g.Data] = g.Result
	}
	m := &config.ManagedV2PoolConfig{ExpectedFactory: ids["ManagedPoolFactory"], ExpectedFactoryCodeSHA256: managedDigest(codes[ids["ManagedPoolFactory"]]), ExpectedPoolRuntimeTemplateSHA256: managedV2Templates["ManagedPool"].SHA256, ExpectedFeeProvider: ids["ProtocolFeePercentagesProvider"], ExpectedFeeProviderCodeSHA256: managedDigest(codes[ids["ProtocolFeePercentagesProvider"]]), ExpectedProtocolCollector: ids["ProtocolFeesCollector"], ExpectedProtocolCollectorCodeSHA256: managedDigest(codes[ids["ProtocolFeesCollector"]]), ExpectedManager: scenario.Configuration.Owner, ExpectedProviderFeeID: domain.Amount(snap.State.ProviderID), ExpectedManagementAUMFee: domain.Amount(snap.State.Annual)}
	for role, name := range managedV2Roles {
		addr := ids[name]
		m.Dependencies = append(m.Dependencies, domain.ManagedV2Dependency{Role: role, Address: addr, CodeSHA256: managedDigest(codes[addr]), TemplateSHA256: managedV2Templates[name].SHA256})
	}
	v := &config.MultiAssetConfig{ExpectedCodeSHA256: managedDigest(codes[scenario.Pool]), ExpectedVault: ids["Vault"], ExpectedVaultCodeSHA256: managedDigest(codes[ids["Vault"]]), PoolID: snap.State.PoolID, TokenPolicy: "standard-no-bpt", ManagedV2: m}
	for _, token := range scenario.Underlying {
		v.Tokens = append(v.Tokens, config.MultiTokenConfig{Token: token, ExpectedCodeSHA256: managedDigest(codes[token])})
	}
	p := config.PoolConfig{ID: scenario.Pool, Protocol: domain.ManagedV2Protocol, Token0: scenario.Underlying[0], Token1: scenario.Underlying[1], MultiAsset: v}
	var header map[string]any
	b.decode(t, snap.Header, "header", &header)
	// Use only getter responses recorded at this snapshot's exact header.
	// Factory storage below supplies code bytes, never replacement call results.
	for _, part := range b.Reference.FactoryStorage.Contracts {
		var c struct{ Hex string }
		b.decode(t, part.Code, "code", &c)
		codes[part.Address] = c.Hex
	}
	state := &managedTestState{Codes: codes, Calls: calls, Header: header, Pool: p}
	if mutate != nil {
		mutate(state)
	}
	p = state.Pool
	headerReads := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     int
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&q)
		var result any
		switch q.Method {
		case "eth_getBlockByNumber":
			headerReads++
			result = header
			if state.Reorg && headerReads > 1 {
				other := map[string]any{}
				for k, v := range header {
					other[k] = v
				}
				other["hash"] = "0x" + strings.Repeat("b", 64)
				result = other
			}
		case "eth_getCode", "eth_call":
			var anchor map[string]any
			json.Unmarshal(q.Params[1], &anchor)
			if anchor["blockHash"] != header["hash"] || anchor["requireCanonical"] != true || len(anchor) != 2 {
				t.Error("unpinned read", anchor)
			}
			if q.Method == "eth_getCode" {
				var addr string
				json.Unmarshal(q.Params[0], &addr)
				if state.RPCFault == "eth_getCode"+addr {
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32000, "message": "injected dependency failure"}})
					return
				}
				result = codes[addr]
			} else {
				var call map[string]string
				json.Unmarshal(q.Params[0], &call)
				template := ""
				switch call["to"] {
				case p.ID:
					template = "ManagedPool"
				case v.ExpectedVault:
					template = "Vault"
				case m.ExpectedFactory:
					template = "ManagedPoolFactory"
				case m.ExpectedFeeProvider:
					template = "ProtocolFeePercentagesProvider"
				case m.ExpectedProtocolCollector:
					template = "ProtocolFeesCollector"
				}
				for _, d := range m.Dependencies {
					if d.Address == call["to"] {
						template = managedV2Roles[d.Role]
					}
				}
				if template != "" {
					valid := false
					for _, sel := range managedV2Templates[template].Methods {
						if strings.HasPrefix(call["data"], "0x"+sel) {
							valid = true
						}
					}
					if !valid {
						t.Errorf("getter absent from published %s ABI: %s", template, call["data"])
					}
				}

				if state.RPCFault == "eth_call"+call["to"]+call["data"] {
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32000, "message": "injected dependency failure"}})
					return
				}
				result = calls[call["to"]+call["data"]]
				if result == "" {
					t.Errorf("unrecorded getter %s %s", call["to"], call["data"])
				}
			}
		default:
			t.Error("unexpected RPC", q.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	a := Adapter{Client: rpc.New([]string{server.URL}, 10*time.Second, 4<<20), Chain: config.Chain{Key: "evm:31337", Pools: []config.PoolConfig{p}}}
	return a, state
}

func TestManagedV2NativeDispatch(t *testing.T) {
	a, _ := managedNativeFixture(t, 0, nil)
	_, pools, e := a.ReadState(context.Background(), "latest", false)
	if e != nil {
		t.Fatalf("native Managed V2 dispatch: %v", e)
	}
	if len(pools) != 1 || pools[0].MultiAsset == nil || pools[0].MultiAsset.ManagedV2 == nil {
		t.Fatal("managed state missing")
	}
}

func TestManagedV2NativeMatrix(t *testing.T) {
	b := managedLoadBank(t)
	for index, scenario := range b.Scenarios {
		t.Run(scenario.Name, func(t *testing.T) {
			a, state := managedNativeFixture(t, index, nil)
			ref, pools, e := a.ReadState(context.Background(), state.Header["number"].(string), false)
			if e != nil {
				t.Fatal(e)
			}
			m := pools[0].MultiAsset.ManagedV2
			if m.Timestamp*1000 != ref.TimestampMS || m.BlockNumber != ref.Number || m.BlockHash != ref.Hash {
				t.Fatal("source header lost")
			}
			if len(pools[0].MultiAsset.Assets) != len(state.Pool.MultiAsset.Tokens) || len(m.TokenInfo) != len(state.Pool.MultiAsset.Tokens) {
				t.Fatal("underlying vector incomplete")
			}
			for _, asset := range pools[0].MultiAsset.Assets {
				if asset.Token == pools[0].PoolID {
					t.Fatal("BPT entered root asset graph")
				}
			}
		})
	}
}

func TestManagedV2RejectsDependencyAndABI(t *testing.T) {
	word := func(n int64) string { return fmt.Sprintf("%064x", n) }
	edit := func(s *managedTestState, to, sig, args string, index int, value string) {
		key := to + managedV2Selector(sig) + args
		raw := s.Calls[key]
		if len(raw) < 2+32*2*(index+1) {
			t.Fatalf("edit missing ABI %s", sig)
		}
		s.Calls[key] = raw[:2+64*index] + value + raw[2+64*(index+1):]
	}
	tests := map[string]func(*managedTestState){
		"factory-membership": func(s *managedTestState) {
			s.Calls[s.Pool.MultiAsset.ManagedV2.ExpectedFactory+managedV2Selector("isPoolFromFactory(address)")+hex.EncodeToString(managedV2AddressWord(s.Pool.ID))] = "0x" + word(0)
		},
		"provider-id-not-config": func(s *managedTestState) { edit(s, s.Pool.ID, "getProviderFeeId(uint256)", word(3), 0, word(2)) },
		"annual-fee-not-config":  func(s *managedTestState) { edit(s, s.Pool.ID, "getManagementAumFeeParams()", "", 0, word(1)) },
		"last-collection-wide": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getManagementAumFeeParams()", "", 1, fmt.Sprintf("%064x", new(big.Int).Lsh(big.NewInt(1), 32)))
		},
		"bool-wide": func(s *managedTestState) { edit(s, s.Pool.ID, "getSwapEnabled()", "", 0, word(2)) },
		"trailing-getter": func(s *managedTestState) {
			key := s.Pool.ID + managedV2Selector("getActualSupply()")
			s.Calls[key] += word(0)
		},
		"missing-getter": func(s *managedTestState) { s.Calls[s.Pool.ID+managedV2Selector("getActualSupply()")] = "0x" },
		"weights-offset": func(s *managedTestState) { edit(s, s.Pool.ID, "getNormalizedWeights()", "", 0, word(64)) },
		"tokens-offset": func(s *managedTestState) {
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokens(bytes32)", s.Pool.MultiAsset.PoolID[2:], 1, word(96))
		},
		"fee-schedule-mismatch": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getGradualSwapFeeUpdateParams()", "", 3, word(20000000000000000))
		},
		"weight-schedule-mismatch": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getGradualWeightUpdateParams()", "", 8, word(750000000000000000))
			edit(s, s.Pool.ID, "getGradualWeightUpdateParams()", "", 9, word(250000000000000000))
		},
		"active-weight": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getGradualWeightUpdateParams()", "", 1, word(4000000000))
		},
		"active-fee": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getGradualSwapFeeUpdateParams()", "", 1, word(4000000000))
		},
		"active-breaker": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getCircuitBreakerState(address)", hex.EncodeToString(managedV2AddressWord(s.Pool.Token0)), 0, word(1))
		},
		"token-decimals": func(s *managedTestState) { edit(s, s.Pool.Token0, "decimals()", "", 0, word(19)) },
		"cash-balance-mismatch": func(s *managedTestState) {
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", s.Pool.MultiAsset.PoolID[2:]+hex.EncodeToString(managedV2AddressWord(s.Pool.Token0)), 0, word(1))
		},
		"future-token-block": func(s *managedTestState) {
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", s.Pool.MultiAsset.PoolID[2:]+hex.EncodeToString(managedV2AddressWord(s.Pool.Token0)), 2, word(1000000))
		},
		"cache-width": func(s *managedTestState) {
			edit(s, s.Pool.ID, "getProtocolFeePercentageCache(uint256)", word(3), 0, fmt.Sprintf("%064x", new(big.Int).Lsh(big.NewInt(1), 64)))
		},
		"zero-provider-cap": func(s *managedTestState) {
			m := s.Pool.MultiAsset.ManagedV2
			arg := fmt.Sprintf("%064x", m.ExpectedProviderFeeID.Big())
			edit(s, m.ExpectedFeeProvider, "getFeeTypePercentage(uint256)", arg, 0, word(0))
			edit(s, m.ExpectedFeeProvider, "getFeeTypeMaximumPercentage(uint256)", arg, 0, word(0))
			edit(s, s.Pool.ID, "getProtocolFeePercentageCache(uint256)", word(3), 0, word(0))
		},
		"cached-provider-cap": func(s *managedTestState) {
			m := s.Pool.MultiAsset.ManagedV2
			arg := fmt.Sprintf("%064x", m.ExpectedProviderFeeID.Big())
			edit(s, m.ExpectedFeeProvider, "getFeeTypePercentage(uint256)", arg, 0, word(100000000000000000))
			edit(s, m.ExpectedFeeProvider, "getFeeTypeMaximumPercentage(uint256)", arg, 0, word(200000000000000000))
		},
		"live-provider-cap": func(s *managedTestState) {
			m := s.Pool.MultiAsset.ManagedV2
			edit(s, m.ExpectedFeeProvider, "getFeeTypePercentage(uint256)", fmt.Sprintf("%064x", m.ExpectedProviderFeeID.Big()), 0, word(1000000000000000001))
		},
		"address-width": func(s *managedTestState) {
			key := s.Pool.ID + managedV2Selector("getVault()")
			raw := s.Calls[key]
			s.Calls[key] = "0x01" + raw[4:]
		},
		"bpt-cash-width": func(s *managedTestState) {
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", s.Pool.MultiAsset.PoolID[2:]+hex.EncodeToString(managedV2AddressWord(s.Pool.ID)), 0, fmt.Sprintf("%064x", new(big.Int).Lsh(big.NewInt(1), 112)))
		},
		"pool-immutable-config-matches-forgery": func(s *managedTestState) {
			code, _ := hex.DecodeString(s.Codes[s.Pool.ID][2:])
			at := managedV2Templates["ManagedPool"].Immutables["_owner"][0]
			code[at+31] ^= 1
			s.Codes[s.Pool.ID] = "0x" + hex.EncodeToString(code)
			s.Pool.MultiAsset.ExpectedCodeSHA256 = managedDigest(s.Codes[s.Pool.ID])
		},
		"pool-metadata-config-matches-forgery": func(s *managedTestState) {
			code, _ := hex.DecodeString(s.Codes[s.Pool.ID][2:])
			code[len(code)-1] ^= 1
			s.Codes[s.Pool.ID] = "0x" + hex.EncodeToString(code)
			s.Pool.MultiAsset.ExpectedCodeSHA256 = managedDigest(s.Codes[s.Pool.ID])
		},
		"provider-code-config-matches-forgery": func(s *managedTestState) {
			m := s.Pool.MultiAsset.ManagedV2
			code, _ := hex.DecodeString(s.Codes[m.ExpectedFeeProvider][2:])
			code[len(code)-1] ^= 1
			s.Codes[m.ExpectedFeeProvider] = "0x" + hex.EncodeToString(code)
			m.ExpectedFeeProviderCodeSHA256 = managedDigest(s.Codes[m.ExpectedFeeProvider])
		},
		"collector-code-config-matches-forgery": func(s *managedTestState) {
			m := s.Pool.MultiAsset.ManagedV2
			code, _ := hex.DecodeString(s.Codes[m.ExpectedProtocolCollector][2:])
			code[len(code)-1] ^= 1
			s.Codes[m.ExpectedProtocolCollector] = "0x" + hex.EncodeToString(code)
			m.ExpectedProtocolCollectorCodeSHA256 = managedDigest(s.Codes[m.ExpectedProtocolCollector])
		},
		"vault-code-config-matches-forgery": func(s *managedTestState) {
			v := s.Pool.MultiAsset
			code, _ := hex.DecodeString(s.Codes[v.ExpectedVault][2:])
			code[len(code)-1] ^= 1
			s.Codes[v.ExpectedVault] = "0x" + hex.EncodeToString(code)
			v.ExpectedVaultCodeSHA256 = managedDigest(s.Codes[v.ExpectedVault])
		},
		"pool-template-config": func(s *managedTestState) {
			s.Pool.MultiAsset.ManagedV2.ExpectedPoolRuntimeTemplateSHA256 = strings.Repeat("a", 64)
		},
		"pool-code-config-matches-forgery": func(s *managedTestState) {
			raw := s.Codes[s.Pool.ID]
			s.Codes[s.Pool.ID] = "0x00" + raw[4:]
			s.Pool.MultiAsset.ExpectedCodeSHA256 = managedDigest(s.Codes[s.Pool.ID])
		},
		"factory-code-config-matches-forgery": func(s *managedTestState) {
			v := s.Pool.MultiAsset.ManagedV2
			raw := s.Codes[v.ExpectedFactory]
			s.Codes[v.ExpectedFactory] = "0x00" + raw[4:]
			v.ExpectedFactoryCodeSHA256 = managedDigest(s.Codes[v.ExpectedFactory])
		},
		"helper-code-config-matches-forgery": func(s *managedTestState) {
			for i, d := range s.Pool.MultiAsset.ManagedV2.Dependencies {
				if d.Role != "recovery-mode-helper" {
					continue
				}
				raw := s.Codes[d.Address]
				s.Codes[d.Address] = "0x00" + raw[4:]
				s.Pool.MultiAsset.ManagedV2.Dependencies[i].CodeSHA256 = managedDigest(s.Codes[d.Address])
			}
		},
		"library-self-config-matches-forgery": func(s *managedTestState) {
			for i, d := range s.Pool.MultiAsset.ManagedV2.Dependencies {
				if d.Role != "circuit-breaker-lib" {
					continue
				}
				raw := s.Codes[d.Address]
				s.Codes[d.Address] = raw[:4] + strings.Repeat("0", 40) + raw[44:]
				s.Pool.MultiAsset.ManagedV2.Dependencies[i].CodeSHA256 = managedDigest(s.Codes[d.Address])
			}
		},
		"library-link-config-matches-forgery": func(s *managedTestState) {
			for i, d := range s.Pool.MultiAsset.ManagedV2.Dependencies {
				if d.Role != "managed-pool-amm-lib" {
					continue
				}
				raw := s.Codes[d.Address]
				at := managedV2Templates["ManagedPoolAmmLib"].Links["CircuitBreakerLib"][0]*2 + 2
				s.Codes[d.Address] = raw[:at] + strings.Repeat("0", 40) + raw[at+40:]
				s.Pool.MultiAsset.ManagedV2.Dependencies[i].CodeSHA256 = managedDigest(s.Codes[d.Address])
			}
		},
		"collector-code": func(s *managedTestState) {
			v := s.Pool.MultiAsset.ManagedV2
			s.Codes[v.ExpectedProtocolCollector] = "0x00"
		},
		"vault-code": func(s *managedTestState) { s.Codes[s.Pool.MultiAsset.ExpectedVault] = "0x00" },
	}
	for name, mutate := range tests {
		t.Run(name, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, mutate)
			_, pools, e := a.ReadState(context.Background(), "latest", false)
			if e == nil || pools != nil {
				t.Fatalf("accepted fault: %v, %d pools", e, len(pools))
			}
		})
	}
}

func TestManagedV2PausedAndDisabledAreCompleteStates(t *testing.T) {
	for _, sig := range []string{"getSwapEnabled()", "getPausedState()", "inRecoveryMode()"} {
		t.Run(sig, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
				key := s.Pool.ID + managedV2Selector(sig)
				value := int64(1)
				if sig == "getSwapEnabled()" {
					value = 0
				}
				s.Calls[key] = "0x" + fmt.Sprintf("%064x", value) + s.Calls[key][66:]
				if sig == "inRecoveryMode()" {
					fee := s.Pool.ID + managedV2Selector("getManagementAumFeeParams()")
					s.Calls[fee] = "0x" + fmt.Sprintf("%064x", 0) + s.Calls[fee][66:]
				}
			})
			_, pools, e := a.ReadState(context.Background(), "latest", false)
			if e != nil {
				t.Fatal(e)
			}
			m := pools[0].MultiAsset.ManagedV2
			if sig == "getSwapEnabled()" && m.SwapsEnabled || sig == "getPausedState()" && !m.Paused || sig == "inRecoveryMode()" && !m.RecoveryMode {
				t.Fatal("control state lost")
			}
		})
	}
}

func TestManagedV2EveryNativeSnapshot(t *testing.T) {
	b := managedLoadBank(t)
	var refs []string
	for hash, o := range b.Objects {
		if o.Kind == "snapshot" {
			refs = append(refs, hash)
		}
	}
	sort.Strings(refs)
	for _, hash := range refs {
		t.Run(hash[:10], func(t *testing.T) {
			var native struct {
				State struct {
					Pool             string
					Total            string `json:"total_supply"`
					Virtual          string `json:"virtual_supply"`
					Actual           string `json:"actual_supply"`
					Pending          string `json:"pending_aum"`
					Annual           string `json:"annual_aum_fee"`
					Last             string `json:"last_collection"`
					Provider         string `json:"provider_fee_id"`
					Cache            string `json:"cached_aum_share"`
					Swaps            bool   `json:"swaps_enabled"`
					Paused, Recovery bool
					Weights, Scaling []string
					BPT              map[string]string              `json:"bpt_balances"`
					VaultBPT         struct{ Cash, Managed string } `json:"vault_bpt"`
				}
			}
			ref := managedBankRef{Ref: hash}
			b.decode(t, ref, "snapshot", &native)
			index := -1
			for i, s := range b.Scenarios {
				if s.Pool == native.State.Pool {
					index = i
				}
			}
			if index < 0 {
				t.Fatal("unknown pool")
			}
			a, replay := managedNativeFixtureAt(t, index, ref, nil)
			_, pools, e := a.ReadState(context.Background(), replay.Header["number"].(string), false)
			if e != nil {
				t.Fatal(e)
			}
			m := pools[0].MultiAsset.ManagedV2
			n := native.State
			for label, pair := range map[string][2]string{"total": {m.TotalSupply.String(), n.Total}, "virtual": {m.VirtualSupply.String(), n.Virtual}, "actual": {m.ActualSupply.String(), n.Actual}, "pending": {m.PendingAUM.String(), n.Pending}, "annual": {m.ManagementAUMFee.String(), n.Annual}, "last": {fmt.Sprint(m.LastCollectionTimestamp), n.Last}, "provider": {m.ProviderFeeID.String(), n.Provider}, "cache": {m.CachedProtocolAUMPercentage.String(), n.Cache}, "BPT cash": {m.VaultBPTCash.String(), n.VaultBPT.Cash}, "BPT managed": {m.VaultBPTManaged.String(), n.VaultBPT.Managed}, "manager ledger": {m.ManagerBPTBalance.String(), n.BPT[m.Manager]}, "collector ledger": {m.ProtocolCollectorBPTBalance.String(), n.BPT[m.ProtocolCollector]}} {
				if pair[0] != pair[1] {
					t.Errorf("%s got %s want %s", label, pair[0], pair[1])
				}
			}
			var source struct {
				State struct {
					Underlying []managedBankRef
					Live       map[string]string `json:"provider_percentages"`
					Scales     []string          `json:"scaling_factors"`
					Weights    []string
				}
			}
			b.decode(t, ref, "snapshot", &source)
			if m.LiveProviderAUMPercentage.String() != source.State.Live[n.Provider] {
				t.Fatal("mapped provider live fee differs from native getter")
			}
			for i, tokenRef := range source.State.Underlying {
				var token struct {
					Address       string
					Decimals      uint8
					Cash, Managed string
					AssetManager  string `json:"asset_manager"`
					Last          string `json:"last_change_block"`
				}
				b.decode(t, tokenRef, "token_state", &token)
				asset := pools[0].MultiAsset.Assets[i]
				info := m.TokenInfo[i]
				cash, _ := new(big.Int).SetString(token.Cash, 10)
				managed, _ := new(big.Int).SetString(token.Managed, 10)
				if asset.Token != token.Address || asset.Balance.String() != new(big.Int).Add(cash, managed).String() || asset.Scaling.String() != source.State.Scales[i] || pools[0].MultiAsset.Weights[i].String() != source.State.Weights[i] || info.Cash.String() != token.Cash || info.Managed.String() != token.Managed || info.AssetManager != token.AssetManager || fmt.Sprint(info.LastChangeBlock) != token.Last {
					t.Fatalf("native token %d state differs", i)
				}
			}
			if m.SwapsEnabled != n.Swaps || m.RecoveryMode != n.Recovery {
				t.Fatal("control flags")
			}
		})
	}
}

func TestManagedV2StrictReturnHex(t *testing.T) {
	for _, kind := range []string{"call", "code"} {
		t.Run(kind, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
				if kind == "code" {
					s.Codes[s.Pool.Token0] = s.Codes[s.Pool.Token0][2:]
				} else {
					key := s.Pool.ID + managedV2Selector("getActualSupply()")
					s.Calls[key] = s.Calls[key][2:]
				}
			})
			_, pools, e := a.ReadState(context.Background(), "latest", false)
			if e == nil || pools != nil {
				t.Fatal("accepted non-RPC hex encoding")
			}
		})
	}
}
func TestManagedV2HeaderIdentity(t *testing.T) {
	zero := "0x" + strings.Repeat("0", 64)
	for _, tc := range []struct {
		name, number, hash, parent, stamp string
		ok                                bool
	}{
		{"genesis", "0x0", "", zero, "0x0", true}, {"non-genesis-zero-parent", "0x1", "", zero, "0x0", false}, {"zero-hash", "0x0", zero, zero, "0x0", false}, {"noncanonical-number", "0x00", "", zero, "0x0", false}, {"noncanonical-time", "0x0", "", zero, "0x00", false}, {"timestamp-overflow", "0x0", "", zero, "0x2000000000000001", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
				s.Header["number"] = tc.number
				s.Header["parentHash"] = tc.parent
				s.Header["timestamp"] = tc.stamp
				if tc.hash != "" {
					s.Header["hash"] = tc.hash
				}
			})
			ref, e := a.Header(context.Background(), "0x0")
			if (e == nil) != tc.ok {
				t.Fatalf("header error %v expectedOK %v", e, tc.ok)
			}
			if tc.ok && (ref.Number != 0 || ref.ParentHash != zero || ref.TimestampMS != 0) {
				t.Fatal("genesis identity lost")
			}
		})
	}
}
func TestManagedV2CanonicalRecheck(t *testing.T) {
	a, _ := managedNativeFixture(t, 0, func(s *managedTestState) { s.Reorg = true })
	_, pools, e := a.ReadState(context.Background(), "latest", false)
	if e == nil || pools != nil || e.Error() != "ANCHOR_REORGED" {
		t.Fatalf("reorg not rejected atomically %v", e)
	}
}
func TestManagedV2DependencyRPCFailureIsAtomic(t *testing.T) {
	for _, role := range []string{"pool", "vault", "factory", "provider", "collector", "token0", "circuit-breaker-lib", "managed-pool-add-remove-token-lib", "managed-pool-amm-lib", "external-weighted-math", "recovery-mode-helper"} {
		t.Run(role, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
				v := s.Pool.MultiAsset
				m := v.ManagedV2
				addr := map[string]string{"pool": s.Pool.ID, "vault": v.ExpectedVault, "factory": m.ExpectedFactory, "provider": m.ExpectedFeeProvider, "collector": m.ExpectedProtocolCollector, "token0": s.Pool.Token0}[role]
				for _, d := range m.Dependencies {
					if d.Role == role {
						addr = d.Address
					}
				}
				s.RPCFault = "eth_getCode" + addr
			})
			_, pools, e := a.ReadState(context.Background(), "latest", false)
			if e == nil || pools != nil {
				t.Fatalf("dependency failure admitted %v", e)
			}
		})
	}
}

func TestManagedV2LiveFeeAndCacheRemainIndependent(t *testing.T) {
	a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
		m := s.Pool.MultiAsset.ManagedV2
		key := m.ExpectedFeeProvider + managedV2Selector("getFeeTypePercentage(uint256)") + fmt.Sprintf("%064x", m.ExpectedProviderFeeID.Big())
		s.Calls[key] = "0x" + fmt.Sprintf("%064x", 400000000000000000)
	})
	_, pools, e := a.ReadState(context.Background(), "latest", false)
	if e != nil {
		t.Fatal(e)
	}
	m := pools[0].MultiAsset.ManagedV2
	if m.LiveProviderAUMPercentage != "400000000000000000" || m.CachedProtocolAUMPercentage != "300000000000000000" {
		t.Fatal("live provider fee substituted for cached AUM split")
	}
}
func TestManagedV2FixedAnchorRequired(t *testing.T) {
	a, s := managedNativeFixture(t, 0, nil)
	var header RawBlock
	raw, _ := json.Marshal(s.Header)
	json.Unmarshal(raw, &header)
	ref, e := blockReference(a.Chain.Key, header)
	if e != nil {
		t.Fatal(e)
	}
	for name, anchor := range map[string]map[string]any{"number": {"blockNumber": header.Number}, "noncanonical": {"blockHash": ref.Hash, "requireCanonical": false}, "other-hash": {"blockHash": "0x" + strings.Repeat("a", 64), "requireCanonical": true}, "extra-tag": {"blockHash": ref.Hash, "requireCanonical": true, "blockNumber": header.Number}} {
		t.Run(name, func(t *testing.T) {
			if _, e := a.readManagedV2(context.Background(), s.Pool, anchor, ref); e == nil {
				t.Fatal("unfixed managed read accepted")
			}
		})
	}
}

// Shape regression: a legitimate original-layout pool can choose the Vault as
// its immutable manager. The global ERC20 wallet may include other pools' BPT.
func TestManagedV2VaultManagerGlobalWallet(t *testing.T) {
	a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
		v := s.Pool.MultiAsset
		m := v.ManagedV2
		m.ExpectedManager = v.ExpectedVault
		s.Calls[s.Pool.ID+managedV2Selector("getOwner()")] = "0x" + hex.EncodeToString(managedV2AddressWord(v.ExpectedVault))
		code, _ := hex.DecodeString(s.Codes[s.Pool.ID][2:])
		for _, at := range managedV2Templates["ManagedPool"].Immutables["_owner"] {
			copy(code[at:at+32], managedV2AddressWord(v.ExpectedVault))
		}
		s.Codes[s.Pool.ID] = "0x" + hex.EncodeToString(code)
		v.ExpectedCodeSHA256 = managedDigest(s.Codes[s.Pool.ID])
		key := s.Pool.ID + managedV2Selector("balanceOf(address)") + hex.EncodeToString(managedV2AddressWord(v.ExpectedVault))
		balance, _ := new(big.Int).SetString(s.Calls[key][2:], 16)
		balance.Add(balance, big.NewInt(1))
		s.Calls[key] = "0x" + fmt.Sprintf("%064x", balance)
	})
	_, pools, e := a.ReadState(context.Background(), "latest", false)
	if e != nil {
		t.Fatal(e)
	}
	m := pools[0].MultiAsset.ManagedV2
	own := new(big.Int).Add(m.VaultBPTCash.Big(), m.VaultBPTManaged.Big())
	if m.VaultBPTActualBalance != m.ManagerBPTBalance {
		t.Fatal("aliased Vault/manager actual wallet differs")
	}
	if m.ManagerBPTBalance.Big().Cmp(own) <= 0 {
		t.Fatal("global wallet was equated to own registration")
	}
	if new(big.Int).Sub(m.TotalSupply.Big(), own).Cmp(m.VirtualSupply.Big()) != 0 {
		t.Fatal("virtual supply subtracted global wallet")
	}
}

// Synthetic RPC observations keep the original ABI/registration but increase
// each Vault-global wallet independently (other pools or internal balances).
func TestManagedV2RetainsVaultActualBalances(t *testing.T) {
	want := map[string]string{}
	a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
		vaultArg := hex.EncodeToString(managedV2AddressWord(s.Pool.MultiAsset.ExpectedVault))
		tokens := []string{s.Pool.ID}
		for _, token := range s.Pool.MultiAsset.Tokens {
			tokens = append(tokens, token.Token)
		}
		for i, token := range tokens {
			key := token + managedV2Selector("balanceOf(address)") + vaultArg
			balance, ok := new(big.Int).SetString(s.Calls[key][2:], 16)
			if !ok {
				t.Fatal("original Vault balance missing")
			}
			balance.Add(balance, big.NewInt(int64(i+17)))
			want[token] = balance.String()
			s.Calls[key] = "0x" + fmt.Sprintf("%064x", balance)
		}
	})
	_, pools, e := a.ReadState(context.Background(), "latest", false)
	if e != nil || len(pools) != 1 {
		t.Fatalf("synthetic Vault-global wallet parse: %v", e)
	}
	pool := pools[0]
	m := pool.MultiAsset.ManagedV2
	if m.VaultBPTActualBalance.String() != want[pool.PoolID] || m.VaultBPTActualBalance.Big().Cmp(m.VaultBPTCash.Big()) <= 0 {
		t.Fatal("BPT Vault wallet was read but not retained independently of registered cash")
	}
	for i, asset := range pool.MultiAsset.Assets {
		info := m.TokenInfo[i]
		if info.VaultActualBalance.String() != want[asset.Token] || info.VaultActualBalance.Big().Cmp(info.Cash.Big()) <= 0 {
			t.Fatalf("token %d Vault wallet was read but not retained independently of registered cash", i)
		}
		if asset.Balance.Big().Cmp(new(big.Int).Add(info.Cash.Big(), info.Managed.Big())) != 0 {
			t.Fatalf("token %d registration balance changed with the global wallet", i)
		}
	}
}

// Every baseline response must come from the selected original snapshot. In
// particular factory pause durations vary by header and the provider cap is 50%.
func TestManagedV2FixtureUsesOnlySnapshotGetters(t *testing.T) {
	b := managedLoadBank(t)
	for hash, obj := range b.Objects {
		if obj.Kind != "snapshot" {
			continue
		}
		ref := managedBankRef{Ref: hash}
		var snap managedBankSnapshot
		b.decode(t, ref, "snapshot", &snap)
		index := -1
		for i, scenario := range b.Scenarios {
			if scenario.Pool == snap.State.Pool {
				index = i
				break
			}
		}
		if index < 0 {
			t.Fatal("unknown snapshot pool")
		}
		t.Run(hash[:10], func(t *testing.T) {
			_, state := managedNativeFixtureAt(t, index, ref, nil)
			want := map[string]string{}
			for _, r := range snap.Getters {
				var g managedBankGetter
				b.decode(t, r, "getter", &g)
				want[g.To+g.Data] = g.Result
			}
			if len(state.Calls) != len(want) {
				t.Fatalf("getter closure size got %d want %d", len(state.Calls), len(want))
			}
			for key, result := range want {
				if state.Calls[key] != result {
					t.Errorf("getter %s differs from original snapshot: got %s want %s", key, state.Calls[key], result)
				}
			}
		})
	}
}

// Counterexamples keep each ABI shape and registration total valid while
// contradicting the original registration, ERC20 ledger or AUM calculation.
func TestManagedV2RejectsInconsistentOriginalState(t *testing.T) {
	word := func(v *big.Int) string { return fmt.Sprintf("%064x", v) }
	edit := func(s *managedTestState, to, sig, args string, index int, value *big.Int) {
		key := to + managedV2Selector(sig) + args
		raw := s.Calls[key]
		at := 2 + index*64
		s.Calls[key] = raw[:at] + word(value) + raw[at+64:]
	}
	infoArg := func(s *managedTestState, token string) string {
		return s.Pool.MultiAsset.PoolID[2:] + hex.EncodeToString(managedV2AddressWord(token))
	}
	wallet := func(s *managedTestState, token, owner string) string {
		return token + managedV2Selector("balanceOf(address)") + hex.EncodeToString(managedV2AddressWord(owner))
	}
	for name, mutate := range map[string]func(*managedTestState){
		"bpt-asset-manager": func(s *managedTestState) {
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, s.Pool.ID), 3, big.NewInt(1))
		},
		"bpt-managed": func(s *managedTestState) {
			key := s.Pool.MultiAsset.ExpectedVault + managedV2Selector("getPoolTokenInfo(bytes32,address)") + infoArg(s, s.Pool.ID)
			cash, _ := new(big.Int).SetString(s.Calls[key][2:66], 16)
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, s.Pool.ID), 0, cash.Sub(cash, big.NewInt(1)))
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, s.Pool.ID), 1, big.NewInt(1))
		},
		"managed-without-manager": func(s *managedTestState) {
			key := s.Pool.MultiAsset.ExpectedVault + managedV2Selector("getPoolTokenInfo(bytes32,address)") + infoArg(s, s.Pool.Token0)
			cash, _ := new(big.Int).SetString(s.Calls[key][2:66], 16)
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, s.Pool.Token0), 0, cash.Sub(cash, big.NewInt(1)))
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, s.Pool.Token0), 1, big.NewInt(1))
		},
		"underlying-wallet-below-cash": func(s *managedTestState) {
			s.Calls[wallet(s, s.Pool.Token0, s.Pool.MultiAsset.ExpectedVault)] = "0x" + word(big.NewInt(0))
		},
		"bpt-wallet-below-cash": func(s *managedTestState) {
			s.Calls[wallet(s, s.Pool.ID, s.Pool.MultiAsset.ExpectedVault)] = "0x" + word(big.NewInt(0))
		},
		"distinct-bpt-wallets-exceed-supply": func(s *managedTestState) {
			key := s.Pool.ID + managedV2Selector("getActualSupply()")
			virtual, _ := new(big.Int).SetString(s.Calls[key][2:], 16)
			s.Calls[wallet(s, s.Pool.ID, s.Pool.MultiAsset.ManagedV2.ExpectedManager)] = "0x" + word(virtual.Add(virtual, big.NewInt(1)))
		},
		"pool-last-change-not-token-maximum": func(s *managedTestState) {
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokens(bytes32)", s.Pool.MultiAsset.PoolID[2:], 2, big.NewInt(1))
			for _, token := range append([]string{s.Pool.ID}, s.Pool.Token0, s.Pool.Token1) {
				edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, token), 2, big.NewInt(0))
			}
		},
		"vault-last-change-wide": func(s *managedTestState) {
			// BalanceAllocation stores lastChangeBlock in exactly 32 bits,
			// even when the source header's uint64 block number is larger.
			s.Header["number"] = "0x100000001"
			wide := new(big.Int).Lsh(big.NewInt(1), 32)
			edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokens(bytes32)", s.Pool.MultiAsset.PoolID[2:], 2, wide)
			for _, token := range []string{s.Pool.ID, s.Pool.Token0, s.Pool.Token1} {
				edit(s, s.Pool.MultiAsset.ExpectedVault, "getPoolTokenInfo(bytes32,address)", infoArg(s, token), 2, wide)
			}
		},
		"future-collection-clock": func(s *managedTestState) {
			var header RawBlock
			raw, _ := json.Marshal(s.Header)
			json.Unmarshal(raw, &header)
			ref, e := blockReference("evm:31337", header)
			if e != nil {
				t.Fatal(e)
			}
			edit(s, s.Pool.ID, "getManagementAumFeeParams()", "", 1, new(big.Int).SetUint64(ref.TimestampMS/1000+1))
		},
		"forged-pending-aum": func(s *managedTestState) {
			key := s.Pool.ID + managedV2Selector("getActualSupply()")
			actual, _ := new(big.Int).SetString(s.Calls[key][2:], 16)
			s.Calls[key] = "0x" + word(actual.Add(actual, big.NewInt(1)))
		},
	} {
		t.Run(name, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, mutate)
			_, pools, e := a.ReadState(context.Background(), "latest", false)
			if e == nil || len(pools) != 0 {
				t.Fatalf("inconsistent original state accepted: %v", e)
			}
		})
	}
}

func TestManagedV2AUMOriginalIntegerBoundaries(t *testing.T) {
	const year = uint64(365 * 24 * 60 * 60)
	for _, tc := range []struct {
		name, virtual, fee string
		timestamp, last    uint64
		want               string
	}{
		{"annual-floor-before-time", "4", "400000000000000000", 2 * year, 0, "4"},
		{"fractional-year-floor", "9", "500000000000000000", year / 2, 0, "4"},
		{"maximum-fee", "1", "950000000000000000", year, 0, "19"},
		{"zero-fee", "9", "0", year, 0, "0"},
		{"same-clock", "9", "500000000000000000", year, year, "0"},
		// The math helper preserves the original early return; canonical source
		// admission independently rejects a future collection clock.
		{"original-future-clock-early-return", "9", "500000000000000000", 0, year, "0"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			virtual, _ := new(big.Int).SetString(tc.virtual, 10)
			fee, _ := new(big.Int).SetString(tc.fee, 10)
			got, e := managedV2PendingAUM(virtual, fee, tc.timestamp, tc.last)
			if e != nil || got.String() != tc.want {
				t.Fatalf("AUM got %v error %v want %s", got, e, tc.want)
			}
			if virtual.String() != tc.virtual || fee.String() != tc.fee {
				t.Fatal("AUM mutated input")
			}
		})
	}
	max := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 256), big.NewInt(1))
	for _, tc := range []struct {
		name         string
		virtual, fee *big.Int
		timestamp    uint64
	}{
		{"annual-product", new(big.Int).Lsh(big.NewInt(1), 255), big.NewInt(10000000000000000), year},
		{"elapsed-product", new(big.Int).Lsh(big.NewInt(1), 199), big.NewInt(10000000000000000), ^uint64(0)},
		{"actual-supply-addition", max, big.NewInt(1), year},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, e := managedV2PendingAUM(tc.virtual, tc.fee, tc.timestamp, 0); e == nil {
				t.Fatal("original SafeMath overflow accepted")
			}
		})
	}
}

func TestManagedV2VaultPauseNeverOverwritesPoolOwnPause(t *testing.T) {
	a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
		key := s.Pool.MultiAsset.ExpectedVault + managedV2Selector("getPausedState()")
		s.Calls[key] = "0x" + fmt.Sprintf("%064x", 1) + s.Calls[key][66:]
	})
	_, pools, err := a.ReadState(context.Background(), "latest", false)
	if err != nil {
		t.Fatal(err)
	}
	if pools[0].MultiAsset.ManagedV2.Paused {
		t.Fatal("Vault pause replaced Pool own pause; original collection is still permitted")
	}
}

func TestManagedV2RecoveryUsesOriginalEffectiveAUMGetter(t *testing.T) {
	a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
		id := s.Pool.ID
		s.Calls[id+managedV2Selector("inRecoveryMode()")] = "0x" + fmt.Sprintf("%064x", 1)
		key := id + managedV2Selector("getManagementAumFeeParams()")
		s.Calls[key] = "0x" + fmt.Sprintf("%064x", 0) + s.Calls[key][66:]
	})
	_, pools, err := a.ReadState(context.Background(), "latest", false)
	if err != nil {
		t.Fatal("original recovery mode must retain complete usable source:", err)
	}
	m := pools[0].MultiAsset.ManagedV2
	if !m.RecoveryMode || m.ManagementAUMFee != "0" || m.PendingAUM != "0" || m.ActualSupply != m.VirtualSupply {
		t.Fatal("recovery effective fee/supply view differs from original getter", m)
	}
}

func TestManagedV2RecoveryKeepsNormalFeePolicyAndRejectsForgedEffectiveFee(t *testing.T) {
	for _, c := range []struct {
		name     string
		recovery bool
		fee      int64
		valid    bool
	}{
		{"normal configured fee", false, 10000000000000000, true},
		{"recovery effective zero", true, 0, true},
		{"forged recovery normal fee", true, 10000000000000000, false},
		{"normal fee silently zeroed", false, 0, false},
		{"normal fee governance change", false, 20000000000000000, false},
	} {
		t.Run(c.name, func(t *testing.T) {
			a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
				flag := int64(0)
				if c.recovery {
					flag = 1
				}
				s.Calls[s.Pool.ID+managedV2Selector("inRecoveryMode()")] = "0x" + fmt.Sprintf("%064x", flag)
				key := s.Pool.ID + managedV2Selector("getManagementAumFeeParams()")
				s.Calls[key] = "0x" + fmt.Sprintf("%064x", c.fee) + s.Calls[key][66:]
			})
			_, pools, err := a.ReadState(context.Background(), "latest", false)
			if (err == nil) != c.valid || !c.valid && pools != nil {
				t.Fatalf("recovery/normal fee policy mismatch: %v", err)
			}
		})
	}
}
