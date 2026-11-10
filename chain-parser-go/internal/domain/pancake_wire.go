package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func pancakePtr[T any](v T) *T { return &v }
func pancakeWireDigest(value string) []byte {
	raw, _ := hex.DecodeString(strings.TrimPrefix(value, "0x"))
	return raw
}
func pancakeWireAmount(value Amount) *pb.Amount {
	raw, _ := value.Bytes()
	return &pb.Amount{Uint256Be: raw}
}
func pancakeDecodeDigest(value []byte, block bool) (string, error) {
	if len(value) != 0 && len(value) != 32 {
		return "", errors.New("PANCAKE_LM_WIRE_DIGEST_WIDTH")
	}
	s := hex.EncodeToString(value)
	if block {
		s = "0x" + s
	}
	return s, nil
}
func pancakeDecodeAmount(value *pb.Amount) (Amount, error) {
	if value == nil {
		return "", errors.New("PANCAKE_LM_WIRE_AMOUNT_REQUIRED")
	}
	return AmountFromBytes(value.Uint256Be)
}
func pancakeWireSigned(value string, bits uint) ([]byte, error) {
	n, ok := new(big.Int).SetString(value, 10)
	if !ok || n.String() != value {
		return nil, errors.New("PANCAKE_LM_SIGNED_CANONICAL")
	}
	limit := new(big.Int).Lsh(big.NewInt(1), bits-1)
	negative := new(big.Int).Neg(new(big.Int).Set(limit))
	if n.Cmp(negative) < 0 || n.Cmp(limit) >= 0 {
		return nil, errors.New("PANCAKE_LM_SIGNED_RANGE")
	}
	if n.Sign() < 0 {
		n.Add(n, new(big.Int).Lsh(big.NewInt(1), bits))
	}
	return n.FillBytes(make([]byte, bits/8)), nil
}
func pancakeDecodeSigned(raw []byte, bits uint) (string, error) {
	if len(raw) != int(bits/8) {
		return "", errors.New("PANCAKE_LM_WIRE_SIGNED_WIDTH")
	}
	n := new(big.Int).SetBytes(raw)
	if raw[0]&128 != 0 {
		n.Sub(n, new(big.Int).Lsh(big.NewInt(1), bits))
	}
	return n.String(), nil
}

