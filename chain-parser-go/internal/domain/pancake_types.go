package domain

type PancakeLmTick struct {
	Tick                int32  `json:"tick"`
	LiquidityGross      Amount `json:"liquidity_gross"`
	LiquidityNet        string `json:"liquidity_net"`
	RewardGrowthOutside Amount `json:"reward_growth_outside"`
}

func (s *PancakeLmTick) UnmarshalJSON(raw []byte) error {
	type alias PancakeLmTick
	var out alias
	required := []string{"tick", "liquidity_gross", "liquidity_net", "reward_growth_outside"}
	if e := pancakeStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = PancakeLmTick(out)
	return nil
}

type PancakeLedgerTick struct {
	Tick                           int32  `json:"tick"`
	LiquidityGross                 Amount `json:"liquidity_gross"`
	LiquidityNet                   string `json:"liquidity_net"`
	FeeGrowthOutside0              Amount `json:"fee_growth_outside0"`
	FeeGrowthOutside1              Amount `json:"fee_growth_outside1"`
	TickCumulativeOutside          string `json:"tick_cumulative_outside"`
	SecondsPerLiquidityOutsideX128 Amount `json:"seconds_per_liquidity_outside_x128"`
	SecondsOutside                 uint32 `json:"seconds_outside"`
	Initialized                    bool   `json:"initialized"`
}

func (s *PancakeLedgerTick) UnmarshalJSON(raw []byte) error {
	type alias PancakeLedgerTick
	var out alias
	required := []string{"tick", "liquidity_gross", "liquidity_net", "fee_growth_outside0", "fee_growth_outside1", "tick_cumulative_outside", "seconds_per_liquidity_outside_x128", "seconds_outside", "initialized"}
	if e := pancakeStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = PancakeLedgerTick(out)
	return nil
}

type PancakeObservation struct {
	BlockTimestamp                    uint32 `json:"block_timestamp"`
	TickCumulative                    string `json:"tick_cumulative"`
	SecondsPerLiquidityCumulativeX128 Amount `json:"seconds_per_liquidity_cumulative_x128"`
	Initialized                       bool   `json:"initialized"`
}

func (s *PancakeObservation) UnmarshalJSON(raw []byte) error {
	type alias PancakeObservation
	var out alias
	required := []string{"block_timestamp", "tick_cumulative", "seconds_per_liquidity_cumulative_x128", "initialized"}
	if e := pancakeStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = PancakeObservation(out)
	return nil
}

type PancakeMasterChef struct {
	Address                   string `json:"address"`
	CodeSHA256                string `json:"code_sha256"`
	TemplateSHA256            string `json:"template_sha256"`
	Cake                      string `json:"cake"`
	Weth                      string `json:"weth"`
	PositionManager           string `json:"position_manager"`
	PoolPid                   Amount `json:"pool_pid"`
	PoolAddress               string `json:"pool_address"`
	PoolToken0                string `json:"pool_token0"`
	PoolToken1                string `json:"pool_token1"`
	PoolFee                   uint32 `json:"pool_fee"`
	PoolAllocPoint            Amount `json:"pool_alloc_point"`
	TotalAllocPoint           Amount `json:"total_alloc_point"`
	LatestPeriodCakePerSecond Amount `json:"latest_period_cake_per_second"`
	LatestPeriodEndTime       Amount `json:"latest_period_end_time"`
	ViewRewardPerSecond       Amount `json:"view_reward_per_second"`
	ViewEndTime               Amount `json:"view_end_time"`
}

func (s *PancakeMasterChef) UnmarshalJSON(raw []byte) error {
	type alias PancakeMasterChef
	var out alias
	required := []string{"address", "code_sha256", "template_sha256", "cake", "weth", "position_manager", "pool_pid", "pool_address", "pool_token0", "pool_token1", "pool_fee", "pool_alloc_point", "total_alloc_point", "latest_period_cake_per_second", "latest_period_end_time", "view_reward_per_second", "view_end_time"}
	if e := pancakeStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = PancakeMasterChef(out)
	return nil
}

type PancakeLm struct {
	Address             string          `json:"address"`
	CodeSHA256          string          `json:"code_sha256"`
	TemplateSHA256      string          `json:"template_sha256"`
	Pool                string          `json:"pool"`
	MasterChef          string          `json:"master_chef"`
	RewardGrowthGlobal  Amount          `json:"reward_growth_global"`
	Liquidity           Amount          `json:"liquidity"`
	LastRewardTimestamp uint32          `json:"last_reward_timestamp"`
	Ticks               []PancakeLmTick `json:"ticks"`
}

func (s *PancakeLm) UnmarshalJSON(raw []byte) error {
	type alias PancakeLm
	var out alias
	required := []string{"address", "code_sha256", "template_sha256", "pool", "master_chef", "reward_growth_global", "liquidity", "last_reward_timestamp", "ticks"}
	if e := pancakeStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = PancakeLm(out)
	return nil
}

type PancakeLmState struct {
	Core                       *V3State             `json:"core"`
	Tick                       int32                `json:"tick"`
	FeeProtocol                uint32               `json:"fee_protocol"`
	PoolTemplateSHA256         string               `json:"pool_template_sha256"`
	FactoryTemplateSHA256      string               `json:"factory_template_sha256"`
	Token0CodeSHA256           string               `json:"token0_code_sha256"`
	Token1CodeSHA256           string               `json:"token1_code_sha256"`
	FactoryRegistered          bool                 `json:"factory_registered"`
	MasterChef                 PancakeMasterChef    `json:"master_chef"`
	Lm                         PancakeLm            `json:"lm"`
	RawBalance0                Amount               `json:"raw_balance0"`
	RawBalance1                Amount               `json:"raw_balance1"`
	FeeGrowthGlobal0           Amount               `json:"fee_growth_global0"`
	FeeGrowthGlobal1           Amount               `json:"fee_growth_global1"`
	ProtocolFees0              Amount               `json:"protocol_fees0"`
	ProtocolFees1              Amount               `json:"protocol_fees1"`
	MaxLiquidityPerTick        Amount               `json:"max_liquidity_per_tick"`
	ObservationIndex           uint32               `json:"observation_index"`
	ObservationCardinality     uint32               `json:"observation_cardinality"`
	ObservationCardinalityNext uint32               `json:"observation_cardinality_next"`
	Unlocked                   bool                 `json:"unlocked"`
	Ticks                      []PancakeLedgerTick  `json:"ticks"`
	Observations               []PancakeObservation `json:"observations"`
	BlockHash                  string               `json:"block_hash"`
	BlockNumber                uint64               `json:"block_number"`
	Timestamp                  uint64               `json:"timestamp"`
	MaxObservations            uint32               `json:"max_observations"`
	Derived                    bool                 `json:"derived"`
}

func (s *PancakeLmState) UnmarshalJSON(raw []byte) error {
	type alias PancakeLmState
	var out alias
	required := []string{"core", "tick", "fee_protocol", "pool_template_sha256", "factory_template_sha256", "token0_code_sha256", "token1_code_sha256", "factory_registered", "master_chef", "lm", "raw_balance0", "raw_balance1", "fee_growth_global0", "fee_growth_global1", "protocol_fees0", "protocol_fees1", "max_liquidity_per_tick", "observation_index", "observation_cardinality", "observation_cardinality_next", "unlocked", "ticks", "observations", "block_hash", "block_number", "timestamp", "max_observations", "derived"}
	if e := pancakeStrictJSON(raw, &out, required); e != nil {
		return e
	}
	*s = PancakeLmState(out)
	return nil
}
