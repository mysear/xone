//! Private stdin/stdout worker protocol for the existing dex-router executable.
//! This worker never listens on a socket and never submits a transaction.
use crate::evm_fixture::{
    self, FixtureState, SimulationBudget, SimulationError, SimulationOutcome, TransactionContext,
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
use xone_execution_plan::{BoundEvmExecution, ExecutorIdentity};

pub const WORKER_PROTOCOL: &str = "xone-local-evm-worker/1";
pub const MAX_WORKER_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_WORKER_OUTPUT_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSimulationBundle {
    pub schema_version: u32,
    pub fixture: FixtureState,
    pub executor: ExecutorIdentity,
    pub transaction_context: TransactionContext,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerRequest {
    pub protocol: String,
    pub bound: BoundEvmExecution,
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
pub struct WorkerResponse {
    pub protocol: String,
    pub worker_binary_sha256: String,
    pub outcome: Option<SimulationOutcome>,
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
    let result = evm_fixture::simulate_bound(
        &request.bound,
        &request.pools,
        &request.allowed_targets,
        &request.fixture,
        &request.transaction_context,
        &request.budget,
        control,
    );
    let (outcome, error) = match result {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    let response = WorkerResponse {
        protocol: WORKER_PROTOCOL.into(),
        worker_binary_sha256: identity,
        outcome,
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
