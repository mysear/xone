package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const SlipstreamContextProtocol = "aerodrome-slipstream-context"
const SlipstreamContextAdapter = "aerodrome-slipstream/f8717faa-module-1"
const SlipstreamImplementationTemplate = "82642d5d25fe9bf09e9a7ef46673ca3922f4c47cbffddfc173ce4866aa65c2e4"
const SlipstreamFactoryTemplate = "0a403e2c20be57a38c85a3d298207b1b25d3ba849dcb36b43be50c7983bb7187"
const SlipstreamCustomSwapTemplate = "ddc9d6554ff8e4a364fb7965029dc7e7f000676c15650cbd336e6fe47209904c"
const SlipstreamCustomUnstakedTemplate = "7f7aa9b5f067b30673550db0232bfec1c4c21344f1f1fa48bae97189e38fcae7"
const SlipstreamDynamicSwapTemplate = "187b7edb56da74a9674a63daac04c68e4e2f5d5d497e6ee8a8695f4fcec3bcc6"
const SlipstreamVoterSHA256 = "621bb8e29a36ec7d8319e019a2ec47de06fee83e332465a893bd0b34955dff74"

func slipDigest(s string) bool {
	b, e := hex.DecodeString(s)
	return e == nil && len(b) == 32 && s == strings.ToLower(s)
}
func slipAddress(s string, nonzero bool) error {
	if e := ValidateEVMAddress(s); e != nil {
		return e
	}
	if nonzero && s == SingletonZeroAddress {
		return errors.New("SLIPSTREAM_ZERO_IDENTITY")
	}
	return nil
}
func slipAmount(value Amount, bits int) (*big.Int, error) {
	a, e := ParseAmount(string(value))
	if e != nil {
		return nil, e
	}
	n := a.Big()
	if n.BitLen() > bits {
		return nil, errors.New("SLIPSTREAM_AMOUNT_WIDTH")
	}
	return n, nil
}
func SlipstreamSigned56(s string) (*big.Int, error) {
	n, ok := new(big.Int).SetString(s, 10)
	if !ok || n.String() != s {
		return nil, errors.New("SLIPSTREAM_SIGNED56")
	}
	limit := new(big.Int).Lsh(big.NewInt(1), 55)
	if n.Cmp(new(big.Int).Neg(new(big.Int).Set(limit))) < 0 || n.Cmp(limit) >= 0 {
		return nil, errors.New("SLIPSTREAM_SIGNED56")
	}
	return n, nil
}
func (d *SlipstreamDynamicFee) ValidateWire() error {
	if d == nil || d.BaseFee > 30000 || d.FeeCap > 50000 || d.ScalingFactor > 1000000000000000000 || d.InitialFee > 50000 || d.DefaultScalingFactor > 1000000000000000000 || d.DefaultFeeCap > 50000 || d.SecondsAgo < 2 || d.SecondsAgo >= 131070 {
		return errors.New("SLIPSTREAM_DYNAMIC_FEE_PARAMETERS")
	}
	return nil
}
func (m *SlipstreamModule) ValidateWire() error {
	if m == nil {
		return errors.New("SLIPSTREAM_MODULE_REQUIRED")
	}
	if e := slipAddress(m.Address, false); e != nil {
		return e
	}
	if e := slipAddress(m.Factory, false); e != nil {
		return e
	}
	switch m.Kind {
	case "none":
		if m.Address != SingletonZeroAddress || m.Factory != SingletonZeroAddress || m.CodeSHA256 != "" || m.TemplateSHA256 != "" || m.CustomFee != 0 || m.Dynamic != nil {
			return errors.New("SLIPSTREAM_ABSENT_MODULE")
		}
	case "custom", "dynamic":
		if slipAddress(m.Address, true) != nil || slipAddress(m.Factory, true) != nil || !slipDigest(m.CodeSHA256) || !slipDigest(m.TemplateSHA256) {
			return errors.New("SLIPSTREAM_MODULE_CODE")
		}
		if m.Kind == "custom" {
			if m.Dynamic != nil || m.CustomFee > 500000 {
				return errors.New("SLIPSTREAM_CUSTOM_FEE")
			}
		} else {
			if m.CustomFee != 0 {
				return errors.New("SLIPSTREAM_DYNAMIC_CUSTOM_FIELD")
			}
			if e := m.Dynamic.ValidateWire(); e != nil {
				return e
			}
		}
	default:
		return errors.New("SLIPSTREAM_UNKNOWN_MODULE")
	}
	return nil
}
func (m *SlipstreamModule) validateRole(factory string, swap bool) error {
	if e := m.ValidateWire(); e != nil {
		return e
	}
	if m.Kind == "none" {
		return nil
	}
	if m.Factory != factory {
		return errors.New("SLIPSTREAM_MODULE_FACTORY")
	}
	expected := SlipstreamCustomUnstakedTemplate
	if m.Kind == "custom" && swap {
		if m.CustomFee > 30000 {
			return errors.New("SLIPSTREAM_CUSTOM_SWAP_RANGE")
		}
		expected = SlipstreamCustomSwapTemplate
	}
	if m.Kind == "dynamic" {
		if !swap {
			return errors.New("SLIPSTREAM_UNSTAKED_MODULE_KIND")
		}
		expected = SlipstreamDynamicSwapTemplate
	}
	if m.TemplateSHA256 != expected {
		return errors.New("SLIPSTREAM_MODULE_TEMPLATE")
	}
	return nil
}
func (o *SlipstreamOrigin) ValidateWire() error {
	if o == nil || slipAddress(o.Origin, true) != nil || o.Discount > 500000 || o.ViewSwapFee > 100000 || o.ViewUnstakedFee > 1000000 {
		return errors.New("SLIPSTREAM_ORIGIN_VALUES")
	}
	return nil
}
func (t *SlipstreamTick) ValidateWire() error {
	if t == nil {
		return errors.New("SLIPSTREAM_TICK_REQUIRED")
	}
	gross, e := slipAmount(t.LiquidityGross, 128)
	if e != nil {
		return e
	}
	net, e := ParseLiquidityNet(t.LiquidityNet)
	if e != nil {
		return e
	}
	stake, e := ParseLiquidityNet(t.StakedLiquidityNet)
	if e != nil {
		return e
	}
	if !t.Initialized || gross.Sign() == 0 || new(big.Int).Abs(net).Cmp(gross) > 0 || new(big.Int).Abs(stake).Cmp(gross) > 0 {
		return errors.New("SLIPSTREAM_TICK_VALUES")
	}
	if _, e = slipAmount(t.SecondsPerLiquidityOutsideX128, 160); e != nil {
		return e
	}
	if _, e = SlipstreamSigned56(t.TickCumulativeOutside); e != nil {
		return e
	}
	for _, v := range []Amount{t.FeeGrowthOutside0, t.FeeGrowthOutside1, t.RewardGrowthOutside} {
		if _, e = slipAmount(v, 256); e != nil {
			return e
		}
	}
	return nil
}
func (o *SlipstreamObservation) ValidateWire() error {
	if o == nil {
		return errors.New("SLIPSTREAM_OBSERVATION_REQUIRED")
	}
	if _, e := SlipstreamSigned56(o.TickCumulative); e != nil {
		return e
	}
	n, e := slipAmount(o.SecondsPerLiquidityCumulativeX128, 160)
	if e != nil {
		return e
	}
	if !o.Initialized && (o.TickCumulative != "0" || n.Sign() != 0 || o.BlockTimestamp > 1) {
		return errors.New("SLIPSTREAM_UNUSED_OBSERVATION")
	}
	return nil
}
func (s *SlipstreamState) Validate(p Pool) error {
	if p.Protocol != SlipstreamContextProtocol || p.AdapterVersion != SlipstreamContextAdapter || p.TokenBehavior != "standard" || !strings.HasPrefix(p.ChainKey, "evm:") || ValidateChain(p.ChainKey) != nil || p.Reserve0 != "0" || p.Reserve1 != "0" || p.FeeNumerator != 0 || p.FeeDenominator != 1000000 {
		return errors.New("SLIPSTREAM_CONTEXT_PROFILE")
	}
	if s == nil || s.Core == nil {
		return errors.New("SLIPSTREAM_CONTEXT_REQUIRED")
	}
	seen := map[string]bool{}
	for _, a := range append([]string{p.PoolID, p.Token0, p.Token1, s.Core.Factory, s.Core.Implementation, s.FactoryVoter}, func() []string {
		var roles []string
		for _, m := range []SlipstreamModule{s.SwapFeeModule, s.UnstakedFeeModule} {
			if m.Kind != "none" {
				roles = append(roles, m.Address)
			}
		}
		return roles
	}()...) {
		if slipAddress(a, true) != nil || seen[a] {
			return errors.New("SLIPSTREAM_IDENTITY_ALIAS")
		}
		seen[a] = true
	}
	for _, o := range s.Origins {
		if seen[o.Origin] {
			return errors.New("SLIPSTREAM_ORIGIN_CLASS")
		}
	}
	return s.ValidateWire()
}
func (s *SlipstreamState) ValidateWire() error {
	if s == nil || s.Core == nil {
		return errors.New("SLIPSTREAM_CORE_REQUIRED")
	}
	if e := s.Core.Validate(); e != nil {
		return e
	}
	if s.Core.FeeProtocol != 0 || !s.Unlocked || !s.FactoryRegistered || s.ImplementationTemplateSHA256 != SlipstreamImplementationTemplate || s.FactoryTemplateSHA256 != SlipstreamFactoryTemplate || s.VoterCodeSHA256 != SlipstreamVoterSHA256 {
		return errors.New("SLIPSTREAM_CONTEXT_CODE_OR_LOCK")
	}
	for _, a := range []string{s.Core.Implementation, s.Core.Factory, s.FactoryVoter} {
		if e := slipAddress(a, true); e != nil {
			return e
		}
	}
	for _, d := range []string{s.Core.ImplementationCodeSHA256, s.Core.FactoryCodeSHA256, s.Token0CodeSHA256, s.Token1CodeSHA256} {
		if !slipDigest(d) {
			return errors.New("SLIPSTREAM_CONTEXT_DIGEST")
		}
	}
	if len(s.BlockHash) != 66 || !strings.HasPrefix(s.BlockHash, "0x") || !slipDigest(s.BlockHash[2:]) || s.Timestamp > ^uint64(0)/1000 {
		return errors.New("SLIPSTREAM_CONTEXT_BANK")
	}
	for _, a := range []string{s.Gauge, s.NFT, s.FactoryRegistry, s.VoterGauge, s.ViewOrigin} {
		if e := slipAddress(a, false); e != nil {
			return e
		}
	}
	if s.FactoryDefaultFee == 0 || s.FactoryDefaultFee > 100000 || s.FactoryDefaultUnstakedFee > 500000 || s.ViewSwapFee > 100000 || s.ViewUnstakedFee > 1000000 {
		return errors.New("SLIPSTREAM_FACTORY_FEE_RANGE")
	}
	if e := s.SwapFeeModule.validateRole(s.Core.Factory, true); e != nil {
		return e
	}
	if e := s.UnstakedFeeModule.validateRole(s.Core.Factory, false); e != nil {
		return e
	}
	if len(s.Origins) > 16 || s.SwapFeeModule.Kind == "dynamic" && len(s.Origins) == 0 {
		return errors.New("SLIPSTREAM_ORIGIN_SCOPE")
	}
	origins := map[string]bool{}
	for i := range s.Origins {
		o := &s.Origins[i]
		if e := o.ValidateWire(); e != nil {
			return e
		}
		if origins[o.Origin] || s.SwapFeeModule.Kind != "dynamic" && o.Discount != 0 {
			return errors.New("SLIPSTREAM_ORIGIN_SCOPE")
		}
		origins[o.Origin] = true
	}
	if s.SwapFeeModule.Kind == "dynamic" && !origins[s.ViewOrigin] {
		return errors.New("SLIPSTREAM_VIEW_ORIGIN")
	}
	stake, e := slipAmount(s.StakedLiquidity, 128)
	if e != nil {
		return e
	}
	liquidity, e := slipAmount(s.Core.Liquidity, 128)
	if e != nil {
		return e
	}
	if stake.Cmp(liquidity) > 0 {
		return errors.New("SLIPSTREAM_STAKED_OR_GAUGE_RANGE")
	}
	for _, v := range []Amount{s.GaugeFees0, s.GaugeFees1} {
		if _, e := slipAmount(v, 128); e != nil {
			return e
		}
	}
	spacing := int64(s.Core.TickSpacing)
	minimum := (-887272 / spacing) * spacing
	maximum := (887272 / spacing) * spacing
	n := (maximum-minimum)/spacing + 1
	expected := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))
	expected.Div(expected, big.NewInt(n))
	limit, e := slipAmount(s.MaxLiquidityPerTick, 128)
	if e != nil {
		return e
	}
	if limit.Cmp(expected) != 0 {
		return errors.New("SLIPSTREAM_MAX_LIQUIDITY_PER_TICK")
	}
	if len(s.Ticks) != len(s.Core.Ticks) {
		return errors.New("SLIPSTREAM_FULL_TICK_SET")
	}
	for i, t := range s.Ticks {
		if e := t.ValidateWire(); e != nil {
			return e
		}
		thin := s.Core.Ticks[i]
		gross, _ := slipAmount(t.LiquidityGross, 128)
		if t.Tick != thin.Tick || t.LiquidityGross != thin.LiquidityGross || t.LiquidityNet != thin.LiquidityNet || gross.Cmp(limit) > 0 {
			return errors.New("SLIPSTREAM_FULL_TICK_PROJECTION")
		}
	}
	for _, v := range []Amount{s.RawBalance0, s.RawBalance1, s.FeeGrowthGlobal0, s.FeeGrowthGlobal1, s.RewardGrowthGlobal, s.RewardRate, s.RewardReserve, s.PeriodFinish, s.Rollover} {
		if _, e = slipAmount(v, 256); e != nil {
			return e
		}
	}
	if s.MaxObservations == 0 || s.MaxObservations > 4096 || s.ObservationCardinality == 0 || s.ObservationCardinality > s.ObservationCardinalityNext || s.ObservationCardinalityNext > s.MaxObservations || s.ObservationIndex >= s.ObservationCardinality || len(s.Observations) != int(s.ObservationCardinalityNext) {
		return errors.New("SLIPSTREAM_OBSERVATION_SCOPE")
	}
	full := true
	for i := range s.Observations {
		o := &s.Observations[i]
		if e = o.ValidateWire(); e != nil {
			return e
		}
		if i >= int(s.ObservationCardinality) && o.Initialized {
			return errors.New("SLIPSTREAM_OBSERVATION_PREFIX")
		}
		if i < int(s.ObservationCardinality) && !o.Initialized {
			full = false
		}
	}
	if !s.Observations[0].Initialized || !s.Observations[s.ObservationIndex].Initialized {
		return errors.New("SLIPSTREAM_LATEST_OBSERVATION_REQUIRED")
	}
	first := 0
	length := int(s.ObservationIndex) + 1
	if full {
		first = (int(s.ObservationIndex) + 1) % int(s.ObservationCardinality)
		length = int(s.ObservationCardinality)
	} else {
		for i := 0; i < int(s.ObservationCardinality); i++ {
			if s.Observations[i].Initialized != (i <= int(s.ObservationIndex)) {
				return errors.New("SLIPSTREAM_OBSERVATION_PREFIX")
			}
		}
	}
	var previous uint32
	for step := 0; step < length; step++ {
		o := s.Observations[(first+step)%int(s.ObservationCardinality)]
		age := uint32(s.Timestamp) - o.BlockTimestamp
		if step > 0 && previous <= age {
			return errors.New("SLIPSTREAM_OBSERVATION_TIME_ORDER")
		}
		previous = age
	}
	for _, zero := range []bool{false, true} {
		l := new(big.Int).Set(liquidity)
		staked := new(big.Int).Set(stake)
		for j := range s.Ticks {
			i := j
			if zero {
				i = len(s.Ticks) - 1 - j
			}
			t := s.Ticks[i]
			if zero && t.Tick > s.Core.Tick || !zero && t.Tick <= s.Core.Tick {
				continue
			}
			net, _ := ParseLiquidityNet(t.LiquidityNet)
			stakeNet, _ := ParseLiquidityNet(t.StakedLiquidityNet)
			if zero {
				l.Sub(l, net)
				staked.Sub(staked, stakeNet)
			} else {
				l.Add(l, net)
				staked.Add(staked, stakeNet)
			}
			if l.Sign() < 0 || staked.Sign() < 0 || l.BitLen() > 128 || staked.BitLen() > 128 || staked.Cmp(l) > 0 {
				return errors.New("SLIPSTREAM_STAKED_CURVE")
			}
		}
	}
	// Contextual dynamic-fee/Oracle consistency is independently revalidated in
	// the Rust quote domain before a source snapshot can become a quote view.
	return nil
}
func (s SlipstreamState) Clone() SlipstreamState {
	if s.Core != nil {
		s.Core = Pool{V3: s.Core}.Clone().V3
	}
	s.Ticks = append([]SlipstreamTick{}, s.Ticks...)
	s.Observations = append([]SlipstreamObservation{}, s.Observations...)
	s.Origins = append([]SlipstreamOrigin{}, s.Origins...)
	if s.SwapFeeModule.Dynamic != nil {
		d := *s.SwapFeeModule.Dynamic
		s.SwapFeeModule.Dynamic = &d
	}
	if s.UnstakedFeeModule.Dynamic != nil {
		d := *s.UnstakedFeeModule.Dynamic
		s.UnstakedFeeModule.Dynamic = &d
	}
	return s
}
