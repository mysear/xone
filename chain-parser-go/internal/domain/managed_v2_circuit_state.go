package domain

import (
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

const ManagedV2BreakerProtocol = "balancer-v2-managed-breaker"
const ManagedV2BreakerAdapterVersion = "balancer-v2/20230411-managed-breaker-1"

// ManagedV2CircuitToken preserves the whole storage word, including both cached
// uint32 ratios. Getter prices use the token's unscaled units.
type ManagedV2CircuitToken struct {
	Token              string `json:"token"`
	StateWord          Amount `json:"state_word"`
	ReferenceBPTPrice  Amount `json:"reference_bpt_price"`
	ReferenceWeight    Amount `json:"reference_weight"`
	LowerPercentage    Amount `json:"lower_percentage"`
	UpperPercentage    Amount `json:"upper_percentage"`
	LowerBPTPriceBound Amount `json:"lower_bpt_price_bound"`
	UpperBPTPriceBound Amount `json:"upper_bpt_price_bound"`
}
type ManagedV2CircuitState struct {
	Tokens []ManagedV2CircuitToken `json:"tokens"`
}

func (s *ManagedV2CircuitToken) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2CircuitToken
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2CircuitToken(v)
	return nil
}
func (s *ManagedV2CircuitState) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2CircuitState
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2CircuitState(v)
	return nil
}
func (s ManagedV2CircuitToken) validate() error {
	if ValidateEVMAddress(s.Token) != nil {
		return errors.New("MANAGED_V2_CIRCUIT_TOKEN")
	}
	for _, a := range []Amount{s.StateWord, s.ReferenceBPTPrice, s.ReferenceWeight, s.LowerPercentage, s.UpperPercentage, s.LowerBPTPriceBound, s.UpperBPTPriceBound} {
		if _, e := ParseAmount(string(a)); e != nil {
			return e
		}
	}
	word := s.StateWord.Big()
	one := big.NewInt(1_000_000_000_000_000_000)
	lower := managedScheduleField(word, 0, 16)
	upper := managedScheduleField(word, 16, 16)
	weight := managedScheduleField(word, 32, 64)
	price := managedScheduleField(word, 96, 96)
	// Compare compressed endpoints: compress(1e17) decompresses slightly below 1e17.
	if lower.Sign() != 0 && (lower.Cmp(new(big.Int).Rsh(big.NewInt(100_000_000_000_000_000), 48)) < 0 || lower.Cmp(new(big.Int).Rsh(new(big.Int).Set(one), 48)) > 0) || upper.Sign() != 0 && (upper.Cmp(lower) < 0 || upper.Cmp(new(big.Int).Rsh(new(big.Int).Mul(big.NewInt(10), one), 48)) > 0) {
		return errors.New("MANAGED_V2_CIRCUIT_PERCENTAGE_RANGE")
	}
	if word.Sign() != 0 && (weight.Cmp(big.NewInt(10_000_000_000_000_000)) < 0 || weight.Cmp(one) > 0) {
		return errors.New("MANAGED_V2_CIRCUIT_REFERENCE_WEIGHT")
	}
	if s.ReferenceWeight.Big().Cmp(weight) != 0 || s.LowerPercentage.Big().Cmp(new(big.Int).Lsh(lower, 48)) != 0 || s.UpperPercentage.Big().Cmp(new(big.Int).Lsh(upper, 48)) != 0 {
		return errors.New("MANAGED_V2_CIRCUIT_RAW_GETTER")
	}
	if price.Sign() == 0 && s.ReferenceBPTPrice != "0" || (s.LowerPercentage == "0" || price.Sign() == 0) && s.LowerBPTPriceBound != "0" || (s.UpperPercentage == "0" || price.Sign() == 0) && s.UpperBPTPriceBound != "0" {
		return errors.New("MANAGED_V2_CIRCUIT_DISABLED_GETTER")
	}
	return nil
}
func (s ManagedV2CircuitState) Validate() error {
	if len(s.Tokens) < 2 || len(s.Tokens) > 50 {
		return errors.New("MANAGED_V2_CIRCUIT_TOKEN_COUNT")
	}
	previous := ""
	for _, t := range s.Tokens {
		if previous >= t.Token {
			return errors.New("MANAGED_V2_CIRCUIT_TOKEN_ORDER")
		}
		if e := t.validate(); e != nil {
			return e
		}
		previous = t.Token
	}
	return nil
}
func circuitMulDiv(a, b, denominator *big.Int, ceil bool) (*big.Int, error) {
	product := new(big.Int).Mul(a, b)
	if product.BitLen() > 256 {
		return nil, errors.New("MANAGED_V2_CIRCUIT_MUL_OVERFLOW")
	}
	quotient, remainder := new(big.Int), new(big.Int)
	quotient.QuoRem(product, denominator, remainder)
	if ceil && remainder.Sign() != 0 {
		quotient.Add(quotient, big.NewInt(1))
	}
	return quotient, nil
}
func (s ManagedV2CircuitState) validatePool(assets []MultiAsset, weights []Amount) error {
	if e := s.Validate(); e != nil {
		return e
	}
	if len(s.Tokens) != len(assets) || len(weights) != len(assets) {
		return errors.New("MANAGED_V2_CIRCUIT_TOKEN_COUNT")
	}
	one := big.NewInt(1_000_000_000_000_000_000)
	for i, t := range s.Tokens {
		a := assets[i]
		if t.Token != a.Token {
			return errors.New("MANAGED_V2_CIRCUIT_TOKEN_IDENTITY")
		}
		word := t.StateWord.Big()
		price := managedScheduleField(word, 96, 96)
		reference, e := circuitMulDiv(price, a.Scaling.Big(), one, false)
		if e != nil {
			return e
		}
		if reference.Cmp(t.ReferenceBPTPrice.Big()) != 0 {
			return errors.New("MANAGED_V2_CIRCUIT_PRICE_GETTER")
		}
		// Dynamic bounds need original fixed-point pow in the pricing layer. This
		// domain validates their presence/range and disabled sides, not that pow.
		if weights[i] != t.ReferenceWeight {
			continue
		}
		for _, side := range []struct {
			percentage Amount
			cache      uint
			getter     Amount
			ceil       bool
		}{{t.LowerPercentage, 192, t.LowerBPTPriceBound, true}, {t.UpperPercentage, 224, t.UpperBPTPriceBound, false}} {
			if side.percentage == "0" || price.Sign() == 0 {
				continue
			}
			ratio, e := circuitMulDiv(managedScheduleField(word, side.cache, 32), new(big.Int).Mul(big.NewInt(10), one), new(big.Int).SetUint64(1<<32-1), false)
			if e != nil {
				return e
			}
			boundary, e := circuitMulDiv(price, ratio, one, side.ceil)
			if e != nil {
				return e
			}
			unscaled, e := circuitMulDiv(boundary, a.Scaling.Big(), one, false)
			if e != nil {
				return e
			}
			if unscaled.Cmp(side.getter.Big()) != 0 {
				return errors.New("MANAGED_V2_CIRCUIT_BOUND_GETTER")
			}
		}
	}
	return nil
}
func ManagedV2CircuitStateToProto(s *ManagedV2CircuitState) (*pb.ManagedV2CircuitState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	out := &pb.ManagedV2CircuitState{}
	for _, t := range s.Tokens {
		w := &pb.ManagedV2CircuitToken{Token: t.Token}
		values := []Amount{t.StateWord, t.ReferenceBPTPrice, t.ReferenceWeight, t.LowerPercentage, t.UpperPercentage, t.LowerBPTPriceBound, t.UpperBPTPriceBound}
		fields := []**pb.Amount{&w.StateWord, &w.ReferenceBptPrice, &w.ReferenceWeight, &w.LowerPercentage, &w.UpperPercentage, &w.LowerBptPriceBound, &w.UpperBptPriceBound}
		for i, a := range values {
			raw, e := a.Bytes()
			if e != nil {
				return nil, e
			}
			*fields[i] = &pb.Amount{Uint256Be: raw}
		}
		out.Tokens = append(out.Tokens, w)
	}
	return out, nil
}
func ManagedV2CircuitStateFromProto(w *pb.ManagedV2CircuitState) (*ManagedV2CircuitState, error) {
	if w == nil {
		return nil, nil
	}
	s := &ManagedV2CircuitState{Tokens: []ManagedV2CircuitToken{}}
	for _, t := range w.Tokens {
		if t == nil {
			return nil, errors.New("MANAGED_V2_CIRCUIT_TOKEN_REQUIRED")
		}
		v := ManagedV2CircuitToken{Token: t.Token}
		fields := []*Amount{&v.StateWord, &v.ReferenceBPTPrice, &v.ReferenceWeight, &v.LowerPercentage, &v.UpperPercentage, &v.LowerBPTPriceBound, &v.UpperBPTPriceBound}
		values := []*pb.Amount{t.StateWord, t.ReferenceBptPrice, t.ReferenceWeight, t.LowerPercentage, t.UpperPercentage, t.LowerBptPriceBound, t.UpperBptPriceBound}
		for i, a := range values {
			if a == nil {
				return nil, errors.New("MANAGED_V2_CIRCUIT_AMOUNT_REQUIRED")
			}
			v, e := AmountFromBytes(a.Uint256Be)
			if e != nil {
				return nil, e
			}
			*fields[i] = v
		}
		s.Tokens = append(s.Tokens, v)
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	return s, nil
}
