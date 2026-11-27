use super::*;
use xone_domain::singleton::ZERO_ADDRESS;
use xone_execution_plan::{exact_out::ExactOutPlan, v4_single, Plan};

pub const SINGLETON_SIMULATOR_VERSION: &str = "provided-local-state/singleton-cancun-v1";
pub const BEACON_ROOTS_ADDRESS: &str = "0x000f3df6d732807ef1319fb7b8bb8522d0beac02";
const BEACON_ROOTS_CODE: &str = "0x3373fffffffffffffffffffffffffffffffffffffffe14604d57602036146024575f5ffd5b5f35801560495762001fff810690815414603c575f5ffd5b62001fff01545f5260205ff35b5f5ffd5b62001fff42064281555f359062001fff015500";

fn apply_beacon_root(
    fixture: &FixtureState,
    mut image: Arc<core::StateImage>,
) -> Result<Arc<core::StateImage>> {
    let env = &fixture.execution_environment;
    let account = fixture
        .accounts
        .iter()
        .find(|row| row.address == BEACON_ROOTS_ADDRESS);
    let Some([timestamp_slot, root_slot]) = env.beacon_root_slots()? else {
        if account.is_some_and(|row| row.exists) {
            return Err(failure(
                SimulationEnvironmentMismatch,
                "beacon_root",
                "missing parent beacon block root",
            ));
        }
        return Ok(image);
    };
    let root = env.parent_beacon_block_root.as_deref().ok_or_else(|| {
        failure(
            SimulationEnvironmentMismatch,
            "beacon_root",
            "missing parent beacon block root",
        )
    })?;
    let account = account.ok_or_else(|| {
        failure(
            SimulationInvalidFixture,
            "beacon_root",
            "missing beacon roots account",
        )
    })?;
    if account.code == "0x" {
        // EIP-4788 skips the system call if the contract is absent.
        return Ok(image);
    }
    if account.code != BEACON_ROOTS_CODE {
        return Err(failure(
            SimulationCodeMismatch,
            "beacon_root",
            BEACON_ROOTS_ADDRESS,
        ));
    }
    let target = address(BEACON_ROOTS_ADDRESS)?;
    let data = Arc::make_mut(&mut image).data_mut();
    // The pinned runtime writes exactly these two ring-buffer entries. Other
    // keys remain unknown and the system write changes no nonce or balance.
    for (slot, value) in [
        (timestamp_slot, U256::from(env.timestamp)),
        (root_slot, word(root)?),
    ] {
        let entry = data
            .storage
            .get_mut(&(target, U256::from(slot)))
            .ok_or_else(|| {
                failure(
                    SimulationInvalidFixture,
                    "beacon_root",
                    "missing pre-state ring buffer slot",
                )
            })?;
        *entry = value;
    }
    Ok(image)
}

#[cfg(test)]
mod beacon_root_tests {
    use super::*;

