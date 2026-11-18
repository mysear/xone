//! Configured local-state diagnostics, isolated in the current executable's worker mode.
use crate::exact_out::ExactOutCached;
use crate::{Cached, CancelOnDrop};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};
use xone_config::ConfigSnapshot;
use xone_domain::{err, hash, Amount, Result};
use xone_execution_plan::{ExecutorIdentity, Transaction};
use xone_simulation::{
    evm_fixture::{NativeFeeMeasurements, SimulationBudget, TransactionContext},
    revm_backend::StateImage,
    worker::{
        executable_sha256, LocalSimulationBundle, WorkerPreparation, WorkerRequest, WorkerResponse,
        MAX_WORKER_INPUT_BYTES, MAX_WORKER_OUTPUT_BYTES, WORKER_PROTOCOL,
    },
};

const MAX_STDERR: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) struct Failure {
    pub code: String,
    pub phase: String,
    pub usage: Option<Value>,
}
impl Failure {
    pub(crate) fn new(code: &str, phase: &str) -> Self {
        Self {
            code: code.into(),
            phase: phase.into(),
            usage: None,
        }
    }
}

#[derive(Clone)]
pub(crate) enum Selection {
    ExactIn(Cached),
    ExactOut(ExactOutCached),
}
impl From<Cached> for Selection {
    fn from(c: Cached) -> Self {
        Self::ExactIn(c)
    }
}
impl From<ExactOutCached> for Selection {
    fn from(c: ExactOutCached) -> Self {
        Self::ExactOut(c)
    }
}
impl Selection {
    pub(crate) fn view(&self) -> &Arc<crate::QuoteView> {
        match self {
            Self::ExactIn(c) => &c.view,
            Self::ExactOut(c) => &c.view,
        }
    }
    pub(crate) fn expires(&self) -> u64 {
        match self {
            Self::ExactIn(c) => c.expires,
            Self::ExactOut(c) => c.expires,
        }
    }
    fn snapshot(&self) -> &xone_domain::SnapshotRef {
        match self {
            Self::ExactIn(c) => &c.plan.snapshot,
            Self::ExactOut(c) => &c.plan.snapshot,
        }
    }
    pub(crate) fn prepared(&self) -> Option<&crate::evm_preparation::Prepared> {
        match self {
            Self::ExactIn(c) => c.evm_preparation.as_ref(),
            Self::ExactOut(c) => c.evm_preparation.as_ref(),
        }
    }
    pub(crate) fn context(&self) -> &xone_execution_plan::Context {
        match self {
            Self::ExactIn(c) => &c.plan.context,
            Self::ExactOut(c) => &c.plan.context,
        }
    }
}
pub(crate) struct LocalSimulation {
    bundle: Arc<LocalSimulationBundle>,
    // Retained immutable validation image; each child creates its own overlay.
    _image: Arc<StateImage>,
    pub bundle_sha256: String,
    pub worker_sha256: String,
    worker_path: PathBuf,
    budget: SimulationBudget,
    pub permits: Arc<Semaphore>,
    #[cfg(test)]
    test_launch: Option<TestLaunch>,
}

// These small values are taken from the exact request serialized to child stdin.
// Keeping them together also makes the reported fee context unambiguous.
struct ExecutionCommitment {
    binding_hash: String,
    quote_kind: &'static str,
    expected_preparation: Option<(String, Option<String>)>,
    transaction: Transaction,
    transaction_context: TransactionContext,
    execution_base_fee: Amount,
    execution_coinbase: String,
    caller_native_before: Option<Amount>,
    coinbase_native_before: Option<Amount>,
    executor: ExecutorIdentity,
}
struct PreparedExecution {
    input: Vec<u8>,
    commitment: ExecutionCommitment,
}
impl PreparedExecution {
    fn from_request(request: WorkerRequest) -> std::result::Result<Self, Failure> {
        let mut output = LimitedWriter(Vec::new());
        serde_json::to_writer(&mut output, &request)
            .map_err(|_| Failure::new("SIMULATION_WORKER_INPUT_LIMIT", "serialize"))?;
        let expected_preparation = if let Some(p) = &request.prepared {
            let component = if request.exact_out_bound.is_some() {
                &p.component["preparation"]
            } else {
                &p.component
            };
            let inner = component["preparation_hash"]
                .as_str()
                .ok_or_else(|| Failure::new("SIMULATION_WORKER_BINDING", "prepare"))?
                .to_owned();
            let outer = if request.exact_out_bound.is_some() {
                Some(
                    p.component["exact_out_preparation_hash"]
                        .as_str()
                        .ok_or_else(|| Failure::new("SIMULATION_WORKER_BINDING", "prepare"))?
                        .to_owned(),
                )
            } else {
                None
            };
            Some((inner, outer))
        } else {
            None
        };
        let (binding_hash, transaction, executor, quote_kind) =
            match (request.bound, request.exact_out_bound) {
                (Some(b), None) => (b.binding_hash, b.transaction, b.executor, "EXACT_IN"),
                (None, Some(b)) => (b.binding_hash, b.transaction, b.executor, "EXACT_OUT"),
                _ => return Err(Failure::new("SIMULATION_WORKER_BINDING", "prepare")),
            };
        let balance = |owner: &str| {
            request
                .fixture
                .accounts
                .iter()
                .find(|a| a.address.eq_ignore_ascii_case(owner))
                .map(|a| {
                    if a.exists {
                        a.balance.clone()
                    } else {
                        Amount::zero()
                    }
                })
        };
        let caller_native_before = balance(&transaction.from);
        let coinbase_native_before = balance(&request.fixture.execution_environment.coinbase);
        Ok(Self {
            input: output.0,
            commitment: ExecutionCommitment {
                binding_hash,
                transaction,
                executor,
                quote_kind,
                expected_preparation,
                transaction_context: request.transaction_context,
                execution_base_fee: request.fixture.execution_environment.base_fee,
                execution_coinbase: request.fixture.execution_environment.coinbase,
                caller_native_before,
                coinbase_native_before,
            },
        })
    }
}

impl LocalSimulation {
    pub(crate) fn matches_snapshot(
        &self,
        reference: &xone_domain::SnapshotRef,
        genesis: &str,
    ) -> bool {
        let source = &self.bundle.fixture.state_anchor;
        let block = &reference.block;
        source.genesis_hash == genesis
            && source.chain_key == block.chain_key
            && source.block_number == block.number_or_slot
            && source.block_hash == block.hash
            && source.parent_hash == block.parent_hash
            && source.timestamp.checked_mul(1000) == Some(block.timestamp_ms)
    }
    pub fn load(config: &ConfigSnapshot) -> Result<Option<Arc<Self>>> {
        let sim = &config.config.simulation;
        if sim.backend != "revm-fixture" {
            return Ok(None);
        }
        let configured = PathBuf::from(&sim.state_file);
        let path = if configured.is_absolute() {
            configured
        } else {
            config.root.join(configured)
        };
        let admitted = std::fs::metadata(&path).map_err(|_| err("SIMULATION_STATE_UNREADABLE"))?;
        if !admitted.is_file() || admitted.len() > sim.max_state_bytes {
            return Err(err("SIMULATION_STATE_LIMIT"));
        }
        let file = std::fs::File::open(&path).map_err(|_| err("SIMULATION_STATE_UNREADABLE"))?;
        let meta = file
            .metadata()
            .map_err(|_| err("SIMULATION_STATE_UNREADABLE"))?;
        if !meta.is_file() || meta.len() > sim.max_state_bytes {
            return Err(err("SIMULATION_STATE_LIMIT"));
        }
        let mut bytes = Vec::new();
        file.take(sim.max_state_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("SIMULATION_STATE_UNREADABLE"))?;
        if bytes.len() as u64 > sim.max_state_bytes {
            return Err(err("SIMULATION_STATE_LIMIT"));
        }
        let bundle: LocalSimulationBundle =
            serde_json::from_slice(&bytes).map_err(|_| err("SIMULATION_STATE_INVALID"))?;
        if bundle.schema_version != 1 {
            return Err(err("SIMULATION_STATE_SCHEMA"));
        }
        let anchor = &bundle.fixture.state_anchor;
        if !config
            .config
            .chains
            .iter()
            .any(|c| c.key == anchor.chain_key && c.genesis_hash == anchor.genesis_hash)
        {
            return Err(err("SIMULATION_STATE_CHAIN_MISMATCH"));
        }
        if !sim.allowed_targets.contains(&bundle.executor.target) {
            return Err(err("EXECUTION_TARGET_NOT_ALLOWED"));
        }
        let budget = SimulationBudget {
            max_fixture_json_bytes: sim.max_state_bytes as usize,
            max_accounts: 2048,
            max_storage_slots: 65536,
            max_code_bytes: sim.max_state_bytes as usize,
            max_image_bytes: sim.max_state_bytes as usize,
            max_reads: sim.max_state_reads,
            max_read_bytes: sim.max_state_bytes * 2,
            max_instructions: sim.max_instructions,
            max_input_bytes: 128 * 1024,
            max_total_input_bytes: sim.max_state_bytes,
            max_transaction_gas: sim.max_gas,
            probe_gas_limit: 200_000,
            max_total_gas: sim.max_gas,
            max_memory_bytes: sim.max_memory_bytes,
            max_output_bytes: 64 * 1024,
        };
        let image = bundle
            .fixture
            .to_image(&budget)
            .map_err(|_| err("SIMULATION_STATE_INVALID"))?;
        // Domain SHA-256 commits the canonical parsed bundle, independent of whitespace.
        let bundle_sha256 = hash(&bundle)?;
        let worker_path =
            std::env::current_exe().map_err(|_| err("SIMULATION_WORKER_IDENTITY_UNAVAILABLE"))?;
        let worker_sha256 = executable_sha256(&worker_path)?;
        Ok(Some(Arc::new(Self {
            bundle: Arc::new(bundle),
            _image: image,
            bundle_sha256,
            worker_sha256,
            worker_path,
            budget,
            permits: Arc::new(Semaphore::new(sim.concurrency)),
            #[cfg(test)]
            test_launch: None,
        })))
    }