func PancakeLmTickToProto(s *PancakeLmTick) (*pb.PancakeLmTick, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.PancakeLmTick{}
	var e error
	_ = e
	out.Tick = pancakePtr(s.Tick)
	out.LiquidityGross = pancakeWireAmount(s.LiquidityGross)
	out.LiquidityNet, e = pancakeWireSigned(s.LiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.RewardGrowthOutside = pancakeWireAmount(s.RewardGrowthOutside)
	return out, nil
}
func PancakeLmTickFromProto(s *pb.PancakeLmTick) (*PancakeLmTick, error) {
	if s == nil {
		return nil, nil
	}
	out := &PancakeLmTick{}
	var e error
	_ = e
	if s.Tick == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_TICK_REQUIRED")
	}
	out.Tick = *s.Tick
	out.LiquidityGross, e = pancakeDecodeAmount(s.LiquidityGross)
	if e != nil {
		return nil, e
	}
	out.LiquidityNet, e = pancakeDecodeSigned(s.LiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.RewardGrowthOutside, e = pancakeDecodeAmount(s.RewardGrowthOutside)
	if e != nil {
		return nil, e
	}
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func PancakeLedgerTickToProto(s *PancakeLedgerTick) (*pb.PancakeLedgerTick, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.PancakeLedgerTick{}
	var e error
	_ = e
	out.Tick = pancakePtr(s.Tick)
	out.LiquidityGross = pancakeWireAmount(s.LiquidityGross)
	out.LiquidityNet, e = pancakeWireSigned(s.LiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthOutside0 = pancakeWireAmount(s.FeeGrowthOutside0)
	out.FeeGrowthOutside1 = pancakeWireAmount(s.FeeGrowthOutside1)
	out.TickCumulativeOutside, e = pancakeWireSigned(s.TickCumulativeOutside, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityOutsideX128 = pancakeWireAmount(s.SecondsPerLiquidityOutsideX128)
	out.SecondsOutside = pancakePtr(s.SecondsOutside)
	out.Initialized = pancakePtr(s.Initialized)
	return out, nil
}
func PancakeLedgerTickFromProto(s *pb.PancakeLedgerTick) (*PancakeLedgerTick, error) {
	if s == nil {
		return nil, nil
	}
	out := &PancakeLedgerTick{}
	var e error
	_ = e
	if s.Tick == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_TICK_REQUIRED")
	}
	out.Tick = *s.Tick
	out.LiquidityGross, e = pancakeDecodeAmount(s.LiquidityGross)
	if e != nil {
		return nil, e
	}
	out.LiquidityNet, e = pancakeDecodeSigned(s.LiquidityNet, 128)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthOutside0, e = pancakeDecodeAmount(s.FeeGrowthOutside0)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthOutside1, e = pancakeDecodeAmount(s.FeeGrowthOutside1)
	if e != nil {
		return nil, e
	}
	out.TickCumulativeOutside, e = pancakeDecodeSigned(s.TickCumulativeOutside, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityOutsideX128, e = pancakeDecodeAmount(s.SecondsPerLiquidityOutsideX128)
	if e != nil {
		return nil, e
	}
	if s.SecondsOutside == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_SECONDS_OUTSIDE_REQUIRED")
	}
	out.SecondsOutside = *s.SecondsOutside
	if s.Initialized == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_INITIALIZED_REQUIRED")
	}
	out.Initialized = *s.Initialized
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func PancakeObservationToProto(s *PancakeObservation) (*pb.PancakeObservation, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.PancakeObservation{}
	var e error
	_ = e
	out.BlockTimestamp = pancakePtr(s.BlockTimestamp)
	out.TickCumulative, e = pancakeWireSigned(s.TickCumulative, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityCumulativeX128 = pancakeWireAmount(s.SecondsPerLiquidityCumulativeX128)
	out.Initialized = pancakePtr(s.Initialized)
	return out, nil
}
func PancakeObservationFromProto(s *pb.PancakeObservation) (*PancakeObservation, error) {
	if s == nil {
		return nil, nil
	}
	out := &PancakeObservation{}
	var e error
	_ = e
	if s.BlockTimestamp == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_BLOCK_TIMESTAMP_REQUIRED")
	}
	out.BlockTimestamp = *s.BlockTimestamp
	out.TickCumulative, e = pancakeDecodeSigned(s.TickCumulative, 56)
	if e != nil {
		return nil, e
	}
	out.SecondsPerLiquidityCumulativeX128, e = pancakeDecodeAmount(s.SecondsPerLiquidityCumulativeX128)
	if e != nil {
		return nil, e
	}
	if s.Initialized == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_INITIALIZED_REQUIRED")
	}
	out.Initialized = *s.Initialized
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func PancakeMasterChefToProto(s *PancakeMasterChef) (*pb.PancakeMasterChef, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.PancakeMasterChef{}
	var e error
	_ = e
	out.Address = s.Address
	out.CodeSha256 = pancakeWireDigest(s.CodeSHA256)
	out.TemplateSha256 = pancakeWireDigest(s.TemplateSHA256)
	out.Cake = s.Cake
	out.Weth = s.Weth
	out.PositionManager = s.PositionManager
	out.PoolPid = pancakeWireAmount(s.PoolPid)
	out.PoolAddress = s.PoolAddress
	out.PoolToken0 = s.PoolToken0
	out.PoolToken1 = s.PoolToken1
	out.PoolFee = pancakePtr(s.PoolFee)
	out.PoolAllocPoint = pancakeWireAmount(s.PoolAllocPoint)
	out.TotalAllocPoint = pancakeWireAmount(s.TotalAllocPoint)
	out.LatestPeriodCakePerSecond = pancakeWireAmount(s.LatestPeriodCakePerSecond)
	out.LatestPeriodEndTime = pancakeWireAmount(s.LatestPeriodEndTime)
	out.ViewRewardPerSecond = pancakeWireAmount(s.ViewRewardPerSecond)
	out.ViewEndTime = pancakeWireAmount(s.ViewEndTime)
	return out, nil
}
func PancakeMasterChefFromProto(s *pb.PancakeMasterChef) (*PancakeMasterChef, error) {
	if s == nil {
		return nil, nil
	}
	out := &PancakeMasterChef{}
	var e error
	_ = e
	out.Address = s.Address
	out.CodeSHA256, e = pancakeDecodeDigest(s.CodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.TemplateSHA256, e = pancakeDecodeDigest(s.TemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.Cake = s.Cake
	out.Weth = s.Weth
	out.PositionManager = s.PositionManager
	out.PoolPid, e = pancakeDecodeAmount(s.PoolPid)
	if e != nil {
		return nil, e
	}
	out.PoolAddress = s.PoolAddress
	out.PoolToken0 = s.PoolToken0
	out.PoolToken1 = s.PoolToken1
	if s.PoolFee == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_POOL_FEE_REQUIRED")
	}
	out.PoolFee = *s.PoolFee
	out.PoolAllocPoint, e = pancakeDecodeAmount(s.PoolAllocPoint)
	if e != nil {
		return nil, e
	}
	out.TotalAllocPoint, e = pancakeDecodeAmount(s.TotalAllocPoint)
	if e != nil {
		return nil, e
	}
	out.LatestPeriodCakePerSecond, e = pancakeDecodeAmount(s.LatestPeriodCakePerSecond)
	if e != nil {
		return nil, e
	}
	out.LatestPeriodEndTime, e = pancakeDecodeAmount(s.LatestPeriodEndTime)
	if e != nil {
		return nil, e
	}
	out.ViewRewardPerSecond, e = pancakeDecodeAmount(s.ViewRewardPerSecond)
	if e != nil {
		return nil, e
	}
	out.ViewEndTime, e = pancakeDecodeAmount(s.ViewEndTime)
	if e != nil {
		return nil, e
	}
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func PancakeLmToProto(s *PancakeLm) (*pb.PancakeLm, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.PancakeLm{}
	var e error
	_ = e
	out.Address = s.Address
	out.CodeSha256 = pancakeWireDigest(s.CodeSHA256)
	out.TemplateSha256 = pancakeWireDigest(s.TemplateSHA256)
	out.Pool = s.Pool
	out.MasterChef = s.MasterChef
	out.RewardGrowthGlobal = pancakeWireAmount(s.RewardGrowthGlobal)
	out.Liquidity = pancakeWireAmount(s.Liquidity)
	out.LastRewardTimestamp = pancakePtr(s.LastRewardTimestamp)
	for i := range s.Ticks {
		value, e := PancakeLmTickToProto(&s.Ticks[i])
		if e != nil {
			return nil, e
		}
		out.Ticks = append(out.Ticks, value)
	}
	return out, nil
}
func PancakeLmFromProto(s *pb.PancakeLm) (*PancakeLm, error) {
	if s == nil {
		return nil, nil
	}
	out := &PancakeLm{}
	var e error
	_ = e
	out.Address = s.Address
	out.CodeSHA256, e = pancakeDecodeDigest(s.CodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.TemplateSHA256, e = pancakeDecodeDigest(s.TemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.Pool = s.Pool
	out.MasterChef = s.MasterChef
	out.RewardGrowthGlobal, e = pancakeDecodeAmount(s.RewardGrowthGlobal)
	if e != nil {
		return nil, e
	}
	out.Liquidity, e = pancakeDecodeAmount(s.Liquidity)
	if e != nil {
		return nil, e
	}
	if s.LastRewardTimestamp == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_LAST_REWARD_TIMESTAMP_REQUIRED")
	}
	out.LastRewardTimestamp = *s.LastRewardTimestamp
	out.Ticks = []PancakeLmTick{}
	for _, item := range s.Ticks {
		if item == nil {
			return nil, errors.New("PANCAKE_LM_WIRE_LIST_ITEM_REQUIRED")
		}
		value, e := PancakeLmTickFromProto(item)
		if e != nil {
			return nil, e
		}
		out.Ticks = append(out.Ticks, *value)
	}
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
func PancakeLmStateToProto(s *PancakeLmState) (*pb.PancakeLmState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	out := &pb.PancakeLmState{}
	var e error
	_ = e
	out.Core, e = V3ToProto(s.Core)
	if e != nil {
		return nil, e
	}
	out.Tick = pancakePtr(s.Tick)
	out.FeeProtocol = pancakePtr(s.FeeProtocol)
	out.PoolTemplateSha256 = pancakeWireDigest(s.PoolTemplateSHA256)
	out.FactoryTemplateSha256 = pancakeWireDigest(s.FactoryTemplateSHA256)
	out.Token0CodeSha256 = pancakeWireDigest(s.Token0CodeSHA256)
	out.Token1CodeSha256 = pancakeWireDigest(s.Token1CodeSHA256)
	out.FactoryRegistered = pancakePtr(s.FactoryRegistered)
	out.MasterChef, e = PancakeMasterChefToProto(&s.MasterChef)
	if e != nil {
		return nil, e
	}
	out.Lm, e = PancakeLmToProto(&s.Lm)
	if e != nil {
		return nil, e
	}
	out.RawBalance0 = pancakeWireAmount(s.RawBalance0)
	out.RawBalance1 = pancakeWireAmount(s.RawBalance1)
	out.FeeGrowthGlobal0 = pancakeWireAmount(s.FeeGrowthGlobal0)
	out.FeeGrowthGlobal1 = pancakeWireAmount(s.FeeGrowthGlobal1)
	out.ProtocolFees0 = pancakeWireAmount(s.ProtocolFees0)
	out.ProtocolFees1 = pancakeWireAmount(s.ProtocolFees1)
	out.MaxLiquidityPerTick = pancakeWireAmount(s.MaxLiquidityPerTick)
	out.ObservationIndex = pancakePtr(s.ObservationIndex)
	out.ObservationCardinality = pancakePtr(s.ObservationCardinality)
	out.ObservationCardinalityNext = pancakePtr(s.ObservationCardinalityNext)
	out.Unlocked = pancakePtr(s.Unlocked)
	for i := range s.Ticks {
		value, e := PancakeLedgerTickToProto(&s.Ticks[i])
		if e != nil {
			return nil, e
		}
		out.Ticks = append(out.Ticks, value)
	}
	for i := range s.Observations {
		value, e := PancakeObservationToProto(&s.Observations[i])
		if e != nil {
			return nil, e
		}
		out.Observations = append(out.Observations, value)
	}
	out.BlockHash = pancakeWireDigest(s.BlockHash)
	out.BlockNumber = pancakePtr(s.BlockNumber)
	out.Timestamp = pancakePtr(s.Timestamp)
	out.MaxObservations = pancakePtr(s.MaxObservations)
	out.Derived = pancakePtr(s.Derived)
	return out, nil
}
func PancakeLmStateFromProto(s *pb.PancakeLmState) (*PancakeLmState, error) {
	if s == nil {
		return nil, nil
	}
	out := &PancakeLmState{}
	var e error
	_ = e
	if s.Core == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_CORE_REQUIRED")
	}
	out.Core, e = V3FromProto(s.Core)
	if e != nil {
		return nil, e
	}
	if s.Tick == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_TICK_REQUIRED")
	}
	out.Tick = *s.Tick
	if s.FeeProtocol == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_FEE_PROTOCOL_REQUIRED")
	}
	out.FeeProtocol = *s.FeeProtocol
	out.PoolTemplateSHA256, e = pancakeDecodeDigest(s.PoolTemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.FactoryTemplateSHA256, e = pancakeDecodeDigest(s.FactoryTemplateSha256, false)
	if e != nil {
		return nil, e
	}
	out.Token0CodeSHA256, e = pancakeDecodeDigest(s.Token0CodeSha256, false)
	if e != nil {
		return nil, e
	}
	out.Token1CodeSHA256, e = pancakeDecodeDigest(s.Token1CodeSha256, false)
	if e != nil {
		return nil, e
	}
	if s.FactoryRegistered == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_FACTORY_REGISTERED_REQUIRED")
	}
	out.FactoryRegistered = *s.FactoryRegistered
	if s.MasterChef == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_MASTER_CHEF_REQUIRED")
	}
	valueMasterChef, errMasterChef := PancakeMasterChefFromProto(s.MasterChef)
	if errMasterChef != nil {
		return nil, errMasterChef
	}
	out.MasterChef = *valueMasterChef
	if s.Lm == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_LM_REQUIRED")
	}
	valueLm, errLm := PancakeLmFromProto(s.Lm)
	if errLm != nil {
		return nil, errLm
	}
	out.Lm = *valueLm
	out.RawBalance0, e = pancakeDecodeAmount(s.RawBalance0)
	if e != nil {
		return nil, e
	}
	out.RawBalance1, e = pancakeDecodeAmount(s.RawBalance1)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthGlobal0, e = pancakeDecodeAmount(s.FeeGrowthGlobal0)
	if e != nil {
		return nil, e
	}
	out.FeeGrowthGlobal1, e = pancakeDecodeAmount(s.FeeGrowthGlobal1)
	if e != nil {
		return nil, e
	}
	out.ProtocolFees0, e = pancakeDecodeAmount(s.ProtocolFees0)
	if e != nil {
		return nil, e
	}
	out.ProtocolFees1, e = pancakeDecodeAmount(s.ProtocolFees1)
	if e != nil {
		return nil, e
	}
	out.MaxLiquidityPerTick, e = pancakeDecodeAmount(s.MaxLiquidityPerTick)
	if e != nil {
		return nil, e
	}
	if s.ObservationIndex == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_OBSERVATION_INDEX_REQUIRED")
	}
	out.ObservationIndex = *s.ObservationIndex
	if s.ObservationCardinality == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_OBSERVATION_CARDINALITY_REQUIRED")
	}
	out.ObservationCardinality = *s.ObservationCardinality
	if s.ObservationCardinalityNext == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_OBSERVATION_CARDINALITY_NEXT_REQUIRED")
	}
	out.ObservationCardinalityNext = *s.ObservationCardinalityNext
	if s.Unlocked == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_UNLOCKED_REQUIRED")
	}
	out.Unlocked = *s.Unlocked
	out.Ticks = []PancakeLedgerTick{}
	for _, item := range s.Ticks {
		if item == nil {
			return nil, errors.New("PANCAKE_LM_WIRE_LIST_ITEM_REQUIRED")
		}
		value, e := PancakeLedgerTickFromProto(item)
		if e != nil {
			return nil, e
		}
		out.Ticks = append(out.Ticks, *value)
	}
	out.Observations = []PancakeObservation{}
	for _, item := range s.Observations {
		if item == nil {
			return nil, errors.New("PANCAKE_LM_WIRE_LIST_ITEM_REQUIRED")
		}
		value, e := PancakeObservationFromProto(item)
		if e != nil {
			return nil, e
		}
		out.Observations = append(out.Observations, *value)
	}
	out.BlockHash, e = pancakeDecodeDigest(s.BlockHash, true)
	if e != nil {
		return nil, e
	}
	if s.BlockNumber == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_BLOCK_NUMBER_REQUIRED")
	}
	out.BlockNumber = *s.BlockNumber
	if s.Timestamp == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_TIMESTAMP_REQUIRED")
	}
	out.Timestamp = *s.Timestamp
	if s.MaxObservations == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_MAX_OBSERVATIONS_REQUIRED")
	}
	out.MaxObservations = *s.MaxObservations
	if s.Derived == nil {
		return nil, errors.New("PANCAKE_LM_WIRE_DERIVED_REQUIRED")
	}
	out.Derived = *s.Derived
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
