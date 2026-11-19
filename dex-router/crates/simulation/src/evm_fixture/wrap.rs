use super::*;
use xone_execution_preparation::evm_native_cost::{
    audited_payer_cost, measured_native_cost, NativeBalances,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapSequence {
    pub wrapper: String,
    pub deposit_context: TransactionContext,
    /// The first allowance operation when funding also requires caller approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_approval_environment: Option<ExecutionEnvironment>,
    pub swap_environment: ExecutionEnvironment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapReceipt {
    pub transaction: Transaction,
    pub block_hash: String,
    pub nonce: u64,
    pub gas_used: u64,
    pub execution_fee: Amount,
    pub token_before: Amount,
    pub token_after: Amount,
    pub payer_before: Amount,
    pub payer_after: Amount,
    pub beneficiary_before: Amount,
    pub beneficiary_after: Amount,
    pub wrapper_before: Amount,
    pub wrapper_after: Amount,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    runner: &mut Runner<'_>,
    image: Arc<core::StateImage>,
    context: &Context,
    input: &Amount,
    executor: &ExecutorIdentity,
    fixture: &FixtureState,
    next_tx: &TransactionContext,
    seq: &WrapSequence,
) -> Result<(Arc<core::StateImage>, WrapReceipt)> {
    let env = &fixture.execution_environment;
    let next = seq
        .first_approval_environment
        .as_ref()
        .unwrap_or(&seq.swap_environment);
    let tx = &seq.deposit_context;
    let bad = || {
        runner.error(
            SimulationTransactionContext,
            "wrap_context",
            "invalid deposit/swap sequence",
        )
    };
    if next.fork != "shanghai"
        || next.chain_id != env.chain_id
        || tx.chain_id != env.chain_id
        || next_tx.chain_id != env.chain_id
        || env.block_number.checked_add(1) != Some(next.block_number)
        || next.parent_hash != env.block_hash
        || next.block_hash == env.block_hash
        || next.block_hash == fixture.state_anchor.block_hash
        || next.timestamp <= env.timestamp
        || tx.nonce.checked_add(1) != Some(next_tx.nonce)
        || tx.gas_limit > env.gas_limit.min(runner.budget.max_transaction_gas)
        || next_tx.gas_limit > next.gas_limit
        || tx.max_priority_fee_per_gas.is_none()
        || next_tx.max_priority_fee_per_gas.is_none()
    {
        return Err(bad());
    }
    hex_length(&next.block_hash, Some(32))?;
    let caller = runner.caller_address;
    let beneficiary = address(&env.coinbase)?;
    let wrapper = address(&seq.wrapper)?;
    if wrapper == caller {
        return Err(bad());
    }
    code_check(&image, &seq.wrapper, ordered_v2::WETH9_CODE_SHA256)?;
    runner.expect_address(image.clone(), &executor.target, "WETH()", &[], &seq.wrapper)?;
    let sender = &context.caller;
    let token_before =
        runner.balance(image.clone(), &seq.wrapper, sender, "wrap_balance_before")?;
    let deposit = ordered_v2::wrapped_input_transaction(
        context,
        input,
        &seq.wrapper,
        &as_amount(token_before)?,
    )
    .map_err(|e| runner.error(SimulationBindingMismatch, "wrap_binding", e))?
    .ok_or_else(|| {
        runner.error(
            SimulationBindingMismatch,
            "wrap_binding",
            "wrapped balance already sufficient",
        )
    })?;
    let principal = amount(&deposit.value)?;
    let result = runner.run(
        image.clone(),
        TxEnv {
            caller,
            transact_to: TxKind::Call(wrapper),
            value: principal,
            data: Bytes::from(calldata("deposit()", &[])),
            nonce: Some(tx.nonce),
            chain_id: Some(tx.chain_id),
            gas_limit: tx.gas_limit,
            gas_price: amount(&tx.gas_price)?,
            gas_priority_fee: tx
                .max_priority_fee_per_gas
                .as_ref()
                .map(amount)
                .transpose()?,
            ..Default::default()
        },
        "wrap_deposit",
        false,
    )?;
    let actual = &result.result_and_state;
    if !actual.result.is_success() || !actual.result.output().is_some_and(|b| b.is_empty()) {
        return Err(runner.error(
            SimulationObservationMismatch,
            "wrap_deposit",
            "deposit failed",
        ));
    }
    let after = Arc::new(
        image
            .apply(&actual.state)
            .map_err(|e| runner.error(SimulationExecutionUnavailable, "wrap_poststate", e))?,
    );
    let token_after = runner.balance(after.clone(), &seq.wrapper, sender, "wrap_balance_after")?;
    let mismatch = || {
        runner.error(
            SimulationObservationMismatch,
            "wrap_balances",
            "principal or fees do not reconcile",
        )
    };
    if token_before.checked_add(principal) != Some(token_after) {
        return Err(mismatch());
    }
    let balance = |state: &core::StateImage, who| match state.data().accounts.get(&who) {
        Some(Some(a)) => Ok(a.balance),
        Some(None) => Ok(U256::ZERO),
        None => Err(mismatch()),
    };
    let payer_before = balance(&image, caller)?;
    let payer_after = balance(&after, caller)?;
    let beneficiary_before = balance(&image, beneficiary)?;
    let beneficiary_after = balance(&after, beneficiary)?;
    let wrapper_before = balance(&image, wrapper)?;
    let wrapper_after = balance(&after, wrapper)?;
    let gas_used = actual.result.gas_used();
    let fees = measured_native_cost(
        gas_used,
        tx.gas_limit,
        &env.base_fee,
        &tx.gas_price,
        tx.max_priority_fee_per_gas.as_ref(),
    )
    .map_err(|_| mismatch())?;
    let funded = payer_before.checked_sub(principal).ok_or_else(mismatch)?;
    let beneficiary_start = if beneficiary == caller {
        funded
    } else if beneficiary == wrapper {
        beneficiary_before
            .checked_add(principal)
            .ok_or_else(mismatch)?
    } else {
        beneficiary_before
    };
    let wrapper_tip = if beneficiary == wrapper {
        amount(&fees.priority_fee_paid)?
    } else {
        U256::ZERO
    };
    if wrapper_before
        .checked_add(principal)
        .and_then(|v| v.checked_add(wrapper_tip))
        != Some(wrapper_after)
    {
        return Err(mismatch());
    }
    audited_payer_cost(
        &fees,
        &NativeBalances {
            payer_before: as_amount(funded)?,
            payer_after: as_amount(payer_after)?,
            beneficiary_before: as_amount(beneficiary_start)?,
            beneficiary_after: as_amount(beneficiary_after)?,
        },
        beneficiary == caller,
    )
    .map_err(|_| mismatch())?;
    Ok((
        after,
        WrapReceipt {
            transaction: deposit,
            block_hash: env.block_hash.clone(),
            nonce: tx.nonce,
            gas_used,
            execution_fee: fees.execution_fee,
            token_before: as_amount(token_before)?,
            token_after: as_amount(token_after)?,
            payer_before: as_amount(payer_before)?,
            payer_after: as_amount(payer_after)?,
            beneficiary_before: as_amount(beneficiary_before)?,
            beneficiary_after: as_amount(beneficiary_after)?,
            wrapper_before: as_amount(wrapper_before)?,
            wrapper_after: as_amount(wrapper_after)?,
        },
    ))
}
