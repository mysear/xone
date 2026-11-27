//! Isolated local-bank SBF execution. No RPC, keys, signing or broadcasting.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};
use xone_domain::{err, hash, Pool, Result};
const WORKER: &str = include_str!("../workers/solana_sbf.py");
pub const PROTOCOL: &str = "xone-local-sbf/1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub schema_version: u32,
    pub chain_key: String,
    pub genesis_hash: String,
    pub python_executable: FileRef,
    pub runtime_directory: String,
    pub runtime_files: BTreeMap<String, String>,
    pub banks: Vec<FileRef>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rpc_capture: bool,
    pub max_compute_units: u64,
    pub timeout_ms: u64,
    pub concurrency: usize,
}
struct Bank {
    value: Value,
    pool: Pool,
    sha256: String,
}
pub struct Engine {
    manifest: RuntimeManifest,
    manifest_hash: String,
    python: PathBuf,
    runtime: Arc<tempfile::TempDir>,
    banks: Vec<Bank>,
    permits: Arc<Semaphore>,
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let f = std::fs::File::open(path).map_err(|_| err("SBF_FILE_UNREADABLE"))?;
    let meta = f.metadata().map_err(|_| err("SBF_FILE_UNREADABLE"))?;
    if !meta.is_file() || meta.len() > limit {
        return Err(err("SBF_FILE_LIMIT"));
    }
    let mut raw = Vec::new();
    f.take(limit + 1)
        .read_to_end(&mut raw)
        .map_err(|_| err("SBF_FILE_UNREADABLE"))?;
    if raw.len() as u64 > limit {
        return Err(err("SBF_FILE_LIMIT"));
    }
    Ok(raw)
}
fn resolve(root: &Path, path: &str) -> Result<PathBuf> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return Err(err("SBF_MANIFEST_PATH"));
    }
    let p = Path::new(path);
    Ok(if p.is_absolute() {
        p.into()
    } else {
        root.join(p)
    })
}
// The constructor thread owns at most two unused code images. Thread-local
// destruction releases them, while engines and active workers retain their own
// Arc. Only already verified Python code is shared; banks, permits, manifests
// and process execution remain per engine. Every load above still reads and
// hashes the source, and reuse revalidates every materialized file as well.
type RuntimeImage = Arc<tempfile::TempDir>;
thread_local! {
    static VERIFIED_RUNTIMES: RefCell<Vec<(String,RuntimeImage)>> = const { RefCell::new(Vec::new()) };
}
fn materialize_verified_runtime(
    source: &Path,
    expected: &BTreeMap<String, String>,
    files: Vec<(PathBuf, Vec<u8>)>,
) -> Result<RuntimeImage> {
    let source_name = source.to_str().ok_or_else(|| err("SBF_RUNTIME_PATH"))?;
    let key = hash(
        &json!({"source":source_name,"files":expected,"workerSha256":digest(WORKER.as_bytes())}),
    )?;
    let cached = VERIFIED_RUNTIMES.with(|images| {
        let mut images = images.borrow_mut();
        let index = images.iter().position(|entry| entry.0 == key)?;
        let entry = images.remove(index);
        let image = entry.1.clone();
        images.push(entry);
        Some(image)
    });
    if let Some(image) = cached {
        // Source bytes were just hashed against every configured digest.
        // Compare the retained image byte-for-byte with that validated input;
        // a second slow debug SHA pass would add constructor latency without
        // providing a stronger check than exact byte equality.
        if files.len() != expected.len() {
            return Err(err("SBF_RUNTIME_CHANGED"));
        }
        for (name, verified_bytes) in &files {
            if read(&image.path().join(name), verified_bytes.len() as u64)
                .map_err(|_| err("SBF_RUNTIME_CHANGED"))?
                != *verified_bytes
            {
                return Err(err("SBF_RUNTIME_CHANGED"));
            }
        }
        if read(&image.path().join("worker.py"), 1024 * 1024)
            .map_err(|_| err("SBF_RUNTIME_CHANGED"))?
            != WORKER.as_bytes()
        {
            return Err(err("SBF_RUNTIME_CHANGED"));
        }
        return Ok(image);
    }
    let image = Arc::new(
        tempfile::Builder::new()
            .prefix("xone-sbf-runtime-")
            .tempdir()
            .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?,
    );
    for (name, bytes) in files {
        let output = image.path().join(name);
        std::fs::create_dir_all(output.parent().ok_or_else(|| err("SBF_RUNTIME_PATH"))?)
            .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
        write_runtime_file(&output, &bytes)?;
    }
    write_runtime_file(&image.path().join("worker.py"), WORKER.as_bytes())?;
    VERIFIED_RUNTIMES.with(|images| {
        let mut images = images.borrow_mut();
        images.push((key, image.clone()));
        if images.len() > 2 {
            images.remove(0);
        }
    });
    Ok(image)
}
fn write_runtime_file(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400))
            .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
    }
    Ok(())
}
impl Engine {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = read(path, 1024 * 1024)?;
        let manifest: RuntimeManifest =
            serde_json::from_slice(&raw).map_err(|_| err("SBF_MANIFEST_INVALID"))?;
        if manifest.schema_version != 1
            || !manifest.chain_key.starts_with("solana:")
            || manifest.genesis_hash.is_empty()
            || !(1..=1_400_000).contains(&manifest.max_compute_units)
            || !(100..=15000).contains(&manifest.timeout_ms)
            || !(1..=8).contains(&manifest.concurrency)
            || manifest.banks.len() > 16
            || (manifest.banks.is_empty() && !manifest.rpc_capture)
            || manifest.runtime_files.len() > 128
            || !manifest.runtime_files.contains_key("solders/__init__.py")
            || !manifest.runtime_files.contains_key("typing_extensions.py")
        {
            return Err(err("SBF_MANIFEST_INVALID"));
        }
        xone_domain::validate_chain(&manifest.chain_key)?;
        let root = path.parent().ok_or_else(|| err("SBF_MANIFEST_PATH"))?;
        let python = resolve(root, &manifest.python_executable.path)?
            .canonicalize()
            .map_err(|_| err("SBF_PYTHON_UNAVAILABLE"))?;
        if digest(&read(&python, 16 * 1024 * 1024)?) != manifest.python_executable.sha256 {
            return Err(err("SBF_PYTHON_CHANGED"));
        }
        let source = resolve(root, &manifest.runtime_directory)?
            .canonicalize()
            .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
        let mut verified_files = Vec::new();
        let mut total = 0u64;
        for (name, expected) in &manifest.runtime_files {
            let relative = Path::new(name);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || !(name.starts_with("solders/")
                    || name == "typing_extensions.py"
                    || name.starts_with("jsonalias/"))
                || !matches!(
                    relative.extension().and_then(|s| s.to_str()),
                    Some("py" | "pyi" | "so" | "pyd")
                )
            {
                return Err(err("SBF_RUNTIME_PATH"));
            }
            let input = source
                .join(relative)
                .canonicalize()
                .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
            if !input.starts_with(&source) {
                return Err(err("SBF_RUNTIME_PATH"));
            }
            let bytes = read(&input, 80 * 1024 * 1024)?;
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| err("SBF_RUNTIME_LIMIT"))?;
            if total > 96 * 1024 * 1024 {
                return Err(err("SBF_RUNTIME_LIMIT"));
            }
            if digest(&bytes) != *expected {
                return Err(err("SBF_RUNTIME_CHANGED"));
            }
            verified_files.push((relative.to_path_buf(), bytes));
        }
        let runtime =
            materialize_verified_runtime(&source, &manifest.runtime_files, verified_files)?;
        let mut banks = Vec::new();
        let mut size = 0u64;
        for file in &manifest.banks {
            let bytes = read(&resolve(root, &file.path)?, 16 * 1024 * 1024)?;
            size += bytes.len() as u64;
            if size > 64 * 1024 * 1024 {
                return Err(err("SBF_BANK_LIMIT"));
            }
            if digest(&bytes) != file.sha256 {
                return Err(err("SBF_BANK_CHANGED"));
            }
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|_| err("SBF_BANK_INVALID"))?;
            let pool: Pool = serde_json::from_value(value["pool"].clone())
                .map_err(|_| err("SBF_BANK_INVALID"))?;
            pool.validate()?;
            if pool.chain_key != manifest.chain_key
                || !(xone_domain::meteora::supports_profile(&pool.protocol)
                    || xone_domain::raydium_clmm::supports_classic_profile(&pool.protocol)
                    || xone_domain::orca::supports_swap_profile(&pool.protocol)
                    || pool.protocol == xone_domain::raydium::PROFILE
                    || xone_domain::pump_amm::supports_profile(&pool.protocol)
                    || pool.protocol == xone_domain::pump::PROFILE)
                || value["accounts"].as_object().is_none_or(|m| m.len() > 128)
            {
                return Err(err("SBF_BANK_INVALID"));
            }
            banks.push(Bank {
                pool,
                value,
                sha256: file.sha256.clone(),
            });
        }
        let permits = Arc::new(Semaphore::new(manifest.concurrency));
        Ok(Self {
            manifest,
            manifest_hash: digest(&raw),
            python,
            runtime,
            banks,
            permits,
        })
    }
    pub fn available(&self, pool: &Pool, genesis: &str) -> bool {
        self.manifest.chain_key == pool.chain_key
            && self.manifest.genesis_hash == genesis
            && (!(xone_domain::orca::supports_swap_profile(&pool.protocol)
                || xone_domain::raydium_clmm::supports_classic_profile(&pool.protocol))
                || self.manifest.max_compute_units >= 1_400_000)
            && (self.banks.iter().any(|b| b.pool == *pool)
                || (self.manifest.rpc_capture
                    && (xone_domain::meteora::supports_profile(&pool.protocol)
                        || xone_domain::raydium_clmm::supports_classic_profile(&pool.protocol)
                        || xone_domain::orca::supports_swap_profile(&pool.protocol)
                        || pool.protocol == xone_domain::raydium::PROFILE
                        || xone_domain::pump_amm::supports_profile(&pool.protocol)
                        || pool.protocol == xone_domain::pump::PROFILE)))
    }
    pub fn uses_rpc_capture(&self) -> bool {
        self.manifest.rpc_capture
    }
    pub async fn simulate(
        &self,
        pool: &Pool,
        caller: &str,
        recipient: &str,
        sell: &str,
        buy: &str,
        preparation: &Value,
    ) -> Result<Value> {
        self.simulate_inner(pool, caller, recipient, sell, buy, preparation, None)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn simulate_captured(
        &self,
        pool: &Pool,
        caller: &str,
        recipient: &str,
        sell: &str,
        buy: &str,
        preparation: &Value,
        capture: &Value,
    ) -> Result<Value> {
        if !self.manifest.rpc_capture {
            return Err(err("SBF_CAPTURE_NOT_ENABLED"));
        }
        self.simulate_inner(
            pool,
            caller,
            recipient,
            sell,
            buy,
            preparation,
            Some(capture),
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn simulate_inner(
        &self,
        pool: &Pool,
        caller: &str,
        recipient: &str,
        sell: &str,
        buy: &str,
        preparation: &Value,
        capture: Option<&Value>,
    ) -> Result<Value> {
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| err("SBF_SIMULATION_BUSY"))?;
        if digest(&read(&self.python, 16 * 1024 * 1024)?) != self.manifest.python_executable.sha256
        {
            return Err(err("SBF_PYTHON_CHANGED"));
        }
        let mut commitment = preparation.clone();
        commitment
            .as_object_mut()
            .ok_or_else(|| err("SBF_PREPARATION_INVALID"))?
            .remove("preparationHash");
        if preparation["preparationHash"].as_str() != Some(hash(&commitment)?.as_str())
            || preparation["ready"] != true
            || preparation["signed"] != false
            || preparation["submitted"] != false
        {
            return Err(err("SBF_PREPARATION_INVALID"));
        }
        let claim = preparation["operation"] == "ORCA_COLLECT_REWARD";
        if claim
            && (capture.is_none()
                || preparation["authority"] != caller
                || preparation["recipient"] != recipient)
        {
            return Err(err("SBF_PREPARATION_INVALID"));
        }
        let source = preparation[if claim {
            "rewardVault"
        } else {
            "sourceTokenAccount"
        }]
        .as_str()
        .ok_or_else(|| err("SBF_PREPARATION_INVALID"))?;
        let destination = preparation[if claim {
            "rewardAccount"
        } else {
            "destinationTokenAccount"
        }]
        .as_str()
        .ok_or_else(|| err("SBF_PREPARATION_INVALID"))?;
        let captured_bank;
        let bank = if let Some(snapshot) = capture {
            pool.validate()?;
            let slot = pool
                .meteora
                .as_ref()
                .map(|m| m.slot)
                .or_else(|| pool.raydium_cpmm.as_ref().map(|r| r.bank_slot))
                .or_else(|| pool.orca_whirlpool.as_ref().map(|r| r.bank_slot))
                .or_else(|| pool.raydium_clmm.as_ref().map(|r| r.bank_slot))
                .or_else(|| pool.pump.as_ref().map(|r| r.slot))
                .ok_or_else(|| err("SBF_BANK_INVALID"))?;
            let expected_hash = hash(snapshot)?;
            if snapshot["schemaVersion"] != 1
                || snapshot["chainKey"] != pool.chain_key
                || snapshot["chainKey"] != self.manifest.chain_key
                || snapshot["genesisHash"] != self.manifest.genesis_hash
                || snapshot["observedSlot"].as_u64() != Some(slot)
                || preparation["observedSlot"] != snapshot["observedSlot"]
                || preparation["accountSnapshot"]["snapshotHash"].as_str()
                    != Some(expected_hash.as_str())
                || preparation["accountSnapshot"]["observedSlot"] != snapshot["observedSlot"]
            {
                return Err(err("SBF_CAPTURE_BINDING_INVALID"));
            }
            let captured = snapshot["accounts"]
                .as_object()
                .filter(|a| !a.is_empty() && a.len() <= 100)
                .ok_or_else(|| err("SBF_BANK_INVALID"))?;
            let mut accounts = serde_json::Map::new();
            let mut absent = Vec::new();
            for (key, value) in captured {
                // System program is the all-zero public key and is valid here.
                if key != "11111111111111111111111111111111" {
                    xone_domain::raydium::validate_pubkey(key)?;
                }
                if value.is_null() {
                    absent.push(key.clone());
                } else {
                    accounts.insert(key.clone(), value.clone());
                }
            }
            captured_bank = Bank {
                pool: pool.clone(),
                sha256: expected_hash,
                value: json!({"pool":pool,"accounts":accounts,"absentAccounts":absent,"rpcCapture":true,"genesis":self.manifest.genesis_hash,"observedSlot":slot}),
            };
            &captured_bank
        } else {
            let candidates = self
                .banks
                .iter()
                .filter(|b| {
                    b.pool == *pool
                        && b.value["accounts"].get(caller).is_some()
                        && (preparation["sourceAccountCreation"] == true
                            || b.value["accounts"].get(source).is_some())
                        && (preparation["accountCreation"] == true
                            || preparation["unwrapDestinationSol"] == true
                            || b.value["accounts"].get(destination).is_some())
                })
                .collect::<Vec<_>>();
            let bank = candidates
                .first()
                .copied()
                .ok_or_else(|| err("SBF_BANK_NOT_COVERED"))?;
            if candidates
                .iter()
                .skip(1)
                .any(|other| other.sha256 != bank.sha256)
            {
                return Err(err("SBF_BANK_AMBIGUOUS"));
            }
            bank
        };
        let request = json!({"protocol":PROTOCOL,"bank":bank.value,"preparation":preparation,"caller":caller,"recipient":recipient,"sellToken":sell,"buyToken":buy,"maxComputeUnits":self.manifest.max_compute_units});
        let bytes = serde_json::to_vec(&request).map_err(|_| err("SBF_PREPARATION_INVALID"))?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(err("SBF_WORKER_INPUT_LIMIT"));
        }
        let mut child = Command::new(&self.python)
            .args(["-I", "-S", "-B"])
            .arg(self.runtime.path().join("worker.py"))
            .arg(self.runtime.path())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| err("SBF_WORKER_START_FAILED"))?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| err("SBF_WORKER_START_FAILED"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| err("SBF_WORKER_START_FAILED"))?;
        let errors = child
            .stderr
            .take()
            .ok_or_else(|| err("SBF_WORKER_START_FAILED"))?;
        let input_progress = AtomicUsize::new(0);
        let output_progress = AtomicUsize::new(0);
        let error_progress = AtomicUsize::new(0);
        let started = Instant::now();
        let operation = async {
            let send = async {
                for chunk in bytes.chunks(8192) {
                    input
                        .write_all(chunk)
                        .await
                        .map_err(|_| err("SBF_WORKER_IO"))?;
                    input_progress.fetch_add(chunk.len(), Ordering::Relaxed);
                }
                drop(input);
                Ok::<(), xone_domain::Error>(())
            };
            let (_, output, errors) = tokio::try_join!(
                send,
                limited_with_progress(output, 512 * 1024, &output_progress),
                limited_with_progress(errors, 64 * 1024, &error_progress)
            )?;
            let status = child.wait().await.map_err(|_| err("SBF_WORKER_IO"))?;
            if !status.success() {
                let _ = errors;
                return Err(err("SBF_WORKER_FAILED"));
            }
            serde_json::from_slice::<Value>(&output).map_err(|_| err("SBF_WORKER_RESPONSE"))
        };
        let value =
            tokio::time::timeout(Duration::from_millis(self.manifest.timeout_ms), operation).await;
        let mut result = match value {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                let exited = child.try_wait().ok().flatten().is_some();
                eprintln!(
                    "{}",
                    worker_failure_diagnostic(
                        &error.0,
                        started.elapsed(),
                        bytes.len(),
                        input_progress.load(Ordering::Relaxed),
                        output_progress.load(Ordering::Relaxed),
                        error_progress.load(Ordering::Relaxed),
                        exited
                    )
                );
                let _ = child.kill().await;
                return Err(error);
            }
            Err(_) => {
                let exited = child.try_wait().ok().flatten().is_some();
                eprintln!(
                    "{}",
                    worker_failure_diagnostic(
                        "SBF_SIMULATION_TIMEOUT",
                        started.elapsed(),
                        bytes.len(),
                        input_progress.load(Ordering::Relaxed),
                        output_progress.load(Ordering::Relaxed),
                        error_progress.load(Ordering::Relaxed),
                        exited
                    )
                );
                let _ = child.kill().await;
                return Err(err("SBF_SIMULATION_TIMEOUT"));
            }
        };
        if result["protocol"] != PROTOCOL
            || result["success"] != true
            || result["sourceBankUnchanged"] != true
        {
            return Err(err(result["error"]
                .as_str()
                .unwrap_or("SBF_WORKER_RESPONSE")));
        }
        let quantities_match = if claim {
            [
                "grossReward",
                "netReward",
                "remainingRewardOwed",
                "transferFee",
            ]
            .iter()
            .all(|key| result[*key] == preparation[*key])
        } else {
            result["actualInput"] == preparation["quotedInput"]
                && result["actualOutput"] == preparation["quotedOutput"]
        };
        if !quantities_match {
            return Err(err("SBF_AMOUNT_MISMATCH"));
        }
        result["level"] = if capture.is_some() {
            "RPC_CAPTURE_SBF_SIMULATED"
        } else {
            "LOCAL_SBF_SIMULATED"
        }
        .into();
        result["accountSource"] = if capture.is_some() {
            "PREPARATION_RPC_CAPTURE"
        } else {
            "PINNED_LOCAL_BANK"
        }
        .into();
        result["historicalBankVerified"] = false.into();
        result["bankSha256"] = bank.sha256.clone().into();
        result["runtimeManifestSha256"] = self.manifest_hash.clone().into();
        result["workerSha256"] = digest(WORKER.as_bytes()).into();
        result["preparationHash"] = preparation["preparationHash"].clone();
        result["planHash"] = preparation["planHash"].clone();
        result["quoteViewId"] = preparation["quoteViewId"].clone();
        result["evidenceHash"] = hash(&result)?.into();
        Ok(result)
    }
}
fn worker_failure_diagnostic(
    error: &str,
    elapsed: Duration,
    request_bytes: usize,
    input_bytes: usize,
    output_bytes: usize,
    stderr_bytes: usize,
    child_exited: bool,
) -> Value {
    json!({"event":"SBF_WORKER_FAILURE","error":error,"elapsedMs":elapsed.as_millis().min(u128::from(u64::MAX)) as u64,"requestBytes":request_bytes,"inputWrittenBytes":input_bytes,"outputReadBytes":output_bytes,"stderrReadBytes":stderr_bytes,"childExited":child_exited})
}
async fn limited_with_progress(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
    progress: &AtomicUsize,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|_| err("SBF_WORKER_IO"))?;
        if count == 0 {
            return Ok(output);
        }
        progress.fetch_add(count, Ordering::Relaxed);
        if output.len() + count > limit {
            return Err(err("SBF_WORKER_OUTPUT_LIMIT"));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

#[cfg(test)]
mod worker_io_tests {
    use super::*;
    #[tokio::test]
    async fn bounded_output_keeps_the_limit_and_counts_observed_bytes() {
        let progress = AtomicUsize::new(0);
        assert_eq!(
            limited_with_progress(&b"original"[..], 8, &progress)
                .await
                .unwrap(),
            b"original"
        );
        assert_eq!(progress.load(Ordering::Relaxed), 8);
        let progress = AtomicUsize::new(0);
        assert_eq!(
            limited_with_progress(&b"oversized"[..], 8, &progress)
                .await
                .unwrap_err()
                .0,
            "SBF_WORKER_OUTPUT_LIMIT"
        );
        assert_eq!(progress.load(Ordering::Relaxed), 9);
    }
    #[test]
    fn failure_diagnostics_contain_counts_not_account_or_transaction_payloads() {
        let result = worker_failure_diagnostic(
            "SBF_SIMULATION_TIMEOUT",
            Duration::from_millis(15001),
            1024,
            512,
            0,
            0,
            false,
        );
        assert_eq!(
            result,
            json!({"event":"SBF_WORKER_FAILURE","error":"SBF_SIMULATION_TIMEOUT","elapsedMs":15001,"requestBytes":1024,"inputWrittenBytes":512,"outputReadBytes":0,"stderrReadBytes":0,"childExited":false})
        );
        assert_eq!(result.as_object().unwrap().len(), 8);
    }
    fn controlled_manifest() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        std::fs::create_dir_all(source.join("solders")).unwrap();
        let executable = directory.path().join("python");
        std::fs::write(&executable, b"controlled-executable-identity").unwrap();
        let mut files = BTreeMap::new();
        for (name, data) in [
            ("solders/__init__.py", b"__version__='0.29.0'".as_slice()),
            ("typing_extensions.py", b"# controlled runtime".as_slice()),
        ] {
            std::fs::write(source.join(name), data).unwrap();
            files.insert(name.into(), digest(data));
        }
        let manifest = RuntimeManifest {
            schema_version: 1,
            chain_key: "solana:localnet".into(),
            genesis_hash: "controlled-genesis".into(),
            python_executable: FileRef {
                path: executable.to_str().unwrap().into(),
                sha256: digest(b"controlled-executable-identity"),
            },
            runtime_directory: source.to_str().unwrap().into(),
            runtime_files: files,
            banks: Vec::new(),
            rpc_capture: true,
            max_compute_units: 1_400_000,
            timeout_ms: 15000,
            concurrency: 1,
        };
        let path = directory.path().join("runtime.json");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        (directory, path)
    }
    #[test]
    fn verified_runtime_code_is_reused_but_engine_limits_remain_distinct() {
        let (_directory, path) = controlled_manifest();
        let first = Engine::load(&path).unwrap();
        let mut manifest: RuntimeManifest =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest.max_compute_units = 200_000;
        manifest.timeout_ms = 100;
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let second = Engine::load(&path).unwrap();
        assert!(Arc::ptr_eq(&first.runtime, &second.runtime));
        assert_ne!(first.manifest_hash, second.manifest_hash);
        assert_eq!(first.manifest.max_compute_units, 1_400_000);
        assert_eq!(second.manifest.max_compute_units, 200_000);
        assert_eq!(first.manifest.timeout_ms, 15000);
        assert_eq!(second.manifest.timeout_ms, 100);
        assert!(!Arc::ptr_eq(&first.permits, &second.permits));
    }
    #[test]
    fn cached_code_does_not_bypass_source_revalidation_or_survive_tampering() {
        let (directory, path) = controlled_manifest();
        let first = Engine::load(&path).unwrap();
        std::fs::write(
            directory.path().join("source/typing_extensions.py"),
            b"changed-source",
        )
        .unwrap();
        assert_eq!(Engine::load(&path).err().unwrap().0, "SBF_RUNTIME_CHANGED");
        std::fs::write(
            directory.path().join("source/typing_extensions.py"),
            b"# controlled runtime",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let worker = first.runtime.path().join("worker.py");
            assert_eq!(
                std::fs::metadata(&worker).unwrap().permissions().mode() & 0o222,
                0
            );
            std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o600)).unwrap();
            std::fs::write(worker, b"changed worker").unwrap();
            assert_eq!(Engine::load(&path).err().unwrap().0, "SBF_RUNTIME_CHANGED");
        }
    }
    #[test]
    fn verified_code_cache_is_bounded_and_eviction_does_not_drop_active_engines() {
        VERIFIED_RUNTIMES.with(|cache| cache.borrow_mut().clear());
        let (_a, pa) = controlled_manifest();
        let first = Engine::load(&pa).unwrap();
        let first_image = first.runtime.path().to_path_buf();
        let (_b, pb) = controlled_manifest();
        let second = Engine::load(&pb).unwrap();
        let (_c, pc) = controlled_manifest();
        let third = Engine::load(&pc).unwrap();
        VERIFIED_RUNTIMES.with(|cache| assert_eq!(cache.borrow().len(), 2));
        assert!(first_image.join("worker.py").is_file());
        assert!(!Arc::ptr_eq(&first.runtime, &second.runtime));
        assert!(!Arc::ptr_eq(&second.runtime, &third.runtime));
        drop(first);
        assert!(!first_image.exists());
    }
}
