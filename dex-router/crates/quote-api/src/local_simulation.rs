//! Configured local-state diagnostics, isolated in the current executable's worker mode.
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
    evm_fixture::{SimulationBudget, TransactionContext},
    revm_backend::StateImage,
    worker::{
        executable_sha256, LocalSimulationBundle, WorkerRequest, WorkerResponse,
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
    transaction: Transaction,
    transaction_context: TransactionContext,
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
        Ok(Self {
            input: output.0,
            commitment: ExecutionCommitment {
                binding_hash: request.bound.binding_hash,
                transaction: request.bound.transaction,
                transaction_context: request.transaction_context,
                executor: request.bound.executor,
            },
        })
    }
}

impl LocalSimulation {
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
        cached: Cached,
        deadline: Instant,
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
                let bound = xone_execution_plan::bind_v2_execution(
                    &cached.plan,
                    &cached.view.pools,
                    &state.bundle.executor,
                    &config.config.simulation.allowed_targets,
                )
                .map_err(|e| Failure::new(&e.0, "binding"))?;
                let genesis = config
                    .config
                    .chains
                    .iter()
                    .find(|c| c.key == cached.plan.context.chain_key)
                    .ok_or_else(|| Failure::new("SIMULATION_STATE_CHAIN_MISMATCH", "anchor"))?
                    .genesis_hash
                    .clone();
                let anchor = &state.bundle.fixture.state_anchor;
                let block = &cached.plan.snapshot.block;
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
                    pools: cached.view.pools.clone(),
                    allowed_targets: config.config.simulation.allowed_targets.clone(),
                    fixture: state.bundle.fixture.clone(),
                    transaction_context: state.bundle.transaction_context.clone(),
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
            diagnostic_response(
                outcome,
                &commitment,
                &self.bundle_sha256,
                &self.worker_sha256,
            )
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
    let gas = outcome["execution"]["gas_used"]
        .as_u64()
        .ok_or_else(|| Failure::new("SIMULATION_WORKER_PROTOCOL", "receipt"))?;
    let gas_amount: Amount = gas
        .to_string()
        .parse()
        .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "receipt"))?;
    let gas_price = &commitment.transaction_context.gas_price;
    let fee = Amount::checked(&gas_amount.0 * &gas_price.0)
        .map_err(|_| Failure::new("SIMULATION_FEE_OVERFLOW", "receipt"))?;
    let success = outcome["matches_plan"] == true && outcome["execution"]["status"] == "SUCCESS";
    Ok(
        json!({"bindingHash":commitment.binding_hash,"bundleHash":bundle_hash,"workerHash":worker_hash,
        "transaction":commitment.transaction,"transactionContext":commitment.transaction_context,"executor":commitment.executor,
        "verification":{"level":"LOCAL_EVM_EXECUTED","success":success,"verified":false},"verified":false,
        "authority":"PROVIDED_LOCAL_STATE","outcome":outcome,
        "fees":{"evmGasUsed":gas,"gasPriceWei":gas_price,"evmFeeWei":fee,"chainFeeCompleteness":"LOCAL_EVM_ONLY"}}),
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
fn decode_response(bytes: &[u8], worker_hash: &str) -> std::result::Result<Value, Failure> {
    let response: WorkerResponse = serde_json::from_slice(bytes)
        .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response"))?;
    if response.protocol != WORKER_PROTOCOL || response.worker_binary_sha256 != worker_hash {
        return Err(Failure::new("SIMULATION_WORKER_IDENTITY", "response"));
    }
    match (response.outcome, response.error) {
        (Some(outcome), None) => serde_json::to_value(outcome)
            .map_err(|_| Failure::new("SIMULATION_WORKER_PROTOCOL", "response")),
        (None, Some(error)) => Err(Failure {
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn supervisor_hard_deadline_reaps_child() {
        // Measure termination/reap, not macOS validation of a large newly linked
        // test executable. The other supervisor/HTTP tests use the real private
        // current-executable probe. This tiny child reports the exact owned PID.
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
            ("stdout", "SIMULATION_WORKER_OUTPUT_LIMIT"),
            ("stderr", "SIMULATION_WORKER_STDERR_LIMIT"),
            ("exit", "SIMULATION_WORKER_EXIT"),
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
            mode: "sleep".into(),
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
                chain_id: 31337,
            },
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
            bound,
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
        Arc::get_mut(api.local_simulation.as_mut().unwrap())
            .unwrap()
            .test_launch = Some(TestLaunch {
            mode: "sleep".into(),
            pid_file: Some(pid.clone()),
        });
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

    #[test]
    #[ignore = "child-process probe, launched only by supervisor tests"]
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
