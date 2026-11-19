//! Native execution-cost arithmetic only. No untrusted token price or implicit
//! L2/approval zero is inserted when the full chain fee coverage is absent.
use xone_domain::{err, Amount, Result};

pub const VERSION: &str = "xone-evm-native-cost/1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCost {
    pub effective_gas_price: Amount,
    pub execution_fee: Amount,
    pub base_fee_burned: Amount,
    pub priority_fee_paid: Amount,
    pub gas_limit_reserve: Amount,
}
pub fn measured_native_cost(
    gas_used: u64,
    gas_limit: u64,
    base_fee: &Amount,
    maximum: &Amount,
    priority: Option<&Amount>,
) -> Result<NativeCost> {
    if gas_used < 21_000 || gas_used > gas_limit || maximum < base_fee {
        return Err(err("EVM_NATIVE_COST_CONTEXT"));
    }
    let tip = if let Some(priority) = priority {
        if priority > maximum {
            return Err(err("EVM_NATIVE_COST_CONTEXT"));
        }
        priority.0.clone().min(&maximum.0 - &base_fee.0)
    } else {
        &maximum.0 - &base_fee.0
    };
    let effective_gas_price = Amount::checked(&base_fee.0 + &tip)?;
    let gas = Amount(gas_used.into());
    let execution_fee = Amount::checked(&gas.0 * &effective_gas_price.0)?;
    let base_fee_burned = Amount::checked(&gas.0 * &base_fee.0)?;
    let priority_fee_paid = Amount::checked(&gas.0 * tip)?;
    let gas_limit_reserve = reserve(gas_limit, maximum)?;
    if &base_fee_burned.0 + &priority_fee_paid.0 != execution_fee.0
        || execution_fee > gas_limit_reserve
    {
        return Err(err("EVM_NATIVE_COST_CONTEXT"));
    }
    Ok(NativeCost {
        effective_gas_price,
        execution_fee,
        base_fee_burned,
        priority_fee_paid,
        gas_limit_reserve,
    })
}

/// Admission reserve is a maximum, never the actual execution charge/refund.
pub fn reserve(gas_limit: u64, maximum: &Amount) -> Result<Amount> {
    if gas_limit == 0 {
        return Err(err("EVM_NATIVE_COST_CONTEXT"));
    }
    Amount::checked(maximum.0.clone())?;
    Amount::checked(&maximum.0 * gas_limit)
}

/// Reconcile the existing zero-value bound ERC20 transaction scope. Native
/// value transfers/unwraps require their own complete flow accounting.
#[derive(Clone, Debug)]
pub struct NativeBalances {
    pub payer_before: Amount,
    pub payer_after: Amount,
    pub beneficiary_before: Amount,
    pub beneficiary_after: Amount,
}

pub fn reconcile_native_balances(
    cost: &NativeCost,
    balances: &NativeBalances,
    same_account: bool,
) -> Result<()> {
    for value in [
        &balances.payer_before,
        &balances.payer_after,
        &balances.beneficiary_before,
        &balances.beneficiary_after,
    ] {
        Amount::checked(value.0.clone())?;
    }
    if same_account {
        if balances.payer_before != balances.beneficiary_before
            || balances.payer_after != balances.beneficiary_after
            || balances.payer_before < cost.base_fee_burned
            || &balances.payer_before.0 - &cost.base_fee_burned.0 != balances.payer_after.0
        {
            return Err(err("EVM_NATIVE_BALANCE_MISMATCH"));
        }
    } else if balances.payer_before < cost.execution_fee
        || &balances.payer_before.0 - &cost.execution_fee.0 != balances.payer_after.0
        || Amount::checked(&balances.beneficiary_before.0 + &cost.priority_fee_paid.0)?
            != balances.beneficiary_after
    {
        return Err(err("EVM_NATIVE_BALANCE_MISMATCH"));
    }
    Ok(())
}

pub const PAYER_COST_VERSION: &str = "source-bound-native-payer/1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayerCost {
    pub gross_execution_charge: Amount,
    pub priority_returned_to_payer: Amount,
    pub net_native_debit: Amount,
}

/// A low final debit cannot bypass the original transaction's maximum funding
/// admission, including when the payer earns its own priority fee.
pub fn validate_maximum_funding(cost: &NativeCost, payer_before: &Amount) -> Result<()> {
    Amount::checked(payer_before.0.clone())?;
    if payer_before < &cost.gas_limit_reserve {
        return Err(err("EVM_NATIVE_MAXIMUM_FUNDING"));
    }
    Ok(())
}

pub fn audited_payer_cost(
    cost: &NativeCost,
    balances: &NativeBalances,
    same_account: bool,
) -> Result<PayerCost> {
    validate_maximum_funding(cost, &balances.payer_before)?;
    reconcile_native_balances(cost, balances, same_account)?;
    let returned = if same_account {
        cost.priority_fee_paid.clone()
    } else {
        Amount::zero()
    };
    let net = Amount::checked(&balances.payer_before.0 - &balances.payer_after.0)?;
    if &net.0 + &returned.0 != cost.execution_fee.0 {
        return Err(err("EVM_NATIVE_BALANCE_MISMATCH"));
    }
    Ok(PayerCost {
        gross_execution_charge: cost.execution_fee.clone(),
        priority_returned_to_payer: returned,
        net_native_debit: net,
    })
}

/// Minimum opening balance when one payer executes sequential zero-value calls.
/// The unused gas reserve from each call is available for the next call.
pub fn sequence_funding<'a>(
    operations: impl IntoIterator<Item = (&'a Amount, &'a Amount)>,
) -> Result<(Amount, Amount)> {
    let mut minimum = Amount::zero();
    let mut spent = Amount::zero();
    for (reserve, debit) in operations {
        if debit > reserve {
            return Err(err("EVM_NATIVE_COST_CONTEXT"));
        }
        minimum = minimum.max(Amount::checked(&spent.0 + &reserve.0)?);
        spent = Amount::checked(&spent.0 + &debit.0)?;
    }
    Ok((minimum, spent))
}
