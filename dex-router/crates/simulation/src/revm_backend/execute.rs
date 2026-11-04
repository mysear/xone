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
    execute_mode(image, request, limits, control, false)
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
    execute_mode(image, request, limits, control, true)
}
fn execute_mode(
    image: Arc<StateImage>,
    request: TransactionRequest,
    limits: ExecutionLimits,
    control: ExecutionControl,
    observation: bool,
) -> Result<ExecutionOutput, ExecutionError> {
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
    if request.fork != SpecId::SHANGHAI {
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
    let mut evm = revm::Evm::builder()
        .with_db(db)
        .with_external_context(inspector)
        .with_spec_id(SpecId::SHANGHAI)
        .modify_cfg_env(|cfg| {
            cfg.chain_id = request.chain_id;
            cfg.memory_limit = limits.max_memory_bytes;
        })
        .with_block_env(request.block)
        .with_tx_env(request.tx)
        .append_handler_register(revm::inspector_handle_register)
        .build();
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
