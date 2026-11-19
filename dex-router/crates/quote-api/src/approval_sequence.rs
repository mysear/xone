//! Parent-side accounting for an explicitly configured two-transaction local replay.
use xone_domain::{err, hash, Amount, Result};
use xone_execution_plan::{ordered_v2, Transaction};
use xone_execution_preparation::evm_native_cost::{
    audited_payer_cost, measured_native_cost, NativeBalances, NativeCost, PayerCost,
};
use xone_simulation::{
    evm_fixture::{ApprovalReceipt, ApprovalSequence, NativeFeeMeasurements},
    worker::WorkerRequest,
};

pub(super) struct Commitment {
    expected: Transaction,
    settings: ApprovalSequence,
    block: String,
    beneficiary: String,
    base_fee: Amount,
    caller_before: Amount,
    beneficiary_before: Amount,
    swap_beneficiary_before: Amount,
}

pub(super) fn validate_bundle(
    bundle: &xone_simulation::worker::LocalSimulationBundle,
    gas_cap: u64,
) -> Result<()> {
    let Some(model) = &bundle.approval_replay else {
        return Ok(());
    };
    use sha2::{Digest, Sha256};
    let bad = || err("SIMULATION_APPROVAL_SEQUENCE_BINDING");
    let first = &bundle.fixture.execution_environment;
    let second = &model.sequence.swap_environment;
    let approval = &model.sequence.approval_context;
    let swap = &model.swap_context;
    let account = |address: &str| {
        bundle
            .fixture
            .accounts
            .iter()
            .find(|a| a.address == address && a.exists)
            .ok_or_else(bad)
    };
    let token = account(&model.token)?;
    let code = hex::decode(token.code.strip_prefix("0x").ok_or_else(bad)?).map_err(|_| bad())?;
    if model.sequence.reset.is_some()
        || bundle.split_executor.is_none()
        || bundle.op_attributes_calldata.is_some()
        || first.fork != "shanghai"
        || second.fork != first.fork
        || account(&model.caller)?.nonce != approval.nonce
        || approval.nonce.checked_add(1) != Some(swap.nonce)
        || first.chain_id != second.chain_id
        || approval.chain_id != first.chain_id
        || swap.chain_id != second.chain_id
        || first.block_number.checked_add(1) != Some(second.block_number)
        || second.parent_hash != first.block_hash
        || second.block_hash == first.block_hash
        || second.block_hash == bundle.fixture.state_anchor.block_hash
        || second.timestamp <= first.timestamp
        || second.timestamp.checked_mul(1000).is_none()
        || code.is_empty()
        || hex::encode(Sha256::digest(&code)) != model.sequence.token_code_sha256
    {
        return Err(bad());
    }
    for (tx, env) in [(approval, first), (swap, second)] {
        if tx.gas_limit > gas_cap.min(env.gas_limit) || tx.max_priority_fee_per_gas.is_none() {
            return Err(bad());
        }
        measured_native_cost(
            21_000,
            tx.gas_limit,
            &env.base_fee,
            &tx.gas_price,
            tx.max_priority_fee_per_gas.as_ref(),
        )?;
    }
    Ok(())
}

impl Commitment {
    pub fn settlement_ms(&self) -> u64 {
        self.settings.swap_environment.timestamp * 1000
    }
    pub fn new(request: &WorkerRequest) -> Result<Option<Self>> {
        let Some(split) = &request.split else {
            return Ok(None);
        };
        let Some(settings) = &split.approval_sequence else {
            return Ok(None);
        };
        let bad = || err("SIMULATION_APPROVAL_SEQUENCE_BINDING");
        let expected = ordered_v2::caller_approval(
            &split.bound,
            &request.pools,
            &split.expected_executor,
            &request.allowed_targets,
            &Amount::zero(),
        )?
        .ok_or_else(bad)?;
        let env = &request.fixture.execution_environment;
        let swap = &settings.swap_environment;
        let a = &settings.approval_context;
        if env.fork != "shanghai"
            || swap.fork != env.fork
            || a.chain_id != env.chain_id
            || swap.chain_id != env.chain_id
            || request.transaction_context.chain_id != env.chain_id
            || env.block_number.checked_add(1) != Some(swap.block_number)
            || swap.parent_hash != env.block_hash
            || swap.block_hash == env.block_hash
            || swap.timestamp <= env.timestamp
            || a.nonce.checked_add(1) != Some(request.transaction_context.nonce)
        {
            return Err(bad());
        }
        let balance = |address: &str| {
            request
                .fixture
                .accounts
                .iter()
                .find(|r| r.address == address)
                .map(|r| {
                    if r.exists {
                        r.balance.clone()
                    } else {
                        Amount::zero()
                    }
                })
                .ok_or_else(bad)
        };
        Ok(Some(Self {
            caller_before: balance(&expected.from)?,
            beneficiary_before: balance(&env.coinbase)?,
            swap_beneficiary_before: balance(&swap.coinbase)?,
            expected,
            settings: settings.clone(),
            block: env.block_hash.clone(),
            beneficiary: env.coinbase.clone(),
            base_fee: env.base_fee.clone(),
        }))
    }
    pub fn audit(
        &self,
        receipt: &ApprovalReceipt,
        swap: &NativeFeeMeasurements,
    ) -> Result<(NativeCost, PayerCost)> {
        let bad = || err("SIMULATION_APPROVAL_SEQUENCE_RECEIPT");
        let tx = &self.settings.approval_context;
        let model = measured_native_cost(
            receipt.gas_used,
            tx.gas_limit,
            &self.base_fee,
            &tx.gas_price,
            tx.max_priority_fee_per_gas.as_ref(),
        )?;
        if hash(&receipt.transaction)? != hash(&self.expected)?
            || receipt.block_hash != self.block
            || receipt.nonce != tx.nonce
            || self.caller_before != receipt.payer_before
            || self.beneficiary_before != receipt.beneficiary_before
            || model.execution_fee != receipt.execution_fee
            || receipt.payer_after != swap.caller_before
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
            self.expected.from == self.beneficiary,
        )?;
        let coinbase = &self.settings.swap_environment.coinbase;
        let swap_start = if coinbase == &self.expected.from {
            &receipt.payer_after
        } else if coinbase == &self.beneficiary {
            &receipt.beneficiary_after
        } else {
            &self.swap_beneficiary_before
        };
        if &swap.coinbase_before != swap_start {
            return Err(bad());
        }
        Ok((model, payer))
    }
}