    #[test]
    fn system_write_updates_only_declared_ring_slots() {
        let target = address(BEACON_ROOTS_ADDRESS).unwrap();
        let zero_hash = format!("{:#x}", B256::ZERO);
        let zero_address = format!("{:#x}", Address::ZERO);
        let fixture: FixtureState = serde_json::from_value(serde_json::json!({
            "schema_version":1,
            "state_anchor":{"chain_key":"evm:1","block_number":1,"block_hash":zero_hash,
                "parent_hash":zero_hash,"timestamp":8192,"genesis_hash":zero_hash},
            "execution_environment":{"chain_id":1,"fork":"cancun","block_number":2,
                "block_hash":zero_hash,"parent_hash":zero_hash,"timestamp":8193,
                "coinbase":zero_address,"gas_limit":30_000_000,"base_fee":"1","prevrandao":zero_hash,
                "excess_blob_gas":0,"parent_beacon_block_root":format!("0x{}", "ab".repeat(32))},
            "accounts":[{"address":BEACON_ROOTS_ADDRESS,"exists":true,"balance":"0","nonce":1,
                "code":BEACON_ROOTS_CODE,"storage":[]}],
            "block_hashes":[]
        })).unwrap();
        let timestamp_key = (target, U256::from(2));
        let root_key = (target, U256::from(8193));
        let other = (target, U256::from(3));
        let code = Bytes::from(hex::decode(&BEACON_ROOTS_CODE[2..]).unwrap());
        let code_hash = keccak256(&code);
        let mut data = core::ImageData {
            accounts: BTreeMap::from([(
                target,
                Some(AccountInfo {
                    balance: U256::from(17),
                    nonce: 1,
                    code_hash,
                    code: None,
                }),
            )]),
            bytecodes: BTreeMap::from([(code_hash, code)]),
            storage: BTreeMap::from([(timestamp_key, U256::from(2)), (other, U256::from(3))]),
            ..core::ImageData::default()
        };
        let image = Arc::new(core::StateImage::new(data.clone()).unwrap());
        assert_eq!(
            apply_beacon_root(&fixture, image).unwrap_err().phase,
            "beacon_root"
        );
        data.storage.insert(root_key, U256::from(4));
        let image = Arc::new(core::StateImage::new(data).unwrap());
        let post = apply_beacon_root(&fixture, image.clone()).unwrap();
        assert_eq!(post.data().storage[&timestamp_key], U256::from(8193));
        assert_eq!(
            post.data().storage[&root_key],
            U256::from_be_bytes([0xab; 32])
        );
        assert_eq!(post.data().storage[&other], U256::from(3));
        assert_eq!(post.data().accounts, image.data().accounts);
        assert_eq!(image.data().storage[&root_key], U256::from(4));
    }
}

#[derive(Clone, Copy)]
pub enum SingletonPlan<'a> {
    ExactIn(&'a Plan),
    ExactOut(&'a ExactOutPlan),
}
impl SingletonPlan<'_> {
    pub(crate) fn bind(
        &self,
        pools: &[Pool],
        executor: v4_single::Executor<'_>,
    ) -> xone_domain::Result<v4_single::BoundV4Swap> {
        match self {
            Self::ExactIn(plan) => v4_single::bind_exact_in(plan, pools, executor),
            Self::ExactOut(plan) => v4_single::bind_exact_out(plan, pools, executor),
        }
    }
}

