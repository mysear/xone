//! Bind a prepared EIP-1559 component to execution against an explicit H/H+1 bank.
use crate::{
    evm_fixture::{
        self, FixtureState, SimulationBudget, SimulationError, SimulationErrorCode,
        SimulationOutcome, TransactionContext,
    },
    revm_backend::ExecutionControl,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Write, sync::atomic::Ordering, time::Instant};
use xone_domain::{hash, Pool};
use xone_execution_plan::{BoundEvmExactOutExecution, BoundEvmExecution};
use xone_execution_preparation::evm::{
    self, ExactOutPreparation, FeeContext, FundingObservation, Preparation,
};

pub struct PreparedExecution<'a> {
    pub bound: &'a BoundEvmExecution,
    pub pools: &'a [Pool],
    pub allowed_targets: &'a [String],
    pub funding: &'a FundingObservation,
    pub fees: &'a FeeContext,
    pub preparation: &'a Preparation,
}
pub struct PreparedExactOutExecution<'a> {
    pub bound: &'a BoundEvmExactOutExecution,
    pub pools: &'a [Pool],
    pub allowed_targets: &'a [String],
    pub funding: &'a FundingObservation,
    pub fees: &'a FeeContext,
    pub preparation: &'a ExactOutPreparation,
}

enum PreparedSource<'a> {
    ExactIn(PreparedExecution<'a>),
    ExactOut(PreparedExactOutExecution<'a>),
}
struct CommonInput<'a> {
    pools: &'a [Pool],
    allowed_targets: &'a [String],
    funding: &'a FundingObservation,
    fees: &'a FeeContext,
    preparation: &'a Preparation,
}
impl PreparedSource<'_> {
    fn common(&self) -> CommonInput<'_> {
        match self {
            Self::ExactIn(i) => CommonInput {
                pools: i.pools,
                allowed_targets: i.allowed_targets,
                funding: i.funding,
                fees: i.fees,
                preparation: i.preparation,
            },
            Self::ExactOut(i) => CommonInput {
                pools: i.pools,
                allowed_targets: i.allowed_targets,
                funding: i.funding,
                fees: i.fees,
                preparation: &i.preparation.preparation,
            },
        }
    }
    fn data(&self) -> &str {
        match self {
            Self::ExactIn(i) => &i.bound.transaction.data,
            Self::ExactOut(i) => &i.bound.transaction.data,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedSimulation {
    pub preparation_hash: String,
    pub unsigned_transaction_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_out_preparation_hash: Option<String>,
    pub simulation: SimulationOutcome,
    pub commitment_hash: String,
}
fn failure(code: SimulationErrorCode, detail: &str) -> SimulationError {
    SimulationError {
        code,
        phase: "prepared_transaction".into(),
        detail: detail.into(),
        usage: Default::default(),
    }
}
fn check_control(control: &ExecutionControl) -> evm_fixture::Result<()> {
    if control.cancellation.load(Ordering::Relaxed) {
        return Err(failure(
            SimulationErrorCode::SimulationCancelled,
            "cancelled",
        ));
    }
    if control
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        return Err(failure(SimulationErrorCode::SimulationDeadline, "deadline"));
    }
    Ok(())
}
struct CountBytes {
    used: usize,
    limit: usize,
}
impl Write for CountBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.used = self
            .used
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| std::io::Error::other("preparation input limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// ExactIn only, with explicit provided-state authority. Successful replay does
/// not authorize VERIFIED, complete chain fees, signing or production execution.
pub fn simulate_prepared(
    input: PreparedExecution<'_>,
    fixture: &FixtureState,
    budget: &SimulationBudget,
    control: ExecutionControl,
) -> evm_fixture::Result<PreparedSimulation> {
    simulate_source(PreparedSource::ExactIn(input), fixture, budget, control)
}

/// Execute the exact output-specified component, including its outer target and budget commitment.
pub fn simulate_prepared_exact_out(
    input: PreparedExactOutExecution<'_>,
    fixture: &FixtureState,
    budget: &SimulationBudget,
    control: ExecutionControl,
) -> evm_fixture::Result<PreparedSimulation> {
    simulate_source(PreparedSource::ExactOut(input), fixture, budget, control)
}
fn simulate_source(
    source: PreparedSource<'_>,
    fixture: &FixtureState,
    budget: &SimulationBudget,
    control: ExecutionControl,
) -> evm_fixture::Result<PreparedSimulation> {
    let input = source.common();
    check_control(&control)?;
    let mismatch = || {
        failure(
            SimulationErrorCode::SimulationBindingMismatch,
            "prepared component differs from bound transaction or fee context",
        )
    };
    let input_bytes = source.data().len().saturating_sub(2) / 2;
    if input_bytes > budget.max_input_bytes || input_bytes as u64 > budget.max_total_input_bytes {
        return Err(failure(
            SimulationErrorCode::SimulationBudgetExceeded,
            "transaction input cap",
        ));
    }
    // Bound serialization before hashing/cloning caller-controlled wire and JSON.
    let mut counter = CountBytes {
        used: 0,
        limit: budget.max_fixture_json_bytes,
    };
    match &source {
        PreparedSource::ExactIn(i) => serde_json::to_writer(&mut counter, i.preparation),
        PreparedSource::ExactOut(i) => serde_json::to_writer(&mut counter, i.preparation),
    }
    .map_err(|_| {
        failure(
            SimulationErrorCode::SimulationFixtureLimit,
            "preparation input cap",
        )
    })?;
    let now = fixture
        .execution_environment
        .timestamp
        .checked_mul(1000)
        .ok_or_else(mismatch)?;
    let observed_base_fee = fixture
        .state_anchor
        .base_fee_per_gas
        .as_ref()
        .unwrap_or(&fixture.execution_environment.base_fee);
    if &input.funding.base_fee_per_gas != observed_base_fee {
        return Err(mismatch());
    }
    let (expected, exact_out_preparation_hash) = match &source {
        PreparedSource::ExactIn(i) => (
            evm::prepare(
                i.bound,
                input.pools,
                input.allowed_targets,
                input.funding,
                input.fees,
                now,
            )
            .map_err(|_| mismatch())?,
            None,
        ),
        PreparedSource::ExactOut(i) => {
            let expected = evm::prepare_exact_out(
                i.bound,
                input.pools,
                input.allowed_targets,
                input.funding,
                input.fees,
                now,
            )
            .map_err(|_| mismatch())?;
            if hash(&expected).map_err(|_| mismatch())?
                != hash(i.preparation).map_err(|_| mismatch())?
            {
                return Err(mismatch());
            }
            (
                expected.preparation,
                Some(expected.exact_out_preparation_hash),
            )
        }
    };
    if !expected.ready
        || hash(&expected).map_err(|_| mismatch())?
            != hash(input.preparation).map_err(|_| mismatch())?
    {
        return Err(mismatch());
    }
    let tx = expected.transaction.as_ref().ok_or_else(mismatch)?;
    let context = TransactionContext {
        nonce: tx.nonce,
        gas_limit: tx.gas_limit,
        gas_price: tx.max_fee_per_gas.clone(),
        max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas.clone()),
        chain_id: tx.chain_id,
    };
    check_control(&control)?;
    let simulation = match &source {
        PreparedSource::ExactIn(i) => evm_fixture::simulate_bound_with_funding(
            i.bound,
            input.pools,
            input.allowed_targets,
            fixture,
            &context,
            budget,
            control.clone(),
            Some(input.funding),
        ),
        PreparedSource::ExactOut(i) => evm_fixture::simulate_bound_exact_out_with_funding(
            i.bound,
            input.pools,
            input.allowed_targets,
            fixture,
            &context,
            budget,
            control.clone(),
            Some(input.funding),
        ),
    }?;
    let native = simulation.native_fees.as_ref().ok_or_else(mismatch)?;
    if native.caller_before != input.funding.native_balance
        || simulation.balances.caller_sell_before != input.funding.token_balance
    {
        let mut error = mismatch();
        error.usage = simulation.usage.clone();
        return Err(error);
    }
    let unsigned = expected
        .unsigned_transaction
        .as_ref()
        .ok_or_else(mismatch)?;
    let mut result = PreparedSimulation {
        preparation_hash: expected.preparation_hash,
        exact_out_preparation_hash,
        unsigned_transaction_hash: hex::encode(Sha256::digest(
            hex::decode(&unsigned[2..]).map_err(|_| mismatch())?,
        )),
        simulation,
        commitment_hash: String::new(),
    };
    let mut value = serde_json::to_value(&result).map_err(|_| mismatch())?;
    value
        .as_object_mut()
        .ok_or_else(mismatch)?
        .remove("commitment_hash");
    result.commitment_hash = hash(&value).map_err(|_| mismatch())?;
    check_control(&control)?;
    Ok(result)
}
