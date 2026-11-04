use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use xone_domain::{err, hash, Amount, Result, SnapshotRef};
use xone_optimizer::Allocation;

mod bound_v2;
pub use bound_v2::{
    bind_v2_execution, validate_bound_execution, BoundEvmExecution, ExecutorIdentity,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub chain_key: String,
    pub caller: String,
    pub recipient: String,
    pub sell_token: String,
    pub buy_token: String,
    pub slippage_bps: u16,
    pub deadline_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub context: Context,
    pub snapshot: SnapshotRef,
    pub allocation: Allocation,
    pub min_buy_amount: Amount,
    pub economic_policy_hash: String,
    pub plan_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub chain_key: String,
    pub from: String,
    pub to: String,
    pub data: String,
    pub value: Amount,
}
pub fn build(
    context: &Context,
    allocation: &Allocation,
    snapshot: &SnapshotRef,
    policy: &str,
) -> Result<Plan> {
    if context.chain_key != snapshot.block.chain_key
        || context.caller.is_empty()
        || context.recipient.is_empty()
        || context.sell_token == context.buy_token
        || context.slippage_bps > 10000
    {
        return Err(err("INVALID_EXECUTION_CONTEXT"));
    }
    let minimum = Amount::checked(
        &allocation.gross_buy_amount.0 * BigUint::from(10000 - context.slippage_bps)
            / BigUint::from(10000u64),
    )?;
    let hash = hash(
        &serde_json::json!({"context":context,"snapshot":snapshot,"allocation":allocation,"min_buy_amount":minimum,"economic_policy_hash":policy}),
    )?;
    Ok(Plan {
        context: context.clone(),
        snapshot: snapshot.clone(),
        allocation: allocation.clone(),
        min_buy_amount: minimum,
        economic_policy_hash: policy.into(),
        plan_hash: hash,
    })
}
/// Recompute the original economic commitment and exact slippage floor.
/// This intentionally accepts generic quote contexts; EVM validation belongs to binding.
pub fn verify_plan_hash(plan: &Plan) -> Result<()> {
    let expected = build(
        &plan.context,
        &plan.allocation,
        &plan.snapshot,
        &plan.economic_policy_hash,
    )?;
    if plan.min_buy_amount != expected.min_buy_amount || plan.plan_hash != expected.plan_hash {
        return Err(err("PLAN_HASH_MISMATCH"));
    }
    Ok(())
}

fn address(s: &str) -> Result<[u8; 20]> {
    let b = hex::decode(
        s.strip_prefix("0x")
            .ok_or_else(|| err("INVALID_EVM_ADDRESS"))?,
    )
    .map_err(|_| err("INVALID_EVM_ADDRESS"))?;
    b.try_into().map_err(|_| err("INVALID_EVM_ADDRESS"))
}
fn word(v: &BigUint) -> Result<[u8; 32]> {
    Amount::checked(v.clone())?.to_be_bytes()
}
/// Build a standard V2 router call only for a single fully ordered route.
/// Split routes require a separately audited batch executor and are rejected here.
pub fn evm_transaction(plan: &Plan, target: &str, allowlist: &[String]) -> Result<Transaction> {
    verify_plan_hash(plan)?;
    if !allowlist.iter().any(|v| v.eq_ignore_ascii_case(target)) {
        return Err(err("EXECUTION_TARGET_NOT_ALLOWED"));
    }
    address(target)?;
    address(&plan.context.caller)?;
    let recipient = address(&plan.context.recipient)?;
    if !plan.context.chain_key.starts_with("evm:") || plan.allocation.routes.len() != 1 {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let route = &plan.allocation.routes[0];
    let legacy =
        !route.path.edges.is_empty() && route.path.edges.iter().all(|e| e.protocol == "uniswap-v2");
    let complete = route.path.edges.len() == 1
        && route.path.tokens.len() == 2
        && route.path.edges[0].protocol == xone_domain::v2_swap::PROFILE;
    if !(legacy || complete) {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let mut data = vec![0x38, 0xed, 0x17, 0x39];
    data.extend(word(&route.amount_in.0)?);
    data.extend(word(&plan.min_buy_amount.0)?);
    data.extend(word(&BigUint::from(160u64))?);
    data.extend([0u8; 12]);
    data.extend(recipient);
    data.extend(word(&BigUint::from(plan.context.deadline_ms / 1000))?);
    data.extend(word(&BigUint::from(route.path.tokens.len()))?);
    for token in &route.path.tokens {
        data.extend([0u8; 12]);
        data.extend(address(token)?);
    }
    Ok(Transaction {
        chain_key: plan.context.chain_key.clone(),
        from: plan.context.caller.clone(),
        to: target.into(),
        data: format!("0x{}", hex::encode(data)),
        value: Amount::zero(),
    })
}
