//! OP fee-only, zero-value accounting. Inputs must already be bound to the
//! signed transaction and execution source. This is not an OP VM or source verifier.
use crate::{evm_native_cost, op_l1_cost, op_packed_cost as packed};
use serde::Serialize;
use std::collections::BTreeMap;
use xone_domain::{err, Amount, Result};

pub const VERSION: &str = "xone-op-chain-cost/1";
pub type Address = [u8; 20];
pub const BASE_FEE_VAULT: Address = vault(0x19);
pub const L1_FEE_VAULT: Address = vault(0x1a);
pub const OPERATOR_FEE_VAULT: Address = vault(0x1b);
const fn vault(last: u8) -> Address {
    let mut a = [0; 20];
    a[0] = 0x42;
    a[19] = last;
    a
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PackedRules {
    Ecotone,
    Fjord,
    Isthmus,
    Jovian,
}
pub enum RollupInputs<'a> {
    Regolith(&'a op_l1_cost::Parameters),
    Packed {
        parameters: &'a packed::PackedParameters,
        rules: PackedRules,
    },
}
pub struct ExecutionInputs<'a> {
    pub gas_used: u64,
    pub gas_limit: u64,
    pub base_fee: &'a Amount,
    pub maximum_gas_price: &'a Amount,
    /// None means a legacy/access-list gas price, not an absent fee context.
    pub priority_gas_price: Option<&'a Amount>,
    pub value: &'a Amount,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChainCost {
    model_version: &'static str,
    attributes_sha256: String,
    encoded_transaction_sha256: String,
    effective_gas_price: Amount,
    execution_fee: Amount,
    base_fee_to_vault: Amount,
    priority_fee_to_beneficiary: Amount,
    l1_data_fee: Amount,
    operator_fee: Amount,
    operator_gas_limit_reserve: Amount,
    gross_charge: Amount,
    upfront_charge: Amount,
    maximum_funding: Amount,
    execution_gas_refund: Amount,
    operator_gas_refund: Amount,
    total_gas_refund: Amount,
}

