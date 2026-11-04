package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const OrcaProgramData = "CtXfPzz36dH5Ws4UYKZvrQ1Xqzn42ecDW6y8NKuiN8nD"
const OrcaProgram = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"

type OrcaRewardInfo struct {
	Mint                  string   `json:"mint"`
	Vault                 string   `json:"vault"`
	Extension             []uint32 `json:"extension"`
	EmissionsPerSecondX64 Amount   `json:"emissions_per_second_x64"`
	GrowthGlobalX64       Amount   `json:"growth_global_x64"`
}
type OrcaTick struct {
	RewardGrowthsOutside []Amount `json:"reward_growths_outside,omitempty"`
	Tick                 int32    `json:"tick"`
	LiquidityGross       Amount   `json:"liquidity_gross"`
	LiquidityNet         string   `json:"liquidity_net"`
	FeeGrowthOutside0    Amount   `json:"fee_growth_outside0"`
	FeeGrowthOutside1    Amount   `json:"fee_growth_outside1"`
}
type OrcaTickArray struct {
	Address           string     `json:"address"`
	StartTickIndex    int32      `json:"start_tick_index"`
	Layout            string     `json:"layout"`
	InitializedBitmap Amount     `json:"initialized_bitmap"`
	Ticks             []OrcaTick `json:"ticks"`
}
type OrcaWhirlpoolState struct {
	Adaptive                   *OrcaAdaptiveState `json:"adaptive,omitempty"`
	RewardInfos                []OrcaRewardInfo   `json:"reward_infos,omitempty"`
	ProgramID                  string             `json:"program_id"`
	Programdata                string             `json:"programdata"`
	ProgramCodeSHA256          string             `json:"program_code_sha256"`
	DeployedSlot               uint64             `json:"deployed_slot"`
	TokenProgramCodeSHA256     string             `json:"token_program_code_sha256"`
	WhirlpoolsConfig           string             `json:"whirlpools_config"`
	WhirlpoolBump              uint32             `json:"whirlpool_bump"`
	Oracle                     string             `json:"oracle"`
	Vault0                     string             `json:"vault0"`
	Vault1                     string             `json:"vault1"`
	RawVault0                  uint64             `json:"raw_vault0"`
	RawVault1                  uint64             `json:"raw_vault1"`
	Mint0Decimals              uint32             `json:"mint0_decimals"`
	Mint1Decimals              uint32             `json:"mint1_decimals"`
	BankSlot                   uint64             `json:"bank_slot"`
	BankUnixTimestamp          uint64             `json:"bank_unix_timestamp"`
	BankEpoch                  uint64             `json:"bank_epoch"`
	FeeTierIndex               uint32             `json:"fee_tier_index"`
	FeeRate                    uint32             `json:"fee_rate"`
	ProtocolFeeRate            uint32             `json:"protocol_fee_rate"`
	SqrtPriceX64               Amount             `json:"sqrt_price_x64"`
	TickCurrentIndex           int32              `json:"tick_current_index"`
	Liquidity                  Amount             `json:"liquidity"`
	TickSpacing                uint32             `json:"tick_spacing"`
	ProtocolFeeOwed0           uint64             `json:"protocol_fee_owed0"`
	ProtocolFeeOwed1           uint64             `json:"protocol_fee_owed1"`
	FeeGrowthGlobal0           Amount             `json:"fee_growth_global0"`
	FeeGrowthGlobal1           Amount             `json:"fee_growth_global1"`
	RewardLastUpdatedTimestamp uint64             `json:"reward_last_updated_timestamp"`
	TickArrays                 []OrcaTickArray    `json:"tick_arrays"`
	MaxSteps                   uint32             `json:"max_steps"`
}

