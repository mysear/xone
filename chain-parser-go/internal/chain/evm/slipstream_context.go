package evm

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func (a *Adapter) readSlipstreamContext(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	c := p.SlipstreamContext
	if c == nil || p.Protocol != domain.SlipstreamContextProtocol || c.TokenPolicy != "standard" || c.MaxObservations == 0 || c.MaxObservations > 4096 || len(c.Origins) > 16 {
		return fail("SLIPSTREAM_CONTEXT_CONFIG")
	}
	for _, caller := range c.Origins {
		if domain.ValidateEVMAddress(caller) != nil || caller == domain.SingletonZeroAddress {
			return fail("SLIPSTREAM_ORIGIN_CONFIG")
		}
	}
	if len(anchor) != 2 || anchor["blockHash"] != block.Hash || anchor["requireCanonical"] != true {
		return fail("SLIPSTREAM_CONTEXT_FIXED_BANK")
	}
	for _, address := range []string{p.ID, p.Token0, p.Token1, c.ExpectedFactory, c.ExpectedImplementation, c.ExpectedVoter} {
		if domain.ValidateEVMAddress(address) != nil || address == domain.SingletonZeroAddress {
			return fail("SLIPSTREAM_CONTEXT_IDENTITY_CONFIG")
		}
	}
	origin := domain.SingletonZeroAddress
	if len(c.Origins) > 0 {
		origin = c.Origins[0]
	}
	if c.SwapFeeModule.Kind == "dynamic" && len(c.Origins) == 0 {
		return fail("SLIPSTREAM_ORIGIN_SCOPE_CONFIG")
	}
	callFrom := func(to, signature, args string, words int, from string) ([]byte, error) {
		var value string
		if e := a.Client.Call(ctx, "eth_call", []any{map[string]string{"to": to, "data": singletonSelector(signature) + args, "from": from}, anchor}, &value); e != nil {
			return nil, e
		}
		if !strings.HasPrefix(value, "0x") {
			return nil, errors.New("SLIPSTREAM_ABI_DATA_PREFIX")
		}
		data, e := hex.DecodeString(value[2:])
		if e != nil || len(data) != words*32 {
			return nil, errors.New("SLIPSTREAM_ABI_SIZE")
		}
		return data, nil
	}
	call := func(to, signature, args string, words int) ([]byte, error) {
		return callFrom(to, signature, args, words, origin)
	}
	scalarFrom := func(to, signature, args string, bits int, from string) (*big.Int, error) {
		data, e := callFrom(to, signature, args, 1, from)
		if e != nil {
			return nil, e
		}
		return wordUint(data, bits)
	}
	scalar := func(to, signature, args string, bits int) (*big.Int, error) {
		return scalarFrom(to, signature, args, bits, origin)
	}
	addr := func(to, signature, args string) (string, error) {
		n, e := scalar(to, signature, args, 160)
		if e != nil {
			return "", e
		}
		return fmt.Sprintf("0x%040x", n), nil
	}
	word := func(n uint64) string { return fmt.Sprintf("%064x", n) }
	addressWord := func(s string) string { return strings.Repeat("0", 24) + s[2:] }
	factoryCode, e := a.verifyCode(ctx, c.ExpectedFactory, c.ExpectedFactoryCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	voter, e := addr(c.ExpectedFactory, "voter()", "")
	if e != nil {
		return domain.Pool{}, e
	}
	implementation, e := addr(c.ExpectedFactory, "poolImplementation()", "")
	if e != nil {
		return domain.Pool{}, e
	}
	registry, e := addr(c.ExpectedFactory, "factoryRegistry()", "")
	if e != nil {
		return domain.Pool{}, e
	}
	if voter != c.ExpectedVoter || implementation != c.ExpectedImplementation {
		return fail("SLIPSTREAM_FACTORY_IMMUTABLE_GETTERS")
	}
	factoryTemplate, e := validateSlipstreamRuntime("factory", factoryCode, map[string]string{"voter": voter, "pool_implementation": implementation, "factory_registry": registry})
	if e != nil {
		return domain.Pool{}, e
	}
	implCode, e := a.verifyCode(ctx, implementation, c.ExpectedImplementationCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	implTemplate, e := validateSlipstreamRuntime("implementation", implCode, nil)
	if e != nil {
		return domain.Pool{}, e
	}
	voterCode, e := a.verifyCode(ctx, voter, c.ExpectedVoterCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if _, e = validateSlipstreamRuntime("voter", voterCode, nil); e != nil {
		return domain.Pool{}, e
	}
	module := func(cfg config.SlipstreamModuleConfig, swap bool) (domain.SlipstreamModule, error) {
		selector := "unstakedFeeModule()"
		if swap {
			selector = "swapFeeModule()"
		}
		observed, e := addr(c.ExpectedFactory, selector, "")
		if e != nil {
			return domain.SlipstreamModule{}, e
		}
		if observed != cfg.Address {
			return domain.SlipstreamModule{}, errors.New("SLIPSTREAM_MODULE_POINTER")
		}
		if cfg.Kind == "none" {
			if observed != domain.SingletonZeroAddress || cfg.ExpectedCodeSHA256 != "" {
				return domain.SlipstreamModule{}, errors.New("SLIPSTREAM_ABSENT_MODULE_CONFIG")
			}
			return domain.SlipstreamModule{Kind: "none", Address: domain.SingletonZeroAddress, Factory: domain.SingletonZeroAddress}, nil
		}
		kind := "custom_unstaked"
		if swap {
			kind = "custom_swap"
		}
		if cfg.Kind == "dynamic" {
			if !swap {
				return domain.SlipstreamModule{}, errors.New("SLIPSTREAM_UNSTAKED_MODULE_KIND")
			}
			kind = "dynamic_swap"
		} else if cfg.Kind != "custom" {
			return domain.SlipstreamModule{}, errors.New("SLIPSTREAM_UNKNOWN_MODULE")
		}
		raw, e := a.verifyCode(ctx, observed, cfg.ExpectedCodeSHA256, anchor)
		if e != nil {
			return domain.SlipstreamModule{}, e
		}
		template, e := validateSlipstreamRuntime(kind, raw, map[string]string{"factory": c.ExpectedFactory})
		if e != nil {
			return domain.SlipstreamModule{}, e
		}
		bound, e := addr(observed, "factory()", "")
		if e != nil {
			return domain.SlipstreamModule{}, e
		}
		if bound != c.ExpectedFactory {
			return domain.SlipstreamModule{}, errors.New("SLIPSTREAM_MODULE_FACTORY")
		}
		out := domain.SlipstreamModule{Kind: cfg.Kind, Address: observed, CodeSHA256: cfg.ExpectedCodeSHA256, TemplateSHA256: template, Factory: bound}
		if cfg.Kind == "custom" {
			value, e := scalar(observed, "customFee(address)", addressWord(p.ID), 24)
			if e != nil {
				return out, e
			}
			out.CustomFee = uint32(value.Uint64())
		} else {
			data, e := call(observed, "dynamicFeeConfig(address)", addressWord(p.ID), 5)
			if e != nil {
				return out, e
			}
			values := make([]uint64, 5)
			for i, bits := range []int{24, 24, 64, 1, 24} {
				n, e := wordUint(data[i*32:(i+1)*32], bits)
				if e != nil {
					return out, e
				}
				values[i] = n.Uint64()
			}
			scale, e := scalar(observed, "defaultScalingFactor()", "", 64)
			if e != nil {
				return out, e
			}
			cap, e := scalar(observed, "defaultFeeCap()", "", 24)
			if e != nil {
				return out, e
			}
			seconds, e := scalar(observed, "secondsAgo()", "", 32)
			if e != nil {
				return out, e
			}
			out.Dynamic = &domain.SlipstreamDynamicFee{BaseFee: uint32(values[0]), FeeCap: uint32(values[1]), ScalingFactor: values[2], InitialFeeEnabled: values[3] != 0, InitialFee: uint32(values[4]), DefaultScalingFactor: scale.Uint64(), DefaultFeeCap: uint32(cap.Uint64()), SecondsAgo: uint32(seconds.Uint64())}
		}
		return out, out.ValidateWire()
	}
	swap, e := module(c.SwapFeeModule, true)
	if e != nil {
		return domain.Pool{}, e
	}
	unstaked, e := module(c.UnstakedFeeModule, false)
	if e != nil {
		return domain.Pool{}, e
	}
	registered, e := scalar(c.ExpectedFactory, "isPool(address)", addressWord(p.ID), 1)
	if e != nil {
		return domain.Pool{}, e
	}
	if registered.Sign() == 0 {
		return fail("SLIPSTREAM_FACTORY_MEMBERSHIP")
	}

	coreConfig := p
	coreConfig.Protocol = "aerodrome-slipstream"
	coreConfig.SlipstreamContext = nil
	coreConfig.V3 = &config.V3PoolConfig{ExpectedFactory: c.ExpectedFactory, ExpectedFactoryCodeSHA256: c.ExpectedFactoryCodeSHA256, ExpectedCodeSHA256: c.ExpectedCodeSHA256, ExpectedImplementation: c.ExpectedImplementation, ExpectedImplementationCodeSHA256: c.ExpectedImplementationCodeSHA256, TickWordRadius: c.TickWordRadius, MaxInitializedTicks: c.MaxInitializedTicks, MaxSteps: c.MaxSteps}
	thin, e := a.readV3Context(ctx, coreConfig, anchor, v3ReadContext{allowKnownSlipstreamModules: true, origin: origin})
	if e != nil {
		return domain.Pool{}, e
	}
	registeredPool, e := addr(c.ExpectedFactory, "getPool(address,address,int24)", addressWord(p.Token0)+addressWord(p.Token1)+word(uint64(thin.V3.TickSpacing)))
	if e != nil {
		return domain.Pool{}, e
	}
	if registeredPool != p.ID {
		return fail("SLIPSTREAM_FACTORY_POOL_KEY")
	}
	slot, e := call(p.ID, "slot0()", "", 6)
	if e != nil {
		return domain.Pool{}, e
	}
	indices := []uint64{}
	for i, bits := range []int{160, 24, 16, 16, 16, 1} {
		if i == 1 {
			n, e := wordInt(slot[32:64], 24)
			if e != nil {
				return domain.Pool{}, e
			}
			if n.Int64() != int64(thin.V3.Tick) {
				return fail("SLIPSTREAM_SLOT0_SOURCE_MISMATCH")
			}
			continue
		}
		n, e := wordUint(slot[i*32:(i+1)*32], bits)
		if e != nil {
			return domain.Pool{}, e
		}
		if i == 0 {
			if n.String() != thin.V3.SqrtPriceX96.String() {
				return fail("SLIPSTREAM_SLOT0_SOURCE_MISMATCH")
			}
		} else {
			indices = append(indices, n.Uint64())
		}
	}
	if len(indices) != 4 || indices[3] != 1 || indices[1] == 0 || indices[0] >= indices[1] || indices[1] > indices[2] || indices[2] > uint64(c.MaxObservations) {
		return fail("SLIPSTREAM_OBSERVATION_SCOPE")
	}
	state := &domain.SlipstreamState{Core: thin.V3, ImplementationTemplateSHA256: implTemplate, FactoryTemplateSHA256: factoryTemplate, Token0CodeSHA256: c.ExpectedToken0CodeSHA256, Token1CodeSHA256: c.ExpectedToken1CodeSHA256, FactoryVoter: voter, VoterCodeSHA256: c.ExpectedVoterCodeSHA256, SwapFeeModule: swap, UnstakedFeeModule: unstaked, FactoryRegistered: true, Origins: []domain.SlipstreamOrigin{}, ViewOrigin: origin, ViewSwapFee: uint32(thin.FeeNumerator), ObservationIndex: uint32(indices[0]), ObservationCardinality: uint32(indices[1]), ObservationCardinalityNext: uint32(indices[2]), Unlocked: true, Ticks: []domain.SlipstreamTick{}, Observations: []domain.SlipstreamObservation{}, BlockHash: block.Hash, BlockNumber: block.Number, Timestamp: block.TimestampMS / 1000, MaxObservations: c.MaxObservations}
	for _, target := range []struct {
		address, digest string
		amount          *domain.Amount
	}{{p.Token0, c.ExpectedToken0CodeSHA256, &state.RawBalance0}, {p.Token1, c.ExpectedToken1CodeSHA256, &state.RawBalance1}} {
		if _, e = a.verifyCode(ctx, target.address, target.digest, anchor); e != nil {
			return domain.Pool{}, e
		}
		n, e := scalar(target.address, "balanceOf(address)", addressWord(p.ID), 256)
		if e != nil {
			return domain.Pool{}, e
		}
		*target.amount = domain.Amount(n.String())
	}
	for _, spec := range []struct {
		signature string
		address   *string
	}{{"gauge()", &state.Gauge}, {"nft()", &state.NFT}, {"factoryRegistry()", &state.FactoryRegistry}} {
		s, e := addr(p.ID, spec.signature, "")
		if e != nil {
			return domain.Pool{}, e
		}
		*spec.address = s
	}
	if state.FactoryRegistry != registry {
		return fail("SLIPSTREAM_REGISTRY_SOURCE_MISMATCH")
	}
	state.VoterGauge, e = addr(voter, "gauges(address)", addressWord(p.ID))
	if e != nil {
		return domain.Pool{}, e
	}
	alive, e := scalar(voter, "isAlive(address)", addressWord(state.VoterGauge), 1)
	if e != nil {
		return domain.Pool{}, e
	}
	state.VoterGaugeAlive = alive.Sign() != 0
	defaultFee, e := scalar(c.ExpectedFactory, "tickSpacingToFee(int24)", word(uint64(thin.V3.TickSpacing)), 24)
	if e != nil {
		return domain.Pool{}, e
	}
	defaultUnstaked, e := scalar(c.ExpectedFactory, "defaultUnstakedFee()", "", 24)
	if e != nil {
		return domain.Pool{}, e
	}
	state.FactoryDefaultFee = uint32(defaultFee.Uint64())
	state.FactoryDefaultUnstakedFee = uint32(defaultUnstaked.Uint64())
	for _, spec := range []struct {
		signature string
		value     *domain.Amount
		bits      int
	}{{"feeGrowthGlobal0X128()", &state.FeeGrowthGlobal0, 256}, {"feeGrowthGlobal1X128()", &state.FeeGrowthGlobal1, 256}, {"rewardGrowthGlobalX128()", &state.RewardGrowthGlobal, 256}, {"rewardRate()", &state.RewardRate, 256}, {"rewardReserve()", &state.RewardReserve, 256}, {"periodFinish()", &state.PeriodFinish, 256}, {"rollover()", &state.Rollover, 256}, {"stakedLiquidity()", &state.StakedLiquidity, 128}, {"maxLiquidityPerTick()", &state.MaxLiquidityPerTick, 128}} {
		n, e := scalar(p.ID, spec.signature, "", spec.bits)
		if e != nil {
			return domain.Pool{}, e
		}
		*spec.value = domain.Amount(n.String())
	}
	updated, e := scalar(p.ID, "lastUpdated()", "", 32)
	if e != nil {
		return domain.Pool{}, e
	}
	state.LastUpdated = uint32(updated.Uint64())
	gaugeFees, e := call(p.ID, "gaugeFees()", "", 2)
	if e != nil {
		return domain.Pool{}, e
	}
	for i, target := range []*domain.Amount{&state.GaugeFees0, &state.GaugeFees1} {
		n, e := wordUint(gaugeFees[i*32:(i+1)*32], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		*target = domain.Amount(n.String())
	}
	for _, thinTick := range thin.V3.Ticks {
		data, e := call(p.ID, "ticks(int24)", fmt.Sprintf("%064x", func() *big.Int {
			v := big.NewInt(int64(thinTick.Tick))
			if v.Sign() < 0 {
				v.Add(v, new(big.Int).Lsh(big.NewInt(1), 256))
			}
			return v
		}()), 10)
		if e != nil {
			return domain.Pool{}, e
		}
		vals := make([]*big.Int, 10)
		for i, bits := range []int{128, 128, 128, 256, 256, 256, 56, 160, 32, 1} {
			var n *big.Int
			var e error
			if i == 1 || i == 2 || i == 6 {
				n, e = wordInt(data[i*32:(i+1)*32], bits)
			} else {
				n, e = wordUint(data[i*32:(i+1)*32], bits)
			}
			if e != nil {
				return domain.Pool{}, e
			}
			vals[i] = n
		}
		if vals[0].String() != thinTick.LiquidityGross.String() || vals[1].String() != thinTick.LiquidityNet {
			return fail("SLIPSTREAM_THIN_FULL_TICK_MISMATCH")
		}
		state.Ticks = append(state.Ticks, domain.SlipstreamTick{Tick: thinTick.Tick, LiquidityGross: domain.Amount(vals[0].String()), LiquidityNet: vals[1].String(), StakedLiquidityNet: vals[2].String(), FeeGrowthOutside0: domain.Amount(vals[3].String()), FeeGrowthOutside1: domain.Amount(vals[4].String()), RewardGrowthOutside: domain.Amount(vals[5].String()), TickCumulativeOutside: vals[6].String(), SecondsPerLiquidityOutsideX128: domain.Amount(vals[7].String()), SecondsOutside: uint32(vals[8].Uint64()), Initialized: vals[9].Sign() != 0})
	}
	for i := uint32(0); i < state.ObservationCardinalityNext; i++ {
		data, e := call(p.ID, "observations(uint256)", word(uint64(i)), 4)
		if e != nil {
			return domain.Pool{}, e
		}
		at, e := wordUint(data[:32], 32)
		if e != nil {
			return domain.Pool{}, e
		}
		tick, e := wordInt(data[32:64], 56)
		if e != nil {
			return domain.Pool{}, e
		}
		seconds, e := wordUint(data[64:96], 160)
		if e != nil {
			return domain.Pool{}, e
		}
		initialized, e := wordUint(data[96:], 1)
		if e != nil {
			return domain.Pool{}, e
		}
		state.Observations = append(state.Observations, domain.SlipstreamObservation{BlockTimestamp: uint32(at.Uint64()), TickCumulative: tick.String(), SecondsPerLiquidityCumulativeX128: domain.Amount(seconds.String()), Initialized: initialized.Sign() != 0})
	}
	viewUnstaked, e := scalar(p.ID, "unstakedFee()", "", 24)
	if e != nil {
		return domain.Pool{}, e
	}
	state.ViewUnstakedFee = uint32(viewUnstaked.Uint64())
	for _, caller := range c.Origins {
		if domain.ValidateEVMAddress(caller) != nil || caller == domain.SingletonZeroAddress {
			return fail("SLIPSTREAM_ORIGIN_CONFIG")
		}
		var code string
		if e = a.Client.Call(ctx, "eth_getCode", []any{caller, anchor}, &code); e != nil {
			return domain.Pool{}, e
		}
		if code != "0x" {
			return fail("SLIPSTREAM_ORIGIN_CLASS_UNSUPPORTED")
		}
		discount := uint64(0)
		if swap.Kind == "dynamic" {
			n, e := scalar(swap.Address, "discounted(address)", addressWord(caller), 24)
			if e != nil {
				return domain.Pool{}, e
			}
			discount = n.Uint64()
		}
		fee, e := scalarFrom(p.ID, "fee()", "", 24, caller)
		if e != nil {
			return domain.Pool{}, e
		}
		ufee, e := scalarFrom(p.ID, "unstakedFee()", "", 24, caller)
		if e != nil {
			return domain.Pool{}, e
		}
		state.Origins = append(state.Origins, domain.SlipstreamOrigin{Origin: caller, Discount: uint32(discount), ViewSwapFee: uint32(fee.Uint64()), ViewUnstakedFee: uint32(ufee.Uint64())})
	}
	pool := domain.Pool{SlipstreamContext: state, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: domain.SlipstreamContextProtocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: 0, FeeDenominator: 1000000, AdapterVersion: domain.SlipstreamContextAdapter, TokenBehavior: "standard"}
	return pool, pool.Validate()
}
