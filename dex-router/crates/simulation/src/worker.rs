//! Private stdin/stdout worker protocol for the existing dex-router executable.
//! This worker never listens on a socket and never submits a transaction.
use crate::evm_fixture::{
    self, FixtureState, SimulationBudget, SimulationError, SimulationOutcome, TransactionContext,
};
use crate::prepared_evm::{
    simulate_prepared, simulate_prepared_exact_out, PreparedExactOutExecution, PreparedExecution,
    PreparedSimulation,
};
use crate::revm_backend::ExecutionControl;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};
use xone_domain::{err, Pool, Result};
use xone_execution_plan::{BoundEvmExactOutExecution, BoundEvmExecution, ExecutorIdentity};
use xone_execution_preparation::evm::{
    ExactOutPreparation, FeeContext, FundingObservation, Preparation,
};

pub const WORKER_PROTOCOL: &str = "xone-local-evm-worker/1";
pub const MAX_WORKER_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_WORKER_OUTPUT_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSimulationBundle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_executor: Option<xone_execution_plan::ordered_v2::SplitExecutor>,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_attributes_calldata: Option<String>,
    pub fixture: FixtureState,
    pub executor: ExecutorIdentity,
    pub transaction_context: TransactionContext,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerPreparation {
    pub funding: FundingObservation,
    pub fees: FeeContext,
    pub component: serde_json::Value,
}
impl WorkerPreparation {
    pub fn transaction_context(&self) -> TransactionContext {
        TransactionContext {
            nonce: self.fees.nonce,
            gas_limit: self.fees.gas_limit,
            gas_price: self.fees.max_fee_per_gas.clone(),
            max_priority_fee_per_gas: Some(self.fees.max_priority_fee_per_gas.clone()),
            chain_id: self.fees.chain_id,
        }
    }
}
/// Full signed envelope and source attributes, never independent VM overrides.
/// This optional extension is absent on the existing Shanghai worker wire.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerOpUser {
    pub transaction_hash: String,
    pub signed_wire: String,
    pub attributes_calldata: String,
}
impl WorkerOpUser {
    pub fn authenticated_context(
        &self,
        fixture: &FixtureState,
        caller: &str,
        budget: &SimulationBudget,
    ) -> Result<evm_fixture::OpTransactionContext> {
        let invalid = || err("SIMULATION_WORKER_OP_BINDING");
        let decode = |value: &str, maximum: usize| -> Result<Vec<u8>> {
            let raw = value.strip_prefix("0x").ok_or_else(invalid)?;
            if raw.len() % 2 != 0
                || raw.len() / 2 > maximum
                || !raw
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid());
            }
            hex::decode(raw).map_err(|_| invalid())
        };
        let wire = decode(
            &self.signed_wire,
            budget.max_input_bytes.min(16 * 1024 * 1024),
        )?;
        let attributes = decode(&self.attributes_calldata, 260)?;
        let hash = decode(&self.transaction_hash, 32)?;
        if hash.len() != 32 {
            return Err(invalid());
        }
        let expected_caller = caller.parse().map_err(|_| invalid())?;
        let transaction = crate::op_transaction::authenticate_user(
            fixture.execution_environment.chain_id,
            revm::primitives::B256::from_slice(&hash),
            expected_caller,
            &wire,
            budget.max_input_bytes,
        )
        .map_err(|_| invalid())?;
        let fork = historical_op_fork(&fixture.execution_environment)?;
        Ok(evm_fixture::OpTransactionContext {
            fork,
            transaction,
            attributes_calldata: attributes,
        })
    }
}
/// Validate the complete configured source attributes before offering a signing template.
pub fn historical_op_attributes(
    env: &evm_fixture::ExecutionEnvironment,
    encoded: &str,
) -> Result<Vec<u8>> {
    let bad = || err("SIMULATION_WORKER_OP_BINDING");
    let fork = historical_op_fork(env)?;
    let raw = encoded.strip_prefix("0x").ok_or_else(bad)?;
    if raw.len() > 520
        || raw.len() % 2 != 0
        || !raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(bad());
    }
    let bytes = hex::decode(raw).map_err(|_| bad())?;
    use crate::revm_backend::OpFork;
    if matches!(fork, OpFork::Regolith | OpFork::Canyon) {
        xone_execution_preparation::op_l1_cost::decode_parameters(
            &format!("evm:{}", env.chain_id),
            env.timestamp,
            &bytes,
        )?;
    } else {
        let (selector, size) = match fork {
            OpFork::Jovian | OpFork::Karst => ([0x3d, 0xb6, 0xbe, 0x2b], 178),
            OpFork::Isthmus => ([0x09, 0x89, 0x99, 0xbe], 176),
            _ => ([0x44, 0x0a, 0x5e, 0x20], 164),
        };
        if bytes.len() != size || bytes[..4] != selector {
            return Err(bad());
        }
        xone_execution_preparation::op_packed_cost::decode_packed_attributes(&bytes)?;
        if u64::from_be_bytes(bytes[20..28].try_into().unwrap()) > env.timestamp
            || (bytes[68..100].iter().all(|b| *b == 0) && bytes[4..12].iter().all(|b| *b == 0))
        {
            return Err(bad());
        }
    }
    Ok(bytes)
}

