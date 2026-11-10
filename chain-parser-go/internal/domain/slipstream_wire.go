package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func slipPtr[T any](v T) *T { return &v }
func slipWireDigest(value string) []byte {
	raw, _ := hex.DecodeString(strings.TrimPrefix(value, "0x"))
	return raw
}
func slipWireAmount(value Amount) *pb.Amount {
	raw, _ := value.Bytes()
	return &pb.Amount{Uint256Be: raw}
}
func slipDecodeDigest(value []byte, block bool) (string, error) {
	if len(value) != 0 && len(value) != 32 {
		return "", errors.New("SLIPSTREAM_WIRE_DIGEST_WIDTH")
	}
	s := hex.EncodeToString(value)
	if block {
		s = "0x" + s
	}
	return s, nil
}
func slipDecodeAmount(value *pb.Amount) (Amount, error) {
	if value == nil {
		return "", errors.New("SLIPSTREAM_WIRE_AMOUNT_REQUIRED")
	}
	return AmountFromBytes(value.Uint256Be)
}
func slipWireSigned(value string, bits uint) ([]byte, error) {
	n, ok := new(big.Int).SetString(value, 10)
	if !ok || n.String() != value {
		return nil, errors.New("SLIPSTREAM_SIGNED_CANONICAL")
	}
	limit := new(big.Int).Lsh(big.NewInt(1), bits-1)
	negative := new(big.Int).Neg(new(big.Int).Set(limit))
	if n.Cmp(negative) < 0 || n.Cmp(limit) >= 0 {
		return nil, errors.New("SLIPSTREAM_SIGNED_RANGE")
	}
	if n.Sign() < 0 {
		n.Add(n, new(big.Int).Lsh(big.NewInt(1), bits))
	}
	return n.FillBytes(make([]byte, bits/8)), nil
}
func slipDecodeSigned(raw []byte, bits uint) (string, error) {
	if len(raw) != int(bits/8) {
		return "", errors.New("SLIPSTREAM_WIRE_SIGNED_WIDTH")
	}
	n := new(big.Int).SetBytes(raw)
	if raw[0]&128 != 0 {
		n.Sub(n, new(big.Int).Lsh(big.NewInt(1), bits))
	}
	return n.String(), nil
}

