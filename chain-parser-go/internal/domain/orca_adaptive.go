package domain

import "errors"

// OrcaAdaptiveState preserves the complete Oracle payload. JSON represents bytes
// as uint32 arrays so validation can reject values before narrowing to protobuf.
type OrcaAdaptiveState struct {
	Whirlpool                    string   `json:"whirlpool"`
	TradeEnableTimestamp         uint64   `json:"trade_enable_timestamp"`
	FilterPeriod                 uint32   `json:"filter_period"`
	DecayPeriod                  uint32   `json:"decay_period"`
	ReductionFactor              uint32   `json:"reduction_factor"`
	AdaptiveFeeControlFactor     uint32   `json:"adaptive_fee_control_factor"`
	MaxVolatilityAccumulator     uint32   `json:"max_volatility_accumulator"`
	TickGroupSize                uint32   `json:"tick_group_size"`
	MajorSwapThresholdTicks      uint32   `json:"major_swap_threshold_ticks"`
	ConstantsReserved            []uint32 `json:"constants_reserved"`
	LastReferenceUpdateTimestamp uint64   `json:"last_reference_update_timestamp"`
	LastMajorSwapTimestamp       uint64   `json:"last_major_swap_timestamp"`
	VolatilityReference          uint32   `json:"volatility_reference"`
	TickGroupIndexReference      int32    `json:"tick_group_index_reference"`
	VolatilityAccumulator        uint32   `json:"volatility_accumulator"`
	VariablesReserved            []uint32 `json:"variables_reserved"`
	Reserved                     []uint32 `json:"reserved"`
}

func (s OrcaWhirlpoolState) validateAdaptiveState() error {
	a := s.Adaptive
	if a == nil {
		return nil
	}
	if ValidateSolanaPubkey(a.Whirlpool) != nil {
		return errors.New("ORCA_ADAPTIVE_WHIRLPOOL")
	}
	if a.TradeEnableTimestamp > s.BankUnixTimestamp {
		return errors.New("ORCA_ADAPTIVE_TRADE_NOT_ENABLED")
	}
	if a.LastReferenceUpdateTimestamp > s.BankUnixTimestamp || a.LastMajorSwapTimestamp > s.BankUnixTimestamp {
		return errors.New("ORCA_ADAPTIVE_TIMESTAMP")
	}
	// The on-chain periods, reduction, group size and major threshold are u16.
	// These checks precede division or any narrowing to the account types.
	if a.FilterPeriod == 0 || a.FilterPeriod > 65535 || a.DecayPeriod > 65535 || a.DecayPeriod <= a.FilterPeriod || a.ReductionFactor >= 10000 || a.AdaptiveFeeControlFactor >= 100000 || a.TickGroupSize == 0 || a.TickGroupSize > s.TickSpacing || s.TickSpacing%a.TickGroupSize != 0 || a.MajorSwapThresholdTicks == 0 || a.MajorSwapThresholdTicks > 65535 || uint64(a.MajorSwapThresholdTicks) > uint64(s.TickSpacing)*88 || uint64(a.MaxVolatilityAccumulator)*uint64(a.TickGroupSize) > 4294967295 {
		return errors.New("ORCA_ADAPTIVE_INVALID_CONSTANTS")
	}
	if a.VolatilityReference > a.MaxVolatilityAccumulator || a.VolatilityAccumulator > a.MaxVolatilityAccumulator {
		return errors.New("ORCA_ADAPTIVE_INVALID_STATE")
	}
	// FeeRateManager::begin admits the shifted minimum tick after an A-to-B
	// boundary crossing. Go integer division truncates, so explicitly floor it.
	size := int32(a.TickGroupSize)
	minGroup := (OrcaMinTick - 1) / size
	if (OrcaMinTick-1)%size != 0 {
		minGroup--
	}
	if a.TickGroupIndexReference < minGroup || a.TickGroupIndexReference > OrcaMaxTick/size {
		return errors.New("ORCA_ADAPTIVE_GROUP_RANGE")
	}
	for _, field := range []struct {
		values []uint32
		size   int
	}{{a.ConstantsReserved, 16}, {a.VariablesReserved, 16}, {a.Reserved, 128}} {
		if len(field.values) != field.size {
			return errors.New("ORCA_ADAPTIVE_RESERVED_WIDTH")
		}
		for _, value := range field.values {
			if value > 255 {
				return errors.New("ORCA_ADAPTIVE_RESERVED_BYTE")
			}
		}
	}
	if len(s.RewardInfos) != 3 {
		return errors.New("ORCA_ADAPTIVE_REWARD_COUNT")
	}
	if len(s.TickArrays) != 3 {
		return errors.New("ORCA_ADAPTIVE_ARRAY_COUNT")
	}
	return nil
}
