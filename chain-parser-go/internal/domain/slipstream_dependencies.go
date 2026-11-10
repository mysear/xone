package domain

import (
	"encoding/json"
	"errors"
	"fmt"
)

func ValidateSlipstreamDependencies(pools []Pool) error {
	facts := map[string]string{}
	for _, p := range pools {
		put := func(address, key string, value any) error {
			raw, e := json.Marshal(value)
			if e != nil {
				return e
			}
			id := p.ChainKey + "\x00" + address + "\x00" + key
			if old, exists := facts[id]; exists && old != string(raw) {
				return errors.New("SLIPSTREAM_SHARED_READONLY_CONFLICT")
			}
			facts[id] = string(raw)
			return nil
		}
		if s := p.SlipstreamContext; s != nil {
			if s.Core == nil {
				return errors.New("SLIPSTREAM_CORE_REQUIRED")
			}
			for _, entry := range []struct{ address, code string }{{p.PoolID, s.Core.CodeSHA256}, {s.Core.Factory, s.Core.FactoryCodeSHA256}, {s.Core.Implementation, s.Core.ImplementationCodeSHA256}, {s.FactoryVoter, s.VoterCodeSHA256}, {p.Token0, s.Token0CodeSHA256}, {p.Token1, s.Token1CodeSHA256}} {
				if e := put(entry.address, "code", entry.code); e != nil {
					return e
				}
			}
			for _, entry := range []struct {
				key   string
				value any
			}{{"voter", s.FactoryVoter}, {"registry", s.FactoryRegistry}, {"pool-implementation", s.Core.Implementation}, {"swap-module", s.SwapFeeModule.Address}, {"unstaked-module", s.UnstakedFeeModule.Address}, {"default-unstaked-fee", s.FactoryDefaultUnstakedFee}} {
				if e := put(s.Core.Factory, entry.key, entry.value); e != nil {
					return e
				}
			}
			if e := put(s.Core.Factory, fmt.Sprintf("spacing-fee/%d", s.Core.TickSpacing), s.FactoryDefaultFee); e != nil {
				return e
			}
			if e := put(s.Core.Factory, fmt.Sprintf("pool-key/%s/%s/%d", p.Token0, p.Token1, s.Core.TickSpacing), p.PoolID); e != nil {
				return e
			}
			if e := put(s.FactoryVoter, "gauge/"+p.PoolID, s.VoterGauge); e != nil {
				return e
			}
			if e := put(s.FactoryVoter, "alive/"+s.VoterGauge, s.VoterGaugeAlive); e != nil {
				return e
			}
			for _, m := range []SlipstreamModule{s.SwapFeeModule, s.UnstakedFeeModule} {
				if m.Kind == "none" {
					continue
				}
				for _, entry := range []struct {
					key   string
					value any
				}{{"code", m.CodeSHA256}, {"kind", m.Kind}, {"template", m.TemplateSHA256}, {"factory", m.Factory}} {
					if e := put(m.Address, entry.key, entry.value); e != nil {
						return e
					}
				}
				if d := m.Dynamic; d != nil {
					for _, entry := range []struct {
						key   string
						value any
					}{{"default-scaling-factor", d.DefaultScalingFactor}, {"default-fee-cap", d.DefaultFeeCap}, {"seconds-ago", d.SecondsAgo}} {
						if e := put(m.Address, entry.key, entry.value); e != nil {
							return e
						}
					}
					for _, o := range s.Origins {
						if e := put(m.Address, "discount/"+o.Origin, o.Discount); e != nil {
							return e
						}
					}
				}
			}
		} else if p.Protocol == "aerodrome-slipstream" && p.V3 != nil {
			if p.V3.FactoryCodeSHA256 != "" {
				if e := put(p.V3.Factory, "code", p.V3.FactoryCodeSHA256); e != nil {
					return e
				}
			}
			for _, key := range []string{"swap-module", "unstaked-module"} {
				if e := put(p.V3.Factory, key, SingletonZeroAddress); e != nil {
					return e
				}
			}
		} else if p.Protocol == SlipstreamContextProtocol {
			return errors.New("SLIPSTREAM_CONTEXT_REQUIRED")
		}
	}
	return nil
}
