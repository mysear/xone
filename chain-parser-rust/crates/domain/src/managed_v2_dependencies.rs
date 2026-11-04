//! Cross-pool facts and execution scope for Managed V2 shared Vault wallets.
//! Unsupported execution combinations remain valid canonical observations.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    err,
    managed_v2_state::{BalancerManagedV2State, ManagedV2ExecutionContext, PROTOCOL},
    Pool, Result,
};

fn managed(pool: &Pool) -> Option<&BalancerManagedV2State> {
    pool.multi_asset.as_ref()?.managed_v2.as_ref()
}

fn clock(state: &BalancerManagedV2State) -> ManagedV2ExecutionContext {
    state
        .execution_context
        .clone()
        .unwrap_or(ManagedV2ExecutionContext {
            block_number: state.block_number,
            timestamp: state.timestamp,
        })
}

/// Validate declared shared facts. General callers may supply multiple chains;
/// canonical source-header and derived-context rules remain with Snapshot.
pub fn validate<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let pools: Vec<_> = pools.into_iter().collect();
    // Validate each Managed declaration before indexing any token information.
    for pool in &pools {
        if pool.protocol == PROTOCOL || managed(pool).is_some() {
            pool.validate()?;
        }
    }
    let mut physical = BTreeMap::new();
    let mut codes = BTreeMap::new();
    let mut templates = BTreeMap::new();
    let mut live = BTreeMap::new();
    for pool in &pools {
        if let Some(previous) = physical.insert((&pool.chain_key, &pool.pool_id), *pool) {
            if previous != *pool {
                return Err(err("DUPLICATE_PHYSICAL_POOL_CONFLICT"));
            }
        }
        let Some(state) = managed(pool) else { continue };
        let multi = pool.multi_asset.as_ref().expect("validated Managed state");
        let mut observations = vec![
            (&pool.pool_id, &multi.code_sha256),
            (&multi.vault, &multi.vault_code_sha256),
            (&state.factory, &state.factory_code_sha256),
            (&state.fee_provider, &state.fee_provider_code_sha256),
            (
                &state.protocol_collector,
                &state.protocol_collector_code_sha256,
            ),
        ];
        observations.extend(
            multi
                .assets
                .iter()
                .map(|asset| (&asset.token, &asset.code_sha256)),
        );
        observations.extend(
            state
                .dependencies
                .iter()
                .map(|dependency| (&dependency.address, &dependency.code_sha256)),
        );
        for (address, code) in observations {
            let key = (
                &pool.chain_key,
                address,
                state.block_number,
                &state.block_hash,
                state.timestamp,
            );
            if let Some(previous) = codes.insert(key, code) {
                if previous != code {
                    let reason = if address == &multi.vault {
                        "SHARED_VAULT_IDENTITY"
                    } else if multi.assets.iter().any(|asset| &asset.token == address) {
                        "SHARED_TOKEN_IDENTITY"
                    } else {
                        "SHARED_CONTRACT_IDENTITY"
                    };
                    return Err(err(reason));
                }
            }
        }
        for dependency in &state.dependencies {
            let key = (
                &pool.chain_key,
                &dependency.address,
                state.block_number,
                &state.block_hash,
                state.timestamp,
            );
            if let Some(previous) = templates.insert(key, &dependency.template_sha256) {
                if previous != &dependency.template_sha256 {
                    return Err(err("SHARED_CONTRACT_TEMPLATE"));
                }
            }
        }
        let key = (
            &pool.chain_key,
            &state.fee_provider,
            &state.provider_fee_id,
            state.block_number,
            &state.block_hash,
            state.timestamp,
        );
        if let Some(previous) = live.insert(key, &state.live_provider_aum_percentage) {
            if previous != &state.live_provider_aum_percentage {
                return Err(err("SHARED_PROVIDER_PERCENTAGE"));
            }
        }
    }
    let mut vaults = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut wallets = BTreeMap::new();
    for pool in &pools {
        let Some(state) = managed(pool) else { continue };
        let multi = pool.multi_asset.as_ref().expect("validated Managed state");
        let key = (&pool.chain_key, &multi.vault);
        let source = (state.block_number, &state.block_hash, state.timestamp);
        let context = clock(state);
        let pause = (
            state.vault_paused,
            state.vault_pause_window_end,
            state.vault_buffer_period_end,
        );
        if let Some((code, previous_source, previous_clock, previous_pause)) = vaults.get(&key) {
            if *code != &multi.vault_code_sha256 {
                return Err(err("SHARED_VAULT_IDENTITY"));
            }
            if *previous_source != source {
                return Err(err("SHARED_VAULT_SOURCE"));
            }
            if *previous_clock != context {
                return Err(err("SHARED_VAULT_CLOCK"));
            }
            if *previous_pause != pause {
                return Err(err("SHARED_VAULT_PAUSE"));
            }
        } else {
            vaults.insert(key, (&multi.vault_code_sha256, source, context, pause));
        }
        // Include the own BPT wallet: another Managed pool can declare that
        // same ERC20 as an underlying even when execution is unsupported.
        for (token, actual) in std::iter::once((&pool.pool_id, &state.vault_bpt_actual_balance))
            .chain(
                multi
                    .assets
                    .iter()
                    .zip(&state.token_info)
                    .map(|(asset, info)| (&asset.token, &info.vault_actual_balance)),
            )
        {
            if let Some(previous) = wallets.insert((&pool.chain_key, &multi.vault, token), actual) {
                if previous != actual {
                    return Err(err("SHARED_VAULT_WALLET"));
                }
            }
        }
        // Scaling has protocol-specific meaning. Compare Managed underlyings
        // only, including across different Vaults on the same chain.
        for asset in &multi.assets {
            let identity = (&asset.code_sha256, &asset.scaling);
            if let Some(previous) = identities.insert((&pool.chain_key, &asset.token), identity) {
                if previous != identity {
                    return Err(err("SHARED_TOKEN_IDENTITY"));
                }
            }
        }
    }
    // CASH is physically backed by the shared wallet; managed funds are
    // external. Sum distinct declared registrations only, not total balances.
    let mut cash = BTreeMap::new();
    let mut registrations = BTreeSet::new();
    for pool in &pools {
        let Some(state) = managed(pool) else { continue };
        let multi = pool.multi_asset.as_ref().expect("validated Managed state");
        for (token, amount) in std::iter::once((&pool.pool_id, &state.vault_bpt_cash)).chain(
            multi
                .assets
                .iter()
                .zip(&state.token_info)
                .map(|(asset, info)| (&asset.token, &info.cash)),
        ) {
            if !registrations.insert((&pool.chain_key, &multi.vault, &pool.pool_id, token)) {
                continue;
            }
            let value = cash
                .entry((&pool.chain_key, &multi.vault, token))
                .or_insert_with(|| num_bigint::BigUint::from(0u8));
            *value += &amount.0;
        }
    }
    for (key, value) in cash {
        if value.bits() > 256 || value > wallets[&key].0 {
            return Err(err("SHARED_VAULT_REGISTERED_CASH"));
        }
    }
    // A mixed adapter still declares a Vault code identity, even though it
    // cannot declare the Managed actual-wallet/source/clock fields.
    for pool in &pools {
        let Some(multi) = &pool.multi_asset else {
            continue;
        };
        if let Some((code, _, _, _)) = vaults.get(&(&pool.chain_key, &multi.vault)) {
            if *code != &multi.vault_code_sha256 {
                return Err(err("SHARED_VAULT_IDENTITY"));
            }
        }
    }
    Ok(())
}

