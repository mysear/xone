//! Verify the deposit principal against the pinned WETH9 balance slot at the source block.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};
use xone_domain::{err, hash, Amount, Result};
use xone_execution_plan::{ordered_v2, Context, Transaction};
use xone_execution_preparation::evm_native_cost::{
    audited_payer_cost, measured_native_cost, sequence_funding, NativeBalances, NativeCost,
};
use xone_simulation::{
    evm_fixture::{NativeFeeMeasurements, WrapReceipt},
    worker::{LocalSimulationBundle, LocalWrapReplay},
};

pub(super) struct Source {
    model: LocalWrapReplay,
    opening_tokens: Amount,
    caller: Amount,
    wrapper: Amount,
    beneficiary: Amount,
    swap_beneficiary: Amount,
    first: xone_simulation::evm_fixture::ExecutionEnvironment,
}

impl Source {
    pub fn load(bundle: &LocalSimulationBundle, cap: u64) -> Result<Option<Self>> {
        let Some(model) = &bundle.wrap_replay else {
            return Ok(None);
        };
        let bad = || err("SIMULATION_WRAP_SOURCE");
        let fixture = &bundle.fixture;
        let first = &fixture.execution_environment;
        let next = &model.sequence.swap_environment;
        let token = &model.sequence.wrapper;
        let caller = &model.caller;
        let deposit = &model.sequence.deposit_context;
        let swap = &model.swap_context;
        if model.sequence.first_approval_environment.is_some()
            || bundle.approval_replay.is_some()
            || bundle.op_attributes_calldata.is_some()
            || caller == token
            || first.fork != "shanghai"
            || next.fork != first.fork
            || next.chain_id != first.chain_id
            || first.block_number.checked_add(1) != Some(next.block_number)
            || next.parent_hash != first.block_hash
            || next.block_hash == first.block_hash
            || next.block_hash == fixture.state_anchor.block_hash
            || next.timestamp <= first.timestamp
            || next.timestamp.checked_mul(1000).is_none()
            || deposit.nonce.checked_add(1) != Some(swap.nonce)
        {
            return Err(bad());
        }
        for (tx, env) in [(deposit, first), (swap, next)] {
            if tx.chain_id != env.chain_id
                || tx.gas_limit > env.gas_limit.min(cap)
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
        }
        let account = |addr: &str| {
            fixture
                .accounts
                .iter()
                .find(|a| a.address == addr)
                .ok_or_else(bad)
        };
        if !account(caller)?.exists || account(caller)?.nonce != deposit.nonce {
            return Err(bad());
        }
        let wrapper = account(token)?;
        let code =
            hex::decode(wrapper.code.strip_prefix("0x").ok_or_else(bad)?).map_err(|_| bad())?;
        if !wrapper.exists || hex::encode(Sha256::digest(code)) != ordered_v2::WETH9_CODE_SHA256 {
            return Err(bad());
        }
        xone_domain::protocols::validate_address(caller)?;
        let mut mapping = [0_u8; 64];
        let owner: [u8; 20] = hex::decode(caller.strip_prefix("0x").ok_or_else(bad)?)
            .map_err(|_| bad())?
            .try_into()
            .map_err(|_| bad())?;
        mapping[12..32].copy_from_slice(&owner);
        mapping[63] = 3;
        let mut hasher = Keccak::v256();
        hasher.update(&mapping);
        let mut key = [0; 32];
        hasher.finalize(&mut key);
        let slot = format!("0x{}", hex::encode(key));
        let stored = &wrapper
            .storage
            .iter()
            .find(|s| s.slot == slot)
            .ok_or_else(bad)?
            .value;
        let opening_tokens = Amount::from_be_bytes(
            &hex::decode(stored.strip_prefix("0x").ok_or_else(bad)?).map_err(|_| bad())?,
        )?;
        Ok(Some(Self {
            model: model.clone(),
            opening_tokens,
            caller: account(caller)?.balance.clone(),
            wrapper: wrapper.balance.clone(),
            beneficiary: account(&first.coinbase)?.balance.clone(),
            swap_beneficiary: account(&next.coinbase)?.balance.clone(),
            first: first.clone(),
        }))
    }
    pub fn for_plan(&self, ctx: &Context, input: &Amount) -> Option<&LocalWrapReplay> {
        (self.model.caller == ctx.caller
            && self.model.sequence.wrapper == ctx.sell_token
            && input > &self.opening_tokens)
            .then_some(&self.model)
    }
    pub fn commitment(&self, ctx: &Context, input: &Amount) -> Result<Commitment> {
        let expected = ordered_v2::wrapped_input_transaction(
            ctx,
            input,
            &self.model.sequence.wrapper,
            &self.opening_tokens,
        )?
        .ok_or_else(|| err("SIMULATION_WRAP_SOURCE"))?;
        Ok(Commitment {
            expected,
            input: input.clone(),
            opening_tokens: self.opening_tokens.clone(),
            caller: self.caller.clone(),
            wrapper: self.wrapper.clone(),
            beneficiary: self.beneficiary.clone(),
            swap_beneficiary: self.swap_beneficiary.clone(),
            model: self.model.clone(),
            first: self.first.clone(),
        })
    }
}

