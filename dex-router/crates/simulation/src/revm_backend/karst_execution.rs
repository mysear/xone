//! Karst execution additions on the locked engine. Register before inspectors
//! so CLZ remains subject to the same instruction/control budgets as every opcode.
use revm::interpreter::{Host, InstructionResult, Interpreter};
use revm::primitives::U256;

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
