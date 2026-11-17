//! Bounded, block-hash pinned RPC observations for the declared Router02 adapter.
//! RPC agreement is provenance, not consensus or independent execution proof.
use crate::evm::{self, FeeContext, FundingObservation};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use xone_domain::{err, hash, v2_swap, Amount, Pool, Result, SnapshotRef};
use xone_execution_plan::{
    BoundEvmExactOutExecution, BoundEvmExecution, Context, ExecutorIdentity, Transaction,
};

#[derive(Clone, Debug)]
pub struct RpcConfig {
    pub url: String,
    pub chain_key: String,
    pub genesis_hash: String,
    pub executor: ExecutorIdentity,
    pub token_code_sha256: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub max_calls: usize,
    pub max_response_bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(with = "evm::decimal_u64")]
    pub gas_limit: u64,
    pub max_fee_per_gas: Amount,
    pub max_priority_fee_per_gas: Amount,
    pub max_network_fee: Amount,
}
#[derive(Clone, Debug, Serialize)]
pub struct RpcObservation {
    pub method: String,
    pub params: Value,
    pub result: Value,
}
#[derive(Clone, Debug, Serialize)]
pub struct CapturedPreparation {
    pub result: Value,
    pub observations: Vec<RpcObservation>,
}
pub enum Bound<'a> {
    ExactIn(&'a BoundEvmExecution),
    ExactOut(&'a BoundEvmExactOutExecution),
}
impl Bound<'_> {
    fn parts(
        &self,
    ) -> (
        &Context,
        &SnapshotRef,
        &ExecutorIdentity,
        &Transaction,
        &[String],
    ) {
        match self {
            Self::ExactIn(b) => (
                &b.plan.context,
                &b.plan.snapshot,
                &b.executor,
                &b.transaction,
                &b.touched_pool_ids,
            ),
            Self::ExactOut(b) => (
                &b.plan.context,
                &b.plan.snapshot,
                &b.executor,
                &b.transaction,
                &b.touched_pool_ids,
            ),
        }
    }
}
fn clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn data(value: &Value, expected: Option<usize>) -> Result<Vec<u8>> {
    let text = value.as_str().ok_or_else(|| err("EVM_RPC_DATA"))?;
    if !text.starts_with("0x")
        || text.len() % 2 != 0
        || expected.is_some_and(|n| text.len() != 2 + 2 * n)
    {
        return Err(err("EVM_RPC_DATA"));
    }
    hex::decode(&text[2..]).map_err(|_| err("EVM_RPC_DATA"))
}
fn quantity(value: &Value) -> Result<Amount> {
    let text = value.as_str().ok_or_else(|| err("EVM_RPC_QUANTITY"))?;
    let digits = text
        .strip_prefix("0x")
        .ok_or_else(|| err("EVM_RPC_QUANTITY"))?;
    if digits.is_empty()
        || digits.len() > 64
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(err("EVM_RPC_QUANTITY"));
    }
    let padded = format!("{:0>64}", digits);
    Amount::from_be_bytes(&hex::decode(padded).map_err(|_| err("EVM_RPC_QUANTITY"))?)
}
fn uint64(value: &Value) -> Result<u64> {
    let amount = quantity(value)?;
    let bytes = amount.to_be_bytes()?;
    if bytes[..24] != [0; 24] {
        return Err(err("EVM_RPC_QUANTITY_WIDTH"));
    }
    Ok(u64::from_be_bytes(
        bytes[24..]
            .try_into()
            .map_err(|_| err("EVM_RPC_QUANTITY_WIDTH"))?,
    ))
}
fn word_address(address: &str) -> Result<String> {
    xone_domain::protocols::validate_address(address)?;
    Ok(format!("{}{}", "0".repeat(24), &address[2..]))
}
fn address_result(value: &Value, expected: &str) -> Result<()> {
    let bytes = data(value, Some(32))?;
    if bytes[..12] != [0; 12] || format!("0x{}", hex::encode(&bytes[12..])) != expected {
        return Err(err("EVM_RPC_CONTRACT_IDENTITY"));
    }
    Ok(())
}
fn digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && text.bytes().any(|b| b != b'0')
}
struct Rpc {
    client: reqwest::Client,
    url: String,
    max_calls: usize,
    bytes_left: usize,
    anchor: Value,
    caller: String,
    observations: Vec<RpcObservation>,
}
impl Rpc {
    async fn exchange(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<std::result::Result<Value, ()>> {
        if self.observations.len() >= self.max_calls {
            return Err(err("EVM_RPC_CALL_BUDGET"));
        }
        let id = self.observations.len() + 1;
        let mut response = self
            .client
            .post(&self.url)
            .json(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| err("EVM_RPC_UNAVAILABLE"))?;
        if !response.status().is_success() {
            return Err(err("EVM_RPC_HTTP"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| err("EVM_RPC_UNAVAILABLE"))?
        {
            if chunk.len() > self.bytes_left {
                return Err(err("EVM_RPC_RESPONSE_BUDGET"));
            }
            self.bytes_left -= chunk.len();
            bytes.extend_from_slice(&chunk);
        }
        let body = crate::evm_rpc_json::parse(&bytes)?;
        if body["jsonrpc"] != "2.0"
            || body["id"] != id
            || (body.get("result").is_some() == body.get("error").is_some())
        {
            return Err(err("EVM_RPC_ENVELOPE"));
        }
        if let Some(error) = body.get("error") {
            if error["code"].as_i64().is_none() || error["message"].as_str().is_none() {
                return Err(err("EVM_RPC_ENVELOPE"));
            }
            // Retain a value-free error marker; provider messages can contain credentials.
            self.observations.push(RpcObservation {
                method: method.into(),
                params,
                result: json!({"rpcError":true}),
            });
            return Ok(Err(()));
        }
        let result = body["result"].clone();
        self.observations.push(RpcObservation {
            method: method.into(),
            params,
            result: result.clone(),
        });
        Ok(Ok(result))
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.exchange(method, params)
            .await?
            .map_err(|_| err("EVM_RPC_METHOD_FAILED"))
    }
    async fn contract(&mut self, target: &str, calldata: &str) -> Result<Value> {
        self.call(
            "eth_call",
            json!([{"from":self.caller,"to":target,"data":calldata},self.anchor]),
        )
        .await
    }
    async fn code(&mut self, target: &str, expected: &str, empty: bool) -> Result<()> {
        let value = self
            .call("eth_getCode", json!([target, self.anchor]))
            .await?;
        let raw = data(&value, None)?;
        if empty {
            if !raw.is_empty() {
                return Err(err("EVM_RPC_CONTRACT_CALLER_UNSUPPORTED"));
            }
        } else if raw.is_empty()
            || raw.len() > 24576
            || format!("{:x}", Sha256::digest(raw)) != expected
        {
            return Err(err("EVM_RPC_CODE_CHANGED"));
        }
        Ok(())
    }
    async fn header(&mut self, snapshot: &SnapshotRef) -> Result<Value> {
        let header = self
            .call(
                "eth_getBlockByNumber",
                json!([format!("0x{:x}", snapshot.block.number_or_slot), false]),
            )
            .await?;
        if uint64(&header["number"])? != snapshot.block.number_or_slot
            || header["hash"] != snapshot.block.hash
            || header["parentHash"] != snapshot.block.parent_hash
            || uint64(&header["timestamp"])?.checked_mul(1000) != Some(snapshot.block.timestamp_ms)
            || data(&header["stateRoot"], Some(32))?
                .iter()
                .all(|b| *b == 0)
        {
            return Err(err("EVM_RPC_BLOCK_CHANGED"));
        }
        quantity(&header["baseFeePerGas"])?;
        Ok(header)
    }
    async fn pool(&mut self, pool: &Pool) -> Result<()> {
        if pool.protocol != v2_swap::PROFILE {
            return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
        }
        let identity = pool
            .v2
            .as_ref()
            .ok_or_else(|| err("V2_IDENTITY_REQUIRED"))?;
        let state = identity
            .swap
            .as_ref()
            .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
        self.code(&pool.pool_id, &identity.code_sha256, false)
            .await?;
        for (selector, expected) in [
            ("0xc45a0155", &identity.factory),
            ("0x0dfe1681", &pool.token0),
            ("0xd21220a7", &pool.token1),
        ] {
            address_result(&self.contract(&pool.pool_id, selector).await?, expected)?;
        }
        for (from, to) in [(&pool.token0, &pool.token1), (&pool.token1, &pool.token0)] {
            let calldata = format!("0xe6a43905{}{}", word_address(from)?, word_address(to)?);
            address_result(
                &self.contract(&identity.factory, &calldata).await?,
                &pool.pool_id,
            )?;
        }
        let reserves = data(&self.contract(&pool.pool_id, "0x0902f1ac").await?, Some(96))?;
        for (index, expected) in [
            (0, pool.reserve0.clone()),
            (1, pool.reserve1.clone()),
            (2, Amount::checked(state.block_timestamp_last.into())?),
        ] {
            if Amount::from_be_bytes(&reserves[index * 32..(index + 1) * 32])? != expected {
                return Err(err("QUOTE_STATE_CHANGED"));
            }
        }
        for (target, calldata, expected) in [
            (
                &pool.token0,
                format!("0x70a08231{}", word_address(&pool.pool_id)?),
                state.raw_balance0.clone(),
            ),
            (
                &pool.token1,
                format!("0x70a08231{}", word_address(&pool.pool_id)?),
                state.raw_balance1.clone(),
            ),
            (
                &pool.pool_id,
                "0x5909c0d5".into(),
                state.price0_cumulative_last.clone(),
            ),
            (
                &pool.pool_id,
                "0x5a3d5493".into(),
                state.price1_cumulative_last.clone(),
            ),
        ] {
            if Amount::from_be_bytes(&data(&self.contract(target, &calldata).await?, Some(32))?)?
                != expected
            {
                return Err(err("QUOTE_STATE_CHANGED"));
            }
        }
        let unlocked = self
            .call(
                "eth_getStorageAt",
                json!([pool.pool_id, "0xc", self.anchor]),
            )
            .await?;
        if Amount::from_be_bytes(&data(&unlocked, Some(32))?)? != Amount::checked(1u8.into())? {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
        Ok(())
    }
}

pub async fn prepare(
    config: &RpcConfig,
    bound: Bound<'_>,
    pools: &[Pool],
    request: Request,
) -> Result<CapturedPreparation> {
    let (context, snapshot, executor, _, _) = bound.parts();
    if context.chain_key != config.chain_key
        || serde_json::to_value(executor).map_err(|_| err("HASH_ENCODING"))?
            != serde_json::to_value(&config.executor).map_err(|_| err("HASH_ENCODING"))?
        || !(100..=15000).contains(&config.timeout_ms)
        || !(32..=256).contains(&config.max_calls)
        || !(65536..=16777216).contains(&config.max_response_bytes)
        || !config.genesis_hash.starts_with("0x")
        || !digest(&config.genesis_hash[2..])
    {
        return Err(err("EVM_RPC_CONFIG"));
    }
    let parsed = reqwest::Url::parse(&config.url).map_err(|_| err("EVM_RPC_CONFIG"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.fragment().is_some()
        || !digest(&executor.code_sha256)
        || !digest(&executor.factory_code_sha256)
        || request.gas_limit > 15_000_000
        || request.gas_limit < 21000
        || request.max_priority_fee_per_gas > request.max_fee_per_gas
    {
        return Err(err("EVM_RPC_CONFIG"));
    }
    for value in [
        &request.max_fee_per_gas,
        &request.max_priority_fee_per_gas,
        &request.max_network_fee,
    ] {
        Amount::checked(value.0.clone())?;
    }
    for pool in pools
        .iter()
        .filter(|p| bound.parts().4.contains(&p.pool_id))
    {
        if [
            &pool.pool_id,
            &pool.token0,
            &pool.token1,
            &executor.factory,
            &executor.target,
        ]
        .iter()
        .any(|address| **address == context.caller || **address == context.recipient)
        {
            return Err(err("EXECUTION_FUNDS_CONTEXT"));
        }
    }
    let allowed = vec![config.executor.target.clone()];
    match &bound {
        Bound::ExactIn(b) => xone_execution_plan::validate_bound_execution(b, pools, &allowed)?,
        Bound::ExactOut(b) => {
            xone_execution_plan::validate_bound_exact_out_execution(b, pools, &allowed)?
        }
    }
    let remaining = context
        .deadline_ms
        .checked_sub(clock_ms())
        .filter(|ms| *ms > 0)
        .ok_or_else(|| err("QUOTE_EXPIRED"))?;
    let limit = Duration::from_millis(config.timeout_ms.min(remaining));
    let started = Instant::now();
    let result = tokio::time::timeout(limit, resolve(config, &bound, pools, &request, snapshot))
        .await
        .map_err(|_| err("EVM_RPC_TIMEOUT"))??;
    // A ready future may finish synchronous JSON/hash work before Tokio polls
    // its timer again. Do not publish a result past the aggregate wall budget.
    if started.elapsed() >= limit {
        return Err(err("EVM_RPC_TIMEOUT"));
    }
    Ok(result)
}
async fn resolve(
    config: &RpcConfig,
    bound: &Bound<'_>,
    pools: &[Pool],
    request: &Request,
    snapshot: &SnapshotRef,
) -> Result<CapturedPreparation> {
    let (context, _, executor, _, touched) = bound.parts();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .map_err(|_| err("EVM_RPC_CONFIG"))?;
    let mut rpc = Rpc {
        client,
        url: config.url.clone(),
        max_calls: config.max_calls,
        bytes_left: config.max_response_bytes,
        anchor: json!({"blockHash":snapshot.block.hash,"requireCanonical":true}),
        caller: context.caller.clone(),
        observations: Vec::new(),
    };
    let chain = uint64(&rpc.call("eth_chainId", json!([])).await?)?;
    if format!("evm:{chain}") != config.chain_key {
        return Err(err("EVM_RPC_WRONG_CHAIN"));
    }
    let genesis = rpc
        .call("eth_getBlockByNumber", json!(["0x0", false]))
        .await?;
    if genesis["hash"] != config.genesis_hash || uint64(&genesis["number"])? != 0 {
        return Err(err("EVM_RPC_WRONG_GENESIS"));
    }
    let header = rpc.header(snapshot).await?;
    let base_fee = quantity(&header["baseFeePerGas"])?;
    // A fresh impossible selector detects providers ignoring EIP-1898. It does
    // not prove provider honesty, consensus or independent failure domains.
    let mut random = [0u8; 32];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| err("EVM_RPC_CONTROL_ENTROPY"))?;
    let control = json!({"blockHash":format!("0x{}",hex::encode(random)),"requireCanonical":true});
    for (method, params) in [
        ("eth_getBalance", json!([context.caller, control])),
        ("eth_getCode", json!([executor.target, control])),
        (
            "eth_getStorageAt",
            json!([executor.factory, "0x0", control]),
        ),
        (
            "eth_call",
            json!([{"from":context.caller,"to":executor.target,"data":"0xc45a0155"},control]),
        ),
    ] {
        if rpc.exchange(method, params).await?.is_ok() {
            return Err(err("EVM_RPC_HASH_SELECTOR_IGNORED"));
        }
    }
    rpc.code(&context.caller, "", true).await?;
    rpc.code(&executor.target, &executor.code_sha256, false)
        .await?;
    rpc.code(&executor.factory, &executor.factory_code_sha256, false)
        .await?;
    address_result(
        &rpc.contract(&executor.target, "0xc45a0155").await?,
        &executor.factory,
    )?;
    let mut tokens = BTreeSet::new();
    for id in touched {
        let pool = pools
            .iter()
            .find(|pool| &pool.pool_id == id)
            .ok_or_else(|| err("MISSING_POOL"))?;
        rpc.pool(pool).await?;
        tokens.insert(pool.token0.clone());
        tokens.insert(pool.token1.clone());
    }
    for token in tokens {
        let pin = config
            .token_code_sha256
            .get(&token)
            .filter(|p| digest(p))
            .ok_or_else(|| err("EVM_RPC_TOKEN_NOT_ADMITTED"))?;
        rpc.code(&token, pin, false).await?;
    }
    let fixed_nonce = uint64(
        &rpc.call(
            "eth_getTransactionCount",
            json!([context.caller, rpc.anchor]),
        )
        .await?,
    )?;
    let pending = uint64(
        &rpc.call(
            "eth_getTransactionCount",
            json!([context.caller, "pending"]),
        )
        .await?,
    )?;
    if pending != fixed_nonce {
        return Err(err("EVM_RPC_PENDING_TRANSACTIONS"));
    }
    let native = quantity(
        &rpc.call("eth_getBalance", json!([context.caller, rpc.anchor]))
            .await?,
    )?;
    let balance_call = format!("0x70a08231{}", word_address(&context.caller)?);
    let allowance_call = format!(
        "0xdd62ed3e{}{}",
        word_address(&context.caller)?,
        word_address(&executor.target)?
    );
    let token_balance = Amount::from_be_bytes(&data(
        &rpc.contract(&context.sell_token, &balance_call).await?,
        Some(32),
    )?)?;
    let allowance = Amount::from_be_bytes(&data(
        &rpc.contract(&context.sell_token, &allowance_call).await?,
        Some(32),
    )?)?;
    let observed = FundingObservation {
        snapshot_hash: hash(snapshot)?,
        caller: context.caller.clone(),
        sell_token: context.sell_token.clone(),
        spender: executor.target.clone(),
        pending_nonce: pending,
        native_balance: native,
        token_balance,
        allowance,
        base_fee_per_gas: base_fee,
    };
    let fees = FeeContext {
        chain_id: chain,
        nonce: pending,
        gas_limit: request.gas_limit,
        max_fee_per_gas: request.max_fee_per_gas.clone(),
        max_priority_fee_per_gas: request.max_priority_fee_per_gas.clone(),
        max_network_fee: request.max_network_fee.clone(),
    };
    let component = match bound {
        Bound::ExactIn(b) => serde_json::to_value(evm::prepare(
            b,
            pools,
            std::slice::from_ref(&executor.target),
            &observed,
            &fees,
            clock_ms(),
        )?),
        Bound::ExactOut(b) => serde_json::to_value(evm::prepare_exact_out(
            b,
            pools,
            std::slice::from_ref(&executor.target),
            &observed,
            &fees,
            clock_ms(),
        )?),
    }
    .map_err(|_| err("HASH_ENCODING"))?;
    // Recheck canonical bank and live nonce after all reads, even for blocked funding.
    if rpc.header(snapshot).await? != header {
        return Err(err("EVM_RPC_BLOCK_CHANGED"));
    }
    if uint64(
        &rpc.call(
            "eth_getTransactionCount",
            json!([context.caller, "pending"]),
        )
        .await?,
    )? != pending
    {
        return Err(err("PREPARATION_EVM_NONCE_CHANGED"));
    }
    if uint64(&rpc.call("eth_chainId", json!([])).await?)? != chain {
        return Err(err("EVM_RPC_WRONG_CHAIN"));
    }
    if clock_ms() >= context.deadline_ms {
        return Err(err("QUOTE_EXPIRED"));
    }
    let capture_hash = hash(&rpc.observations)?;
    let mut result = json!({"schema_version":1,"component":component,
        "rpc_observation":{"schemaVersion":1,"authority":"HASH_PINNED_RPC_OBSERVATION","snapshotHash":hash(snapshot)?,
        "captureHash":capture_hash,"observedAtMs":clock_ms().to_string(),"calls":rpc.observations.len(),"consensusVerified":false,
        "independentExecutionVerified":false,"currentNonceRechecked":true,"chainFeesComplete":false}});
    // Preserve the component hash as a nested commitment; this outer hash binds
    // the RPC evidence as well. No URL, credentials or provider error text is exposed.
    result["rpc_preparation_hash"] = hash(&result)?.into();
    Ok(CapturedPreparation {
        result,
        observations: rpc.observations,
    })
}
