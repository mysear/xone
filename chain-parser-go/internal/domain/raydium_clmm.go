package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const RayCLMMProgramData = "HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx"
const RayCLMMProgram = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK"

type RaydiumCLMMRewardInfo struct {
	RewardState           uint32 `json:"reward_state"`
	OpenTime              uint64 `json:"open_time"`
	EndTime               uint64 `json:"end_time"`
	LastUpdateTime        uint64 `json:"last_update_time"`
	EmissionsPerSecondX64 Amount `json:"emissions_per_second_x64"`
	RewardTotalEmitted    uint64 `json:"reward_total_emitted"`
	RewardClaimed         uint64 `json:"reward_claimed"`
	TokenMint             string `json:"token_mint"`
	TokenVault            string `json:"token_vault"`
	Authority             string `json:"authority"`
	RewardGrowthGlobalX64 Amount `json:"reward_growth_global_x64"`
}
type RaydiumCLMMTick struct {
	RewardGrowthsOutside []Amount `json:"reward_growths_outside,omitempty"`
	Tick                 int32    `json:"tick"`
	LiquidityGross       Amount   `json:"liquidity_gross"`
	LiquidityNet         string   `json:"liquidity_net"`
	FeeGrowthOutside0    Amount   `json:"fee_growth_outside0"`
	FeeGrowthOutside1    Amount   `json:"fee_growth_outside1"`
}
type RaydiumCLMMArray struct {
	Address        string            `json:"address"`
	StartTickIndex int32             `json:"start_tick_index"`
	RecentEpoch    uint64            `json:"recent_epoch"`
	Ticks          []RaydiumCLMMTick `json:"ticks"`
}
type RaydiumCLMMObservation struct {
	BlockTimestamp uint32 `json:"block_timestamp"`
	TickCumulative int64  `json:"tick_cumulative"`
}
type RaydiumCLMMBitmapExtension struct {
	Address  string   `json:"address"`
	Positive []uint64 `json:"positive"`
	Negative []uint64 `json:"negative"`
}
type RaydiumCLMMDynamicFee struct {
	FilterPeriod              uint32 `json:"filter_period"`
	DecayPeriod               uint32 `json:"decay_period"`
	ReductionFactor           uint32 `json:"reduction_factor"`
	DynamicFeeControl         uint32 `json:"dynamic_fee_control"`
	MaxVolatilityAccumulator  uint32 `json:"max_volatility_accumulator"`
	TickSpacingIndexReference int32  `json:"tick_spacing_index_reference"`
	VolatilityReference       uint32 `json:"volatility_reference"`
	VolatilityAccumulator     uint32 `json:"volatility_accumulator"`
	LastUpdateTimestamp       uint64 `json:"last_update_timestamp"`
}
type RaydiumCLMMState struct {
	RewardInfos            []RaydiumCLMMRewardInfo     `json:"reward_infos,omitempty"`
	DynamicFee             *RaydiumCLMMDynamicFee      `json:"dynamic_fee,omitempty"`
	SeedIndex              uint32                      `json:"seed_index,omitempty"`
	BitmapExtension        *RaydiumCLMMBitmapExtension `json:"bitmap_extension,omitempty"`
	CollectFeeOn           uint32                      `json:"collect_fee_on,omitempty"`
	ProgramID              string                      `json:"program_id"`
	Programdata            string                      `json:"programdata"`
	ProgramCodeSHA256      string                      `json:"program_code_sha256"`
	DeployedSlot           uint64                      `json:"deployed_slot"`
	TokenProgramCodeSHA256 string                      `json:"token_program_code_sha256"`
	AmmConfig              string                      `json:"amm_config"`
	PoolBump               uint32                      `json:"pool_bump"`
	Observation            string                      `json:"observation"`
	Vault0                 string                      `json:"vault0"`
	Vault1                 string                      `json:"vault1"`
	RawVault0              uint64                      `json:"raw_vault0"`
	RawVault1              uint64                      `json:"raw_vault1"`
	Mint0Decimals          uint32                      `json:"mint0_decimals"`
	Mint1Decimals          uint32                      `json:"mint1_decimals"`
	BankSlot               uint64                      `json:"bank_slot"`
	BankUnixTimestamp      uint64                      `json:"bank_unix_timestamp"`
	BankEpoch              uint64                      `json:"bank_epoch"`
	TradeFeeRate           uint32                      `json:"trade_fee_rate"`
	ProtocolFeeRate        uint32                      `json:"protocol_fee_rate"`
	FundFeeRate            uint32                      `json:"fund_fee_rate"`
	TickSpacing            uint32                      `json:"tick_spacing"`
	SqrtPriceX64           Amount                      `json:"sqrt_price_x64"`
	TickCurrent            int32                       `json:"tick_current"`
	Liquidity              Amount                      `json:"liquidity"`
	FeeGrowthGlobal0       Amount                      `json:"fee_growth_global0"`
	FeeGrowthGlobal1       Amount                      `json:"fee_growth_global1"`
	ProtocolFees0          uint64                      `json:"protocol_fees0"`
	ProtocolFees1          uint64                      `json:"protocol_fees1"`
	FundFees0              uint64                      `json:"fund_fees0"`
	FundFees1              uint64                      `json:"fund_fees1"`
	OpenTime               uint64                      `json:"open_time"`
	RecentEpoch            uint64                      `json:"recent_epoch"`
	Bitmap                 []uint64                    `json:"bitmap"`
	TickArrays             []RaydiumCLMMArray          `json:"tick_arrays"`
	MaxSteps               uint32                      `json:"max_steps"`
	ObservationInitialized bool                        `json:"observation_initialized"`
	ObservationIndex       uint32                      `json:"observation_index"`
	ObservationRecentEpoch uint64                      `json:"observation_recent_epoch"`
	Observations           []RaydiumCLMMObservation    `json:"observations"`
}