pub(super) struct Commitment {
    expected: Transaction,
    input: Amount,
    opening_tokens: Amount,
    caller: Amount,
    wrapper: Amount,
    beneficiary: Amount,
    swap_beneficiary: Amount,
    model: LocalWrapReplay,
    first: xone_simulation::evm_fixture::ExecutionEnvironment,
}
impl Commitment {
    pub fn audit(
        &self,
        receipt: &WrapReceipt,
        swap: &NativeFeeMeasurements,
        swap_fee: &NativeCost,
        costs: &mut Value,
    ) -> Result<()> {
        let bad = || err("SIMULATION_WRAP_RECEIPT");
        let tx = &self.model.sequence.deposit_context;
        let next = &self.model.sequence.swap_environment;
        let principal = &self.expected.value;
        let token = &self.expected.to;
        let caller = &self.expected.from;
        let beneficiary = &self.first.coinbase;
        let fees = measured_native_cost(
            receipt.gas_used,
            tx.gas_limit,
            &self.first.base_fee,
            &tx.gas_price,
            tx.max_priority_fee_per_gas.as_ref(),
        )?;
        let add = |a: &Amount, b: &Amount| Amount::checked(&a.0 + &b.0);
        if hash(&receipt.transaction)? != hash(&self.expected)?
            || receipt.token_before != self.opening_tokens
            || receipt.token_after != self.input
            || receipt.block_hash != self.first.block_hash
            || receipt.nonce != tx.nonce
            || receipt.execution_fee != fees.execution_fee
            || receipt.payer_before != self.caller
            || receipt.wrapper_before != self.wrapper
            || receipt.beneficiary_before != self.beneficiary
            || self.caller < *principal
        {
            return Err(bad());
        }
        let funded = Amount::checked(&self.caller.0 - &principal.0)?;
        let deposited = add(&self.wrapper, principal)?;
        let wrapper_after = if beneficiary == token {
            add(&deposited, &fees.priority_fee_paid)?
        } else {
            deposited.clone()
        };
        let adjusted = if beneficiary == caller {
            funded.clone()
        } else if beneficiary == token {
            deposited
        } else {
            self.beneficiary.clone()
        };
        let payer = audited_payer_cost(
            &fees,
            &NativeBalances {
                payer_before: funded,
                payer_after: receipt.payer_after.clone(),
                beneficiary_before: adjusted,
                beneficiary_after: receipt.beneficiary_after.clone(),
            },
            beneficiary == caller,
        )?;
        let swap_beneficiary = if &next.coinbase == caller {
            &receipt.payer_after
        } else if &next.coinbase == token {
            &receipt.wrapper_after
        } else if &next.coinbase == beneficiary {
            &receipt.beneficiary_after
        } else {
            &self.swap_beneficiary
        };
        if receipt.wrapper_after != wrapper_after
            || swap.caller_before != receipt.payer_after
            || &swap.coinbase_before != swap_beneficiary
        {
            return Err(bad());
        }
        let swap_debit: Amount =
            serde_json::from_value(costs["payerNetNativeDebit"].clone()).map_err(|_| bad())?;
        let swap_tip: Amount =
            serde_json::from_value(costs["priorityReturnedToPayerNative"].clone())
                .map_err(|_| bad())?;
        let (reserves, spent) = sequence_funding([
            (&fees.gas_limit_reserve, &payer.net_native_debit),
            (&swap_fee.gas_limit_reserve, &swap_debit),
        ])?;
        let opening = add(principal, &reserves)?;
        if self.caller < opening {
            return Err(bad());
        }
        costs["nativeKnownSubtotal"] = json!(add(&fees.execution_fee, &swap_fee.execution_fee)?);
        costs["payerNetNativeDebit"] = json!(spent);
        costs["priorityReturnedToPayerNative"] =
            json!(add(&swap_tip, &payer.priority_returned_to_payer)?);
        costs["scope"] = "DECLARED_LOCAL_WRAP_AND_SWAP".into();
        costs["settlementTimestampMs"] = json!(next.timestamp * 1000);
        costs["gasNativeScope"] = "SWAP_TRANSACTION".into();
        costs["wrapNative"] = json!(fees.execution_fee);
        costs["nativeFunding"] = json!({"scope":"DECLARED_WRAP_AND_SWAP","bindingHash":costs["bindingHash"],"payer":caller,"sourceBalanceNative":self.caller,"minimumOpeningBalanceNative":opening,"totalNetDebitNative":add(principal,&spent)?,"wrappedPrincipalNative":principal,"networkFeeDebitNative":spent,"additionalNativeRequired":"0"});
        costs["feeBreakdown"].as_array_mut().ok_or_else(bad)?.insert(0,json!({"kind":"WRAP_EXECUTION","amountNative":fees.execution_fee,"gasUnits":receipt.gas_used.to_string(),"blockHash":receipt.block_hash,"effectiveGasPrice":fees.effective_gas_price,"parts":{"baseFeeBurnedNative":fees.base_fee_burned,"priorityFeeNative":fees.priority_fee_paid}}));
        Ok(())
    }
}

