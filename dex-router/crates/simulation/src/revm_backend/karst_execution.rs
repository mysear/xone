//! Karst execution additions on the locked engine. Register before inspectors
//! so CLZ remains subject to the same instruction/control budgets as every opcode.
use super::ExecutionFailure;
use revm::interpreter::{Host, InstructionResult, Interpreter};
use revm::primitives::{BlockEnv, TxEnv, U256};

pub(super) const TX_GAS_LIMIT_CAP: u64 = 1 << 24;

pub(super) fn clz<H: Host>(interp: &mut Interpreter, _: &mut H) {
    if !interp.gas.record_cost(5) {
        interp.instruction_result = InstructionResult::OutOfGas;
        return;
    }
    let value = match interp.stack.pop() {
        Ok(value) => value,
        Err(error) => {
            interp.instruction_result = error;
            return;
        }
    };
    if let Err(error) = interp.stack.push(U256::from(value.leading_zeros())) {
        interp.instruction_result = error;
    }
}

// Keep the admitted interface within the exact representation exercised by the
// fixed full original OpEvm. Reject wider fields instead of narrowing them.
pub(super) fn validate_context(
    tx: &TxEnv,
    block: &BlockEnv,
    observation: bool,
) -> Result<(), ExecutionFailure> {
    if !observation && tx.gas_limit > TX_GAS_LIMIT_CAP {
        return Err(ExecutionFailure::GasLimit {
            requested: tx.gas_limit,
            maximum: TX_GAS_LIMIT_CAP,
        });
    }
    if u64::try_from(block.basefee).is_err()
        || u64::try_from(block.gas_limit).is_err()
        || u128::try_from(tx.gas_price).is_err()
        || tx
            .gas_priority_fee
            .is_some_and(|p| u128::try_from(p).is_err())
    {
        return Err(ExecutionFailure::InvalidOpContext(
            "Karst original interface width",
        ));
    }
    Ok(())
}