func (s OrcaWhirlpoolState) validateWire() error {
	if e := s.validateRewardState(); e != nil {
		return e
	}
	if s.ProgramID != OrcaProgram || s.Programdata != OrcaProgramData || s.WhirlpoolBump > 255 || s.Mint0Decimals > 255 || s.Mint1Decimals > 255 || s.DeployedSlot > s.BankSlot || s.BankUnixTimestamp > 9223372036854775807 || s.RewardLastUpdatedTimestamp > s.BankUnixTimestamp || s.TickSpacing == 0 || s.TickSpacing > 65535 || s.FeeTierIndex > 65535 || (s.Adaptive == nil && s.FeeTierIndex != s.TickSpacing) || (s.Adaptive != nil && s.FeeTierIndex == s.TickSpacing) || s.FeeRate > 60000 || s.ProtocolFeeRate > 2500 || s.MaxSteps == 0 || s.MaxSteps > 8192 || len(s.TickArrays) < 3 || len(s.TickArrays) > 7 {
		return errors.New("ORCA_STATIC_PARAMETERS")
	}
	if e := s.validateAdaptiveState(); e != nil {
		return e
	}
	seen := map[string]bool{}
	for _, key := range []string{s.Programdata, s.WhirlpoolsConfig, s.Oracle, s.Vault0, s.Vault1} {
		if ValidateSolanaPubkey(key) != nil || seen[key] {
			return errors.New("ORCA_ACCOUNT_IDENTITY")
		}
		seen[key] = true
	}
	for _, h := range []string{s.ProgramCodeSHA256, s.TokenProgramCodeSHA256} {
		b, e := hex.DecodeString(h)
		if e != nil || len(b) != 32 || h != strings.ToLower(h) {
			return errors.New("ORCA_CODE_IDENTITY")
		}
	}
	amount128 := func(a Amount) (*big.Int, error) {
		v, e := ParseAmount(a.String())
		if e != nil || v.Big().BitLen() > 128 {
			return nil, errors.New("ORCA_U128_WIDTH")
		}
		return v.Big(), nil
	}
	price, e := amount128(s.SqrtPriceX64)
	if e != nil {
		return e
	}
	for _, v := range []Amount{s.Liquidity, s.FeeGrowthGlobal0, s.FeeGrowthGlobal1} {
		if _, e = amount128(v); e != nil {
			return e
		}
	}
	if s.TickCurrentIndex < OrcaMinTick-1 || s.TickCurrentIndex > OrcaMaxTick {
		return errors.New("ORCA_TICK_RANGE")
	}
	min, _ := OrcaSqrtPriceAtTick(OrcaMinTick)
	max, _ := OrcaSqrtPriceAtTick(OrcaMaxTick)
	if price.Cmp(min) < 0 || price.Cmp(max) > 0 {
		return errors.New("ORCA_PRICE_RANGE")
	}
	if s.TickCurrentIndex >= OrcaMinTick {
		lo, _ := OrcaSqrtPriceAtTick(s.TickCurrentIndex)
		if price.Cmp(lo) < 0 {
			return errors.New("ORCA_PRICE_TICK")
		}
	}
	if s.TickCurrentIndex < OrcaMaxTick {
		hi, _ := OrcaSqrtPriceAtTick(s.TickCurrentIndex + 1)
		if price.Cmp(hi) > 0 {
			return errors.New("ORCA_PRICE_TICK")
		}
	}
	span := int64(88) * int64(s.TickSpacing)
	minStart := int64(OrcaMinTick) / span
	if int64(OrcaMinTick)%span != 0 {
		minStart--
	}
	minStart *= span
	for i, a := range s.TickArrays {
		start := int64(a.StartTickIndex)
		if ValidateSolanaPubkey(a.Address) != nil || seen[a.Address] || a.Layout != "fixed" && a.Layout != "dynamic" || start%span != 0 || start < minStart || start > int64(OrcaMaxTick) || i > 0 && start != int64(s.TickArrays[i-1].StartTickIndex)+span {
			return errors.New("ORCA_ARRAY_SEQUENCE")
		}
		seen[a.Address] = true
		bitmap, e := ParseAmount(a.InitializedBitmap.String())
		if e != nil || bitmap.Big().BitLen() > 88 {
			return errors.New("ORCA_ARRAY_BITMAP")
		}
		expected := []int32{}
		for bit := 0; bit < 88; bit++ {
			if bitmap.Big().Bit(bit) == 1 {
				tick := start + int64(bit)*int64(s.TickSpacing)
				if tick < int64(OrcaMinTick) || tick > int64(OrcaMaxTick) {
					return errors.New("ORCA_TICK_RANGE")
				}
				expected = append(expected, int32(tick))
			}
		}
		if len(expected) != len(a.Ticks) {
			return errors.New("ORCA_INCOMPLETE_TICKS")
		}
		for n, t := range a.Ticks {
			gross, e := amount128(t.LiquidityGross)
			if e != nil || gross.Sign() == 0 || t.Tick != expected[n] {
				return errors.New("ORCA_TICK_GROSS")
			}
			net, e := ParseLiquidityNet(t.LiquidityNet)
			if e != nil || new(big.Int).Abs(net).Cmp(gross) > 0 {
				return errors.New("ORCA_TICK_NET")
			}
			for _, v := range []Amount{t.FeeGrowthOutside0, t.FeeGrowthOutside1} {
				if _, e = amount128(v); e != nil {
					return e
				}
			}
		}
	}
	low := int64(s.TickArrays[0].StartTickIndex)
	hi := int64(s.TickArrays[len(s.TickArrays)-1].StartTickIndex) + span
	if int64(s.TickCurrentIndex) < low-1 || int64(s.TickCurrentIndex) >= hi {
		return errors.New("ORCA_CURRENT_TICK_UNPROVEN")
	}
	return nil
}
func (s OrcaWhirlpoolState) Validate(p Pool) error {
	if p.Protocol == "orca-whirlpools-adaptive" || p.Protocol == "orca-whirlpools-adaptive-rewards" {
		for _, array := range s.TickArrays {
			if array.Layout != "fixed" {
				return errors.New("ORCA_ADAPTIVE_ARRAY_LAYOUT")
			}
		}
	}
	if p.Protocol == "orca-whirlpools-adaptive" {
		for _, reward := range s.RewardInfos {
			if reward.Mint != "11111111111111111111111111111111" {
				return errors.New("ORCA_ADAPTIVE_REWARD_INITIALIZED")
			}
		}
	}
	if e := s.validateWire(); e != nil {
		return e
	}
	if !strings.HasPrefix(p.ChainKey, "solana:") || ValidateChain(p.ChainKey) != nil || !((p.Protocol == "orca-whirlpools-static" && p.AdapterVersion == "orca-whirlpools/cc683ec9-spl-static-1" && len(s.RewardInfos) == 0 && s.Adaptive == nil) || (p.Protocol == "orca-whirlpools-rewards" && p.AdapterVersion == "orca-whirlpools/cc683ec9-spl-rewards-1" && len(s.RewardInfos) == 3 && s.Adaptive == nil) || (p.Protocol == "orca-whirlpools-adaptive" && p.AdapterVersion == "orca-whirlpools/cc683ec9-spl-adaptive-1" && s.Adaptive != nil && s.Adaptive.Whirlpool == p.PoolID) || (p.Protocol == "orca-whirlpools-adaptive-rewards" && p.AdapterVersion == "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1" && s.Adaptive != nil && s.Adaptive.Whirlpool == p.PoolID) || (p.Protocol == "orca-whirlpools-adaptive-dynamic" && p.AdapterVersion == "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1" && s.Adaptive != nil && s.Adaptive.Whirlpool == p.PoolID)) || p.FeeNumerator != uint64(s.FeeRate) || p.FeeDenominator != 1000000 || p.Reserve0 != "0" || p.Reserve1 != "0" || p.TokenBehavior != "standard" || p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil {
		return errors.New("ORCA_POOL_IDENTITY")
	}
	seen := map[string]bool{}
	for _, key := range []string{p.PoolID, p.Token0, p.Token1, s.ProgramID, s.Programdata, s.WhirlpoolsConfig, s.Oracle, s.Vault0, s.Vault1, RaydiumTokenProgram, SolanaClock} {
		if ValidateSolanaPubkey(key) != nil || seen[key] {
			return errors.New("ORCA_ACCOUNT_ALIAS")
		}
		seen[key] = true
	}
	if SolanaPubkeyOrder(p.Token0, p.Token1) >= 0 {
		return errors.New("ORCA_MINT_ORDER")
	}
	for _, a := range s.TickArrays {
		if seen[a.Address] {
			return errors.New("ORCA_ACCOUNT_ALIAS")
		}
	}
	return nil
}

