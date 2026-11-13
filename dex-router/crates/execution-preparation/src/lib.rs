//! Dynamic Solana account resolution and unsigned preparation. No quote cache,
//! HTTP authorization, wallet keys, signing, or transaction submission lives here.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
mod classic_funding;
mod pump_amm;
mod pump_curve;
mod raydium;
use std::sync::{Arc, Mutex};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use xone_domain::{err, hash, meteora as m, Amount, Pool, Result, SnapshotRef};
use xone_execution_plan::{
    solana_unsigned::{
        compile_message_many, unsigned_packet, SolanaAccountMeta as Meta,
        SolanaInstruction as Instruction,
    },
    Context,
};
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const IMMUTABLE: &str = "BPFLoader2111111111111111111111111111111111";
const SYSTEM: &str = "11111111111111111111111111111111";
const ASSOCIATED: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const ASSOCIATED_CODE: &str = "6804554e69fd3a58caa191dc4a58f4c67223d30ca28ab8987f39fc18d2f7374d";
const RENT: &str = "SysvarRent111111111111111111111111111111111";
const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SolanaRequest {
    source_token_account: Option<String>,
    destination_token_account: Option<String>,
    #[serde(default)]
    create_destination_ata: bool,
    #[serde(default)]
    simulate: bool,
    #[serde(default)]
    wrap_source_sol: bool,
    #[serde(default)]
    refund_wrapped_source: bool,
    #[serde(default)]
    unwrap_destination_sol: bool,
}

