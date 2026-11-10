package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"reflect"
	"testing"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

type scheduledSharedBank struct {
	scheduledOriginalState
	Namespace string
	Pools     []struct {
		Name  string
		State scheduledOriginalState
	}
}
type scheduledSharedOriginal struct {
	Branches map[string]scheduledOriginalBranch
	Banks    []scheduledSharedBank
}
type scheduledSharedProjection struct {
	OriginalRaw struct {
		SHA256 string
		Bytes  int
	} `json:"original_raw"`
	Banks map[string]struct {
		Header map[string]any
		Pools  []struct {
			Name   string
			Pool   domain.Pool
			Config config.PoolConfig
		}
	}
}

func scheduledSharedOriginals(t *testing.T) (scheduledSharedOriginal, scheduledSharedProjection) {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/managed-v2-scheduled-shared.json")
	if e != nil {
		t.Fatal(e)
	}
	data, e := os.ReadFile("../../../test/contracts/managed-v2-scheduled-shared-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var native scheduledSharedOriginal
	var thin scheduledSharedProjection
	if e = json.Unmarshal(raw, &native); e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(data, &thin); e != nil {
		t.Fatal(e)
	}
	digest := sha256.Sum256(raw)
	if thin.OriginalRaw.SHA256 != hex.EncodeToString(digest[:]) || thin.OriginalRaw.Bytes != len(raw) {
		t.Fatal("shared projection detached from original raw")
	}
	return native, thin
}
func scheduledSharedState(bank scheduledSharedBank) scheduledOriginalState {
	state := bank.scheduledOriginalState
	state.Getters = nil
	for _, p := range bank.Pools {
		state.Getters = append(state.Getters, p.State.Getters...)
	}
	return state
}
func TestManagedScheduledSharedEveryOriginalCommonBank(t *testing.T) {
	native, thin := scheduledSharedOriginals(t)
	if len(native.Banks) != 72 || len(thin.Banks) != 72 {
		t.Fatal("incomplete common-H inventory")
	}
	for _, bank := range native.Banks {
		t.Run(bank.ID, func(t *testing.T) {
			projected, ok := thin.Banks[bank.ID]
			if !ok || len(projected.Pools) != 2 {
				t.Fatal("missing two-pool source projection")
			}
			if !reflect.DeepEqual(projected.Header, bank.Header) {
				t.Fatal("projection header differs")
			}
			configs := []config.PoolConfig{projected.Pools[0].Config, projected.Pools[1].Config}
			a := scheduledOriginalAdapter(t, native.Branches[bank.Namespace], scheduledSharedState(bank), configs[0])
			a.Chain.Pools = configs
			ref, actual, e := a.ReadState(context.Background(), bank.Header["number"].(string), false)
			if e != nil {
				t.Fatal(e)
			}
			if len(actual) != 2 || ref.Hash != bank.Header["hash"] {
				t.Fatal("bank cardinality/source H differs")
			}
			expected := map[string]domain.Pool{}
			for _, row := range projected.Pools {
				expected[row.Pool.PoolID] = row.Pool
			}
			for _, pool := range actual {
				want, ok := expected[pool.PoolID]
				if !ok || !reflect.DeepEqual(pool, want) {
					got, _ := json.Marshal(pool)
					target, _ := json.Marshal(want)
					t.Fatalf("complete shared native pool differs\ngot=%s\nwant=%s", got, target)
				}
			}
		})
	}
}
func TestManagedScheduledSharedMissingPeerIsAtomicAndLegacyCannotPromote(t *testing.T) {
	native, thin := scheduledSharedOriginals(t)
	var bank scheduledSharedBank
	for _, candidate := range native.Banks {
		if candidate.Namespace == "active" {
			m := thin.Banks[candidate.ID].Pools[1].Pool.MultiAsset.ManagedV2
			if m.Timestamp > 0 && m.Timestamp < m.GradualWeightEnd {
				bank = candidate
				break
			}
		}
	}
	if bank.ID == "" {
		t.Fatal("no active peer source")
	}
	for _, fault := range []string{"missing-peer-getter", "legacy-promotion"} {
		t.Run(fault, func(t *testing.T) {
			projected := thin.Banks[bank.ID]
			configs := []config.PoolConfig{projected.Pools[0].Config, projected.Pools[1].Config}
			state := scheduledSharedState(bank)
			if fault == "legacy-promotion" {
				configs[1].Protocol = domain.ManagedV2Protocol
			} else {
				filtered := make([]managedBankGetter, 0, len(state.Getters))
				removed := false
				for _, g := range state.Getters {
					if g.To == configs[1].ID && g.Method == "getActualSupply" {
						removed = true
						continue
					}
					filtered = append(filtered, g)
				}
				if !removed {
					t.Fatal("fault did not remove actual peer read")
				}
				state.Getters = filtered
			}
			a := scheduledOriginalAdapter(t, native.Branches[bank.Namespace], state, configs[0])
			a.Chain.Pools = configs
			_, pools, e := a.ReadState(context.Background(), bank.Header["number"].(string), false)
			if e == nil || pools != nil {
				t.Fatal("unqualified peer allowed a partial source bank or implicit old-profile promotion", e)
			}
		})
	}
}
