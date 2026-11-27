package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

const ComposableV6Protocol = "balancer-v2-composable-v6"
const ComposableV6Adapter = ComposableV6Protocol + "/no-rates-1"
const ComposableV6CachedRatesAdapter = ComposableV6Protocol + "/cached-rates-1"

type BalancerComposableV6RateCache struct {
	TokenIndex          uint32 `json:"token_index"`
	Provider            string `json:"provider"`
	ProviderCodeSHA256  string `json:"provider_code_sha256"`
	Current             Amount `json:"current"`
	Old                 Amount `json:"old"`
	Duration            uint32 `json:"duration"`
	Expires             uint32 `json:"expires"`
	ObservedRefreshRate Amount `json:"observed_refresh_rate,omitempty"`
}

type BalancerComposableV6State struct {
	TotalSupply           Amount                          `json:"total_supply"`
	LastJoinExitAmp       Amount                          `json:"last_join_exit_amp"`
	LastJoinExitInvariant Amount                          `json:"last_join_exit_invariant"`
	ProtocolSwapFee       Amount                          `json:"protocol_swap_fee"`
	ProtocolYieldFee      Amount                          `json:"protocol_yield_fee"`
	FeeCollector          string                          `json:"fee_collector"`
	CollectorBPT          Amount                          `json:"collector_bpt"`
	YieldExempt           bool                            `json:"yield_exempt"`
	RateCaches            []BalancerComposableV6RateCache `json:"rate_caches,omitempty"`
	RateSnapshotTimestamp *uint64                         `json:"rate_snapshot_timestamp,omitempty"`
}

func (s *BalancerComposableV6State) validRates(assets []MultiAsset, pool, adapter string) bool {
	if adapter == ComposableV6Adapter {
		return len(s.RateCaches) == 0 && s.RateSnapshotTimestamp == nil
	}
	if adapter != ComposableV6CachedRatesAdapter || len(s.RateCaches) == 0 || s.RateSnapshotTimestamp == nil || *s.RateSnapshotTimestamp == 0 {
		return false
	}
	last := -1
	for _, rate := range s.RateCaches {
		i := int(rate.TokenIndex)
		if i <= last || i >= len(assets) || assets[i].Token == pool || ValidateEVMIdentity(rate.Provider, rate.ProviderCodeSHA256) != nil || rate.Provider == pool {
			return false
		}
		stamp := *s.RateSnapshotTimestamp
		if stamp > uint64(rate.Expires) {
			obs, err := ParseAmount(string(rate.ObservedRefreshRate))
			if err != nil || obs.Big().Sign() == 0 || obs.Big().BitLen() > 96 || stamp > uint64(^uint32(0)-rate.Duration) {
				return false
			}
		} else if rate.ObservedRefreshRate != "" {
			return false
		}
		current, err := ParseAmount(string(rate.Current))
		old, oldErr := ParseAmount(string(rate.Old))
		if err != nil || oldErr != nil || current.Big().Sign() == 0 || old.Big().Sign() == 0 || current.Big().BitLen() > 96 || old.Big().BitLen() > 96 {
			return false
		}
		var rem big.Int
		factor, _ := new(big.Int).QuoRem(assets[i].Scaling.Big(), current.Big(), &rem)
		if rem.Sign() != 0 || !power10Between(Amount(factor.String()), 0, 18) {
			return false
		}
		last = i
	}
	return true
}

func (s *BalancerComposableV6State) hasRate(i int) bool {
	for _, rate := range s.RateCaches {
		if int(rate.TokenIndex) == i {
			return true
		}
	}
	return false
}

func (s *BalancerComposableV6State) amountFields() []*Amount {
	return []*Amount{&s.TotalSupply, &s.LastJoinExitAmp, &s.LastJoinExitInvariant, &s.ProtocolSwapFee, &s.ProtocolYieldFee, &s.CollectorBPT}
}

func (s *BalancerComposableV6State) valid(vaultBPT Amount) bool {
	if s == nil || ValidateEVMAddress(s.FeeCollector) != nil {
		return false
	}
	for _, field := range s.amountFields() {
		if _, err := ParseAmount(string(*field)); err != nil {
			return false
		}
	}
	amp := s.LastJoinExitAmp.Big()
	one := big.NewInt(1000000000000000000)
	available := new(big.Int).Sub(s.TotalSupply.Big(), vaultBPT.Big())
	return amp.Cmp(big.NewInt(1000)) >= 0 && amp.Cmp(big.NewInt(5000000)) <= 0 &&
		s.LastJoinExitInvariant.Big().Sign() > 0 && s.LastJoinExitInvariant.Big().BitLen() <= 233 &&
		s.ProtocolSwapFee.Big().Cmp(one) <= 0 && s.ProtocolYieldFee.Big().Cmp(one) <= 0 &&
		available.Sign() > 0 && available.Cmp(s.CollectorBPT.Big()) >= 0
}

