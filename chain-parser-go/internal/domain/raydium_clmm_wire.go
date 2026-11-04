package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

func RaydiumCLMMToProto(s *RaydiumCLMMState) (*pb.RaydiumCLMMState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.validateWire(); e != nil {
		return nil, e
	}
	raw := func(v string) []byte { b, _ := hex.DecodeString(v); return b }
	amount := func(a Amount) *pb.Amount { b, _ := a.Bytes(); return &pb.Amount{Uint256Be: b} }
	out := &pb.RaydiumCLMMState{SeedIndex: s.SeedIndex, CollectFeeOn: s.CollectFeeOn,
		ProgramId: s.ProgramID, Programdata: s.Programdata, ProgramCodeSha256: raw(s.ProgramCodeSHA256), DeployedSlot: s.DeployedSlot, TokenProgramCodeSha256: raw(s.TokenProgramCodeSHA256), AmmConfig: s.AmmConfig, PoolBump: s.PoolBump, Observation: s.Observation, Vault0: s.Vault0, Vault1: s.Vault1, RawVault0: s.RawVault0, RawVault1: s.RawVault1, Mint0Decimals: s.Mint0Decimals, Mint1Decimals: s.Mint1Decimals, BankSlot: s.BankSlot, BankUnixTimestamp: s.BankUnixTimestamp, BankEpoch: s.BankEpoch, TradeFeeRate: s.TradeFeeRate, ProtocolFeeRate: s.ProtocolFeeRate, FundFeeRate: s.FundFeeRate, TickSpacing: s.TickSpacing, SqrtPriceX64: amount(s.SqrtPriceX64), TickCurrent: s.TickCurrent, Liquidity: amount(s.Liquidity), FeeGrowthGlobal0: amount(s.FeeGrowthGlobal0), FeeGrowthGlobal1: amount(s.FeeGrowthGlobal1), ProtocolFees0: s.ProtocolFees0, ProtocolFees1: s.ProtocolFees1, FundFees0: s.FundFees0, FundFees1: s.FundFees1, OpenTime: s.OpenTime, RecentEpoch: s.RecentEpoch, Bitmap: append([]uint64{}, s.Bitmap...), MaxSteps: s.MaxSteps, ObservationInitialized: s.ObservationInitialized, ObservationIndex: s.ObservationIndex, ObservationRecentEpoch: s.ObservationRecentEpoch}
	for _, r := range s.RewardInfos {
		out.RewardInfos = append(out.RewardInfos, &pb.RaydiumCLMMRewardInfo{RewardState: r.RewardState, OpenTime: r.OpenTime, EndTime: r.EndTime, LastUpdateTime: r.LastUpdateTime, EmissionsPerSecondX64: amount(r.EmissionsPerSecondX64), RewardTotalEmitted: r.RewardTotalEmitted, RewardClaimed: r.RewardClaimed, TokenMint: r.TokenMint, TokenVault: r.TokenVault, Authority: r.Authority, RewardGrowthGlobalX64: amount(r.RewardGrowthGlobalX64)})
	}
	if s.BitmapExtension != nil {
		e := s.BitmapExtension
		out.BitmapExtension = &pb.RaydiumCLMMBitmapExtension{Address: e.Address, Positive: append([]uint64{}, e.Positive...), Negative: append([]uint64{}, e.Negative...)}
	}
	if d := s.DynamicFee; d != nil {
		out.DynamicFee = &pb.RaydiumCLMMDynamicFee{FilterPeriod: d.FilterPeriod, DecayPeriod: d.DecayPeriod, ReductionFactor: d.ReductionFactor, DynamicFeeControl: d.DynamicFeeControl, MaxVolatilityAccumulator: d.MaxVolatilityAccumulator, TickSpacingIndexReference: d.TickSpacingIndexReference, VolatilityReference: d.VolatilityReference, VolatilityAccumulator: d.VolatilityAccumulator, LastUpdateTimestamp: d.LastUpdateTimestamp}
	}
	for _, a := range s.TickArrays {
		outArray := &pb.RaydiumCLMMArray{Address: a.Address, StartTickIndex: a.StartTickIndex, RecentEpoch: a.RecentEpoch}
		for _, t := range a.Ticks {
			net, _ := ParseLiquidityNet(t.LiquidityNet)
			if net.Sign() < 0 {
				net.Add(net, new(big.Int).Lsh(big.NewInt(1), 128))
			}
			growth := make([]*pb.Amount, len(t.RewardGrowthsOutside))
			for i, v := range t.RewardGrowthsOutside {
				growth[i] = amount(v)
			}
			outArray.Ticks = append(outArray.Ticks, &pb.RaydiumCLMMTick{RewardGrowthsOutside: growth, Tick: t.Tick, LiquidityGross: amount(t.LiquidityGross), LiquidityNetI128Be: net.FillBytes(make([]byte, 16)), FeeGrowthOutside0: amount(t.FeeGrowthOutside0), FeeGrowthOutside1: amount(t.FeeGrowthOutside1)})
		}
		out.TickArrays = append(out.TickArrays, outArray)
	}
	for _, o := range s.Observations {
		out.Observations = append(out.Observations, &pb.RaydiumCLMMObservation{BlockTimestamp: o.BlockTimestamp, TickCumulative: o.TickCumulative})
	}
	return out, nil
}
func RaydiumCLMMFromProto(s *pb.RaydiumCLMMState) (*RaydiumCLMMState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.ProgramCodeSha256) != 32 || len(s.TokenProgramCodeSha256) != 32 {
		return nil, errors.New("RAYCLMM_DIGEST_WIDTH")
	}
	out := &RaydiumCLMMState{SeedIndex: s.SeedIndex, CollectFeeOn: s.CollectFeeOn,
		ProgramID: s.ProgramId, Programdata: s.Programdata, ProgramCodeSHA256: hex.EncodeToString(s.ProgramCodeSha256), DeployedSlot: s.DeployedSlot, TokenProgramCodeSHA256: hex.EncodeToString(s.TokenProgramCodeSha256), AmmConfig: s.AmmConfig, PoolBump: s.PoolBump, Observation: s.Observation, Vault0: s.Vault0, Vault1: s.Vault1, RawVault0: s.RawVault0, RawVault1: s.RawVault1, Mint0Decimals: s.Mint0Decimals, Mint1Decimals: s.Mint1Decimals, BankSlot: s.BankSlot, BankUnixTimestamp: s.BankUnixTimestamp, BankEpoch: s.BankEpoch, TradeFeeRate: s.TradeFeeRate, ProtocolFeeRate: s.ProtocolFeeRate, FundFeeRate: s.FundFeeRate, TickSpacing: s.TickSpacing, TickCurrent: s.TickCurrent, ProtocolFees0: s.ProtocolFees0, ProtocolFees1: s.ProtocolFees1, FundFees0: s.FundFees0, FundFees1: s.FundFees1, OpenTime: s.OpenTime, RecentEpoch: s.RecentEpoch, Bitmap: append([]uint64{}, s.Bitmap...), MaxSteps: s.MaxSteps, ObservationInitialized: s.ObservationInitialized, ObservationIndex: s.ObservationIndex, ObservationRecentEpoch: s.ObservationRecentEpoch}
	if s.BitmapExtension != nil {
		e := s.BitmapExtension
		out.BitmapExtension = &RaydiumCLMMBitmapExtension{Address: e.Address, Positive: append([]uint64{}, e.Positive...), Negative: append([]uint64{}, e.Negative...)}
	}
	if d := s.DynamicFee; d != nil {
		out.DynamicFee = &RaydiumCLMMDynamicFee{FilterPeriod: d.FilterPeriod, DecayPeriod: d.DecayPeriod, ReductionFactor: d.ReductionFactor, DynamicFeeControl: d.DynamicFeeControl, MaxVolatilityAccumulator: d.MaxVolatilityAccumulator, TickSpacingIndexReference: d.TickSpacingIndexReference, VolatilityReference: d.VolatilityReference, VolatilityAccumulator: d.VolatilityAccumulator, LastUpdateTimestamp: d.LastUpdateTimestamp}
	}
	amount := func(v *pb.Amount) (Amount, error) {
		if v == nil {
			return "", errors.New("RAYCLMM_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(v.Uint256Be)
	}
	var e error
	for _, r := range s.RewardInfos {
		if r == nil {
			return nil, errors.New("RAYCLMM_REWARD_REQUIRED")
		}
		emissions, e := amount(r.EmissionsPerSecondX64)
		if e != nil {
			return nil, e
		}
		growth, e := amount(r.RewardGrowthGlobalX64)
		if e != nil {
			return nil, e
		}
		out.RewardInfos = append(out.RewardInfos, RaydiumCLMMRewardInfo{RewardState: r.RewardState, OpenTime: r.OpenTime, EndTime: r.EndTime, LastUpdateTime: r.LastUpdateTime, EmissionsPerSecondX64: emissions, RewardTotalEmitted: r.RewardTotalEmitted, RewardClaimed: r.RewardClaimed, TokenMint: r.TokenMint, TokenVault: r.TokenVault, Authority: r.Authority, RewardGrowthGlobalX64: growth})
	}

	out.SqrtPriceX64, e = amount(s.SqrtPriceX64)
	if e != nil {
		return nil, e
	}
	out.Liquidity, e = amount(s.Liquidity)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthGlobal0, e = amount(s.FeeGrowthGlobal0)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthGlobal1, e = amount(s.FeeGrowthGlobal1)
	if e != nil {
		return nil, e
	}
	out.TickArrays = []RaydiumCLMMArray{}
	out.Observations = []RaydiumCLMMObservation{}
	for _, a := range s.TickArrays {
		if a == nil {
			return nil, errors.New("RAYCLMM_ARRAY_REQUIRED")
		}
		array := RaydiumCLMMArray{Address: a.Address, StartTickIndex: a.StartTickIndex, RecentEpoch: a.RecentEpoch, Ticks: []RaydiumCLMMTick{}}
		for _, t := range a.Ticks {
			if t == nil || len(t.LiquidityNetI128Be) != 16 {
				return nil, errors.New("RAYCLMM_NET_WIDTH")
			}
			gross, e := amount(t.LiquidityGross)
			if e != nil {
				return nil, e
			}
			f0, e := amount(t.FeeGrowthOutside0)
			if e != nil {
				return nil, e
			}
			f1, e := amount(t.FeeGrowthOutside1)
			if e != nil {
				return nil, e
			}
			net := new(big.Int).SetBytes(t.LiquidityNetI128Be)
			if t.LiquidityNetI128Be[0]&128 != 0 {
				net.Sub(net, new(big.Int).Lsh(big.NewInt(1), 128))
			}
			growth := make([]Amount, len(t.RewardGrowthsOutside))
			for i, v := range t.RewardGrowthsOutside {
				growth[i], e = amount(v)
				if e != nil {
					return nil, e
				}
			}
			array.Ticks = append(array.Ticks, RaydiumCLMMTick{RewardGrowthsOutside: growth, Tick: t.Tick, LiquidityGross: gross, LiquidityNet: net.String(), FeeGrowthOutside0: f0, FeeGrowthOutside1: f1})
		}
		out.TickArrays = append(out.TickArrays, array)
	}
	for _, o := range s.Observations {
		if o == nil {
			return nil, errors.New("RAYCLMM_OBSERVATION_REQUIRED")
		}
		out.Observations = append(out.Observations, RaydiumCLMMObservation{BlockTimestamp: o.BlockTimestamp, TickCumulative: o.TickCumulative})
	}
	if e = out.validateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
