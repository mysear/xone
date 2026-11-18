//! Bind source attributes to the exact strict VM bank, then reconcile actual
//! fee-account deltas with the shared checked arithmetic after execution.
use super::{ExecutionFailure, ExecutionOutput, OpFork, StateImage, StrictDB};
use revm::{
    primitives::{Address, BlockEnv, EVMError, TxEnv, U256},
    Database,
};
use std::collections::BTreeMap;
use xone_domain::Amount;
use xone_execution_preparation::{op_chain_cost as cost, op_l1_cost, op_packed_cost};

fn bad() -> ExecutionFailure {
    ExecutionFailure::InvalidOpContext("attributes/bank/fee account mismatch")
}
fn l1() -> Address {
    Address::from_slice(&hex::decode("4200000000000000000000000000000000000015").unwrap())
}
pub(super) fn accounts(tx: &TxEnv, beneficiary: Address) -> [Address; 5] {
    [
        tx.caller,
        beneficiary,
        Address::from(cost::BASE_FEE_VAULT),
        Address::from(cost::L1_FEE_VAULT),
        Address::from(cost::OPERATOR_FEE_VAULT),
    ]
}
fn read_error(e: super::DbError) -> ExecutionFailure {
    ExecutionFailure::Evm(Box::new(EVMError::Database(e)))
}
fn amount(n: U256) -> Amount {
    Amount::from_be_bytes(&n.to_be_bytes::<32>()).unwrap()
}
fn rollup<'a>(
    fork: OpFork,
    old: &'a Option<op_l1_cost::Parameters>,
    packed: &'a Option<op_packed_cost::PackedParameters>,
) -> cost::RollupInputs<'a> {
    if let Some(old) = old {
        return cost::RollupInputs::Regolith(old);
    }
    let rules = match fork {
        OpFork::Ecotone => cost::PackedRules::Ecotone,
        OpFork::Isthmus => cost::PackedRules::Isthmus,
        _ => cost::PackedRules::Fjord,
    };
    cost::RollupInputs::Packed {
        parameters: packed.as_ref().unwrap(),
        rules,
    }
}
fn parameters(
    fork: OpFork,
    chain: u64,
    time: u64,
    a: &[u8],
) -> Result<
    (
        Option<op_l1_cost::Parameters>,
        Option<op_packed_cost::PackedParameters>,
    ),
    ExecutionFailure,
> {
    if matches!(fork, OpFork::Regolith | OpFork::Canyon) {
        return Ok((
            Some(
                op_l1_cost::decode_parameters(&format!("evm:{chain}"), time, a)
                    .map_err(|_| bad())?,
            ),
            None,
        ));
    }
    let (selector, len) = if fork == OpFork::Isthmus {
        ([0x09, 0x89, 0x99, 0xbe], 176)
    } else {
        ([0x44, 0x0a, 0x5e, 0x20], 164)
    };
    if a.len() != len
        || a[..4] != selector
        || u64::from_be_bytes(a[20..28].try_into().unwrap()) > time
    {
        return Err(bad());
    }
    Ok((
        None,
        Some(op_packed_cost::decode_packed_attributes(a).map_err(|_| bad())?),
    ))
}

