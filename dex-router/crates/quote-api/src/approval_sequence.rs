//! Rebuild each approval from the plan and audit fee-account continuity from the bank.
use std::collections::{BTreeMap, BTreeSet};
use xone_domain::{err, hash, Amount, Result};
use xone_execution_plan::{ordered_v2, Transaction};
use xone_execution_preparation::evm_native_cost::{
    audited_payer_cost, measured_native_cost, NativeBalances, NativeCost, PayerCost,
};
use xone_simulation::{
    evm_fixture::{
        ApprovalReceipt, ApprovalSequence, ExecutionEnvironment, FixtureState,
        NativeFeeMeasurements, TransactionContext,
    },
    worker::{LocalSimulationBundle, WorkerRequest},
};

pub(super) struct Commitment {
    steps: Vec<(Transaction, TransactionContext, ExecutionEnvironment)>,
    balances: BTreeMap<String, Amount>,
    swap: ExecutionEnvironment,
}

fn schedule<'a>(
    fixture: &'a FixtureState,
    seq: &'a ApprovalSequence,
    swap: &'a TransactionContext,
    caller: &str,
    gas_cap: u64,
) -> Result<Vec<(&'a TransactionContext, &'a ExecutionEnvironment)>> {
    let bad = || err("SIMULATION_APPROVAL_SEQUENCE_BINDING");
    let first = &fixture.execution_environment;
    let exact_env = seq
        .reset
        .as_ref()
        .map_or(first, |r| &r.approval_environment);
    let mut schedule = Vec::with_capacity(3);
    if let Some(reset) = &seq.reset {
        schedule.push((&reset.context, first));
    }
    schedule.push((&seq.approval_context, exact_env));
    schedule.push((swap, &seq.swap_environment));
    let source = &fixture.state_anchor;
    let mut previous = (
        source.block_number,
        source.block_hash.as_str(),
        source.timestamp,
    );
    let mut hashes = BTreeSet::from([source.block_hash.as_str()]);
    let mut nonce = fixture
        .accounts
        .iter()
        .find(|a| a.exists && a.address == caller)
        .ok_or_else(bad)?
        .nonce;
    for &(tx, env) in &schedule {
        if env.fork != "shanghai"
            || env.chain_id != first.chain_id
            || tx.chain_id != env.chain_id
            || previous.0.checked_add(1) != Some(env.block_number)
            || env.parent_hash != previous.1
            || !hashes.insert(&env.block_hash)
            || env.timestamp <= previous.2
            || env.timestamp.checked_mul(1000).is_none()
            || tx.nonce != nonce
            || tx.gas_limit > gas_cap.min(env.gas_limit)
            || tx.max_priority_fee_per_gas.is_none()
        {
            return Err(bad());
        }
        measured_native_cost(
            21_000,
            tx.gas_limit,
            &env.base_fee,
            &tx.gas_price,
            tx.max_priority_fee_per_gas.as_ref(),
        )?;
        previous = (env.block_number, &env.block_hash, env.timestamp);
        nonce = nonce.checked_add(1).ok_or_else(bad)?;
    }
    Ok(schedule)
}

pub(super) fn validate_bundle(bundle: &LocalSimulationBundle, gas_cap: u64) -> Result<()> {
    let Some(model) = &bundle.approval_replay else {
        return Ok(());
    };
    use sha2::{Digest, Sha256};
    let bad = || err("SIMULATION_APPROVAL_SEQUENCE_BINDING");
    let token = bundle
        .fixture
        .accounts
        .iter()
        .find(|a| a.exists && a.address == model.token)
        .ok_or_else(bad)?;
    let code = hex::decode(token.code.strip_prefix("0x").ok_or_else(bad)?).map_err(|_| bad())?;
    if bundle.split_executor.is_none()
        || bundle.op_attributes_calldata.is_some()
        || code.is_empty()
        || hex::encode(Sha256::digest(code)) != model.sequence.token_code_sha256
    {
        return Err(bad());
    }
    schedule(
        &bundle.fixture,
        &model.sequence,
        &model.swap_context,
        &model.caller,
        gas_cap,
    )?;
    Ok(())
}

