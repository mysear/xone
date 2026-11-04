package domain

import (
	"encoding/binary"
	"encoding/json"
	"os"
	"testing"
)

func seedTestPool(t *testing.T, seed uint32) Pool {
	t.Helper()
	p := rayclmmTestPool(t)
	s := p.RaydiumCLMM
	p.Protocol = "raydium-clmm-custom-seed"
	p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-custom-seed-1"
	s.SeedIndex = seed
	s.BitmapExtension = &RaydiumCLMMBitmapExtension{Positive: make([]uint64, 112), Negative: make([]uint64, 112)}
	cfg, _ := decode58(s.AmmConfig)
	m0, _ := decode58(p.Token0)
	m1, _ := decode58(p.Token1)
	b := make([]byte, 2)
	binary.LittleEndian.PutUint16(b, uint16(seed))
	var e error
	p.PoolID, s.PoolBump, e = rayCLMMPDA([][]byte{[]byte("pool"), cfg, m0, m1, b})
	if e != nil {
		t.Fatal(e)
	}
	pool, _ := decode58(p.PoolID)
	s.Vault0, _, _ = rayCLMMPDA([][]byte{[]byte("pool_vault"), pool, m0})
	s.Vault1, _, _ = rayCLMMPDA([][]byte{[]byte("pool_vault"), pool, m1})
	s.Observation, _, _ = rayCLMMPDA([][]byte{[]byte("observation"), pool})
	s.BitmapExtension.Address, _, _ = rayCLMMPDA([][]byte{[]byte("pool_tick_array_bitmap_extension"), pool})
	for i := range s.TickArrays {
		b := make([]byte, 4)
		binary.BigEndian.PutUint32(b, uint32(s.TickArrays[i].StartTickIndex))
		s.TickArrays[i].Address, _, _ = rayCLMMPDA([][]byte{[]byte("tick_array"), pool, b})
	}
	return p
}
func TestRayCLMMCustomSeedIdentityAndWire(t *testing.T) {
	for _, seed := range []uint32{1, 255, 256, 65535} {
		p := seedTestPool(t, seed)
		if e := p.Validate(); e != nil {
			t.Fatal(seed, e)
		}
		w, e := RaydiumCLMMToProto(p.RaydiumCLMM)
		if e != nil {
			t.Fatal(e)
		}
		back, e := RaydiumCLMMFromProto(w)
		if e != nil || back.SeedIndex != seed {
			t.Fatal(seed, back, e)
		}
		p.RaydiumCLMM.SeedIndex ^= 1
		if p.Validate() == nil {
			t.Fatal("seedidentitychange accepted", seed)
		}
	}
}
func TestRayCLMMCustomSeedRejectsLegacyAndWidth(t *testing.T) {
	for _, seed := range []uint32{0, 65536, 4294967295} {
		p := seedTestPool(t, seed)
		if p.Validate() == nil {
			t.Fatal("invalidseed", seed)
		}
	}
	p := seedTestPool(t, 1)
	p.Protocol = "raydium-clmm-bitmap"
	p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-bitmap-1"
	if p.Validate() == nil {
		t.Fatal("oldprofile acceptedseed")
	}
	p = seedTestPool(t, 1)
	p.RaydiumCLMM.BitmapExtension = nil
	if p.Validate() == nil {
		t.Fatal("custompool missingextension accepted")
	}
}
func TestRayCLMMSeedZeroOmittedForLegacyGolden(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/rayclmm-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var p Pool
	if e = json.Unmarshal(b, &p); e != nil {
		t.Fatal(e)
	}
	raw, e := json.Marshal(p.RaydiumCLMM)
	if e != nil {
		t.Fatal(e)
	}
	var doc map[string]any
	_ = json.Unmarshal(raw, &doc)
	if _, exists := doc["seed_index"]; exists {
		t.Fatal("legacyzero seed notomitted")
	}
}

func TestRayCLMMOriginalCustomSeedIdentityVectors(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/rayclmm-seed-identities.json")
	if e != nil {
		t.Fatal(e)
	}
	type binding struct {
		Address string `json:"address"`
		Bump    uint32 `json:"bump"`
		Start   int32  `json:"start_tick_index"`
	}
	var f struct {
		Vectors []struct {
			Seed      uint32 `json:"seed_index"`
			Pool      string `json:"pool_id"`
			Bump      uint32 `json:"pool_bump"`
			Config    string `json:"amm_config"`
			Token0    string `json:"token0"`
			Token1    string `json:"token1"`
			BEPool    string `json:"wrong_be_pool_id"`
			BEBump    uint32 `json:"wrong_be_pool_bump"`
			BEDiffers bool   `json:"be_differs"`
			Deps      struct {
				Vault0      binding   `json:"vault0"`
				Vault1      binding   `json:"vault1"`
				Observation binding   `json:"observation"`
				Extension   binding   `json:"bitmap_extension"`
				Arrays      []binding `json:"tick_arrays"`
			} `json:"dependencies"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(b, &f); e != nil {
		t.Fatal(e)
	}
	if len(f.Vectors) != 9 {
		t.Fatal("missingoriginalseedvectors")
	}
	for _, v := range f.Vectors {
		config, _ := decode58(v.Config)
		m0, _ := decode58(v.Token0)
		m1, _ := decode58(v.Token1)
		seeds := [][]byte{[]byte("pool"), config, m0, m1}
		le := make([]byte, 2)
		binary.LittleEndian.PutUint16(le, uint16(v.Seed))
		if v.Seed > 0 {
			seeds = append(seeds, le)
		}
		id, bump, e := rayCLMMPDA(seeds)
		if e != nil || id != v.Pool || bump != v.Bump {
			t.Fatal(v.Seed, id, bump, e)
		}
		if v.Seed > 0 {
			be := make([]byte, 2)
			binary.BigEndian.PutUint16(be, uint16(v.Seed))
			seeds[len(seeds)-1] = be
			id, bump, e = rayCLMMPDA(seeds)
			if e != nil || id != v.BEPool || bump != v.BEBump || ((id != v.Pool) != v.BEDiffers) {
				t.Fatal("BEcontrol", v.Seed, id, e)
			}
		}
		pool, _ := decode58(v.Pool)
		for _, x := range []struct {
			Binding binding
			Seeds   [][]byte
		}{{v.Deps.Vault0, [][]byte{[]byte("pool_vault"), pool, m0}}, {v.Deps.Vault1, [][]byte{[]byte("pool_vault"), pool, m1}}, {v.Deps.Observation, [][]byte{[]byte("observation"), pool}}, {v.Deps.Extension, [][]byte{[]byte("pool_tick_array_bitmap_extension"), pool}}} {
			id, bump, e = rayCLMMPDA(x.Seeds)
			if e != nil || id != x.Binding.Address || bump != x.Binding.Bump {
				t.Fatal("dependency", v.Seed, id, bump, e)
			}
		}
		for _, a := range v.Deps.Arrays {
			start := make([]byte, 4)
			binary.BigEndian.PutUint32(start, uint32(a.Start))
			id, bump, e = rayCLMMPDA([][]byte{[]byte("tick_array"), pool, start})
			if e != nil || id != a.Address || bump != a.Bump {
				t.Fatal("array", v.Seed, a.Start, id, e)
			}
		}
	}
}
