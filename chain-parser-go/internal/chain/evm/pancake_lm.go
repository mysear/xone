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

type pancakeRead struct {
	adapter *Adapter
	ctx     context.Context
	anchor  map[string]any
	err     error
}

func (r *pancakeRead) call(to, sig, args string, words int) []byte {
	empty := make([]byte, words*32)
	if r.err != nil {
		return empty
	}
	var value string
	r.err = r.adapter.Client.Call(r.ctx, "eth_call", []any{map[string]string{"to": to, "data": singletonSelector(sig) + args}, r.anchor}, &value)
	if r.err != nil {
		return empty
	}
	if !strings.HasPrefix(value, "0x") {
		r.err = errors.New("PANCAKE_LM_ABI_PREFIX")
		return empty
	}
	raw, e := hex.DecodeString(value[2:])
	if e != nil || len(raw) != words*32 {
		r.err = errors.New("PANCAKE_LM_ABI_SIZE")
		return empty
	}
	return raw
}
func (r *pancakeRead) unsigned(raw []byte, index, bits int) *big.Int {
	if r.err != nil {
		return new(big.Int)
	}
	if index < 0 || (index+1)*32 > len(raw) {
		r.err = errors.New("PANCAKE_LM_ABI_WORD")
		return new(big.Int)
	}
	n, e := wordUint(raw[index*32:(index+1)*32], bits)
	if e != nil {
		r.err = e
		return new(big.Int)
	}
	return n
}
func (r *pancakeRead) signed(raw []byte, index, bits int) string {
	if r.err != nil {
		return "0"
	}
	if index < 0 || (index+1)*32 > len(raw) {
		r.err = errors.New("PANCAKE_LM_ABI_WORD")
		return "0"
	}
	n, e := wordInt(raw[index*32:(index+1)*32], bits)
	if e != nil {
		r.err = e
		return "0"
	}
	return n.String()
}
func (r *pancakeRead) one(to, sig, args string, bits int) *big.Int {
	return r.unsigned(r.call(to, sig, args, 1), 0, bits)
}
func (r *pancakeRead) addr(to, sig, args string) string {
	return fmt.Sprintf("0x%040x", r.one(to, sig, args, 160))
}
func (r *pancakeRead) amount(to, sig, args string, bits int) domain.Amount {
	return domain.Amount(r.one(to, sig, args, bits).String())
}
func (r *pancakeRead) address(raw []byte, index int) string {
	return fmt.Sprintf("0x%040x", r.unsigned(raw, index, 160))
}
func (a *Adapter) readPancakeLm(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	return a.readPancakeLedger(ctx, p, anchor, block, validatePancakeRuntime, true)
}
func (a *Adapter) readPancakeLedger(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef, template func(string, []byte, map[string]string) (string, error), validate bool) (domain.Pool, error) {
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	c := p.PancakeLm
	if c == nil || p.Protocol != domain.PancakeLmProtocol || c.TokenPolicy != "standard" || c.MaxObservations == 0 || c.MaxObservations > 4096 || len(anchor) != 2 || anchor["blockHash"] != block.Hash || anchor["requireCanonical"] != true || block.TimestampMS%1000 != 0 {
		return fail("PANCAKE_LM_FIXED_BANK_CONFIG")
	}
	legacy := p
	legacy.Protocol = "pancake-v3"
	legacy.PancakeLm = nil
	legacy.V3 = &config.V3PoolConfig{ExpectedCodeSHA256: c.ExpectedCodeSHA256, ExpectedFactory: c.ExpectedFactory, ExpectedFactoryCodeSHA256: c.ExpectedFactoryCodeSHA256, TickWordRadius: c.TickWordRadius, MaxInitializedTicks: c.MaxInitializedTicks, MaxSteps: c.MaxSteps}
	pool, e := a.readV3Context(ctx, legacy, anchor, v3ReadContext{allowKnownPancakeLm: true})
	if e != nil {
		return domain.Pool{}, e
	}
	core := pool.V3
	r := &pancakeRead{adapter: a, ctx: ctx, anchor: anchor}
	slot := r.call(p.ID, "slot0()", "", 7)
	if r.unsigned(slot, 0, 160).Cmp(core.SqrtPriceX96.Big()) != 0 || r.signed(slot, 1, 24) != fmt.Sprint(core.Tick) || uint32(r.unsigned(slot, 5, 32).Uint64()) != core.FeeProtocol {
		if r.err != nil {
			return domain.Pool{}, r.err
		}
		return fail("PANCAKE_LM_REPEATED_SLOT0")
	}
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	s := &domain.PancakeLmState{Core: core, Tick: core.Tick, FeeProtocol: core.FeeProtocol, Token0CodeSHA256: c.ExpectedToken0CodeSHA256, Token1CodeSHA256: c.ExpectedToken1CodeSHA256, BlockHash: block.Hash, BlockNumber: block.Number, Timestamp: block.TimestampMS / 1000, MaxObservations: c.MaxObservations, Unlocked: r.unsigned(slot, 6, 1).Uint64() == 1, ObservationIndex: uint32(r.unsigned(slot, 2, 16).Uint64()), ObservationCardinality: uint32(r.unsigned(slot, 3, 16).Uint64()), ObservationCardinalityNext: uint32(r.unsigned(slot, 4, 16).Uint64()), Ticks: []domain.PancakeLedgerTick{}, Observations: []domain.PancakeObservation{}}
	word := func(n uint64) string { return fmt.Sprintf("%064x", n) }
	addressWord := func(address string) string {
		if domain.ValidateEVMAddress(address) != nil {
			r.err = errors.New("PANCAKE_LM_ADDRESS_ARG")
			return strings.Repeat("0", 64)
		}
		return strings.Repeat("0", 24) + address[2:]
	}
	registered := r.addr(c.ExpectedFactory, "getPool(address,address,uint24)", addressWord(p.Token0)+addressWord(p.Token1)+word(p.FeeNumerator))
	s.FactoryRegistered = registered == p.ID
	lmAddress := r.addr(p.ID, "lmPool()", "")
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	if lmAddress != c.ExpectedLmPool {
		return fail("PANCAKE_LM_POINTER")
	}
	lm := domain.PancakeLm{Address: lmAddress, CodeSHA256: c.ExpectedLmPoolCodeSHA256, Pool: r.addr(lmAddress, "pool()", ""), MasterChef: r.addr(lmAddress, "masterChef()", ""), RewardGrowthGlobal: r.amount(lmAddress, "rewardGrowthGlobalX128()", "", 256), Liquidity: r.amount(lmAddress, "lmLiquidity()", "", 128), LastRewardTimestamp: uint32(r.one(lmAddress, "lastRewardTimestamp()", "", 32).Uint64()), Ticks: []domain.PancakeLmTick{}}
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	if lm.MasterChef != c.ExpectedMasterChef {
		return fail("PANCAKE_LM_MASTER_CHEF_POINTER")
	}
	chefAddress := lm.MasterChef
	chef := domain.PancakeMasterChef{Address: chefAddress, CodeSHA256: c.ExpectedMasterChefCodeSHA256, Cake: r.addr(chefAddress, "CAKE()", ""), Weth: r.addr(chefAddress, "WETH()", ""), PositionManager: r.addr(chefAddress, "nonfungiblePositionManager()", ""), PoolPid: r.amount(chefAddress, "v3PoolAddressPid(address)", addressWord(p.ID), 256), TotalAllocPoint: r.amount(chefAddress, "totalAllocPoint()", "", 256), LatestPeriodCakePerSecond: r.amount(chefAddress, "latestPeriodCakePerSecond()", "", 256), LatestPeriodEndTime: r.amount(chefAddress, "latestPeriodEndTime()", "", 256)}
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	if chef.Cake != c.ExpectedCake || chef.Weth != c.ExpectedWeth || chef.PositionManager != c.ExpectedPositionManager {
		return fail("PANCAKE_LM_MASTER_CHEF_IMMUTABLE")
	}
	info := r.call(chefAddress, "poolInfo(uint256)", fmt.Sprintf("%064x", chef.PoolPid.Big()), 7)
	chef.PoolAllocPoint = domain.Amount(r.unsigned(info, 0, 256).String())
	chef.PoolAddress = r.address(info, 1)
	chef.PoolToken0 = r.address(info, 2)
	chef.PoolToken1 = r.address(info, 3)
	chef.PoolFee = uint32(r.unsigned(info, 4, 24).Uint64())
	r.unsigned(info, 5, 256)
	r.unsigned(info, 6, 256)
	view := r.call(chefAddress, "getLatestPeriodInfo(address)", addressWord(p.ID), 2)
	chef.ViewRewardPerSecond = domain.Amount(r.unsigned(view, 0, 256).String())
	chef.ViewEndTime = domain.Amount(r.unsigned(view, 1, 256).String())
	s.RawBalance0 = r.amount(p.Token0, "balanceOf(address)", addressWord(p.ID), 256)
	s.RawBalance1 = r.amount(p.Token1, "balanceOf(address)", addressWord(p.ID), 256)
	s.FeeGrowthGlobal0 = r.amount(p.ID, "feeGrowthGlobal0X128()", "", 256)
	s.FeeGrowthGlobal1 = r.amount(p.ID, "feeGrowthGlobal1X128()", "", 256)
	fees := r.call(p.ID, "protocolFees()", "", 2)
	s.ProtocolFees0 = domain.Amount(r.unsigned(fees, 0, 128).String())
	s.ProtocolFees1 = domain.Amount(r.unsigned(fees, 1, 128).String())
	s.MaxLiquidityPerTick = r.amount(p.ID, "maxLiquidityPerTick()", "", 128)
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	if s.ObservationCardinalityNext == 0 || s.ObservationCardinalityNext > c.MaxObservations {
		return fail("PANCAKE_LM_OBSERVATION_LIMIT")
	}
	for index := uint32(0); index < s.ObservationCardinalityNext; index++ {
		value := r.call(p.ID, "observations(uint256)", word(uint64(index)), 4)
		s.Observations = append(s.Observations, domain.PancakeObservation{BlockTimestamp: uint32(r.unsigned(value, 0, 32).Uint64()), TickCumulative: r.signed(value, 1, 56), SecondsPerLiquidityCumulativeX128: domain.Amount(r.unsigned(value, 2, 160).String()), Initialized: r.unsigned(value, 3, 1).Uint64() == 1})
	}
	for _, t := range core.Ticks {
		value := r.call(p.ID, "ticks(int24)", signedArg(t.Tick), 8)
		full := domain.PancakeLedgerTick{Tick: t.Tick, LiquidityGross: domain.Amount(r.unsigned(value, 0, 128).String()), LiquidityNet: r.signed(value, 1, 128), FeeGrowthOutside0: domain.Amount(r.unsigned(value, 2, 256).String()), FeeGrowthOutside1: domain.Amount(r.unsigned(value, 3, 256).String()), TickCumulativeOutside: r.signed(value, 4, 56), SecondsPerLiquidityOutsideX128: domain.Amount(r.unsigned(value, 5, 160).String()), SecondsOutside: uint32(r.unsigned(value, 6, 32).Uint64()), Initialized: r.unsigned(value, 7, 1).Uint64() == 1}
		s.Ticks = append(s.Ticks, full)
		value = r.call(lmAddress, "lmTicks(int24)", signedArg(t.Tick), 3)
		lm.Ticks = append(lm.Ticks, domain.PancakeLmTick{Tick: t.Tick, LiquidityGross: domain.Amount(r.unsigned(value, 0, 128).String()), LiquidityNet: r.signed(value, 1, 128), RewardGrowthOutside: domain.Amount(r.unsigned(value, 2, 256).String())})
	}
	deployer := r.addr(c.ExpectedFactory, "poolDeployer()", "")
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	for _, entry := range []struct {
		role, address, digest string
		bindings              map[string]string
	}{
		{"pool", p.ID, c.ExpectedCodeSHA256, map[string]string{"factory": core.Factory, "token0": p.Token0, "token1": p.Token1, "fee": word(pool.FeeNumerator), "tickSpacing": word(uint64(core.TickSpacing)), "maxLiquidityPerTick": fmt.Sprintf("%064x", s.MaxLiquidityPerTick.Big())}},
		{"factory", c.ExpectedFactory, c.ExpectedFactoryCodeSHA256, map[string]string{"poolDeployer": deployer}},
		{"lm", lmAddress, c.ExpectedLmPoolCodeSHA256, map[string]string{"pool": p.ID, "masterChef": chefAddress}},
		{"masterChef", chefAddress, c.ExpectedMasterChefCodeSHA256, map[string]string{"CAKE": chef.Cake, "WETH": chef.Weth, "nonfungiblePositionManager": chef.PositionManager}},
	} {
		raw, e := a.verifyCode(ctx, entry.address, entry.digest, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		template, e := template(entry.role, raw, entry.bindings)
		if e != nil {
			return domain.Pool{}, e
		}
		switch entry.role {
		case "pool":
			s.PoolTemplateSHA256 = template
		case "factory":
			s.FactoryTemplateSHA256 = template
		case "lm":
			lm.TemplateSHA256 = template
		case "masterChef":
			chef.TemplateSHA256 = template
		}
	}
	for _, entry := range []struct{ address, digest string }{{p.Token0, c.ExpectedToken0CodeSHA256}, {p.Token1, c.ExpectedToken1CodeSHA256}} {
		if _, e := a.verifyCode(ctx, entry.address, entry.digest, anchor); e != nil {
			return domain.Pool{}, e
		}
	}
	s.MasterChef = chef
	s.Lm = lm
	pool.Protocol = domain.PancakeLmProtocol
	pool.AdapterVersion = domain.PancakeLmAdapter
	pool.V3 = nil
	pool.PancakeLm = s
	if validate {
		return pool, pool.Validate()
	}
	return pool, nil
}
