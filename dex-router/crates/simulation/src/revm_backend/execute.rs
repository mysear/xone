use super::{DbError, ExecutionControl, ExecutionLimits, StateImage, Telemetry};
use revm::primitives::{BlockEnv, EVMError, ResultAndState, SpecId, TxEnv};
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct TransactionRequest {
    pub fork: SpecId,
    pub chain_id: u64,
    pub block: BlockEnv,
    pub tx: TxEnv,
}
#[derive(Debug)]
pub struct ExecutionOutput {
    pub result_and_state: ResultAndState,
    pub telemetry: Telemetry,
}
#[derive(Debug)]
pub enum ExecutionFailure {
    UnsupportedFork(SpecId),
    InvalidOpContext(&'static str),
    InvalidObservation,
    MissingNonce,
    MissingChainId,
    ChainIdMismatch,
    GasLimit { requested: u64, maximum: u64 },
    InputLimit { actual: usize, maximum: usize },
    InvalidMemoryLimit(u64),
    Evm(Box<EVMError<DbError>>),
}
#[derive(Debug)]
pub struct ExecutionError {
    pub failure: ExecutionFailure,
    pub telemetry: Telemetry,
}
impl ExecutionError {
    pub fn reason(&self) -> &'static str {
        match &self.failure {
            ExecutionFailure::UnsupportedFork(_) => "unsupported_fork",
            ExecutionFailure::InvalidOpContext(_) => "op_execution_context",
            ExecutionFailure::InvalidObservation => "invalid_observation",
            ExecutionFailure::MissingNonce => "missing_nonce",
            ExecutionFailure::MissingChainId => "missing_chain_id",
            ExecutionFailure::ChainIdMismatch => "chain_id_mismatch",
            ExecutionFailure::GasLimit { .. } => "gas_limit",
            ExecutionFailure::InputLimit { .. } => "input_limit",
            ExecutionFailure::InvalidMemoryLimit(_) => "invalid_memory_limit",
            ExecutionFailure::Evm(error) => match error.as_ref() {
                EVMError::Database(error) => error.reason(),
                EVMError::Transaction(_) => "transaction_validation",
                EVMError::Header(_) => "block_validation",
                _ => "evm_error",
            },
        }
    }
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.reason(), self.failure)
    }
}
impl std::error::Error for ExecutionError {}
/// Execute with ordinary REVM validation and fees. Guard failures return no usable state delta.
/// The image is not mutated. Chain/block identity and state provenance are the caller's responsibility.
pub fn execute(
    image: Arc<StateImage>,
    request: TransactionRequest,
    limits: ExecutionLimits,
    control: ExecutionControl,
) -> Result<ExecutionOutput, ExecutionError> {
    execute_mode(image, request, limits, control, false, None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpFork {
    Regolith,
    Canyon,
    Ecotone,
    Fjord,
    Granite,
    Holocene,
    Isthmus,
    Jovian,
    /// Scoped OP Mainnet ordinary calls with explicit Osaka adaptations.
    Karst,
}
impl OpFork {
    pub(crate) fn spec(self) -> SpecId {
        match self {
            Self::Regolith => SpecId::REGOLITH,
            Self::Canyon => SpecId::CANYON,
            Self::Ecotone => SpecId::ECOTONE,
            Self::Fjord => SpecId::FJORD,
            Self::Granite => SpecId::GRANITE,
            Self::Holocene => SpecId::HOLOCENE,
            Self::Isthmus | Self::Jovian | Self::Karst => SpecId::ISTHMUS,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Regolith => "op-regolith",
            Self::Canyon => "op-canyon",
            Self::Ecotone => "op-ecotone",
            Self::Fjord => "op-fjord",
            Self::Granite => "op-granite",
            Self::Holocene => "op-holocene",
            Self::Isthmus => "op-isthmus",
            Self::Jovian => "op-jovian",
            Self::Karst => "op-karst",
        }
    }
    pub(crate) fn at(chain: u64, time: u64) -> Option<Self> {
        let start = match chain {
            10 => 1_686_068_903,
            8453 => 1_686_789_347,
            _ => return None,
        };
        // The pinned registry bounds OP Mainnet Karst before Lagoon.
        // Base has no authenticated later schedule here. Reject activation
        // blocks: their attributes/bank require separate upgrade processing.
        let end = if chain == 10 {
            1_799_683_201
        } else {
            1_764_691_201
        };
        if time < start || time >= end {
            return None;
        }
        let mut result = Self::Regolith;
        for (timestamp, fork) in [
            (1_704_992_401, Self::Canyon),
            (1_710_374_401, Self::Ecotone),
            (1_720_627_201, Self::Fjord),
            (1_726_070_401, Self::Granite),
            (1_736_445_601, Self::Holocene),
            (1_746_806_401, Self::Isthmus),
            (1_764_691_201, Self::Jovian),
            (1_783_526_401, Self::Karst),
        ] {
            if time == timestamp {
                return None;
            }
            if time > timestamp {
                result = fork;
            }
        }
        Some(result)
    }
}

pub struct OpExecutionRequest {
    pub fork: OpFork,
    pub block: BlockEnv,
    pub transaction: crate::op_transaction::AuthenticatedUser,
    pub attributes_calldata: Vec<u8>,
}

#[derive(Debug)]
pub struct OpExecutionOutput {
    pub execution: ExecutionOutput,
    pub costs: xone_execution_preparation::op_chain_cost::ChainCost,
    pub payer_cost: xone_execution_preparation::op_chain_cost::PayerCost,
}

/// Historical regular-user OP execution over a strict provided state image.
/// The source owner still authenticates the block, state and L1Block slots.
/// A caller cannot swap VM transaction fields independently of the signed wire.
pub fn execute_op_user(
    image: Arc<StateImage>,
    request: OpExecutionRequest,
    limits: ExecutionLimits,
    control: ExecutionControl,
) -> Result<OpExecutionOutput, ExecutionError> {
    let tx = request.transaction.into_environment();
    let chain_id = tx.chain_id.unwrap();
    let time = u64::try_from(request.block.timestamp).ok();
    if time.and_then(|time| OpFork::at(chain_id, time)) != Some(request.fork)
        || !tx.value.is_zero()
        || !matches!(tx.transact_to, revm::primitives::TxKind::Call(_))
    {
        return Err(ExecutionError {
            failure: ExecutionFailure::InvalidOpContext("unsupported chain/fork/time/native flow"),
            telemetry: Telemetry::default(),
        });
    }
    let fee_tx = tx.clone();
    let beneficiary = request.block.coinbase;
    let base_fee = request.block.basefee;
    let execution = execute_mode(
        image.clone(),
        TransactionRequest {
            fork: request.fork.spec(),
            chain_id,
            block: request.block,
            tx,
        },
        limits,
        control.clone(),
        false,
        Some((&request.attributes_calldata, request.fork)),
    )?;
    let (costs, payer_cost) = super::op_fees::audit(
        &image,
        &execution,
        &fee_tx,
        beneficiary,
        base_fee,
        time.unwrap(),
        request.fork,
        &request.attributes_calldata,
    )
    .map_err(|failure| ExecutionError {
        failure,
        telemetry: execution.telemetry,
    })?;
    super::StrictDB::new(image, limits.reads, control)
        .check_control()
        .map_err(|error| ExecutionError {
            failure: ExecutionFailure::Evm(Box::new(EVMError::Database(error))),
            telemetry: execution.telemetry,
        })?;
    Ok(OpExecutionOutput {
        execution,
        costs,
        payer_cost,
    })
}

/// Bounded zero-value observation call. Preserve the real caller balance,
/// GASPRICE and nonce; no synthetic gas payment or nonce increment is applied.
/// This crate-private path discards every state delta and is never used for swaps.
pub(crate) fn observe(
    image: Arc<StateImage>,
    request: TransactionRequest,
    limits: ExecutionLimits,
    control: ExecutionControl,
) -> Result<ExecutionOutput, ExecutionError> {
    execute_mode(image, request, limits, control, true, None)
}

/// Internal read-only probes use the selected OP VM rules without paying fees
/// or changing nonce. They discard state and cannot produce swap evidence.
pub(crate) fn observe_op(
    image: Arc<StateImage>,
    request: TransactionRequest,
    fork: OpFork,
    limits: ExecutionLimits,
    control: ExecutionControl,
) -> Result<ExecutionOutput, ExecutionError> {
    if request.tx.optimism != Default::default()
        || request.fork != fork.spec()
        || u64::try_from(request.block.timestamp)
            .ok()
            .and_then(|time| OpFork::at(request.chain_id, time))
            != Some(fork)
    {
        return Err(ExecutionError {
            failure: ExecutionFailure::InvalidOpContext("OP observation context"),
            telemetry: Telemetry::default(),
        });
    }
    execute_mode(image, request, limits, control, true, Some((&[], fork)))
}
fn execute_mode(
    image: Arc<StateImage>,
    request: TransactionRequest,
    limits: ExecutionLimits,
    control: ExecutionControl,
    observation: bool,
    op: Option<(&[u8], OpFork)>,
) -> Result<ExecutionOutput, ExecutionError> {
    let optimism = op.is_some();
    let reject = |failure| ExecutionError {
        failure,
        telemetry: Telemetry::default(),
    };
    if observation
        && (!request.tx.value.is_zero()
            || !matches!(request.tx.transact_to, revm::primitives::TxKind::Call(_)))
    {
        return Err(reject(ExecutionFailure::InvalidObservation));
    }
    if !optimism && request.tx.optimism != Default::default() {
        return Err(reject(ExecutionFailure::InvalidOpContext(
            "OP fields on Shanghai path",
        )));
    }
    if !optimism && request.fork != SpecId::SHANGHAI {
        return Err(reject(ExecutionFailure::UnsupportedFork(request.fork)));
    }
    if request.tx.nonce.is_none() {
        return Err(reject(ExecutionFailure::MissingNonce));
    }
    let chain_id = request
        .tx
        .chain_id
        .ok_or_else(|| reject(ExecutionFailure::MissingChainId))?;
    if chain_id != request.chain_id {
        return Err(reject(ExecutionFailure::ChainIdMismatch));
    }
    if op.is_some_and(|(_, fork)| fork == OpFork::Karst) {
        super::karst_execution::validate_context(&request.tx, &request.block, observation)
            .map_err(reject)?;
    }
    if request.tx.gas_limit == 0 || request.tx.gas_limit > limits.max_gas_limit {
        return Err(reject(ExecutionFailure::GasLimit {
            requested: request.tx.gas_limit,
            maximum: limits.max_gas_limit,
        }));
    }
    if request.tx.data.len() > limits.max_input_bytes {
        return Err(reject(ExecutionFailure::InputLimit {
            actual: request.tx.data.len(),
            maximum: limits.max_input_bytes,
        }));
    }
    if limits.max_memory_bytes == 0 || limits.max_memory_bytes > 256 * 1024 * 1024 {
        return Err(reject(ExecutionFailure::InvalidMemoryLimit(
            limits.max_memory_bytes,
        )));
    }
    let inspector = BudgetInspector {
        max_instructions: limits.max_instructions,
        telemetry: Telemetry::default(),
        progress: control.progress.clone(),
    };
    let mut db = super::StrictDB::new(image, limits.reads, control);
    db.check_control()
        .map_err(|error| reject(ExecutionFailure::Evm(Box::new(EVMError::Database(error)))))?;
    if let Some((attributes, fork)) = op.filter(|_| !observation) {
        super::op_fees::validate_bank(&mut db, attributes, fork, &request.tx, &request.block)
            .map_err(|failure| ExecutionError {
                failure,
                telemetry: Telemetry {
                    reads: db.usage(),
                    ..Default::default()
                },
            })?;
    }
    let builder = revm::Evm::builder()
        .with_db(db)
        .with_external_context(inspector)
        .with_spec_id(request.fork)
        .modify_cfg_env(|cfg| {
            cfg.chain_id = request.chain_id;
            cfg.memory_limit = limits.max_memory_bytes;
        })
        .with_block_env(request.block)
        .with_tx_env(request.tx);
    let builder = if optimism {
        builder.optimism()
    } else {
        builder.with_handler_cfg(revm::primitives::HandlerCfg::new_with_optimism(
            request.fork,
            false,
        ))
    };
    let karst = op.is_some_and(|(_, fork)| fork == OpFork::Karst);
    let mut evm = builder
        .append_handler_register_box(Box::new(move |handler| {
            if karst {
                handler
                    .instruction_table
                    .insert(0x1e, super::karst_execution::clz);
            }
        }))
        .append_handler_register(revm::inspector_handle_register)
        .build();
    if let Some((_, fork)) = op {
        evm.handler.pre_execution.load_precompiles =
            Arc::new(move || super::op_precompiles::load::<super::StrictDB, BudgetInspector>(fork));
    }
    if op.is_some_and(|(_, fork)| matches!(fork, OpFork::Jovian | OpFork::Karst)) && !observation {
        evm.handler.validation.tx_against_state = Arc::new(|context| {
            revm::optimism::validate_tx_against_state::<
                revm::primitives::IsthmusSpec,
                BudgetInspector,
                super::StrictDB,
            >(context)?;
            // Jovian charges raw_scalar * gas * 100. The pinned engine's
            // Isthmus handler divides by 1e6, so scale only its private cache.
            // Source slot8 remains untouched; validate_bank already checked
            // the true Jovian maximum funding using independent checked math.
            let info = context
                .evm
                .inner
                .l1_block_info
                .as_mut()
                .expect("validated OP bank");
            info.operator_fee_scalar = info
                .operator_fee_scalar
                .map(|scalar| scalar * revm::primitives::U256::from(100_000_000));
            Ok(())
        });
    }
    if observation {
        // These hooks apply only to read observations, never actual execution.
        // Do not fund the caller or rewrite the gas price: contracts may inspect
        // both. Keep strict account/code admission and all execution guards.
        evm.handler.validation.tx_against_state = Arc::new(|context| {
            let caller = context.evm.env.tx.caller;
            context
                .evm
                .inner
                .journaled_state
                .load_code(caller, &mut context.evm.inner.db)?;
            Ok(())
        });
        evm.handler.pre_execution.deduct_caller = Arc::new(|_| Ok(()));
        evm.handler.post_execution.reimburse_caller = Arc::new(|_, _| Ok(()));
        evm.handler.post_execution.reward_beneficiary = Arc::new(|_, _| Ok(()));
    }
    let result = evm.transact();
    // Critical: guards stay fatal even if an inner CALL or engine handler produced a receipt.
    // Also catches cancellation/deadline arriving during an uninterruptible native precompile.
    let final_guard = evm.context.evm.db.check_control();
    let mut telemetry = evm.context.external.telemetry;
    telemetry.reads = evm.context.evm.db.usage();
    if let Err(error) = final_guard {
        return Err(ExecutionError {
            failure: ExecutionFailure::Evm(Box::new(EVMError::Database(error))),
            telemetry,
        });
    }
    result
        .map(|mut result_and_state| {
            if observation {
                result_and_state.state.clear();
            }
            ExecutionOutput {
                result_and_state,
                telemetry,
            }
        })
        .map_err(|error| ExecutionError {
            failure: ExecutionFailure::Evm(Box::new(error)),
            telemetry,
        })
}

use super::StrictDB;
use revm::{
    interpreter::{
        CallInputs, CallOutcome, CreateInputs, CreateOutcome, Gas, InstructionResult, Interpreter,
        InterpreterResult,
    },
    EvmContext, Inspector,
};
use std::sync::atomic::{AtomicU64, Ordering};
struct BudgetInspector {
    max_instructions: u64,
    telemetry: Telemetry,
    progress: Option<Arc<AtomicU64>>,
}
impl BudgetInspector {
    fn boundary(&mut self, context: &mut EvmContext<StrictDB>) -> Result<(), DbError> {
        context.db.check_control()?;
        if self.telemetry.instructions >= self.max_instructions {
            return Err(context.db.fail(DbError::InstructionLimit));
        }
        Ok(())
    }
    fn fatal(
        context: &mut EvmContext<StrictDB>,
        error: DbError,
        gas_limit: u64,
    ) -> InterpreterResult {
        let error = context.db.fail(error);
        context.error = Err(EVMError::Database(error));
        InterpreterResult {
            result: InstructionResult::FatalExternalError,
            gas: Gas::new(gas_limit),
            output: revm::primitives::Bytes::new(),
        }
    }
}
impl Inspector<StrictDB> for BudgetInspector {
    fn initialize_interp(&mut self, interp: &mut Interpreter, context: &mut EvmContext<StrictDB>) {
        self.telemetry.initializations += 1;
        if let Err(error) = self.boundary(context) {
            Self::fatal(context, error, 0);
            interp.instruction_result = InstructionResult::FatalExternalError;
        }
    }
    fn step(&mut self, interp: &mut Interpreter, context: &mut EvmContext<StrictDB>) {
        if let Err(error) = self.boundary(context) {
            Self::fatal(context, error, 0);
            interp.instruction_result = InstructionResult::FatalExternalError;
            return;
        }
        self.telemetry.instructions += 1;
        if let Some(progress) = &self.progress {
            progress.store(self.telemetry.instructions, Ordering::Release);
        }
    }
    fn step_end(&mut self, interp: &mut Interpreter, context: &mut EvmContext<StrictDB>) {
        // A nested CALL must not swallow an executor memory-policy failure.
        if interp.instruction_result == InstructionResult::MemoryLimitOOG {
            Self::fatal(context, DbError::MemoryLimit, 0);
            interp.instruction_result = InstructionResult::FatalExternalError;
        }
    }
    fn call(
        &mut self,
        context: &mut EvmContext<StrictDB>,
        inputs: &mut CallInputs,
    ) -> Option<CallOutcome> {
        self.telemetry.calls += 1;
        self.boundary(context).err().map(|error| {
            CallOutcome::new(
                Self::fatal(context, error, inputs.gas_limit),
                inputs.return_memory_offset.clone(),
            )
        })
    }
    fn create(
        &mut self,
        context: &mut EvmContext<StrictDB>,
        inputs: &mut CreateInputs,
    ) -> Option<CreateOutcome> {
        self.telemetry.creates += 1;
        self.boundary(context)
            .err()
            .map(|error| CreateOutcome::new(Self::fatal(context, error, inputs.gas_limit), None))
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use crate::revm_backend::{BoundReadLimits, ImageData};
    use revm::primitives::{keccak256, AccountInfo, Address, Bytes, TxKind, KECCAK_EMPTY, U256};

    fn setup(code: &[u8]) -> (Arc<StateImage>, TransactionRequest, ExecutionLimits) {
        let caller = Address::repeat_byte(0x11);
        let target = Address::repeat_byte(0x22);
        let hash = keccak256(code);
        let mut data = ImageData::default();
        data.accounts.insert(
            caller,
            Some(AccountInfo {
                balance: U256::from(123456),
                nonce: 7,
                code_hash: KECCAK_EMPTY,
                code: None,
            }),
        );
        data.accounts.insert(
            target,
            Some(AccountInfo {
                balance: U256::ZERO,
                nonce: 1,
                code_hash: hash,
                code: None,
            }),
        );
        data.bytecodes.insert(hash, Bytes::copy_from_slice(code));
        let image = Arc::new(StateImage::new(data).unwrap());
        let request = TransactionRequest {
            fork: SpecId::SHANGHAI,
            chain_id: 31337,
            block: BlockEnv {
                prevrandao: Some(Default::default()),
                basefee: U256::from(1),
                gas_limit: U256::from(1000000),
                ..Default::default()
            },
            tx: TxEnv {
                caller,
                transact_to: TxKind::Call(target),
                gas_limit: 200000,
                gas_price: U256::from(2_000_000_000u64),
                nonce: Some(7),
                chain_id: Some(31337),
                ..Default::default()
            },
        };
        let limits = ExecutionLimits {
            reads: BoundReadLimits {
                max_reads: 100,
                max_bytes: 100000,
            },
            max_gas_limit: 200000,
            max_instructions: 1000,
            max_input_bytes: 1024,
            max_memory_bytes: 65536,
        };
        (image, request, limits)
    }

    #[test]
    fn observation_preserves_real_caller_balance_gasprice_nonce_without_prepayment() {
        // Return CALLER.balance and GASPRICE from the unchanged image/environment.
        let (image, request, limits) = setup(&[
            0x33, 0x31, 0x5f, 0x52, 0x3a, 0x60, 0x20, 0x52, 0x60, 0x40, 0x5f, 0xf3,
        ]);
        let out = observe(
            image.clone(),
            request.clone(),
            limits,
            ExecutionControl::default(),
        )
        .unwrap();
        let bytes = out.result_and_state.result.output().unwrap();
        assert_eq!(U256::from_be_slice(&bytes[..32]), U256::from(123456));
        assert_eq!(U256::from_be_slice(&bytes[32..]), request.tx.gas_price);
        assert!(
            out.result_and_state.state.is_empty(),
            "observation deltas must never be reusable"
        );
        let actual = execute(image, request, limits, ExecutionControl::default()).unwrap_err();
        assert_eq!(
            actual.reason(),
            "transaction_validation",
            "actual transaction must retain upfront fee validation"
        );
    }

    #[test]
    fn observation_retains_instruction_and_cancellation_guards() {
        let (image, request, mut limits) = setup(&[0x5b, 0x5f, 0x56]);
        limits.max_instructions = 10;
        assert_eq!(
            observe(
                image.clone(),
                request.clone(),
                limits,
                ExecutionControl::default()
            )
            .unwrap_err()
            .reason(),
            "instruction_limit"
        );
        let control = ExecutionControl::default();
        control.cancellation.store(true, Ordering::Release);
        assert_eq!(
            observe(image, request, limits, control)
                .unwrap_err()
                .reason(),
            "cancelled"
        );
    }

    #[test]
    fn op_observation_preserves_native_context_without_l1_fee_or_nonce_payment() {
        for (fork, time) in [
            (OpFork::Regolith, 1_686_068_904u64),
            (OpFork::Isthmus, 1_746_806_403),
        ] {
            // CALLER.balance and GASPRICE; Cancun+ additionally BLOBBASEFEE.
            let mut code = vec![0x33, 0x31, 0x60, 0, 0x52, 0x3a, 0x60, 0x20, 0x52];
            if fork == OpFork::Isthmus {
                code.extend([0x4a, 0x60, 0x40, 0x52]);
            }
            let width = if fork == OpFork::Isthmus { 96 } else { 64 };
            code.extend([0x60, width, 0x60, 0, 0xf3]);
            let (image, mut request, limits) = setup(&code);
            request.fork = fork.spec();
            request.chain_id = 10;
            request.tx.chain_id = Some(10);
            request.block.timestamp = U256::from(time);
            request.block.set_blob_excess_gas_and_price(0, false);
            let original = image.clone();
            let out =
                observe_op(image, request, fork, limits, ExecutionControl::default()).unwrap();
            let bytes = out.result_and_state.result.output().unwrap();
            assert_eq!(bytes.len(), width as usize);
            assert_eq!(U256::from_be_slice(&bytes[..32]), U256::from(123456));
            assert_eq!(
                U256::from_be_slice(&bytes[32..64]),
                U256::from(2_000_000_000u64)
            );
            if fork == OpFork::Isthmus {
                assert_eq!(U256::from_be_slice(&bytes[64..]), U256::from(1));
            }
            assert!(out.result_and_state.state.is_empty());
            assert_eq!(
                original.data().accounts[&Address::repeat_byte(0x11)]
                    .as_ref()
                    .unwrap()
                    .nonce,
                7
            );
        }
    }
    #[test]
    fn observation_cannot_create_or_transfer_value() {
        let (image, mut request, limits) = setup(&[0x00]);
        request.tx.value = U256::from(1);
        assert_eq!(
            observe(
                image.clone(),
                request.clone(),
                limits,
                ExecutionControl::default()
            )
            .unwrap_err()
            .reason(),
            "invalid_observation"
        );
        request.tx.value = U256::ZERO;
        request.tx.transact_to = TxKind::Create;
        assert_eq!(
            observe(image, request, limits, ExecutionControl::default())
                .unwrap_err()
                .reason(),
            "invalid_observation"
        );
    }
}

#[cfg(test)]
mod jovian_schedule_tests {
    use super::OpFork;
    #[test]
    fn jovian_admission_uses_only_the_pinned_op_interval_and_rejects_upgrade_blocks() {
        assert_eq!(OpFork::at(10, 1_764_691_200), Some(OpFork::Isthmus));
        assert_eq!(OpFork::at(10, 1_764_691_201), None);
        assert_eq!(OpFork::at(10, 1_764_691_202), Some(OpFork::Jovian));
        assert_eq!(OpFork::at(10, 1_783_526_400), Some(OpFork::Jovian));
        assert_eq!(OpFork::at(10, 1_783_526_401), None);
        assert_eq!(OpFork::at(10, 1_799_999_999), None);
        for chain in [8453, 1, 42161] {
            assert_eq!(OpFork::at(chain, 1_764_691_202), None);
        }
    }
}

#[cfg(test)]
#[path = "karst_execution_tests.rs"]
mod karst_execution_tests;
