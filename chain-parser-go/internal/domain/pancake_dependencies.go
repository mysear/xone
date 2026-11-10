package domain

import (
	"encoding/json"
	"errors"
	"fmt"
)

func ValidatePancakeLmDependencies(pools []Pool) error {
	facts := map[string]string{}
	for _, p := range pools {
		put := func(addr, key string, value any) error {
			raw, e := json.Marshal(value)
			if e != nil {
				return e
			}
			id := p.ChainKey + "/" + addr + "/" + key
			if old, ok := facts[id]; ok && old != string(raw) {
				return errors.New("PANCAKE_LM_SHARED_READONLY_CONFLICT")
			}
			facts[id] = string(raw)
			return nil
		}
		s := p.PancakeLm
		if p.PancakeBscLm != nil {
			s = &p.PancakeBscLm.Ledger
		}
		if s != nil {
			if s.Core == nil {
				return errors.New("PANCAKE_LM_CORE_REQUIRED")
			}
			for _, e := range []struct{ a, c string }{{p.PoolID, s.Core.CodeSHA256}, {s.Core.Factory, s.Core.FactoryCodeSHA256}, {p.Token0, s.Token0CodeSHA256}, {p.Token1, s.Token1CodeSHA256}, {s.Lm.Address, s.Lm.CodeSHA256}, {s.MasterChef.Address, s.MasterChef.CodeSHA256}} {
				if e := put(e.a, "code", e.c); e != nil {
					return e
				}
			}
			c := s.MasterChef
			if e := put(c.Address, "period-inputs", []any{c.TemplateSHA256, c.Cake, c.Weth, c.PositionManager, c.TotalAllocPoint, c.LatestPeriodCakePerSecond, c.LatestPeriodEndTime}); e != nil {
				return e
			}
			if e := put(c.Address, "pid/"+string(c.PoolPid), []any{c.PoolAddress, c.PoolToken0, c.PoolToken1, c.PoolFee, c.PoolAllocPoint}); e != nil {
				return e
			}
			if e := put(c.Address, "pool-pid/"+p.PoolID, c.PoolPid); e != nil {
				return e
			}
			if e := put(s.Lm.Address, "immutable-bindings", []string{s.Lm.TemplateSHA256, s.Lm.Pool, s.Lm.MasterChef}); e != nil {
				return e
			}
			if e := put(p.PoolID, "lm", s.Lm.Address); e != nil {
				return e
			}
			if e := put(s.Core.Factory, fmt.Sprintf("pool-key/%s/%s/%d", p.Token0, p.Token1, p.FeeNumerator), p.PoolID); e != nil {
				return e
			}
		} else if p.Protocol == "pancake-v3" && p.V3 != nil {
			if e := put(p.PoolID, "lm", SingletonZeroAddress); e != nil {
				return e
			}
			if e := put(p.PoolID, "code", p.V3.CodeSHA256); e != nil {
				return e
			}
			if p.V3.FactoryCodeSHA256 != "" {
				if e := put(p.V3.Factory, "code", p.V3.FactoryCodeSHA256); e != nil {
					return e
				}
			}
		}
		if s := p.PancakeBscLm; s != nil {
			for _, previous := range s.Predecessors {
				if e := put(previous.Address, "code", previous.CodeSHA256); e != nil {
					return e
				}
				if e := put(previous.Address, "predecessor-bank", previous); e != nil {
					return e
				}
			}
		}
		if s := p.SlipstreamContext; s != nil {
			for _, e := range []struct{ a, c string }{{p.PoolID, s.Core.CodeSHA256}, {s.Core.Factory, s.Core.FactoryCodeSHA256}, {p.Token0, s.Token0CodeSHA256}, {p.Token1, s.Token1CodeSHA256}} {
				if e := put(e.a, "code", e.c); e != nil {
					return e
				}
			}
		}
	}
	return nil
}
