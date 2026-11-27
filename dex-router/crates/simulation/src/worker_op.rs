//! Parent-held signed OP commitment. Fee consistency is additional to the
//! identity-pinned worker; this module never executes or proves VM transitions.
use crate::{evm_fixture::*, revm_backend as core, worker::*};
use revm::primitives::{Address, BlockEnv, TxKind, B256, U256};
use serde_json::{json, Value};
use std::sync::Arc;
use xone_domain::{err, hash, Result};

pub struct OpReceiptCommitment {
    request: core::OpExecutionRequest,
    receipt_input: Value,
    budget: SimulationBudget,
}
pub struct OpReceiptAudit {
    pub transaction_hash: String,
    pub fees: core::AuditedOpFeeMeasurements,
    pub total_usage: BudgetUsage,
}

impl OpReceiptCommitment {
    pub fn from_request(request: &WorkerRequest) -> Result<Self> {
        let bad = || err("SIMULATION_WORKER_OP_BINDING");
        if request.protocol != WORKER_PROTOCOL
            || request.prepared.is_some()
            || request.expected_genesis_hash != request.fixture.state_anchor.genesis_hash
        {
            return Err(bad());
        }
        let (bound, snapshot, tx) = match (&request.bound, &request.exact_out_bound) {
            (Some(b), None) => {
                xone_execution_plan::validate_bound_execution(
                    b,
                    &request.pools,
                    &request.allowed_targets,
                )?;
                (serde_json::to_value(b), &b.plan.snapshot, &b.transaction)
            }
            (None, Some(b)) => {
                xone_execution_plan::validate_bound_exact_out_execution(
                    b,
                    &request.pools,
                    &request.allowed_targets,
                )?;
                (serde_json::to_value(b), &b.plan.snapshot, &b.transaction)
            }
            _ => return Err(bad()),
        };
        let op = request
            .op_user
            .as_ref()
            .ok_or_else(bad)?
            .authenticated_context(&request.fixture, &tx.from, &request.budget)?;
        let tx_context = op_context(&op).map_err(|_| bad())?;
        if hash(&tx_context)? != hash(&request.transaction_context)? {
            return Err(bad());
        }
        request
            .fixture
            .validate_snapshot_environment(snapshot, op.fork.label())
            .map_err(|_| bad())?;
        let e = op.transaction.environment();
        if tx.chain_key != format!("evm:{}", tx_context.chain_id)
            || e.caller != tx.from.parse::<Address>().map_err(|_| bad())?
            || e.transact_to != TxKind::Call(tx.to.parse().map_err(|_| bad())?)
            || !e.value.is_zero()
            || tx.value != xone_domain::Amount::zero()
            || e.data.as_ref()
                != hex::decode(tx.data.strip_prefix("0x").ok_or_else(bad)?).map_err(|_| bad())?
        {
            return Err(bad());
        }
        let env = &request.fixture.execution_environment;
        let block = BlockEnv {
            number: U256::from(env.block_number),
            coinbase: env.coinbase.parse().map_err(|_| bad())?,
            timestamp: U256::from(env.timestamp),
            gas_limit: U256::from(env.gas_limit),
            basefee: U256::from_be_slice(&env.base_fee.to_be_bytes()?),
            prevrandao: Some(env.prevrandao.parse::<B256>().map_err(|_| bad())?),
            ..Default::default()
        };
        Ok(Self {
            request: core::OpExecutionRequest {
                fork: op.fork,
                block,
                transaction: op.transaction,
                attributes_calldata: op.attributes_calldata,
            },
            receipt_input: json!({"bound":bound.map_err(|_| bad())?,
                "source_fixture_hash":hash(&request.fixture)?,"transaction_context":tx_context,
                "limits":request.budget,"simulator_version":"provided-local-state/op-signed-v1"}),
            budget: request.budget.clone(),
        })
    }

    pub fn audit(
        &self,
        image: Arc<core::StateImage>,
        outcome: &SimulationOutcome,
        control: core::ExecutionControl,
    ) -> Result<OpReceiptAudit> {
        let bad = || err("SIMULATION_WORKER_OP_RECEIPT");
        if outcome.source_fixture_hash != self.receipt_input["source_fixture_hash"]
            || outcome.simulator_version != "provided-local-state/op-signed-v1"
            || outcome.plan_validation != "BOUND_REVALIDATED"
            || outcome.native_fees.is_some()
            || outcome.execution.sender_nonce_before
                != self.request.transaction.environment().nonce.unwrap()
            || Some(outcome.execution.sender_nonce_after)
                != outcome.execution.sender_nonce_before.checked_add(1)
        {
            return Err(bad());
        }
        let mut value = serde_json::to_value(outcome).map_err(|_| bad())?;
        value
            .as_object_mut()
            .ok_or_else(bad)?
            .remove("receipt_hash");
        let mut input = self.receipt_input.clone();
        input["outcome"] = value;
        if hash(&input)? != outcome.receipt_hash {
            return Err(bad());
        }
        let usage = &outcome.usage;
        if usage.instructions > self.budget.max_instructions
            || usage.input_bytes > self.budget.max_total_input_bytes
            || usage.gas_used > self.budget.max_total_gas
            || usage.gas_used < outcome.execution.gas_used
            || usage.executions == 0
        {
            return Err(bad());
        }
        let fees = core::audit_op_fee_measurements(
            image,
            &self.request,
            outcome.execution.gas_used,
            outcome.op_fees.as_ref().ok_or_else(bad)?,
            core::ExecutionLimits {
                reads: core::BoundReadLimits {
                    max_reads: self
                        .budget
                        .max_reads
                        .checked_sub(usage.reads)
                        .ok_or_else(bad)?,
                    max_bytes: self
                        .budget
                        .max_read_bytes
                        .checked_sub(usage.read_bytes)
                        .ok_or_else(bad)?,
                },
                max_gas_limit: self.budget.max_transaction_gas,
                max_instructions: self.budget.max_instructions,
                max_input_bytes: self.budget.max_input_bytes,
                max_memory_bytes: self.budget.max_memory_bytes,
            },
            control,
        )
        .map_err(|_| bad())?;
        let mut total_usage = usage.clone();
        total_usage.reads = total_usage
            .reads
            .checked_add(fees.telemetry.reads.reads)
            .ok_or_else(bad)?;
        total_usage.read_bytes = total_usage
            .read_bytes
            .checked_add(fees.telemetry.reads.bytes)
            .ok_or_else(bad)?;
        Ok(OpReceiptAudit {
            transaction_hash: format!("{:#x}", self.request.transaction.transaction_hash()),
            fees,
            total_usage,
        })
    }
}