    pub async fn execute(
        self: Arc<Self>,
        config: Arc<ConfigSnapshot>,
        cached: impl Into<Selection>,
        deadline: Instant,
    ) -> std::result::Result<Value, Failure> {
        self.execute_inner(config, cached.into(), deadline, None)
            .await
    }
    pub async fn execute_prepared(
        self: Arc<Self>,
        config: Arc<ConfigSnapshot>,
        cached: Selection,
        deadline: Instant,
        prepared: WorkerPreparation,
    ) -> std::result::Result<Value, Failure> {
        self.execute_inner(config, cached, deadline, Some(prepared))
            .await
    }
    async fn execute_inner(
        self: Arc<Self>,
        config: Arc<ConfigSnapshot>,
        cached: Selection,
        deadline: Instant,
        prepared: Option<WorkerPreparation>,
    ) -> std::result::Result<Value, Failure> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| Failure::new("SIMULATION_BUSY", "admission"))?;
        let cancellation = Arc::new(AtomicBool::new(false));
        let _cancel_on_drop = CancelOnDrop(cancellation.clone());
        // This task owns the permit until all child cleanup is complete. Dropping the
        // HTTP handler only signals cancellation; it cannot detach an unowned child.
        let job_cancellation = cancellation.clone();
        let mut job = tokio::spawn(async move {
            let cancellation = job_cancellation;
            let _permit = permit;
            let state = self.clone();
            let timeout_ms = config.config.simulation.timeout_ms.min(60_000);
            let prepared = tokio::task::spawn_blocking(move || {
                let identity = executable_sha256(&state.worker_path)
                    .map_err(|_| Failure::new("SIMULATION_WORKER_IDENTITY", "identity"))?;
                if identity != state.worker_sha256 {
                    return Err(Failure::new("SIMULATION_WORKER_IDENTITY", "identity"));
                }
                let allowed = &config.config.simulation.allowed_targets;
                let (bound, exact_out_bound) = match &cached {
                    Selection::ExactIn(c) => (
                        Some(
                            xone_execution_plan::bind_v2_execution(
                                &c.plan,
                                &c.view.pools,
                                &state.bundle.executor,
                                allowed,
                            )
                            .map_err(|e| Failure::new(&e.0, "binding"))?,
                        ),
                        None,
                    ),
                    Selection::ExactOut(c) => (
                        None,
                        Some(
                            xone_execution_plan::bind_v2_exact_out_execution(
                                &c.plan,
                                &c.view.pools,
                                &state.bundle.executor,
                                allowed,
                            )
                            .map_err(|e| Failure::new(&e.0, "binding"))?,
                        ),
                    ),
                };
                let genesis = config
                    .config
                    .chains
                    .iter()
                    .find(|c| c.key == cached.context().chain_key)
                    .ok_or_else(|| Failure::new("SIMULATION_STATE_CHAIN_MISMATCH", "anchor"))?
                    .genesis_hash
                    .clone();
                let anchor = &state.bundle.fixture.state_anchor;
                let block = &cached.snapshot().block;
                if anchor.chain_key != block.chain_key
                    || anchor.genesis_hash != genesis
                    || anchor.block_number != block.number_or_slot
                    || anchor.block_hash != block.hash
                    || anchor.parent_hash != block.parent_hash
                    || anchor.timestamp.checked_mul(1000) != Some(block.timestamp_ms)
                {
                    return Err(Failure::new("SIMULATION_ANCHOR_MISMATCH", "anchor"));
                }
                let request = WorkerRequest {
                    protocol: WORKER_PROTOCOL.into(),
                    bound,
                    exact_out_bound,
                    pools: cached.view().pools.clone(),
                    allowed_targets: config.config.simulation.allowed_targets.clone(),
                    fixture: state.bundle.fixture.clone(),
                    transaction_context: prepared
                        .as_ref()
                        .map(WorkerPreparation::transaction_context)
                        .unwrap_or_else(|| state.bundle.transaction_context.clone()),
                    prepared,
                    budget: state.budget.clone(),
                    expected_genesis_hash: genesis,
                    timeout_ms,
                };
                PreparedExecution::from_request(request)
            });
            // The HTTP future has its own deadline; this owned job retains
            // admission while non-interruptible blocking preparation finishes.
            let PreparedExecution { input, commitment } = prepared
                .await
                .map_err(|_| Failure::new("SIMULATION_PREPARE_FAILED", "prepare"))??;
            if cancellation.load(Ordering::Acquire) {
                return Err(Failure::new("SIMULATION_CANCELLED", "spawn"));
            }
            if Instant::now() >= deadline {
                return Err(Failure::new("SIMULATION_TIMEOUT", "spawn"));
            }
            let mut command = Command::new(&self.worker_path);
            command.arg("--evm-simulation-worker");
            #[cfg(test)]
            if let Some(launch) = &self.test_launch {
                command = launch.command();
            }
            let output = run_process(command, input, deadline, cancellation.clone()).await?;
            let worker_hash = self.worker_sha256.clone();
            let parsed =
                tokio::task::spawn_blocking(move || decode_response(&output, &worker_hash));
            let outcome = parsed
                .await
                .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))??;
            if cancellation.load(Ordering::Acquire) {
                return Err(Failure::new("SIMULATION_CANCELLED", "response"));
            }
            if Instant::now() >= deadline {
                return Err(Failure::new("SIMULATION_TIMEOUT", "response"));
            }
            if outcome.prepared.as_ref().map(|p| {
                (
                    p.preparation_hash.clone(),
                    p.exact_out_preparation_hash.clone(),
                )
            }) != commitment.expected_preparation
            {
                return Err(Failure::new("SIMULATION_WORKER_PROTOCOL", "preparation"));
            }
            let mut response = diagnostic_response(
                outcome.outcome,
                &commitment,
                &self.bundle_sha256,
                &self.worker_sha256,
            )?;
            if let Some(prepared) = outcome.prepared {
                response["preparedExecution"] = serde_json::to_value(prepared)
                    .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "preparation"))?;
            }
            Ok(response)
        });
        tokio::select! {
            result = &mut job => result.map_err(|_| Failure::new("SIMULATION_SUPERVISOR_FAILED", "supervisor"))?,
            _ = tokio::time::sleep_until(deadline.into()) => {
                cancellation.store(true, Ordering::Release);
                // The signalled owned job keeps its permit through process reap.
                Err(Failure::new("SIMULATION_TIMEOUT", "supervisor"))
            }
        }
    }
}