impl Commitment {
    pub fn settlement_ms(&self) -> u64 {
        self.swap.timestamp * 1000
    }
    pub fn requires_reset(&self) -> bool {
        self.steps.len() == 2
    }
    pub fn new(request: &WorkerRequest) -> Result<Option<Self>> {
        let Some((split, seq)) = request
            .split
            .as_ref()
            .and_then(|s| Some((s, s.approval_sequence.as_ref()?)))
        else {
            return Ok(None);
        };
        let bad = || err("SIMULATION_APPROVAL_SEQUENCE_BINDING");
        let exact = ordered_v2::caller_approval(
            &split.bound,
            &request.pools,
            &split.expected_executor,
            &request.allowed_targets,
            &Amount::zero(),
        )?
        .ok_or_else(bad)?;
        let schedule = schedule(
            &request.fixture,
            seq,
            &request.transaction_context,
            &exact.from,
            request.budget.max_transaction_gas,
        )?;
        let mut balances = BTreeMap::new();
        for addr in std::iter::once(&exact.from).chain(schedule.iter().map(|(_, e)| &e.coinbase)) {
            let a = request
                .fixture
                .accounts
                .iter()
                .find(|a| a.address == *addr)
                .ok_or_else(bad)?;
            balances.insert(
                addr.clone(),
                if a.exists {
                    a.balance.clone()
                } else {
                    Amount::zero()
                },
            );
        }
        let mut operations = Vec::new();
        if seq.reset.is_some() {
            // Keep the exact bound selector and spender. Only the uint256 amount is cleared.
            let mut zero = exact.clone();
            zero.data.replace_range(74.., &"0".repeat(64));
            operations.push(zero);
        }
        operations.push(exact);
        let steps = operations
            .into_iter()
            .zip(schedule)
            .map(|(op, (tx, env))| (op, tx.clone(), env.clone()))
            .collect();
        Ok(Some(Self {
            steps,
            balances,
            swap: seq.swap_environment.clone(),
        }))
    }
    pub fn audit(
        &self,
        receipts: &[ApprovalReceipt],
        swap: &NativeFeeMeasurements,
    ) -> Result<Vec<(NativeCost, PayerCost)>> {
        let bad = || err("SIMULATION_APPROVAL_SEQUENCE_RECEIPT");
        if receipts.len() != self.steps.len() {
            return Err(bad());
        }
        let mut balances = self.balances.clone();
        let mut costs = Vec::new();
        for (receipt, (expected, tx, env)) in receipts.iter().zip(&self.steps) {
            let model = measured_native_cost(
                receipt.gas_used,
                tx.gas_limit,
                &env.base_fee,
                &tx.gas_price,
                tx.max_priority_fee_per_gas.as_ref(),
            )?;
            if hash(&receipt.transaction)? != hash(expected)?
                || receipt.block_hash != env.block_hash
                || receipt.nonce != tx.nonce
                || model.execution_fee != receipt.execution_fee
                || balances.get(&expected.from) != Some(&receipt.payer_before)
                || balances.get(&env.coinbase) != Some(&receipt.beneficiary_before)
            {
                return Err(bad());
            }
            let payer = audited_payer_cost(
                &model,
                &NativeBalances {
                    payer_before: receipt.payer_before.clone(),
                    payer_after: receipt.payer_after.clone(),
                    beneficiary_before: receipt.beneficiary_before.clone(),
                    beneficiary_after: receipt.beneficiary_after.clone(),
                },
                expected.from == env.coinbase,
            )?;
            balances.insert(expected.from.clone(), receipt.payer_after.clone());
            balances.insert(env.coinbase.clone(), receipt.beneficiary_after.clone());
            costs.push((model, payer));
        }
        if balances.get(&self.steps[0].0.from) != Some(&swap.caller_before)
            || balances.get(&self.swap.coinbase) != Some(&swap.coinbase_before)
        {
            return Err(bad());
        }
        Ok(costs)
    }
}