func SlipstreamDynamicFeeToProto(s *SlipstreamDynamicFee) (*pb.SlipstreamDynamicFee, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.SlipstreamDynamicFee{}
	var e error
	_ = e
	out.BaseFee = slipPtr(s.BaseFee)
	out.FeeCap = slipPtr(s.FeeCap)
	out.ScalingFactor = slipPtr(s.ScalingFactor)
	out.InitialFeeEnabled = slipPtr(s.InitialFeeEnabled)
	out.InitialFee = slipPtr(s.InitialFee)
	out.DefaultScalingFactor = slipPtr(s.DefaultScalingFactor)
	out.DefaultFeeCap = slipPtr(s.DefaultFeeCap)
	out.SecondsAgo = slipPtr(s.SecondsAgo)
	return out, nil
}
func SlipstreamDynamicFeeFromProto(s *pb.SlipstreamDynamicFee) (*SlipstreamDynamicFee, error) {
	if s == nil {
		return nil, nil
	}
	out := &SlipstreamDynamicFee{}
	var e error
	_ = e
	if s.BaseFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_BASE_FEE_REQUIRED")
	}
	out.BaseFee = *s.BaseFee
	if s.FeeCap == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_FEE_CAP_REQUIRED")
	}
	out.FeeCap = *s.FeeCap
	if s.ScalingFactor == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_SCALING_FACTOR_REQUIRED")
	}
	out.ScalingFactor = *s.ScalingFactor
	if s.InitialFeeEnabled == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_INITIAL_FEE_ENABLED_REQUIRED")
	}
	out.InitialFeeEnabled = *s.InitialFeeEnabled
	if s.InitialFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_INITIAL_FEE_REQUIRED")
	}
	out.InitialFee = *s.InitialFee
	if s.DefaultScalingFactor == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_DEFAULT_SCALING_FACTOR_REQUIRED")
	}
	out.DefaultScalingFactor = *s.DefaultScalingFactor
	if s.DefaultFeeCap == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_DEFAULT_FEE_CAP_REQUIRED")
	}
	out.DefaultFeeCap = *s.DefaultFeeCap
	if s.SecondsAgo == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_SECONDS_AGO_REQUIRED")
	}
	out.SecondsAgo = *s.SecondsAgo
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func SlipstreamModuleToProto(s *SlipstreamModule) (*pb.SlipstreamModule, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.SlipstreamModule{}
	var e error
	_ = e
	out.Kind = s.Kind
	out.Address = s.Address
	out.CodeSha256 = slipWireDigest(s.CodeSHA256)
	out.TemplateSha256 = slipWireDigest(s.TemplateSHA256)
	out.Factory = s.Factory
	out.CustomFee = slipPtr(s.CustomFee)
	out.Dynamic, e = SlipstreamDynamicFeeToProto(s.Dynamic)
	if e != nil {
		return nil, e
	}
	return out, nil
}
func SlipstreamModuleFromProto(s *pb.SlipstreamModule) (*SlipstreamModule, error) {
	if s == nil {
		return nil, nil
	}
	out := &SlipstreamModule{}
	var e error
	_ = e
	out.Kind = s.Kind
	out.Address = s.Address
	out.CodeSHA256, e = slipDecodeDigest(s.CodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.TemplateSHA256, e = slipDecodeDigest(s.TemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.Factory = s.Factory
	if s.CustomFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_CUSTOM_FEE_REQUIRED")
	}
	out.CustomFee = *s.CustomFee
	out.Dynamic, e = SlipstreamDynamicFeeFromProto(s.Dynamic)
	if e != nil {
		return nil, e
	}
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func SlipstreamOriginToProto(s *SlipstreamOrigin) (*pb.SlipstreamOrigin, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.SlipstreamOrigin{}
	var e error
	_ = e
	out.Origin = s.Origin
	out.Discount = slipPtr(s.Discount)
	out.ViewSwapFee = slipPtr(s.ViewSwapFee)
	out.ViewUnstakedFee = slipPtr(s.ViewUnstakedFee)
	return out, nil
}
func SlipstreamOriginFromProto(s *pb.SlipstreamOrigin) (*SlipstreamOrigin, error) {
	if s == nil {
		return nil, nil
	}
	out := &SlipstreamOrigin{}
	var e error
	_ = e
	out.Origin = s.Origin
	if s.Discount == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_DISCOUNT_REQUIRED")
	}
	out.Discount = *s.Discount
	if s.ViewSwapFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_VIEW_SWAP_FEE_REQUIRED")
	}
	out.ViewSwapFee = *s.ViewSwapFee
	if s.ViewUnstakedFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_VIEW_UNSTAKED_FEE_REQUIRED")
	}
	out.ViewUnstakedFee = *s.ViewUnstakedFee
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func SlipstreamTickToProto(s *SlipstreamTick) (*pb.SlipstreamTick, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.SlipstreamTick{}
	var e error
	_ = e
	out.Tick = slipPtr(s.Tick)
	out.LiquidityGross = slipWireAmount(s.LiquidityGross)
	out.LiquidityNet, e = slipWireSigned(s.LiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.StakedLiquidityNet, e = slipWireSigned(s.StakedLiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthOutside0 = slipWireAmount(s.FeeGrowthOutside0)
	out.FeeGrowthOutside1 = slipWireAmount(s.FeeGrowthOutside1)
	out.RewardGrowthOutside = slipWireAmount(s.RewardGrowthOutside)
	out.TickCumulativeOutside, e = slipWireSigned(s.TickCumulativeOutside, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityOutsideX128 = slipWireAmount(s.SecondsPerLiquidityOutsideX128)
	out.SecondsOutside = slipPtr(s.SecondsOutside)
	out.Initialized = slipPtr(s.Initialized)
	return out, nil
}
func SlipstreamTickFromProto(s *pb.SlipstreamTick) (*SlipstreamTick, error) {
	if s == nil {
		return nil, nil
	}
	out := &SlipstreamTick{}
	var e error
	_ = e
	if s.Tick == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_TICK_REQUIRED")
	}
	out.Tick = *s.Tick
	out.LiquidityGross, e = slipDecodeAmount(s.LiquidityGross)
	if e != nil {
		return nil, e
	}
	out.LiquidityNet, e = slipDecodeSigned(s.LiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.StakedLiquidityNet, e = slipDecodeSigned(s.StakedLiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthOutside0, e = slipDecodeAmount(s.FeeGrowthOutside0)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthOutside1, e = slipDecodeAmount(s.FeeGrowthOutside1)
	if e != nil {
		return nil, e
	}
	out.RewardGrowthOutside, e = slipDecodeAmount(s.RewardGrowthOutside)
	if e != nil {
		return nil, e
	}
	out.TickCumulativeOutside, e = slipDecodeSigned(s.TickCumulativeOutside, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityOutsideX128, e = slipDecodeAmount(s.SecondsPerLiquidityOutsideX128)
	if e != nil {
		return nil, e
	}
	if s.SecondsOutside == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_SECONDS_OUTSIDE_REQUIRED")
	}
	out.SecondsOutside = *s.SecondsOutside
	if s.Initialized == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_INITIALIZED_REQUIRED")
	}
	out.Initialized = *s.Initialized
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func SlipstreamObservationToProto(s *SlipstreamObservation) (*pb.SlipstreamObservation, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.SlipstreamObservation{}
	var e error
	_ = e
	out.BlockTimestamp = slipPtr(s.BlockTimestamp)
	out.TickCumulative, e = slipWireSigned(s.TickCumulative, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityCumulativeX128 = slipWireAmount(s.SecondsPerLiquidityCumulativeX128)
	out.Initialized = slipPtr(s.Initialized)
	return out, nil
}
func SlipstreamObservationFromProto(s *pb.SlipstreamObservation) (*SlipstreamObservation, error) {
	if s == nil {
		return nil, nil
	}
	out := &SlipstreamObservation{}
	var e error
	_ = e
	if s.BlockTimestamp == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_BLOCK_TIMESTAMP_REQUIRED")
	}
	out.BlockTimestamp = *s.BlockTimestamp
	out.TickCumulative, e = slipDecodeSigned(s.TickCumulative, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityCumulativeX128, e = slipDecodeAmount(s.SecondsPerLiquidityCumulativeX128)
	if e != nil {
		return nil, e
	}
	if s.Initialized == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_INITIALIZED_REQUIRED")
	}
	out.Initialized = *s.Initialized
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func SlipstreamStateToProto(s *SlipstreamState) (*pb.SlipstreamState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.SlipstreamState{}
	var e error
	_ = e
	out.Core, e = V3ToProto(s.Core)
	if e != nil {
		return nil, e
	}
	out.ImplementationTemplateSha256 = slipWireDigest(s.ImplementationTemplateSHA256)
	out.FactoryTemplateSha256 = slipWireDigest(s.FactoryTemplateSHA256)
	out.Token0CodeSha256 = slipWireDigest(s.Token0CodeSHA256)
	out.Token1CodeSha256 = slipWireDigest(s.Token1CodeSHA256)
	out.FactoryVoter = s.FactoryVoter
	out.VoterCodeSha256 = slipWireDigest(s.VoterCodeSHA256)
	out.VoterGauge = s.VoterGauge
	out.VoterGaugeAlive = slipPtr(s.VoterGaugeAlive)
	out.FactoryDefaultFee = slipPtr(s.FactoryDefaultFee)
	out.FactoryDefaultUnstakedFee = slipPtr(s.FactoryDefaultUnstakedFee)
	out.FactoryRegistered = slipPtr(s.FactoryRegistered)
	out.SwapFeeModule, e = SlipstreamModuleToProto(&s.SwapFeeModule)
	if e != nil {
		return nil, e
	}
	out.UnstakedFeeModule, e = SlipstreamModuleToProto(&s.UnstakedFeeModule)
	if e != nil {
		return nil, e
	}
	for i := range s.Origins {
		value, e := SlipstreamOriginToProto(&s.Origins[i])
		if e != nil {
			return nil, e
		}
		out.Origins = append(out.Origins, value)
	}
	out.ViewOrigin = s.ViewOrigin
	out.ViewSwapFee = slipPtr(s.ViewSwapFee)
	out.ViewUnstakedFee = slipPtr(s.ViewUnstakedFee)
	out.RawBalance0 = slipWireAmount(s.RawBalance0)
	out.RawBalance1 = slipWireAmount(s.RawBalance1)
	out.FeeGrowthGlobal0 = slipWireAmount(s.FeeGrowthGlobal0)
	out.FeeGrowthGlobal1 = slipWireAmount(s.FeeGrowthGlobal1)
	out.RewardGrowthGlobal = slipWireAmount(s.RewardGrowthGlobal)
	out.GaugeFees0 = slipWireAmount(s.GaugeFees0)
	out.GaugeFees1 = slipWireAmount(s.GaugeFees1)
	out.RewardRate = slipWireAmount(s.RewardRate)
	out.RewardReserve = slipWireAmount(s.RewardReserve)
	out.PeriodFinish = slipWireAmount(s.PeriodFinish)
	out.Rollover = slipWireAmount(s.Rollover)
	out.StakedLiquidity = slipWireAmount(s.StakedLiquidity)
	out.LastUpdated = slipPtr(s.LastUpdated)
	out.MaxLiquidityPerTick = slipWireAmount(s.MaxLiquidityPerTick)
	out.Gauge = s.Gauge
	out.Nft = s.NFT
	out.FactoryRegistry = s.FactoryRegistry
	out.ObservationIndex = slipPtr(s.ObservationIndex)
	out.ObservationCardinality = slipPtr(s.ObservationCardinality)
	out.ObservationCardinalityNext = slipPtr(s.ObservationCardinalityNext)
	out.Unlocked = slipPtr(s.Unlocked)
	for i := range s.Ticks {
		value, e := SlipstreamTickToProto(&s.Ticks[i])
		if e != nil {
			return nil, e
		}
		out.Ticks = append(out.Ticks, value)
	}
	for i := range s.Observations {
		value, e := SlipstreamObservationToProto(&s.Observations[i])
		if e != nil {
			return nil, e
		}
		out.Observations = append(out.Observations, value)
	}
	out.BlockHash = slipWireDigest(s.BlockHash)
	out.BlockNumber = slipPtr(s.BlockNumber)
	out.Timestamp = slipPtr(s.Timestamp)
	out.MaxObservations = slipPtr(s.MaxObservations)
	out.Derived = slipPtr(s.Derived)
	return out, nil
}
func SlipstreamStateFromProto(s *pb.SlipstreamState) (*SlipstreamState, error) {
	if s == nil {
		return nil, nil
	}
	out := &SlipstreamState{}
	var e error
	_ = e
	if s.Core == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_CORE_REQUIRED")
	}
	out.Core, e = V3FromProto(s.Core)
	if e != nil {
		return nil, e
	}
	out.ImplementationTemplateSHA256, e = slipDecodeDigest(s.ImplementationTemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.FactoryTemplateSHA256, e = slipDecodeDigest(s.FactoryTemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.Token0CodeSHA256, e = slipDecodeDigest(s.Token0CodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.Token1CodeSHA256, e = slipDecodeDigest(s.Token1CodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.FactoryVoter = s.FactoryVoter
	out.VoterCodeSHA256, e = slipDecodeDigest(s.VoterCodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.VoterGauge = s.VoterGauge
	if s.VoterGaugeAlive == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_VOTER_GAUGE_ALIVE_REQUIRED")
	}
	out.VoterGaugeAlive = *s.VoterGaugeAlive
	if s.FactoryDefaultFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_FACTORY_DEFAULT_FEE_REQUIRED")
	}
	out.FactoryDefaultFee = *s.FactoryDefaultFee
	if s.FactoryDefaultUnstakedFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_FACTORY_DEFAULT_UNSTAKED_FEE_REQUIRED")
	}
	out.FactoryDefaultUnstakedFee = *s.FactoryDefaultUnstakedFee
	if s.FactoryRegistered == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_FACTORY_REGISTERED_REQUIRED")
	}
	out.FactoryRegistered = *s.FactoryRegistered
	if s.SwapFeeModule == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_SWAP_FEE_MODULE_REQUIRED")
	}
	valueSwapFeeModule, errSwapFeeModule := SlipstreamModuleFromProto(s.SwapFeeModule)
	if errSwapFeeModule != nil {
		return nil, errSwapFeeModule
	}
	out.SwapFeeModule = *valueSwapFeeModule
	if s.UnstakedFeeModule == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_UNSTAKED_FEE_MODULE_REQUIRED")
	}
	valueUnstakedFeeModule, errUnstakedFeeModule := SlipstreamModuleFromProto(s.UnstakedFeeModule)
	if errUnstakedFeeModule != nil {
		return nil, errUnstakedFeeModule
	}
	out.UnstakedFeeModule = *valueUnstakedFeeModule
	out.Origins = []SlipstreamOrigin{}
	for _, item := range s.Origins {
		if item == nil {
			return nil, errors.New("SLIPSTREAM_WIRE_LIST_ITEM_REQUIRED")
		}
		value, e := SlipstreamOriginFromProto(item)
		if e != nil {
			return nil, e
		}
		out.Origins = append(out.Origins, *value)
	}
	out.ViewOrigin = s.ViewOrigin
	if s.ViewSwapFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_VIEW_SWAP_FEE_REQUIRED")
	}
	out.ViewSwapFee = *s.ViewSwapFee
	if s.ViewUnstakedFee == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_VIEW_UNSTAKED_FEE_REQUIRED")
	}
	out.ViewUnstakedFee = *s.ViewUnstakedFee
	out.RawBalance0, e = slipDecodeAmount(s.RawBalance0)
	if e != nil {
		return nil, e
	}
	out.RawBalance1, e = slipDecodeAmount(s.RawBalance1)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthGlobal0, e = slipDecodeAmount(s.FeeGrowthGlobal0)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthGlobal1, e = slipDecodeAmount(s.FeeGrowthGlobal1)
	if e != nil {
		return nil, e
	}
	out.RewardGrowthGlobal, e = slipDecodeAmount(s.RewardGrowthGlobal)
	if e != nil {
		return nil, e
	}
	out.GaugeFees0, e = slipDecodeAmount(s.GaugeFees0)
	if e != nil {
		return nil, e
	}
	out.GaugeFees1, e = slipDecodeAmount(s.GaugeFees1)
	if e != nil {
		return nil, e
	}
	out.RewardRate, e = slipDecodeAmount(s.RewardRate)
	if e != nil {
		return nil, e
	}
	out.RewardReserve, e = slipDecodeAmount(s.RewardReserve)
	if e != nil {
		return nil, e
	}
	out.PeriodFinish, e = slipDecodeAmount(s.PeriodFinish)
	if e != nil {
		return nil, e
	}
	out.Rollover, e = slipDecodeAmount(s.Rollover)
	if e != nil {
		return nil, e
	}
	out.StakedLiquidity, e = slipDecodeAmount(s.StakedLiquidity)
	if e != nil {
		return nil, e
	}
	if s.LastUpdated == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_LAST_UPDATED_REQUIRED")
	}
	out.LastUpdated = *s.LastUpdated
	out.MaxLiquidityPerTick, e = slipDecodeAmount(s.MaxLiquidityPerTick)
	if e != nil {
		return nil, e
	}
	out.Gauge = s.Gauge
	out.NFT = s.Nft
	out.FactoryRegistry = s.FactoryRegistry
	if s.ObservationIndex == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_OBSERVATION_INDEX_REQUIRED")
	}
	out.ObservationIndex = *s.ObservationIndex
	if s.ObservationCardinality == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_OBSERVATION_CARDINALITY_REQUIRED")
	}
	out.ObservationCardinality = *s.ObservationCardinality
	if s.ObservationCardinalityNext == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_OBSERVATION_CARDINALITY_NEXT_REQUIRED")
	}
	out.ObservationCardinalityNext = *s.ObservationCardinalityNext
	if s.Unlocked == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_UNLOCKED_REQUIRED")
	}
	out.Unlocked = *s.Unlocked
	out.Ticks = []SlipstreamTick{}
	for _, item := range s.Ticks {
		if item == nil {
			return nil, errors.New("SLIPSTREAM_WIRE_LIST_ITEM_REQUIRED")
		}
		value, e := SlipstreamTickFromProto(item)
		if e != nil {
			return nil, e
		}
		out.Ticks = append(out.Ticks, *value)
	}
	out.Observations = []SlipstreamObservation{}
	for _, item := range s.Observations {
		if item == nil {
			return nil, errors.New("SLIPSTREAM_WIRE_LIST_ITEM_REQUIRED")
		}
		value, e := SlipstreamObservationFromProto(item)
		if e != nil {
			return nil, e
		}
		out.Observations = append(out.Observations, *value)
	}
	out.BlockHash, e = slipDecodeDigest(s.BlockHash, true)
	if e != nil {
		return nil, e
	}
	if s.BlockNumber == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_BLOCK_NUMBER_REQUIRED")
	}
	out.BlockNumber = *s.BlockNumber
	if s.Timestamp == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_TIMESTAMP_REQUIRED")
	}
	out.Timestamp = *s.Timestamp
	if s.MaxObservations == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_MAX_OBSERVATIONS_REQUIRED")
	}
	out.MaxObservations = *s.MaxObservations
	if s.Derived == nil {
		return nil, errors.New("SLIPSTREAM_WIRE_DERIVED_REQUIRED")
	}
	out.Derived = *s.Derived
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
