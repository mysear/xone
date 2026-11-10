//! Immutable LM bindings and shared MasterChef period inputs in a single bank.
use crate::{err, Pool, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub fn validate<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let mut facts: BTreeMap<(String, String, String), Value> = BTreeMap::new();
    for pool in pools {
        let mut put = |address: &str, key: String, value: Value| -> Result<()> {
            let id = (pool.chain_key.clone(), address.to_owned(), key);
            if let Some(old) = facts.get(&id) {
                if old != &value {
                    return Err(err("PANCAKE_LM_SHARED_READONLY_CONFLICT"));
                }
            } else {
                facts.insert(id, value);
            }
            Ok(())
        };
        if let Some(s) = pool
            .pancake_lm
            .as_ref()
            .or_else(|| pool.pancake_bsc_lm.as_ref().map(|s| &s.ledger))
        {
            for (a, c) in [
                (&pool.pool_id, &s.core.code_sha256),
                (&s.core.factory, &s.core.factory_code_sha256),
                (&pool.token0, &s.token0_code_sha256),
                (&pool.token1, &s.token1_code_sha256),
                (&s.lm.address, &s.lm.code_sha256),
                (&s.master_chef.address, &s.master_chef.code_sha256),
            ] {
                put(a, "code".into(), json!(c))?;
            }
            let chef = &s.master_chef;
            put(
                &chef.address,
                "period-inputs".into(),
                json!([
                    chef.template_sha256,
                    chef.cake,
                    chef.weth,
                    chef.position_manager,
                    chef.total_alloc_point,
                    chef.latest_period_cake_per_second,
                    chef.latest_period_end_time
                ]),
            )?;
            put(
                &chef.address,
                format!("pid/{}", chef.pool_pid),
                json!([
                    chef.pool_address,
                    chef.pool_token0,
                    chef.pool_token1,
                    chef.pool_fee,
                    chef.pool_alloc_point
                ]),
            )?;
            put(
                &chef.address,
                format!("pool-pid/{}", pool.pool_id),
                json!(chef.pool_pid),
            )?;
            put(
                &s.lm.address,
                "immutable-bindings".into(),
                json!([s.lm.template_sha256, s.lm.pool, s.lm.master_chef]),
            )?;
            put(&pool.pool_id, "lm".into(), json!(s.lm.address))?;
            put(
                &s.core.factory,
                format!(
                    "pool-key/{}/{}/{}",
                    pool.token0, pool.token1, pool.fee_numerator
                ),
                json!(pool.pool_id),
            )?;
        } else if pool.protocol == "pancake-v3" {
            if let Some(s) = &pool.v3 {
                put(
                    &pool.pool_id,
                    "lm".into(),
                    json!(crate::singleton::ZERO_ADDRESS),
                )?;
                put(&pool.pool_id, "code".into(), json!(s.code_sha256))?;
                if !s.factory_code_sha256.is_empty() {
                    put(&s.factory, "code".into(), json!(s.factory_code_sha256))?;
                }
            }
        }
        if let Some(s) = &pool.pancake_bsc_lm {
            for predecessor in &s.predecessors {
                put(
                    &predecessor.address,
                    "code".into(),
                    json!(predecessor.code_sha256),
                )?;
                put(
                    &predecessor.address,
                    "predecessor-bank".into(),
                    json!(predecessor),
                )?;
            }
        }
        if let Some(s) = &pool.slipstream_context {
            for (a, c) in [
                (&pool.pool_id, &s.core.code_sha256),
                (&s.core.factory, &s.core.factory_code_sha256),
                (&pool.token0, &s.token0_code_sha256),
                (&pool.token1, &s.token1_code_sha256),
            ] {
                put(a, "code".into(), json!(c))?;
            }
        }
    }
    Ok(())
}
