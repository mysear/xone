//! EVM preparation over explicitly supplied observations. The caller must
//! acquire and authenticate funding/fee observations before using this component.
//! Independent execution authority is provided by the execution backend.
use serde::{Deserialize, Serialize};
use xone_domain::{err, hash, Amount, Pool, Result};
use xone_execution_plan::{
    validate_bound_exact_out_execution, validate_bound_execution, BoundEvmExactOutExecution,
    BoundEvmExecution,
};

mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, output: S) -> Result<S::Ok, S::Error> {
        output.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<u64, D::Error> {
        let value = String::deserialize(input)?;
        if value.is_empty()
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(serde::de::Error::custom("NON_CANONICAL_U64_STRING"));
        }
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingObservation {
    pub snapshot_hash: String,
    pub caller: String,
    pub sell_token: String,
    pub spender: String,
    #[serde(with = "decimal_u64")]
    pub pending_nonce: u64,
    pub native_balance: Amount,
    pub token_balance: Amount,
    pub allowance: Amount,
    pub base_fee_per_gas: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeContext {
    #[serde(with = "decimal_u64")]
    pub chain_id: u64,
    #[serde(with = "decimal_u64")]
    pub nonce: u64,
    #[serde(with = "decimal_u64")]
    pub gas_limit: u64,
    pub max_fee_per_gas: Amount,
    pub max_priority_fee_per_gas: Amount,
    pub max_network_fee: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Eip1559Transaction {
    #[serde(with = "decimal_u64")]
    pub chain_id: u64,
    #[serde(with = "decimal_u64")]
    pub nonce: u64,
    #[serde(with = "decimal_u64")]
    pub gas_limit: u64,
    pub max_fee_per_gas: Amount,
    pub max_priority_fee_per_gas: Amount,
    pub from: String,
    pub to: String,
    pub value: Amount,
    pub data: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preparation {
    pub schema_version: u32,
    pub plan_hash: String,
    pub binding_hash: String,
    pub snapshot_hash: String,
    pub funding_observation_hash: String,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub required_input: Amount,
    pub max_network_fee: Amount,
    pub transaction: Option<Eip1559Transaction>,
    pub unsigned_transaction: Option<String>,
    pub rpc_transaction: Option<serde_json::Value>,
    pub required_signers: Vec<String>,
    pub signed: bool,
    pub submitted: bool,
    pub independent_production_verification: bool,
    pub preparation_hash: String,
}
impl Eip1559Transaction {
    /// Wallet/RPC parameters use canonical hex quantities, including wide nonce
    /// and fee values. The unsigned payload remains the signing commitment.
    pub fn rpc_parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"0x2", "chainId":format!("0x{:x}",self.chain_id),
            "nonce":format!("0x{:x}",self.nonce), "gas":format!("0x{:x}",self.gas_limit),
            "maxFeePerGas":format!("0x{:x}",self.max_fee_per_gas.0),
            "maxPriorityFeePerGas":format!("0x{:x}",self.max_priority_fee_per_gas.0),
            "from":self.from, "to":self.to, "value":format!("0x{:x}",self.value.0),
            "data":self.data,"accessList":[]
        })
    }
}

fn integer(value: &Amount) -> Vec<u8> {
    let full = value.0.to_bytes_be();
    full.iter()
        .position(|v| *v != 0)
        .map_or(Vec::new(), |n| full[n..].to_vec())
}
fn rlp_length(length: usize, short: u8, long: u8) -> Vec<u8> {
    if length < 56 {
        return vec![short + length as u8];
    }
    let full = length.to_be_bytes();
    let start = full
        .iter()
        .position(|v| *v != 0)
        .expect("nonzero long RLP length");
    let bytes = &full[start..];
    let mut out = vec![long + bytes.len() as u8];
    out.extend_from_slice(bytes);
    out
}
fn rlp_string(value: &[u8]) -> Vec<u8> {
    if value.len() == 1 && value[0] < 0x80 {
        return value.to_vec();
    }
    let mut out = rlp_length(value.len(), 0x80, 0xb7);
    out.extend_from_slice(value);
    out
}
fn word(value: u64) -> Amount {
    Amount::checked(value.into()).expect("u64 fits uint256")
}
fn address(value: &str) -> Result<Vec<u8>> {
    xone_domain::protocols::validate_address(value)?;
    let bytes = hex::decode(&value[2..]).map_err(|_| err("PREPARATION_EVM_ADDRESS"))?;
    if bytes.iter().all(|v| *v == 0) {
        return Err(err("PREPARATION_EVM_ADDRESS"));
    }
    Ok(bytes)
}
fn unsigned(tx: &Eip1559Transaction) -> Result<String> {
    let data = hex::decode(
        tx.data
            .strip_prefix("0x")
            .ok_or_else(|| err("PREPARATION_EVM_DATA"))?,
    )
    .map_err(|_| err("PREPARATION_EVM_DATA"))?;
    if data.len() > 128 * 1024 {
        return Err(err("PREPARATION_EVM_DATA_LIMIT"));
    }
    let mut fields = Vec::new();
    for item in [
        &word(tx.chain_id),
        &word(tx.nonce),
        &tx.max_priority_fee_per_gas,
        &tx.max_fee_per_gas,
        &word(tx.gas_limit),
    ] {
        fields.extend(rlp_string(&integer(item)));
    }
    fields.extend(rlp_string(&address(&tx.to)?));
    fields.extend(rlp_string(&integer(&tx.value)));
    fields.extend(rlp_string(&data));
    fields.push(0xc0); // Explicit empty access list; no caller-supplied entries.
    let mut bytes = vec![2];
    bytes.extend(rlp_length(fields.len(), 0xc0, 0xf7));
    bytes.extend(fields);
    Ok(format!("0x{}", hex::encode(bytes)))
}
pub fn prepare(
    bound: &BoundEvmExecution,
    pools: &[Pool],
    allowed_targets: &[String],
    observed: &FundingObservation,
    fees: &FeeContext,
    now_ms: u64,
) -> Result<Preparation> {
    // Rebuild the original full plan binding, calldata and math context first.
    // This is internal consistency, not independent execution verification.
    validate_bound_execution(bound, pools, allowed_targets)?;
    prepare_bound(
        &bound.plan.context,
        &bound.plan.snapshot,
        &bound.plan.plan_hash,
        &bound.binding_hash,
        &bound.transaction,
        &bound.plan.allocation.sell_amount,
        observed,
        fees,
        now_ms,
    )
}
#[allow(clippy::too_many_arguments)]
fn prepare_bound(
    context: &xone_execution_plan::Context,
    snapshot: &xone_domain::SnapshotRef,
    plan_hash: &str,
    binding_hash: &str,
    transaction: &xone_execution_plan::Transaction,
    input: &Amount,
    observed: &FundingObservation,
    fees: &FeeContext,
    now_ms: u64,
) -> Result<Preparation> {
    if fees.chain_id == 0
        || context.chain_key != format!("evm:{}", fees.chain_id)
        || transaction.chain_key != context.chain_key
    {
        return Err(err("PREPARATION_EVM_CHAIN"));
    }
    if now_ms >= context.deadline_ms || now_ms < snapshot.block.timestamp_ms {
        return Err(err("QUOTE_EXPIRED"));
    }
    let snapshot_hash = hash(snapshot)?;
    if observed.snapshot_hash != snapshot_hash
        || observed.caller != context.caller
        || observed.sell_token != context.sell_token
        || observed.spender != transaction.to
    {
        return Err(err("PREPARATION_EVM_OBSERVATION_BINDING"));
    }
    if fees.nonce != observed.pending_nonce {
        return Err(err("PREPARATION_EVM_NONCE_CHANGED"));
    }
    // EIP-2681 permits an account nonce at u64::MAX but no transaction
    // using that nonce: processing it would overflow the account counter.
    if fees.nonce == u64::MAX {
        return Err(err("PREPARATION_EVM_NONCE_EXHAUSTED"));
    }
    if fees.gas_limit < 21000
        || fees.gas_limit > 15_000_000
        || fees.max_priority_fee_per_gas > fees.max_fee_per_gas
        || observed.base_fee_per_gas > fees.max_fee_per_gas
        || fees.max_fee_per_gas == Amount::zero()
    {
        return Err(err("PREPARATION_EVM_FEE_CONTEXT"));
    }
    for value in [
        &observed.native_balance,
        &observed.token_balance,
        &observed.allowance,
        &observed.base_fee_per_gas,
        &fees.max_fee_per_gas,
        &fees.max_priority_fee_per_gas,
        &fees.max_network_fee,
    ] {
        Amount::checked(value.0.clone())?;
    }
    let fee_reserve = Amount::checked(&fees.max_fee_per_gas.0 * fees.gas_limit)?;
    if fee_reserve > fees.max_network_fee {
        return Err(err("PREPARATION_EVM_FEE_BUDGET"));
    }
    if transaction.value != Amount::zero() || transaction.from != context.caller {
        return Err(err("PREPARATION_EVM_TRANSACTION_BINDING"));
    }
    address(&context.caller)?;
    address(&context.sell_token)?;
    let input = input.clone();
    let mut blockers = Vec::new();
    if observed.native_balance < fee_reserve {
        blockers.push("INSUFFICIENT_EVM_FEE_BALANCE".into());
    }
    if observed.token_balance < input {
        blockers.push("INSUFFICIENT_EVM_TOKEN_BALANCE".into());
    }
    if observed.allowance < input {
        blockers.push("EVM_APPROVAL_REQUIRED".into());
    }
    let ready = blockers.is_empty();
    let tx = Eip1559Transaction {
        chain_id: fees.chain_id,
        nonce: fees.nonce,
        gas_limit: fees.gas_limit,
        max_fee_per_gas: fees.max_fee_per_gas.clone(),
        max_priority_fee_per_gas: fees.max_priority_fee_per_gas.clone(),
        from: transaction.from.clone(),
        to: transaction.to.clone(),
        value: transaction.value.clone(),
        data: transaction.data.clone(),
    };
    let (wire, tx) = if ready {
        (Some(unsigned(&tx)?), Some(tx))
    } else {
        (None, None)
    };
    let mut prepared = Preparation {
        schema_version: 1,
        plan_hash: plan_hash.into(),
        binding_hash: binding_hash.into(),
        snapshot_hash,
        funding_observation_hash: hash(observed)?,
        ready,
        blockers,
        required_input: input,
        max_network_fee: fee_reserve,
        rpc_transaction: tx.as_ref().map(Eip1559Transaction::rpc_parameters),
        transaction: tx,
        unsigned_transaction: wire,
        required_signers: vec![context.caller.clone()],
        signed: false,
        submitted: false,
        independent_production_verification: false,
        preparation_hash: String::new(),
    };
    let mut commitment = serde_json::to_value(&prepared).map_err(|_| err("HASH_ENCODING"))?;
    commitment
        .as_object_mut()
        .expect("preparation serializes as object")
        .remove("preparation_hash");
    prepared.preparation_hash = hash(&commitment)?;
    Ok(prepared)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutPreparation {
    pub preparation: Preparation,
    pub target_output: Amount,
    pub max_sell_amount: Amount,
    pub unused_quoted_input_budget: Amount,
    pub exact_out_preparation_hash: String,
}
pub fn prepare_exact_out(
    bound: &BoundEvmExactOutExecution,
    pools: &[Pool],
    allowed: &[String],
    observed: &FundingObservation,
    fees: &FeeContext,
    now_ms: u64,
) -> Result<ExactOutPreparation> {
    validate_bound_exact_out_execution(bound, pools, allowed)?;
    let p = &bound.plan;
    let prepared = prepare_bound(
        &p.context,
        &p.snapshot,
        &p.plan_hash,
        &bound.binding_hash,
        &bound.transaction,
        &p.allocation.required_input,
        observed,
        fees,
        now_ms,
    )?;
    let mut out = ExactOutPreparation {
        preparation: prepared,
        target_output: p.allocation.target_output.clone(),
        max_sell_amount: p.max_sell_amount.clone(),
        unused_quoted_input_budget: p.allocation.unused_input_budget.clone(),
        exact_out_preparation_hash: String::new(),
    };
    let mut value = serde_json::to_value(&out).map_err(|_| err("HASH_ENCODING"))?;
    value
        .as_object_mut()
        .expect("preparation object")
        .remove("exact_out_preparation_hash");
    out.exact_out_preparation_hash = hash(&value)?;
    Ok(out)
}
