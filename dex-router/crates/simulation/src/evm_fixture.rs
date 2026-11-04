//! Historical replay of PROVIDED_LOCAL_STATE, never a canonicality or VERIFIED claim.
//! Source H and execution H+1 are distinct. Probes discard their state; only swap effects commit.
use crate::revm_backend as core;
use revm::primitives::{
    keccak256, AccountInfo, Address, BlockEnv, Bytes, ExecutionResult, SpecId, TxEnv, TxKind, B256,
    U256,
};
use revm::Database;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{atomic::Ordering, Arc},
    time::Instant,
};
use xone_domain::{Amount, Pool};
use xone_execution_plan::{validate_bound_execution, BoundEvmExecution};
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
    pub gas_price: Amount,
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
    /// This wrapper supports historical H -> H+1 replay only, not an implicit mix of contexts.
    pub fn validate_environment(&self, bound: &BoundEvmExecution) -> Result<()> {
        let source = &self.state_anchor;
        let block = &bound.plan.snapshot.block;
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
            || env.fork != "shanghai"
            || source.block_number.checked_add(1) != Some(env.block_number)
            || env.parent_hash != source.block_hash
            || env.timestamp < source.timestamp
            || env.block_hash == source.block_hash
            || env.gas_limit == 0
        {
            return Err(failure(
                SimulationEnvironmentMismatch,
                "environment",
                "require Shanghai H+1 with parent H and nondecreasing timestamp",
            ));
        }
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationOutcome {
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
    /// Covered exact-input adapter has no separately modeled refund leg; matching requires full input consumption.
    pub refund: Amount,
    pub actual_buy_amount: Amount,
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
}
impl Runner<'_> {
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
        let request = core::TransactionRequest {
            fork: SpecId::SHANGHAI,
            chain_id: env.chain_id,
            block: BlockEnv {
                number: U256::from(env.block_number),
                coinbase: address(&env.coinbase)?,
                timestamp: U256::from(env.timestamp),
                gas_limit: U256::from(env.gas_limit),
                basefee: amount(&env.base_fee)?,
                prevrandao: Some(B256::from(fixed::<32>(&env.prevrandao)?)),
                blob_excess_gas_and_price: None,
                ..BlockEnv::default()
            },
            tx,
        };
        let engine = if probe { core::observe } else { core::execute };
        let result = engine(
            image,
            request,
            core::ExecutionLimits {
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
            },
            self.control.clone(),
        );
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
        bound: &BoundEvmExecution,
        pool: &Pool,
    ) -> Result<()> {
        let executor = &bound.executor;
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
    control_check(&control, "entry")?;
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
    validate_bound_execution(bound, pools, allowlist)
        .map_err(|e| failure(SimulationBindingMismatch, "binding", e))?;
    fixture.validate_shape(budget)?;
    fixture.validate_environment(bound)?;
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
    amount(&tx.gas_price)?;
    let image = fixture.to_image(budget)?;
    let pool_id = bound
        .touched_pool_ids
        .first()
        .ok_or_else(|| failure(SimulationBindingMismatch, "pool", "selected pool missing"))?;
    let pool = pools
        .iter()
        .find(|p| &p.pool_id == pool_id)
        .ok_or_else(|| failure(SimulationBindingMismatch, "pool", "selected pool missing"))?;
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
    code_check(&image, &bound.executor.target, &bound.executor.code_sha256)?;
    code_check(
        &image,
        &bound.executor.factory,
        &bound.executor.factory_code_sha256,
    )?;
    code_check(
        &image,
        &pool.pool_id,
        &pool
            .v2
            .as_ref()
            .ok_or_else(|| failure(SimulationBindingMismatch, "pool", "V2 identity required"))?
            .code_sha256,
    )?;
    let source_fixture_hash = canonical_hash(fixture)?;
    let context = &bound.plan.context;
    let caller = address(&bound.transaction.from)?;
    let mut runner = Runner {
        budget,
        env: &fixture.execution_environment,
        tx,
        control,
        usage: BudgetUsage::default(),
        observations: vec![],
        caller_address: caller,
    };
    runner.preflight(image.clone(), bound, pool)?;
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
    let output = runner.run(
        image.clone(),
        TxEnv {
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
            gas_price: amount(&tx.gas_price)?,
            ..TxEnv::default()
        },
        "swap",
        false,
    )?;
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
    if pool.protocol == xone_domain::v2_swap::PROFILE {
        runner.complete_poststate(post.clone(), pool)?;
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
    let matches_plan = status == EvmStatus::Success
        && spent == amount(&bound.plan.allocation.sell_amount)?
        && bought == amount(&bound.plan.allocation.gross_buy_amount)?
        && bought >= amount(&bound.plan.min_buy_amount)?;
    let mut outcome = SimulationOutcome {
        verified: false,
        authority: DataAuthority::ProvidedLocalState,
        plan_validation: "BOUND_REVALIDATED".into(),
        execution: EvmReceipt {
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
        actual_buy_amount: as_amount(bought)?,
        observations: runner.observations,
        usage: runner.usage,
        source_fixture_hash,
        simulator_version: SIMULATOR_VERSION.into(),
        receipt_hash: String::new(),
    };
    let mut value =
        serde_json::to_value(&outcome).map_err(|e| failure(SimulationInvalidFixture, "hash", e))?;
    if let Some(map) = value.as_object_mut() {
        map.remove("receipt_hash");
    }
    outcome.receipt_hash = canonical_hash(
        &serde_json::json!({"bound":bound,"source_fixture_hash":outcome.source_fixture_hash,"transaction_context":transaction_context,"limits":budget,"simulator_version":SIMULATOR_VERSION,"outcome":value}),
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
