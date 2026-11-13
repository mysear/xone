//! Isolated local-bank SBF execution. No RPC, keys, signing or broadcasting.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
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
            || !(1..=16).contains(&manifest.banks.len())
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
        let runtime = Arc::new(
            tempfile::Builder::new()
                .prefix("xone-sbf-runtime-")
                .tempdir()
                .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?,
        );
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
            let output = runtime.path().join(relative);
            std::fs::create_dir_all(output.parent().ok_or_else(|| err("SBF_RUNTIME_PATH"))?)
                .map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
            std::fs::write(output, bytes).map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
        }
        let mut banks = Vec::new();
        let mut size = 0u64;
        for file in &manifest.banks {
            let bytes = read(&resolve(root, &file.path)?, 8 * 1024 * 1024)?;
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
                    || pool.protocol == xone_domain::raydium::PROFILE)
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
        let worker = runtime.path().join("worker.py");
        std::fs::write(&worker, WORKER).map_err(|_| err("SBF_RUNTIME_UNAVAILABLE"))?;
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
            && self.banks.iter().any(|b| b.pool == *pool)
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
        let source = preparation["sourceTokenAccount"]
            .as_str()
            .ok_or_else(|| err("SBF_PREPARATION_INVALID"))?;
        let destination = preparation["destinationTokenAccount"]
            .as_str()
            .ok_or_else(|| err("SBF_PREPARATION_INVALID"))?;
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
            .ok_or_else(|| err("SBF_BANK_NOT_COVERED"))?;
        if candidates
            .iter()
            .skip(1)
            .any(|other| other.sha256 != bank.sha256)
        {
            return Err(err("SBF_BANK_AMBIGUOUS"));
        }
        let request = json!({"protocol":PROTOCOL,"bank":bank.value,"preparation":preparation,"caller":caller,"recipient":recipient,"sellToken":sell,"buyToken":buy,"maxComputeUnits":self.manifest.max_compute_units});
        let bytes = serde_json::to_vec(&request).map_err(|_| err("SBF_PREPARATION_INVALID"))?;
        if bytes.len() > 12 * 1024 * 1024 {
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
        let operation = async {
            let send = async {
                input
                    .write_all(&bytes)
                    .await
                    .map_err(|_| err("SBF_WORKER_IO"))?;
                drop(input);
                Ok::<(), xone_domain::Error>(())
            };
            let (_, output, errors) = tokio::try_join!(
                send,
                limited(output, 512 * 1024),
                limited(errors, 64 * 1024)
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
                let _ = child.kill().await;
                return Err(error);
            }
            Err(_) => {
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
        if result["actualInput"] != preparation["quotedInput"]
            || result["actualOutput"] != preparation["quotedOutput"]
        {
            return Err(err("SBF_AMOUNT_MISMATCH"));
        }
        result["level"] = "LOCAL_SBF_SIMULATED".into();
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
async fn limited(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
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
        if output.len() + count > limit {
            return Err(err("SBF_WORKER_OUTPUT_LIMIT"));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}
