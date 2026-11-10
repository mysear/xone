package evm

import (
	"encoding/hex"
	"math/big"
	"xone/chain-parser-go/internal/domain"
)

// Mapping12 is authenticated by the fixed compiler/layout and original linked
// circuit library identity already checked by the parent Managed reader.
func (a *Adapter) readManagedV2CircuitToken(r *managedV2Reader, pool, token string, scaling, weight domain.Amount, m *domain.BalancerManagedV2State, getter [][]byte, library string) {
	encoded := append(managedV2AddressWord(token), managedV2Word(big.NewInt(12))...)
	key := "0x" + hex.EncodeToString(managedV2Hash(string(encoded)))
	word := r.storageWord(pool, key)
	fields := make([]domain.Amount, 6)
	for i, g := range getter {
		fields[i] = domain.Amount(r.uintWord(g, 256).String())
	}
	one := big.NewInt(1000000000000000000)
	fixed := func(a, b *big.Int, ceil bool) *big.Int {
		product := new(big.Int).Mul(a, b)
		r.require(product.BitLen() <= 256, "CIRCUIT_MUL_OVERFLOW")
		if ceil && product.Sign() > 0 {
			return new(big.Int).Add(new(big.Int).Div(new(big.Int).Sub(product, big.NewInt(1)), one), big.NewInt(1))
		}
		return new(big.Int).Div(product, one)
	}
	price := managedV2ScheduledBits(word.Big(), 96, 96)
	reference := managedV2ScheduledBits(word.Big(), 32, 64)
	lower := new(big.Int).Lsh(managedV2ScheduledBits(word.Big(), 0, 16), 48)
	upper := new(big.Int).Lsh(managedV2ScheduledBits(word.Big(), 16, 16), 48)
	r.require(fields[0].Big().Cmp(fixed(price, scaling.Big(), false)) == 0 && fields[1].Big().Cmp(reference) == 0 && fields[2].Big().Cmp(lower) == 0 && fields[3].Big().Cmp(upper) == 0, "CIRCUIT_RAW_GETTERS")
	for index, bound := range []*big.Int{lower, upper} {
		expected := new(big.Int)
		if bound.Sign() != 0 {
			var ratio *big.Int
			if reference.Cmp(weight.Big()) == 0 {
				offset := uint(192)
				if index == 1 {
					offset = 224
				}
				ratio = new(big.Int).Div(new(big.Int).Mul(managedV2ScheduledBits(word.Big(), offset, 32), new(big.Int).Mul(one, big.NewInt(10))), new(big.Int).SetUint64(1<<32-1))
			} else {
				flag := int64(1)
				if index == 1 {
					flag = 0
				}
				args := hex.EncodeToString(managedV2Word(bound)) + hex.EncodeToString(managedV2Word(weight.Big())) + hex.EncodeToString(managedV2Word(big.NewInt(flag)))
				ratio = r.uint(library, "calcAdjustedBound(uint256,uint256,bool)", args, 256)
			}
			expected = fixed(fixed(price, ratio, index == 0), scaling.Big(), false)
		}
		r.require(fields[index+4].Big().Cmp(expected) == 0, "CIRCUIT_CURRENT_BOUND_GETTER")
	}
	m.CircuitBreakers.Tokens = append(m.CircuitBreakers.Tokens, domain.ManagedV2CircuitToken{Token: token, StateWord: word, ReferenceBPTPrice: fields[0], ReferenceWeight: fields[1], LowerPercentage: fields[2], UpperPercentage: fields[3], LowerBPTPriceBound: fields[4], UpperBPTPriceBound: fields[5]})
}
