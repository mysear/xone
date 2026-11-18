//! Fee-receipt consistency against a separately retained authenticated request.
//! This performs no VM execution and does not attest the reported transition.
use super::{
    op_fees, ExecutionControl, ExecutionError, ExecutionFailure, ExecutionLimits,
    OpExecutionRequest, OpFork, StateImage, StrictDB, Telemetry,
};
use revm::primitives::{EVMError, TxKind, U256};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use xone_domain::Amount;
use xone_execution_preparation::op_chain_cost::{ChainCost, PayerCost};

#[derive(Debug)]
pub struct AuditedOpFeeMeasurements {
    pub costs: ChainCost,
    pub payer_cost: PayerCost,
    /// Source reads consumed by this audit, additional to worker execution.
    pub telemetry: Telemetry,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Balances {
    before: Amount,
    after: Amount,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report<'a> {
    transaction_hash: &'a str,
    transaction_type: u8,
    #[serde(rename = "chain_cost")]
    _chain_cost: serde::de::IgnoredAny,
    #[serde(rename = "payer_cost")]
    _payer_cost: serde::de::IgnoredAny,
    #[serde(borrow)]
    accounts: BTreeMap<&'a str, Balances>,
}

/// Recompute every fee from the retained signature and source attributes,
/// require source balances and maximum funding, and reconcile all fee roles.
/// The caller must aggregate these reads with its other work under one budget.
pub fn audit_op_fee_measurements(
    image: Arc<StateImage>,
    request: &OpExecutionRequest,
    gas_used: u64,
    reported: &Value,
    limits: ExecutionLimits,
    control: ExecutionControl,
) -> Result<AuditedOpFeeMeasurements, ExecutionError> {
    let bad = || ExecutionFailure::InvalidOpContext("OP fee receipt/source mismatch");
    let database = |e| ExecutionFailure::Evm(Box::new(EVMError::Database(e)));
    let mut db = StrictDB::new(image.clone(), limits.reads, control);
    let result = (|| {
        db.check_control().map_err(database)?;
        let tx = request.transaction.environment();
        let time = u64::try_from(request.block.timestamp).map_err(|_| bad())?;
        let wire = tx.optimism.enveloped_tx.as_ref().ok_or_else(bad)?;
        if OpFork::at(tx.chain_id.unwrap(), time) != Some(request.fork)
            || !tx.value.is_zero()
            || !matches!(tx.transact_to, TxKind::Call(_))
            || request.attributes_calldata.len() > 260
        {
            return Err(bad());
        }
        if tx.gas_limit > limits.max_gas_limit {
            return Err(ExecutionFailure::GasLimit {
                requested: tx.gas_limit,
                maximum: limits.max_gas_limit,
            });
        }
        if wire.len() > limits.max_input_bytes {
            return Err(ExecutionFailure::InputLimit {
                actual: wire.len(),
                maximum: limits.max_input_bytes,
            });
        }
        // Bound deserialization without cloning untrusted nested JSON values.
        if reported["accounts"].as_object().is_none_or(|a| a.len() > 5) {
            return Err(bad());
        }
        let report = Report::deserialize(reported).map_err(|_| bad())?;
        op_fees::validate_bank(
            &mut db,
            &request.attributes_calldata,
            request.fork,
            tx,
            &request.block,
        )?;
        let expected = op_fees::calculate(
            request.fork,
            tx,
            request.block.basefee,
            time,
            &request.attributes_calldata,
            gas_used,
        )?;
        if report.transaction_hash != format!("{:#x}", request.transaction.transaction_hash())
            || report.transaction_type != request.transaction.transaction_type()
            || reported["chain_cost"] != serde_json::to_value(&expected).map_err(|_| bad())?
        {
            return Err(bad());
        }
        let mut before = BTreeMap::new();
        let mut after = BTreeMap::new();
        for owner in op_fees::accounts(tx, request.block.coinbase) {
            // validate_bank has already charged and required this exact source account.
            let original = image.data().accounts.get(&owner).ok_or_else(bad)?;
            if owner == tx.caller
                && original.as_ref().map_or(0, |a| a.nonce) != tx.nonce.ok_or_else(bad)?
            {
                return Err(bad());
            }
            let balance = original.as_ref().map_or(U256::ZERO, |a| a.balance);
            let balance = Amount::from_be_bytes(&balance.to_be_bytes::<32>()).map_err(|_| bad())?;
            let name = format!("{owner:#x}");
            let balances = report.accounts.get(name.as_str()).ok_or_else(bad)?;
            if balances.before != balance {
                return Err(bad());
            }
            before.insert(owner.into_array(), balance);
            after.insert(owner.into_array(), balances.after.clone());
        }
        if before.len() != report.accounts.len() {
            return Err(bad());
        }
        let payer = expected
            .audit_fee_only_balances(
                tx.caller.into_array(),
                request.block.coinbase.into_array(),
                &before,
                &after,
            )
            .map_err(|_| bad())?;
        if reported["payer_cost"] != serde_json::to_value(&payer).map_err(|_| bad())? {
            return Err(bad());
        }
        db.check_control().map_err(database)?;
        Ok((expected, payer))
    })();
    let telemetry = Telemetry {
        reads: db.usage(),
        ..Default::default()
    };
    let (costs, payer_cost) = result.map_err(|failure| ExecutionError { failure, telemetry })?;
    Ok(AuditedOpFeeMeasurements {
        costs,
        payer_cost,
        telemetry,
    })
}
