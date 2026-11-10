package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"os"
	"reflect"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

type circuitThin struct {
	OriginalRaw struct {
		SHA256 string
		Bytes  int
	} `json:"original_raw"`
	Banks []struct {
		ID    string
		Pools []struct {
			Name string
			Pool domain.Pool
		}
	}
}

func circuitOriginalData(t *testing.T) (scheduledSharedOriginal, circuitThin) {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/managed-v2-breaker.json")
	if e != nil {
		t.Fatal(e)
	}
	data, e := os.ReadFile("../../../test/contracts/managed-v2-breaker-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var native scheduledSharedOriginal
	var thin circuitThin
	if e = json.Unmarshal(raw, &native); e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(data, &thin); e != nil {
		t.Fatal(e)
	}
	sum := sha256.Sum256(raw)
	if thin.OriginalRaw.SHA256 != hex.EncodeToString(sum[:]) || thin.OriginalRaw.Bytes != len(raw) {
		t.Fatal("breaker raw qualification identity")
	}
	return native, thin
}
func circuitOriginalConfig(p domain.Pool) config.PoolConfig {
	c := bptOriginalConfig(p)
	c.MultiAsset.TokenPolicy = "standard-breaker-bpt"
	return c
}
func TestManagedV2BreakerEveryOriginalCompleteFixedBank(t *testing.T) {
	native, thin := circuitOriginalData(t)
	if len(native.Banks) != 300 || len(thin.Banks) != 300 {
		t.Fatal("complete original scenario inventory")
	}
	count := 0
	for _, bank := range native.Banks {
		t.Run(bank.ID, func(t *testing.T) {
			var pools []domain.Pool
			for _, row := range thin.Banks {
				if row.ID == bank.ID {
					for _, p := range row.Pools {
						pools = append(pools, p.Pool)
					}
					break
				}
			}
			if len(pools) != 2 {
				t.Fatal("both physical pools required")
			}
			configs := []config.PoolConfig{circuitOriginalConfig(pools[0]), circuitOriginalConfig(pools[1])}
			a := scheduledOriginalAdapter(t, native.Branches[bank.Namespace], scheduledSharedState(bank), configs[0])
			a.Chain.Pools = configs
			ref, actual, e := a.ReadState(context.Background(), bank.Header["number"].(string), false)
			if e != nil {
				t.Fatal(e)
			}
			if ref.Hash != bank.Header["hash"] || !reflect.DeepEqual(actual, pools) {
				got, _ := json.Marshal(actual)
				want, _ := json.Marshal(pools)
				t.Fatalf("complete original breaker mismatch\ngot=%s\nwant=%s", got, want)
			}
			count += len(pools)
		})
	}
	if count != 600 {
		t.Fatal("physical state inventory", count)
	}
}

func TestManagedV2BreakerSourceContradictionsAreAtomic(t *testing.T) {
	native, thin := circuitOriginalData(t)
	var selected scheduledSharedBank
	var expected domain.Pool
	found := false
	for _, bank := range native.Banks {
		if bank.Namespace != "dynamic" {
			continue
		}
		for _, row := range thin.Banks {
			if row.ID != bank.ID {
				continue
			}
			p := row.Pools[0].Pool
			m := p.MultiAsset.ManagedV2
			for i, c := range m.CircuitBreakers.Tokens {
				if c.StateWord != "0" && c.ReferenceWeight != p.MultiAsset.Weights[i] {
					selected = bank
					expected = p
					found = true
					break
				}
			}
		}
		if found {
			break
		}
	}
	if !found {
		t.Fatal("original dynamic cache-miss source absent")
	}
	cfg := circuitOriginalConfig(expected)
	token := expected.MultiAsset.Assets[0].Token
	slot := "0x" + hex.EncodeToString(managedV2Hash(string(append(managedV2AddressWord(token), managedV2Word(big.NewInt(12))...))))
	for _, kind := range []string{"missing-word", "short-word", "raw-price-getter", "dynamic-bound-getter", "original-pure-ratio", "missing-pure-ratio", "old-profile-smuggling"} {
		t.Run(kind, func(t *testing.T) {
			s := scheduledSharedState(selected)
			s.Storage = append(s.Storage[:0:0], s.Storage...)
			changed := false
			switch kind {
			case "missing-word", "short-word":
				for i := range s.Storage {
					w := &s.Storage[i]
					if w.Address == cfg.ID && w.Key == slot {
						if kind == "missing-word" {
							s.Storage = append(s.Storage[:i], s.Storage[i+1:]...)
						} else {
							w.Value = "0x1"
						}
						changed = true
						break
					}
				}
			case "raw-price-getter", "dynamic-bound-getter":
				for i := range s.Getters {
					g := &s.Getters[i]
					if g.To == cfg.ID && g.Method == "getCircuitBreakerState" && strings.Contains(g.Data, token[2:]) {
						offset := 2
						if kind == "dynamic-bound-getter" {
							offset += 4 * 64
						}
						n, _ := new(big.Int).SetString(g.Result[offset:offset+64], 16)
						n.Add(n, big.NewInt(1))
						g.Result = g.Result[:offset] + hex.EncodeToString(managedV2Word(n)) + g.Result[offset+64:]
						changed = true
					}
				}
			case "original-pure-ratio", "missing-pure-ratio":
				for i := range s.Getters {
					g := &s.Getters[i]
					if g.Method == "calcAdjustedBound" {
						if kind == "missing-pure-ratio" {
							g.Data = "0xdeadbeef"
						} else {
							n, _ := new(big.Int).SetString(g.Result[2:], 16)
							n.Add(n, big.NewInt(100000000000000000))
							g.Result = "0x" + hex.EncodeToString(managedV2Word(n))
						}
						changed = true
					}
				}
			case "old-profile-smuggling":
				cfg.Protocol = domain.ManagedV2BptProtocol
				cfg.MultiAsset.TokenPolicy = "standard-bpt"
				changed = true
			}
			if !changed {
				t.Fatal("counterexample did not mutate exact original read")
			}
			a := scheduledOriginalAdapter(t, native.Branches[selected.Namespace], s, cfg)
			_, pools, e := a.ReadState(context.Background(), selected.Header["number"].(string), false)
			if e == nil || len(pools) != 0 {
				t.Fatal("contradictory circuit source published", e, len(pools))
			}
		})
	}
}
