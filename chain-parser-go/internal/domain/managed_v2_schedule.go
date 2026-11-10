package domain

import (
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

const ManagedV2ScheduledProtocol = "balancer-v2-managed-scheduled"
const ManagedV2ScheduledAdapterVersion = "balancer-v2/20230411-managed-scheduled-1"

// These words are the fixed 20230411 original storage encoding. They are not
// compressed weights/fees and contain no inferred canonical block identity.
type ManagedV2ScheduledToken struct {
	Token     string `json:"token"`
	StateWord Amount `json:"state_word"`
}
type ManagedV2Schedule struct {
	PoolStateWord Amount                    `json:"pool_state_word"`
	Tokens        []ManagedV2ScheduledToken `json:"tokens"`
}
type ManagedV2ScheduleView struct {
	Weights                                                       []Amount
	Scaling                                                       []Amount
	SwapFee                                                       Amount
	WeightStart, WeightEnd, FeeStart, FeeEnd                      uint64
	SwapsEnabled, MustAllowlistLPs, RecoveryMode, JoinExitEnabled bool
}

func (s *ManagedV2ScheduledToken) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2ScheduledToken
	var value plain
	if e := decodeManagedV2JSON(raw, &value, true); e != nil {
		return e
	}
	*s = ManagedV2ScheduledToken(value)
	return nil
}
func (s *ManagedV2Schedule) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2Schedule
	var value plain
	if e := decodeManagedV2JSON(raw, &value, true); e != nil {
		return e
	}
	*s = ManagedV2Schedule(value)
	return nil
}
func managedScheduleField(word *big.Int, offset, width uint) *big.Int {
	shifted := new(big.Int).Rsh(new(big.Int).Set(word), offset)
	mask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), width), big.NewInt(1))
	return shifted.And(shifted, mask)
}

type managedScheduleFields struct {
	starts, ends     []*big.Int
	feeStart, feeEnd *big.Int
	view             ManagedV2ScheduleView
}

func (s ManagedV2Schedule) decode() (managedScheduleFields, error) {
	fail := func(label string) (managedScheduleFields, error) {
		return managedScheduleFields{}, errors.New("MANAGED_V2_SCHEDULE_" + label)
	}
	if _, e := ParseAmount(s.PoolStateWord.String()); e != nil {
		return fail("WORD")
	}
	if s.Tokens == nil || len(s.Tokens) < 2 || len(s.Tokens) > 50 {
		return fail("TOKEN_COUNT")
	}
	one := big.NewInt(1000000000000000000)
	minimum := big.NewInt(10000000000000000)
	word := s.PoolStateWord.Big()
	field := func(offset, width uint) *big.Int { return managedScheduleField(word, offset, width) }
	f := managedScheduleFields{feeStart: field(128, 62), feeEnd: field(190, 62)}
	f.view.WeightStart = field(0, 32).Uint64()
	f.view.WeightEnd = field(32, 32).Uint64()
	f.view.FeeStart = field(64, 32).Uint64()
	f.view.FeeEnd = field(96, 32).Uint64()
	f.view.SwapsEnabled = field(252, 1).Sign() != 0
	f.view.MustAllowlistLPs = field(253, 1).Sign() != 0
	f.view.RecoveryMode = field(254, 1).Sign() != 0
	f.view.JoinExitEnabled = field(255, 1).Sign() != 0
	if f.view.WeightStart > f.view.WeightEnd || f.view.FeeStart > f.view.FeeEnd {
		return fail("TIME_ORDER")
	}
	for _, value := range []*big.Int{f.feeStart, f.feeEnd} {
		if value.Cmp(big.NewInt(1000000000000)) < 0 || value.Cmp(big.NewInt(950000000000000000)) > 0 {
			return fail("FEE_RANGE")
		}
	}
	targetSum := new(big.Int)
	for i, t := range s.Tokens {
		if ValidateEVMAddress(t.Token) != nil || i > 0 && s.Tokens[i-1].Token >= t.Token {
			return fail("TOKEN_ORDER")
		}
		if _, e := ParseAmount(t.StateWord.String()); e != nil {
			return fail("TOKEN_WORD")
		}
		token := t.StateWord.Big()
		if token.BitLen() > 133 {
			return fail("TOKEN_RESERVED_BITS")
		}
		start, end := managedScheduleField(token, 0, 64), managedScheduleField(token, 64, 64)
		for _, value := range []*big.Int{start, end} {
			if value.Cmp(minimum) < 0 || value.Cmp(one) > 0 {
				return fail("WEIGHT_RANGE")
			}
		}
		decimals := managedScheduleField(token, 128, 5).Uint64()
		if decimals > 18 {
			return fail("DECIMALS")
		}
		f.starts = append(f.starts, start)
		f.ends = append(f.ends, end)
		targetSum.Add(targetSum, end)
		factor := new(big.Int).Exp(big.NewInt(10), new(big.Int).SetUint64(decimals), nil)
		f.view.Scaling = append(f.view.Scaling, Amount(new(big.Int).Mul(one, factor).String()))
	}
	// The caller targets are checked by the original management entry point.
	// Computed starts come from the previous live getter and may sum to ONE±n.
	if targetSum.Cmp(one) != 0 {
		return fail("TARGET_WEIGHT_SUM")
	}
	return f, nil
}
func (s ManagedV2Schedule) Validate() error { _, e := s.decode(); return e }
func managedScheduleProgress(start, end, timestamp uint64) *big.Int {
	one := big.NewInt(1000000000000000000)
	if timestamp >= end {
		return one
	}
	if timestamp <= start {
		return new(big.Int)
	}
	elapsed := new(big.Int).SetUint64(timestamp - start)
	return elapsed.Quo(elapsed.Mul(elapsed, one), new(big.Int).SetUint64(end-start))
}
func managedScheduleValue(start, end, progress *big.Int) (*big.Int, error) {
	one := big.NewInt(1000000000000000000)
	delta := new(big.Int).Sub(end, start)
	descending := delta.Sign() < 0
	delta.Abs(delta)
	product := new(big.Int).Mul(delta, progress)
	if product.BitLen() > 256 {
		return nil, errors.New("MANAGED_V2_SCHEDULE_UINT256_OVERFLOW")
	}
	product.Quo(product, one)
	if descending {
		return product.Sub(start, product), nil
	}
	return product.Add(start, product), nil
}

