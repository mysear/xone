//! Propagate Managed V2 global Vault wallets after a complete successful swap.
//! Shared facts and execution capability ownership live in xone-domain.
//! Scope: Managed V2 swaps of ordinary underlying tokens in one chain. Known
//! same-Vault cross-pool BPT underlyings and mixed adapters fail explicitly.
use std::collections::BTreeMap;
use xone_domain::{
    err,
    managed_v2_state::{BalancerManagedV2State, ManagedV2ExecutionContext},
    Amount, Pool, Result,
};
fn managed(p: &Pool) -> Option<&BalancerManagedV2State> {
    p.multi_asset.as_ref()?.managed_v2.as_ref()
}
fn context(m: &BalancerManagedV2State) -> ManagedV2ExecutionContext {
    m.execution_context
        .clone()
        .unwrap_or(ManagedV2ExecutionContext {
            block_number: m.block_number,
            timestamp: m.timestamp,
        })
}
/// Quote math already applied full gross input and raw output to selected
/// global wallets. Copy just changed token wallets, never registered balances.
/// Validate every sibling replacement before installing any of them.
pub(super) fn propagate(pools: &mut BTreeMap<String, Pool>, next: &Pool) -> Result<()> {
    let Some(nm) = managed(next) else {
        return Ok(());
    };
    next.validate()?;
    let ns = next
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let previous = pools
        .get(&next.pool_id)
        .ok_or_else(|| err("MISSING_POOL"))?;
    let pm = managed(previous).ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let clock = context(nm);
    let changed: BTreeMap<&str, &Amount> = ns
        .assets
        .iter()
        .zip(nm.token_info.iter().zip(&pm.token_info))
        .filter(|(_, (new, old))| new.vault_actual_balance != old.vault_actual_balance)
        .map(|(a, (info, _))| (a.token.as_str(), &info.vault_actual_balance))
        .collect();
    let mut replacements = Vec::new();
    for other in pools.values() {
        if other.pool_id == next.pool_id || other.chain_key != next.chain_key {
            continue;
        }
        let Some(os) = &other.multi_asset else {
            continue;
        };
        if os.vault != ns.vault {
            continue;
        }
        let affected = os
            .assets
            .iter()
            .any(|a| changed.contains_key(a.token.as_str()));
        let clock_changed = managed(other).is_some_and(|m| context(m) != clock);
        if !affected && !clock_changed {
            continue;
        }
        if managed(other).is_none() {
            return Err(err("SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED"));
        }
        // A future Vault clock belongs to every Managed sibling, including
        // disjoint tokens. Each sibling projects its own raw schedule and AUM.
        let mut candidate = xone_protocol_math::managed_v2_pool::reclock(
            other,
            clock.block_number,
            clock.timestamp,
        )?;
        let cs = candidate
            .multi_asset
            .as_mut()
            .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
        let cm = cs
            .managed_v2
            .as_mut()
            .ok_or_else(|| err("SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED"))?;
        for (asset, info) in cs.assets.iter().zip(&mut cm.token_info) {
            if let Some(wallet) = changed.get(asset.token.as_str()) {
                info.vault_actual_balance = (*wallet).clone();
            }
        }
        // Includes uint256 wallet width and wallet >= own CASH, not assets.
        candidate.validate()?;
        replacements.push(candidate);
    }
    for candidate in replacements {
        pools.insert(candidate.pool_id.clone(), candidate);
    }
    Ok(())
}