/// Select only an explicit, supported regular-user interval. Activation
/// deposits, arbitrary native transfers and unsigned-wire estimates are not
/// represented by this accounting contract.
pub fn fee_only_cost(
    execution: &ExecutionInputs<'_>,
    rollup: RollupInputs<'_>,
    signed_wire: &[u8],
    max_bytes: usize,
) -> Result<ChainCost> {
    if execution.value != &Amount::zero() {
        return Err(err("OP_CHAIN_COST_NATIVE_FLOW_UNSUPPORTED"));
    }
    Amount::checked(execution.base_fee.0.clone())?;
    Amount::checked(execution.maximum_gas_price.0.clone())?;
    if let Some(priority) = execution.priority_gas_price {
        Amount::checked(priority.0.clone())?;
    }
    let native = evm_native_cost::measured_native_cost(
        execution.gas_used,
        execution.gas_limit,
        execution.base_fee,
        execution.maximum_gas_price,
        execution.priority_gas_price,
    )?;
    let (attributes_sha256, encoded_transaction_sha256, data_fee, operator_fee, operator_max) =
        match rollup {
            RollupInputs::Regolith(p) => {
                if signed_wire.len() > packed::MAX_ENCODED_BYTES {
                    return Err(err("OP_CHAIN_COST_TRANSACTION_BYTES"));
                }
                let data = op_l1_cost::data_cost(p, signed_wire, max_bytes)?;
                (
                    data.attributes_sha256,
                    data.encoded_transaction_sha256,
                    data.l1_data_fee_native,
                    Amount::zero(),
                    Amount::zero(),
                )
            }
            RollupInputs::Packed { parameters, rules } => {
                let (format, data_rule, operator_rule) = match rules {
                    PackedRules::Ecotone => ("ecotone", packed::DataFeeRule::Ecotone, None),
                    PackedRules::Fjord => ("ecotone", packed::DataFeeRule::Fjord, None),
                    PackedRules::Isthmus => (
                        "isthmus",
                        packed::DataFeeRule::Fjord,
                        Some(packed::OperatorFeeRule::Isthmus),
                    ),
                    PackedRules::Jovian => (
                        "jovian",
                        packed::DataFeeRule::Fjord,
                        Some(packed::OperatorFeeRule::Jovian),
                    ),
                };
                if parameters.format() != format {
                    return Err(err("OP_CHAIN_COST_RULES_MISMATCH"));
                }
                let data = packed::data_cost(parameters, data_rule, signed_wire, max_bytes)?;
                let op = |gas| -> Result<Amount> {
                    match operator_rule {
                        Some(rule) => {
                            Ok(packed::operator_cost(parameters, rule, gas)?.operator_fee_native)
                        }
                        None => Ok(Amount::zero()),
                    }
                };
                (
                    data.parameters.attributes_sha256().to_owned(),
                    data.encoded_transaction_sha256,
                    data.l1_data_fee_native,
                    op(execution.gas_used)?,
                    op(execution.gas_limit)?,
                )
            }
        };
    let gross_charge = Amount::checked(&native.execution_fee.0 + &data_fee.0 + &operator_fee.0)?;
    let upfront_execution = &native.effective_gas_price.0 * execution.gas_limit;
    let upfront_charge = Amount::checked(&upfront_execution + &data_fee.0 + &operator_max.0)?;
    let maximum_funding =
        Amount::checked(&native.gas_limit_reserve.0 + &data_fee.0 + &operator_max.0)?;
    if operator_max < operator_fee
        || upfront_charge < gross_charge
        || maximum_funding < upfront_charge
    {
        return Err(err("OP_CHAIN_COST_CONTEXT"));
    }
    let execution_gas_refund = Amount::checked(upfront_execution - &native.execution_fee.0)?;
    let operator_gas_refund = Amount::checked(&operator_max.0 - &operator_fee.0)?;
    let total_gas_refund = Amount::checked(&execution_gas_refund.0 + &operator_gas_refund.0)?;
    if &gross_charge.0 + &total_gas_refund.0 != upfront_charge.0 {
        return Err(err("OP_CHAIN_COST_CONTEXT"));
    }
    Ok(ChainCost {
        model_version: VERSION,
        attributes_sha256,
        encoded_transaction_sha256,
        effective_gas_price: native.effective_gas_price,
        execution_fee: native.execution_fee,
        base_fee_to_vault: native.base_fee_burned,
        priority_fee_to_beneficiary: native.priority_fee_paid,
        l1_data_fee: data_fee,
        operator_fee,
        operator_gas_limit_reserve: operator_max,
        gross_charge,
        upfront_charge,
        maximum_funding,
        execution_gas_refund,
        operator_gas_refund,
        total_gas_refund,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct PayerCost {
    pub gross_charge: Amount,
    pub fee_credits_returned_to_payer: Amount,
    pub net_native_debit: Amount,
}

impl ChainCost {
    pub fn maximum_funding(&self) -> &Amount {
        &self.maximum_funding
    }

    /// Before balances come from the bound execution source; after balances
    /// come from independent execution. Exact address coverage is required.
    /// Fee roles are aggregated before comparing, including coinbase/vault
    /// aliases. No Ethereum burn assumption is applied to OP base fees.
    pub fn audit_fee_only_balances(
        &self,
        payer: Address,
        beneficiary: Address,
        source_before: &BTreeMap<Address, Amount>,
        execution_after: &BTreeMap<Address, Amount>,
    ) -> Result<PayerCost> {
        let mut credits = BTreeMap::<Address, Amount>::new();
        credits.insert(payer, Amount::zero());
        for (address, amount) in [
            (beneficiary, &self.priority_fee_to_beneficiary),
            (BASE_FEE_VAULT, &self.base_fee_to_vault),
            (L1_FEE_VAULT, &self.l1_data_fee),
            (OPERATOR_FEE_VAULT, &self.operator_fee),
        ] {
            let prior = credits.entry(address).or_insert_with(Amount::zero);
            *prior = Amount::checked(&prior.0 + &amount.0)?;
        }
        if credits.len() != source_before.len() || credits.len() != execution_after.len() {
            return Err(err("OP_CHAIN_COST_BALANCE_COVERAGE"));
        }
        let payer_before = source_before
            .get(&payer)
            .ok_or_else(|| err("OP_CHAIN_COST_BALANCE_COVERAGE"))?;
        if payer_before < &self.maximum_funding {
            return Err(err("OP_CHAIN_COST_MAXIMUM_FUNDING"));
        }
        for (address, credit) in &credits {
            let before = source_before
                .get(address)
                .ok_or_else(|| err("OP_CHAIN_COST_BALANCE_COVERAGE"))?;
            let after = execution_after
                .get(address)
                .ok_or_else(|| err("OP_CHAIN_COST_BALANCE_COVERAGE"))?;
            Amount::checked(before.0.clone())?;
            Amount::checked(after.0.clone())?;
            // Subtract the upfront gross debit before adding credits. A wide
            // before+credit intermediate need not itself fit uint256.
            let debit = if *address == payer {
                &self.gross_charge
            } else {
                &Amount::zero()
            };
            if before < debit || &before.0 - &debit.0 + &credit.0 != after.0 {
                return Err(err("OP_CHAIN_COST_BALANCE_MISMATCH"));
            }
        }
        let returned = credits.remove(&payer).unwrap();
        if returned > self.gross_charge {
            return Err(err("OP_CHAIN_COST_BALANCE_MISMATCH"));
        }
        Ok(PayerCost {
            gross_charge: self.gross_charge.clone(),
            net_native_debit: Amount::checked(&self.gross_charge.0 - &returned.0)?,
            fee_credits_returned_to_payer: returned,
        })
    }
}