// At follows the original divDown -> mulDown -> directional add/subtract order.
// Equal timestamps are a direct transition, and clocks after uint32 remain
// valid once the schedule has ended. No current-weight normalization is done.
func (s ManagedV2Schedule) At(timestamp uint64) (ManagedV2ScheduleView, error) {
	if timestamp > ^uint64(0)/1000 {
		return ManagedV2ScheduleView{}, errors.New("MANAGED_V2_SCHEDULE_CLOCK")
	}
	fields, e := s.decode()
	if e != nil {
		return ManagedV2ScheduleView{}, e
	}
	out := fields.view
	progress := managedScheduleProgress(out.WeightStart, out.WeightEnd, timestamp)
	for i, start := range fields.starts {
		weight, e := managedScheduleValue(start, fields.ends[i], progress)
		if e != nil {
			return ManagedV2ScheduleView{}, e
		}
		out.Weights = append(out.Weights, Amount(weight.String()))
	}
	fee, e := managedScheduleValue(fields.feeStart, fields.feeEnd, managedScheduleProgress(out.FeeStart, out.FeeEnd, timestamp))
	if e != nil {
		return ManagedV2ScheduleView{}, e
	}
	out.SwapFee = Amount(fee.String())
	return out, nil
}

// Codec accepts absent schedule only for the older parent profile. The parent
// protocol validator requires field40 for the explicit scheduled adapter.
func ManagedV2ScheduleToProto(s *ManagedV2Schedule) (*pb.ManagedV2Schedule, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	word, e := s.PoolStateWord.Bytes()
	if e != nil {
		return nil, e
	}
	out := &pb.ManagedV2Schedule{PoolStateWord: &pb.Amount{Uint256Be: word}}
	for _, token := range s.Tokens {
		word, e := token.StateWord.Bytes()
		if e != nil {
			return nil, e
		}
		out.Tokens = append(out.Tokens, &pb.ManagedV2ScheduledToken{Token: token.Token, StateWord: &pb.Amount{Uint256Be: word}})
	}
	return out, nil
}
func ManagedV2ScheduleFromProto(s *pb.ManagedV2Schedule) (*ManagedV2Schedule, error) {
	if s == nil {
		return nil, nil
	}
	if s.PoolStateWord == nil {
		return nil, errors.New("MANAGED_V2_SCHEDULE_WORD_REQUIRED")
	}
	word, e := AmountFromBytes(s.PoolStateWord.Uint256Be)
	if e != nil {
		return nil, e
	}
	out := &ManagedV2Schedule{PoolStateWord: word, Tokens: []ManagedV2ScheduledToken{}}
	for _, token := range s.Tokens {
		if token == nil || token.StateWord == nil {
			return nil, errors.New("MANAGED_V2_SCHEDULE_TOKEN_WORD_REQUIRED")
		}
		word, e := AmountFromBytes(token.StateWord.Uint256Be)
		if e != nil {
			return nil, e
		}
		out.Tokens = append(out.Tokens, ManagedV2ScheduledToken{Token: token.Token, StateWord: word})
	}
	if e = out.Validate(); e != nil {
		return nil, e
	}
	return out, nil
}
