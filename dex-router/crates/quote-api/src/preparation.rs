//! Read-only, snapshot-bound account preparation and unsigned legacy Solana
//! messages for admitted single-pool Meteora instructions. Never signs/sends.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use num_traits::ToPrimitive;
use sha2::{Digest, Sha256};
use xone_domain::{meteora as m, Pool};
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const IMMUTABLE: &str = "BPFLoader2111111111111111111111111111111111";
const SYSTEM: &str = "11111111111111111111111111111111";
const ASSOCIATED: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    source_token_account: Option<String>,
    destination_token_account: Option<String>,
}
#[derive(Clone)]
struct Selection {
    view: Arc<QuoteView>,
    context: Context,
    pool: Pool,
    plan_hash: String,
    zero: bool,
    exact: bool,
    amount: u64,
    limit: u64,
}
use xone_execution_plan::solana_unsigned::{
    compile_message, unsigned_packet, SolanaAccountMeta as Meta, SolanaInstruction as Instruction,
};

struct Account {
    owner: String,
    data: Vec<u8>,
    lamports: u64,
    executable: bool,
}
fn bad() -> xone_domain::Error {
    err("PREPARATION_ACCOUNT_INVALID")
}
fn select(api: &Api, id: &str) -> Result<Selection> {
    if let Some(c) = api
        .cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned()
    {
        xone_execution_plan::verify_plan_hash(&c.plan)?;
        if c.expires <= now_ms() {
            return Err(err("QUOTE_EXPIRED"));
        }
        if c.plan.allocation.routes.len() != 1 {
            return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
        }
        let route = &c.plan.allocation.routes[0];
        if route.path.edges.len() != 1 {
            return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
        }
        return selected(
            c.view,
            c.plan.context,
            c.plan.plan_hash,
            &route.path.edges[0],
            false,
            &route.amount_in,
            &c.plan.min_buy_amount,
        );
    }
    let c = api
        .exact_out_cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned()
        .ok_or_else(|| err("UNKNOWN_QUOTE"))?;
    xone_execution_plan::exact_out::verify(&c.plan)?;
    if c.expires <= now_ms() {
        return Err(err("QUOTE_EXPIRED"));
    }
    if c.plan.allocation.routes.len() != 1 {
        return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
    }
    let route = &c.plan.allocation.routes[0];
    if route.edges.len() != 1 {
        return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
    }
    selected(
        c.view,
        c.plan.context,
        c.plan.plan_hash,
        &route.edges[0],
        true,
        route.output_target(),
        &c.plan.max_sell_amount,
    )
}
fn selected(
    view: Arc<QuoteView>,
    context: Context,
    plan_hash: String,
    edge: &xone_routing::Edge,
    exact: bool,
    amount: &Amount,
    limit: &Amount,
) -> Result<Selection> {
    let pool = view
        .pools
        .iter()
        .find(|p| p.pool_id == edge.pool_id)
        .cloned()
        .ok_or_else(bad)?;
    if !m::supports_profile(&pool.protocol) || edge.from_token.is_some() {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    pool.validate()?;
    Ok(Selection {
        view,
        context,
        pool,
        plan_hash,
        zero: edge.zero_for_one,
        exact,
        amount: amount.0.to_u64().ok_or_else(bad)?,
        limit: limit.0.to_u64().ok_or_else(bad)?,
    })
}
fn current(api: &Api, s: &Selection) -> Result<()> {
    api.check_view(&s.view)?;
    if now_ms() >= s.context.deadline_ms {
        return Err(err("QUOTE_EXPIRED"));
    }
    if api.replica.view(&s.context.chain_key)?.view_id != s.view.view_id {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    Ok(())
}
fn ata(owner: &str, mint: &str, token: &str) -> Result<String> {
    m::pda(
        &[&m::pubkey(owner)?, &m::pubkey(token)?, &m::pubkey(mint)?],
        ASSOCIATED,
    )
}
struct Rpc {
    client: reqwest::Client,
    url: String,
}
impl Rpc {
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| err("PREPARATION_RPC_UNAVAILABLE"))?;
        if !response.status().is_success() {
            return Err(err("PREPARATION_RPC_UNAVAILABLE"));
        }
        let mut response = response;
        let mut raw = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| err("PREPARATION_RPC_UNAVAILABLE"))?
        {
            if raw.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(err("PREPARATION_RPC_LIMIT"));
            }
            raw.extend_from_slice(&chunk);
        }
        let body: Value =
            serde_json::from_slice(&raw).map_err(|_| err("PREPARATION_RPC_INVALID"))?;
        if body.get("error").is_some() || body.get("result").is_none() || body["id"] != 1 {
            return Err(err("PREPARATION_RPC_INVALID"));
        }
        Ok(body["result"].clone())
    }
}
fn account(v: &Value) -> Result<Option<Account>> {
    if v.is_null() {
        return Ok(None);
    }
    let owner = v["owner"].as_str().ok_or_else(bad)?.to_string();
    if owner != SYSTEM {
        m::pubkey(&owner)?;
    }
    if v["data"][1] != "base64" {
        return Err(bad());
    }
    let data = STANDARD
        .decode(v["data"][0].as_str().ok_or_else(bad)?)
        .map_err(|_| bad())?;
    Ok(Some(Account {
        owner,
        data,
        lamports: v["lamports"].as_u64().ok_or_else(bad)?,
        executable: v["executable"].as_bool().ok_or_else(bad)?,
    }))
}
fn get<'a>(bank: &'a BTreeMap<String, Option<Account>>, id: &str) -> Result<&'a Account> {
    bank.get(id).and_then(Option::as_ref).ok_or_else(bad)
}
fn raw(
    bank: &BTreeMap<String, Option<Account>>,
    id: &str,
    owner: &str,
    expected: &[u8],
) -> Result<()> {
    let a = get(bank, id)?;
    if a.owner != owner || a.executable || a.data != expected {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    Ok(())
}
fn code(
    bank: &BTreeMap<String, Option<Account>>,
    program: &str,
    expected: &str,
    slot: u64,
    deployed: Option<u64>,
) -> Result<()> {
    let a = get(bank, program)?;
    if !a.executable {
        return Err(bad());
    }
    let bytes = if a.owner == IMMUTABLE {
        &a.data[..]
    } else {
        if a.owner != LOADER || a.data.len() != 36 || a.data[..4] != [2, 0, 0, 0] {
            return Err(bad());
        }
        let pd = m::pda(&[&m::pubkey(program)?], LOADER)?;
        if m::key(&a.data, 4) != pd {
            return Err(bad());
        }
        let a = get(bank, &pd)?;
        if a.owner != LOADER
            || a.executable
            || a.data.len() <= 45
            || a.data[..4] != [3, 0, 0, 0]
            || a.data[12] > 1
            || m::u64_at(&a.data, 4) > slot
            || deployed.is_some_and(|d| d != m::u64_at(&a.data, 4))
        {
            return Err(bad());
        }
        &a.data[45..]
    };
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err(err("PREPARATION_PROGRAM_CHANGED"));
    }
    Ok(())
}
fn token(a: &Account, mint: &str, owner: &str, token: &str) -> Result<u64> {
    let n = if token == TOKEN { 165 } else { 178 };
    if a.executable
        || a.owner != token
        || a.data.len() != n
        || m::key(&a.data, 0) != mint
        || m::key(&a.data, 32) != owner
        || a.data[108] != 1
        || a.data[72..76] != [0; 4]
        || a.data[129..133] != [0; 4]
        || m::u64_at(&a.data, 121) != 0
    {
        return Err(bad());
    }
    if n == 178 && a.data[165..170] != [2, 2, 0, 8, 0] {
        return Err(bad());
    }
    let amount = m::u64_at(&a.data, 64);
    if mint == xone_domain::pump_amm::WSOL {
        if a.data[109..113] != [1, 0, 0, 0]
            || a.lamports
                != m::u64_at(&a.data, 113)
                    .checked_add(amount)
                    .ok_or_else(bad)?
        {
            return Err(bad());
        }
    } else if a.data[109..113] != [0; 4] {
        return Err(bad());
    }
    Ok(amount)
}
pub(super) async fn prepare(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<Request>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure(err("RATE_LIMITED")))?;
    let s = select(&api, &id).map_err(failure)?;
    current(&api, &s).map_err(failure)?;
    let deadline = Duration::from_millis(api.config.config.api.quote_timeout_ms.clamp(1000, 15000));
    let result = tokio::time::timeout(deadline, prepare_inner(&api, &s, request))
        .await
        .map_err(|_| failure(err("PREPARATION_RPC_TIMEOUT")))?
        .map_err(failure)?;
    current(&api, &s).map_err(failure)?;
    let mut result = result;
    result["quoteId"] = id.into();
    result
        .as_object_mut()
        .ok_or_else(|| failure(bad()))?
        .remove("preparationHash");
    result["preparationHash"] = hash(&result).map_err(failure)?.into();
    Ok(Json(result))
}
async fn prepare_inner(api: &Api, s: &Selection, request: Request) -> Result<Value> {
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == s.context.chain_key)
        .ok_or_else(bad)?;
    let url = chain
        .rpc_urls
        .first()
        .ok_or_else(|| err("PREPARATION_RPC_NOT_CONFIGURED"))?
        .clone();
    let rpc = Rpc {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| bad())?,
        url,
    };
    if rpc.call("getGenesisHash", json!([])).await?.as_str() != Some(chain.genesis_hash.as_str()) {
        return Err(err("PREPARATION_WRONG_CHAIN"));
    }
    let p = &s.pool;
    let m = p.meteora.as_ref().ok_or_else(bad)?;
    let d = m.pair()?;
    let tx = if m.token2022.is_some() {
        xone_domain::meteora_token2022::PROGRAM
    } else {
        TOKEN
    };
    let (input, output, input_program, output_program) = if s.zero {
        (&p.token0, &p.token1, tx, TOKEN)
    } else {
        (&p.token1, &p.token0, TOKEN, tx)
    };
    let src = request
        .source_token_account
        .unwrap_or(ata(&s.context.caller, input, input_program)?);
    let dst = request.destination_token_account.unwrap_or(ata(
        &s.context.recipient,
        output,
        output_program,
    )?);
    if src.len() > 44 || dst.len() > 44 {
        return Err(bad());
    }
    m::pubkey(&src)?;
    m::pubkey(&dst)?;
    let mut ids = vec![
        p.pool_id.clone(),
        m::key(&d, 152),
        m::key(&d, 184),
        m::key(&d, 552),
        p.token0.clone(),
        p.token1.clone(),
        s.context.caller.clone(),
        src.clone(),
        dst.clone(),
        m::PROGRAM.into(),
        m.programdata.clone(),
        TOKEN.into(),
        m::pda(&[&m::pubkey(TOKEN)?], LOADER)?,
        MEMO.into(),
        "SysvarC1ock11111111111111111111111111111111".into(),
    ];
    ids.extend(m.bin_arrays.iter().map(|a| a.address.clone()));
    if let Some(b) = &m.bitmap_extension {
        ids.push(b.address.clone());
    }
    if let Some(t) = &m.token2022 {
        ids.extend([tx.to_string(), t.programdata.clone()]);
    }
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    if ids.len() > 100 {
        return Err(err("PREPARATION_ACCOUNT_LIMIT"));
    }
    let response = rpc
        .call(
            "getMultipleAccounts",
            json!([ids,{"encoding":"base64","commitment":"finalized","minContextSlot":m.slot}]),
        )
        .await?;
    let slot = response["context"]["slot"].as_u64().ok_or_else(bad)?;
    if slot < m.slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let values = response["value"].as_array().ok_or_else(bad)?;
    if ids.len() != values.len() {
        return Err(bad());
    }
    let bank = ids
        .iter()
        .zip(values)
        .map(|(id, v)| Ok((id.clone(), account(v)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    raw(&bank, &p.pool_id, m::PROGRAM, &d)?;
    raw(
        &bank,
        &m::key(&d, 552),
        m::PROGRAM,
        &hex::decode(&m.oracle_data).map_err(|_| bad())?,
    )?;
    for a in &m.bin_arrays {
        raw(
            &bank,
            &a.address,
            m::PROGRAM,
            &hex::decode(&a.data).map_err(|_| bad())?,
        )?;
    }
    if let Some(b) = &m.bitmap_extension {
        raw(
            &bank,
            &b.address,
            m::PROGRAM,
            &hex::decode(&b.data).map_err(|_| bad())?,
        )?;
    }
    code(
        &bank,
        m::PROGRAM,
        &m.program_code_sha256,
        slot,
        Some(m.deployed_slot),
    )?;
    code(&bank, TOKEN, &m.token_program_code_sha256, slot, None)?;
    if let Some(t) = &m.token2022 {
        raw(
            &bank,
            &m::key(&d, 152),
            tx,
            &hex::decode(&t.vault_data).map_err(|_| bad())?,
        )?;
        code(
            &bank,
            tx,
            &t.program_code_sha256,
            slot,
            Some(t.deployed_slot),
        )?;
        raw(
            &bank,
            &p.token0,
            tx,
            &hex::decode(&t.mint_data).map_err(|_| bad())?,
        )?;
    }
    let clock = get(&bank, "SysvarC1ock11111111111111111111111111111111")?;
    if clock.executable
        || clock.data.len() != 40
        || clock.owner != "Sysvar1111111111111111111111111111111111111"
        || m::u64_at(&clock.data, 0) != slot
        || m::u64_at(&clock.data, 32) != m.timestamp
        || m.token2022
            .as_ref()
            .is_some_and(|t| m::u64_at(&clock.data, 16) != t.bank_epoch)
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    for (mint, program) in [(&p.token0, tx), (&p.token1, TOKEN)] {
        let a = get(&bank, mint)?;
        if a.owner != program
            || a.executable
            || a.data.len() < 82
            || a.data[45] != 1
            || (program == TOKEN && a.data.len() != 82)
        {
            return Err(bad());
        }
    }
    for (id, mint, program, balance) in [
        (m::key(&d, 152), &p.token0, tx, m.raw_vault0),
        (m::key(&d, 184), &p.token1, TOKEN, m.raw_vault1),
    ] {
        if token(get(&bank, &id)?, mint, &p.pool_id, program)? != balance {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    if !get(&bank, MEMO)?.executable {
        return Err(bad());
    }
    let mut blockers = Vec::new();
    let required = if s.exact { s.limit } else { s.amount };
    let source = bank.get(&src).and_then(Option::as_ref);
    if let Some(a) = source {
        if token(a, input, &s.context.caller, input_program)? < required {
            blockers.push("INSUFFICIENT_TOKEN_BALANCE");
        }
    } else {
        blockers.push("SOURCE_ACCOUNT_MISSING");
    }
    if let Some(a) = bank.get(&dst).and_then(Option::as_ref) {
        token(a, output, &s.context.recipient, output_program)?;
    } else {
        blockers.push("DESTINATION_ACCOUNT_MISSING");
    }
    let payer = get(&bank, &s.context.caller)?;
    if payer.owner != SYSTEM || payer.executable || !payer.data.is_empty() {
        return Err(bad());
    }
    let mut result = json!({"quoteId":s.view.view_id,"planHash":s.plan_hash,"quoteViewId":s.view.view_id,"snapshotRef":s.view.reference,"observedSlot":slot,"ready":false,"blockers":blockers,"sourceTokenAccount":src,"destinationTokenAccount":dst,"requiredInput":required.to_string(),"payerLamports":payer.lamports.to_string(),"simulationRequired":true,"signed":false,"submitted":false,"unsignedTransaction":Value::Null});
    if !blockers.is_empty() {
        return Ok(result);
    }
    let block = rpc
        .call(
            "getLatestBlockhash",
            json!([{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if block["context"]["slot"].as_u64().ok_or_else(bad)? < slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let blockhash = block["value"]["blockhash"].as_str().ok_or_else(bad)?;
    let last = block["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(bad)?;
    let mut metas = vec![];
    for (key, signer, writable) in [
        (p.pool_id.clone(), false, true),
        (
            m.bitmap_extension
                .as_ref()
                .map_or(m::PROGRAM.to_string(), |b| b.address.clone()),
            false,
            m.bitmap_extension.is_some(),
        ),
        (m::key(&d, 152), false, true),
        (m::key(&d, 184), false, true),
        (src.clone(), false, true),
        (dst.clone(), false, true),
        (p.token0.clone(), false, false),
        (p.token1.clone(), false, false),
        (m::key(&d, 552), false, true),
        (m::PROGRAM.into(), false, false),
        (s.context.caller.clone(), true, false),
        (tx.into(), false, false),
        (TOKEN.into(), false, false),
        (MEMO.into(), false, false),
        (m::pda(&[b"__event_authority"], m::PROGRAM)?, false, false),
        (m::PROGRAM.into(), false, false),
    ] {
        metas.push(Meta {
            pubkey: key,
            is_signer: signer,
            is_writable: writable,
        });
    }
    let mut arrays = m.bin_arrays.clone();
    arrays.sort_by_key(|a| a.index);
    if s.zero {
        arrays.reverse();
    }
    metas.extend(arrays.into_iter().map(|a| Meta {
        pubkey: a.address,
        is_signer: false,
        is_writable: true,
    }));
    let mut data = if s.exact {
        vec![43, 215, 247, 132, 137, 60, 243, 81]
    } else {
        vec![65, 75, 63, 76, 235, 91, 91, 136]
    };
    for n in if s.exact {
        [s.limit, s.amount]
    } else {
        [s.amount, s.limit]
    } {
        data.extend(n.to_le_bytes());
    }
    data.extend([0; 4]);
    let ix = Instruction {
        program_id: m::PROGRAM.into(),
        accounts: metas,
        data_base64: STANDARD.encode(data),
    };
    let msg = compile_message(&s.context.caller, blockhash, &ix)?;
    let fee = rpc
        .call(
            "getFeeForMessage",
            json!([STANDARD.encode(&msg),{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if fee["context"]["slot"].as_u64().ok_or_else(bad)? < slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let cost = fee["value"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    if payer.lamports < cost {
        result["blockers"] = json!(["INSUFFICIENT_FEE_BALANCE"]);
        return Ok(result);
    }
    let transaction = unsigned_packet(&msg)?;
    result["ready"] = true.into();
    result["feeLamports"] = cost.to_string().into();
    result["recentBlockhash"] = blockhash.into();
    result["lastValidBlockHeight"] = last.into();
    result["instructions"] = json!([ix]);
    result["messageBase64"] = STANDARD.encode(msg).into();
    result["unsignedTransaction"] = STANDARD.encode(transaction).into();
    result["requiredSigners"] = json!([s.context.caller]);
    result["preparationHash"] = hash(&result)?.into();
    Ok(result)
}
