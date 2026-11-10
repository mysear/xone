package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const PancakeLmProtocol = "pancake-v3-lm-context"
const PancakeLmAdapter = "pancake-v3/98684794-lm-1"
const PancakePoolTemplate = "7b39db168808f3bb63aac29a396464c672dd6509dade03f7078499fc3803684c"
const PancakeLmTemplate = "3d6848ee2e6cf3d9c6857bab9a877f5bdd59a830d16a6638485745ebbcd9b197"
const PancakeChefTemplate = "c79545617ca1a25eaf407e57b1231031b574a63332e23c409af64157f896ba24"
const PancakeFactoryTemplate = "ca9caada1721c9dffa86fa20f7cf8ccc3c199e9019646f42c2e85617f896bed5"

func pancakeDigest(s string) bool {
	v, e := hex.DecodeString(s)
	return e == nil && len(v) == 32 && s == strings.ToLower(s)
}
func pancakeAddress(s string) error {
	if ValidateEVMAddress(s) != nil || s == SingletonZeroAddress {
		return errors.New("PANCAKE_LM_IDENTITY")
	}
	return nil
}
func pancakeAmount(s Amount, bits int) (*big.Int, error) {
	a, e := ParseAmount(string(s))
	if e != nil {
		return nil, e
	}
	n := a.Big()
	if n.BitLen() > bits {
		return nil, errors.New("PANCAKE_LM_AMOUNT_WIDTH")
	}
	return n, nil
}
func pancakeSigned56(s string) (*big.Int, error) {
	n, ok := new(big.Int).SetString(s, 10)
	limit := new(big.Int).Lsh(big.NewInt(1), 55)
	if !ok || n.String() != s || n.Cmp(new(big.Int).Neg(new(big.Int).Set(limit))) < 0 || n.Cmp(limit) >= 0 {
		return nil, errors.New("PANCAKE_LM_SIGNED56")
	}
	return n, nil
}
func (t *PancakeLmTick) ValidateWire() error {
	if t == nil {
		return errors.New("PANCAKE_LM_TICK_REQUIRED")
	}
	g, e := pancakeAmount(t.LiquidityGross, 128)
	if e != nil {
		return e
	}
	n, e := ParseLiquidityNet(t.LiquidityNet)
	if e != nil {
		return e
	}
	r, e := pancakeAmount(t.RewardGrowthOutside, 256)
	if e != nil {
		return e
	}
	if t.Tick < MinV3Tick || t.Tick > MaxV3Tick || new(big.Int).Abs(n).Cmp(g) > 0 || g.Sign() == 0 && r.Sign() != 0 {
		return errors.New("PANCAKE_LM_TICK_VALUES")
	}
	return nil
}
func (t *PancakeLedgerTick) ValidateWire() error {
	if t == nil {
		return errors.New("PANCAKE_LM_CORE_TICK_REQUIRED")
	}
	g, e := pancakeAmount(t.LiquidityGross, 128)
	if e != nil {
		return e
	}
	n, e := ParseLiquidityNet(t.LiquidityNet)
	if e != nil {
		return e
	}
	if !t.Initialized || g.Sign() == 0 || new(big.Int).Abs(n).Cmp(g) > 0 {
		return errors.New("PANCAKE_LM_CORE_TICK_VALUES")
	}
	for _, a := range []Amount{t.FeeGrowthOutside0, t.FeeGrowthOutside1} {
		if _, e = pancakeAmount(a, 256); e != nil {
			return e
		}
	}
	if _, e = pancakeAmount(t.SecondsPerLiquidityOutsideX128, 160); e != nil {
		return e
	}
	_, e = pancakeSigned56(t.TickCumulativeOutside)
	return e
}
func (o *PancakeObservation) ValidateWire() error {
	if o == nil {
		return errors.New("PANCAKE_LM_OBSERVATION_REQUIRED")
	}
	if _, e := pancakeSigned56(o.TickCumulative); e != nil {
		return e
	}
	a, e := pancakeAmount(o.SecondsPerLiquidityCumulativeX128, 160)
	if e != nil {
		return e
	}
	if !o.Initialized && (o.TickCumulative != "0" || a.Sign() != 0 || o.BlockTimestamp > 1) {
		return errors.New("PANCAKE_LM_UNUSED_OBSERVATION")
	}
	return nil
}
func (c *PancakeMasterChef) ValidateWire() error {
	if c == nil {
		return errors.New("PANCAKE_LM_MASTER_CHEF_REQUIRED")
	}
	for _, a := range []string{c.Address, c.Cake, c.Weth, c.PositionManager, c.PoolAddress, c.PoolToken0, c.PoolToken1} {
		if e := pancakeAddress(a); e != nil {
			return e
		}
	}
	if !pancakeDigest(c.CodeSHA256) || c.TemplateSHA256 != PancakeChefTemplate || c.PoolFee >= 1000000 {
		return errors.New("PANCAKE_LM_MASTER_CHEF_IDENTITY")
	}
	for _, a := range []Amount{c.PoolPid, c.PoolAllocPoint, c.TotalAllocPoint, c.LatestPeriodCakePerSecond, c.LatestPeriodEndTime, c.ViewRewardPerSecond, c.ViewEndTime} {
		if _, e := pancakeAmount(a, 256); e != nil {
			return e
		}
	}
	if c.PoolPid.Big().Sign() == 0 {
		return errors.New("PANCAKE_LM_POOL_PID")
	}
	rate := new(big.Int)
	if c.TotalAllocPoint.Big().Sign() != 0 {
		rate.Mul(c.LatestPeriodCakePerSecond.Big(), c.PoolAllocPoint.Big())
		if rate.BitLen() > 256 {
			return errors.New("PANCAKE_LM_MASTER_CHEF_CHECKED_OVERFLOW")
		}
		rate.Div(rate, c.TotalAllocPoint.Big())
	}
	if rate.Cmp(c.ViewRewardPerSecond.Big()) != 0 || c.ViewEndTime != c.LatestPeriodEndTime {
		return errors.New("PANCAKE_LM_REWARD_VIEW_MISMATCH")
	}
	return nil
}
func (l *PancakeLm) ValidateWire() error {
	if l == nil {
		return errors.New("PANCAKE_LM_REQUIRED")
	}
	for _, a := range []string{l.Address, l.Pool, l.MasterChef} {
		if e := pancakeAddress(a); e != nil {
			return e
		}
	}
	if !pancakeDigest(l.CodeSHA256) || l.TemplateSHA256 != PancakeLmTemplate || len(l.Ticks) > 4096 {
		return errors.New("PANCAKE_LM_IMPLEMENTATION")
	}
	if _, e := pancakeAmount(l.RewardGrowthGlobal, 256); e != nil {
		return e
	}
	if _, e := pancakeAmount(l.Liquidity, 128); e != nil {
		return e
	}
	for i := range l.Ticks {
		if e := l.Ticks[i].ValidateWire(); e != nil {
			return e
		}
		if i > 0 && l.Ticks[i].Tick <= l.Ticks[i-1].Tick {
			return errors.New("PANCAKE_LM_TICK_ORDER")
		}
	}
	return nil
}
func (s *PancakeLmState) Validate(p Pool) error {
	if s == nil || s.Core == nil {
		return errors.New("PANCAKE_LM_CONTEXT_REQUIRED")
	}
	if p.Protocol != PancakeLmProtocol || p.AdapterVersion != PancakeLmAdapter || p.TokenBehavior != "standard" || !strings.HasPrefix(p.ChainKey, "evm:") || ValidateChain(p.ChainKey) != nil || p.Reserve0 != "0" || p.Reserve1 != "0" || p.FeeNumerator >= 1000000 || p.FeeDenominator != 1000000 {
		return errors.New("PANCAKE_LM_PROFILE")
	}
	if p.Token0 >= p.Token1 {
		return errors.New("PANCAKE_LM_TOKEN_ORDER")
	}
	distinct := map[string]bool{}
	for _, a := range []string{p.PoolID, p.Token0, p.Token1, s.Core.Factory, s.Lm.Address, s.MasterChef.Address} {
		if pancakeAddress(a) != nil || distinct[a] {
			return errors.New("PANCAKE_LM_IDENTITY_ALIAS")
		}
		distinct[a] = true
	}
	c := s.MasterChef
	if s.Lm.Pool != p.PoolID || s.Lm.MasterChef != c.Address || c.PoolAddress != p.PoolID || c.PoolToken0 != p.Token0 || c.PoolToken1 != p.Token1 || uint64(c.PoolFee) != p.FeeNumerator {
		return errors.New("PANCAKE_LM_PARENT_BINDING")
	}
	return s.ValidateWire()
}
func (s *PancakeLmState) ValidateWire() error {
	if s == nil || s.Core == nil {
		return errors.New("PANCAKE_LM_CORE_REQUIRED")
	}
	if e := s.Core.Validate(); e != nil {
		return e
	}
	if s.Tick != s.Core.Tick || s.FeeProtocol != s.Core.FeeProtocol {
		return errors.New("PANCAKE_LM_CORE_OBSERVATION_BINDING")
	}
	if e := s.MasterChef.ValidateWire(); e != nil {
		return e
	}
	if e := s.Lm.ValidateWire(); e != nil {
		return e
	}
	if s.Core.Implementation != "" || s.Core.ImplementationCodeSHA256 != "" || s.PoolTemplateSHA256 != PancakePoolTemplate || s.FactoryTemplateSHA256 != PancakeFactoryTemplate || !s.FactoryRegistered || !s.Unlocked || s.FeeProtocol&65535 > 10000 || s.FeeProtocol>>16 > 10000 {
		return errors.New("PANCAKE_LM_CODE_OR_LOCK")
	}
	for _, d := range []string{s.Core.FactoryCodeSHA256, s.Token0CodeSHA256, s.Token1CodeSHA256} {
		if !pancakeDigest(d) {
			return errors.New("PANCAKE_LM_CODE_DIGEST")
		}
	}
	if len(s.BlockHash) != 66 || !strings.HasPrefix(s.BlockHash, "0x") || !pancakeDigest(s.BlockHash[2:]) || s.Timestamp > ^uint64(0)/1000 {
		return errors.New("PANCAKE_LM_BANK")
	}
	for _, a := range []Amount{s.RawBalance0, s.RawBalance1, s.FeeGrowthGlobal0, s.FeeGrowthGlobal1} {
		if _, e := pancakeAmount(a, 256); e != nil {
			return e
		}
	}
	for _, a := range []Amount{s.ProtocolFees0, s.ProtocolFees1, s.MaxLiquidityPerTick} {
		if _, e := pancakeAmount(a, 128); e != nil {
			return e
		}
	}
	if s.ProtocolFees0.Big().Cmp(s.RawBalance0.Big()) > 0 || s.ProtocolFees1.Big().Cmp(s.RawBalance1.Big()) > 0 {
		return errors.New("PANCAKE_LM_PROTOCOL_RAW_BALANCE")
	}
	spacing := s.Core.TickSpacing
	lo := (MinV3Tick / spacing) * spacing
	hi := (MaxV3Tick / spacing) * spacing
	count := (hi-lo)/spacing + 1
	maximum := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))
	maximum.Div(maximum, big.NewInt(int64(count)))
	if maximum.Cmp(s.MaxLiquidityPerTick.Big()) != 0 {
		return errors.New("PANCAKE_LM_MAX_LIQUIDITY")
	}
	if len(s.Ticks) != len(s.Core.Ticks) || len(s.Lm.Ticks) != len(s.Core.Ticks) {
		return errors.New("PANCAKE_LM_COMPLETE_TICK_SCOPE")
	}
	for i, c := range s.Core.Ticks {
		t, l := s.Ticks[i], s.Lm.Ticks[i]
		if e := t.ValidateWire(); e != nil {
			return e
		}
		if t.Tick != c.Tick || t.LiquidityGross != c.LiquidityGross || t.LiquidityNet != c.LiquidityNet || l.Tick != c.Tick || t.LiquidityGross.Big().Cmp(maximum) > 0 || l.LiquidityGross.Big().Cmp(maximum) > 0 {
			return errors.New("PANCAKE_LM_TICK_CORRESPONDENCE")
		}
	}
	if s.MaxObservations == 0 || s.MaxObservations > 4096 || s.ObservationCardinality == 0 || s.ObservationCardinality > s.ObservationCardinalityNext || s.ObservationCardinalityNext > s.MaxObservations || s.ObservationIndex >= s.ObservationCardinality || uint32(len(s.Observations)) != s.ObservationCardinalityNext {
		return errors.New("PANCAKE_LM_OBSERVATION_SCOPE")
	}
	for i := range s.Observations {
		if e := s.Observations[i].ValidateWire(); e != nil {
			return e
		}
	}
	if !s.Observations[0].Initialized || !s.Observations[s.ObservationIndex].Initialized {
		return errors.New("PANCAKE_LM_LATEST_OBSERVATION")
	}
	for _, zero := range []bool{false, true} {
		current := new(big.Int).Set(s.Lm.Liquidity.Big())
		for offset := range s.Lm.Ticks {
			i := offset
			if zero {
				i = len(s.Lm.Ticks) - 1 - offset
			}
			t := s.Lm.Ticks[i]
			if zero && t.Tick > s.Core.Tick || !zero && t.Tick <= s.Core.Tick {
				continue
			}
			net, e := ParseLiquidityNet(t.LiquidityNet)
			if e != nil {
				return e
			}
			negative := net.Sign() < 0
			m := new(big.Int).Abs(net)
			if negative != zero {
				if current.Cmp(m) < 0 {
					return errors.New("PANCAKE_LM_LIQUIDITY_CURVE")
				}
				current.Sub(current, m)
			} else {
				current.Add(current, m)
			}
			if current.BitLen() > 128 {
				return errors.New("PANCAKE_LM_LIQUIDITY_CURVE")
			}
		}
	}
	return nil
}
func (s PancakeLmState) Clone() PancakeLmState {
	out := s
	if s.Core != nil {
		c := *s.Core
		c.TickWords = append([]V3BitmapWord{}, s.Core.TickWords...)
		c.Ticks = append([]V3Tick{}, s.Core.Ticks...)
		out.Core = &c
	}
	out.Lm.Ticks = append([]PancakeLmTick{}, s.Lm.Ticks...)
	out.Ticks = append([]PancakeLedgerTick{}, s.Ticks...)
	out.Observations = append([]PancakeObservation{}, s.Observations...)
	return out
}
