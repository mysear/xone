//! Readonly facts shared by contextual pools must agree inside one chain bank.
use crate::{err, slipstream_context, Pool, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub fn validate<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let mut facts: BTreeMap<(String, String, String), Value> = BTreeMap::new();
    for pool in pools {
        let mut put = |address: &str, key: String, value: Value| -> Result<()> {
            let id = (pool.chain_key.clone(), address.to_owned(), key);
            if let Some(old) = facts.get(&id) {
                if old != &value {
                    return Err(err("SLIPSTREAM_SHARED_READONLY_CONFLICT"));
                }
            } else {
                facts.insert(id, value);
            }
            Ok(())
        };
        if let Some(s) = &pool.slipstream_context {
            put(&pool.pool_id, "code".into(), json!(s.core.code_sha256))?;
            put(
                &s.core.factory,
                "code".into(),
                json!(s.core.factory_code_sha256),
            )?;
            put(
                &s.core.implementation,
                "code".into(),
                json!(s.core.implementation_code_sha256),
            )?;
            put(&s.factory_voter, "code".into(), json!(s.voter_code_sha256))?;
            for (address, code) in [
                (&pool.token0, &s.token0_code_sha256),
                (&pool.token1, &s.token1_code_sha256),
            ] {
                put(address, "code".into(), json!(code))?;
            }
            for (key, value) in [
                ("voter", json!(s.factory_voter)),
                ("registry", json!(s.factory_registry)),
                ("pool-implementation", json!(s.core.implementation)),
                ("swap-module", json!(s.swap_fee_module.address)),
                ("unstaked-module", json!(s.unstaked_fee_module.address)),
                (
                    "default-unstaked-fee",
                    json!(s.factory_default_unstaked_fee),
                ),
            ] {
                put(&s.core.factory, key.into(), value)?;
            }
            put(
                &s.core.factory,
                format!("spacing-fee/{}", s.core.tick_spacing),
                json!(s.factory_default_fee),
            )?;
            put(
                &s.core.factory,
                format!(
                    "pool-key/{}/{}/{}",
                    pool.token0, pool.token1, s.core.tick_spacing
                ),
                json!(pool.pool_id),
            )?;
            put(
                &s.factory_voter,
                format!("gauge/{}", pool.pool_id),
                json!(s.voter_gauge),
            )?;
            put(
                &s.factory_voter,
                format!("alive/{}", s.voter_gauge),
                json!(s.voter_gauge_alive),
            )?;
            for module in [&s.swap_fee_module, &s.unstaked_fee_module] {
                if module.kind == "none" {
                    continue;
                }
                put(&module.address, "code".into(), json!(module.code_sha256))?;
                put(&module.address, "kind".into(), json!(module.kind))?;
                put(
                    &module.address,
                    "template".into(),
                    json!(module.template_sha256),
                )?;
                put(&module.address, "factory".into(), json!(module.factory))?;
                if let Some(d) = &module.dynamic {
                    put(
                        &module.address,
                        "default-scaling-factor".into(),
                        json!(d.default_scaling_factor),
                    )?;
                    put(
                        &module.address,
                        "default-fee-cap".into(),
                        json!(d.default_fee_cap),
                    )?;
                    put(&module.address, "seconds-ago".into(), json!(d.seconds_ago))?;
                    for origin in &s.origins {
                        put(
                            &module.address,
                            format!("discount/{}", origin.origin),
                            json!(origin.discount),
                        )?;
                    }
                }
            }
        } else if pool.protocol == "aerodrome-slipstream" {
            if let Some(core) = &pool.v3 {
                if !core.factory_code_sha256.is_empty() {
                    put(
                        &core.factory,
                        "code".into(),
                        json!(core.factory_code_sha256),
                    )?;
                }
                put(
                    &core.factory,
                    "swap-module".into(),
                    json!(crate::singleton::ZERO_ADDRESS),
                )?;
                put(
                    &core.factory,
                    "unstaked-module".into(),
                    json!(crate::singleton::ZERO_ADDRESS),
                )?;
            }
        } else if pool.protocol == slipstream_context::PROFILE {
            return Err(err("SLIPSTREAM_CONTEXT_REQUIRED"));
        }
    }
    Ok(())
}
