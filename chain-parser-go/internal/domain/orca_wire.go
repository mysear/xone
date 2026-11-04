package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

func OrcaWhirlpoolToProto(s *OrcaWhirlpoolState) (*pb.OrcaWhirlpoolState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.validateWire(); e != nil {
		return nil, e
	}
	raw := func(v string) []byte { b, _ := hex.DecodeString(v); return b }
	amount := func(a Amount) *pb.Amount { b, _ := a.Bytes(); return &pb.Amount{Uint256Be: b} }
	out := &pb.OrcaWhirlpoolState{
		ProgramId: s.ProgramID, Programdata: s.Programdata, ProgramCodeSha256: raw(s.ProgramCodeSHA256), DeployedSlot: s.DeployedSlot, TokenProgramCodeSha256: raw(s.TokenProgramCodeSHA256), WhirlpoolsConfig: s.WhirlpoolsConfig, WhirlpoolBump: s.WhirlpoolBump, Oracle: s.Oracle, Vault0: s.Vault0, Vault1: s.Vault1, RawVault0: s.RawVault0, RawVault1: s.RawVault1, Mint0Decimals: s.Mint0Decimals, Mint1Decimals: s.Mint1Decimals, BankSlot: s.BankSlot, BankUnixTimestamp: s.BankUnixTimestamp, BankEpoch: s.BankEpoch, FeeTierIndex: s.FeeTierIndex, FeeRate: s.FeeRate, ProtocolFeeRate: s.ProtocolFeeRate, SqrtPriceX64: amount(s.SqrtPriceX64), TickCurrentIndex: s.TickCurrentIndex, Liquidity: amount(s.Liquidity), TickSpacing: s.TickSpacing, ProtocolFeeOwed0: s.ProtocolFeeOwed0, ProtocolFeeOwed1: s.ProtocolFeeOwed1, FeeGrowthGlobal0: amount(s.FeeGrowthGlobal0), FeeGrowthGlobal1: amount(s.FeeGrowthGlobal1), RewardLastUpdatedTimestamp: s.RewardLastUpdatedTimestamp, MaxSteps: s.MaxSteps}
	out.Adaptive = orcaAdaptiveToProto(s.Adaptive)
	for _, r := range s.RewardInfos {
		extension := make([]byte, len(r.Extension))
		for i, v := range r.Extension {
			extension[i] = byte(v)
		}
		out.RewardInfos = append(out.RewardInfos, &pb.OrcaRewardInfo{Mint: r.Mint, Vault: r.Vault, Extension: extension, EmissionsPerSecondX64: amount(r.EmissionsPerSecondX64), GrowthGlobalX64: amount(r.GrowthGlobalX64)})
	}
	for _, a := range s.TickArrays {
		array := &pb.OrcaTickArray{Address: a.Address, StartTickIndex: a.StartTickIndex, Layout: a.Layout, InitializedBitmap: amount(a.InitializedBitmap)}
		for _, t := range a.Ticks {
			net, _ := ParseLiquidityNet(t.LiquidityNet)
			if net.Sign() < 0 {
				net.Add(net, new(big.Int).Lsh(big.NewInt(1), 128))
			}
			growth := make([]*pb.Amount, len(t.RewardGrowthsOutside))
			for i, v := range t.RewardGrowthsOutside {
				growth[i] = amount(v)
			}
			array.Ticks = append(array.Ticks, &pb.OrcaTick{RewardGrowthsOutside: growth, Tick: t.Tick, LiquidityGross: amount(t.LiquidityGross), LiquidityNetI128Be: net.FillBytes(make([]byte, 16)), FeeGrowthOutside0: amount(t.FeeGrowthOutside0), FeeGrowthOutside1: amount(t.FeeGrowthOutside1)})
		}
		out.TickArrays = append(out.TickArrays, array)
	}
	return out, nil
}
func OrcaWhirlpoolFromProto(s *pb.OrcaWhirlpoolState) (*OrcaWhirlpoolState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.ProgramCodeSha256) != 32 || len(s.TokenProgramCodeSha256) != 32 {
		return nil, errors.New("ORCA_DIGEST_WIDTH")
	}
	out := &OrcaWhirlpoolState{
		ProgramID: s.ProgramId, Programdata: s.Programdata, ProgramCodeSHA256: hex.EncodeToString(s.ProgramCodeSha256), DeployedSlot: s.DeployedSlot, TokenProgramCodeSHA256: hex.EncodeToString(s.TokenProgramCodeSha256), WhirlpoolsConfig: s.WhirlpoolsConfig, WhirlpoolBump: s.WhirlpoolBump, Oracle: s.Oracle, Vault0: s.Vault0, Vault1: s.Vault1, RawVault0: s.RawVault0, RawVault1: s.RawVault1, Mint0Decimals: s.Mint0Decimals, Mint1Decimals: s.Mint1Decimals, BankSlot: s.BankSlot, BankUnixTimestamp: s.BankUnixTimestamp, BankEpoch: s.BankEpoch, FeeTierIndex: s.FeeTierIndex, FeeRate: s.FeeRate, ProtocolFeeRate: s.ProtocolFeeRate, TickCurrentIndex: s.TickCurrentIndex, TickSpacing: s.TickSpacing, ProtocolFeeOwed0: s.ProtocolFeeOwed0, ProtocolFeeOwed1: s.ProtocolFeeOwed1, RewardLastUpdatedTimestamp: s.RewardLastUpdatedTimestamp, MaxSteps: s.MaxSteps}
	out.Adaptive = orcaAdaptiveFromProto(s.Adaptive)
	amount := func(v *pb.Amount) (Amount, error) {
		if v == nil {
			return "", errors.New("ORCA_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(v.Uint256Be)
	}
	var e error
	for _, r := range s.RewardInfos {
		if r == nil {
			return nil, errors.New("ORCA_REWARD_REQUIRED")
		}
		emissions, e := amount(r.EmissionsPerSecondX64)
		if e != nil {
			return nil, e
		}
		growth, e := amount(r.GrowthGlobalX64)
		if e != nil {
			return nil, e
		}
		extension := make([]uint32, len(r.Extension))
		for i, v := range r.Extension {
			extension[i] = uint32(v)
		}
		out.RewardInfos = append(out.RewardInfos, OrcaRewardInfo{Mint: r.Mint, Vault: r.Vault, Extension: extension, EmissionsPerSecondX64: emissions, GrowthGlobalX64: growth})
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
	out.TickArrays = []OrcaTickArray{}
	for _, a := range s.TickArrays {
		if a == nil {
			return nil, errors.New("ORCA_ARRAY_REQUIRED")
		}
		bitmap, e := amount(a.InitializedBitmap)
		if e != nil {
			return nil, e
		}
		array := OrcaTickArray{Address: a.Address, StartTickIndex: a.StartTickIndex, Layout: a.Layout, InitializedBitmap: bitmap, Ticks: []OrcaTick{}}
		for _, t := range a.Ticks {
			if t == nil || len(t.LiquidityNetI128Be) != 16 {
				return nil, errors.New("ORCA_NET_WIDTH")
			}
			gross, e := amount(t.LiquidityGross)
			if e != nil {
				return nil, e
			}
			fee0, e := amount(t.FeeGrowthOutside0)
			if e != nil {
				return nil, e
			}
			fee1, e := amount(t.FeeGrowthOutside1)
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
			array.Ticks = append(array.Ticks, OrcaTick{RewardGrowthsOutside: growth, Tick: t.Tick, LiquidityGross: gross, LiquidityNet: net.String(), FeeGrowthOutside0: fee0, FeeGrowthOutside1: fee1})
		}
		out.TickArrays = append(out.TickArrays, array)
	}
	if e = out.validateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
