package evm

import (
	"context"
	"errors"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func (a *Adapter) readPancakeBscLm(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	if p.PancakeBscLm == nil || len(p.PancakeBscLm.Predecessors) != 3 {
		return domain.Pool{}, errors.New("BSC4_CONFIG_REQUIRED")
	}
	cfg := p.PancakeBscLm
	projected := p
	projected.PancakeBscLm = nil
	projected.Protocol = domain.PancakeLmProtocol
	projected.PancakeLm = &cfg.Ledger
	checkTemplate := func(role string, raw []byte, bindings map[string]string) (string, error) {
		if role == "lm" {
			for i, key := range []string{"firstLMPool", "secondLMPool", "thirdLMPool"} {
				bindings[key] = cfg.Predecessors[i].Address
			}
		}
		return validatePancakeBscRuntime(role, raw, bindings)
	}
	pool, e := a.readPancakeLedger(ctx, projected, anchor, block, checkTemplate, false)
	if e != nil {
		return domain.Pool{}, e
	}
	r := &pancakeRead{adapter: a, ctx: ctx, anchor: anchor}
	ledger := pool.PancakeLm
	state := &domain.PancakeBscLmState{Ledger: *ledger, Initialization: r.one(ledger.Lm.Address, "initialization()", "", 1).Uint64() == 1}
	roles := []string{"firstLMPool()", "secondLMPool()", "thirdLMPool()"}
	bindings := map[string]string{"pool": p.ID, "masterChef": ledger.MasterChef.Address}
	for i, role := range roles {
		address := r.addr(ledger.Lm.Address, role, "")
		if r.err != nil {
			return domain.Pool{}, r.err
		}
		if address != cfg.Predecessors[i].Address {
			return domain.Pool{}, errors.New("BSC4_PREDECESSOR_BINDING")
		}
		bindings[role[:len(role)-2]] = address
	}
	raw, e := a.verifyCode(ctx, ledger.Lm.Address, cfg.Ledger.ExpectedLmPoolCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if _, e = validatePancakeBscRuntime("lm", raw, bindings); e != nil {
		return domain.Pool{}, e
	}
	for _, t := range ledger.Lm.Ticks {
		state.Flags = append(state.Flags, domain.PancakeBscLmFlag{Tick: t.Tick, Migrated: r.one(ledger.Lm.Address, "lmTicksFlag(int24)", signedArg(t.Tick), 1).Uint64() == 1})
	}
	for generation, source := range cfg.Predecessors {
		row := domain.PancakeBscLmPredecessor{Address: source.Address, CodeSHA256: source.CodeSHA256, TemplateSHA256: domain.BscPredecessorTemplates[generation], Flags: []domain.PancakeBscLmFlag{}}
		// Each allowed original getter has a fixed v4 runtime. All immutable values
		// are verified even though swap only calls its context-free mapping getters.
		predecessorBindings := map[string]string{"pool": r.addr(source.Address, "pool()", ""), "masterChef": r.addr(source.Address, "masterChef()", "")}
		templateRole := []string{"first", "second", "third"}[generation]
		for _, binding := range pancakeBscTemplates[templateRole].Bindings {
			if _, ok := predecessorBindings[binding.Role]; !ok {
				predecessorBindings[binding.Role] = r.addr(source.Address, binding.Role+"()", "")
			}
		}
		expectedLinks := map[string]string{"OldLMPool": cfg.Predecessors[0].Address, "firstLMPool": cfg.Predecessors[0].Address, "secondLMPool": cfg.Predecessors[1].Address}
		for key, expected := range expectedLinks {
			if observed, ok := predecessorBindings[key]; ok && observed != expected {
				return domain.Pool{}, errors.New("BSC4_PREDECESSOR_LINEAGE")
			}
		}
		if predecessorBindings["pool"] != p.ID || predecessorBindings["masterChef"] != ledger.MasterChef.Address {
			return domain.Pool{}, errors.New("BSC4_PREDECESSOR_PARENT")
		}
		raw, e := a.verifyCode(ctx, source.Address, source.CodeSHA256, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if _, e = validatePancakeBscRuntime(templateRole, raw, predecessorBindings); e != nil {
			return domain.Pool{}, e
		}
		for _, t := range ledger.Lm.Ticks {
			value := r.call(source.Address, "lmTicks(int24)", signedArg(t.Tick), 3)
			row.Ticks = append(row.Ticks, domain.PancakeLmTick{Tick: t.Tick, LiquidityGross: domain.Amount(r.unsigned(value, 0, 128).String()), LiquidityNet: r.signed(value, 1, 128), RewardGrowthOutside: domain.Amount(r.unsigned(value, 2, 256).String())})
			if generation != 0 {
				row.Flags = append(row.Flags, domain.PancakeBscLmFlag{Tick: t.Tick, Migrated: r.one(source.Address, "lmTicksFlag(int24)", signedArg(t.Tick), 1).Uint64() == 1})
			}
		}
		state.Predecessors = append(state.Predecessors, row)
	}
	if r.err != nil {
		return domain.Pool{}, r.err
	}
	pool.PancakeLm = nil
	pool.PancakeBscLm = state
	pool.Protocol = domain.PancakeBscLmProtocol
	pool.AdapterVersion = domain.PancakeBscLmAdapter
	return pool, pool.Validate()
}