pub(super) fn bind(
    source: Option<&Source>,
    req: &xone_simulation::worker::WorkerRequest,
    seq: Option<&xone_simulation::evm_fixture::WrapSequence>,
) -> Result<Option<Commitment>> {
    let bad = || err("SIMULATION_WRAP_SOURCE");
    let (ctx, input) = match (&req.bound, &req.exact_out_bound, &req.split) {
        (Some(b), None, None) => (&b.plan.context, &b.plan.allocation.sell_amount),
        (None, Some(b), None) => (&b.plan.context, &b.plan.allocation.required_input),
        (None, None, Some(s)) if req.direct_wrap.is_none() => match &s.bound.plan {
            ordered_v2::SplitPlan::ExactIn(p) => (&p.context, &p.allocation.sell_amount),
            ordered_v2::SplitPlan::ExactOut(p) => (&p.context, &p.allocation.required_input),
        },
        _ => return Err(bad()),
    };
    match (
        source.and_then(|s| s.for_plan(ctx, input).map(|m| (s, m))),
        seq,
    ) {
        (None, None) => Ok(None),
        (Some((s, m)), Some(w))
            if req.op_user.is_none()
                && req.prepared.is_none()
                && req
                    .split
                    .as_ref()
                    .is_none_or(|s| s.approval_sequence.is_none())
                && hash(w)? == hash(&m.sequence)?
                && hash(&req.transaction_context)? == hash(&m.swap_context)? =>
        {
            s.commitment(ctx, input).map(Some)
        }
        _ => Err(bad()),
    }
}
