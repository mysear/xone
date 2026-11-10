package domain

import "encoding/json"

type SlipstreamDynamicFee struct {
	BaseFee              uint32 `json:"base_fee"`
	FeeCap               uint32 `json:"fee_cap"`
	ScalingFactor        uint64 `json:"scaling_factor"`
	InitialFeeEnabled    bool   `json:"initial_fee_enabled"`
	InitialFee           uint32 `json:"initial_fee"`
	DefaultScalingFactor uint64 `json:"default_scaling_factor"`
	DefaultFeeCap        uint32 `json:"default_fee_cap"`
	SecondsAgo           uint32 `json:"seconds_ago"`
}

func (s *SlipstreamDynamicFee) UnmarshalJSON(raw []byte) error {
	type alias SlipstreamDynamicFee
	var out alias
	required := []string{"base_fee", "fee_cap", "scaling_factor", "initial_fee_enabled", "initial_fee", "default_scaling_factor", "default_fee_cap", "seconds_ago"}
	if e := lfjStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = SlipstreamDynamicFee(out)
	return nil
}

type SlipstreamModule struct {
	Kind           string                `json:"kind"`
	Address        string                `json:"address"`
	CodeSHA256     string                `json:"code_sha256"`
	TemplateSHA256 string                `json:"template_sha256"`
	Factory        string                `json:"factory"`
	CustomFee      uint32                `json:"custom_fee"`
	Dynamic        *SlipstreamDynamicFee `json:"dynamic,omitempty"`
}

func (s *SlipstreamModule) UnmarshalJSON(raw []byte) error {
	type alias SlipstreamModule
	var out alias
	required := []string{"kind", "address", "code_sha256", "template_sha256", "factory", "custom_fee"}
	var present map[string]json.RawMessage
	if e := json.Unmarshal(raw, &present); e != nil {
		return e
	}
	if _, ok := present["dynamic"]; ok {
		required = append(required, "dynamic")
	}
	if e := lfjStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = SlipstreamModule(out)
	return nil
}

type SlipstreamOrigin struct {
	Origin          string `json:"origin"`
	Discount        uint32 `json:"discount"`
	ViewSwapFee     uint32 `json:"view_swap_fee"`
	ViewUnstakedFee uint32 `json:"view_unstaked_fee"`
}

func (s *SlipstreamOrigin) UnmarshalJSON(raw []byte) error {
	type alias SlipstreamOrigin
	var out alias
	required := []string{"origin", "discount", "view_swap_fee", "view_unstaked_fee"}
	if e := lfjStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = SlipstreamOrigin(out)
	return nil
}

type SlipstreamTick struct {
	Tick                           int32  `json:"tick"`
	LiquidityGross                 Amount `json:"liquidity_gross"`
	LiquidityNet                   string `json:"liquidity_net"`
	StakedLiquidityNet             string `json:"staked_liquidity_net"`
	FeeGrowthOutside0              Amount `json:"fee_growth_outside0"`
	FeeGrowthOutside1              Amount `json:"fee_growth_outside1"`
	RewardGrowthOutside            Amount `json:"reward_growth_outside"`
	TickCumulativeOutside          string `json:"tick_cumulative_outside"`
	SecondsPerLiquidityOutsideX128 Amount `json:"seconds_per_liquidity_outside_x128"`
	SecondsOutside                 uint32 `json:"seconds_outside"`
	Initialized                    bool   `json:"initialized"`
}

func (s *SlipstreamTick) UnmarshalJSON(raw []byte) error {
	type alias SlipstreamTick
	var out alias
	required := []string{"tick", "liquidity_gross", "liquidity_net", "staked_liquidity_net", "fee_growth_outside0", "fee_growth_outside1", "reward_growth_outside", "tick_cumulative_outside", "seconds_per_liquidity_outside_x128", "seconds_outside", "initialized"}
	if e := lfjStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = SlipstreamTick(out)
	return nil
}

type SlipstreamObservation struct {
	BlockTimestamp                    uint32 `json:"block_timestamp"`
	TickCumulative                    string `json:"tick_cumulative"`
	SecondsPerLiquidityCumulativeX128 Amount `json:"seconds_per_liquidity_cumulative_x128"`
	Initialized                       bool   `json:"initialized"`
}

func (s *SlipstreamObservation) UnmarshalJSON(raw []byte) error {
	type alias SlipstreamObservation
	var out alias
	required := []string{"block_timestamp", "tick_cumulative", "seconds_per_liquidity_cumulative_x128", "initialized"}
	if e := lfjStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = SlipstreamObservation(out)
	return nil
}