fn diagnostic_response(
    outcome: Value,
    commitment: &ExecutionCommitment,
    bundle_hash: &str,
    worker_hash: &str,
) -> std::result::Result<Value, Failure> {
    // This normal worker contract still binds an unsigned Shanghai plan. A
    // signed OP receipt needs its own authenticated envelope/attribute source.
    if outcome.get("op_fees").is_some_and(|v| !v.is_null()) {
        return Err(Failure::new(
            "SIMULATION_WORKER_PROTOCOL",
            "op_execution_not_enabled",
        ));
    }
    let gas = outcome["execution"]["gas_used"]
        .as_u64()
        .ok_or_else(|| Failure::new("SIMULATION_WORKER_PROTOCOL", "receipt"))?;
    let model = xone_execution_preparation::evm_native_cost::measured_native_cost(
        gas,
        commitment.transaction_context.gas_limit,
        &commitment.execution_base_fee,
        &commitment.transaction_context.gas_price,
        commitment
            .transaction_context
            .max_priority_fee_per_gas
            .as_ref(),
    )
    .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "fees"))?;
    let before = commitment
        .caller_native_before
        .as_ref()
        .ok_or_else(|| Failure::new("SIMULATION_WORKER_PROTOCOL", "native_fee_funding"))?;
    xone_execution_preparation::evm_native_cost::validate_maximum_funding(&model, before)
        .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "native_fee_funding"))?;
    let mut payer_net_debit = Value::Null;
    let mut returned_priority = Value::Null;
    let mut balance_reconciliation =
        json!({"status":"UNAVAILABLE","reason":"LEGACY_NATIVE_BALANCES_NOT_REPORTED"});
    if commitment
        .transaction_context
        .max_priority_fee_per_gas
        .is_some()
        || outcome.get("native_fees").is_some_and(|v| !v.is_null())
    {
        let native: NativeFeeMeasurements = serde_json::from_value(outcome["native_fees"].clone())
            .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "fees"))?;
        if commitment.caller_native_before.as_ref() != Some(&native.caller_before)
            || commitment.coinbase_native_before.as_ref() != Some(&native.coinbase_before)
            || native.effective_gas_price != model.effective_gas_price
            || native.execution_fee != model.execution_fee
            || native.base_fee_burned != model.base_fee_burned
            || native.priority_fee_paid != model.priority_fee_paid
        {
            return Err(Failure::new("SIMULATION_WORKER_PROTOCOL", "fees"));
        }
        let account = |value: &str| {
            if value.len() != 42 || !value.starts_with("0x") {
                return Err(Failure::new(
                    "SIMULATION_WORKER_PROTOCOL",
                    "native_fee_balances",
                ));
            }
            hex::decode(&value[2..])
                .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "native_fee_balances"))
        };
        if commitment.transaction.value != Amount::zero() {
            return Err(Failure::new(
                "SIMULATION_WORKER_PROTOCOL",
                "native_fee_balances",
            ));
        }
        let same =
            account(&commitment.transaction.from)? == account(&commitment.execution_coinbase)?;
        let payer_cost = xone_execution_preparation::evm_native_cost::audited_payer_cost(
            &model,
            &xone_execution_preparation::evm_native_cost::NativeBalances {
                payer_before: native.caller_before,
                payer_after: native.caller_after,
                beneficiary_before: native.coinbase_before,
                beneficiary_after: native.coinbase_after,
            },
            same,
        )
        .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "native_fee_balances"))?;
        payer_net_debit = serde_json::to_value(&payer_cost.net_native_debit)
            .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "native_fee_balances"))?;
        returned_priority = serde_json::to_value(&payer_cost.priority_returned_to_payer)
            .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "native_fee_balances"))?;
        balance_reconciliation = json!({"status":"MATCHED","scope":"ZERO_VALUE_BOUND_ERC20_TRANSACTION",
            "caller":commitment.transaction.from,"coinbase":commitment.execution_coinbase,"sameAccount":same});
    }
    let costs = json!({
        "schemaVersion":1,"modelVersion":xone_execution_preparation::evm_native_cost::VERSION,
        "chainKey":commitment.transaction.chain_key,"bindingHash":commitment.binding_hash,
        "scope":"DECLARED_LOCAL_EVM_TRANSACTION","authority":"PROVIDED_LOCAL_STATE",
        "gasUnits":gas.to_string(),"gasNative":model.execution_fee,
        "nativeUnit":"EVM_NATIVE_ATOMS","balanceReconciliation":balance_reconciliation,
        "gasPriceNativePerUnit":model.effective_gas_price,
        "maxExecutionReserveNative":model.gas_limit_reserve,
        "reserveHeadroomNative":Amount::checked(&model.gas_limit_reserve.0 - &model.execution_fee.0)
            .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "fees"))?,
        "feeBreakdown":[{"kind":"EVM_EXECUTION","amountNative":model.execution_fee,
            "parts":{"baseFeeBurnedNative":model.base_fee_burned,"priorityFeeNative":model.priority_fee_paid}}],
        "nativeKnownSubtotal":model.execution_fee,"nativeKnownSubtotalKind":"GROSS_EXECUTION_CHARGE",
        "payerCostModelVersion":xone_execution_preparation::evm_native_cost::PAYER_COST_VERSION,
        "payerNetNativeDebit":payer_net_debit,"priorityReturnedToPayerNative":returned_priority,
        "maximumFundingAdmission":"SOURCE_BALANCE_COVERS_BOUND_GAS_RESERVE",
        "totalNativeEstimate":Value::Null,
        "l1DataNative":Value::Null,"approvalNative":Value::Null,
        "netBuyAmountEstimate":Value::Null,"netSellAmountEstimate":Value::Null,
        "missingComponents":["CHAIN_SPECIFIC_DATA_OR_OTHER_NATIVE_FEES","ADDITIONAL_APPROVAL_OR_FUNDING_OPERATIONS","TRUSTED_TOKEN_PRICE_CONVERSION"],
        "unavailableReason":"COMPLETE_CHAIN_COST_COVERAGE_UNAVAILABLE",
        "deductedFromQuoteOutput":false,"chainFeeCompleteness":"LOCAL_EVM_ONLY"
    });
    let success = outcome["matches_plan"] == true && outcome["execution"]["status"] == "SUCCESS";
    Ok(
        json!({"quoteKind":commitment.quote_kind,"bindingHash":commitment.binding_hash,"bundleHash":bundle_hash,"workerHash":worker_hash,
        "transaction":commitment.transaction,"transactionContext":commitment.transaction_context,"executionBaseFee":commitment.execution_base_fee,"executionCoinbase":commitment.execution_coinbase,"executor":commitment.executor,
        "verification":{"level":"LOCAL_EVM_EXECUTED","success":success,"verified":false},"verified":false,
        "authority":"PROVIDED_LOCAL_STATE","outcome":outcome,
        "fees":{"evmGasUsed":gas,"gasPriceWei":model.effective_gas_price,"evmFeeWei":model.execution_fee,"chainFeeCompleteness":"LOCAL_EVM_ONLY"},"costs":costs}),
    )
}