func composableV6ToProto(s *BalancerComposableV6State) (*pb.BalancerComposableV6State, error) {
	if s == nil {
		return nil, nil
	}
	fields := s.amountFields()
	values := make([]*pb.Amount, len(fields))
	for i, field := range fields {
		encoded, err := field.Bytes()
		if err != nil {
			return nil, err
		}
		values[i] = &pb.Amount{Uint256Be: encoded}
	}
	exempt := s.YieldExempt
	out := &pb.BalancerComposableV6State{
		TotalSupply: values[0], LastJoinExitAmp: values[1], LastJoinExitInvariant: values[2],
		ProtocolSwapFee: values[3], ProtocolYieldFee: values[4], FeeCollector: s.FeeCollector,
		CollectorBpt: values[5], YieldExempt: &exempt,
	}
	if s.RateSnapshotTimestamp != nil {
		stamp := *s.RateSnapshotTimestamp
		out.RateSnapshotTimestamp = &stamp
	}
	for _, rate := range s.RateCaches {
		digest, err := hex.DecodeString(rate.ProviderCodeSHA256)
		if err != nil || len(digest) != 32 {
			return nil, errors.New("COMPOSABLE_V6_RATE_PROVIDER_IDENTITY")
		}
		current, err := rate.Current.Bytes()
		if err != nil {
			return nil, err
		}
		old, err := rate.Old.Bytes()
		if err != nil {
			return nil, err
		}
		duration, expires := rate.Duration, rate.Expires
		wire := &pb.BalancerComposableV6RateCache{TokenIndex: rate.TokenIndex, Provider: rate.Provider, ProviderCodeSha256: digest,
			Current: &pb.Amount{Uint256Be: current}, Old: &pb.Amount{Uint256Be: old}, Duration: &duration, Expires: &expires}
		if rate.ObservedRefreshRate != "" {
			value, err := rate.ObservedRefreshRate.Bytes()
			if err != nil {
				return nil, err
			}
			wire.ObservedRefreshRate = &pb.Amount{Uint256Be: value}
		}
		out.RateCaches = append(out.RateCaches, wire)
	}
	return out, nil
}

func composableV6FromProto(p *pb.BalancerComposableV6State) (*BalancerComposableV6State, error) {
	if p == nil {
		return nil, nil
	}
	if p.YieldExempt == nil {
		return nil, errors.New("COMPOSABLE_V6_EXEMPTION_REQUIRED")
	}
	s := &BalancerComposableV6State{FeeCollector: p.FeeCollector, YieldExempt: *p.YieldExempt}
	fields := s.amountFields()
	for i, value := range []*pb.Amount{p.TotalSupply, p.LastJoinExitAmp, p.LastJoinExitInvariant, p.ProtocolSwapFee, p.ProtocolYieldFee, p.CollectorBpt} {
		if value == nil {
			return nil, errors.New("COMPOSABLE_V6_AMOUNT_REQUIRED")
		}
		decoded, err := AmountFromBytes(value.Uint256Be)
		if err != nil {
			return nil, err
		}
		*fields[i] = decoded
	}
	if p.RateSnapshotTimestamp != nil {
		stamp := *p.RateSnapshotTimestamp
		s.RateSnapshotTimestamp = &stamp
	}
	for _, rate := range p.RateCaches {
		if rate == nil || rate.Current == nil || rate.Old == nil || rate.Duration == nil || rate.Expires == nil || len(rate.ProviderCodeSha256) != 32 {
			return nil, errors.New("COMPOSABLE_V6_RATE_CACHE_REQUIRED")
		}
		current, err := AmountFromBytes(rate.Current.Uint256Be)
		if err != nil {
			return nil, err
		}
		old, err := AmountFromBytes(rate.Old.Uint256Be)
		if err != nil {
			return nil, err
		}
		cache := BalancerComposableV6RateCache{TokenIndex: rate.TokenIndex, Provider: rate.Provider, ProviderCodeSHA256: hex.EncodeToString(rate.ProviderCodeSha256),
			Current: current, Old: old, Duration: *rate.Duration, Expires: *rate.Expires}
		if rate.ObservedRefreshRate != nil {
			cache.ObservedRefreshRate, err = AmountFromBytes(rate.ObservedRefreshRate.Uint256Be)
			if err != nil {
				return nil, err
			}
		}
		s.RateCaches = append(s.RateCaches, cache)
	}
	return s, nil
}
