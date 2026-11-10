//! Independently named BSC V4 LM migration state. Internal legacy projections
//! share arithmetic only; these projections must never be canonical field33 state.
use crate::{
    err,
    pancake_lm::{self, PancakeLmState, PancakeLmTick},
    wire, Pool, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const PROFILE: &str = "pancake-v3-bsc-lm-v4";
pub const ADAPTER: &str = "pancake-v3/bsc-p400-lm1000000-mc999-v4";
pub const POOL_TEMPLATE: &str = "128755d0e9816da897b0634fec16d89fb47a0c7c41bc2660a68b0c7352f84d3e";
pub const LM_TEMPLATE: &str = "df34ef41e16729a12160b2b0e3410729141a521ede0301f52e141b8595014389";
pub const CHEF_TEMPLATE: &str = "64d48f6819e38b7561e0e92a414795f28cbb75e09475d982246659c939365309";
pub const FACTORY_TEMPLATE: &str =
    "829e62aa0f1d3471118cabd7931dc0ba702f63ff04496c7d1a1832453970fd52";
pub const PREDECESSOR_TEMPLATES: [&str; 3] = [
    "baac406328c78d73ac2077353a7e657864be21a0a26bad057fe84f18d50cd5b7",
    "1f251fdf65b944bcde07aec7c5bcfb8cbd2a5b55c4808d6ab99184a8ae3dbad2",
    "baf90c4558c993d9fefd1d102d42a92ee1bc8d0209badca100835cbe38ce2749",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeBscLmFlag {
    pub tick: i32,
    pub migrated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeBscLmPredecessor {
    pub address: String,
    pub code_sha256: String,
    pub template_sha256: String,
    pub ticks: Vec<PancakeLmTick>,
    pub flags: Vec<PancakeBscLmFlag>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeBscLmState {
    pub ledger: PancakeLmState,
    pub initialization: bool,
    pub flags: Vec<PancakeBscLmFlag>,
    pub predecessors: Vec<PancakeBscLmPredecessor>,
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl PancakeBscLmState {
    pub fn effective_ticks(&self) -> Result<Vec<PancakeLmTick>> {
        if self.predecessors.len() != 3 || self.flags.len() != self.ledger.lm.ticks.len() {
            return Err(err("BSC4_MIGRATION_SCOPE"));
        }
        let mut out = Vec::new();
        for (index, (local, flag)) in self.ledger.lm.ticks.iter().zip(&self.flags).enumerate() {
            if local.tick != flag.tick {
                return Err(err("BSC4_MIGRATION_TICK_BINDING"));
            }
            for (generation, predecessor) in self.predecessors.iter().enumerate() {
                if predecessor.ticks.len() != self.flags.len()
                    || (generation != 0 && predecessor.flags.len() != self.flags.len())
                    || (generation == 0 && !predecessor.flags.is_empty())
                    || predecessor.ticks[index].tick != local.tick
                    || (generation != 0 && predecessor.flags[index].tick != local.tick)
                {
                    return Err(err("BSC4_PREDECESSOR_TICK_SCOPE"));
                }
            }
            let effective = if flag.migrated {
                local
            } else {
                let third = &self.predecessors[2];
                let second = &self.predecessors[1];
                if third.flags[index].migrated {
                    &third.ticks[index]
                } else if second.flags[index].migrated {
                    &second.ticks[index]
                } else {
                    &self.predecessors[0].ticks[index]
                }
            };
            out.push(effective.clone());
        }
        Ok(out)
    }
    pub(crate) fn arithmetic_ledger(&self) -> Result<PancakeLmState> {
        let mut s = self.ledger.clone();
        s.pool_template_sha256 = pancake_lm::POOL_TEMPLATE.into();
        s.factory_template_sha256 = pancake_lm::FACTORY_TEMPLATE.into();
        s.master_chef.template_sha256 = pancake_lm::CHEF_TEMPLATE.into();
        s.lm.template_sha256 = pancake_lm::LM_TEMPLATE.into();
        s.lm.ticks = self.effective_ticks()?;
        Ok(s)
    }
    pub fn validate_wire(&self) -> Result<()> {
        if self.predecessors.len() != 3 {
            return Err(err("BSC4_MIGRATION_SCOPE"));
        }
        let s = &self.ledger;
        if s.pool_template_sha256 != POOL_TEMPLATE
            || s.lm.template_sha256 != LM_TEMPLATE
            || s.master_chef.template_sha256 != CHEF_TEMPLATE
            || s.factory_template_sha256 != FACTORY_TEMPLATE
        {
            return Err(err("BSC4_CODE_TEMPLATE"));
        }
        let mut distinct = BTreeSet::from([
            s.lm.address.as_str(),
            s.master_chef.address.as_str(),
            s.core.factory.as_str(),
            s.lm.pool.as_str(),
        ]);
        for (generation, p) in self.predecessors.iter().enumerate() {
            crate::protocols::validate_address(&p.address)?;
            if p.address == crate::singleton::ZERO_ADDRESS
                || !distinct.insert(&p.address)
                || !digest(&p.code_sha256)
                || p.template_sha256 != PREDECESSOR_TEMPLATES[generation]
            {
                return Err(err("BSC4_PREDECESSOR_IDENTITY"));
            }
            for t in &p.ticks {
                t.validate_wire()?;
            }
        }
        self.arithmetic_ledger()?.validate_wire()?;
        // Unmigrated storage is still observable and must be retained and well formed.
        for t in &s.lm.ticks {
            t.validate_wire()?;
        }
        Ok(())
    }
    pub fn arithmetic_pool(&self, p: &Pool) -> Result<Pool> {
        self.validate_wire()?;
        if p.protocol != PROFILE
            || p.adapter_version != ADAPTER
            || p.pancake_bsc_lm.as_ref() != Some(self)
        {
            return Err(err("BSC4_PARENT_BINDING"));
        }
        if self
            .predecessors
            .iter()
            .any(|x| x.address == p.token0 || x.address == p.token1)
        {
            return Err(err("BSC4_PREDECESSOR_TOKEN_ALIAS"));
        }
        let mut thin = p.clone();
        thin.protocol = pancake_lm::PROFILE.into();
        thin.adapter_version = pancake_lm::ADAPTER.into();
        thin.pancake_bsc_lm = None;
        thin.pancake_lm = Some(self.arithmetic_ledger()?);
        thin.validate()?;
        Ok(thin)
    }
    pub fn validate_pool(&self, p: &Pool) -> Result<()> {
        self.arithmetic_pool(p).map(|_| ())
    }
    pub fn to_proto(&self) -> Result<wire::PancakeBscLmState> {
        self.validate_wire()?;
        // Encode the complete raw ledger, including unmigrated local slots. Its
        // standalone legacy validator deliberately rejects these BSC identities.
        let normalized = self.arithmetic_ledger()?;
        let mut ledger = normalized.to_proto()?;
        ledger.factory_template_sha256 =
            hex::decode(FACTORY_TEMPLATE).map_err(|_| err("BSC4_TEMPLATE_HEX"))?;
        ledger.pool_template_sha256 =
            hex::decode(POOL_TEMPLATE).map_err(|_| err("BSC4_TEMPLATE_HEX"))?;
        ledger
            .master_chef
            .as_mut()
            .ok_or_else(|| err("BSC4_CHEF_REQUIRED"))?
            .template_sha256 = hex::decode(CHEF_TEMPLATE).map_err(|_| err("BSC4_TEMPLATE_HEX"))?;
        let lm = ledger.lm.as_mut().ok_or_else(|| err("BSC4_LM_REQUIRED"))?;
        lm.template_sha256 = hex::decode(LM_TEMPLATE).map_err(|_| err("BSC4_TEMPLATE_HEX"))?;
        lm.ticks = self
            .ledger
            .lm
            .ticks
            .iter()
            .map(PancakeLmTick::to_proto)
            .collect::<Result<_>>()?;
        Ok(wire::PancakeBscLmState {
            ledger: Some(ledger),
            initialization: Some(self.initialization),
            flags: self
                .flags
                .iter()
                .map(|f| wire::PancakeBscLmFlag {
                    tick: Some(f.tick),
                    migrated: Some(f.migrated),
                })
                .collect(),
            predecessors: self
                .predecessors
                .iter()
                .map(|p| {
                    Ok(wire::PancakeBscLmPredecessor {
                        address: p.address.clone(),
                        code_sha256: hex::decode(&p.code_sha256).map_err(|_| err("BSC4_DIGEST"))?,
                        template_sha256: hex::decode(&p.template_sha256)
                            .map_err(|_| err("BSC4_DIGEST"))?,
                        ticks: p
                            .ticks
                            .iter()
                            .map(PancakeLmTick::to_proto)
                            .collect::<Result<_>>()?,
                        flags: p
                            .flags
                            .iter()
                            .map(|f| wire::PancakeBscLmFlag {
                                tick: Some(f.tick),
                                migrated: Some(f.migrated),
                            })
                            .collect(),
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
    pub fn from_proto(s: wire::PancakeBscLmState) -> Result<Self> {
        let mut raw = s.ledger.ok_or_else(|| err("BSC4_LEDGER_REQUIRED"))?;
        let pool_template = hex::encode(&raw.pool_template_sha256);
        let factory_template = hex::encode(&raw.factory_template_sha256);
        raw.factory_template_sha256 =
            hex::decode(pancake_lm::FACTORY_TEMPLATE).map_err(|_| err("BSC4_HEX"))?;
        let chef_template = hex::encode(
            &raw.master_chef
                .as_ref()
                .ok_or_else(|| err("BSC4_CHEF_REQUIRED"))?
                .template_sha256,
        );
        let lm_template = hex::encode(
            &raw.lm
                .as_ref()
                .ok_or_else(|| err("BSC4_LM_REQUIRED"))?
                .template_sha256,
        );
        raw.pool_template_sha256 =
            hex::decode(pancake_lm::POOL_TEMPLATE).map_err(|_| err("BSC4_HEX"))?;
        raw.master_chef
            .as_mut()
            .ok_or_else(|| err("BSC4_CHEF_REQUIRED"))?
            .template_sha256 =
            hex::decode(pancake_lm::CHEF_TEMPLATE).map_err(|_| err("BSC4_HEX"))?;
        raw.lm
            .as_mut()
            .ok_or_else(|| err("BSC4_LM_REQUIRED"))?
            .template_sha256 = hex::decode(pancake_lm::LM_TEMPLATE).map_err(|_| err("BSC4_HEX"))?;
        let decode_flag = |f: wire::PancakeBscLmFlag| -> Result<PancakeBscLmFlag> {
            Ok(PancakeBscLmFlag {
                tick: f.tick.ok_or_else(|| err("BSC4_FLAG_TICK_REQUIRED"))?,
                migrated: f.migrated.ok_or_else(|| err("BSC4_FLAG_VALUE_REQUIRED"))?,
            })
        };
        let flags = s
            .flags
            .into_iter()
            .map(decode_flag)
            .collect::<Result<Vec<_>>>()?;
        let predecessors = s
            .predecessors
            .into_iter()
            .map(|p| {
                Ok(PancakeBscLmPredecessor {
                    address: p.address,
                    code_sha256: hex::encode(p.code_sha256),
                    template_sha256: hex::encode(p.template_sha256),
                    ticks: p
                        .ticks
                        .into_iter()
                        .map(PancakeLmTick::from_proto)
                        .collect::<Result<Vec<_>>>()?,
                    flags: p
                        .flags
                        .into_iter()
                        .map(decode_flag)
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        // The legacy decoder validates the effective curve. Retain raw local slots
        // separately while validating all optional fields and numeric widths.
        let raw_ticks = raw
            .lm
            .as_ref()
            .ok_or_else(|| err("BSC4_LM_REQUIRED"))?
            .ticks
            .clone();
        let local = raw_ticks
            .iter()
            .cloned()
            .map(PancakeLmTick::from_proto)
            .collect::<Result<Vec<_>>>()?;
        if flags.len() != local.len() || predecessors.len() != 3 {
            return Err(err("BSC4_MIGRATION_SCOPE"));
        }
        for (i, flag) in flags.iter().enumerate() {
            if !flag.migrated {
                let selected = if predecessors[2].flags.get(i).is_some_and(|f| f.migrated) {
                    2
                } else if predecessors[1].flags.get(i).is_some_and(|f| f.migrated) {
                    1
                } else {
                    0
                };
                raw.lm
                    .as_mut()
                    .ok_or_else(|| err("BSC4_LM_REQUIRED"))?
                    .ticks[i] = predecessors[selected]
                    .ticks
                    .get(i)
                    .ok_or_else(|| err("BSC4_PREDECESSOR_TICK_SCOPE"))?
                    .to_proto()?;
            }
        }
        let mut ledger = PancakeLmState::from_proto(raw)?;
        ledger.pool_template_sha256 = pool_template;
        ledger.factory_template_sha256 = factory_template;
        ledger.master_chef.template_sha256 = chef_template;
        ledger.lm.template_sha256 = lm_template;
        ledger.lm.ticks = local;
        let out = Self {
            ledger,
            initialization: s
                .initialization
                .ok_or_else(|| err("BSC4_INITIALIZATION_REQUIRED"))?,
            flags,
            predecessors,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
