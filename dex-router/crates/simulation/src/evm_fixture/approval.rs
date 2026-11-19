use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    runner: &mut Runner<'_>,
    image: Arc<core::StateImage>,
    split: &BoundSplit,
    context: &Context,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowlist: &[String],
    initial_allowance: &[u8],
    fixture: &FixtureState,
    tx: &TransactionContext,
    seq: &ApprovalSequence,
) -> Result<(Arc<core::StateImage>, ApprovalReceipt)> {
    let a = &seq.approval_context;
    let env = &fixture.execution_environment;
    let swap_env = &seq.swap_environment;
    let caller = runner.caller_address;
    if swap_env.chain_id != env.chain_id
        || a.chain_id != env.chain_id
        || swap_env.fork != "shanghai"
        || env.block_number.checked_add(1) != Some(swap_env.block_number)
        || swap_env.parent_hash != env.block_hash
        || swap_env.timestamp <= env.timestamp
        || swap_env.block_hash == env.block_hash
        || swap_env.block_hash == fixture.state_anchor.block_hash
        || a.nonce.checked_add(1) != Some(tx.nonce)
        || a.gas_limit > env.gas_limit
        || tx.gas_limit > swap_env.gas_limit
        || a.max_priority_fee_per_gas.is_none()
        || tx.max_priority_fee_per_gas.is_none()
    {
        return Err(runner.error(
            SimulationTransactionContext,
            "approval_sequence",
            "consecutive transaction and block contexts required",
        ));
    }
    hex_length(&swap_env.block_hash, Some(32))?;
    code_check(&image, &context.sell_token, &seq.token_code_sha256)?;
    let allowance = Amount::from_be_bytes(initial_allowance)
        .map_err(|e| runner.error(SimulationObservationMismatch, "approval_allowance", e))?;
    let approval_tx = ordered_v2::caller_approval(split, pools, executor, allowlist, &allowance)
        .map_err(|e| runner.error(SimulationBindingMismatch, "approval_transaction", e))?
        .ok_or_else(|| {
            runner.error(
                SimulationBindingMismatch,
                "approval_transaction",
                "already approved",
            )
        })?;
    let args = TxEnv {
        caller,
        transact_to: TxKind::Call(address(&approval_tx.to)?),
        data: Bytes::from(
            hex::decode(&approval_tx.data[2..])
                .map_err(|e| runner.error(SimulationBindingMismatch, "approval_transaction", e))?,
        ),
        nonce: Some(a.nonce),
        chain_id: Some(a.chain_id),
        gas_limit: a.gas_limit,
        gas_price: amount(&a.gas_price)?,
        gas_priority_fee: a
            .max_priority_fee_per_gas
            .as_ref()
            .map(amount)
            .transpose()?,
        ..Default::default()
    };
    let result = runner.run(image.clone(), args, "approve", false)?;
    let applied = &result.result_and_state;
    let mut yes = [0u8; 32];
    yes[31] = 1;
    if !applied.result.is_success()
        || applied
            .result
            .output()
            .is_none_or(|out| out.as_ref() != yes)
    {
        return Err(runner.error(
            SimulationObservationMismatch,
            "approve",
            "canonical true required",
        ));
    }
    let after = Arc::new(
        image
            .apply(&applied.state)
            .map_err(|e| runner.error(SimulationExecutionUnavailable, "approval_poststate", e))?,
    );
    let balance = |state: &core::StateImage, who| -> Result<Amount> {
        match state.data().accounts.get(&who) {
            Some(Some(account)) => as_amount(account.balance),
            Some(None) => Ok(Amount::zero()),
            None => Err(runner.error(
                SimulationObservationUnavailable,
                "approval_fees",
                "account missing",
            )),
        }
    };
    use xone_execution_preparation::evm_native_cost::{
        measured_native_cost, reconcile_native_balances, NativeBalances,
    };
    let gas_used = applied.result.gas_used();
    let fees = measured_native_cost(
        gas_used,
        a.gas_limit,
        &env.base_fee,
        &a.gas_price,
        a.max_priority_fee_per_gas.as_ref(),
    )
    .map_err(|e| runner.error(SimulationTransactionContext, "approval_fees", e))?;
    let beneficiary = address(&env.coinbase)?;
    let balances = NativeBalances {
        payer_before: balance(&image, caller)?,
        payer_after: balance(&after, caller)?,
        beneficiary_before: balance(&image, beneficiary)?,
        beneficiary_after: balance(&after, beneficiary)?,
    };
    reconcile_native_balances(&fees, &balances, caller == beneficiary)
        .map_err(|e| runner.error(SimulationObservationMismatch, "approval_fees", e))?;
    let authorized = runner.probe(
        after.clone(),
        &context.sell_token,
        calldata(
            "allowance(address,address)",
            &[
                address_word(&context.caller)?,
                address_word(&executor.target)?,
            ],
        ),
        "split_caller_allowance_after_approval",
        32,
    )?;
    if hex::encode(authorized) != approval_tx.data[74..] {
        return Err(runner.error(
            SimulationObservationMismatch,
            "approval_poststate",
            "allowance differs from bound amount",
        ));
    }
    let receipt = ApprovalReceipt {
        transaction: approval_tx,
        nonce: a.nonce,
        block_hash: env.block_hash.clone(),
        gas_used,
        execution_fee: fees.execution_fee,
        payer_before: balances.payer_before,
        payer_after: balances.payer_after,
        post_state_digest: delta_hash(&applied.state)?,
    };
    Ok((after, receipt))
}
