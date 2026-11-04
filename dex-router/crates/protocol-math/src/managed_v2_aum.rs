//! Published 20230411 ManagedPool AUM fees, distinct from old swap-share BPT.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Amount, Result};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AumAccrual {
    pub total_bpt: Amount,
    pub protocol_bpt: Amount,
    pub manager_bpt: Amount,
    pub actual_supply: Amount,
}
fn checked(v: BigUint) -> Result<BigUint> {
    if v.bits() > 256 {
        return Err(err("MANAGED_V2_UINT256_OVERFLOW"));
    }
    Ok(v)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AumSupplyView {
    pub pending_bpt: Amount,
    pub actual_supply: Amount,
}
/// The actual getActualSupply view does not distribute protocol/manager BPT.
/// Keep it separate: collecting may fail in a later checked percentage product.
pub fn view_aum(
    virtual_supply: &Amount,
    current_timestamp: u64,
    last_collection_timestamp: u32,
    annual_fee: &Amount,
) -> Result<AumSupplyView> {
    let one = BigUint::from(1_000_000_000_000_000_000u64);
    if virtual_supply.0.bits() > 256 || annual_fee.0 > BigUint::from(950_000_000_000_000_000u64) {
        return Err(err("MANAGED_V2_AUM_PARAMETERS"));
    }
    if current_timestamp <= u64::from(last_collection_timestamp) || annual_fee.0.is_zero() {
        return Ok(AumSupplyView {
            pending_bpt: Amount::zero(),
            actual_supply: virtual_supply.clone(),
        });
    }
    let annual = checked(&virtual_supply.0 * &annual_fee.0)? / (&one - &annual_fee.0);
    let elapsed = BigUint::from(current_timestamp - u64::from(last_collection_timestamp));
    let pending = checked(annual * elapsed)? / BigUint::from(365u32 * 86400u32);
    let actual = checked(&virtual_supply.0 + &pending)?;
    Ok(AumSupplyView {
        pending_bpt: Amount(pending),
        actual_supply: Amount(actual),
    })
}
/// ExternalFees annual ownership inflation floors once, then ExternalAUMFees
/// floors elapsed/year. Splitting uses FixedPoint.mulUp for the protocol.
/// This is a preview: updating the packed uint32 collection time is separate.
pub fn preview_aum(
    virtual_supply: &Amount,
    current_timestamp: u64,
    last_collection_timestamp: u32,
    annual_fee: &Amount,
    cached_protocol_share: &Amount,
) -> Result<AumAccrual> {
    let one = BigUint::from(1_000_000_000_000_000_000u64);
    if virtual_supply.0.bits() > 256
        || annual_fee.0 > BigUint::from(950_000_000_000_000_000u64)
        || cached_protocol_share.0 > one
    {
        return Err(err("MANAGED_V2_AUM_PARAMETERS"));
    }
    let view = view_aum(
        virtual_supply,
        current_timestamp,
        last_collection_timestamp,
        annual_fee,
    )?;
    let total = view.pending_bpt.0;
    let product = checked(&total * &cached_protocol_share.0)?;
    // Original mulUp avoids an overflowing product + ONE - 1 formulation.
    let protocol = if product.is_zero() {
        BigUint::zero()
    } else {
        (&product - BigUint::one()) / &one + BigUint::one()
    };
    if protocol > total {
        return Err(err("MANAGED_V2_PROTOCOL_SPLIT"));
    }
    let manager = &total - &protocol;
    let actual = view.actual_supply.0;
    Ok(AumAccrual {
        total_bpt: Amount(total),
        protocol_bpt: Amount(protocol),
        manager_bpt: Amount(manager),
        actual_supply: Amount(actual),
    })
}

/// Monetary collection inputs. `vault_bpt_balance` is this Pool's registered
/// Vault cash + managed BPT, not the Vault's aggregate ERC20 wallet balance.
/// The full-state adapter derives recipient equality from actual addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AumCollectionInput {
    pub total_supply: Amount,
    pub vault_bpt_balance: Amount,
    pub manager_bpt_balance: Amount,
    pub protocol_bpt_balance: Amount,
    pub same_recipient: bool,
    pub last_collection_timestamp: u32,
    pub annual_fee: Amount,
    pub cached_protocol_share: Amount,
    pub paused: bool,
    pub buffer_period_end: u64,
    pub recovery_mode: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AumCollection {
    pub accrual: AumAccrual,
    pub next: AumCollectionInput,
    pub virtual_supply_after: Amount,
    pub actual_supply_after: Amount,
}
/// Exact public collection monetary transition. Caller/source/version checks
/// and the actual contract identities are obligations of the full-state adapter.
pub fn collect_aum(input: &AumCollectionInput, execution_timestamp: u64) -> Result<AumCollection> {
    if execution_timestamp > u64::from(u32::MAX) {
        return Err(err("MANAGED_V2_COLLECTION_TIME_WIDTH"));
    }
    if input.paused && execution_timestamp <= input.buffer_period_end {
        return Err(err("MANAGED_V2_PAUSED"));
    }
    for value in [
        &input.total_supply,
        &input.vault_bpt_balance,
        &input.manager_bpt_balance,
        &input.protocol_bpt_balance,
    ] {
        if value.0.bits() > 256 {
            return Err(err("MANAGED_V2_UINT256_OVERFLOW"));
        }
    }
    if input.vault_bpt_balance.0.bits() > 112 || input.total_supply.0 <= input.vault_bpt_balance.0 {
        return Err(err("MANAGED_V2_UNINITIALIZED_OR_BPT_BALANCE"));
    }
    if input.manager_bpt_balance.0 > input.total_supply.0
        || input.protocol_bpt_balance.0 > input.total_supply.0
    {
        return Err(err("MANAGED_V2_BPT_LEDGER"));
    }
    if input.same_recipient {
        if input.manager_bpt_balance != input.protocol_bpt_balance {
            return Err(err("MANAGED_V2_RECIPIENT_ALIAS"));
        }
    } else if &input.manager_bpt_balance.0 + &input.protocol_bpt_balance.0 > input.total_supply.0 {
        return Err(err("MANAGED_V2_BPT_LEDGER"));
    }
    let virtual_before = Amount(&input.total_supply.0 - &input.vault_bpt_balance.0);
    let effective_fee = if input.recovery_mode {
        Amount::zero()
    } else {
        input.annual_fee.clone()
    };
    let accrual = preview_aum(
        &virtual_before,
        execution_timestamp,
        input.last_collection_timestamp,
        &effective_fee,
        &input.cached_protocol_share,
    )?;
    let mut next = input.clone();
    next.last_collection_timestamp = execution_timestamp as u32;
    next.total_supply = Amount(checked(&input.total_supply.0 + &accrual.total_bpt.0)?);
    if input.same_recipient {
        let balance = Amount(checked(
            &input.manager_bpt_balance.0 + &accrual.total_bpt.0,
        )?);
        next.manager_bpt_balance = balance.clone();
        next.protocol_bpt_balance = balance;
    } else {
        next.manager_bpt_balance = Amount(checked(
            &input.manager_bpt_balance.0 + &accrual.manager_bpt.0,
        )?);
        next.protocol_bpt_balance = Amount(checked(
            &input.protocol_bpt_balance.0 + &accrual.protocol_bpt.0,
        )?);
    }
    let virtual_after = Amount(&next.total_supply.0 - &next.vault_bpt_balance.0);
    // At the same selected clock no uncollected time remains. Actual supply is
    // continuous: before virtual+pending equals after virtual+zero pending.
    if virtual_after != accrual.actual_supply {
        return Err(err("MANAGED_V2_SUPPLY_CONTINUITY"));
    }
    Ok(AumCollection {
        accrual,
        next,
        virtual_supply_after: virtual_after.clone(),
        actual_supply_after: virtual_after,
    })
}