/// Report known unsupported execution groups after validating their facts.
/// Keep the complete source bank; callers use these IDs to filter candidates.
pub fn blocked_pools<'a>(
    pools: impl IntoIterator<Item = &'a Pool>,
) -> Result<BTreeMap<String, String>> {
    let pools: Vec<_> = pools.into_iter().collect();
    validate(pools.iter().copied())?;
    let mut groups: BTreeMap<_, Vec<&Pool>> = BTreeMap::new();
    for pool in &pools {
        if let Some(multi) = &pool.multi_asset {
            if !multi.vault.is_empty() {
                groups
                    .entry((&pool.chain_key, &multi.vault))
                    .or_default()
                    .push(pool);
            }
        }
    }
    let mut blocked = BTreeMap::new();
    for group in groups.values() {
        if !group.iter().any(|pool| managed(pool).is_some()) {
            continue;
        }
        let physical: BTreeSet<_> = group.iter().map(|pool| &pool.pool_id).collect();
        let bpt = group
            .iter()
            .filter(|pool| managed(pool).is_some())
            .any(|pool| {
                pool.multi_asset
                    .as_ref()
                    .expect("Managed state")
                    .assets
                    .iter()
                    .any(|asset| asset.token != pool.pool_id && physical.contains(&asset.token))
            });
        let reason = if bpt {
            "SHARED_VAULT_BPT_UNSUPPORTED"
        } else if group.iter().any(|pool| managed(pool).is_none()) {
            "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED"
        } else {
            continue;
        };
        for pool in group {
            blocked.insert(pool.pool_id.clone(), reason.to_owned());
        }
    }
    Ok(blocked)
}