func (s OrcaWhirlpoolState) validateRewardState() error {
	if len(s.RewardInfos) != 0 && len(s.RewardInfos) != 3 {
		return errors.New("ORCA_REWARD_COUNT")
	}
	zero := "11111111111111111111111111111111"
	amount128 := func(a Amount) bool { n, e := ParseAmount(string(a)); return e == nil && n.Big().BitLen() <= 128 }
	for i, r := range s.RewardInfos {
		if (r.Mint != zero && ValidateSolanaPubkey(r.Mint) != nil) || (r.Vault != zero && ValidateSolanaPubkey(r.Vault) != nil) || !amount128(r.EmissionsPerSecondX64) || !amount128(r.GrowthGlobalX64) {
			return errors.New("ORCA_REWARD_PARAMETERS")
		}
		if len(r.Extension) != 32 {
			return errors.New("ORCA_REWARD_CONTROL_UNSUPPORTED")
		}
		for _, v := range r.Extension {
			if v > 255 || i > 0 && v != 0 {
				return errors.New("ORCA_REWARD_CONTROL_UNSUPPORTED")
			}
		}
		if r.Mint != zero {
			if r.Vault == zero {
				return errors.New("ORCA_REWARD_VAULT")
			}
		} else if r.Vault != zero || r.EmissionsPerSecondX64 != "0" || r.GrowthGlobalX64 != "0" {
			return errors.New("ORCA_UNINITIALIZED_REWARD")
		}
	}
	for _, a := range s.TickArrays {
		for _, t := range a.Ticks {
			if len(t.RewardGrowthsOutside) != len(s.RewardInfos) {
				return errors.New("ORCA_REWARD_TICK_WIDTH")
			}
			for i, v := range t.RewardGrowthsOutside {
				if !amount128(v) {
					return errors.New("ORCA_U128_WIDTH")
				}
				if s.RewardInfos[i].Mint == zero && v != "0" {
					return errors.New("ORCA_UNINITIALIZED_REWARD_GROWTH")
				}
			}
		}
	}
	return nil
}
