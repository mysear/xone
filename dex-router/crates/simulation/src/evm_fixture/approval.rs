use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn execute<'a>(
    runner: &mut Runner<'a>,
    mut image: Arc<core::StateImage>,
    split: &BoundSplit,
    context: &Context,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowlist: &[String],
    initial_allowance: &[u8],
    fixture: &'a FixtureState,
    tx: &TransactionContext,
    seq: &'a ApprovalSequence,
) -> Result<(
    Arc<core::StateImage>,
    Option<ApprovalReceipt>,
    ApprovalReceipt,
)> {
    use ordered_v2::ApprovalPolicy;
    let first = &fixture.execution_environment;
    let approval_env = seq
        .reset
        .as_ref()
        .map_or(first, |r| &r.approval_environment);
    let invalid = || {
        runner.error(
            SimulationTransactionContext,
            "approval_sequence",
            "invalid transaction sequence",
        )
    };
    let follows = |a: &ExecutionEnvironment, b: &ExecutionEnvironment| {
        a.block_number.checked_add(1) == Some(b.block_number)
            && b.parent_hash == a.block_hash
            && b.timestamp > a.timestamp
            && a.chain_id == b.chain_id
            && b.fork == "shanghai"
            && b.block_hash != a.block_hash
            && b.block_hash != fixture.state_anchor.block_hash
    };
    let valid_tx = |t: &TransactionContext, e: &ExecutionEnvironment| {
        t.chain_id == e.chain_id
            && t.gas_limit <= e.gas_limit
            && t.gas_limit <= runner.budget.max_transaction_gas
            && t.max_priority_fee_per_gas.is_some()
    };
    if !follows(approval_env, &seq.swap_environment)
        || !valid_tx(&seq.approval_context, approval_env)
        || !valid_tx(tx, &seq.swap_environment)
        || seq.approval_context.nonce.checked_add(1) != Some(tx.nonce)
        || seq.swap_environment.block_hash == first.block_hash
    {
        return Err(invalid());
    }
    hex_length(&seq.swap_environment.block_hash, Some(32))?;
    if let Some(reset) = &seq.reset {
        if !follows(first, approval_env)
            || !valid_tx(&reset.context, first)
            || reset.context.nonce.checked_add(1) != Some(seq.approval_context.nonce)
        {
            return Err(invalid());
        }
        hex_length(&approval_env.block_hash, Some(32))?;
    }
    code_check(&image, &context.sell_token, &seq.token_code_sha256)?;
    let allowance = Amount::from_be_bytes(initial_allowance)
        .map_err(|e| runner.error(SimulationObservationMismatch, "approval_allowance", e))?;
    let policy = if seq.reset.is_some() {
        ApprovalPolicy::ResetThenExact
    } else {
        ApprovalPolicy::ExactOnly
    };
    let mut operations =
        ordered_v2::caller_approvals(split, pools, executor, allowlist, &allowance, policy)
            .map_err(|e| runner.error(SimulationBindingMismatch, "approval_transaction", e))?
            .into_iter();
    if operations.len() != 1 + usize::from(seq.reset.is_some()) {
        return Err(runner.error(
            SimulationBindingMismatch,
            "approval_transaction",
            "configured operation count differs from observed allowance",
        ));
    }
    let mut reset_receipt = None;
    if seq.reset.is_some() {
        let (after, receipt) = apply(runner, image, context, executor, operations.next().unwrap())?;
        image = after;
        reset_receipt = Some(receipt);
    }
    runner.env = approval_env;
    runner.tx = &seq.approval_context;
    let (after, receipt) = apply(runner, image, context, executor, operations.next().unwrap())?;
    Ok((after, reset_receipt, receipt))
}

fn apply(
    runner: &mut Runner<'_>,
    image: Arc<core::StateImage>,
    context: &Context,
    executor: &SplitExecutor,
    approval_tx: Transaction,
) -> Result<(Arc<core::StateImage>, ApprovalReceipt)> {
    let a = runner.tx;
    let env = runner.env;
    let caller = runner.caller_address;
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
        beneficiary_before: balances.beneficiary_before,
        beneficiary_after: balances.beneficiary_after,
        post_state_digest: delta_hash(&applied.state)?,
    };
    Ok((after, receipt))
}
