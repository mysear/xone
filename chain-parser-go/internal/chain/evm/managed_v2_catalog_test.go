package evm

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestManagedV2CatalogAllOriginalBanks(t *testing.T) {
	fixture := "../../../test/contracts/managed-v2-aum-catalog.json"
	if override := os.Getenv("XONE_CATALOG_RAW"); override != "" {
		if !filepath.IsAbs(override) {
			t.Fatal("temporary catalog override must be absolute")
		}
		fixture = override
	}
	compareManagedV2NativeCatalog(t, fixture, "b843e859a5e92efcc1880eed437e475042785a7967bf2277b6f9770cf93b2813", 10, 4)
}
func TestManagedV2SharedVaultAllOriginalBanks(t *testing.T) {
	compareManagedV2NativeCatalog(t, "../../../test/contracts/managed-v2-shared-vault.json", "1f917c9951269028d24f927e1d415229f200019881bd05991e8a84f0b49c79e2", 7, 2)
}
func compareManagedV2NativeCatalog(t *testing.T, fixture, digest string, banks, poolCount int) {
	t.Helper()
	raw, err := os.ReadFile(fixture)
	if err != nil {
		t.Fatal(err)
	}
	if fmt.Sprintf("%x", sha256.Sum256(raw)) != digest {
		t.Fatal("catalog raw identity")
	}

	var corpus struct {
		Identities []struct {
			Address string
			Code    string `json:"code_hex"`
		} `json:"identities"`
		Storage []struct {
			Address string
			Code    string `json:"code_hex"`
		} `json:"factory_creation_code_storage"`
		Banks map[string]struct {
			Header map[string]any `json:"header"`
			Pools  []struct {
				Pool    domain.Pool         `json:"pool"`
				Getters []managedBankGetter `json:"getters"`
			} `json:"pools"`
		} `json:"banks"`
	}
	if err = json.Unmarshal(raw, &corpus); err != nil {
		t.Fatal(err)
	}
	if len(corpus.Banks) != banks {
		t.Fatal("catalog common bank inventory", len(corpus.Banks))
	}
	codes := map[string]string{}
	for _, i := range corpus.Identities {
		codes[i.Address] = i.Code
	}
	for _, i := range corpus.Storage {
		codes[i.Address] = i.Code
	}
	seen := 0
	for name, bank := range corpus.Banks {
		t.Run(name, func(t *testing.T) {
			calls := map[string]string{}
			var pools []config.PoolConfig
			if len(bank.Pools) != poolCount {
				t.Fatal("incomplete common source")
			}
			for _, item := range bank.Pools {
				p := item.Pool
				s := p.MultiAsset
				m := s.ManagedV2
				cfg := config.PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator}
				cfg.MultiAsset = &config.MultiAssetConfig{ExpectedCodeSHA256: s.CodeSHA256, ExpectedVault: s.Vault, ExpectedVaultCodeSHA256: s.VaultCodeSHA256, PoolID: s.PoolID, TokenPolicy: "standard-no-bpt"}
				cfg.MultiAsset.ManagedV2 = &config.ManagedV2PoolConfig{ExpectedFactory: m.Factory, ExpectedFactoryCodeSHA256: m.FactoryCodeSHA256, ExpectedPoolRuntimeTemplateSHA256: m.PoolRuntimeTemplateSHA256, ExpectedFeeProvider: m.FeeProvider, ExpectedFeeProviderCodeSHA256: m.FeeProviderCodeSHA256, ExpectedProtocolCollector: m.ProtocolCollector, ExpectedProtocolCollectorCodeSHA256: m.ProtocolCollectorCodeSHA256, ExpectedManager: m.Manager, ExpectedProviderFeeID: m.ProviderFeeID, ExpectedManagementAUMFee: m.ManagementAUMFee, Dependencies: append([]domain.ManagedV2Dependency(nil), m.Dependencies...)}
				for _, a := range s.Assets {
					cfg.MultiAsset.Tokens = append(cfg.MultiAsset.Tokens, config.MultiTokenConfig{Token: a.Token, ExpectedCodeSHA256: a.CodeSHA256})
				}
				pools = append(pools, cfg)
				for _, g := range item.Getters {
					key := g.To + g.Data
					if old, ok := calls[key]; ok && old != g.Result {
						t.Fatal("conflicting original shared getter", key)
					}
					calls[key] = g.Result
				}
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
				switch q.Method {
				case "eth_getBlockByNumber":
					result = bank.Header
				case "eth_getCode", "eth_call":
					var anchor map[string]any
					if e := json.Unmarshal(q.Params[1], &anchor); e != nil {
						t.Error(e)
					}
					if len(anchor) != 2 || anchor["blockHash"] != bank.Header["hash"] || anchor["requireCanonical"] != true {
						t.Error("catalog read lost canonical anchor", anchor)
					}
					if q.Method == "eth_getCode" {
						var address string
						json.Unmarshal(q.Params[0], &address)
						value, ok := codes[address]
						if !ok {
							t.Error("unknown catalog code", address)
						}
						result = value
					} else {
						var c map[string]string
						json.Unmarshal(q.Params[0], &c)
						value, ok := calls[c["to"]+c["data"]]
						if !ok {
							t.Error("unknown catalog getter", c)
						}
						result = value
					}
				default:
					t.Error("unexpected catalog RPC", q.Method)
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
			}))
			defer server.Close()
			adapter := Adapter{Client: rpc.New([]string{server.URL}, 15*time.Second, 4<<20), Chain: config.Chain{Key: "evm:31337", Pools: pools}}
			_, got, e := adapter.ReadState(context.Background(), "latest", false)
			if e != nil {
				t.Fatal("original fixed-fixture four-pool bank", e)
			}
			if len(got) != poolCount {
				t.Fatal("bank scope lost")
			}
			for i, pool := range got {
				expected, e := json.Marshal(bank.Pools[i].Pool)
				if e != nil {
					t.Fatal(e)
				}
				managedV2ComparePublicPool(t, pool, expected)
				seen++
			}
		})
	}
	if seen != banks*poolCount {
		t.Fatal("incomplete original pool/header projection", seen)
	}
	t.Logf("verified %d complete public Pools at 10 common source headers", seen)
}