/// Rebind a single-pool quote and replay its transaction against supplied H state.
/// The fixture and H+1 environment are local evidence, with no receipt authority.
#[allow(clippy::too_many_arguments)]
pub fn simulate_singleton(
    plan: SingletonPlan<'_>,
    pools: &[Pool],
    executor: v4_single::Executor<'_>,
    token_digests: &BTreeMap<String, String>,
    fixture: &FixtureState,
    tx: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
) -> Result<SimulationOutcome> {
    simulate_singleton_transaction(
        plan,
        pools,
        executor,
        token_digests,
        fixture,
        tx,
        budget,
        control,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_singleton_transaction(
    plan: SingletonPlan<'_>,
    pools: &[Pool],
    executor: v4_single::Executor<'_>,
    token_digests: &BTreeMap<String, String>,
    fixture: &FixtureState,
    tx: &TransactionContext,
    budget: &SimulationBudget,
    control: core::ExecutionControl,
    signed: Option<&crate::worker::SignedEvmTransaction>,
) -> Result<SimulationOutcome> {
    let binding_error = |e: xone_domain::Error| failure(SimulationBindingMismatch, "singleton", e);
    let bound = plan.bind(pools, executor).map_err(binding_error)?;
    let (context, snapshot, pool_id, quoted_input, quoted_output, exact_out) = match plan {
        SingletonPlan::ExactIn(p) => (
            &p.context,
            &p.snapshot,
            &p.allocation.routes[0].path.edges[0].pool_id,
            &p.allocation.sell_amount,
            &p.allocation.gross_buy_amount,
            false,
        ),
        SingletonPlan::ExactOut(p) => (
            &p.context,
            &p.snapshot,
            &p.allocation.routes[0].edges[0].pool_id,
            &p.allocation.required_input,
            &p.allocation.target_output,
            true,
        ),
    };
    control_check(&control, "singleton")?;
    fixture.validate_snapshot_environment(snapshot, "cancun")?;
    let env = &fixture.execution_environment;
    let max_fee = amount(&tx.gas_price)?;
    let priority = tx
        .max_priority_fee_per_gas
        .as_ref()
        .map(amount)
        .transpose()?;
    if tx.chain_id != env.chain_id
        || tx.gas_limit == 0
        || tx.gas_limit > env.gas_limit
        || max_fee < amount(&env.base_fee)?
        || priority.is_some_and(|tip| tip > max_fee)
    {
        return Err(failure(
            SimulationTransactionContext,
            "singleton",
            "invalid chain, gas limit, or fees",
        ));
    }
    let pre = apply_beacon_root(fixture, fixture.to_image(budget)?)?;
    let target = &bound.transaction.to;
    code_check(&pre, target, &bound.executor_code_sha256)?;
    code_check(&pre, &bound.manager, &bound.manager_code_sha256)?;
    if let Some(vault) = &bound.infinity {
        code_check(&pre, &vault.vault, &vault.vault_code_sha256)?;
    }
    let pool = pools
        .iter()
        .find(|pool| pool.pool_id == *pool_id)
        .ok_or_else(|| {
            failure(
                SimulationBindingMismatch,
                "singleton",
                "selected pool missing",
            )
        })?;
    verify_hook(&pre, pool, &bound)?;
    for token in [&context.sell_token, &context.buy_token] {
        if token != ZERO_ADDRESS {
            let digest = token_digests
                .get(token)
                .ok_or_else(|| failure(SimulationCodeMismatch, "token_runtime", token))?;
            code_check(&pre, token, digest)?;
        }
    }
    let caller = address(&context.caller)?;
    let recipient = address(&context.recipient)?;
    let mut transaction = TxEnv {
        caller,
        transact_to: TxKind::Call(address(target)?),
        value: amount(&bound.transaction.value)?,
        data: Bytes::from(
            hex::decode(&bound.transaction.data[2..])
                .map_err(|e| failure(SimulationBindingMismatch, "transaction_input", e))?,
        ),
        nonce: Some(tx.nonce),
        chain_id: Some(tx.chain_id),
        gas_limit: tx.gas_limit,
        gas_price: max_fee,
        gas_priority_fee: priority,
        ..TxEnv::default()
    };
    let mut input_bytes = 0;
    if let Some(signed) = signed {
        let (authenticated, wire_len) = signed
            .authenticate(tx.chain_id, caller, budget)
            .map_err(|e| failure(SimulationBindingMismatch, "singleton_signed", e))?;
        let mut signed_env = authenticated.into_environment();
        // Access lists are part of signed gas semantics. Compare all other fields to the template.
        let access_list = std::mem::take(&mut signed_env.access_list);
        if signed_env != transaction {
            return Err(failure(
                SimulationBindingMismatch,
                "singleton_signed",
                "signed transaction differs from bound quote or bank context",
            ));
        }
        transaction.access_list = access_list;
        input_bytes = wire_len as u64;
    }
    let mut runner = Runner {
        caller_address: caller,
        budget,
        env,
        tx,
        control,
        usage: BudgetUsage {
            input_bytes,
            ..BudgetUsage::default()
        },
        observations: Vec::new(),
        op: None,
        op_fees: None,
    };
    let balances =
        |runner: &mut Runner<'_>, image: &Arc<core::StateImage>, sell_phase, buy_phase| {
            let mut read = |token: &str, owner: &str, address, phase| {
                if token == ZERO_ADDRESS {
                    runner.native_balance(image, address, phase)
                } else {
                    runner.balance(image.clone(), token, owner, phase)
                }
            };
            Ok::<_, SimulationError>((
                read(&context.sell_token, &context.caller, caller, sell_phase)?,
                read(&context.buy_token, &context.recipient, recipient, buy_phase)?,
            ))
        };
    let (sell_before, buy_before) = balances(
        &mut runner,
        &pre,
        "caller_sell_before",
        "recipient_buy_before",
    )?;
    let actual = runner
        .run(pre.clone(), transaction, "singleton_swap", false)?
        .result_and_state;
    let post = Arc::new(
        pre.apply(&actual.state)
            .map_err(|e| runner.error(SimulationExecutionUnavailable, "post_state", e))?,
    );
    let gas_used = actual.result.gas_used();
    let fees = runner.native_fees(&pre, &post, gas_used)?;
    let (sell_after, buy_after) = balances(
        &mut runner,
        &post,
        "caller_sell_after",
        "recipient_buy_after",
    )?;
    let coinbase = address(&env.coinbase)?;
    let price = amount(&fees.execution_fee)?;
    let tip = amount(&fees.priority_fee_paid)?;
    let direction = || {
        runner.error(
            SimulationBalanceDirection,
            "balances",
            "invalid swap balance delta",
        )
    };
    let spent = if context.sell_token == ZERO_ADDRESS {
        let before = if caller == coinbase {
            sell_before.checked_add(tip).ok_or_else(direction)?
        } else {
            sell_before
        };
        (before.checked_sub(sell_after).ok_or_else(direction)?)
            .checked_sub(price)
            .ok_or_else(direction)?
    } else {
        sell_before.checked_sub(sell_after).ok_or_else(direction)?
    };
    let bought = if context.buy_token == ZERO_ADDRESS {
        let before = if recipient == coinbase {
            buy_before.checked_add(tip).ok_or_else(direction)?
        } else {
            buy_before
        };
        let after = if recipient == caller {
            buy_after.checked_add(price).ok_or_else(direction)?
        } else {
            buy_after
        };
        after.checked_sub(before).ok_or_else(direction)?
    } else {
        buy_after.checked_sub(buy_before).ok_or_else(direction)?
    };
    let (status, gas_refunded, halt_reason) = match &actual.result {
        ExecutionResult::Success { gas_refunded, .. } => (EvmStatus::Success, *gas_refunded, None),
        ExecutionResult::Revert { .. } => (EvmStatus::Revert, 0, None),
        ExecutionResult::Halt { reason, .. } => (EvmStatus::Halt, 0, Some(format!("{reason:?}"))),
    };
    let returned = actual.result.output().map_or(&[][..], |b| b.as_ref());
    let logs = actual.result.logs().iter().map(|log| {
        serde_json::json!({
            "address": format!("{:#x}", log.address),
            "topics": log.data.topics().iter().map(|topic| format!("{topic:#x}")).collect::<Vec<_>>(),
            "data": format!("0x{}", hex::encode(log.data.data.as_ref())),
        })
    }).collect::<Vec<_>>();
    let success = status == EvmStatus::Success;
    let unused = amount(&bound.input_limit)?
        .checked_sub(spent)
        .ok_or_else(direction)?;
    if (success
        && (returned.len() != 64
            || U256::from_be_slice(&returned[..32]) != spent
            || U256::from_be_slice(&returned[32..]) != bought
            || bought < amount(&bound.output_limit)?
            || (!exact_out && !unused.is_zero())))
        || (!success && (!spent.is_zero() || !bought.is_zero()))
    {
        return Err(runner.error(
            SimulationObservationMismatch,
            "singleton_settlement",
            "return data or limits disagree with balance deltas",
        ));
    }
    let mut outcome = SimulationOutcome {
        signed_transaction_hash: signed.map(|s| s.transaction_hash.clone()),
        approval: None,
        allowance_reset: None,
        wrap: None,
        verified: false,
        authority: DataAuthority::ProvidedLocalState,
        plan_validation: "BOUND_REVALIDATED".into(),
        execution: EvmReceipt {
            logs_hash: Some(canonical_hash(&logs)?),
            status,
            gas_used,
            gas_refunded,
            output: format!("0x{}", hex::encode(returned)),
            return_classification: if success {
                "ABI_UINT128_PAIR"
            } else if returned.is_empty() {
                "EMPTY"
            } else {
                "OPAQUE"
            }
            .into(),
            halt_reason,
            sender_nonce_before: tx.nonce,
            sender_nonce_after: runner.tx_caller(&post)?.1,
            post_state_digest: delta_hash(&actual.state)?,
        },
        matches_plan: success && spent == amount(quoted_input)? && bought == amount(quoted_output)?,
        balances: BalanceMeasurements {
            caller_sell_before: as_amount(sell_before)?,
            caller_sell_after: as_amount(sell_after)?,
            recipient_buy_before: as_amount(buy_before)?,
            recipient_buy_after: as_amount(buy_after)?,
        },
        actual_input_consumed: as_amount(spent)?,
        refund: if success && context.sell_token == ZERO_ADDRESS {
            as_amount(unused)?
        } else {
            Amount::zero()
        },
        unused_input_budget: exact_out.then(|| as_amount(unused)).transpose()?,
        actual_buy_amount: as_amount(bought)?,
        native_fees: Some(fees),
        op_fees: None,
        observations: std::mem::take(&mut runner.observations),
        usage: runner.usage.clone(),
        source_fixture_hash: canonical_hash(fixture)?,
        simulator_version: SINGLETON_SIMULATOR_VERSION.into(),
        receipt_hash: String::new(),
    };
    outcome.receipt_hash = canonical_hash(&(&bound.binding_hash, tx, budget, &outcome))?;
    control_check(&runner.control, "receipt").map_err(|e| runner.observe_error(e))?;
    Ok(outcome)
}

fn verify_hook(
    image: &core::StateImage,
    pool: &Pool,
    bound: &v4_single::BoundV4Swap,
) -> Result<()> {
    let hook = pool
        .singleton_advanced
        .as_ref()
        .filter(|state| !state.hook_model.is_empty())
        .map(|state| &state.snapshot.hooks)
        .or_else(|| {
            pool.infinity_bin
                .as_ref()
                .filter(|state| !state.hook_model.is_empty())
                .map(|state| &state.hooks)
        });
    let Some(hook) = hook else {
        return Ok(());
    };
    let invalid = || failure(SimulationCodeMismatch, "hook_runtime", hook);
    let info = image
        .data()
        .accounts
        .get(&address(hook)?)
        .and_then(Option::as_ref)
        .ok_or_else(invalid)?;
    let bytes = image
        .data()
        .bytecodes
        .get(&info.code_hash)
        .ok_or_else(invalid)?;
    // These are the runtime templates and immutables checked by the Go collector.
    let (length, digest, manager_offsets) = if bound.infinity.is_some() {
        (
            1354,
            "b19fdab8fb2b4ef318403ac9001afeaa4c89ebe1efbd1ede4e20a6e7b73482d8",
            [844, 1121],
        )
    } else {
        (
            901,
            "713061d76fed7f5ec294ea9d743f8b803b8f085e0d13deaa2a99f28262aaca3f",
            [146, 459],
        )
    };
    if bytes.len() != length {
        return Err(invalid());
    }
    let mut normalized = bytes.to_vec();
    let mut normalize = |target, offsets: [usize; 2]| -> Result<()> {
        let expected = address_word(target)?;
        for offset in offsets {
            let word = &mut normalized[offset..offset + 32];
            if word != expected {
                return Err(invalid());
            }
            word.fill(0);
        }
        Ok(())
    };
    normalize(&bound.manager, manager_offsets)?;
    if let Some(vault) = &bound.infinity {
        normalize(&vault.vault, [929, 1169])?;
    }
    if hex::encode(Sha256::digest(normalized)) != digest {
        return Err(invalid());
    }
    Ok(())
}
