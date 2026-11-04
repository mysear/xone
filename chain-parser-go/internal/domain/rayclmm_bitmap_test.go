package domain

import (
	"encoding/json"
	"os"
	"testing"
)

func TestRayCLMMBitmapExtensionIndexAndProofIdentity(t *testing.T) {
	p := rayclmmTestPool(t)
	p.Protocol = "raydium-clmm-bitmap"
	p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-bitmap-1"
	pool, _ := decode58(p.PoolID)
	address, _, e := rayCLMMPDA([][]byte{[]byte("pool_tick_array_bitmap_extension"), pool})
	if e != nil {
		t.Fatal(e)
	}
	s := p.RaydiumCLMM
	s.BitmapExtension = &RaydiumCLMMBitmapExtension{Address: address, Positive: make([]uint64, 112), Negative: make([]uint64, 112)}
	for _, index := range []int32{512, 1023, 1024, 7679, -513, -1024, -1025, -7680} {
		var words []uint64
		offset := int(index - 512)
		if index >= 512 {
			words = s.BitmapExtension.Positive
		} else {
			group := (-index-1)/512 - 1
			bit := index + (group+2)*512
			offset = int(group*512 + bit)
			words = s.BitmapExtension.Negative
		}
		words[offset/64] |= uint64(1) << uint(offset%64)
		active, e := s.ArrayInitialized(index)
		if e != nil || !active {
			t.Fatal(index, active, e)
		}
	}
	if e = p.Validate(); e != nil {
		t.Fatal(e)
	}
	w, e := RaydiumCLMMToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	back, e := RaydiumCLMMFromProto(w)
	if e != nil || back.BitmapExtension.Negative[7] != s.BitmapExtension.Negative[7] {
		t.Fatal(e)
	}
	cloned := p.Clone()
	cloned.RaydiumCLMM.BitmapExtension.Positive[0] ^= 1
	if cloned.RaydiumCLMM.BitmapExtension.Positive[0] == s.BitmapExtension.Positive[0] {
		t.Fatal("bitmap clone alias")
	}
	s.BitmapExtension.Address = OrcaProgramData
	if p.Validate() == nil {
		t.Fatal("foreign extension accepted")
	}
}
func TestRayCLMMBitmapExtensionRequiresExplicitProfileAndWidth(t *testing.T) {
	p := rayclmmTestPool(t)
	p.RaydiumCLMM.BitmapExtension = &RaydiumCLMMBitmapExtension{Address: OrcaProgramData, Positive: make([]uint64, 112), Negative: make([]uint64, 112)}
	if p.Validate() == nil {
		t.Fatal("old profile accepts extension")
	}
	p.Protocol = "raydium-clmm-bitmap"
	p.AdapterVersion = "raydium-clmm/6d7a0f82-spl-bitmap-1"
	p.RaydiumCLMM.BitmapExtension.Positive = p.RaydiumCLMM.BitmapExtension.Positive[:111]
	if p.Validate() == nil {
		t.Fatal("truncatedbitmap accepted")
	}
	p.RaydiumCLMM.BitmapExtension = nil
	if p.Validate() == nil {
		t.Fatal("missing extension")
	}
}

func TestRayCLMMOriginalExtensionIndexVectors(t *testing.T) {
	b, e := os.ReadFile("../../test/contracts/rayclmm-bitmap-indices.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Vectors []struct {
			Index    int32    `json:"index"`
			Positive []uint64 `json:"positive"`
			Negative []uint64 `json:"negative"`
			Expected bool     `json:"expected_initialized"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(b, &f); e != nil {
		t.Fatal(e)
	}
	if len(f.Vectors) < 80 {
		t.Fatal("missing sourcevectors")
	}
	for _, v := range f.Vectors {
		s := rayclmmTestPool(t).RaydiumCLMM
		s.TickSpacing = 1
		s.BitmapExtension = &RaydiumCLMMBitmapExtension{Address: OrcaProgramData, Positive: v.Positive, Negative: v.Negative}
		active, e := s.ArrayInitialized(v.Index)
		if e != nil || active != v.Expected {
			t.Fatal(v.Index, active, e)
		}
		for _, neighbor := range []int32{v.Index - 1, v.Index + 1} {
			if neighbor >= -7680 && neighbor <= 7679 && (neighbor < -512 || neighbor > 511) {
				active, e = s.ArrayInitialized(neighbor)
				if e != nil || active {
					t.Fatal("spurious bit", v.Index, neighbor, active, e)
				}
			}
		}
	}
}