/// Exact pinned historical OP/Base interval, never an inferred current VM.
pub fn historical_op_fork(
    env: &evm_fixture::ExecutionEnvironment,
) -> Result<crate::revm_backend::OpFork> {
    let fork = crate::revm_backend::OpFork::at(env.chain_id, env.timestamp)
        .ok_or_else(|| err("SIMULATION_WORKER_OP_BINDING"))?;
    if fork.label() != env.fork {
        return Err(err("SIMULATION_WORKER_OP_BINDING"));
    }
    Ok(fork)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerRequest {
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<WorkerSplit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound: Option<BoundEvmExecution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_out_bound: Option<BoundEvmExactOutExecution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared: Option<WorkerPreparation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_user: Option<WorkerOpUser>,
    pub pools: Vec<Pool>,
    pub allowed_targets: Vec<String>,
    pub fixture: FixtureState,
    pub transaction_context: TransactionContext,
    pub budget: SimulationBudget,
    pub expected_genesis_hash: String,
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSplit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_sequence: Option<evm_fixture::ApprovalSequence>,
    pub bound: xone_execution_plan::ordered_v2::BoundSplit,
    pub expected_executor: xone_execution_plan::ordered_v2::SplitExecutor,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerResponse {
    pub protocol: String,
    pub worker_binary_sha256: String,
    pub outcome: Option<SimulationOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared: Option<PreparedSimulation>,
    pub error: Option<SimulationError>,
}
pub fn executable_sha256(path: &std::path::Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).map_err(|_| err("SIMULATION_WORKER_IDENTITY_UNAVAILABLE"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let size = file
            .read(&mut buffer)
            .map_err(|_| err("SIMULATION_WORKER_IDENTITY_UNAVAILABLE"))?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(hex::encode(hash.finalize()))
}
pub fn run_stdio() -> Result<()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_WORKER_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|_| err("SIMULATION_WORKER_INPUT"))?;
    if input.len() > MAX_WORKER_INPUT_BYTES {
        return Err(err("SIMULATION_WORKER_INPUT_LIMIT"));
    }
    let request: WorkerRequest =
        serde_json::from_slice(&input).map_err(|_| err("SIMULATION_WORKER_INPUT"))?;
    if request.protocol != WORKER_PROTOCOL
        || !(1..=60000).contains(&request.timeout_ms)
        || request.fixture.state_anchor.genesis_hash != request.expected_genesis_hash
    {
        return Err(err("SIMULATION_WORKER_BINDING"));
    }
    let identity = executable_sha256(
        &std::env::current_exe().map_err(|_| err("SIMULATION_WORKER_IDENTITY_UNAVAILABLE"))?,
    )?;
    let control = ExecutionControl {
        cancellation: Arc::default(),
        deadline: Some(Instant::now() + Duration::from_millis(request.timeout_ms)),
        progress: None,
    };
    enum Completed {
        Plain(SimulationOutcome),
        Prepared(PreparedSimulation),
    }
    let op_context = if let Some(op) = &request.op_user {
        if request.prepared.is_some() {
            return Err(err("SIMULATION_WORKER_OP_BINDING"));
        }
        let caller = match (&request.bound, &request.exact_out_bound) {
            (Some(bound), None) => &bound.transaction.from,
            (None, Some(bound)) => &bound.transaction.from,
            _ => return Err(err("SIMULATION_WORKER_OP_BINDING")),
        };
        let authenticated = op.authenticated_context(&request.fixture, caller, &request.budget)?;
        if xone_domain::hash(&request.transaction_context)?
            != xone_domain::hash(
                &evm_fixture::op_context(&authenticated)
                    .map_err(|_| err("SIMULATION_WORKER_OP_BINDING"))?,
            )?
        {
            return Err(err("SIMULATION_WORKER_OP_BINDING"));
        }
        Some(authenticated)
    } else {
        None
    };
    let result = if let Some(split) = &request.split {
        if request.bound.is_some()
            || request.exact_out_bound.is_some()
            || request.prepared.is_some()
            || op_context.is_some()
        {
            return Err(err("SIMULATION_WORKER_BINDING"));
        }
        evm_fixture::simulate_bound_split(
            &split.bound,
            &split.expected_executor,
            split.approval_sequence.as_ref(),
            &request.pools,
            &request.allowed_targets,
            &request.fixture,
            &request.transaction_context,
            &request.budget,
            control,
        )
        .map(Completed::Plain)
    } else {
        match (
            &request.bound,
            &request.exact_out_bound,
            &request.prepared,
            &op_context,
        ) {
            (Some(bound), None, None, Some(op)) => evm_fixture::simulate_bound_op_user(
                bound,
                &request.pools,
                &request.allowed_targets,
                &request.fixture,
                op,
                &request.budget,
                control,
            )
            .map(Completed::Plain),
            (None, Some(bound), None, Some(op)) => evm_fixture::simulate_bound_op_user_exact_out(
                bound,
                &request.pools,
                &request.allowed_targets,
                &request.fixture,
                op,
                &request.budget,
                control,
            )
            .map(Completed::Plain),
            (Some(bound), None, None, None) => evm_fixture::simulate_bound(
                bound,
                &request.pools,
                &request.allowed_targets,
                &request.fixture,
                &request.transaction_context,
                &request.budget,
                control,
            )
            .map(Completed::Plain),
            (None, Some(bound), None, None) => evm_fixture::simulate_bound_exact_out(
                bound,
                &request.pools,
                &request.allowed_targets,
                &request.fixture,
                &request.transaction_context,
                &request.budget,
                control,
            )
            .map(Completed::Plain),
            (Some(bound), None, Some(prepared), None) => {
                if xone_domain::hash(&request.transaction_context)?
                    != xone_domain::hash(&prepared.transaction_context())?
                {
                    return Err(err("SIMULATION_WORKER_BINDING"));
                }
                let component: Preparation = serde_json::from_value(prepared.component.clone())
                    .map_err(|_| err("SIMULATION_WORKER_BINDING"))?;
                simulate_prepared(
                    PreparedExecution {
                        bound,
                        pools: &request.pools,
                        allowed_targets: &request.allowed_targets,
                        funding: &prepared.funding,
                        fees: &prepared.fees,
                        preparation: &component,
                    },
                    &request.fixture,
                    &request.budget,
                    control,
                )
                .map(Completed::Prepared)
            }
            (None, Some(bound), Some(prepared), None) => {
                if xone_domain::hash(&request.transaction_context)?
                    != xone_domain::hash(&prepared.transaction_context())?
                {
                    return Err(err("SIMULATION_WORKER_BINDING"));
                }
                let component: ExactOutPreparation =
                    serde_json::from_value(prepared.component.clone())
                        .map_err(|_| err("SIMULATION_WORKER_BINDING"))?;
                simulate_prepared_exact_out(
                    PreparedExactOutExecution {
                        bound,
                        pools: &request.pools,
                        allowed_targets: &request.allowed_targets,
                        funding: &prepared.funding,
                        fees: &prepared.fees,
                        preparation: &component,
                    },
                    &request.fixture,
                    &request.budget,
                    control,
                )
                .map(Completed::Prepared)
            }
            _ => return Err(err("SIMULATION_WORKER_BINDING")),
        }
    };
    let (outcome, prepared, error) = match result {
        Ok(Completed::Plain(value)) => (Some(value), None, None),
        Ok(Completed::Prepared(value)) => (None, Some(value), None),
        Err(error) => (None, None, Some(error)),
    };
    let response = WorkerResponse {
        protocol: WORKER_PROTOCOL.into(),
        worker_binary_sha256: identity,
        outcome,
        prepared,
        error,
    };
    let output = serde_json::to_vec(&response).map_err(|_| err("SIMULATION_WORKER_OUTPUT"))?;
    if output.len() > MAX_WORKER_OUTPUT_BYTES {
        return Err(err("SIMULATION_WORKER_OUTPUT_LIMIT"));
    }
    std::io::stdout()
        .write_all(&output)
        .map_err(|_| err("SIMULATION_WORKER_OUTPUT"))?;
    Ok(())
}
