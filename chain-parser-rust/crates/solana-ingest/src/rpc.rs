use crate::source_wal::Recorder;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use xone_config::Chain;
use xone_domain::{err, hash, scope_hash, Amount, BlockRef, Result, Snapshot, SnapshotRef};
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const CPMM_PROGRAM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
#[derive(Clone)]
pub struct Rpc {
    pub http: reqwest::Client,
    pub max_bytes: usize,
}
impl Rpc {
    pub async fn call(&self, url: &str, method: &str, params: Value) -> Result<Value> {
        self.call_recorded(url, method, params, None).await
    }
    async fn call_recorded(
        &self,
        url: &str,
        method: &str,
        params: Value,
        recorder: Option<&Recorder>,
    ) -> Result<Value> {
        self.call_capped(
            url,
            method,
            params,
            recorder,
            self.max_bytes,
            "RPC_RESPONSE_LIMIT",
        )
        .await
        .map(|(value, _)| value)
    }
    async fn call_capped(
        &self,
        url: &str,
        method: &str,
        params: Value,
        recorder: Option<&Recorder>,
        limit: usize,
        limit_error: &'static str,
    ) -> Result<(Value, usize)> {
        if limit == 0 {
            return Err(err(limit_error));
        }
        let mut response = self
            .http
            .post(url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| err("RPC_NETWORK"))?;
        if !response.status().is_success() {
            return Err(err("RPC_HTTP_STATUS"));
        }
        let mut b = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| err("RPC_NETWORK"))? {
            if b.len()
                .checked_add(chunk.len())
                .is_none_or(|size| size > limit)
            {
                return Err(err(limit_error));
            }
            b.extend(chunk);
        }
        if let Some(recorder) = recorder {
            recorder.capture_async(url, method, &params, &b).await?;
        }
        let mut v: Value = serde_json::from_slice(&b).map_err(|_| err("RPC_INVALID_JSON"))?;
        if v["jsonrpc"] != "2.0"
            || v["id"] != 1
            || v.get("error").is_some()
            || v["result"].is_null()
        {
            return Err(err("RPC_INVALID_RESULT"));
        }
        Ok((v["result"].take(), b.len()))
    }
    pub async fn current_baseline(&self, ch: &Chain) -> Result<Snapshot> {
        self.current_baseline_recorded(ch, None).await
    }
    pub async fn current_baseline_recorded(
        &self,
        ch: &Chain,
        recorder: Option<&Recorder>,
    ) -> Result<Snapshot> {
        if ch.pools.iter().any(|p| {
            matches!(
                p.protocol.as_str(),
                "raydium-clmm-static"
                    | "raydium-clmm-token-fee"
                    | "raydium-clmm-bitmap"
                    | "raydium-clmm-custom-seed"
                    | "raydium-clmm-rewards"
                    | "raydium-clmm-dynamic"
            )
        }) && ch.confirmation_tag != "finalized"
        {
            return Err(err("RAYCLMM_REQUIRES_FINALIZED"));
        }
        if ch
            .pools
            .iter()
            .any(|p| p.protocol == "raydium-cpmm-creator")
            && ch.confirmation_tag != "finalized"
        {
            return Err(err("RAYDIUM_REQUIRES_FINALIZED"));
        }
        if ch.pools.iter().any(|p| {
            matches!(
                p.protocol.as_str(),
                "orca-whirlpools-static"
                    | "orca-whirlpools-rewards"
                    | "orca-whirlpools-adaptive"
                    | "orca-whirlpools-adaptive-rewards"
                    | "orca-whirlpools-adaptive-dynamic"
            )
        }) && ch.confirmation_tag != "finalized"
        {
            return Err(err("ORCA_REQUIRES_FINALIZED"));
        }
        let mut last = err("NO_HEALTHY_PROVIDER");
        for url in &ch.rpc_urls {
            match self.baseline_at(url, ch, recorder).await {
                Ok(s) => return Ok(s),
                Err(e) if e.0.starts_with("SOURCE_WAL_") => return Err(e),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
    async fn block_at(
        &self,
        url: &str,
        ch: &Chain,
        slot: u64,
        recorder: Option<&Recorder>,
    ) -> Result<Value> {
        self.call_recorded(
            url,
            "getBlock",
            json!([slot,{
                "commitment":ch.confirmation_tag,"transactionDetails":"none","rewards":false,
                "maxSupportedTransactionVersion":0
            }]),
            recorder,
        )
        .await
    }
    /// A provider-declared current bootstrap, not an independent account-bank
    /// proof or historical snapshot. No context chasing or cross-provider merge.
    async fn sharded_accounts(
        &self,
        url: &str,
        ch: &Chain,
        required: &BTreeSet<String>,
        recorder: Option<&Recorder>,
    ) -> Result<(u64, BTreeMap<String, Value>, Option<Value>)> {
        if ch.confirmation_tag != "finalized" {
            return Err(err("ACCOUNT_SHARDS_REQUIRES_FINALIZED"));
        }
        let clock = crate::raydium::CLOCK;
        let ids = required
            .iter()
            .filter(|id| id.as_str() != clock)
            .cloned()
            .collect::<Vec<_>>();
        // 32 account requests plus genesis and two anchors: at most 35 calls.
        if ids.len() > 32 * 99 {
            return Err(err("ACCOUNT_SHARD_COUNT_LIMIT"));
        }
        let mut remaining = self.max_bytes;
        let mut slot = None;
        let mut guard = None;
        let mut anchor = None;
        let mut accounts = BTreeMap::new();
        for part in ids.chunks(99) {
            let mut requested = part.to_vec();
            requested.push(clock.into());
            let (result, bytes) = self
                .call_capped(
                    url,
                    "getMultipleAccounts",
                    json!([requested,{"encoding":"base64","commitment":"finalized"}]),
                    recorder,
                    remaining,
                    "ACCOUNT_SHARD_RESPONSE_BUDGET",
                )
                .await?;
            remaining = remaining
                .checked_sub(bytes)
                .ok_or_else(|| err("ACCOUNT_SHARD_RESPONSE_BUDGET"))?;
            let current = result["context"]["slot"]
                .as_u64()
                .ok_or_else(|| err("ACCOUNT_CONTEXT_MISSING"))?;
            if slot.is_some_and(|slot| slot != current) {
                return Err(err("ACCOUNT_SHARD_CONTEXT_MISMATCH"));
            }
            let mut values = result["value"]
                .as_array()
                .ok_or_else(|| err("ACCOUNT_DATA_MISSING"))?
                .clone();
            if values.len() != requested.len() {
                return Err(err("ACCOUNT_BATCH_INCOMPLETE"));
            }
            let clock_value = values
                .pop()
                .ok_or_else(|| err("ACCOUNT_SHARD_CLOCK_INVALID"))?;
            validate_shard_clock(&clock_value, current)?;
            if guard
                .as_ref()
                .is_some_and(|previous| previous != &clock_value)
            {
                return Err(err("ACCOUNT_SHARD_CLOCK_CHANGED"));
            }
            if slot.is_none() {
                let block = self.block_at(url, ch, current, recorder).await?;
                validate_bank_anchor(&block, current)?;
                anchor = Some(block);
                slot = Some(current);
                guard = Some(clock_value.clone());
                accounts.insert(clock.into(), clock_value);
            }
            for (id, value) in part.iter().cloned().zip(values) {
                if accounts.insert(id, value).is_some() {
                    return Err(err("ACCOUNT_SHARD_COVERAGE"));
                }
            }
        }
        if accounts.len() != required.len() + usize::from(!required.contains(clock))
            || !required.iter().all(|id| accounts.contains_key(id))
        {
            return Err(err("ACCOUNT_SHARD_COVERAGE"));
        }
        Ok((
            slot.ok_or_else(|| err("EMPTY_POOL_SCOPE"))?,
            accounts,
            anchor,
        ))
    }
    async fn baseline_at(
        &self,
        url: &str,
        ch: &Chain,
        recorder: Option<&Recorder>,
    ) -> Result<Snapshot> {
        let genesis = self
            .call_recorded(url, "getGenesisHash", json!([]), recorder)
            .await?;
        if genesis.as_str() != Some(&ch.genesis_hash) {
            return Err(err("RPC_WRONG_GENESIS"));
        }
        if ch.pools.is_empty() {
            return Err(err("EMPTY_POOL_SCOPE"));
        }
        let mut ids = BTreeSet::new();
        let mut creator_ids = BTreeSet::new();
        let mut clmm_ids = BTreeSet::new();
        let mut archived_ids = BTreeSet::new();
        let mut orca_ids = BTreeSet::new();
        let mut orca_oracles = BTreeSet::new();
        let mut adaptive_oracles = BTreeSet::new();
        for p in &ch.pools {
            match p.protocol.as_str() {
                "raydium-clmm-static"
                | "raydium-clmm-token-fee"
                | "raydium-clmm-bitmap"
                | "raydium-clmm-custom-seed"
                | "raydium-clmm-rewards"
                | "raydium-clmm-dynamic" => {
                    clmm_ids.extend(crate::raydium_clmm::required_accounts(p)?);
                }
                "orca-whirlpools-static"
                | "orca-whirlpools-rewards"
                | "orca-whirlpools-adaptive"
                | "orca-whirlpools-adaptive-rewards"
                | "orca-whirlpools-adaptive-dynamic" => {
                    if matches!(
                        p.protocol.as_str(),
                        "orca-whirlpools-adaptive"
                            | "orca-whirlpools-adaptive-rewards"
                            | "orca-whirlpools-adaptive-dynamic"
                    ) {
                        adaptive_oracles.insert(crate::orca::oracle_address(p)?);
                    }
                    orca_ids.extend(crate::orca::required_accounts(p)?);
                    orca_oracles.insert(crate::orca::oracle_address(p)?);
                }
                "raydium-cpmm-creator" => {
                    creator_ids.extend(crate::raydium::required_accounts(p)?);
                }
                "raydium-cpmm"
                    if p.program_id == CPMM_PROGRAM
                        && p.accounts.len() == 6
                        && p.accounts[0] == p.id =>
                {
                    archived_ids.extend(p.accounts.iter().cloned());
                }
                _ => return Err(err("PROTOCOL_LAYOUT_UNSUPPORTED")),
            }
        }
        ids.extend(creator_ids.iter().cloned());
        ids.extend(clmm_ids.iter().cloned());
        ids.extend(archived_ids.iter().cloned());
        ids.extend(orca_ids.iter().cloned());
        let sharded = ids.len() > 100;
        let (slot, values, initial_anchor) = if sharded {
            self.sharded_accounts(url, ch, &ids, recorder).await?
        } else {
            let ids = ids.into_iter().collect::<Vec<_>>();
            let result = self
                .call_recorded(
                    url,
                    "getMultipleAccounts",
                    json!([ids,{"encoding":"base64","commitment":ch.confirmation_tag}]),
                    recorder,
                )
                .await?;
            let slot = result["context"]["slot"]
                .as_u64()
                .ok_or_else(|| err("ACCOUNT_CONTEXT_MISSING"))?;
            let values = result["value"]
                .as_array()
                .ok_or_else(|| err("ACCOUNT_DATA_MISSING"))?;
            if values.len() != ids.len() {
                return Err(err("ACCOUNT_BATCH_INCOMPLETE"));
            }
            (
                slot,
                ids.into_iter()
                    .zip(values.iter().cloned())
                    .collect::<BTreeMap<_, _>>(),
                None,
            )
        };
        // Executable proof accounts belong to the pinned decoders. The archive
        // decoder retains its strict non-executable Account contract.
        let accounts = archived_ids
            .iter()
            .map(|id| Ok((id.clone(), Account::decode(&values[id])?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let clmm_accounts = clmm_ids
            .iter()
            .map(|id| Ok((id.clone(), crate::raydium::RpcAccount::decode(&values[id])?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let creator_accounts = creator_ids
            .iter()
            .map(|id| Ok((id.clone(), crate::raydium::RpcAccount::decode(&values[id])?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let orca_accounts = orca_ids
            .iter()
            .map(|id| {
                let oracle = if orca_oracles.contains(id) {
                    id.as_str()
                } else {
                    ""
                };
                Ok((
                    id.clone(),
                    if adaptive_oracles.contains(id) {
                        crate::orca::decode_adaptive_rpc_account(&values[id])?
                    } else {
                        crate::orca::decode_rpc_account(id, oracle, &values[id])?
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let pinned_bank =
            sharded || !creator_ids.is_empty() || !orca_ids.is_empty() || !clmm_ids.is_empty();
        // minContextSlot is intentionally not used as an exact historical account selector.
        let block = match initial_anchor {
            Some(block) => block,
            None => self.block_at(url, ch, slot, recorder).await?,
        };
        let timestamp = block["blockTime"]
            .as_u64()
            .ok_or_else(|| err("BLOCK_TIME_MISSING"))?
            .checked_mul(1000)
            .ok_or_else(|| err("TIMESTAMP_OVERFLOW"))?;
        let bank = block["blockhash"]
            .as_str()
            .ok_or_else(|| err("BLOCK_HASH_MISSING"))?;
        let parent = block["previousBlockhash"]
            .as_str()
            .ok_or_else(|| err("BLOCK_PARENT_MISSING"))?;
        if pinned_bank {
            validate_bank_anchor(&block, slot)?;
        }
        let mut pools = Vec::new();
        for p in &ch.pools {
            pools.push(
                if matches!(
                    p.protocol.as_str(),
                    "raydium-clmm-static"
                        | "raydium-clmm-token-fee"
                        | "raydium-clmm-bitmap"
                        | "raydium-clmm-custom-seed"
                        | "raydium-clmm-rewards"
                        | "raydium-clmm-dynamic"
                ) {
                    crate::raydium_clmm::decode_raydium_clmm(ch, p, &clmm_accounts, slot)?
                } else if p.protocol == "orca-whirlpools-static"
                    || p.protocol == "orca-whirlpools-rewards"
                    || p.protocol == "orca-whirlpools-adaptive"
                    || p.protocol == "orca-whirlpools-adaptive-rewards"
                    || p.protocol == "orca-whirlpools-adaptive-dynamic"
                {
                    crate::orca::decode_orca_whirlpool(ch, p, &orca_accounts, slot)?
                } else if p.protocol == "raydium-cpmm-creator" {
                    crate::raydium::decode_raydium_cpmm(ch, p, &creator_accounts, slot)?
                } else {
                    decode_cpmm_at(ch, p, &accounts, timestamp / 1000)?
                },
            );
        }
        if pinned_bank {
            let after = self
                .call_recorded(
                    url,
                    "getBlock",
                    json!([slot,{
                        "commitment":"finalized","transactionDetails":"none","rewards":false,
                        "maxSupportedTransactionVersion":0
                    }]),
                    recorder,
                )
                .await?;
            if after["blockhash"] != block["blockhash"]
                || after["previousBlockhash"] != block["previousBlockhash"]
                || after["parentSlot"] != block["parentSlot"]
                || after["blockTime"] != block["blockTime"]
            {
                return Err(err("ACCOUNT_BANK_ANCHOR_CHANGED"));
            }
        }
        let adapters = pools
            .iter()
            .map(|p| p.adapter_version.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let scope = scope_hash(&pools)?;
        Ok(Snapshot {
            reference: SnapshotRef {
                block: BlockRef {
                    chain_key: ch.key.clone(),
                    number_or_slot: slot,
                    hash: bank.into(),
                    parent_hash: parent.into(),
                    timestamp_ms: timestamp,
                },
                canonical_epoch: 1,
                journal_seq: 1,
                schema_version: 1,
                adapter_set_hash: hash(&adapters)?,
                online_generation: 1,
                scope_hash: scope,
                writer_epoch: 1,
                owner_service: ch.owner_service.clone(),
                owner_assignment_version: ch.assignment_version,
            },
            pools,
            source: if sharded {
                "RPC_SAME_FINALIZED_CONTEXT_SHARDS"
            } else {
                "RPC_CONTEXT_SLOT_ACCOUNTS"
            }
            .into(),
            history_gaps: Vec::new(),
        })
    }
}
fn validate_bank_anchor(block: &Value, slot: u64) -> Result<()> {
    block["blockTime"]
        .as_u64()
        .ok_or_else(|| err("BLOCK_TIME_MISSING"))?
        .checked_mul(1000)
        .ok_or_else(|| err("TIMESTAMP_OVERFLOW"))?;
    let hash = block["blockhash"]
        .as_str()
        .ok_or_else(|| err("BLOCK_HASH_MISSING"))?;
    let parent = block["previousBlockhash"]
        .as_str()
        .ok_or_else(|| err("BLOCK_PARENT_MISSING"))?;
    if block["parentSlot"]
        .as_u64()
        .is_none_or(|parent| parent >= slot)
    {
        return Err(err("BLOCK_PARENT_SLOT_INVALID"));
    }
    xone_domain::raydium::validate_pubkey(hash)?;
    xone_domain::raydium::validate_pubkey(parent)?;
    if hash == parent {
        return Err(err("BLOCK_PARENT_HASH_INVALID"));
    }
    Ok(())
}
fn validate_shard_clock(value: &Value, slot: u64) -> Result<()> {
    let clock = crate::raydium::RpcAccount::decode(value)
        .map_err(|_| err("ACCOUNT_SHARD_CLOCK_INVALID"))?;
    if clock.owner != "Sysvar1111111111111111111111111111111111111"
        || clock.executable
        || clock.data.len() != 40
        || u64_at(&clock.data, 0)? != slot
    {
        return Err(err("ACCOUNT_SHARD_CLOCK_INVALID"));
    }
    Ok(())
}
#[derive(Clone)]
pub struct Account {
    pub owner: String,
    pub data: Vec<u8>,
}
impl Account {
    fn decode(v: &Value) -> Result<Self> {
        let owner = v["owner"]
            .as_str()
            .ok_or_else(|| err("ACCOUNT_MISSING"))?
            .to_owned();
        if v["executable"] != false {
            return Err(err("ACCOUNT_EXECUTABLE_UNEXPECTED"));
        }
        let encoded = v["data"][0]
            .as_str()
            .ok_or_else(|| err("ACCOUNT_ENCODING"))?;
        if v["data"][1] != "base64" {
            return Err(err("ACCOUNT_ENCODING"));
        }
        let data = STANDARD
            .decode(encoded)
            .map_err(|_| err("ACCOUNT_ENCODING"))?;
        Ok(Self { owner, data })
    }
}
fn u64_at(data: &[u8], offset: usize) -> Result<u64> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| err("ACCOUNT_LAYOUT"))?;
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| err("ACCOUNT_LAYOUT"))?,
    ))
}
fn key_at(data: &[u8], offset: usize) -> Result<String> {
    Ok(bs58::encode(
        data.get(offset..offset + 32)
            .ok_or_else(|| err("ACCOUNT_LAYOUT"))?,
    )
    .into_string())
}
/// Decode archived data without a bank timestamp; only pools with zero open time qualify.
pub fn decode_cpmm(
    ch: &Chain,
    p: &xone_config::PoolConfig,
    accounts: &BTreeMap<String, Account>,
) -> Result<xone_domain::Pool> {
    decode_cpmm_at(ch, p, accounts, 0)
}

/// Decode the archived layout with its opening time checked against the account bank.
pub fn decode_cpmm_at(
    ch: &Chain,
    p: &xone_config::PoolConfig,
    accounts: &BTreeMap<String, Account>,
    bank_timestamp_seconds: u64,
) -> Result<xone_domain::Pool> {
    if p.accounts.len() != 6 || p.program_id != CPMM_PROGRAM {
        return Err(err("PROTOCOL_LAYOUT_UNSUPPORTED"));
    }
    let get = |i: usize| {
        accounts
            .get(&p.accounts[i])
            .ok_or_else(|| err("ACCOUNT_DEPENDENCY_MISSING"))
    };
    let state = get(0)?;
    let v0 = get(1)?;
    let v1 = get(2)?;
    let cfg = get(3)?;
    let m0 = get(4)?;
    let m1 = get(5)?;
    if state.owner != CPMM_PROGRAM
        || cfg.owner != CPMM_PROGRAM
        || state.data.len() != 637
        || state.data[..8] != [247, 237, 227, 245, 215, 195, 222, 70]
        || cfg.data.len() != 236
        || cfg.data[..8] != [218, 244, 33, 104, 203, 203, 43, 111]
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    // Current programs repurpose archive padding for creator modes, rates, and balances.
    // Matching lengths/discriminators alone cannot establish the archived semantics.
    if state.data[389..].iter().any(|byte| *byte != 0)
        || cfg.data[108..].iter().any(|byte| *byte != 0)
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    if key_at(&state.data, 8)? != p.accounts[3]
        || key_at(&state.data, 72)? != p.accounts[1]
        || key_at(&state.data, 104)? != p.accounts[2]
        || key_at(&state.data, 168)? != p.token0
        || key_at(&state.data, 200)? != p.token1
        || p.accounts[4] != p.token0
        || p.accounts[5] != p.token1
    {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    if state.data[329] & 4 != 0 {
        return Err(err("POOL_SWAP_DISABLED"));
    }
    if u64_at(&state.data, 373)? > bank_timestamp_seconds {
        return Err(err("POOL_NOT_OPEN"));
    }
    for (vault, mint, expected, offset) in [(v0, m0, &p.token0, 232), (v1, m1, &p.token1, 264)] {
        if vault.owner != TOKEN_PROGRAM
            || mint.owner != TOKEN_PROGRAM
            || key_at(&state.data, offset)? != TOKEN_PROGRAM
            || vault.data.len() != 165
            || mint.data.len() != 82
            || vault.data[108] != 1
            || mint.data[45] != 1
            || key_at(&vault.data, 0)? != *expected
        {
            return Err(err("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED"));
        }
    }
    let fee = u64_at(&cfg.data, 12)?;
    if fee >= 1_000_000 {
        return Err(err("INVALID_FEE"));
    }
    let r0 = u64_at(&v0.data, 64)?
        .checked_sub(
            u64_at(&state.data, 341)?
                .checked_add(u64_at(&state.data, 357)?)
                .ok_or_else(|| err("FEE_OVERFLOW"))?,
        )
        .ok_or_else(|| err("RESERVE_UNDERFLOW"))?;
    let r1 = u64_at(&v1.data, 64)?
        .checked_sub(
            u64_at(&state.data, 349)?
                .checked_add(u64_at(&state.data, 365)?)
                .ok_or_else(|| err("FEE_OVERFLOW"))?,
        )
        .ok_or_else(|| err("RESERVE_UNDERFLOW"))?;
    Ok(xone_domain::Pool {
        pancake_bsc_lm: None,
        pancake_lm: None,
        slipstream_context: None,
        lfj: None,
        infinity_bin: None,
        orca_whirlpool: None,
        raydium_clmm: None,
        v2: None,
        v3: None,
        stable: None,
        weighted: None,
        multi_asset: None,
        singleton_advanced: None,
        singleton_cl: None,
        raydium_cpmm: None,
        chain_key: ch.key.clone(),
        pool_id: p.id.clone(),
        protocol: "raydium-cpmm".into(),
        token0: p.token0.clone(),
        token1: p.token1.clone(),
        reserve0: Amount::checked(r0.into())?,
        reserve1: Amount::checked(r1.into())?,
        fee_numerator: fee,
        fee_denominator: 1_000_000,
        adapter_version: "raydium-cpmm/archive-v1".into(),
        token_behavior: "standard".into(),
    })
}
pub fn skipped_slots(
    from: u64,
    to: u64,
    first_available: u64,
    present: &[u64],
) -> Result<Vec<u64>> {
    if from > to || to - from > 10000 || from < first_available {
        return Err(err("SLOT_RANGE_NOT_PROVABLE"));
    }
    let mut seen = BTreeSet::new();
    for s in present {
        if *s < from || *s > to || !seen.insert(*s) {
            return Err(err("INVALID_SLOT_RANGE_RESPONSE"));
        }
    }
    Ok((from..=to).filter(|s| !seen.contains(s)).collect())
}

/// Exact dependency closure for the currently admitted catalog, including
/// executable/programdata, token, clock, oracle and tick-array dependencies.
pub fn required_scope(ch: &Chain) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    for p in &ch.pools {
        match p.protocol.as_str() {
            "raydium-clmm-static"
            | "raydium-clmm-token-fee"
            | "raydium-clmm-bitmap"
            | "raydium-clmm-custom-seed"
            | "raydium-clmm-rewards"
            | "raydium-clmm-dynamic" => ids.extend(crate::raydium_clmm::required_accounts(p)?),
            "orca-whirlpools-static"
            | "orca-whirlpools-rewards"
            | "orca-whirlpools-adaptive"
            | "orca-whirlpools-adaptive-rewards"
            | "orca-whirlpools-adaptive-dynamic" => ids.extend(crate::orca::required_accounts(p)?),
            "raydium-cpmm-creator" => ids.extend(crate::raydium::required_accounts(p)?),
            "raydium-cpmm"
                if p.program_id == CPMM_PROGRAM
                    && p.accounts.len() == 6
                    && p.accounts[0] == p.id =>
            {
                ids.extend(p.accounts.iter().cloned())
            }
            _ => return Err(err("PROTOCOL_LAYOUT_UNSUPPORTED")),
        }
    }
    Ok(ids)
}
/// Decode only independently verified, complete bank accounts. No RPC context
/// conversion occurs at this boundary; all ordinary protocol pin checks still run.
pub fn snapshot_from_bank(
    ch: &Chain,
    bank: &crate::stream::CommittedBank,
    timestamp_ms: u64,
) -> Result<Snapshot> {
    let required = required_scope(ch)?;
    let values=bank.accounts.iter().map(|(id,a)| (bs58::encode(id).into_string(),json!({"owner":bs58::encode(a.owner).into_string(),"lamports":a.lamports,"executable":a.executable,"rentEpoch":a.rent_epoch,"data":[STANDARD.encode(&a.data),"base64"]}))).collect::<BTreeMap<_,_>>();
    if !required.iter().eq(values.keys()) {
        return Err(err("BANK_SCOPE_INCOMPLETE"));
    }
    let mut pools = Vec::new();
    for p in &ch.pools {
        let pool = match p.protocol.as_str() {
            "raydium-clmm-static"
            | "raydium-clmm-token-fee"
            | "raydium-clmm-bitmap"
            | "raydium-clmm-custom-seed"
            | "raydium-clmm-rewards"
            | "raydium-clmm-dynamic" => {
                let a = crate::raydium_clmm::required_accounts(p)?
                    .into_iter()
                    .map(|k| {
                        let decoded = crate::raydium::RpcAccount::decode(&values[&k])?;
                        Ok((k, decoded))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                crate::raydium_clmm::decode_raydium_clmm(ch, p, &a, bank.bank.slot)?
            }
            "raydium-cpmm-creator" => {
                let a = crate::raydium::required_accounts(p)?
                    .into_iter()
                    .map(|k| {
                        let decoded = crate::raydium::RpcAccount::decode(&values[&k])?;
                        Ok((k, decoded))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                crate::raydium::decode_raydium_cpmm(ch, p, &a, bank.bank.slot)?
            }
            "orca-whirlpools-static"
            | "orca-whirlpools-rewards"
            | "orca-whirlpools-adaptive"
            | "orca-whirlpools-adaptive-rewards"
            | "orca-whirlpools-adaptive-dynamic" => {
                let oracle = crate::orca::oracle_address(p)?;
                let adaptive = p.protocol.contains("adaptive");
                let a = crate::orca::required_accounts(p)?
                    .into_iter()
                    .map(|k| {
                        let v = &values[&k];
                        Ok((
                            k.clone(),
                            if adaptive && k == oracle {
                                crate::orca::decode_adaptive_rpc_account(v)?
                            } else {
                                crate::orca::decode_rpc_account(
                                    &k,
                                    if k == oracle { &oracle } else { "" },
                                    v,
                                )?
                            },
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                crate::orca::decode_orca_whirlpool(ch, p, &a, bank.bank.slot)?
            }
            "raydium-cpmm" => {
                let a = p
                    .accounts
                    .iter()
                    .map(|k| Ok((k.clone(), Account::decode(&values[k])?)))
                    .collect::<Result<BTreeMap<_, _>>>()?;
                decode_cpmm_at(ch, p, &a, timestamp_ms / 1000)?
            }
            _ => return Err(err("PROTOCOL_LAYOUT_UNSUPPORTED")),
        };
        pools.push(pool);
    }
    let adapters = pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    Ok(Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: ch.key.clone(),
                number_or_slot: bank.bank.slot,
                hash: bank.blockhash.clone(),
                parent_hash: bank.parent.blockhash.clone(),
                timestamp_ms,
            },
            canonical_epoch: 1,
            journal_seq: 1,
            schema_version: 1,
            adapter_set_hash: hash(&adapters)?,
            online_generation: 1,
            scope_hash: scope_hash(&pools)?,
            writer_epoch: 1,
            owner_service: ch.owner_service.clone(),
            owner_assignment_version: ch.assignment_version,
        },
        pools,
        source: "YELLOWSTONE_INDEPENDENT_VERSIONED_BANK_V1".into(),
        history_gaps: vec![],
    })
}
