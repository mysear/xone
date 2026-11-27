//! Historical replay of PROVIDED_LOCAL_STATE, never a canonicality or VERIFIED claim.
//! Source H and execution H+1 are distinct. Probes discard their state; only swap effects commit.
mod approval;
mod singleton;
mod wrap;
use crate::revm_backend as core;
use revm::primitives::{
    keccak256, AccountInfo, Address, BlobExcessGasAndPrice, BlockEnv, Bytes, ExecutionResult,
    SpecId, TxEnv, TxKind, B256, U256,
};
use revm::Database;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub(crate) use singleton::simulate_singleton_transaction;
pub use singleton::BEACON_ROOTS_ADDRESS;
pub use singleton::{simulate_singleton, SingletonPlan, SINGLETON_SIMULATOR_VERSION};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{atomic::Ordering, Arc},
    time::Instant,
};
pub use wrap::{WrapReceipt, WrapSequence};
use xone_domain::{Amount, Pool};
use xone_execution_plan::ordered_v2::{self, BoundSplit, SplitExecutor, SplitPlan};
use xone_execution_plan::{
    validate_bound_exact_out_execution, validate_bound_execution, BoundEvmExactOutExecution,
    BoundEvmExecution, Context, ExecutorIdentity, Transaction,
};
pub const SIMULATOR_VERSION: &str = "provided-local-state/shanghai-v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateAnchor {
    pub chain_key: String,
    pub block_number: u64,
    pub block_hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub genesis_hash: String,
    /// Explicit source-H base fee, separate from the H+1 execution environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_fee_per_gas: Option<Amount>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEnvironment {
    pub chain_id: u64,
    pub fork: String,
    pub block_number: u64,
    pub block_hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub coinbase: String,
    pub gas_limit: u64,
    pub base_fee: Amount,
    pub prevrandao: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excess_blob_gas: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_beacon_block_root: Option<String>,
}
impl ExecutionEnvironment {
    pub fn beacon_root_slots(&self) -> Result<Option<[u64; 2]>> {
        let Some(root) = self.parent_beacon_block_root.as_deref() else {
            return Ok(None);
        };
        if self.fork != "cancun" {
            return Err(failure(
                SimulationEnvironmentMismatch,
                "environment",
                "parent beacon block root requires Cancun",
            ));
        }
        hex_length(root, Some(32))?;
        let index = self.timestamp % 8191;
        Ok(Some([index, index + 8191]))
    }