#[derive(Clone, Debug)]
pub struct SolanaQuote {
    pub context: Context,
    pub pool: Pool,
    pub plan_hash: String,
    pub quote_view_id: String,
    pub snapshot: SnapshotRef,
    pub zero: bool,
    pub exact: bool,
    pub amount: u64,
    pub limit: u64,
    pub expected_input: u64,
    pub expected_output: u64,
}
#[derive(Clone, Debug)]
pub struct SolanaRpcConfig {
    pub url: String,
    pub genesis_hash: String,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
/// The owner verifies the cached plan hash and current QuoteView before and
/// after this resolver. The resolver independently checks its local contract.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountSnapshot {
    pub schema_version: u32,
    pub chain_key: String,
    pub genesis_hash: String,
    pub observed_slot: u64,
    pub accounts: BTreeMap<String, Value>,
}
impl AccountSnapshot {
    pub fn summary(&self) -> Result<Value> {
        Ok(
            json!({"schemaVersion":1,"source":"PREPARATION_RPC_CAPTURE","observedSlot":self.observed_slot,"accountCount":self.accounts.len(),"snapshotHash":hash(self)?,"historicalBankVerified":false}),
        )
    }
}
pub struct SolanaPreparation {
    pub result: Value,
    pub accounts: Option<AccountSnapshot>,
}
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Option<AccountSnapshot>>>);
pub async fn prepare_solana(
    config: &SolanaRpcConfig,
    s: &SolanaQuote,
    request: SolanaRequest,
) -> Result<Value> {
    Ok(prepare_solana_captured(config, s, request).await?.result)
}
pub async fn prepare_solana_captured(
    config: &SolanaRpcConfig,
    s: &SolanaQuote,
    request: SolanaRequest,
) -> Result<SolanaPreparation> {
    s.pool.validate()?;
    if !m::supports_profile(&s.pool.protocol)
        && s.pool.protocol != xone_domain::raydium::PROFILE
        && !xone_domain::pump_amm::supports_profile(&s.pool.protocol)
        && s.pool.protocol != xone_domain::pump::PROFILE
    {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let (sell, buy) = if s.zero {
        (&s.pool.token0, &s.pool.token1)
    } else {
        (&s.pool.token1, &s.pool.token0)
    };
    if s.context.chain_key != s.pool.chain_key
        || s.context.chain_key != s.snapshot.block.chain_key
        || &s.context.sell_token != sell
        || &s.context.buy_token != buy
        || s.plan_hash.len() != 64
        || !s
            .plan_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || s.expected_input == 0
        || s.expected_output == 0
        || (s.exact
            && ((s.amount != s.expected_output
                && !((xone_domain::pump_amm::supports_profile(&s.pool.protocol)
                    || s.pool.protocol == xone_domain::pump::PROFILE)
                    && s.zero
                    && s.amount < s.expected_output))
                || s.limit < s.expected_input))
        || (!s.exact && (s.amount != s.expected_input || s.limit > s.expected_output))
    {
        return Err(err("PREPARATION_QUOTE_INVALID"));
    }
    let remaining = s
        .context
        .deadline_ms
        .checked_sub(now_ms())
        .filter(|n| *n > 0)
        .ok_or_else(|| err("QUOTE_EXPIRED"))?;
    let capture = Capture::default();
    let result = tokio::time::timeout(
        Duration::from_millis(remaining.min(15000)),
        resolve(config, s, request, capture.clone()),
    )
    .await
    .map_err(|_| err("PREPARATION_RPC_TIMEOUT"))??;
    let mut captured = capture
        .0
        .lock()
        .map_err(|_| err("PREPARATION_CAPTURE_LOCK"))?
        .take();
    if result["ready"] != true {
        captured = None;
    }
    Ok(SolanaPreparation {
        result,
        accounts: captured,
    })
}
struct Account {
    owner: String,
    data: Vec<u8>,
    lamports: u64,
    executable: bool,
}
fn bad() -> xone_domain::Error {
    err("PREPARATION_ACCOUNT_INVALID")
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
    capture: Capture,
    chain_key: String,
    genesis_hash: String,
}
impl Rpc {
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":&params}))
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
            if raw.len() + chunk.len() > 16 * 1024 * 1024 {
                return Err(err("PREPARATION_RPC_LIMIT"));
            }
            raw.extend_from_slice(&chunk);
        }
        let body: Value =
            serde_json::from_slice(&raw).map_err(|_| err("PREPARATION_RPC_INVALID"))?;
        if body.get("error").is_some() || body.get("result").is_none() || body["id"] != 1 {
            return Err(err("PREPARATION_RPC_INVALID"));
        }
        let value = body["result"].clone();
        if method == "getMultipleAccounts" {
            let ids = params[0]
                .as_array()
                .filter(|a| !a.is_empty() && a.len() <= 100)
                .ok_or_else(bad)?;
            let values = value["value"]
                .as_array()
                .filter(|a| a.len() == ids.len())
                .ok_or_else(bad)?;
            let mut accounts = BTreeMap::new();
            for (id, entry) in ids.iter().zip(values) {
                let key = id.as_str().ok_or_else(bad)?;
                if accounts.insert(key.to_owned(), entry.clone()).is_some() {
                    return Err(bad());
                }
            }
            let snapshot = AccountSnapshot {
                schema_version: 1,
                chain_key: self.chain_key.clone(),
                genesis_hash: self.genesis_hash.clone(),
                observed_slot: value["context"]["slot"].as_u64().ok_or_else(bad)?,
                accounts,
            };
            let mut capture = self
                .capture
                .0
                .lock()
                .map_err(|_| err("PREPARATION_CAPTURE_LOCK"))?;
            if capture.is_some() {
                return Err(err("PREPARATION_CAPTURE_MULTIPLE_BANKS"));
            }
            *capture = Some(snapshot);
        }
        Ok(value)
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
    if a.executable
        || a.owner != token
        || a.data.len() < 165
        || m::key(&a.data, 0) != mint
        || m::key(&a.data, 32) != owner
        || a.data[108] != 1
        || a.data[72..76] != [0; 4]
        || a.data[129..133] != [0; 4]
        || m::u64_at(&a.data, 121) != 0
    {
        return Err(bad());
    }
    if token == TOKEN {
        if a.data.len() != 165 {
            return Err(bad());
        }
    } else if token == xone_domain::meteora_token2022::PROGRAM {
        withheld(a)?;
    } else {
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
// Only TransferFeeAmount and the ImmutableOwner added by the canonical ATA
// program are admitted. No unknown/duplicate/padded extension may be ignored.
fn withheld(a: &Account) -> Result<u64> {
    if a.owner != xone_domain::meteora_token2022::PROGRAM
        || !matches!(a.data.len(), 178 | 182)
        || a.data[165] != 2
    {
        return Err(bad());
    }
    let mut cursor = 166;
    let mut fee = None;
    let mut immutable = false;
    while cursor < a.data.len() {
        let header = a.data.get(cursor..cursor + 4).ok_or_else(bad)?;
        let kind = u16::from_le_bytes(header[..2].try_into().map_err(|_| bad())?);
        let size = usize::from(u16::from_le_bytes(
            header[2..].try_into().map_err(|_| bad())?,
        ));
        cursor += 4;
        let value = a.data.get(cursor..cursor + size).ok_or_else(bad)?;
        match (kind, size) {
            (2, 8) if fee.is_none() => {
                fee = Some(u64::from_le_bytes(value.try_into().map_err(|_| bad())?));
            }
            (7, 0) if !immutable => immutable = true,
            _ => return Err(bad()),
        }
        cursor += size;
    }
    fee.ok_or_else(bad)
}
async fn resolve(
    config: &SolanaRpcConfig,
    s: &SolanaQuote,
    request: SolanaRequest,
    capture: Capture,
) -> Result<Value> {
    if s.pool.protocol == xone_domain::pump::PROFILE {
        return pump_curve::resolve(config, s, request, capture).await;
    }
    if xone_domain::pump_amm::supports_profile(&s.pool.protocol) {
        return pump_amm::resolve(config, s, request, capture).await;
    }
    if s.pool.protocol == xone_domain::raydium::PROFILE {
        return raydium::resolve(config, s, request, capture).await;
    }
    let rpc = Rpc {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| bad())?,
        url: config.url.clone(),
        capture,
        chain_key: s.context.chain_key.clone(),
        genesis_hash: config.genesis_hash.clone(),
    };
    if rpc.call("getGenesisHash", json!([])).await?.as_str() != Some(config.genesis_hash.as_str()) {
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
    if request.wrap_source_sol
        && (input.as_str() != xone_domain::pump_amm::WSOL || input_program != TOKEN)
    {
        return Err(err("PREPARATION_WRAP_REQUIRES_WSOL"));
    }
    if request.refund_wrapped_source
        && (!request.wrap_source_sol || request.source_token_account.is_some())
    {
        return Err(err("PREPARATION_REFUND_REQUIRES_TEMP_SOURCE"));
    }
    if request.unwrap_destination_sol
        && (output.as_str() != xone_domain::pump_amm::WSOL
            || output_program != TOKEN
            || s.context.caller != s.context.recipient
            || request.destination_token_account.is_some()
            || request.create_destination_ata)
    {
        return Err(err("PREPARATION_UNWRAP_CONTEXT_UNSUPPORTED"));
    }
    let temporary_seed = if request.refund_wrapped_source || request.unwrap_destination_sol {
        Some(format!(
            "xone-{}-{}",
            if request.refund_wrapped_source {
                "in"
            } else {
                "out"
            },
            &s.plan_hash[..20]
        ))
    } else {
        None
    };
    let temporary = temporary_seed
        .as_ref()
        .map(|seed| xone_execution_plan::solana_wsol::address(&s.context.caller, seed))
        .transpose()?;
    let src = if request.refund_wrapped_source {
        temporary.clone().ok_or_else(bad)?
    } else {
        request
            .source_token_account
            .unwrap_or(ata(&s.context.caller, input, input_program)?)
    };
    let dst = if request.unwrap_destination_sol {
        temporary.clone().ok_or_else(bad)?
    } else {
        request.destination_token_account.unwrap_or(ata(
            &s.context.recipient,
            output,
            output_program,
        )?)
    };
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
        s.context.recipient.clone(),
        src.clone(),
        dst.clone(),
        m::PROGRAM.into(),
        m.programdata.clone(),
        TOKEN.into(),
        m::pda(&[&m::pubkey(TOKEN)?], LOADER)?,
        MEMO.into(),
        RENT.into(),
        m::pda(&[b"__event_authority"], m::PROGRAM)?,
        "SysvarC1ock11111111111111111111111111111111".into(),
    ];
    if request.create_destination_ata || request.wrap_source_sol || temporary.is_some() {
        ids.extend([ASSOCIATED.into(), SYSTEM.into()]);
    }
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
        raw(
            &bank,
            &m::key(&d, 152),
            tx,
            &hex::decode(&t.vault_data).map_err(|_| bad())?,
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
    if temporary
        .as_ref()
        .is_some_and(|address| bank.get(address).and_then(Option::as_ref).is_some())
    {
        return Err(err("PREPARATION_TEMP_ACCOUNT_OCCUPIED"));
    }
    let mut blockers = Vec::new();
    let required = if s.exact { s.limit } else { s.amount };
    let source = bank.get(&src).and_then(Option::as_ref);
    let source_balance = if let Some(a) = source {
        token(a, input, &s.context.caller, input_program)?
    } else {
        0
    };
    let create_source = source.is_none() && request.wrap_source_sol;
    if create_source
        && !request.refund_wrapped_source
        && src != ata(&s.context.caller, input, input_program)?
    {
        return Err(err("PREPARATION_CREATE_REQUIRES_ATA"));
    }
    let wrap = if request.wrap_source_sol {
        required.saturating_sub(source_balance)
    } else {
        0
    };
    if !request.wrap_source_sol {
        if source.is_none() {
            blockers.push("SOURCE_ACCOUNT_MISSING");
        } else if source_balance < required {
            blockers.push("INSUFFICIENT_TOKEN_BALANCE");
        }
    }
    let destination = bank.get(&dst).and_then(Option::as_ref);
    let existing_output = if let Some(a) = destination {
        token(a, output, &s.context.recipient, output_program)?
    } else {
        0
    };
    let create_destination = destination.is_none() && request.create_destination_ata;
    if create_destination && dst != ata(&s.context.recipient, output, output_program)? {
        return Err(err("PREPARATION_CREATE_REQUIRES_ATA"));
    }
    let creations = u64::from(create_source)
        + u64::from(create_destination)
        + u64::from(request.unwrap_destination_sol);
    let destination_size = if output_program == TOKEN {
        165u64
    } else {
        182u64
    };
    if creations > 0 || wrap > 0 {
        let system = get(&bank, SYSTEM)?;
        if !system.executable || system.owner != "NativeLoader1111111111111111111111111111111" {
            return Err(bad());
        }
    }
    if (create_source && !request.refund_wrapped_source) || create_destination {
        code(&bank, ASSOCIATED, ASSOCIATED_CODE, slot, None)?;
    }
    let mut rents = BTreeMap::new();
    for size in [
        (create_source || request.unwrap_destination_sol).then_some(165u64),
        create_destination.then_some(destination_size),
    ]
    .into_iter()
    .flatten()
    {
        if rents.contains_key(&size) {
            continue;
        }
        let rent = rpc
            .call(
                "getMinimumBalanceForRentExemption",
                json!([size,{"commitment":"finalized"}]),
            )
            .await?
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(bad)?;
        rents.insert(size, rent);
    }
    let source_rent = if create_source {
        *rents.get(&165).ok_or_else(bad)?
    } else {
        0
    };
    let destination_rent = if create_destination {
        *rents.get(&destination_size).ok_or_else(bad)?
    } else {
        0
    };
    let temporary_rent = if request.refund_wrapped_source {
        source_rent
    } else if request.unwrap_destination_sol {
        *rents.get(&165).ok_or_else(bad)?
    } else {
        0
    };
    let rent = source_rent
        .checked_add(destination_rent)
        .and_then(|n| {
            n.checked_add(if request.unwrap_destination_sol {
                temporary_rent
            } else {
                0
            })
        })
        .ok_or_else(bad)?;
    let source_withheld = source
        .filter(|_| input_program != TOKEN)
        .map(withheld)
        .transpose()?
        .unwrap_or(0);
    let destination_withheld = destination
        .filter(|_| output_program != TOKEN)
        .map(withheld)
        .transpose()?
        .unwrap_or(0);
    let destination_fee = if output_program == xone_domain::meteora_token2022::PROGRAM {
        let next_pool = if s.exact {
            xone_protocol_math::meteora::quote_exact_out(p, &Amount(s.amount.into()), s.zero)?
                .next_pool
        } else {
            xone_protocol_math::meteora::quote_exact_in(p, &Amount(s.amount.into()), s.zero)?
                .next_pool
        };
        let after = next_pool.meteora.as_ref().ok_or_else(bad)?;
        let gross = m.raw_vault0.checked_sub(after.raw_vault0).ok_or_else(bad)?;
        let fee = gross.checked_sub(s.expected_output).ok_or_else(bad)?;
        if m.token2022.as_ref().ok_or_else(bad)?.fee(gross)? != fee {
            return Err(bad());
        }
        fee
    } else {
        0
    };
    if destination.is_none() && !create_destination && !request.unwrap_destination_sol {
        blockers.push("DESTINATION_ACCOUNT_MISSING");
    }
    let payer = get(&bank, &s.context.caller)?;
    if payer.owner != SYSTEM || payer.executable || !payer.data.is_empty() {
        return Err(bad());
    }
    let mut result = json!({"quoteId":s.quote_view_id,"planHash":s.plan_hash,"quoteViewId":s.quote_view_id,"snapshotRef":s.snapshot,"observedSlot":slot,"ready":false,"blockers":blockers,"sourceTokenAccount":src,"destinationTokenAccount":dst,"requiredInput":required.to_string(),"quotedInput":s.expected_input.to_string(),"quotedOutput":s.expected_output.to_string(),"accountCreation":create_destination,"sourceAccountCreation":create_source,"wrapLamports":wrap.to_string(),"sourceWsolRetained":request.wrap_source_sol && !request.refund_wrapped_source,"refundWrappedSource":request.refund_wrapped_source,"unwrapDestinationSol":request.unwrap_destination_sol,"temporaryWsolAccount":temporary,"temporarySeed":temporary_seed,"settlementAsset":if request.unwrap_destination_sol{xone_domain::pump::NATIVE_SOL}else{output.as_str()},"rentRefundLamports":if temporary.is_some(){temporary_rent}else{0}.to_string(),"expectedNativeRefundLamports":if request.refund_wrapped_source{temporary_rent.checked_add(required.checked_sub(s.expected_input).ok_or_else(bad)?).ok_or_else(bad)?}else if request.unwrap_destination_sol{temporary_rent.checked_add(s.expected_output).ok_or_else(bad)?}else{0}.to_string(),"expectedResidualWsol":source_balance.checked_add(wrap).and_then(|n|n.checked_sub(s.expected_input)).unwrap_or(0).to_string(),"rentLamports":rent.to_string(),"payerLamports":payer.lamports.to_string(),"simulationRequired":true,"signed":false,"submitted":false,"unsignedTransaction":Value::Null});
    result["sourceTokenProgram"] = input_program.into();
    result["destinationTokenProgram"] = output_program.into();
    result["destinationAccountSize"] = destination
        .map_or(destination_size, |a| a.data.len() as u64)
        .into();
    result["destinationRentLamports"] = destination_rent.to_string().into();
    result["expectedDestinationTransferFee"] = destination_fee.to_string().into();
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
    let mut instructions = Vec::new();
    if let Some(seed) = &temporary_seed {
        instructions.extend(xone_execution_plan::solana_wsol::create(
            &s.context.caller,
            seed,
            temporary_rent,
        )?);
    }
    if create_source && !request.refund_wrapped_source {
        instructions.push(create_ata(
            &s.context.caller,
            &src,
            &s.context.caller,
            input,
            input_program,
        ));
    }
    if wrap > 0 {
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend(wrap.to_le_bytes());
        instructions.push(Instruction {
            program_id: SYSTEM.into(),
            accounts: vec![meta(&s.context.caller, true, true), meta(&src, false, true)],
            data_base64: STANDARD.encode(data),
        });
        instructions.push(Instruction {
            program_id: TOKEN.into(),
            accounts: vec![meta(&src, false, true)],
            data_base64: STANDARD.encode([17u8]),
        });
    }
    if create_destination {
        instructions.push(create_ata(
            &s.context.caller,
            &dst,
            &s.context.recipient,
            output,
            output_program,
        ));
    }
    instructions.push(ix);
    if let Some(seed) = &temporary_seed {
        instructions.push(xone_execution_plan::solana_wsol::close(
            &s.context.caller,
            seed,
        )?);
    }
    let msg = compile_message_many(&s.context.caller, blockhash, &instructions)?;
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
    let funding = cost
        .checked_add(rent)
        .and_then(|n| n.checked_add(wrap))
        .ok_or_else(bad)?;
    if payer.lamports < funding {
        result["blockers"] = json!([if payer.lamports < cost {
            "INSUFFICIENT_FEE_BALANCE"
        } else if payer.lamports < cost.checked_add(rent).ok_or_else(bad)? {
            "INSUFFICIENT_RENT_BALANCE"
        } else {
            "INSUFFICIENT_WRAP_BALANCE"
        }]);
        return Ok(result);
    }
    let transaction = unsigned_packet(&msg)?;
    result["ready"] = true.into();
    result["feeLamports"] = cost.to_string().into();
    result["recentBlockhash"] = blockhash.into();
    result["lastValidBlockHeight"] = last.into();
    result["instructions"] = json!(instructions);
    result["totalFundingLamports"] = funding.to_string().into();
    let persistent_rent = rent
        .checked_sub(if temporary.is_some() {
            temporary_rent
        } else {
            0
        })
        .ok_or_else(bad)?;
    result["expectedPayerLamportDelta"] = (if request.refund_wrapped_source {
        -(i128::from(cost) + i128::from(persistent_rent) + i128::from(s.expected_input))
    } else if request.unwrap_destination_sol {
        i128::from(s.expected_output) - i128::from(cost) - i128::from(persistent_rent)
    } else {
        -i128::from(funding)
    })
    .to_string()
    .into();
    if request.simulate {
        let mut addresses = vec![src.clone(), dst.clone()];
        if temporary.is_some() {
            addresses.push(s.context.caller.clone());
        }
        let response=rpc.call("simulateTransaction",json!([STANDARD.encode(&transaction),{"encoding":"base64","commitment":"finalized","minContextSlot":slot,"sigVerify":false,"replaceRecentBlockhash":false,"accounts":{"encoding":"base64","addresses":addresses}}])).await?;
        let simulation_slot = response["context"]["slot"]
            .as_u64()
            .filter(|n| *n >= slot)
            .ok_or_else(bad)?;
        let value = &response["value"];
        if value.get("err") != Some(&Value::Null) {
            return Err(err("PREPARATION_SIMULATION_FAILED"));
        }
        let accounts = value["accounts"]
            .as_array()
            .filter(|v| v.len() == addresses.len())
            .ok_or_else(bad)?;
        let native_after = if temporary.is_some() {
            let account = account(&accounts[2])?.ok_or_else(bad)?;
            if account.owner != SYSTEM || account.executable || !account.data.is_empty() {
                return Err(bad());
            }
            account.lamports
        } else {
            0
        };
        let persistent_rent = rent
            .checked_sub(if temporary.is_some() {
                temporary_rent
            } else {
                0
            })
            .ok_or_else(bad)?;
        let paid = if request.refund_wrapped_source {
            if !accounts[0].is_null() {
                return Err(err("PREPARATION_TEMP_ACCOUNT_NOT_CLOSED"));
            }
            payer
                .lamports
                .checked_sub(
                    native_after
                        .checked_add(cost)
                        .and_then(|n| n.checked_add(persistent_rent))
                        .ok_or_else(bad)?,
                )
                .ok_or_else(bad)?
        } else {
            let after = token(
                &account(&accounts[0])?.ok_or_else(bad)?,
                input,
                &s.context.caller,
                input_program,
            )?;
            source_balance
                .checked_add(wrap)
                .and_then(|n| n.checked_sub(after))
                .ok_or_else(bad)?
        };
        let received = if request.unwrap_destination_sol {
            if !accounts[1].is_null() {
                return Err(err("PREPARATION_TEMP_ACCOUNT_NOT_CLOSED"));
            }
            native_after
                .checked_add(cost)
                .and_then(|n| n.checked_add(persistent_rent))
                .and_then(|n| n.checked_sub(payer.lamports))
                .ok_or_else(bad)?
        } else {
            token(
                &account(&accounts[1])?.ok_or_else(bad)?,
                output,
                &s.context.recipient,
                output_program,
            )?
            .checked_sub(existing_output)
            .ok_or_else(bad)?
        };
        if paid != s.expected_input || received != s.expected_output {
            return Err(err("PREPARATION_SIMULATION_AMOUNT_MISMATCH"));
        }
        if input_program != TOKEN
            && withheld(&account(&accounts[0])?.ok_or_else(bad)?)? != source_withheld
        {
            return Err(err("PREPARATION_SIMULATION_FEE_MISMATCH"));
        }
        if output_program != TOKEN
            && withheld(&account(&accounts[1])?.ok_or_else(bad)?)?
                != destination_withheld
                    .checked_add(destination_fee)
                    .ok_or_else(bad)?
        {
            return Err(err("PREPARATION_SIMULATION_FEE_MISMATCH"));
        }

        result["simulation"] = json!({"level":"RPC_SIMULATED","success":true,"observedSlot":simulation_slot,"actualInput":paid.to_string(),"actualOutput":received.to_string(),"destinationTransferFee":destination_fee.to_string(),"settlementAsset":if request.unwrap_destination_sol{xone_domain::pump::NATIVE_SOL}else{output.as_str()},"temporaryAccountClosed":temporary.is_some(),"unitsConsumed":value["unitsConsumed"],"independentProductionVerification":false});
    }
    result["messageBase64"] = STANDARD.encode(msg).into();
    result["unsignedTransaction"] = STANDARD.encode(transaction).into();
    result["requiredSigners"] = json!([s.context.caller]);
    result["preparationHash"] = hash(&result)?.into();
    Ok(result)
}

fn meta(pubkey: &str, is_signer: bool, is_writable: bool) -> Meta {
    Meta {
        pubkey: pubkey.into(),
        is_signer,
        is_writable,
    }
}
fn create_ata(
    payer: &str,
    account: &str,
    owner: &str,
    mint: &str,
    token_program: &str,
) -> Instruction {
    Instruction {
        program_id: ASSOCIATED.into(),
        accounts: vec![
            meta(payer, true, true),
            meta(account, false, true),
            meta(owner, false, false),
            meta(mint, false, false),
            meta(SYSTEM, false, false),
            meta(token_program, false, false),
        ],
        data_base64: STANDARD.encode([1u8]),
    }
}

fn empty_system(a: &Account) -> bool {
    a.owner == SYSTEM && !a.executable && a.data.is_empty()
}
fn before_token_balance(
    before: Option<&Account>,
    after: &Account,
    mint: &str,
    owner: &str,
    program: &str,
    creating: bool,
) -> Result<u64> {
    match before {
        None => Ok(0),
        Some(a) if creating && empty_system(a) && program == TOKEN => {
            token(after, mint, owner, program)?;
            Ok(if mint == xone_domain::pump_amm::WSOL {
                a.lamports.saturating_sub(m::u64_at(&after.data, 113))
            } else {
                0
            })
        }
        Some(a) => token(a, mint, owner, program),
    }
}
fn created_rent(
    before: Option<&Account>,
    after: Option<&Account>,
    mint: &str,
    owner: &str,
    program: &str,
    creating: bool,
) -> Result<u64> {
    if !creating {
        return Ok(0);
    }
    if let Some(a) = before {
        if !empty_system(a) {
            token(a, mint, owner, program)?;
            return Ok(0);
        }
    }
    let after = after.ok_or_else(bad)?;
    token(after, mint, owner, program)?;
    let reserve = if mint == xone_domain::pump_amm::WSOL {
        m::u64_at(&after.data, 113)
    } else {
        after.lamports
    };
    Ok(reserve.saturating_sub(before.map_or(0, |a| a.lamports)))
}

/// Validate a signed RPC simulation's account deltas against the immutable
/// preparation, including native settlement and Token2022 withheld balances.
pub fn verify_simulated_settlement(
    context: &Context,
    prepared: &Value,
    before: &[Value],
    after: &[Value],
    fee: u64,
) -> Result<(u64, u64)> {
    if before.len() != 3 || after.len() != 3 {
        return Err(bad());
    }
    let source_native = prepared["sourceNativeSol"] == true;
    let destination_native = prepared["destinationNativeSol"] == true;
    if source_native != (context.sell_token == xone_domain::pump::NATIVE_SOL)
        || destination_native != (context.buy_token == xone_domain::pump::NATIVE_SOL)
        || (source_native
            && (prepared["sourceTokenAccount"] != context.caller
                || before[0] != before[2]
                || after[0] != after[2]
                || prepared["sourceTokenProgram"] != SYSTEM))
        || (destination_native
            && (prepared["destinationTokenAccount"] != context.caller
                || context.recipient != context.caller
                || before[1] != before[2]
                || after[1] != after[2]
                || prepared["destinationTokenProgram"] != SYSTEM))
    {
        return Err(err("PREPARATION_SIMULATION_AMOUNT_MISMATCH"));
    }
    let source_program = prepared["sourceTokenProgram"].as_str().ok_or_else(bad)?;
    let destination_program = prepared["destinationTokenProgram"]
        .as_str()
        .ok_or_else(bad)?;
    let before_source = account(&before[0])?;
    let before_destination = account(&before[1])?;
    let after_source = account(&after[0])?;
    let after_destination = account(&after[1])?;
    let payer_before = account(&before[2])?.ok_or_else(bad)?;
    let payer_after = account(&after[2])?.ok_or_else(bad)?;
    if payer_before.owner != SYSTEM
        || payer_after.owner != SYSTEM
        || payer_before.executable
        || payer_after.executable
        || !payer_before.data.is_empty()
        || !payer_after.data.is_empty()
    {
        return Err(bad());
    }
    let number = |key: &str| {
        prepared[key]
            .as_str()
            .ok_or_else(bad)?
            .parse::<u64>()
            .map_err(|_| bad())
    };
    if number("feeLamports")? != fee {
        return Err(err("PREPARATION_FEE_CHANGED"));
    }
    let persistent = number("rentLamports")?
        .checked_sub(number("rentRefundLamports")?)
        .ok_or_else(bad)?;
    let actual_rent = created_rent(
        before_source.as_ref(),
        after_source.as_ref(),
        &context.sell_token,
        &context.caller,
        source_program,
        prepared["sourceAccountCreation"] == true && prepared["refundWrappedSource"] != true,
    )?
    .checked_add(created_rent(
        before_destination.as_ref(),
        after_destination.as_ref(),
        &context.buy_token,
        &context.recipient,
        destination_program,
        prepared["accountCreation"] == true,
    )?)
    .ok_or_else(bad)?;
    if actual_rent != persistent {
        return Err(err("PREPARATION_RENT_CHANGED"));
    }
    let native_final = payer_after
        .lamports
        .checked_add(fee)
        .and_then(|v| v.checked_add(persistent))
        .ok_or_else(bad)?;
    let paid = if source_native {
        payer_before
            .lamports
            .checked_sub(native_final)
            .ok_or_else(bad)?
    } else if prepared["refundWrappedSource"] == true {
        if !before[0].is_null() || !after[0].is_null() {
            return Err(err("PREPARATION_TEMP_ACCOUNT_OCCUPIED"));
        }
        payer_before
            .lamports
            .checked_sub(native_final)
            .ok_or_else(bad)?
    } else {
        let source_after = after_source.as_ref().ok_or_else(bad)?;
        let initial = before_token_balance(
            before_source.as_ref(),
            source_after,
            &context.sell_token,
            &context.caller,
            source_program,
            prepared["sourceAccountCreation"] == true,
        )?;
        let end = token(
            source_after,
            &context.sell_token,
            &context.caller,
            source_program,
        )?;
        initial
            .checked_add(number("wrapLamports")?)
            .and_then(|v| v.checked_sub(end))
            .ok_or_else(bad)?
    };
    let received = if destination_native {
        native_final
            .checked_sub(payer_before.lamports)
            .ok_or_else(bad)?
    } else if prepared["unwrapDestinationSol"] == true {
        if !before[1].is_null() || !after[1].is_null() {
            return Err(err("PREPARATION_TEMP_ACCOUNT_OCCUPIED"));
        }
        native_final
            .checked_sub(payer_before.lamports)
            .ok_or_else(bad)?
    } else {
        let destination_after = after_destination.as_ref().ok_or_else(bad)?;
        let initial = before_token_balance(
            before_destination.as_ref(),
            destination_after,
            &context.buy_token,
            &context.recipient,
            destination_program,
            prepared["accountCreation"] == true,
        )?;
        token(
            destination_after,
            &context.buy_token,
            &context.recipient,
            destination_program,
        )?
        .checked_sub(initial)
        .ok_or_else(bad)?
    };
    if paid != number("quotedInput")? || received != number("quotedOutput")? {
        return Err(err("PREPARATION_SIMULATION_AMOUNT_MISMATCH"));
    }
    if !source_native
        && source_program != TOKEN
        && withheld(&account(&after[0])?.ok_or_else(bad)?)?
            != before_source
                .as_ref()
                .map(withheld)
                .transpose()?
                .unwrap_or(0)
    {
        return Err(err("PREPARATION_SIMULATION_FEE_MISMATCH"));
    }
    if !destination_native
        && destination_program != TOKEN
        && withheld(&account(&after[1])?.ok_or_else(bad)?)?
            != before_destination
                .as_ref()
                .map(withheld)
                .transpose()?
                .unwrap_or(0)
                .checked_add(number("expectedDestinationTransferFee")?)
                .ok_or_else(bad)?
    {
        return Err(err("PREPARATION_SIMULATION_FEE_MISMATCH"));
    }
    let expected_native = prepared["expectedPayerLamportDelta"]
        .as_str()
        .ok_or_else(bad)?
        .parse::<i128>()
        .map_err(|_| bad())?;
    if i128::from(payer_after.lamports) - i128::from(payer_before.lamports) != expected_native {
        return Err(err("PREPARATION_SIMULATION_AMOUNT_MISMATCH"));
    }
    Ok((paid, received))
}
