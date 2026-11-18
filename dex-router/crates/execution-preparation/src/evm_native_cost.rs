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