type SlipstreamState struct {
	Core                         *V3State                `json:"core"`
	ImplementationTemplateSHA256 string                  `json:"implementation_template_sha256"`
	FactoryTemplateSHA256        string                  `json:"factory_template_sha256"`
	Token0CodeSHA256             string                  `json:"token0_code_sha256"`
	Token1CodeSHA256             string                  `json:"token1_code_sha256"`
	FactoryVoter                 string                  `json:"factory_voter"`
	VoterCodeSHA256              string                  `json:"voter_code_sha256"`
	VoterGauge                   string                  `json:"voter_gauge"`
	VoterGaugeAlive              bool                    `json:"voter_gauge_alive"`
	FactoryDefaultFee            uint32                  `json:"factory_default_fee"`
	FactoryDefaultUnstakedFee    uint32                  `json:"factory_default_unstaked_fee"`
	FactoryRegistered            bool                    `json:"factory_registered"`
	SwapFeeModule                SlipstreamModule        `json:"swap_fee_module"`
	UnstakedFeeModule            SlipstreamModule        `json:"unstaked_fee_module"`
	Origins                      []SlipstreamOrigin      `json:"origins"`
	ViewOrigin                   string                  `json:"view_origin"`
	ViewSwapFee                  uint32                  `json:"view_swap_fee"`
	ViewUnstakedFee              uint32                  `json:"view_unstaked_fee"`
	RawBalance0                  Amount                  `json:"raw_balance0"`
	RawBalance1                  Amount                  `json:"raw_balance1"`
	FeeGrowthGlobal0             Amount                  `json:"fee_growth_global0"`
	FeeGrowthGlobal1             Amount                  `json:"fee_growth_global1"`
	RewardGrowthGlobal           Amount                  `json:"reward_growth_global"`
	GaugeFees0                   Amount                  `json:"gauge_fees0"`
	GaugeFees1                   Amount                  `json:"gauge_fees1"`
	RewardRate                   Amount                  `json:"reward_rate"`
	RewardReserve                Amount                  `json:"reward_reserve"`
	PeriodFinish                 Amount                  `json:"period_finish"`
	Rollover                     Amount                  `json:"rollover"`
	StakedLiquidity              Amount                  `json:"staked_liquidity"`
	LastUpdated                  uint32                  `json:"last_updated"`
	MaxLiquidityPerTick          Amount                  `json:"max_liquidity_per_tick"`
	Gauge                        string                  `json:"gauge"`
	NFT                          string                  `json:"nft"`
	FactoryRegistry              string                  `json:"factory_registry"`
	ObservationIndex             uint32                  `json:"observation_index"`
	ObservationCardinality       uint32                  `json:"observation_cardinality"`
	ObservationCardinalityNext   uint32                  `json:"observation_cardinality_next"`
	Unlocked                     bool                    `json:"unlocked"`
	Ticks                        []SlipstreamTick        `json:"ticks"`
	Observations                 []SlipstreamObservation `json:"observations"`
	BlockHash                    string                  `json:"block_hash"`
	BlockNumber                  uint64                  `json:"block_number"`
	Timestamp                    uint64                  `json:"timestamp"`
	MaxObservations              uint32                  `json:"max_observations"`
	Derived                      bool                    `json:"derived"`
}

func (s *SlipstreamState) UnmarshalJSON(raw []byte) error {
	type alias SlipstreamState
	var out alias
	required := []string{"core", "implementation_template_sha256", "factory_template_sha256", "token0_code_sha256", "token1_code_sha256", "factory_voter", "voter_code_sha256", "voter_gauge", "voter_gauge_alive", "factory_default_fee", "factory_default_unstaked_fee", "factory_registered", "swap_fee_module", "unstaked_fee_module", "origins", "view_origin", "view_swap_fee", "view_unstaked_fee", "raw_balance0", "raw_balance1", "fee_growth_global0", "fee_growth_global1", "reward_growth_global", "gauge_fees0", "gauge_fees1", "reward_rate", "reward_reserve", "period_finish", "rollover", "staked_liquidity", "last_updated", "max_liquidity_per_tick", "gauge", "nft", "factory_registry", "observation_index", "observation_cardinality", "observation_cardinality_next", "unlocked", "ticks", "observations", "block_hash", "block_number", "timestamp", "max_observations", "derived"}
	if e := lfjStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = SlipstreamState(out)
	return nil
}
