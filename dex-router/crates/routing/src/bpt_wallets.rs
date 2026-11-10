//! Carry observed Managed BPT wallets through external standard-token legs.
//! Registered BPT balances and supply belong to the issuer pool and do not
//! change when its ERC20 token moves through another pool's custody.
use crate::Funds;
use std::collections::BTreeMap;
use xone_domain::{err, Amount, Pool, Result};
fn transfer(
    balance: &mut Amount,
    holder: &str,
    from: &str,
    to: &str,
    value: &Amount,
) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if holder == from {
        if *balance < *value {
            return Err(err("MANAGED_V2_BPT_INSUFFICIENT_BALANCE"));
        }
        *balance = Amount(&balance.0 - &value.0);
    } else if holder == to {
        *balance = Amount::checked(&balance.0 + &value.0)?;
    }
    Ok(())
}
pub(super) fn propagate(
    pools: &mut BTreeMap<String, Pool>,
    selected: &Pool,
    token_in: &str,
    token_out: &str,
    input: &Amount,
    output: &Amount,
    funds: Option<Funds<'_>>,
) -> Result<()> {
    let owners: Vec<_> = pools
        .values()
        .filter(|p| {
            p.pool_id != selected.pool_id
                && (p.pool_id == token_in || p.pool_id == token_out)
                && p.multi_asset
                    .as_ref()
                    .is_some_and(|s| s.managed_v2.is_some())
        })
        .map(|p| p.pool_id.clone())
        .collect();
    if owners.is_empty() {
        return Ok(());
    }
    let funds = funds.ok_or_else(|| err("MANAGED_V2_BPT_FUNDS_REQUIRED"))?;
    // Standard V2 pairs hold both tokens at the pair address. Other adapters
    // need their own custody/transfer semantics before this path can model them.
    if xone_domain::protocols::protocol(&selected.protocol).is_none_or(|p| p.family != "v2") {
        return Err(err("MANAGED_V2_EXTERNAL_BPT_CUSTODY_UNSUPPORTED"));
    }
    xone_domain::protocols::validate_address(&selected.pool_id)?;
    if funds.sender == selected.pool_id || funds.recipient == selected.pool_id {
        // The reserve-only V2 kernel assumes actual input credit and output
        // debit at the pair. A pair paying itself has different settlement.
        return Err(err("MANAGED_V2_EXTERNAL_BPT_FUNDS_ALIAS_UNSUPPORTED"));
    }
    for id in owners {
        let owner = pools.get_mut(&id).ok_or_else(|| err("MISSING_POOL"))?;
        let s = owner
            .multi_asset
            .as_mut()
            .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
        let m = s
            .managed_v2
            .as_mut()
            .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
        let (from, to, amount) = if id == token_in {
            (funds.sender, selected.pool_id.as_str(), input)
        } else {
            (selected.pool_id.as_str(), funds.recipient, output)
        };
        transfer(&mut m.vault_bpt_actual_balance, &s.vault, from, to, amount)?;
        transfer(&mut m.manager_bpt_balance, &m.manager, from, to, amount)?;
        transfer(
            &mut m.protocol_collector_bpt_balance,
            &m.protocol_collector,
            from,
            to,
            amount,
        )?;
        m.execution_context.get_or_insert(
            xone_domain::managed_v2_state::ManagedV2ExecutionContext {
                block_number: m.block_number,
                timestamp: m.timestamp,
            },
        );
        owner.validate()?;
    }
    Ok(())
}