pub(super) fn validate_bank(
    db: &mut StrictDB,
    a: &[u8],
    fork: OpFork,
    tx: &TxEnv,
    block: &BlockEnv,
) -> Result<(), ExecutionFailure> {
    let time = u64::try_from(block.timestamp).map_err(|_| bad())?;
    let costs = calculate(fork, tx, block.basefee, time, a, tx.gas_limit)?;
    if db.basic(l1()).map_err(read_error)?.is_none() {
        return Err(bad());
    }
    let mut expected = Vec::new();
    if matches!(fork, OpFork::Regolith | OpFork::Canyon) {
        for (slot, start) in [(1, 68), (5, 196), (6, 228)] {
            expected.push((slot, U256::from_be_slice(&a[start..start + 32])));
        }
    } else {
        expected.push((1, U256::from_be_slice(&a[36..68])));
        expected.push((7, U256::from_be_slice(&a[68..100])));
        // Slot3 also contains the sequence number. Compare the two scalars,
        // not unrelated bits which are not fee inputs to this execution.
        let scalars = db
            .storage(l1(), U256::from(3))
            .map_err(read_error)?
            .to_be_bytes::<32>();
        if scalars[16..24] != a[4..12] {
            return Err(bad());
        }
        if fork == OpFork::Isthmus {
            let op = db
                .storage(l1(), U256::from(8))
                .map_err(read_error)?
                .to_be_bytes::<32>();
            if op[20..32] != a[164..176] {
                return Err(bad());
            }
        }
        // With all Ecotone fields unset the upstream falls back to Bedrock;
        // that first-block semantics is outside regular packed-user scope.
        if a[68..100].iter().all(|b| *b == 0) && a[4..12].iter().all(|b| *b == 0) {
            return Err(bad());
        }
    }
    for (slot, value) in expected {
        if db.storage(l1(), U256::from(slot)).map_err(read_error)? != value {
            return Err(bad());
        }
    }
    for address in accounts(tx, block.coinbase) {
        let account = db.basic(address).map_err(read_error)?;
        if address == tx.caller
            && amount(account.as_ref().map_or(U256::ZERO, |a| a.balance)) < *costs.maximum_funding()
        {
            return Err(ExecutionFailure::InvalidOpContext(
                "insufficient OP maximum funding",
            ));
        }
    }
    Ok(())
}

pub(super) fn calculate(
    fork: OpFork,
    tx: &TxEnv,
    base: U256,
    time: u64,
    attributes: &[u8],
    gas_used: u64,
) -> Result<cost::ChainCost, ExecutionFailure> {
    let (old, packed) = parameters(fork, tx.chain_id.unwrap(), time, attributes)?;
    let base = amount(base);
    let maximum = amount(tx.gas_price);
    let priority = tx.gas_priority_fee.map(amount);
    let value = amount(tx.value);
    let wire = tx.optimism.enveloped_tx.as_ref().ok_or_else(bad)?;
    cost::fee_only_cost(
        &cost::ExecutionInputs {
            gas_used,
            gas_limit: tx.gas_limit,
            base_fee: &base,
            maximum_gas_price: &maximum,
            priority_gas_price: priority.as_ref(),
            value: &value,
        },
        rollup(fork, &old, &packed),
        wire,
        wire.len(),
    )
    .map_err(|_| bad())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn audit(
    image: &StateImage,
    output: &ExecutionOutput,
    tx: &TxEnv,
    beneficiary: Address,
    base: U256,
    time: u64,
    fork: OpFork,
    attributes: &[u8],
) -> Result<(cost::ChainCost, cost::PayerCost), ExecutionFailure> {
    let costs = calculate(
        fork,
        tx,
        base,
        time,
        attributes,
        output.result_and_state.result.gas_used(),
    )?;
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    for address in accounts(tx, beneficiary) {
        let known = image.data().accounts.get(&address).ok_or_else(bad)?;
        let initial = known.as_ref().map_or(U256::ZERO, |a| a.balance);
        let final_balance = output
            .result_and_state
            .state
            .get(&address)
            .map_or(initial, |a| a.info.balance);
        before.insert(address.into_array(), amount(initial));
        after.insert(address.into_array(), amount(final_balance));
    }
    for (address, account) in &output.result_and_state.state {
        if before.contains_key(&address.into_array()) {
            continue;
        }
        let initial = image
            .data()
            .accounts
            .get(address)
            .ok_or_else(bad)?
            .as_ref()
            .map_or(U256::ZERO, |a| a.balance);
        if account.info.balance != initial {
            return Err(bad());
        }
    }
    let payer = costs
        .audit_fee_only_balances(
            tx.caller.into_array(),
            beneficiary.into_array(),
            &before,
            &after,
        )
        .map_err(|_| bad())?;
    Ok((costs, payer))
}