func (s RaydiumCLMMState) validateWire() error {
	if e := s.validateRewardState(); e != nil {
		return e
	}
	if s.SeedIndex > 65535 || s.CollectFeeOn > 2 || s.ProgramID != RayCLMMProgram || s.Programdata != RayCLMMProgramData || s.PoolBump > 255 || s.Mint0Decimals > 255 || s.Mint1Decimals > 255 || s.DeployedSlot > s.BankSlot || s.BankUnixTimestamp > 4294967295 || s.OpenTime >= s.BankUnixTimestamp || s.RecentEpoch > s.BankEpoch || s.ObservationRecentEpoch > s.BankEpoch || s.TickSpacing == 0 || s.TickSpacing > 1000 || s.TradeFeeRate >= 1000000 || s.ProtocolFeeRate > 1000000 || s.FundFeeRate > 1000000 || uint64(s.ProtocolFeeRate)+uint64(s.FundFeeRate) > 1000000 || s.MaxSteps == 0 || s.MaxSteps > 8192 || len(s.Bitmap) != 16 || len(s.TickArrays) < 3 || len(s.TickArrays) > 7 || len(s.Observations) != 100 || s.ObservationIndex >= 100 {
		return errors.New("RAYCLMM_STATE_PARAMETERS")
	}
	if s.BitmapExtension != nil && (ValidateSolanaPubkey(s.BitmapExtension.Address) != nil || len(s.BitmapExtension.Positive) != 112 || len(s.BitmapExtension.Negative) != 112) {
		return errors.New("RAYCLMM_EXTENSION_WIDTH")
	}
	if d := s.DynamicFee; d != nil {
		spacing := int64(s.TickSpacing)
		minimum := int64(RayCLMMMinTick) / spacing
		if int64(RayCLMMMinTick)%spacing != 0 {
			minimum--
		}
		maximum := int64(RayCLMMMaxTick) / spacing
		if d.FilterPeriod == 0 || d.FilterPeriod > 65535 || d.DecayPeriod <= d.FilterPeriod || d.DecayPeriod > 65535 || d.ReductionFactor == 0 || d.ReductionFactor >= 10000 || d.DynamicFeeControl == 0 || d.DynamicFeeControl >= 100000 || uint64(d.MaxVolatilityAccumulator)*uint64(s.TickSpacing) > 4294967295 || d.VolatilityReference > d.MaxVolatilityAccumulator || d.VolatilityAccumulator > d.MaxVolatilityAccumulator || int64(d.TickSpacingIndexReference) < minimum || int64(d.TickSpacingIndexReference) > maximum || d.LastUpdateTimestamp > 4294967295 {
			return errors.New("RAYCLMM_DYNAMIC_PARAMETERS")
		}
	}
	seen := map[string]bool{}
	for _, key := range []string{s.Programdata, s.AmmConfig, s.Observation, s.Vault0, s.Vault1} {
		if ValidateSolanaPubkey(key) != nil || seen[key] {
			return errors.New("RAYCLMM_ACCOUNT_IDENTITY")
		}
		seen[key] = true
	}
	for _, h := range []string{s.ProgramCodeSHA256, s.TokenProgramCodeSHA256} {
		b, e := hex.DecodeString(h)
		if e != nil || len(b) != 32 || h != strings.ToLower(h) {
			return errors.New("RAYCLMM_CODE_IDENTITY")
		}
	}
	width := func(a Amount) (*big.Int, error) {
		v, e := ParseAmount(a.String())
		if e != nil || v.Big().BitLen() > 128 {
			return nil, errors.New("RAYCLMM_U128_WIDTH")
		}
		return v.Big(), nil
	}
	for _, a := range []Amount{s.Liquidity, s.FeeGrowthGlobal0, s.FeeGrowthGlobal1} {
		if _, e := width(a); e != nil {
			return e
		}
	}
	price, e := width(s.SqrtPriceX64)
	if e != nil {
		return e
	}
	min, _ := RayCLMMSqrtPriceAtTick(RayCLMMMinTick)
	max, _ := RayCLMMSqrtPriceAtTick(RayCLMMMaxTick)
	if price.Cmp(min) < 0 || price.Cmp(max) >= 0 || s.TickCurrent < RayCLMMMinTick || s.TickCurrent >= RayCLMMMaxTick {
		return errors.New("RAYCLMM_PRICE_TICK")
	}
	lo, _ := RayCLMMSqrtPriceAtTick(s.TickCurrent)
	hi, _ := RayCLMMSqrtPriceAtTick(s.TickCurrent + 1)
	if price.Cmp(lo) < 0 || price.Cmp(hi) > 0 {
		return errors.New("RAYCLMM_PRICE_TICK")
	}
	span := int64(60) * int64(s.TickSpacing)
	for i, a := range s.TickArrays {
		start := int64(a.StartTickIndex)
		idx := start / span
		if ValidateSolanaPubkey(a.Address) != nil || seen[a.Address] || start%span != 0 || (s.BitmapExtension == nil && (idx < -512 || idx > 511)) || idx < -7680 || idx > 7679 || start > int64(RayCLMMMaxTick) || start+span <= int64(RayCLMMMinTick) || a.RecentEpoch > s.BankEpoch || i > 0 && start != int64(s.TickArrays[i-1].StartTickIndex)+span {
			return errors.New("RAYCLMM_ARRAY_SEQUENCE")
		}
		seen[a.Address] = true
		active, e := s.ArrayInitialized(int32(idx))
		if e != nil {
			return e
		}
		if active != (len(a.Ticks) > 0) {
			return errors.New("RAYCLMM_BITMAP_PROOF")
		}
		if len(a.Ticks) > 60 {
			return errors.New("RAYCLMM_TICK_LIMIT")
		}
		for j, t := range a.Ticks {
			if int64(t.Tick) < start || int64(t.Tick) >= start+span || t.Tick%int32(s.TickSpacing) != 0 || t.Tick < RayCLMMMinTick || t.Tick > RayCLMMMaxTick || j > 0 && t.Tick <= a.Ticks[j-1].Tick {
				return errors.New("RAYCLMM_TICK_INDEX")
			}
			gross, e := width(t.LiquidityGross)
			if e != nil || gross.Sign() == 0 {
				return errors.New("RAYCLMM_TICK_GROSS")
			}
			net, e := ParseLiquidityNet(t.LiquidityNet)
			if e != nil || new(big.Int).Abs(net).Cmp(gross) > 0 {
				return errors.New("RAYCLMM_TICK_NET")
			}
			for _, f := range []Amount{t.FeeGrowthOutside0, t.FeeGrowthOutside1} {
				if _, e = width(f); e != nil {
					return e
				}
			}
		}
	}
	low := int64(s.TickArrays[0].StartTickIndex)
	high := int64(s.TickArrays[len(s.TickArrays)-1].StartTickIndex) + span
	if int64(s.TickCurrent) < low-1 || int64(s.TickCurrent) >= high {
		return errors.New("RAYCLMM_CURRENT_TICK_UNPROVEN")
	}
	return nil
}
func (s RaydiumCLMMState) Validate(p Pool) error {
	if (p.Protocol == "raydium-clmm-rewards") != (len(s.RewardInfos) == 3) {
		return errors.New("RAYCLMM_REWARD_PROFILE")
	}
	if e := s.validateWire(); e != nil {
		return e
	}
	if (p.Protocol == "raydium-clmm-bitmap" || p.Protocol == "raydium-clmm-custom-seed" || p.Protocol == "raydium-clmm-dynamic") != (s.BitmapExtension != nil) {
		return errors.New("RAYCLMM_EXTENSION_PROFILE")
	}
	if p.Protocol != "raydium-clmm-dynamic" && (p.Protocol == "raydium-clmm-custom-seed") != (s.SeedIndex > 0) {
		return errors.New("RAYCLMM_SEED_PROFILE")
	}
	if (p.Protocol == "raydium-clmm-dynamic") != (s.DynamicFee != nil) {
		return errors.New("RAYCLMM_DYNAMIC_PROFILE")
	}
	if ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "solana:") || !((p.Protocol == "raydium-clmm-static" && p.AdapterVersion == "raydium-clmm/6d7a0f82-spl-static-1" && s.CollectFeeOn == 0) || (p.Protocol == "raydium-clmm-token-fee" && p.AdapterVersion == "raydium-clmm/6d7a0f82-spl-token-fee-1" && (s.CollectFeeOn == 1 || s.CollectFeeOn == 2)) || (p.Protocol == "raydium-clmm-bitmap" && p.AdapterVersion == "raydium-clmm/6d7a0f82-spl-bitmap-1") || (p.Protocol == "raydium-clmm-custom-seed" && p.AdapterVersion == "raydium-clmm/6d7a0f82-spl-custom-seed-1") || (p.Protocol == "raydium-clmm-dynamic" && p.AdapterVersion == "raydium-clmm/6d7a0f82-spl-dynamic-1") || (p.Protocol == "raydium-clmm-rewards" && p.AdapterVersion == "raydium-clmm/6d7a0f82-spl-rewards-1")) || p.FeeNumerator != uint64(s.TradeFeeRate) || p.FeeDenominator != 1000000 || p.Reserve0 != "0" || p.Reserve1 != "0" || p.TokenBehavior != "standard" || p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil {
		return errors.New("RAYCLMM_POOL_IDENTITY")
	}
	seen := map[string]bool{}
	for _, key := range []string{p.PoolID, p.Token0, p.Token1, s.ProgramID, s.Programdata, s.AmmConfig, s.Observation, s.Vault0, s.Vault1, RaydiumTokenProgram, SolanaClock} {
		if ValidateSolanaPubkey(key) != nil || seen[key] {
			return errors.New("RAYCLMM_ACCOUNT_ALIAS")
		}
		seen[key] = true
	}
	if SolanaPubkeyOrder(p.Token0, p.Token1) >= 0 {
		return errors.New("RAYCLMM_MINT_ORDER")
	}
	for _, a := range s.TickArrays {
		if seen[a.Address] {
			return errors.New("RAYCLMM_ACCOUNT_ALIAS")
		}
	}
	return s.validatePDAs(p)
}