struct LimitedWriter(Vec<u8>);
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_WORKER_INPUT_BYTES {
            return Err(std::io::Error::other("input limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
async fn cancelled(flag: Arc<AtomicBool>) {
    while !flag.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn read_limited(
    reader: impl AsyncRead + Unpin,
    cap: usize,
    code: &'static str,
) -> std::result::Result<Vec<u8>, Failure> {
    let mut reader = reader.take(cap as u64 + 1);
    let mut output = Vec::new();
    reader
        .read_to_end(&mut output)
        .await
        .map_err(|_| Failure::new("SIMULATION_WORKER_IO", "read"))?;
    if output.len() > cap {
        return Err(Failure::new(code, "read"));
    }
    Ok(output)
}
async fn run_process(
    mut command: Command,
    input: Vec<u8>,
    deadline: Instant,
    cancellation: Arc<AtomicBool>,
) -> std::result::Result<Vec<u8>, Failure> {
    if input.len() > MAX_WORKER_INPUT_BYTES {
        return Err(Failure::new("SIMULATION_WORKER_INPUT_LIMIT", "write"));
    }
    // OS process creation can block (for example while validating a large image
    // on macOS). Keep it off the reactor. The owned job still awaits this handle,
    // and checks its cancellation/deadline immediately after the child exists.
    let mut child = tokio::task::spawn_blocking(move || {
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| Failure::new("SIMULATION_WORKER_SPAWN", "spawn"))
    })
    .await
    .map_err(|_| Failure::new("SIMULATION_WORKER_SPAWN", "spawn"))??;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let result = {
        let work = async {
            let write = async {
                stdin
                    .write_all(&input)
                    .await
                    .map_err(|_| Failure::new("SIMULATION_WORKER_IO", "write"))?;
                drop(stdin);
                Ok(())
            };
            let wait = async {
                child
                    .wait()
                    .await
                    .map_err(|_| Failure::new("SIMULATION_WORKER_IO", "wait"))
            };
            let (_, output, _, status) = tokio::try_join!(
                write,
                read_limited(
                    stdout,
                    MAX_WORKER_OUTPUT_BYTES,
                    "SIMULATION_WORKER_OUTPUT_LIMIT"
                ),
                read_limited(stderr, MAX_STDERR, "SIMULATION_WORKER_STDERR_LIMIT"),
                wait
            )?;
            if !status.success() {
                return Err(Failure::new("SIMULATION_WORKER_EXIT", "exit"));
            }
            Ok(output)
        };
        tokio::select! {
            value = work => value,
            _ = tokio::time::sleep_until(deadline.into()) => Err(Failure::new("SIMULATION_TIMEOUT", "execute")),
            _ = cancelled(cancellation) => Err(Failure::new("SIMULATION_CANCELLED", "execute")),
        }
    };
    if result.is_err() {
        let _ = child.start_kill();
        child
            .wait()
            .await
            .map_err(|_| Failure::new("SIMULATION_WORKER_REAP", "cleanup"))?;
    }
    result
}
#[derive(Debug)]
struct Decoded {
    outcome: Value,
    prepared: Option<xone_simulation::prepared_evm::PreparedSimulation>,
}
fn decode_response(bytes: &[u8], worker_hash: &str) -> std::result::Result<Decoded, Failure> {
    let response: WorkerResponse = serde_json::from_slice(bytes)
        .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?;
    if response.protocol != WORKER_PROTOCOL || response.worker_binary_sha256 != worker_hash {
        return Err(Failure::new("SIMULATION_WORKER_IDENTITY", "response"));
    }
    match (response.outcome, response.prepared, response.error) {
        (Some(outcome), None, None) => Ok(Decoded {
            outcome: serde_json::to_value(outcome)
                .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?,
            prepared: None,
        }),
        (None, Some(prepared), None) => {
            let mut value = serde_json::to_value(&prepared)
                .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?;
            value
                .as_object_mut()
                .ok_or_else(|| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?
                .remove("commitment_hash");
            if hash(&value).map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?
                != prepared.commitment_hash
            {
                return Err(Failure::new("SIMULATION_WORKER_PROTOCOL", "preparation"));
            }
            Ok(Decoded {
                outcome: serde_json::to_value(&prepared.simulation)
                    .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?,
                prepared: Some(prepared),
            })
        }
        (None, None, Some(error)) => Err(Failure {
            code: serde_json::to_value(error.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "SIMULATION_WORKER_PROTOCOL".into()),
            phase: error.phase.chars().take(64).collect(),
            usage: serde_json::to_value(error.usage).ok(),
        }),
        _ => Err(Failure::new("SIMULATION_WORKER_PROTOCOL", "response")),
    }
}

#[cfg(test)]
#[derive(Clone)]
struct TestLaunch {
    mode: String,
    pid_file: Option<PathBuf>,
}
#[cfg(test)]
impl TestLaunch {
    fn command(&self) -> Command {
        let script = match self.mode.as_str() {
            "lifecycle-sleep" | "http-cancellation" => {
                Some(r#"printf '%s' "$$" > "$1"; exec /bin/sleep 30"#)
            }
            "lifecycle-stdout" => Some(
                r#"printf '%s' "$$" > "$1"; exec /bin/dd if=/dev/zero bs=1024 count=1025 2>/dev/null"#,
            ),
            "lifecycle-stderr" => {
                Some(r#"printf '%s' "$$" > "$1"; exec /bin/dd if=/dev/zero bs=1024 count=65 1>&2"#)
            }
            "lifecycle-exit" => Some(r#"printf '%s' "$$" > "$1"; exit 7"#),
            _ => None,
        };
        if let Some(script) = script {
            // Lifecycle assertions start at a real, immediately owned PID.
            // They do not measure OS validation of the large Rust test image;
            // protocol/identity and actual EVM worker tests cover that boundary.
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script, "xone-owned-worker"]);
            command.arg(self.pid_file.as_ref().expect("owned worker PID path"));
            return command;
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "local_simulation::tests::worker_probe",
                "--ignored",
                "--nocapture",
            ])
            .env("XONE_PRIVATE_SIMULATION_PROBE_MODE", &self.mode);
        if let Some(path) = &self.pid_file {
            command.env("XONE_PRIVATE_SIMULATION_PID_FILE", path);
        }
        command
    }
}

#[cfg(test)]
#[path = "../../execution-preparation/tests/support/evm_rpc_fixture.rs"]
mod execution_source;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn supervisor_hard_deadline_reaps_child() {
        // Measure termination/reap, not macOS validation of a large newly linked
        // test executable. Lifecycle probes use tiny owned children; the
        // evm-execution smoke separately checks the actual binary/protocol.
        // This child reports its exact owned PID before blocking.
        let pid = pid_path();
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args([
            "-c",
            r#"printf '%s' "$$" > "$1"; exec /bin/sleep 30"#,
            "xone-worker-timeout",
        ]);
        command.arg(&pid);
        let started = std::time::Instant::now();
        let error = run_process(
            command,
            Vec::new(),
            started + std::time::Duration::from_millis(50),
            std::sync::Arc::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "SIMULATION_TIMEOUT");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert_reaped(&pid);
    }

    fn pid_path() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "xone-api-worker-{}-{}.pid",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn use_owned_cancellation_worker(api: &mut crate::Api, pid: PathBuf) {
        let worker = Arc::get_mut(api.local_simulation.as_mut().unwrap()).unwrap();
        // The cancellation probe actually launches this tiny executable. Keep
        // its real identity guard, rather than timing a debug Rust image hash
        // before the owned child exists. Real Rust worker checks stay separate.
        worker.worker_path = PathBuf::from("/bin/sh");
        worker.worker_sha256 = executable_sha256(&worker.worker_path).unwrap();
        worker.test_launch = Some(TestLaunch {
            mode: "http-cancellation".into(),
            pid_file: Some(pid),
        });
    }
    fn assert_reaped(path: &std::path::Path) {
        if let Ok(pid) = std::fs::read_to_string(path) {
            assert!(
                !std::process::Command::new("/bin/kill")
                    .args(["-0", &pid])
                    .stderr(Stdio::null())
                    .status()
                    .unwrap()
                    .success(),
                "worker {pid} remains alive"
            );
            std::fs::remove_file(path).unwrap();
        }
    }
    #[tokio::test]
    async fn supervisor_enforces_stdout_stderr_caps_and_child_exit() {
        for (mode, expected) in [
            ("lifecycle-stdout", "SIMULATION_WORKER_OUTPUT_LIMIT"),
            ("lifecycle-stderr", "SIMULATION_WORKER_STDERR_LIMIT"),
            ("lifecycle-exit", "SIMULATION_WORKER_EXIT"),
        ] {
            let pid = pid_path();
            let launch = TestLaunch {
                mode: mode.into(),
                pid_file: Some(pid.clone()),
            };
            let error = run_process(
                launch.command(),
                Vec::new(),
                Instant::now() + Duration::from_secs(10),
                Arc::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, expected);
            assert_reaped(&pid);
        }
    }
    #[tokio::test]
    async fn supervisor_cancellation_interrupts_blocked_stdin_and_reaps() {
        let pid = pid_path();
        let flag = Arc::new(AtomicBool::new(false));
        let cancel = flag.clone();
        let launch = TestLaunch {
            mode: "lifecycle-sleep".into(),
            pid_file: Some(pid.clone()),
        };
        let task = tokio::spawn(run_process(
            launch.command(),
            vec![0; 1024 * 1024],
            Instant::now() + Duration::from_secs(10),
            flag,
        ));
        let deadline = Instant::now() + Duration::from_secs(8);
        while !pid.exists() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(pid.exists(), "child did not start");
        cancel.store(true, Ordering::Release);
        assert_eq!(
            task.await.unwrap().unwrap_err().code,
            "SIMULATION_CANCELLED"
        );
        assert_reaped(&pid);
    }
    #[test]
    fn protocol_identity_shape_and_error_redaction_are_enforced() {
        let error = json!({"code":"SIMULATION_DEADLINE","phase":"swap","detail":"private stderr, code and state must not leak","usage":{"reads":0,"read_bytes":0,"instructions":0,"input_bytes":0,"gas_used":0,"executions":0}});
        let response = json!({"protocol":WORKER_PROTOCOL,"worker_binary_sha256":"expected","outcome":null,"error":error});
        let failure =
            decode_response(&serde_json::to_vec(&response).unwrap(), "expected").unwrap_err();
        assert_eq!(failure.code, "SIMULATION_DEADLINE");
        assert_eq!(failure.phase, "swap");
        assert!(!format!("{failure:?}").contains("private stderr"));
        for case in ["protocol", "hash", "no_result", "unknown", "malformed"] {
            let mut value = response.clone();
            match case {
                "protocol" => value["protocol"] = "wrong/1".into(),
                "hash" => value["worker_binary_sha256"] = "other".into(),
                "no_result" => value["error"] = Value::Null,
                "unknown" => value["extra"] = true.into(),
                _ => (),
            }
            let bytes = if case == "malformed" {
                b"not json".to_vec()
            } else {
                serde_json::to_vec(&value).unwrap()
            };
            assert!(decode_response(&bytes, "expected").is_err(), "{case}");
        }
    }
    #[test]
    fn serialization_input_cap_is_enforced_before_spawn() {
        let mut writer = LimitedWriter(vec![0; MAX_WORKER_INPUT_BYTES]);
        assert!(writer.write_all(&[1]).is_err());
    }

    fn outcome(status: &str, matches_plan: bool) -> Value {
        json!({"verified":false,"authority":"PROVIDED_LOCAL_STATE","plan_validation":"BOUND", "execution":{"status":status,"gas_used":21000,"gas_refunded":0,"output":"0x","return_classification":"test","halt_reason":null,"sender_nonce_before":0,"sender_nonce_after":1,"post_state_digest":"digest"},"matches_plan":matches_plan,"balances":{"caller_sell_before":"1","caller_sell_after":"0","recipient_buy_before":"0","recipient_buy_after":"1"},"actual_input_consumed":"1","refund":"0","actual_buy_amount":"1","observations":[],"usage":{"reads":0,"read_bytes":0,"instructions":0,"input_bytes":0,"gas_used":21000,"executions":1},"source_fixture_hash":"fixture","simulator_version":"test-only","receipt_hash":"receipt"})
    }
    fn diagnostic_commitment(gas_price: &str) -> ExecutionCommitment {
        ExecutionCommitment {
            binding_hash: "binding".into(),
            quote_kind: "EXACT_IN",
            expected_preparation: None,
            transaction: Transaction {
                chain_key: "evm:31337".into(),
                from: "0x0000000000000000000000000000000000000011".into(),
                to: "0x0000000000000000000000000000000000000022".into(),
                data: "0x38ed1739".into(),
                value: Amount::zero(),
            },
            transaction_context: TransactionContext {
                nonce: 7,
                gas_limit: 1_000_000,
                gas_price: gas_price.parse().unwrap(),
                max_priority_fee_per_gas: None,
                chain_id: 31337,
            },
            execution_base_fee: "1".parse().unwrap(),
            execution_coinbase: "0x0000000000000000000000000000000000000033".into(),
            caller_native_before: Some("1000000000000000000000000000000".parse().unwrap()),
            coinbase_native_before: Some(Amount::zero()),
            executor: ExecutorIdentity {
                target: "0x0000000000000000000000000000000000000022".into(),
                code_sha256: "ab".repeat(32),
                factory: "0x0000000000000000000000000000000000000033".into(),
                factory_code_sha256: "cd".repeat(32),
                adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
            },
        }
    }
    #[test]
    fn normal_shanghai_worker_rejects_unbound_op_fee_receipts() {
        let mut value = outcome("SUCCESS", true);
        value["op_fees"] =
            json!({"transaction_hash":"different","chain_cost":{"gross_charge":"0"}});
        let rejected = diagnostic_response(
            value,
            &diagnostic_commitment("2000000000"),
            "bundle",
            "worker",
        );
        assert!(rejected.is_err());
    }
    #[test]
    fn actual_success_and_fee_token_mismatch_never_claim_verified_or_full_chain_fees() {
        for (status, matched, success) in [
            ("SUCCESS", true, true),
            ("SUCCESS", false, false),
            ("REVERT", false, false),
            ("HALT", false, false),
        ] {
            let value = diagnostic_response(
                outcome(status, matched),
                &diagnostic_commitment("2000000000"),
                "bundle",
                "worker",
            )
            .unwrap();
            assert_eq!(value["verification"]["success"], success);
            assert_eq!(value["verified"], false);
            assert_eq!(value["authority"], "PROVIDED_LOCAL_STATE");
            assert_eq!(value["fees"]["evmFeeWei"], "42000000000000");
            assert_eq!(value["fees"]["chainFeeCompleteness"], "LOCAL_EVM_ONLY");
        }
        let mut forged = outcome("SUCCESS", true);
        forged["verified"] = true.into();
        let response = json!({"protocol":WORKER_PROTOCOL,"worker_binary_sha256":"expected","outcome":forged,"error":null});
        assert!(decode_response(&serde_json::to_vec(&response).unwrap(), "expected").is_err());
    }

    #[test]
    fn dynamic_fee_api_reports_measured_effective_price_and_rejects_inconsistent_fees() {
        for (cap, tip, price) in [("10", "2", "3"), ("2", "2", "2"), ("1", "0", "1")] {
            let mut commitment = diagnostic_commitment(cap);
            commitment.transaction_context.gas_limit = 100000;
            commitment.caller_native_before = Some("1000000".parse().unwrap());
            commitment.transaction_context.max_priority_fee_per_gas = Some(tip.parse().unwrap());
            let mut value = outcome("SUCCESS", true);
            let price: u64 = price.parse().unwrap();
            value["native_fees"] = json!({"caller_before":"1000000","caller_after":(1000000 - 21000*price).to_string(),
                "coinbase_before":"0","coinbase_after":(21000*(price-1)).to_string(),
                "effective_gas_price":price.to_string(),"execution_fee":(21000*price).to_string(),
                "base_fee_burned":"21000","priority_fee_paid":(21000*(price-1)).to_string()});
            let response =
                diagnostic_response(value.clone(), &commitment, "bundle", "worker").unwrap();
            assert_eq!(response["fees"]["gasPriceWei"], price.to_string());
            assert_eq!(response["fees"]["evmFeeWei"], (21000 * price).to_string());
            assert_eq!(response["executionBaseFee"], "1");
            assert_eq!(response["fees"]["chainFeeCompleteness"], "LOCAL_EVM_ONLY");
            for field in [
                "effective_gas_price",
                "execution_fee",
                "base_fee_burned",
                "priority_fee_paid",
            ] {
                let mut changed = value.clone();
                changed["native_fees"][field] = "0".into();
                if value["native_fees"][field] == "0" {
                    changed["native_fees"][field] = "1".into();
                }
                assert!(
                    diagnostic_response(changed, &commitment, "bundle", "worker").is_err(),
                    "{field}"
                );
            }
            value.as_object_mut().unwrap().remove("native_fees");
            assert!(diagnostic_response(value, &commitment, "bundle", "worker").is_err());
        }
    }

    #[test]
    fn native_cost_breakdown_discloses_unknown_components_without_double_counting_or_net_claim() {
        let commitment = diagnostic_commitment("2000000000");
        let value =
            diagnostic_response(outcome("SUCCESS", true), &commitment, "bundle", "worker").unwrap();
        let costs = &value["costs"];
        assert_eq!(
            costs["modelVersion"],
            xone_execution_preparation::evm_native_cost::VERSION
        );
        assert_eq!(costs["gasUnits"], "21000");
        assert_eq!(costs["gasNative"], value["fees"]["evmFeeWei"]);
        assert_eq!(costs["nativeKnownSubtotal"], costs["gasNative"]);
        assert_eq!(costs["feeBreakdown"][0]["amountNative"], costs["gasNative"]);
        assert_eq!(costs["maxExecutionReserveNative"], "2000000000000000");
        assert_eq!(costs["reserveHeadroomNative"], "1958000000000000");
        assert_eq!(costs["balanceReconciliation"]["status"], "UNAVAILABLE");
        assert_eq!(costs["deductedFromQuoteOutput"], false);
        assert_eq!(costs["chainFeeCompleteness"], "LOCAL_EVM_ONLY");
        assert!(costs["l1DataNative"].is_null());
        assert!(costs["approvalNative"].is_null());
        assert!(costs["totalNativeEstimate"].is_null());
        assert!(costs["netBuyAmountEstimate"].is_null());
        assert!(costs["netSellAmountEstimate"].is_null());
        assert_eq!(costs["missingComponents"].as_array().unwrap().len(), 3);
        assert_eq!(costs["bindingHash"], commitment.binding_hash);
        assert_eq!(costs["chainKey"], commitment.transaction.chain_key);
        assert_eq!(value["verified"], false);
    }

    #[test]
    fn native_cost_receipt_cannot_exceed_signed_gas_or_use_invalid_base_price() {
        let mut commitment = diagnostic_commitment("2");
        for gas in [0, 20999, commitment.transaction_context.gas_limit + 1] {
            let mut receipt = outcome("SUCCESS", true);
            receipt["execution"]["gas_used"] = gas.into();
            assert!(diagnostic_response(receipt, &commitment, "bundle", "worker").is_err());
        }
        commitment.execution_base_fee = "3".parse().unwrap();
        assert!(
            diagnostic_response(outcome("SUCCESS", true), &commitment, "bundle", "worker").is_err()
        );
        commitment.execution_base_fee = "1".parse().unwrap();
        let mut receipt = outcome("SUCCESS", true);
        receipt["native_fees"] = json!({"execution_fee":"0"});
        assert!(diagnostic_response(receipt, &commitment, "bundle", "worker").is_err());
    }

    #[test]
    fn dynamic_fee_balance_reconciliation_rejects_changed_measurements_and_handles_alias() {
        for same in [false, true] {
            for base in ["0", "1"] {
                let mut commitment = diagnostic_commitment("10");
                commitment.transaction_context.gas_limit = 100000;
                commitment.execution_base_fee = base.parse().unwrap();
                commitment.transaction_context.max_priority_fee_per_gas =
                    Some("2".parse().unwrap());
                if same {
                    commitment.execution_coinbase = commitment.transaction.from.clone();
                }
                let base: u64 = base.parse().unwrap();
                let before = 1_000_000u64;
                let fee = 21_000 * (base + 2);
                let burn = 21_000 * base;
                let tip = 42_000;
                let (payer_after, coinbase_before, coinbase_after) = if same {
                    (before - burn, before, before - burn)
                } else {
                    (before - fee, 17, 17 + tip)
                };
                commitment.caller_native_before = Some(before.to_string().parse().unwrap());
                commitment.coinbase_native_before =
                    Some(coinbase_before.to_string().parse().unwrap());
                let mut value = outcome("SUCCESS", true);
                value["native_fees"] = json!({"caller_before":before.to_string(),"caller_after":payer_after.to_string(),"coinbase_before":coinbase_before.to_string(),"coinbase_after":coinbase_after.to_string(),"effective_gas_price":(base+2).to_string(),"execution_fee":fee.to_string(),"base_fee_burned":burn.to_string(),"priority_fee_paid":tip.to_string()});
                let result =
                    diagnostic_response(value.clone(), &commitment, "bundle", "worker").unwrap();
                assert_eq!(
                    result["costs"]["balanceReconciliation"]["status"],
                    "MATCHED"
                );
                assert_eq!(
                    result["costs"]["balanceReconciliation"]["sameAccount"],
                    same
                );
                assert_eq!(result["executionCoinbase"], commitment.execution_coinbase);
                assert_eq!(result["costs"]["nativeUnit"], "EVM_NATIVE_ATOMS");
                assert_eq!(
                    result["costs"]["payerNetNativeDebit"],
                    (before - payer_after).to_string()
                );
                assert_eq!(
                    result["costs"]["priorityReturnedToPayerNative"],
                    if same { tip.to_string() } else { "0".into() }
                );
                for field in [
                    "caller_before",
                    "caller_after",
                    "coinbase_before",
                    "coinbase_after",
                ] {
                    let mut changed = value.clone();
                    let n: u64 = changed["native_fees"][field]
                        .as_str()
                        .unwrap()
                        .parse()
                        .unwrap();
                    changed["native_fees"][field] = (n + 1).to_string().into();
                    assert!(
                        diagnostic_response(changed, &commitment, "bundle", "worker").is_err(),
                        "{field} same={same}"
                    );
                }
                for fields in [
                    ["caller_before", "caller_after"],
                    ["coinbase_before", "coinbase_after"],
                ] {
                    let mut changed = value.clone();
                    for field in fields {
                        let n: u64 = changed["native_fees"][field]
                            .as_str()
                            .unwrap()
                            .parse()
                            .unwrap();
                        changed["native_fees"][field] = (n + 1).to_string().into();
                    }
                    assert!(
                        diagnostic_response(changed, &commitment, "bundle", "worker").is_err(),
                        "shifted balances must still bind to original input"
                    );
                }
                let before_commitment = commitment.caller_native_before.take();
                assert!(
                    diagnostic_response(value.clone(), &commitment, "bundle", "worker").is_err()
                );
                commitment.caller_native_before = before_commitment;
                commitment.transaction.value = "1".parse().unwrap();
                assert!(diagnostic_response(value, &commitment, "bundle", "worker").is_err());
            }
        }
    }

    #[test]
    fn legacy_fee_balance_reconciliation_binds_source_and_rejects_shifted_or_forged_native_cost() {
        for same in [false, true] {
            for base in ["0", "1"] {
                let mut commitment = diagnostic_commitment("10");
                commitment.transaction_context.gas_limit = 100000;
                commitment.execution_base_fee = base.parse().unwrap();
                commitment.transaction_context.max_priority_fee_per_gas = None;
                if same {
                    commitment.execution_coinbase = commitment.transaction.from.clone();
                }
                let base: u64 = base.parse().unwrap();
                let before = 1_000_000u64;
                let fee = 21_000 * 10;
                let burn = 21_000 * base;
                let tip = 21_000 * (10 - base);
                let (payer_after, coinbase_before, coinbase_after) = if same {
                    (before - burn, before, before - burn)
                } else {
                    (before - fee, 17, 17 + tip)
                };
                commitment.caller_native_before = Some(before.to_string().parse().unwrap());
                commitment.coinbase_native_before =
                    Some(coinbase_before.to_string().parse().unwrap());
                let mut value = outcome("SUCCESS", true);
                value["native_fees"] = json!({"caller_before":before.to_string(),"caller_after":payer_after.to_string(),"coinbase_before":coinbase_before.to_string(),"coinbase_after":coinbase_after.to_string(),"effective_gas_price":"10","execution_fee":fee.to_string(),"base_fee_burned":burn.to_string(),"priority_fee_paid":tip.to_string()});
                let result =
                    diagnostic_response(value.clone(), &commitment, "bundle", "worker").unwrap();
                assert_eq!(
                    result["costs"]["balanceReconciliation"]["status"],
                    "MATCHED"
                );
                assert_eq!(
                    result["costs"]["balanceReconciliation"]["sameAccount"],
                    same
                );
                assert_eq!(result["executionCoinbase"], commitment.execution_coinbase);
                assert_eq!(result["costs"]["nativeUnit"], "EVM_NATIVE_ATOMS");
                assert_eq!(
                    result["costs"]["payerNetNativeDebit"],
                    (before - payer_after).to_string()
                );
                assert_eq!(
                    result["costs"]["priorityReturnedToPayerNative"],
                    if same { tip.to_string() } else { "0".into() }
                );
                for field in [
                    "effective_gas_price",
                    "execution_fee",
                    "base_fee_burned",
                    "priority_fee_paid",
                ] {
                    let mut changed = value.clone();
                    changed["native_fees"][field] = "1".into();
                    assert!(
                        diagnostic_response(changed, &commitment, "bundle", "worker").is_err(),
                        "{field}"
                    );
                }
                for field in [
                    "caller_before",
                    "caller_after",
                    "coinbase_before",
                    "coinbase_after",
                ] {
                    let mut changed = value.clone();
                    let n: u64 = changed["native_fees"][field]
                        .as_str()
                        .unwrap()
                        .parse()
                        .unwrap();
                    changed["native_fees"][field] = (n + 1).to_string().into();
                    assert!(
                        diagnostic_response(changed, &commitment, "bundle", "worker").is_err(),
                        "{field} same={same}"
                    );
                }
                for fields in [
                    ["caller_before", "caller_after"],
                    ["coinbase_before", "coinbase_after"],
                ] {
                    let mut changed = value.clone();
                    for field in fields {
                        let n: u64 = changed["native_fees"][field]
                            .as_str()
                            .unwrap()
                            .parse()
                            .unwrap();
                        changed["native_fees"][field] = (n + 1).to_string().into();
                    }
                    assert!(
                        diagnostic_response(changed, &commitment, "bundle", "worker").is_err(),
                        "shifted balances must still bind to original input"
                    );
                }
                let before_commitment = commitment.caller_native_before.take();
                assert!(
                    diagnostic_response(value.clone(), &commitment, "bundle", "worker").is_err()
                );
                commitment.caller_native_before = before_commitment;
                commitment.transaction.value = "1".parse().unwrap();
                assert!(diagnostic_response(value, &commitment, "bundle", "worker").is_err());
            }
        }
    }

    #[test]
    fn fee_funding_source_must_cover_maximum_even_with_low_spend_and_legacy_net_stays_unknown() {
        let mut commitment = diagnostic_commitment("10");
        let mut value = outcome("SUCCESS", true);
        // Legacy has no post native measurement, but before is still bound to
        // the serialized worker input and must cover the maximum reserve.
        commitment.caller_native_before = Some("9999999".parse().unwrap());
        assert!(diagnostic_response(value.clone(), &commitment, "bundle", "worker").is_err());
        commitment.caller_native_before = Some("10000000".parse().unwrap());
        let legacy = diagnostic_response(value.clone(), &commitment, "bundle", "worker").unwrap();
        assert!(legacy["costs"]["payerNetNativeDebit"].is_null());
        assert_eq!(
            legacy["costs"]["nativeKnownSubtotalKind"],
            "GROSS_EXECUTION_CHARGE"
        );
        commitment.caller_native_before = None;
        assert!(diagnostic_response(value.clone(), &commitment, "bundle", "worker").is_err());
        commitment.transaction_context.max_priority_fee_per_gas = Some("2".parse().unwrap());
        commitment.execution_base_fee = "0".parse().unwrap();
        commitment.execution_coinbase = commitment.transaction.from.clone();
        commitment.caller_native_before = Some(Amount::zero());
        commitment.coinbase_native_before = Some(Amount::zero());
        value["native_fees"] = json!({"caller_before":"0","caller_after":"0","coinbase_before":"0","coinbase_after":"0","effective_gas_price":"2","execution_fee":"42000","base_fee_burned":"0","priority_fee_paid":"42000"});
        assert!(diagnostic_response(value, &commitment, "bundle", "worker").is_err());
    }

    async fn api_with_cached_quote() -> (crate::Api, String) {
        let corpus: Value =
            serde_json::from_str(include_str!("../../../test/contracts/evm-execution.json"))
                .unwrap();
        let c = &corpus["cases"][0];
        let bundle = json!({"schema_version":1,"fixture":{"schema_version":1,"state_anchor":c["state_anchor"],"execution_environment":c["execution_environment"],"accounts":c["accounts"],"block_hashes":c["block_hashes"]},"executor":c["executor"],"transaction_context":{"nonce":c["transaction_context"]["nonce"],"gas_limit":c["transaction_context"]["gas_limit"].as_u64().expect("native fixture schema: transaction_context.gas_limit must be u64"),"gas_price":c["transaction_context"]["gas_price"],"chain_id":c["transaction_context"]["chain_id"]}});
        let file = pid_path().with_extension("json");
        std::fs::write(&file, serde_json::to_vec(&bundle).unwrap()).unwrap();
        let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
        config.token = "private-supervisor-test-token".into();
        config.config.routing.max_state_age_ms = 315_360_000_000;
        config.config.routing.max_splits = 1;
        config.config.routing.quote_ttl_ms = 60_000;
        let sim = &mut config.config.simulation;
        sim.backend = "revm-fixture".into();
        sim.rpc_url.clear();
        sim.state_file = file.to_string_lossy().into_owned();
        sim.max_state_bytes = 4 << 20;
        sim.max_instructions = 1_000_000;
        sim.max_memory_bytes = 4 << 20;
        sim.concurrency = 1;
        sim.timeout_ms = 10_000;
        sim.allowed_targets = vec![c["executor"]["target"].as_str().unwrap().into()];
        config
            .config
            .chains
            .iter_mut()
            .find(|c| c.key == "evm:31337")
            .unwrap()
            .genesis_hash = bundle["fixture"]["state_anchor"]["genesis_hash"]
            .as_str()
            .unwrap()
            .into();
        let api = crate::Api::new(config).unwrap();
        let captured = api.local_simulation.as_ref().unwrap().bundle_sha256.clone();
        std::fs::write(&file, b"invalid replacement after capture").unwrap();
        assert_eq!(
            api.local_simulation.as_ref().unwrap().bundle_sha256,
            captured
        );
        assert_eq!(
            api.local_simulation
                .as_ref()
                .unwrap()
                .bundle
                .fixture
                .state_anchor
                .block_hash,
            c["state_anchor"]["block_hash"].as_str().unwrap()
        );
        std::fs::remove_file(file).unwrap();
        let mut snapshot: xone_domain::Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        snapshot.pools = vec![serde_json::from_value(c["pool"].clone()).unwrap()];
        snapshot.reference.block = xone_domain::BlockRef {
            chain_key: "evm:31337".into(),
            number_or_slot: c["state_anchor"]["block_number"].as_u64().unwrap(),
            hash: c["state_anchor"]["block_hash"].as_str().unwrap().into(),
            parent_hash: c["state_anchor"]["parent_hash"].as_str().unwrap().into(),
            timestamp_ms: c["state_anchor"]["timestamp"].as_u64().unwrap() * 1000,
        };
        snapshot.reference.scope_hash = xone_domain::scope_hash(&snapshot.pools).unwrap();
        snapshot.reference.adapter_set_hash = hash(&vec!["uniswap-v2/1"]).unwrap();
        api.replica.install_baseline(snapshot).unwrap();
        let quote = api.quote(serde_json::from_value(json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":c["token_in"],"buyToken":c["token_out"],"sellAmount":c["amount_in"],"caller":c["transaction"]["from"],"recipient":c["recipient"],"slippageBps":50,"verification":"FAST"})).unwrap()).await.unwrap();
        (api, quote["quoteId"].as_str().unwrap().into())
    }

    #[tokio::test]
    async fn post_execution_freshness_is_rechecked_and_evidence_preserves_quote_data() {
        let (api, id) = api_with_cached_quote().await;
        let cached = api.cache.lock().unwrap()[&id].clone();
        let backend = api.local_simulation.as_ref().unwrap();
        let bound = xone_execution_plan::bind_v2_execution(
            &cached.plan,
            &cached.view.pools,
            &backend.bundle.executor,
            &api.config.config.simulation.allowed_targets,
        )
        .unwrap();
        let request = WorkerRequest {
            protocol: WORKER_PROTOCOL.into(),
            bound: Some(bound),
            exact_out_bound: None,
            prepared: None,
            pools: cached.view.pools.clone(),
            allowed_targets: api.config.config.simulation.allowed_targets.clone(),
            fixture: backend.bundle.fixture.clone(),
            transaction_context: backend.bundle.transaction_context.clone(),
            budget: backend.budget.clone(),
            expected_genesis_hash: backend.bundle.fixture.state_anchor.genesis_hash.clone(),
            timeout_ms: 10_000,
        };
        let expected_input = serde_json::to_value(&request).unwrap();
        let prepared = PreparedExecution::from_request(request).unwrap();
        let child_input: Value = serde_json::from_slice(&prepared.input).unwrap();
        assert_eq!(child_input, expected_input);
        let response = diagnostic_response(
            outcome("SUCCESS", true),
            &prepared.commitment,
            "bundle",
            "worker",
        )
        .unwrap();
        let result = crate::publish_simulation(&api, &id, &cached, response.clone())
            .unwrap()
            .0;
        let after = api.cache.lock().unwrap()[&id].clone();
        assert_eq!(after.response, cached.response);
        assert_eq!(result["transaction"], child_input["bound"]["transaction"]);
        assert_eq!(
            result["transactionContext"],
            child_input["transaction_context"]
        );
        assert_eq!(result["executor"], child_input["bound"]["executor"]);
        assert_eq!(result["bindingHash"], child_input["bound"]["binding_hash"]);
        assert_eq!(
            serde_json::to_value(&after.plan).unwrap(),
            serde_json::to_value(&cached.plan).unwrap()
        );
        assert_eq!(after.simulation_evidence, Some(result.clone()));
        use tower::ServiceExt;
        let evidence = api
            .handler()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/internal/quotes/{id}/evidence"))
                    .header("authorization", format!("Bearer {}", api.config.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let evidence: Value = serde_json::from_slice(
            &axum::body::to_bytes(evidence.into_body(), 1 << 20)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(evidence["quote"], cached.response);
        assert_eq!(
            evidence["execution_plan"],
            serde_json::to_value(&cached.plan).unwrap()
        );
        assert_eq!(evidence["simulation_evidence"], result);
        api.cache.lock().unwrap().get_mut(&id).unwrap().expires = 0;
        assert_eq!(
            crate::publish_simulation(&api, &id, &cached, response.clone())
                .unwrap_err()
                .1
                 .0["error"],
            "QUOTE_EXPIRED"
        );
        api.cache.lock().unwrap().get_mut(&id).unwrap().expires = cached.expires;
        api.replica.suspend("evm:31337");
        assert_eq!(
            crate::publish_simulation(&api, &id, &cached, response)
                .unwrap_err()
                .1
                 .0["error"],
            "SNAPSHOT_REORGED"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http_drop_kills_worker_releases_owned_permit_and_keeps_cache_unlocked() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (mut api, id) = api_with_cached_quote().await;
        let pid = pid_path();
        use_owned_cancellation_worker(&mut api, pid.clone());
        let handler = api.handler();
        let request = || {
            Request::builder()
                .method("POST")
                .uri(format!("/v1/quotes/{id}/simulate"))
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(Body::empty())
                .unwrap()
        };
        let task = tokio::spawn(handler.oneshot(request()));
        let deadline = Instant::now() + Duration::from_secs(8);
        while !pid.exists() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if !pid.exists() && task.is_finished() {
            let response = task.await.unwrap().unwrap();
            let status = response.status();
            let body = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap();
            panic!(
                "worker request completed before startup: {status} {}",
                String::from_utf8_lossy(&body)
            );
        }
        assert!(pid.exists(), "worker did not start");
        assert!(
            api.cache.try_lock().is_ok(),
            "cache lock held during worker I/O"
        );
        let quote_permit = api.permits.clone().try_acquire_owned().unwrap();
        assert_eq!(
            api.local_simulation
                .as_ref()
                .unwrap()
                .permits
                .available_permits(),
            0
        );
        let busy = api.handler().oneshot(request()).await.unwrap();
        assert_eq!(busy.status(), StatusCode::TOO_MANY_REQUESTS);
        drop(quote_permit);
        task.abort();
        let _ = task.await;
        let deadline = Instant::now() + Duration::from_secs(5);
        while api
            .local_simulation
            .as_ref()
            .unwrap()
            .permits
            .available_permits()
            == 0
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            api.local_simulation
                .as_ref()
                .unwrap()
                .permits
                .available_permits(),
            1
        );
        assert_reaped(&pid);
        assert!(api.cache.lock().unwrap()[&id].simulation_evidence.is_none());
    }

    async fn api_with_exact_out_quote() -> (crate::Api, String) {
        let (bound, pools, rpc, original) = super::execution_source::fixture();
        let file = pid_path().with_extension("state.json");
        let value = json!({"schema_version":1,"fixture":{"schema_version":1,
            "state_anchor":original["state_anchor"],"execution_environment":original["execution_environment"],
            "accounts":original["accounts"],"block_hashes":[]},"executor":rpc.executor,
            "transaction_context":original["transaction_context"]});
        std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
        config.token = "exact-out-local-api-test-20261117".into();
        config
            .config
            .chains
            .retain(|c| c.key == bound.plan.context.chain_key);
        config.config.chains[0].genesis_hash = rpc.genesis_hash;
        config.config.simulation.backend = "revm-fixture".into();
        config.config.simulation.state_file = file.to_str().unwrap().into();
        config.config.simulation.allowed_targets = vec![bound.executor.target.clone()];
        config.config.simulation.max_state_bytes = 8 * 1024 * 1024;
        config.config.simulation.max_state_reads = 8192;
        config.config.simulation.max_instructions = 2_000_000;
        config.config.simulation.max_memory_bytes = 64 * 1024 * 1024;
        config.config.simulation.concurrency = 1;
        config.config.simulation.max_gas = 15_000_000;
        config.config.simulation.timeout_ms = 10000;
        config.config.routing.max_splits = 1;
        config.config.routing.quote_ttl_ms = 60000;
        let api = crate::Api::new(config).unwrap();
        std::fs::remove_file(file).unwrap();
        api.replica
            .install_baseline(xone_domain::Snapshot {
                reference: bound.plan.snapshot.clone(),
                pools,
                source: "RETAINED_ORIGINAL_BANK".into(),
                history_gaps: vec![],
            })
            .unwrap();
        let quote=api.quote_exact_out(serde_json::from_value(json!({"chainKey":bound.plan.context.chain_key,
            "kind":"EXACT_OUT","sellToken":bound.plan.context.sell_token,"buyToken":bound.plan.context.buy_token,
            "buyAmount":bound.plan.allocation.gross_buy_amount,
            "maxSellAmount":Amount::checked(&bound.plan.allocation.sell_amount.0*2u64).unwrap(),
            "caller":bound.plan.context.caller,"recipient":bound.plan.context.recipient,
            "slippageBps":50,"verification":"FAST"})).unwrap()).await.unwrap();
        (api, quote["quoteId"].as_str().unwrap().into())
    }

    #[tokio::test]
    async fn exact_out_worker_request_selects_one_mode_and_preserves_legacy_wire() {
        let (api, id) = api_with_exact_out_quote().await;
        let cached = api.exact_out_cache.lock().unwrap()[&id].clone();
        let backend = api.local_simulation.as_ref().unwrap();
        let bound = xone_execution_plan::bind_v2_exact_out_execution(
            &cached.plan,
            &cached.view.pools,
            &backend.bundle.executor,
            &api.config.config.simulation.allowed_targets,
        )
        .unwrap();
        let request = WorkerRequest {
            protocol: WORKER_PROTOCOL.into(),
            bound: None,
            exact_out_bound: Some(bound),
            prepared: None,
            pools: cached.view.pools.clone(),
            allowed_targets: api.config.config.simulation.allowed_targets.clone(),
            fixture: backend.bundle.fixture.clone(),
            transaction_context: backend.bundle.transaction_context.clone(),
            budget: backend.budget.clone(),
            expected_genesis_hash: backend.bundle.fixture.state_anchor.genesis_hash.clone(),
            timeout_ms: 10000,
        };
        let value = serde_json::to_value(&request).unwrap();
        assert!(value.get("bound").is_none());
        let prepared = PreparedExecution::from_request(request.clone()).unwrap();
        assert_eq!(prepared.commitment.quote_kind, "EXACT_OUT");
        assert!(prepared
            .commitment
            .transaction
            .data
            .starts_with("0x8803dbee"));
        let mut missing = request.clone();
        missing.exact_out_bound = None;
        assert!(PreparedExecution::from_request(missing).is_err());
        let (other, other_id) = api_with_cached_quote().await;
        let other_cached = other.cache.lock().unwrap()[&other_id].clone();
        let other_backend = other.local_simulation.as_ref().unwrap();
        let legacy = xone_execution_plan::bind_v2_execution(
            &other_cached.plan,
            &other_cached.view.pools,
            &other_backend.bundle.executor,
            &other.config.config.simulation.allowed_targets,
        )
        .unwrap();
        let mut ambiguous = request;
        ambiguous.bound = Some(legacy);
        assert!(PreparedExecution::from_request(ambiguous.clone()).is_err());
        ambiguous.exact_out_bound = None;
        let legacy_value = serde_json::to_value(&ambiguous).unwrap();
        assert!(legacy_value.get("bound").unwrap().is_object());
        assert!(legacy_value.get("exact_out_bound").is_none());
    }

    #[tokio::test]
    async fn exact_out_publication_rechecks_expiry_view_and_preserves_quote_evidence() {
        let (api, id) = api_with_exact_out_quote().await;
        let cached = api.exact_out_cache.lock().unwrap()[&id].clone();
        let mut response = outcome("SUCCESS", true);
        response =
            json!({"quoteKind":"EXACT_OUT","verification":{"success":true},"outcome":response});
        let result = crate::publish_exact_out_simulation(&api, &id, &cached, response.clone())
            .unwrap()
            .0;
        assert_eq!(result["quoteId"], id);
        assert_eq!(result["planHash"], cached.plan.plan_hash);
        assert_eq!(result["verification"]["verified"], false);
        assert_eq!(
            api.exact_out_cache.lock().unwrap()[&id].response,
            cached.response
        );
        let evidence = api.exact_out_evidence(&id).unwrap();
        assert_eq!(evidence["quote"], cached.response);
        assert_eq!(evidence["simulation_evidence"], result);
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .expires = 0;
        assert_eq!(
            crate::publish_exact_out_simulation(&api, &id, &cached, response.clone())
                .unwrap_err()
                .1
                 .0["error"],
            "QUOTE_EXPIRED"
        );
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .expires = cached.expires;
        api.replica.suspend(&cached.plan.context.chain_key);
        assert_eq!(
            crate::publish_exact_out_simulation(&api, &id, &cached, response)
                .unwrap_err()
                .1
                 .0["error"],
            "SNAPSHOT_REORGED"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exact_out_http_cancellation_reaps_worker_and_releases_shared_admission() {
        use tower::ServiceExt;
        let (mut api, id) = api_with_exact_out_quote().await;
        let pid = pid_path();
        use_owned_cancellation_worker(&mut api, pid.clone());
        let request = axum::http::Request::builder()
            .method("POST")
            .uri(format!("/v1/quotes/{id}/simulate"))
            .header("authorization", format!("Bearer {}", api.config.token))
            .body(axum::body::Body::empty())
            .unwrap();
        let task = tokio::spawn(api.handler().oneshot(request));
        let deadline = Instant::now() + Duration::from_secs(8);
        while !pid.exists() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(pid.exists(), "ExactOut worker did not start");
        assert!(api.exact_out_cache.try_lock().is_ok());
        assert_eq!(
            api.local_simulation
                .as_ref()
                .unwrap()
                .permits
                .available_permits(),
            0
        );
        task.abort();
        let _ = task.await;
        let deadline = Instant::now() + Duration::from_secs(5);
        while api
            .local_simulation
            .as_ref()
            .unwrap()
            .permits
            .available_permits()
            == 0
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            api.local_simulation
                .as_ref()
                .unwrap()
                .permits
                .available_permits(),
            1
        );
        assert_reaped(&pid);
        assert!(api.exact_out_cache.lock().unwrap()[&id]
            .simulation_evidence
            .is_none());
    }

    #[tokio::test]
    async fn prepared_cache_replacement_blocks_late_evidence_and_reclaims_bounded_bytes() {
        let (api, id) = api_with_exact_out_quote().await;
        let cached = api.exact_out_cache.lock().unwrap()[&id].clone();
        let backend = api.local_simulation.as_ref().unwrap();
        let funding = xone_execution_preparation::evm::FundingObservation {
            snapshot_hash: hash(&cached.plan.snapshot).unwrap(),
            caller: cached.plan.context.caller.clone(),
            sell_token: cached.plan.context.sell_token.clone(),
            spender: backend.bundle.executor.target.clone(),
            pending_nonce: backend.bundle.transaction_context.nonce,
            native_balance: "1".parse().unwrap(),
            token_balance: "1".parse().unwrap(),
            allowance: "1".parse().unwrap(),
            base_fee_per_gas: backend
                .bundle
                .fixture
                .execution_environment
                .base_fee
                .clone(),
        };
        let tx = &backend.bundle.transaction_context;
        let input = WorkerPreparation {
            funding,
            fees: xone_execution_preparation::evm::FeeContext {
                chain_id: tx.chain_id,
                nonce: tx.nonce,
                gas_limit: tx.gas_limit,
                max_fee_per_gas: tx.gas_price.clone(),
                max_priority_fee_per_gas: Amount::zero(),
                max_network_fee: "1000000000000000000".parse().unwrap(),
            },
            component: json!({}),
        };
        let capacity = api.evm_preparation_budget.available_permits();
        let first = crate::evm_preparation::Prepared::retain(
            &api,
            "1".repeat(64),
            "a".repeat(64),
            input.clone(),
        )
        .unwrap();
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .evm_preparation = Some(first.clone());
        assert!(api.evm_preparation_budget.available_permits() < capacity);
        let late = json!({"apiPreparationHash":first.hash,"verification":{"success":true}});
        let second = crate::evm_preparation::Prepared::retain(
            &api,
            "2".repeat(64),
            "b".repeat(64),
            input.clone(),
        )
        .unwrap();
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .evm_preparation = Some(second);
        let error = crate::publish_exact_out_simulation(&api, &id, &cached, late).unwrap_err();
        assert_eq!(error.1 .0["error"], "EVM_PREPARATION_REPLACED");
        assert!(api.exact_out_cache.lock().unwrap()[&id]
            .simulation_evidence
            .is_none());
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .expires = 0;
        api.prune_cached_views(u64::MAX).unwrap();
        // The in-flight first handle still owns its reservation.
        assert!(api.evm_preparation_budget.available_permits() < capacity);
        drop(first);
        assert_eq!(api.evm_preparation_budget.available_permits(), capacity);
        let hold = api
            .evm_preparation_budget
            .clone()
            .try_acquire_many_owned(capacity as u32)
            .unwrap();
        let limited =
            crate::evm_preparation::Prepared::retain(&api, "3".repeat(64), "c".repeat(64), input);
        assert!(limited.is_err());
        drop(hold);
        assert_eq!(api.evm_preparation_budget.available_permits(), capacity);
    }

    #[test]
    fn prepared_worker_response_is_exclusive_and_its_outer_commitment_is_recomputed() {
        let mut prepared = json!({"preparation_hash":"component","unsigned_transaction_hash":"wire",
            "exact_out_preparation_hash":"outer-component","simulation":outcome("SUCCESS",true)});
        prepared["commitment_hash"] = hash(&prepared).unwrap().into();
        let response = json!({"protocol":WORKER_PROTOCOL,"worker_binary_sha256":"expected",
            "outcome":null,"prepared":prepared,"error":null});
        let decoded = decode_response(&serde_json::to_vec(&response).unwrap(), "expected").unwrap();
        assert_eq!(decoded.outcome["receipt_hash"], "receipt");
        assert_eq!(
            decoded.prepared.as_ref().unwrap().preparation_hash,
            "component"
        );
        for mode in 0..4 {
            let mut changed = response.clone();
            match mode {
                0 => changed["outcome"] = outcome("SUCCESS", true),
                1 => changed["prepared"]["simulation"]["actual_buy_amount"] = "2".into(),
                2 => changed["prepared"]["simulation"]["verified"] = true.into(),
                _ => changed["prepared"]["extra"] = true.into(),
            }
            assert!(decode_response(&serde_json::to_vec(&changed).unwrap(), "expected").is_err());
        }
    }

    #[tokio::test]
    async fn runtime_evm_independent_capability_requires_matching_bank_and_rpc_preparation() {
        use tower::ServiceExt;
        let (mut api, _) = api_with_exact_out_quote().await;
        let (_, _, rpc, _) = super::execution_source::fixture();
        let reference = api.replica.view(&rpc.chain_key).unwrap().reference.clone();
        let backend = api.local_simulation.as_ref().unwrap();
        assert!(backend.matches_snapshot(&reference, &rpc.genesis_hash));
        let mut wrong = reference.clone();
        wrong.block.hash = format!("0x{}", "ff".repeat(32));
        assert!(!backend.matches_snapshot(&wrong, &rpc.genesis_hash));
        assert!(!backend.matches_snapshot(&reference, &format!("0x{}", "ff".repeat(32))));
        let config = Arc::make_mut(&mut api.config);
        config.config.chains[0].rpc_urls = vec!["http://127.0.0.1:1".into()];
        config.config.api.evm_preparation.insert(
            rpc.chain_key.clone(),
            xone_config::EvmPreparationConfig {
                router: rpc.executor.target,
                router_code_sha256: rpc.executor.code_sha256,
                factory: rpc.executor.factory,
                factory_code_sha256: rpc.executor.factory_code_sha256,
                token_code_sha256: rpc.token_code_sha256,
                timeout_ms: rpc.timeout_ms,
                max_calls: rpc.max_calls,
                max_response_bytes: rpc.max_response_bytes,
            },
        );
        let request = || {
            axum::http::Request::builder()
                .uri("/v1/quote-options")
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(axum::body::Body::empty())
                .unwrap()
        };
        let response = api.handler().oneshot(request()).await.unwrap();
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value["chains"][0]["pools"][0]["execution"]["evmIndependentLocalSimulation"],
            true
        );
        api.local_simulation = None;
        let request = axum::http::Request::builder()
            .uri("/v1/quote-options")
            .header("authorization", format!("Bearer {}", api.config.token))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = api.handler().oneshot(request).await.unwrap();
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value["chains"][0]["pools"][0]["execution"]["evmIndependentLocalSimulation"],
            false
        );
    }

    #[test]
    #[ignore = "optional current-executable startup probe; actual worker runs in execution smoke"]
    fn worker_probe() {
        if let Ok(path) = std::env::var("XONE_PRIVATE_SIMULATION_PID_FILE") {
            std::fs::write(path, std::process::id().to_string()).unwrap();
        }
        match std::env::var("XONE_PRIVATE_SIMULATION_PROBE_MODE").as_deref() {
            Ok("stdout") => {
                let _ = std::io::stdout().write_all(&vec![b'x'; MAX_WORKER_OUTPUT_BYTES + 1]);
            }
            Ok("stderr") => {
                let _ = std::io::stderr().write_all(&vec![b'x'; MAX_STDERR + 1]);
            }
            Ok("exit") => std::process::exit(7),
            _ => (),
        }
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