    fn cancun_blobs(&self) -> Result<Option<BlobExcessGasAndPrice>> {
        match (self.fork.as_str(), self.excess_blob_gas) {
            ("cancun", Some(excess)) => core::execute::cancun_blob_context(excess)
                .map(Some)
                .ok_or_else(|| {
                    failure(
                        SimulationEnvironmentMismatch,
                        "environment",
                        "invalid excess blob gas",
                    )
                }),
            ("cancun", None) => Err(failure(
                SimulationEnvironmentMismatch,
                "environment",
                "missing Cancun excess blob gas",
            )),
            (_, None) => Ok(None),
            (_, Some(_)) => Err(failure(
                SimulationEnvironmentMismatch,
                "environment",
                "unexpected excess blob gas",
            )),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEntry {
    pub slot: String,
    pub value: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureAccount {
    pub address: String,
    pub exists: bool,
    pub balance: Amount,
    pub nonce: u64,
    pub code: String,
    pub storage: Vec<StorageEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureBlockHash {
    pub number: u64,
    pub hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureState {
    pub schema_version: u32,
    pub state_anchor: StateAnchor,
    pub execution_environment: ExecutionEnvironment,
    pub accounts: Vec<FixtureAccount>,
    pub block_hashes: Vec<FixtureBlockHash>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionContext {
    pub nonce: u64,
    pub gas_limit: u64,
    /// Legacy gas price, or the EIP-1559 maximum when a priority fee is present.
    pub gas_price: Amount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_priority_fee_per_gas: Option<Amount>,
    pub chain_id: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationBudget {
    pub max_fixture_json_bytes: usize,
    pub max_accounts: usize,
    pub max_storage_slots: usize,
    pub max_code_bytes: usize,
    pub max_image_bytes: usize,
    pub max_reads: u64,
    pub max_read_bytes: u64,
    pub max_instructions: u64,
    pub max_input_bytes: usize,
    pub max_total_input_bytes: u64,
    pub max_transaction_gas: u64,
    pub probe_gas_limit: u64,
    pub max_total_gas: u64,
    pub max_memory_bytes: u64,
    pub max_output_bytes: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetUsage {
    pub reads: u64,
    pub read_bytes: u64,
    pub instructions: u64,
    pub input_bytes: u64,
    pub gas_used: u64,
    pub executions: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
// Preserve the machine-readable SIMULATION_ namespace in the public error variants.
#[allow(clippy::enum_variant_names)]
pub enum SimulationErrorCode {
    SimulationInvalidFixture,
    SimulationFixtureLimit,
    SimulationDuplicateAccount,
    SimulationDuplicateStorage,
    SimulationDuplicateBlockHash,
    SimulationAnchorMismatch,
    SimulationEnvironmentMismatch,
    SimulationBindingMismatch,
    SimulationCodeMismatch,
    SimulationObservationUnavailable,
    SimulationObservationMismatch,
    SimulationBalanceDirection,
    SimulationBudgetExceeded,
    SimulationCancelled,
    SimulationDeadline,
    SimulationExecutionUnavailable,
    SimulationTransactionContext,
    SimulationOutputLimit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationError {
    pub code: SimulationErrorCode,
    pub phase: String,
    pub detail: String,
    pub usage: BudgetUsage,
}
impl std::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}/{}: {}", self.code, self.phase, self.detail)
    }
}
impl std::error::Error for SimulationError {}
pub type Result<T> = std::result::Result<T, SimulationError>;
use SimulationErrorCode::*;
fn failure(code: SimulationErrorCode, phase: &str, detail: impl ToString) -> SimulationError {
    SimulationError {
        code,
        phase: phase.into(),
        detail: detail.to_string(),
        usage: BudgetUsage::default(),
    }
}
/// Production JSON is strict; callers reading native case envelopes must extract fields explicitly.
pub fn parse_fixture_json(bytes: &[u8], budget: &SimulationBudget) -> Result<FixtureState> {
    if bytes.len() > budget.max_fixture_json_bytes {
        return Err(failure(
            SimulationFixtureLimit,
            "decode",
            "fixture JSON bytes",
        ));
    }
    let fixture: FixtureState = serde_json::from_slice(bytes)
        .map_err(|e| failure(SimulationInvalidFixture, "decode", e))?;
    fixture.validate_shape(budget)?;
    Ok(fixture)
}
fn hex_length(value: &str, width: Option<usize>) -> Result<usize> {
    let raw = value
        .strip_prefix("0x")
        .ok_or_else(|| failure(SimulationInvalidFixture, "hex", "0x prefix required"))?;
    if raw.len() % 2 != 0
        || width.is_some_and(|n| raw.len() != n * 2)
        || !raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(failure(
            SimulationInvalidFixture,
            "hex",
            "canonical lowercase hex and exact width required",
        ));
    }
    Ok(raw.len() / 2)
}
fn fixed<const N: usize>(value: &str) -> Result<[u8; N]> {
    hex_length(value, Some(N))?;
    let mut out = [0; N];
    hex::decode_to_slice(&value[2..], &mut out)
        .map_err(|e| failure(SimulationInvalidFixture, "hex", e))?;
    Ok(out)
}
fn address(value: &str) -> Result<Address> {
    Ok(Address::from(fixed::<20>(value)?))
}
fn word(value: &str) -> Result<U256> {
    Ok(U256::from_be_bytes(fixed::<32>(value)?))
}
fn amount(value: &Amount) -> Result<U256> {
    value
        .to_be_bytes()
        .map(U256::from_be_bytes)
        .map_err(|e| failure(SimulationInvalidFixture, "amount", e))
}
fn as_amount(value: U256) -> Result<Amount> {
    Amount::from_be_bytes(&value.to_be_bytes::<32>())
        .map_err(|e| failure(SimulationInvalidFixture, "amount", e))
}
fn checked_add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| failure(SimulationFixtureLimit, "image", "size overflow"))
}
fn chain_id(key: &str) -> Result<u64> {
    xone_domain::validate_chain(key).map_err(|e| failure(SimulationInvalidFixture, "chain", e))?;
    key.strip_prefix("evm:")
        .and_then(|n| n.parse().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            failure(
                SimulationInvalidFixture,
                "chain",
                "EVM chain ID must fit u64",
            )
        })
}
struct SizeCheck {
    bytes: usize,
    max: usize,
}
impl std::io::Write for SizeCheck {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= self.max)
            .ok_or_else(|| std::io::Error::other("fixture JSON bytes"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl FixtureState {
    fn validate_shape(&self, budget: &SimulationBudget) -> Result<()> {
        if self.accounts.len() > budget.max_accounts {
            return Err(failure(SimulationFixtureLimit, "image", "account count"));
        }
        // Count serialized bytes without materializing a second JSON vector, including direct typed callers.
        serde_json::to_writer(
            SizeCheck {
                bytes: 0,
                max: budget.max_fixture_json_bytes,
            },
            self,
        )
        .map_err(|e| failure(SimulationFixtureLimit, "image", e))?;
        if self.schema_version != 1 {
            return Err(failure(
                SimulationInvalidFixture,
                "schema",
                "expected schema_version 1",
            ));
        }
        chain_id(&self.state_anchor.chain_key)?;
        if let Some(base) = &self.state_anchor.base_fee_per_gas {
            amount(base)?;
        }
        for h in [
            &self.state_anchor.block_hash,
            &self.state_anchor.parent_hash,
            &self.state_anchor.genesis_hash,
            &self.execution_environment.block_hash,
            &self.execution_environment.parent_hash,
            &self.execution_environment.prevrandao,
        ] {
            hex_length(h, Some(32))?;
        }
        address(&self.execution_environment.coinbase)?;
        amount(&self.execution_environment.base_fee)?;
        self.execution_environment.cancun_blobs()?;
        self.execution_environment.beacon_root_slots()?;
        let mut slots = 0;
        let mut codes = 0;
        let mut image_bytes = 0;
        let mut accounts = BTreeSet::new();
        for a in &self.accounts {
            hex_length(&a.address, Some(20))?;
            if !accounts.insert(&a.address) {
                return Err(failure(SimulationDuplicateAccount, "image", &a.address));
            }
            amount(&a.balance)?;
            slots = checked_add(slots, a.storage.len())?;
            codes = checked_add(codes, hex_length(&a.code, None)?)?;
            image_bytes = checked_add(image_bytes, 93)?;
            image_bytes = checked_add(image_bytes, hex_length(&a.code, None)?)?;
            if slots > budget.max_storage_slots
                || codes > budget.max_code_bytes
                || image_bytes > budget.max_image_bytes
            {
                return Err(failure(
                    SimulationFixtureLimit,
                    "image",
                    "storage/code/image cap",
                ));
            }
            if !a.exists
                && (a.balance != Amount::zero()
                    || a.nonce != 0
                    || a.code != "0x"
                    || !a.storage.is_empty())
            {
                return Err(failure(
                    SimulationInvalidFixture,
                    "image",
                    "absent account has data",
                ));
            }
            let mut seen = BTreeSet::new();
            for slot in &a.storage {
                hex_length(&slot.slot, Some(32))?;
                hex_length(&slot.value, Some(32))?;
                if !seen.insert(&slot.slot) {
                    return Err(failure(SimulationDuplicateStorage, "image", &slot.slot));
                }
                image_bytes = checked_add(image_bytes, 84)?;
                if image_bytes > budget.max_image_bytes {
                    return Err(failure(SimulationFixtureLimit, "image", "storage bytes"));
                }
            }
        }
        let mut hashes = BTreeSet::new();
        for row in &self.block_hashes {
            if !hashes.insert(row.number) {
                return Err(failure(SimulationDuplicateBlockHash, "image", row.number));
            }
            hex_length(&row.hash, Some(32))?;
            if row.number >= self.execution_environment.block_number
                || (row.number == self.state_anchor.block_number
                    && row.hash != self.state_anchor.block_hash)
                || (self.state_anchor.block_number.checked_sub(1) == Some(row.number)
                    && row.hash != self.state_anchor.parent_hash)
            {
                return Err(failure(
                    SimulationEnvironmentMismatch,
                    "block_hashes",
                    "hash conflicts with source or execution window",
                ));
            }
            image_bytes = checked_add(image_bytes, 40)?;
            if image_bytes > budget.max_image_bytes {
                return Err(failure(SimulationFixtureLimit, "image", "block hash bytes"));
            }
        }
        Ok(())
    }
    /// All caps and duplicate checks precede bytecode decode, storage decode and map conversion.
    pub fn to_image(&self, budget: &SimulationBudget) -> Result<Arc<core::StateImage>> {
        self.validate_shape(budget)?;
        let mut data = core::ImageData::default();
        for row in &self.accounts {
            let addr = address(&row.address)?;
            if !row.exists {
                data.accounts.insert(addr, None);
                continue;
            }
            let code = Bytes::from(
                hex::decode(&row.code[2..])
                    .map_err(|e| failure(SimulationInvalidFixture, "code", e))?,
            );
            let hash = keccak256(&code);
            data.accounts.insert(
                addr,
                Some(AccountInfo {
                    balance: amount(&row.balance)?,
                    nonce: row.nonce,
                    code_hash: hash,
                    code: None,
                }),
            );
            data.bytecodes.insert(hash, code);
            for slot in &row.storage {
                data.storage
                    .insert((addr, word(&slot.slot)?), word(&slot.value)?);
            }
        }
        for row in &self.block_hashes {
            data.block_hashes
                .insert(row.number, B256::from(fixed::<32>(&row.hash)?));
        }
        core::StateImage::new(data)
            .map(Arc::new)
            .map_err(|e| failure(SimulationInvalidFixture, "image", e))
    }
    /// Validate only the pinned historical OP/Base execution interval and H -> H+1 identity.
    pub fn validate_historical_op_environment(
        &self,
        snapshot: &xone_domain::SnapshotRef,
    ) -> Result<core::OpFork> {
        let env = &self.execution_environment;
        let fork = core::OpFork::at(env.chain_id, env.timestamp)
            .filter(|fork| fork.label() == env.fork)
            .ok_or_else(|| {
                failure(
                    SimulationEnvironmentMismatch,
                    "op_environment",
                    "unsupported historical fork",
                )
            })?;
        self.validate_snapshot_environment(snapshot, fork.label())?;
        Ok(fork)
    }
    /// This wrapper supports historical H -> H+1 replay only, not an implicit mix of contexts.
    pub fn validate_environment(&self, bound: &BoundEvmExecution) -> Result<()> {
        self.validate_snapshot_environment(&bound.plan.snapshot, "shanghai")
    }
    pub(crate) fn validate_snapshot_environment(
        &self,
        snapshot: &xone_domain::SnapshotRef,
        fork: &str,
    ) -> Result<()> {
        let source = &self.state_anchor;
        let block = &snapshot.block;
        let env = &self.execution_environment;
        let timestamp = source.timestamp.checked_mul(1000).ok_or_else(|| {
            failure(
                SimulationAnchorMismatch,
                "anchor",
                "timestamp milliseconds overflow",
            )
        })?;
        if block.chain_key != source.chain_key
            || block.number_or_slot != source.block_number
            || block.hash != source.block_hash
            || block.parent_hash != source.parent_hash
            || block.timestamp_ms != timestamp
        {
            return Err(failure(
                SimulationAnchorMismatch,
                "anchor",
                "plan snapshot differs from supplied source H",
            ));
        }
        if chain_id(&source.chain_key)? != env.chain_id
            || env.fork != fork
            || source.block_number.checked_add(1) != Some(env.block_number)
            || env.parent_hash != source.block_hash
            || env.timestamp < source.timestamp
            || env.block_hash == source.block_hash
            || env.gas_limit == 0
        {
            return Err(failure(
                SimulationEnvironmentMismatch,
                "environment",
                "require declared fork H+1 with parent H and nondecreasing timestamp",
            ));
        }
        env.cancun_blobs()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataAuthority {
    ProvidedLocalState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvmStatus {
    Success,
    Revert,
    Halt,
}
/// Storage evidence names its access type and slot explicitly; legacy call records
/// retain their original wire shape by omitting the optional fields.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationType {
    Storage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub phase: String,
    pub target: String,
    pub calldata: String,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub observation_type: Option<ObservationType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    pub output: String,
    pub usage: BudgetUsage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceMeasurements {
    pub caller_sell_before: Amount,
    pub caller_sell_after: Amount,
    pub recipient_buy_before: Amount,
    pub recipient_buy_after: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvmReceipt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs_hash: Option<String>,
    pub status: EvmStatus,
    pub gas_used: u64,
    pub gas_refunded: u64,
    pub output: String,
    pub return_classification: String,
    pub halt_reason: Option<String>,
    pub sender_nonce_before: u64,
    pub sender_nonce_after: u64,
    pub post_state_digest: String,
}
/// Native balances are observed from the same REVM pre/post images as token deltas.
/// Execution fees cover the declared Shanghai L1 environment; L2 data fees are separate.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFeeMeasurements {
    pub caller_before: Amount,
    pub caller_after: Amount,
    pub coinbase_before: Amount,
    pub coinbase_after: Amount,
    pub effective_gas_price: Amount,
    pub execution_fee: Amount,
    pub base_fee_burned: Amount,
    pub priority_fee_paid: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_transaction_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance_reset: Option<ApprovalReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<WrapReceipt>,
    #[serde(
        serialize_with = "serialize_false",
        deserialize_with = "deserialize_false"
    )]
    pub verified: bool,
    pub authority: DataAuthority,
    pub plan_validation: String,
    pub execution: EvmReceipt,
    pub matches_plan: bool,
    pub balances: BalanceMeasurements,
    pub actual_input_consumed: Amount,
    /// Native prefunding returned to the caller after a successful swap.
    pub refund: Amount,
    /// ExactOut input limit minus measured input. Includes any native refund.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unused_input_budget: Option<Amount>,
    pub actual_buy_amount: Amount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_fees: Option<NativeFeeMeasurements>,
    /// Complete signed historical OP fee report, never a Shanghai burn measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_fees: Option<serde_json::Value>,
    pub observations: Vec<Observation>,
    pub usage: BudgetUsage,
    pub source_fixture_hash: String,
    pub simulator_version: String,
    pub receipt_hash: String,
}
fn control_check(control: &core::ExecutionControl, phase: &str) -> Result<()> {
    if control.cancellation.load(Ordering::Acquire) {
        return Err(failure(SimulationCancelled, phase, "cancelled"));
    }
    if control.deadline.is_some_and(|d| Instant::now() >= d) {
        return Err(failure(SimulationDeadline, phase, "deadline"));
    }
    Ok(())
}
fn canonical_hash<T: Serialize>(value: &T) -> Result<String> {
    xone_domain::hash(value).map_err(|e| failure(SimulationInvalidFixture, "hash", e))
}
fn calldata(signature: &str, args: &[[u8; 32]]) -> Vec<u8> {
    let mut data = keccak256(signature.as_bytes())[..4].to_vec();
    for arg in args {
        data.extend(arg);
    }
    data
}
fn address_word(value: &str) -> Result<[u8; 32]> {
    let mut w = [0; 32];
    w[12..].copy_from_slice(address(value)?.as_slice());
    Ok(w)
}
fn decode_address(bytes: &[u8], phase: &str) -> Result<Address> {
    if bytes.len() != 32 || bytes[..12].iter().any(|b| *b != 0) {
        return Err(failure(
            SimulationObservationUnavailable,
            phase,
            "expected canonical ABI address word",
        ));
    }
    Ok(Address::from_slice(&bytes[12..]))
}
fn code_check(image: &core::StateImage, target: &str, expected: &str) -> Result<()> {
    let info = image
        .data()
        .accounts
        .get(&address(target)?)
        .and_then(|a| a.as_ref())
        .ok_or_else(|| failure(SimulationCodeMismatch, "runtime_code", target))?;
    let code = image
        .data()
        .bytecodes
        .get(&info.code_hash)
        .ok_or_else(|| failure(SimulationCodeMismatch, "runtime_code", "missing code"))?;
    if hex::encode(Sha256::digest(code)) != expected {
        return Err(failure(SimulationCodeMismatch, "runtime_code", target));
    }
    Ok(())
}
fn delta_hash(state: &revm::primitives::EvmState) -> Result<String> {
    let mut rows = BTreeMap::new();
    for (addr, account) in state {
        let storage=account.storage.iter().map(|(slot,value)|(format!("{slot:064x}"),serde_json::json!({"original":format!("{:064x}",value.original_value()),"present":format!("{:064x}",value.present_value())}))).collect::<BTreeMap<_,_>>();
        rows.insert(format!("{addr:#x}"),serde_json::json!({"balance":account.info.balance.to_string(),"nonce":account.info.nonce,"code_hash":format!("{:#x}",account.info.code_hash),"status":account.status.bits(),"storage":storage}));
    }
    canonical_hash(&rows)
}
struct Runner<'a> {
    caller_address: Address,
    budget: &'a SimulationBudget,
    env: &'a ExecutionEnvironment,
    tx: &'a TransactionContext,
    control: core::ExecutionControl,
    usage: BudgetUsage,
    observations: Vec<Observation>,
    op: Option<&'a OpTransactionContext>,
    op_fees: Option<serde_json::Value>,
}
impl Runner<'_> {
    fn native_balance(
        &self,
        image: &core::StateImage,
        owner: Address,
        phase: &str,
    ) -> Result<U256> {
        match image.data().accounts.get(&owner) {
            Some(Some(info)) => Ok(info.balance),
            Some(None) => Ok(U256::ZERO),
            None => Err(self.error(
                SimulationObservationUnavailable,
                phase,
                "account not captured",
            )),
        }
    }

    fn native_fees(
        &self,
        before: &core::StateImage,
        after: &core::StateImage,
        gas_used: u64,
    ) -> Result<NativeFeeMeasurements> {
        let phase = "native_fees";
        let base_fee = amount(&self.env.base_fee)?;
        let max_fee = amount(&self.tx.gas_price)?;
        let fee_budget = max_fee.checked_sub(base_fee).ok_or_else(|| {
            self.error(
                SimulationTransactionContext,
                phase,
                "fee cap below base fee",
            )
        })?;
        let tip = self
            .tx
            .max_priority_fee_per_gas
            .as_ref()
            .map(amount)
            .transpose()?;
        let priority = tip.map_or(fee_budget, |tip| tip.min(fee_budget));
        let price = base_fee + priority;
        let coinbase = address(&self.env.coinbase)?;
        let multiply = |value: U256| {
            value
                .checked_mul(U256::from(gas_used))
                .ok_or_else(|| self.error(SimulationTransactionContext, phase, "fee overflow"))
        };
        Ok(NativeFeeMeasurements {
            caller_before: as_amount(self.native_balance(before, self.caller_address, phase)?)?,
            caller_after: as_amount(self.native_balance(after, self.caller_address, phase)?)?,
            coinbase_before: as_amount(self.native_balance(before, coinbase, phase)?)?,
            coinbase_after: as_amount(self.native_balance(after, coinbase, phase)?)?,
            effective_gas_price: as_amount(price)?,
            execution_fee: as_amount(multiply(price)?)?,
            base_fee_burned: as_amount(multiply(base_fee)?)?,
            priority_fee_paid: as_amount(multiply(priority)?)?,
        })
    }

    fn error(
        &self,
        code: SimulationErrorCode,
        phase: &str,
        detail: impl ToString,
    ) -> SimulationError {
        let mut e = failure(code, phase, detail);
        e.usage = self.usage.clone();
        e
    }
    fn observe_error(&self, mut e: SimulationError) -> SimulationError {
        e.usage = self.usage.clone();
        e
    }
    fn run(
        &mut self,
        image: Arc<core::StateImage>,
        tx: TxEnv,
        phase: &str,
        probe: bool,
    ) -> Result<core::ExecutionOutput> {
        control_check(&self.control, phase).map_err(|e| self.observe_error(e))?;
        let input = u64::try_from(tx.data.len())
            .map_err(|e| self.error(SimulationBudgetExceeded, phase, e))?;
        let total_input = self
            .usage
            .input_bytes
            .checked_add(input)
            .filter(|n| *n <= self.budget.max_total_input_bytes)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "aggregate input bytes"))?;
        if tx.data.len() > self.budget.max_input_bytes
            || self
                .usage
                .gas_used
                .checked_add(tx.gas_limit)
                .is_none_or(|n| n > self.budget.max_total_gas)
        {
            return Err(self.error(
                SimulationBudgetExceeded,
                phase,
                "input cap or insufficient gas reservation for next execution",
            ));
        }
        let reads = self
            .budget
            .max_reads
            .checked_sub(self.usage.reads)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "reads"))?;
        let read_bytes = self
            .budget
            .max_read_bytes
            .checked_sub(self.usage.read_bytes)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "read bytes"))?;
        let instructions = self
            .budget
            .max_instructions
            .checked_sub(self.usage.instructions)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "instructions"))?;
        self.usage.input_bytes = total_input;
        self.usage.executions = self.usage.executions.checked_add(1).ok_or_else(|| {
            self.error(SimulationBudgetExceeded, phase, "execution count overflow")
        })?;
        let gas_limit = tx.gas_limit;
        let env = self.env;
        let cancun_blobs = env.cancun_blobs()?;
        let fork = if let Some(op) = self.op {
            op.fork.spec()
        } else if cancun_blobs.is_some() {
            SpecId::CANCUN
        } else {
            SpecId::SHANGHAI
        };
        let request = core::TransactionRequest {
            fork,
            chain_id: env.chain_id,
            block: BlockEnv {
                number: U256::from(env.block_number),
                coinbase: address(&env.coinbase)?,
                timestamp: U256::from(env.timestamp),
                gas_limit: U256::from(env.gas_limit),
                basefee: amount(&env.base_fee)?,
                prevrandao: Some(B256::from(fixed::<32>(&env.prevrandao)?)),
                // OP does not carry blobs: its Ecotone+ header uses zero excess gas.
                blob_excess_gas_and_price: cancun_blobs.or_else(|| {
                    self.op
                        .filter(|op| {
                            !matches!(op.fork, core::OpFork::Regolith | core::OpFork::Canyon)
                        })
                        .map(|_| BlobExcessGasAndPrice::new(0, false))
                }),
                ..BlockEnv::default()
            },
            tx,
        };
        let limits = core::ExecutionLimits {
            reads: core::BoundReadLimits {
                max_reads: reads,
                max_bytes: read_bytes,
            },
            max_gas_limit: if probe {
                self.budget.probe_gas_limit
            } else {
                self.budget.max_transaction_gas
            },
            max_instructions: instructions,
            max_input_bytes: self.budget.max_input_bytes,
            max_memory_bytes: self.budget.max_memory_bytes,
        };
        let result = if let Some(op) = self.op {
            if probe {
                core::observe_op(image, request, op.fork, limits, self.control.clone())
            } else {
                if request.tx != *op.transaction.environment() {
                    return Err(self.error(
                        SimulationBindingMismatch,
                        phase,
                        "signed OP transaction differs",
                    ));
                }
                core::execute_op_user(
                    image.clone(),
                    core::OpExecutionRequest {
                        fork: op.fork,
                        block: request.block,
                        transaction: op.transaction.clone(),
                        attributes_calldata: op.attributes_calldata.clone(),
                    },
                    limits,
                    self.control.clone(),
                )
                .map(|out| {
                    use xone_execution_preparation::op_chain_cost as cost;
                    let mut accounts = BTreeMap::new();
                    // The engine has already required every fee-role account
                    // and reconciled its before/after balance under the budget.
                    for owner in [
                        self.caller_address,
                        address(&self.env.coinbase).unwrap(),
                        Address::from(cost::BASE_FEE_VAULT),
                        Address::from(cost::L1_FEE_VAULT),
                        Address::from(cost::OPERATOR_FEE_VAULT),
                    ] {
                        let before = image.data().accounts[&owner]
                            .as_ref()
                            .map_or(U256::ZERO, |a| a.balance);
                        let after = out
                            .execution
                            .result_and_state
                            .state
                            .get(&owner)
                            .map_or(before, |a| a.info.balance);
                        accounts.insert(
                            format!("{owner:#x}"),
                            serde_json::json!({
                                "before": before.to_string(), "after": after.to_string()
                            }),
                        );
                    }
                    self.op_fees = Some(serde_json::json!({
                        "transaction_hash": format!("{:#x}", op.transaction.transaction_hash()),
                        "transaction_type": op.transaction.transaction_type(),
                        "chain_cost": out.costs,
                        "payer_cost": out.payer_cost,
                        "accounts": accounts
                    }));
                    out.execution
                })
            }
        } else {
            let engine = if probe { core::observe } else { core::execute };
            engine(image, request, limits, self.control.clone())
        };
        let telemetry = match &result {
            Ok(out) => out.telemetry,
            Err(e) => e.telemetry,
        };
        let gas = match &result {
            Ok(out) => out.result_and_state.result.gas_used(),
            Err(_) => gas_limit,
        };
        self.usage.reads = self
            .usage
            .reads
            .checked_add(telemetry.reads.reads)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "read count overflow"))?;
        self.usage.read_bytes = self
            .usage
            .read_bytes
            .checked_add(telemetry.reads.bytes)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "read byte overflow"))?;
        self.usage.instructions = self
            .usage
            .instructions
            .checked_add(telemetry.instructions)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "instruction overflow"))?;
        self.usage.gas_used = self
            .usage
            .gas_used
            .checked_add(gas)
            .filter(|n| *n <= self.budget.max_total_gas)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "aggregate gas"))?;
        let out = result.map_err(|e| {
            let code = match e.reason() {
                "cancelled" => SimulationCancelled,
                "deadline" => SimulationDeadline,
                "read_limit" | "byte_limit" | "instruction_limit" | "memory_limit"
                | "counter_overflow" | "input_limit" | "gas_limit" => SimulationBudgetExceeded,
                _ => {
                    if probe {
                        SimulationObservationUnavailable
                    } else {
                        SimulationExecutionUnavailable
                    }
                }
            };
            self.error(code, phase, e)
        })?;
        control_check(&self.control, phase).map_err(|e| self.observe_error(e))?;
        if out
            .result_and_state
            .result
            .output()
            .is_some_and(|b| b.len() > self.budget.max_output_bytes)
        {
            return Err(self.error(SimulationOutputLimit, phase, "returndata cap"));
        }
        Ok(out)
    }
    fn probe(
        &mut self,
        image: Arc<core::StateImage>,
        target: &str,
        data: Vec<u8>,
        phase: &str,
        expected: usize,
    ) -> Result<Vec<u8>> {
        let caller = self.tx_caller(image.as_ref())?;
        let from = caller.0;
        let nonce = caller.1;
        let output = self.run(
            image,
            TxEnv {
                caller: from,
                transact_to: TxKind::Call(address(target)?),
                data: Bytes::copy_from_slice(&data),
                value: U256::ZERO,
                nonce: Some(nonce),
                chain_id: Some(self.tx.chain_id),
                gas_limit: self.budget.probe_gas_limit,
                gas_price: amount(&self.tx.gas_price)?,
                gas_priority_fee: self
                    .tx
                    .max_priority_fee_per_gas
                    .as_ref()
                    .map(amount)
                    .transpose()?,
                ..TxEnv::default()
            },
            phase,
            true,
        )?;
        let result = &output.result_and_state.result;
        if !result.is_success() || result.output().is_none_or(|bytes| bytes.len() != expected) {
            return Err(self.error(
                SimulationObservationUnavailable,
                phase,
                format!("expected successful {expected}-byte ABI return"),
            ));
        }
        let bytes = result
            .output()
            .ok_or_else(|| self.error(SimulationObservationUnavailable, phase, "missing output"))?;
        self.observations.push(Observation {
            phase: phase.into(),
            target: target.into(),
            calldata: format!("0x{}", hex::encode(data)),
            observation_type: None,
            slot: None,
            output: format!("0x{}", hex::encode(bytes)),
            usage: BudgetUsage {
                reads: output.telemetry.reads.reads,
                read_bytes: output.telemetry.reads.bytes,
                instructions: output.telemetry.instructions,
                input_bytes: 0,
                gas_used: result.gas_used(),
                executions: 1,
            },
        });
        if let Some(last) = self.observations.last_mut() {
            last.usage.input_bytes = ((last.calldata.len() - 2) / 2) as u64;
        }
        // Intentionally drop the complete result state: nonce, gas, balance and storage writes included.
        Ok(bytes.to_vec())
    }
    /// Private Pair lock access uses the same strict DB and remaining aggregate
    /// read/byte/control budgets as EVM calls. Missing storage is never zero-filled.
    fn pair_lock(&mut self, image: Arc<core::StateImage>, pair: &str, phase: &str) -> Result<()> {
        control_check(&self.control, phase).map_err(|e| self.observe_error(e))?;
        let reads = self
            .budget
            .max_reads
            .checked_sub(self.usage.reads)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "reads"))?;
        let bytes = self
            .budget
            .max_read_bytes
            .checked_sub(self.usage.read_bytes)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "read bytes"))?;
        let mut db = core::StrictDB::new(
            image,
            core::BoundReadLimits {
                max_reads: reads,
                max_bytes: bytes,
            },
            self.control.clone(),
        );
        let slot = U256::from(12);
        let result = db.storage(address(pair)?, slot);
        let used = db.usage();
        self.usage.reads =
            self.usage.reads.checked_add(used.reads).ok_or_else(|| {
                self.error(SimulationBudgetExceeded, phase, "read count overflow")
            })?;
        self.usage.read_bytes = self
            .usage
            .read_bytes
            .checked_add(used.bytes)
            .ok_or_else(|| self.error(SimulationBudgetExceeded, phase, "read bytes overflow"))?;
        let value = result.map_err(|e| {
            let code = match e.reason() {
                "cancelled" => SimulationCancelled,
                "deadline" => SimulationDeadline,
                "read_limit" | "byte_limit" | "counter_overflow" => SimulationBudgetExceeded,
                _ => SimulationObservationUnavailable,
            };
            self.error(code, phase, e)
        })?;
        control_check(&self.control, phase).map_err(|e| self.observe_error(e))?;
        self.observations.push(Observation {
            phase: phase.into(),
            target: pair.into(),
            calldata: String::new(),
            observation_type: Some(ObservationType::Storage),
            slot: Some(format!("0x{slot:064x}")),
            output: format!("0x{value:064x}"),
            usage: BudgetUsage {
                reads: used.reads,
                read_bytes: used.bytes,
                ..Default::default()
            },
        });
        if value != U256::from(1) {
            return Err(self.error(
                SimulationObservationMismatch,
                phase,
                "Pair lock slot12 must equal 1",
            ));
        }
        Ok(())
    }

    /// These observations read only the overlay produced by the actual swap.
    /// They carry no new bank/source identity and do not use mathematical outputs.
    fn complete_poststate(&mut self, image: Arc<core::StateImage>, pool: &Pool) -> Result<()> {
        self.probe(
            image.clone(),
            &pool.pool_id,
            calldata("getReserves()", &[]),
            "pair_reserves_after",
            96,
        )?;
        self.balance(
            image.clone(),
            &pool.token0,
            &pool.pool_id,
            "pair_balance0_after",
        )?;
        self.balance(
            image.clone(),
            &pool.token1,
            &pool.pool_id,
            "pair_balance1_after",
        )?;
        for (signature, phase) in [
            ("price0CumulativeLast()", "pair_price0_cumulative_after"),
            ("price1CumulativeLast()", "pair_price1_cumulative_after"),
        ] {
            self.probe(
                image.clone(),
                &pool.pool_id,
                calldata(signature, &[]),
                phase,
                32,
            )?;
        }
        self.pair_lock(image, &pool.pool_id, "pair_lock_after")
    }

    fn tx_caller(&self, image: &core::StateImage) -> Result<(Address, u64)> {
        let from = self.caller_address;
        let info = image
            .data()
            .accounts
            .get(&from)
            .and_then(|v| v.as_ref())
            .ok_or_else(|| {
                self.error(
                    SimulationObservationUnavailable,
                    "probe_caller",
                    "explicit present real caller required",
                )
            })?;
        Ok((from, info.nonce))
    }
    fn balance(
        &mut self,
        image: Arc<core::StateImage>,
        token: &str,
        owner: &str,
        phase: &str,
    ) -> Result<U256> {
        let output = self.probe(
            image,
            token,
            calldata("balanceOf(address)", &[address_word(owner)?]),
            phase,
            32,
        )?;
        Ok(U256::from_be_slice(&output))
    }
    fn expect_address(
        &mut self,
        image: Arc<core::StateImage>,
        target: &str,
        sig: &str,
        args: &[[u8; 32]],
        expected: &str,
    ) -> Result<()> {
        let bytes = self.probe(image, target, calldata(sig, args), sig, 32)?;
        if decode_address(&bytes, sig).map_err(|e| self.observe_error(e))? != address(expected)? {
            return Err(self.error(
                SimulationObservationMismatch,
                sig,
                "physical address mismatch",
            ));
        }
        Ok(())
    }
    fn preflight(
        &mut self,
        image: Arc<core::StateImage>,
        executor: &ExecutorIdentity,
        pool: &Pool,
    ) -> Result<()> {
        self.expect_address(
            image.clone(),
            &executor.target,
            "factory()",
            &[],
            &executor.factory,
        )?;
        self.expect_address(
            image.clone(),
            &executor.factory,
            "getPair(address,address)",
            &[address_word(&pool.token0)?, address_word(&pool.token1)?],
            &pool.pool_id,
        )?;
        self.expect_address(
            image.clone(),
            &pool.pool_id,
            "factory()",
            &[],
            &executor.factory,
        )?;
        self.expect_address(image.clone(), &pool.pool_id, "token0()", &[], &pool.token0)?;
        self.expect_address(image.clone(), &pool.pool_id, "token1()", &[], &pool.token1)?;
        let reserves = self.probe(
            image.clone(),
            &pool.pool_id,
            calldata("getReserves()", &[]),
            "getReserves()",
            96,
        )?;
        if reserves[..18]
            .iter()
            .chain(reserves[32..50].iter())
            .chain(reserves[64..92].iter())
            .any(|b| *b != 0)
        {
            return Err(self.error(
                SimulationObservationUnavailable,
                "getReserves()",
                "invalid uint112/uint32 ABI widths",
            ));
        }
        if U256::from_be_slice(&reserves[..32]) != amount(&pool.reserve0)?
            || U256::from_be_slice(&reserves[32..64]) != amount(&pool.reserve1)?
        {
            return Err(self.error(
                SimulationObservationMismatch,
                "getReserves()",
                "selected pool reserves differ",
            ));
        }
        // Raw balances may include unsynced donations; observe them separately from stored reserves.
        // The actual swap enforces the pair invariant and supplies the post-state delta.
        let raw0 = self.balance(image.clone(), &pool.token0, &pool.pool_id, "pair_balance0")?;
        let raw1 = self.balance(image.clone(), &pool.token1, &pool.pool_id, "pair_balance1")?;
        if pool.protocol == xone_domain::v2_swap::PROFILE {
            let swap = pool
                .v2
                .as_ref()
                .and_then(|v| v.swap.as_ref())
                .ok_or_else(|| {
                    self.error(
                        SimulationBindingMismatch,
                        "complete_state",
                        "swap state required",
                    )
                })?;
            if raw0 != amount(&swap.raw_balance0)? || raw1 != amount(&swap.raw_balance1)? {
                return Err(self.error(
                    SimulationObservationMismatch,
                    "pair_raw_balances",
                    "raw balances differ from source state",
                ));
            }
            if U256::from_be_slice(&reserves[64..96]) != U256::from(swap.block_timestamp_last) {
                return Err(self.error(
                    SimulationObservationMismatch,
                    "getReserves()",
                    "source LastTimestamp differs",
                ));
            }
            for (signature, expected) in [
                ("price0CumulativeLast()", &swap.price0_cumulative_last),
                ("price1CumulativeLast()", &swap.price1_cumulative_last),
            ] {
                let bytes = self.probe(
                    image.clone(),
                    &pool.pool_id,
                    calldata(signature, &[]),
                    signature,
                    32,
                )?;
                if U256::from_be_slice(&bytes) != amount(expected)? {
                    return Err(self.error(
                        SimulationObservationMismatch,
                        signature,
                        "source cumulative price differs",
                    ));
                }
            }
            self.pair_lock(image, &pool.pool_id, "pair_lock_before")?;
        }
        Ok(())
    }
}
/// Internal historical OP invocation. Source bank authentication stays separate.
#[derive(Clone, Debug)]
pub struct OpTransactionContext {
    pub fork: core::OpFork,
    pub transaction: crate::op_transaction::AuthenticatedUser,
    pub attributes_calldata: Vec<u8>,
}
pub fn op_context(op: &OpTransactionContext) -> Result<TransactionContext> {
    let e = op.transaction.environment();
    Ok(TransactionContext {
        nonce: e.nonce.unwrap(),
        gas_limit: e.gas_limit,
        gas_price: as_amount(e.gas_price)?,
        max_priority_fee_per_gas: e.gas_priority_fee.map(as_amount).transpose()?,
        chain_id: e.chain_id.unwrap(),
    })
}
/// Execute the exact BOUND plan under authenticated historical OP rules.
/// Observations and the swap share budgets; fee-only native scope remains closed.
pub fn simulate_bound_op_user(
    bound: &BoundEvmExecution,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    op: &OpTransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    simulate_execution(
        BoundSource::ExactIn(bound, None),
        pools,
        allowlist,
        fixture,
        &op_context(op)?,
        budget,
        control,
        None,
        Some(op),
    )
}
/// Output-specified equivalent; no implicit change to calldata, recipient or fees.
pub fn simulate_bound_op_user_exact_out(
    bound: &BoundEvmExactOutExecution,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    op: &OpTransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    simulate_execution(
        BoundSource::ExactOut(bound, None),
        pools,
        allowlist,
        fixture,
        &op_context(op)?,
        budget,
        control,
        None,
        Some(op),
    )
}

