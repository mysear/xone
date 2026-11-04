package domain

import pb "xone/common/protoc/golang/dexpb"

// validateWire checks byte widths before this conversion narrows JSON values.
func orcaAdaptiveToProto(a *OrcaAdaptiveState) *pb.OrcaAdaptiveState {
	if a == nil {
		return nil
	}
	bytes := func(values []uint32) []byte {
		out := make([]byte, len(values))
		for i, v := range values {
			out[i] = byte(v)
		}
		return out
	}
	return &pb.OrcaAdaptiveState{
		Whirlpool: a.Whirlpool, TradeEnableTimestamp: a.TradeEnableTimestamp,
		FilterPeriod: a.FilterPeriod, DecayPeriod: a.DecayPeriod, ReductionFactor: a.ReductionFactor,
		AdaptiveFeeControlFactor: a.AdaptiveFeeControlFactor, MaxVolatilityAccumulator: a.MaxVolatilityAccumulator,
		TickGroupSize: a.TickGroupSize, MajorSwapThresholdTicks: a.MajorSwapThresholdTicks,
		ConstantsReserved: bytes(a.ConstantsReserved), LastReferenceUpdateTimestamp: a.LastReferenceUpdateTimestamp,
		LastMajorSwapTimestamp: a.LastMajorSwapTimestamp, VolatilityReference: a.VolatilityReference,
		TickGroupIndexReference: a.TickGroupIndexReference, VolatilityAccumulator: a.VolatilityAccumulator,
		VariablesReserved: bytes(a.VariablesReserved), Reserved: bytes(a.Reserved),
	}
}

func orcaAdaptiveFromProto(a *pb.OrcaAdaptiveState) *OrcaAdaptiveState {
	if a == nil {
		return nil
	}
	values := func(bytes []byte) []uint32 {
		out := make([]uint32, len(bytes))
		for i, v := range bytes {
			out[i] = uint32(v)
		}
		return out
	}
	return &OrcaAdaptiveState{
		Whirlpool: a.Whirlpool, TradeEnableTimestamp: a.TradeEnableTimestamp,
		FilterPeriod: a.FilterPeriod, DecayPeriod: a.DecayPeriod, ReductionFactor: a.ReductionFactor,
		AdaptiveFeeControlFactor: a.AdaptiveFeeControlFactor, MaxVolatilityAccumulator: a.MaxVolatilityAccumulator,
		TickGroupSize: a.TickGroupSize, MajorSwapThresholdTicks: a.MajorSwapThresholdTicks,
		ConstantsReserved: values(a.ConstantsReserved), LastReferenceUpdateTimestamp: a.LastReferenceUpdateTimestamp,
		LastMajorSwapTimestamp: a.LastMajorSwapTimestamp, VolatilityReference: a.VolatilityReference,
		TickGroupIndexReference: a.TickGroupIndexReference, VolatilityAccumulator: a.VolatilityAccumulator,
		VariablesReserved: values(a.VariablesReserved), Reserved: values(a.Reserved),
	}
}
