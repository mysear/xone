package domain

import (
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"math/big"
)

// These public seeds use Solana's SHA-256 and compressed Edwards-Y rejection.
// Variable-time modular arithmetic here handles only public addresses, never keys.
func rayCLMMPDA(seeds [][]byte) (string, uint32, error) {
	program, err := decode58(RayCLMMProgram)
	if err != nil {
		return "", 0, err
	}
	prime := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 255), big.NewInt(19))
	d := new(big.Int).ModInverse(big.NewInt(121666), prime)
	d.Mul(d, big.NewInt(-121665)).Mod(d, prime)
	for bump := 255; bump >= 0; bump-- {
		h := sha256.New()
		for _, seed := range seeds {
			if len(seed) > 32 {
				return "", 0, errors.New("SOLANA_PDA_SEED")
			}
			h.Write(seed)
		}
		h.Write([]byte{byte(bump)})
		h.Write(program)
		h.Write([]byte("ProgramDerivedAddress"))
		raw := h.Sum(nil)
		compressed := append([]byte(nil), raw...)
		compressed[31] &= 127
		for i, j := 0, 31; i < j; i, j = i+1, j-1 {
			compressed[i], compressed[j] = compressed[j], compressed[i]
		}
		y := new(big.Int).SetBytes(compressed)
		y.Mod(y, prime)
		y2 := new(big.Int).Mul(y, y)
		y2.Mod(y2, prime)
		numerator := new(big.Int).Sub(y2, big.NewInt(1))
		numerator.Mod(numerator, prime)
		denominator := new(big.Int).Mul(d, y2)
		denominator.Add(denominator, big.NewInt(1)).Mod(denominator, prime)
		inverse := new(big.Int).ModInverse(denominator, prime)
		if inverse == nil {
			return encode58(raw), uint32(bump), nil
		}
		x2 := new(big.Int).Mul(numerator, inverse)
		x2.Mod(x2, prime)
		if new(big.Int).ModSqrt(x2, prime) == nil {
			return encode58(raw), uint32(bump), nil
		}
	}
	return "", 0, errors.New("SOLANA_PDA_DERIVATION")
}

func (s RaydiumCLMMState) validatePDAs(p Pool) error {
	config, e := decode58(s.AmmConfig)
	if e != nil {
		return e
	}
	mint0, e := decode58(p.Token0)
	if e != nil {
		return e
	}
	mint1, e := decode58(p.Token1)
	if e != nil {
		return e
	}
	seeds := [][]byte{[]byte("pool"), config, mint0, mint1}
	if s.SeedIndex > 0 {
		index := make([]byte, 2)
		binary.LittleEndian.PutUint16(index, uint16(s.SeedIndex))
		seeds = append(seeds, index)
	}
	pool, bump, e := rayCLMMPDA(seeds)
	if e != nil {
		return e
	}
	if pool != p.PoolID || bump != s.PoolBump {
		return errors.New("RAYCLMM_POOL_PDA")
	}
	poolBytes, e := decode58(p.PoolID)
	if e != nil {
		return e
	}
	for _, binding := range []struct {
		address string
		seeds   [][]byte
	}{
		{s.Vault0, [][]byte{[]byte("pool_vault"), poolBytes, mint0}},
		{s.Vault1, [][]byte{[]byte("pool_vault"), poolBytes, mint1}},
		{s.Observation, [][]byte{[]byte("observation"), poolBytes}},
	} {
		address, _, e := rayCLMMPDA(binding.seeds)
		if e != nil {
			return e
		}
		if address != binding.address {
			return errors.New("RAYCLMM_DEPENDENCY_PDA")
		}
	}
	if s.BitmapExtension != nil {
		address, _, e := rayCLMMPDA([][]byte{[]byte("pool_tick_array_bitmap_extension"), poolBytes})
		if e != nil {
			return e
		}
		if address != s.BitmapExtension.Address {
			return errors.New("RAYCLMM_EXTENSION_PDA")
		}
	}
	for _, a := range s.TickArrays {
		start := make([]byte, 4)
		binary.BigEndian.PutUint32(start, uint32(a.StartTickIndex))
		address, _, e := rayCLMMPDA([][]byte{[]byte("tick_array"), poolBytes, start})
		if e != nil {
			return e
		}
		if address != a.Address {
			return errors.New("RAYCLMM_ARRAY_PDA")
		}
	}
	return nil
}