func (s RaydiumCLMMState) validateRewardState() error {
	if len(s.RewardInfos) != 0 && len(s.RewardInfos) != 3 {
		return errors.New("RAYCLMM_REWARD_COUNT")
	}
	zero := "11111111111111111111111111111111"
	wide := func(a Amount) bool { n, e := ParseAmount(string(a)); return e == nil && n.Big().BitLen() <= 128 }
	for _, r := range s.RewardInfos {
		if (r.TokenMint != zero && ValidateSolanaPubkey(r.TokenMint) != nil) || (r.TokenVault != zero && ValidateSolanaPubkey(r.TokenVault) != nil) || (r.Authority != zero && ValidateSolanaPubkey(r.Authority) != nil) || !wide(r.EmissionsPerSecondX64) || !wide(r.RewardGrowthGlobalX64) {
			return errors.New("RAYCLMM_REWARD_PARAMETERS")
		}
		if r.TokenMint != zero {
			if r.RewardState < 1 || r.RewardState > 3 || r.TokenVault == zero || r.OpenTime >= r.EndTime || r.LastUpdateTime < r.OpenTime || r.LastUpdateTime > r.EndTime || (r.LastUpdateTime > s.BankUnixTimestamp && s.BankUnixTimestamp >= r.OpenTime) || r.RewardClaimed > r.RewardTotalEmitted {
				return errors.New("RAYCLMM_REWARD_PARAMETERS")
			}
		} else if r.RewardState != 0 || r.OpenTime != 0 || r.EndTime != 0 || r.LastUpdateTime != 0 || r.RewardTotalEmitted != 0 || r.RewardClaimed != 0 || r.TokenVault != zero || r.EmissionsPerSecondX64 != "0" || r.RewardGrowthGlobalX64 != "0" {
			return errors.New("RAYCLMM_UNINITIALIZED_REWARD")
		}
	}
	for _, a := range s.TickArrays {
		for _, t := range a.Ticks {
			if len(t.RewardGrowthsOutside) != len(s.RewardInfos) {
				return errors.New("RAYCLMM_REWARD_TICK_WIDTH")
			}
			for i, v := range t.RewardGrowthsOutside {
				if !wide(v) {
					return errors.New("RAYCLMM_U128_WIDTH")
				}
				if s.RewardInfos[i].TokenMint == zero && v != "0" {
					return errors.New("RAYCLMM_UNINITIALIZED_REWARD_GROWTH")
				}
			}
		}
	}
	return nil
}
