package domain

import (
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

const ComposableV6Protocol = "balancer-v2-composable-v6"
const ComposableV6Adapter = ComposableV6Protocol + "/no-rates-1"

type BalancerComposableV6State struct {
	TotalSupply           Amount `json:"total_supply"`
	LastJoinExitAmp       Amount `json:"last_join_exit_amp"`
	LastJoinExitInvariant Amount `json:"last_join_exit_invariant"`
	ProtocolSwapFee       Amount `json:"protocol_swap_fee"`
	ProtocolYieldFee      Amount `json:"protocol_yield_fee"`
	FeeCollector          string `json:"fee_collector"`
	CollectorBPT          Amount `json:"collector_bpt"`
	YieldExempt           bool   `json:"yield_exempt"`
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
	return &pb.BalancerComposableV6State{
		TotalSupply: values[0], LastJoinExitAmp: values[1], LastJoinExitInvariant: values[2],
		ProtocolSwapFee: values[3], ProtocolYieldFee: values[4], FeeCollector: s.FeeCollector,
		CollectorBpt: values[5], YieldExempt: &exempt,
	}, nil
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
	return s, nil
}