/// Revalidate a complete binding and replay local source H in the explicit H+1 environment.
/// Every probe and swap shares remaining read/instruction/input/gas budgets and one control context.
/// No input receipt or caller assertion is accepted as execution evidence.
pub fn simulate_bound(
    bound: &BoundEvmExecution,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    transaction_context: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    simulate_bound_with_funding(
        bound,
        pools,
        allowlist,
        fixture,
        transaction_context,
        budget,
        control,
        None,
    )
}

// The optional preparation observation shares the same image, probes and budgets.
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_bound_with_funding(
    bound: &BoundEvmExecution,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    transaction_context: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
    funding: Option<&xone_execution_preparation::evm::FundingObservation>,
) -> Result<SimulationOutcome> {
    simulate_execution(
        BoundSource::ExactIn(bound, None),
        pools,
        allowlist,
        fixture,
        transaction_context,
        budget,
        control,
        funding,
        None,
    )
}

/// Simple-path original Router02 ExactOut execution over provided H/H+1 state.
pub fn simulate_bound_exact_out(
    bound: &BoundEvmExactOutExecution,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    transaction_context: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    simulate_bound_exact_out_with_funding(
        bound,
        pools,
        allowlist,
        fixture,
        transaction_context,
        budget,
        control,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_bound_exact_out_with_funding(
    bound: &BoundEvmExactOutExecution,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    transaction_context: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
    funding: Option<&xone_execution_preparation::evm::FundingObservation>,
) -> Result<SimulationOutcome> {
    simulate_execution(
        BoundSource::ExactOut(bound, None),
        pools,
        allowlist,
        fixture,
        transaction_context,
        budget,
        control,
        funding,
        None,
    )
}

pub(crate) fn simulate_wrapped_direct(
    request: &crate::worker::WorkerRequest,
    wrap: &WrapSequence,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    let source = match (&request.bound, &request.exact_out_bound) {
        (Some(b), None) => BoundSource::ExactIn(b, Some(wrap)),
        (None, Some(b)) => BoundSource::ExactOut(b, Some(wrap)),
        _ => {
            return Err(failure(
                SimulationBindingMismatch,
                "wrap_source",
                "one bound swap required",
            ))
        }
    };
    simulate_execution(
        source,
        &request.pools,
        &request.allowed_targets,
        &request.fixture,
        &request.transaction_context,
        &request.budget,
        control,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn simulate_bound_split(
    bound: &BoundSplit,
    expected: &SplitExecutor,
    sequence: Option<&ApprovalSequence>,
    wrap: Option<&WrapSequence>,
    pools: &[Pool],
    allowed: &[String],
    fixture: &FixtureState,
    tx: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    simulate_execution(
        BoundSource::Split(bound, expected, sequence, wrap),
        pools,
        allowed,
        fixture,
        tx,
        budget,
        control,
        None,
        None,
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalReceipt {
    pub transaction: Transaction,
    pub nonce: u64,
    pub block_hash: String,
    pub gas_used: u64,
    pub execution_fee: Amount,
    pub payer_before: Amount,
    pub payer_after: Amount,
    pub beneficiary_before: Amount,
    pub beneficiary_after: Amount,
    pub post_state_digest: String,
}

/// Reset executes in the fixture's first child. Exact approval uses the next block.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowanceReset {
    pub context: TransactionContext,
    pub approval_environment: ExecutionEnvironment,
}
/// Exact approval, optionally preceded by a reset, followed by the swap.
/// Transaction targets, payloads and the approval amount are derived from the bound plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalSequence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<AllowanceReset>,
    pub token_code_sha256: String,
    pub approval_context: TransactionContext,
    pub swap_environment: ExecutionEnvironment,
}
#[derive(Clone, Copy)]
enum BoundSource<'a> {
    ExactIn(&'a BoundEvmExecution, Option<&'a WrapSequence>),
    ExactOut(&'a BoundEvmExactOutExecution, Option<&'a WrapSequence>),
    Split(
        &'a BoundSplit,
        &'a SplitExecutor,
        Option<&'a ApprovalSequence>,
        Option<&'a WrapSequence>,
    ),
}
impl Serialize for BoundSource<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::ExactIn(b, None) => b.serialize(serializer),
            Self::ExactIn(b, Some(w)) => ("ordered-wrap-sequence/1", b, w).serialize(serializer),
            Self::ExactOut(b, None) => b.serialize(serializer),
            Self::ExactOut(b, Some(w)) => ("ordered-wrap-sequence/1", b, w).serialize(serializer),
            Self::Split(b, _, None, None) => b.serialize(serializer),
            Self::Split(b, _, Some(seq), Some(wrap)) => {
                ("ordered-wrap-approval-sequence/1", b, wrap, seq).serialize(serializer)
            }
            Self::Split(b, _, None, Some(wrap)) => {
                ("ordered-wrap-sequence/1", b, wrap).serialize(serializer)
            }
            Self::Split(b, _, Some(seq), None) => {
                ("ordered-approval-sequence/1", b, seq).serialize(serializer)
            }
        }
    }
}
struct BoundView<'a> {
    context: &'a Context,
    snapshot: &'a xone_domain::SnapshotRef,
    transaction: &'a Transaction,
    executor: &'a ExecutorIdentity,
    touched_pool_ids: Vec<&'a String>,
}
impl BoundSource<'_> {
    fn view(&self) -> BoundView<'_> {
        match self {
            Self::ExactIn(b, _) => BoundView {
                context: &b.plan.context,
                snapshot: &b.plan.snapshot,
                transaction: &b.transaction,
                executor: &b.executor,
                touched_pool_ids: b.touched_pool_ids.iter().collect(),
            },
            Self::ExactOut(b, _) => BoundView {
                context: &b.plan.context,
                snapshot: &b.plan.snapshot,
                transaction: &b.transaction,
                executor: &b.executor,
                touched_pool_ids: b.touched_pool_ids.iter().collect(),
            },
            Self::Split(b, e, _, _) => {
                let (context, snapshot) = match &b.plan {
                    SplitPlan::ExactIn(p) => (&p.context, &p.snapshot),
                    SplitPlan::ExactOut(p) => (&p.context, &p.snapshot),
                };
                let mut seen = BTreeSet::new();
                BoundView {
                    context,
                    snapshot,
                    transaction: &b.transaction,
                    executor: &e.router,
                    touched_pool_ids: b
                        .leg_pools
                        .iter()
                        .flatten()
                        .filter(|id| seen.insert(*id))
                        .collect(),
                }
            }
        }
    }
    fn validate(&self, pools: &[Pool], allowed: &[String]) -> xone_domain::Result<()> {
        match self {
            Self::ExactIn(b, _) => validate_bound_execution(b, pools, allowed),
            Self::ExactOut(b, _) => validate_bound_exact_out_execution(b, pools, allowed),
            Self::Split(b, e, _, _) => ordered_v2::validate(b, pools, e, allowed),
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn simulate_execution(
    source: BoundSource<'_>,
    pools: &[Pool],
    allowlist: &[String],
    fixture: &FixtureState,
    transaction_context: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
    funding: Option<&xone_execution_preparation::evm::FundingObservation>,
    op: Option<&OpTransactionContext>,
) -> Result<SimulationOutcome> {
    control_check(&control, "entry")?;
    let bound = source.view();
    // Bound calldata is untrusted too: cap it before binding validation serializes/clones it.
    let input_length = bound.transaction.data.len().saturating_sub(2) / 2;
    let total_input = u64::try_from(input_length)
        .map_err(|e| failure(SimulationBudgetExceeded, "transaction_input", e))?;
    if input_length > budget.max_input_bytes || total_input > budget.max_total_input_bytes {
        return Err(failure(
            SimulationBudgetExceeded,
            "transaction_input",
            "input bytes",
        ));
    }
    hex_length(&bound.transaction.data, None)?;
    source
        .validate(pools, allowlist)
        .map_err(|e| failure(SimulationBindingMismatch, "binding", e))?;
    fixture.validate_shape(budget)?;
    fixture
        .validate_snapshot_environment(bound.snapshot, op.map_or("shanghai", |o| o.fork.label()))?;
    let tx = transaction_context;
    if tx.chain_id != fixture.execution_environment.chain_id
        || tx.gas_limit == 0
        || tx.gas_limit > budget.max_transaction_gas
        || bound.transaction.value != Amount::zero()
        || budget.probe_gas_limit < 21000
        || budget.max_memory_bytes == 0
        || budget.max_memory_bytes > 256 * 1024 * 1024
    {
        return Err(failure(
            SimulationTransactionContext,
            "transaction_context",
            "chain/gas/memory/zero ETH scope",
        ));
    }
    let max_fee = amount(&tx.gas_price)?;
    let priority_fee = tx
        .max_priority_fee_per_gas
        .as_ref()
        .map(amount)
        .transpose()?;
    let (sequence, wrapping) = match source {
        BoundSource::Split(_, _, _, Some(w)) if w.router_approval.is_some() => {
            return Err(failure(
                SimulationBindingMismatch,
                "approval_spender",
                "router approval supplied for batch executor",
            ))
        }
        BoundSource::Split(_, _, seq, wrap) => (seq, wrap),
        BoundSource::ExactIn(_, wrap) | BoundSource::ExactOut(_, wrap) => {
            (wrap.and_then(|w| w.router_approval.as_ref()), wrap)
        }
    };
    if let Some(wrap) = wrapping {
        let valid = match (sequence, &wrap.first_approval_environment) {
            (None, None) => true,
            (Some(seq), Some(first)) => {
                let mut blocks = BTreeSet::from([&fixture.state_anchor.block_hash]);
                canonical_hash(&seq.swap_environment)? == canonical_hash(&wrap.swap_environment)?
                    && [&fixture.execution_environment, first]
                        .into_iter()
                        .chain(seq.reset.iter().map(|r| &r.approval_environment))
                        .chain(std::iter::once(&seq.swap_environment))
                        .all(|b| blocks.insert(&b.block_hash))
            }
            _ => false,
        };
        if !valid {
            return Err(failure(
                SimulationBindingMismatch,
                "funding_sequence",
                "inconsistent deposit and allowance schedule",
            ));
        }
    }
    let swap_env = sequence
        .map(|s| &s.swap_environment)
        .or_else(|| wrapping.map(|s| &s.swap_environment))
        .unwrap_or(&fixture.execution_environment);
    let base_fee = amount(&swap_env.base_fee)?;
    if max_fee < base_fee || priority_fee.is_some_and(|tip| tip > max_fee) {
        return Err(failure(
            SimulationTransactionContext,
            "transaction_context",
            "invalid gas price, EIP-1559 fee cap or priority fee",
        ));
    }
    let op_input = if let Some(op) = op {
        let authenticated = op.transaction.environment();
        if authenticated.caller != address(&bound.transaction.from)?
            || authenticated.transact_to != TxKind::Call(address(&bound.transaction.to)?)
            || authenticated.value != amount(&bound.transaction.value)?
            || authenticated.data.as_ref()
                != hex::decode(&bound.transaction.data[2..])
                    .unwrap()
                    .as_slice()
        {
            return Err(failure(
                SimulationBindingMismatch,
                "op_signed_binding",
                "signed envelope differs from BOUND plan",
            ));
        }
        let wire = authenticated.optimism.enveloped_tx.as_ref().unwrap();
        let bytes = wire
            .len()
            .checked_add(op.attributes_calldata.len())
            .filter(|n| {
                u64::try_from(*n).is_ok_and(|n| n <= budget.max_total_input_bytes)
                    && wire.len() <= budget.max_input_bytes
                    && op.attributes_calldata.len() <= 260
            })
            .ok_or_else(|| {
                failure(
                    SimulationBudgetExceeded,
                    "op_signed_input",
                    "signed wire and attributes budget",
                )
            })?;
        bytes as u64
    } else {
        0
    };
    let mut image = fixture.to_image(budget)?;
    let touched_pools = bound
        .touched_pool_ids
        .into_iter()
        .map(|id| {
            pools
                .iter()
                .find(|p| &p.pool_id == id)
                .ok_or_else(|| failure(SimulationBindingMismatch, "pool", "selected pool missing"))
        })
        .collect::<Result<Vec<_>>>()?;
    if touched_pools.is_empty() {
        return Err(failure(
            SimulationBindingMismatch,
            "pool",
            "selected pool missing",
        ));
    }
    for pool in &touched_pools {
        if pool.protocol == xone_domain::v2_swap::PROFILE {
            let v2 = pool.v2.as_ref().ok_or_else(|| {
                failure(
                    SimulationBindingMismatch,
                    "complete_state",
                    "V2 identity required",
                )
            })?;
            let swap = v2.swap.as_ref().ok_or_else(|| {
                failure(
                    SimulationBindingMismatch,
                    "complete_state",
                    "swap state required",
                )
            })?;
            let source = &fixture.state_anchor;
            if swap.bank_block_number != source.block_number
                || swap.bank_block_hash != source.block_hash
                || swap.bank_unix_timestamp != source.timestamp
            {
                return Err(failure(
                    SimulationAnchorMismatch,
                    "complete_state",
                    "swap bank must identify source H",
                ));
            }
            if v2.code_sha256 != xone_domain::v2_swap::PAIR_CODE_SHA256
                || swap.factory_code_sha256 != xone_domain::v2_swap::FACTORY_CODE_SHA256
                || swap.factory_code_sha256 != bound.executor.factory_code_sha256
            {
                return Err(failure(
                    SimulationCodeMismatch,
                    "complete_state",
                    "fixed Pair/Factory identity required",
                ));
            }
        }
        code_check(
            &image,
            &pool.pool_id,
            &pool
                .v2
                .as_ref()
                .ok_or_else(|| failure(SimulationBindingMismatch, "pool", "V2 identity required"))?
                .code_sha256,
        )?;
    }
    code_check(&image, &bound.executor.target, &bound.executor.code_sha256)?;
    code_check(
        &image,
        &bound.executor.factory,
        &bound.executor.factory_code_sha256,
    )?;
    let source_fixture_hash = canonical_hash(fixture)?;
    let context = bound.context;
    let caller = address(&bound.transaction.from)?;
    let mut runner = Runner {
        budget,
        env: &fixture.execution_environment,
        tx: sequence.map_or(tx, |s| {
            s.reset.as_ref().map_or(&s.approval_context, |r| &r.context)
        }),
        control,
        usage: BudgetUsage {
            input_bytes: op_input,
            ..Default::default()
        },
        observations: vec![],
        op,
        op_fees: None,
        caller_address: caller,
    };
    let mut wrapped = None;
    let mut approval = None;
    let mut allowance_reset = None;
    let input = match source {
        BoundSource::ExactIn(b, _) => &b.plan.allocation.sell_amount,
        BoundSource::ExactOut(b, _) => &b.plan.allocation.required_input,
        BoundSource::Split(b, _, _, _) => match &b.plan {
            SplitPlan::ExactIn(p) => &p.allocation.sell_amount,
            SplitPlan::ExactOut(p) => &p.allocation.required_input,
        },
    };
    if let Some(wrap) = wrapping {
        runner.tx = &wrap.deposit_context;
        let next_tx = sequence.map_or(tx, |s| {
            s.reset.as_ref().map_or(&s.approval_context, |r| &r.context)
        });
        let (after, receipt) = wrap::execute(
            &mut runner,
            image,
            context,
            input,
            bound.executor,
            fixture,
            next_tx,
            wrap,
        )?;
        image = after;
        wrapped = Some(receipt);
        runner.env = wrap.first_approval_environment.as_ref().unwrap_or(swap_env);
        runner.tx = next_tx;
    }
    let allowance_phase = match source {
        BoundSource::Split(_, e, _, _) => {
            code_check(&image, &e.target, &e.code_sha256)?;
            runner.expect_address(image.clone(), &e.target, "router()", &[], &e.router.target)?;
            Some("split_caller_allowance_before")
        }
        _ if wrapping.is_some() => Some("direct_caller_allowance_before"),
        _ => None,
    };
    if let Some(phase) = allowance_phase {
        let spender = &bound.transaction.to;
        let allowance = runner.probe(
            image.clone(),
            &context.sell_token,
            calldata(
                "allowance(address,address)",
                &[address_word(&context.caller)?, address_word(spender)?],
            ),
            phase,
            32,
        )?;
        if let Some(seq) = sequence {
            let (after, reset, receipt) = approval::execute(
                &mut runner,
                image,
                context,
                input,
                spender,
                &allowance,
                fixture,
                tx,
                seq,
            )?;
            image = after;
            allowance_reset = reset;
            approval = Some(receipt);
            runner.env = swap_env;
            runner.tx = tx;
        }
    }
    for pool in &touched_pools {
        runner.preflight(image.clone(), bound.executor, pool)?;
    }
    let sell_before = runner.balance(
        image.clone(),
        &context.sell_token,
        &context.caller,
        "caller_sell_before",
    )?;
    let buy_before = runner.balance(
        image.clone(),
        &context.buy_token,
        &context.recipient,
        "recipient_buy_before",
    )?;
    if let Some(observed) = funding {
        let allowance = runner.probe(
            image.clone(),
            &context.sell_token,
            calldata(
                "allowance(address,address)",
                &[
                    address_word(&context.caller)?,
                    address_word(&bound.executor.target)?,
                ],
            ),
            "caller_allowance_before",
            32,
        )?;
        let (native, nonce) = match image.data().accounts.get(&caller) {
            Some(Some(info)) => (info.balance, info.nonce),
            Some(None) => (U256::ZERO, 0),
            None => {
                return Err(runner.error(
                    SimulationObservationUnavailable,
                    "prepared_funding",
                    "caller account not captured",
                ))
            }
        };
        if native != amount(&observed.native_balance)?
            || nonce != observed.pending_nonce
            || sell_before != amount(&observed.token_balance)?
            || U256::from_be_slice(&allowance) != amount(&observed.allowance)?
        {
            return Err(runner.error(
                SimulationObservationMismatch,
                "prepared_funding",
                "funding observation differs from execution bank",
            ));
        }
    }
    let expected_tx = TxEnv {
        caller,
        transact_to: TxKind::Call(address(&bound.transaction.to)?),
        value: U256::ZERO,
        data: Bytes::from(
            hex::decode(&bound.transaction.data[2..])
                .map_err(|e| failure(SimulationInvalidFixture, "transaction_input", e))?,
        ),
        nonce: Some(tx.nonce),
        chain_id: Some(tx.chain_id),
        gas_limit: tx.gas_limit,
        gas_price: max_fee,
        gas_priority_fee: priority_fee,
        ..TxEnv::default()
    };
    let actual_tx = if let Some(op) = op {
        let authenticated = op.transaction.environment();
        if authenticated.caller != expected_tx.caller
            || authenticated.transact_to != expected_tx.transact_to
            || authenticated.value != expected_tx.value
            || authenticated.data != expected_tx.data
            || authenticated.nonce != expected_tx.nonce
            || authenticated.chain_id != expected_tx.chain_id
            || authenticated.gas_limit != expected_tx.gas_limit
            || authenticated.gas_price != expected_tx.gas_price
            || authenticated.gas_priority_fee != expected_tx.gas_priority_fee
        {
            return Err(runner.error(
                SimulationBindingMismatch,
                "op_signed_binding",
                "signed envelope differs from BOUND plan",
            ));
        }
        authenticated.clone()
    } else {
        expected_tx
    };
    let output = runner.run(image.clone(), actual_tx, "swap", false)?;
    let actual = &output.result_and_state;
    let post = Arc::new(
        image
            .apply(&actual.state)
            .map_err(|e| runner.error(SimulationExecutionUnavailable, "post_state", e))?,
    );
    let sell_after = runner.balance(
        post.clone(),
        &context.sell_token,
        &context.caller,
        "caller_sell_after",
    )?;
    let buy_after = runner.balance(
        post.clone(),
        &context.buy_token,
        &context.recipient,
        "recipient_buy_after",
    )?;
    for pool in &touched_pools {
        if pool.protocol == xone_domain::v2_swap::PROFILE {
            runner.complete_poststate(post.clone(), pool)?;
        }
    }
    if sell_after > sell_before || buy_after < buy_before {
        return Err(runner.error(
            SimulationBalanceDirection,
            "balances",
            "sell increased or buy decreased",
        ));
    }
    let spent = sell_before - sell_after;
    let bought = buy_after - buy_before;
    let (status, gas_refunded, halt_reason) = match &actual.result {
        ExecutionResult::Success { gas_refunded, .. } => (EvmStatus::Success, *gas_refunded, None),
        ExecutionResult::Revert { .. } => (EvmStatus::Revert, 0, None),
        ExecutionResult::Halt { reason, .. } => (EvmStatus::Halt, 0, Some(format!("{reason:?}"))),
    };
    let returned = actual.result.output().map_or(&[][..], |b| b.as_ref());
    let classification = if returned.is_empty() {
        "EMPTY"
    } else if returned.len() == 128
        && U256::from_be_slice(&returned[..32]) == U256::from(32)
        && U256::from_be_slice(&returned[32..64]) == U256::from(2)
    {
        "ABI_UINT256_ARRAY_2"
    } else {
        "OPAQUE"
    };
    let nonce_after = post
        .data()
        .accounts
        .get(&caller)
        .and_then(|v| v.as_ref())
        .map(|a| a.nonce)
        .ok_or_else(|| {
            runner.error(
                SimulationExecutionUnavailable,
                "post_caller",
                "caller missing",
            )
        })?;
    enum Amounts<'a> {
        In(&'a xone_execution_plan::Plan),
        Out(&'a xone_execution_plan::exact_out::ExactOutPlan),
    }
    let amounts = match source {
        BoundSource::ExactIn(b, _) => Amounts::In(&b.plan),
        BoundSource::ExactOut(b, _) => Amounts::Out(&b.plan),
        BoundSource::Split(b, _, _, _) => match &b.plan {
            SplitPlan::ExactIn(p) => Amounts::In(p),
            SplitPlan::ExactOut(p) => Amounts::Out(p),
        },
    };
    let (matches_plan, unused_input_budget) = match amounts {
        Amounts::In(p) => (
            status == EvmStatus::Success
                && spent == amount(&p.allocation.sell_amount)?
                && bought == amount(&p.allocation.gross_buy_amount)?
                && bought >= amount(&p.min_buy_amount)?,
            None,
        ),
        Amounts::Out(p) => {
            let maximum = amount(&p.max_sell_amount)?;
            let unused = maximum.checked_sub(spent).ok_or_else(|| {
                runner.error(
                    SimulationBalanceDirection,
                    "exact_out_budget",
                    "actual input exceeds maximum",
                )
            })?;
            (
                status == EvmStatus::Success
                    && spent == amount(&p.allocation.required_input)?
                    && bought == amount(&p.allocation.target_output)?,
                Some(as_amount(unused)?),
            )
        }
    };
    let native_fees = op
        .is_none()
        .then(|| runner.native_fees(&image, &post, actual.result.gas_used()))
        .transpose()?;
    let simulator_version = if op.is_some() {
        "provided-local-state/op-signed-v1"
    } else {
        SIMULATOR_VERSION
    };
    let mut outcome = SimulationOutcome {
        signed_transaction_hash: None,
        wrap: wrapped,
        allowance_reset,
        approval,
        verified: false,
        authority: DataAuthority::ProvidedLocalState,
        plan_validation: "BOUND_REVALIDATED".into(),
        execution: EvmReceipt {
            logs_hash: None,
            status,
            gas_used: actual.result.gas_used(),
            gas_refunded,
            output: format!("0x{}", hex::encode(returned)),
            return_classification: classification.into(),
            halt_reason,
            sender_nonce_before: tx.nonce,
            sender_nonce_after: nonce_after,
            post_state_digest: delta_hash(&actual.state)?,
        },
        matches_plan,
        balances: BalanceMeasurements {
            caller_sell_before: as_amount(sell_before)?,
            caller_sell_after: as_amount(sell_after)?,
            recipient_buy_before: as_amount(buy_before)?,
            recipient_buy_after: as_amount(buy_after)?,
        },
        actual_input_consumed: as_amount(spent)?,
        refund: Amount::zero(),
        unused_input_budget,
        actual_buy_amount: as_amount(bought)?,
        native_fees,
        op_fees: runner.op_fees,
        observations: runner.observations,
        usage: runner.usage,
        source_fixture_hash,
        simulator_version: simulator_version.into(),
        receipt_hash: String::new(),
    };
    let mut value =
        serde_json::to_value(&outcome).map_err(|e| failure(SimulationInvalidFixture, "hash", e))?;
    if let Some(map) = value.as_object_mut() {
        map.remove("receipt_hash");
    }
    outcome.receipt_hash = canonical_hash(
        &serde_json::json!({"bound":source,"source_fixture_hash":outcome.source_fixture_hash,"transaction_context":transaction_context,"limits":budget,"simulator_version":simulator_version,"outcome":value}),
    )?;
    control_check(&runner.control, "receipt").map_err(|mut e| {
        e.usage = outcome.usage.clone();
        e
    })?;
    Ok(outcome)
}

fn deserialize_false<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<bool, D::Error> {
    if bool::deserialize(deserializer)? {
        Err(serde::de::Error::custom(
            "VERIFIED is unavailable for provided local state",
        ))
    } else {
        Ok(false)
    }
}
fn serialize_false<S: serde::Serializer>(
    value: &bool,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if *value {
        Err(serde::ser::Error::custom(
            "VERIFIED is unavailable for provided local state",
        ))
    } else {
        serializer.serialize_bool(false)
    }
}
