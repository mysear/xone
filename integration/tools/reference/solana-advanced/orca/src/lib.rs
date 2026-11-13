#![recursion_limit = "512"]
//! Independent native reference. No XONE domain, parser, router or production math imports.
//! All prices, swaps, crossings and pool fee updates execute the unchanged pinned program.
use ::whirlpool as upstream;
use anchor_lang::{prelude::*, AccountSerialize, Discriminator};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, str::FromStr};
use upstream::{manager::swap_manager, math, state::*, util::SparseSwapTickSequenceBuilder};

pub const REVISION: &str = "cc683ec92acb5e7606caa3d8e514d9383e9c7c11";
pub const PROGRAM_CODE: &str =
    "XONE synthetic Orca Whirlpool proof fixture; NOT deployed program bytecode; cc683ec9";
pub const TOKEN_CODE: &str =
    "XONE synthetic classic SPL Token proof fixture; NOT deployed token bytecode";
pub const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn pk(v: &Value) -> std::result::Result<Pubkey, String> {
    Pubkey::from_str(v.as_str().ok_or("missing public key")?).map_err(|e| e.to_string())
}
fn key(label: &str) -> Pubkey {
    // Stable synthetic on-curve keys. Vaults are arbitrary signer keys, never PDAs.
    for i in 0u32.. {
        let b: [u8; 32] = Sha256::digest(format!("xone-orca-native:{label}:{i}").as_bytes()).into();
        let k = Pubkey::new_from_array(b);
        if k.is_on_curve() {
            return k;
        }
    }
    unreachable!()
}
fn num(v: &Value) -> std::result::Result<u64, String> {
    v.as_u64().ok_or_else(|| "expected exact JSON u64".into())
}
fn wide(v: &Value) -> std::result::Result<u128, String> {
    let s = v.as_str().ok_or("expected decimal u128 string")?;
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid unsigned decimal".into());
    }
    s.parse::<u128>().map_err(|e| e.to_string())
}
fn signed(v: &Value) -> std::result::Result<i128, String> {
    v.as_str()
        .ok_or("expected decimal i128 string")?
        .parse::<i128>()
        .map_err(|e| e.to_string())
}
fn integer(v: &Value) -> std::result::Result<i32, String> {
    i32::try_from(v.as_i64().ok_or("expected i32")?).map_err(|e| e.to_string())
}
pub fn tick_array_address(pool: &Pubkey, start: i32) -> (Pubkey, u8) {
    // Exact seeds in original program sparse_swap.rs and official rust-sdk/client/src/pda/tick_array.rs.
    Pubkey::find_program_address(
        &[b"tick_array", pool.as_ref(), start.to_string().as_bytes()],
        &upstream::ID,
    )
}

pub fn base_pool() -> Value {
    let config = key("config");
    let mut mints = [key("mint-a"), key("mint-b")];
    mints.sort_by_key(|k| k.to_bytes());
    let spacing = 64u16;
    let (pool, bump) = Pubkey::find_program_address(
        &[
            b"whirlpool",
            config.as_ref(),
            mints[0].as_ref(),
            mints[1].as_ref(),
            &spacing.to_le_bytes(),
        ],
        &upstream::ID,
    );
    let oracle = Pubkey::find_program_address(&[b"oracle", pool.as_ref()], &upstream::ID).0;
    let programdata =
        Pubkey::find_program_address(&[upstream::ID.as_ref()], &Pubkey::from_str(LOADER).unwrap())
            .0;
    let arrays: Vec<Value> = [-5632, 0, 5632].into_iter().map(|start| {
        let ticks: Vec<Value> = (0..88).map(|i| start + i * 64).filter(|t| [-5632, -128, -64, 64, 128, 5632].contains(t)).map(|tick| json!({
            "tick": tick, "liquidity_gross": "20000000000", "liquidity_net": if tick < 0 { "10000000000" } else { "-10000000000" },
            "fee_growth_outside0": "37", "fee_growth_outside1": "53"
        })).collect();
        let bitmap = ticks.iter().fold(0u128, |b, t| b | (1u128 << ((t["tick"].as_i64().unwrap() as i32 - start) / 64)));
        json!({"address": tick_array_address(&pool, start).0.to_string(), "start_tick_index": start, "layout": "fixed", "initialized_bitmap": bitmap.to_string(), "ticks": ticks})
    }).collect();
    json!({
        "pool_id": pool.to_string(), "chain_key": "solana:localnet", "protocol": "orca-whirlpools-static", "adapter_version": "orca-whirlpools/cc683ec9-spl-static-1",
        "token0": mints[0].to_string(), "token1": mints[1].to_string(), "reserve0": "0", "reserve1": "0", "fee_numerator": 3000, "fee_denominator": 1000000, "token_behavior": "standard",
        "orca_whirlpool": {
            "program_id": upstream::ID.to_string(), "programdata": programdata.to_string(), "program_code_sha256": hash(PROGRAM_CODE.as_bytes()), "deployed_slot": 200,
            "token_program_code_sha256": hash(TOKEN_CODE.as_bytes()), "whirlpools_config": config.to_string(), "whirlpool_bump": bump, "oracle": oracle.to_string(),
            "vault0": key("vault-a").to_string(), "vault1": key("vault-b").to_string(), "raw_vault0": 1000000000000000000u64, "raw_vault1": 1100000000000000000u64,
            "mint0_decimals": 9, "mint1_decimals": 6, "bank_slot": 123456, "bank_unix_timestamp": 1793059200u64, "bank_epoch": 987,
            "fee_tier_index": spacing, "fee_rate": 3000, "protocol_fee_rate": 2500, "sqrt_price_x64": math::sqrt_price_from_tick_index(0).to_string(),
            "tick_current_index": 0, "liquidity": "1000000000000", "tick_spacing": spacing, "protocol_fee_owed0": 101, "protocol_fee_owed1": 103,
            "fee_growth_global0": "1009", "fee_growth_global1": "1013", "reward_last_updated_timestamp": 1793059190u64, "tick_arrays": arrays, "max_steps": 8192
        }
    })
}

pub fn whirlpool_state(pool: &Value) -> std::result::Result<Whirlpool, String> {
    let s = &pool["orca_whirlpool"];
    let spacing = u16::try_from(num(&s["tick_spacing"])?).map_err(|e| e.to_string())?;
    let fee = u16::try_from(num(&s["fee_rate"])?).map_err(|e| e.to_string())?;
    let protocol_fee = u16::try_from(num(&s["protocol_fee_rate"])?).map_err(|e| e.to_string())?;
    if spacing == 0 || fee > math::MAX_FEE_RATE || protocol_fee > math::MAX_PROTOCOL_FEE_RATE {
        return Err("static profile: fee/spacing exceeds original initialization bounds".into());
    }
    if num(&pool["fee_numerator"])? != fee as u64 || num(&pool["fee_denominator"])? != 1_000_000 {
        return Err("inconsistent pool fee".into());
    }
    if num(&s["fee_tier_index"])? != spacing as u64 {
        return Err("adaptive tier rejected".into());
    }
    let sqrt_price = wide(&s["sqrt_price_x64"])?;
    if !(math::MIN_SQRT_PRICE_X64..=math::MAX_SQRT_PRICE_X64).contains(&sqrt_price) {
        return Err("sqrt price out of bounds".into());
    }
    let tick = integer(&s["tick_current_index"])?;
    let price_tick = math::tick_index_from_sqrt_price(&sqrt_price);
    if tick != price_tick
        && !(tick == price_tick - 1 && sqrt_price == math::sqrt_price_from_tick_index(price_tick))
    {
        return Err("tick and price disagree".into());
    }
    if !(MIN_TICK_INDEX..=MAX_TICK_INDEX).contains(&tick) {
        return Err("tick out of bounds".into());
    }
    if num(&s["reward_last_updated_timestamp"])? > num(&s["bank_unix_timestamp"])? {
        return Err("reward timestamp after bank time".into());
    }
    if num(&s["max_steps"])? != 8192 {
        return Err("reference corpus uses the full 8192-step profile budget".into());
    }
    let mut rewards = [WhirlpoolRewardInfo::default(); NUM_REWARDS];
    rewards[0].extension = key("reward-authority").to_bytes();
    if let Some(infos) = s["reward_infos"].as_array() {
        for (out, r) in rewards.iter_mut().zip(infos) {
            out.mint = pk(&r["mint"])?; out.vault = pk(&r["vault"])?;
            out.extension = serde_json::from_value::<Vec<u8>>(r["extension"].clone()).map_err(|e| e.to_string())?.try_into().map_err(|_| "extension width")?;
            out.emissions_per_second_x64 = wide(&r["emissions_per_second_x64"])?;
            out.growth_global_x64 = wide(&r["growth_global_x64"])?;
        }
    }
    Ok(Whirlpool {
        whirlpools_config: pk(&s["whirlpools_config"])?,
        whirlpool_bump: [u8::try_from(num(&s["whirlpool_bump"])?).map_err(|e| e.to_string())?],
        tick_spacing: spacing,
        fee_tier_index_seed: spacing.to_le_bytes(),
        fee_rate: fee,
        protocol_fee_rate: protocol_fee,
        liquidity: wide(&s["liquidity"])?,
        sqrt_price,
        tick_current_index: tick,
        protocol_fee_owed_a: num(&s["protocol_fee_owed0"])?,
        protocol_fee_owed_b: num(&s["protocol_fee_owed1"])?,
        token_mint_a: pk(&pool["token0"])?,
        token_mint_b: pk(&pool["token1"])?,
        token_vault_a: pk(&s["vault0"])?,
        token_vault_b: pk(&s["vault1"])?,
        fee_growth_global_a: wide(&s["fee_growth_global0"])?,
        fee_growth_global_b: wide(&s["fee_growth_global1"])?,
        reward_last_updated_timestamp: num(&s["reward_last_updated_timestamp"])?,
        reward_infos: rewards,
    })
}

pub fn array_bytes(a: &Value, pool: &Pubkey, spacing: u16) -> std::result::Result<Vec<u8>, String> {
    let start = integer(&a["start_tick_index"])?;
    if !Tick::check_is_valid_start_tick(start, spacing) {
        return Err("invalid start tick".into());
    }
    if pk(&a["address"])? != tick_array_address(pool, start).0 {
        return Err("tick array PDA mismatch".into());
    }
    let bitmap = wide(&a["initialized_bitmap"])?;
    if bitmap >> 88 != 0 {
        return Err("bitmap exceeds 88 bits".into());
    }
    let ticks = a["ticks"].as_array().ok_or("missing ticks")?;
    let mut map = BTreeMap::new();
    for t in ticks {
        let tick = integer(&t["tick"])?;
        if tick < start
            || tick >= start + 88 * spacing as i32
            || tick % spacing as i32 != 0
            || !(MIN_TICK_INDEX..=MAX_TICK_INDEX).contains(&tick)
        {
            return Err("invalid initialized tick".into());
        }
        if map.insert((tick - start) / spacing as i32, t).is_some() {
            return Err("duplicate initialized tick".into());
        }
        let gross = wide(&t["liquidity_gross"])?;
        let net = signed(&t["liquidity_net"])?;
        if gross == 0 || net.unsigned_abs() > gross {
            return Err("invalid tick liquidity".into());
        }
    }
    let actual = map.keys().fold(0u128, |b, i| b | (1u128 << i));
    if bitmap != actual {
        return Err("bitmap and initialized ticks disagree".into());
    }
    let dynamic = match a["layout"].as_str() {
        Some("fixed") => false,
        Some("dynamic") => true,
        _ => return Err("unknown tick layout".into()),
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(if dynamic {
        DynamicTickArray::DISCRIMINATOR
    } else {
        FixedTickArray::DISCRIMINATOR
    });
    bytes.extend_from_slice(&start.to_le_bytes());
    if dynamic {
        bytes.extend_from_slice(pool.as_ref());
        bytes.extend_from_slice(&bitmap.to_le_bytes());
    }
    for i in 0..88 {
        bytes.push(if map.contains_key(&i) { 1 } else { 0 });
        if let Some(t) = map.get(&i) {
            bytes.extend_from_slice(&signed(&t["liquidity_net"])?.to_le_bytes());
            bytes.extend_from_slice(&wide(&t["liquidity_gross"])?.to_le_bytes());
            bytes.extend_from_slice(&wide(&t["fee_growth_outside0"])?.to_le_bytes());
            bytes.extend_from_slice(&wide(&t["fee_growth_outside1"])?.to_le_bytes());
            if let Some(growths) = t["reward_growths_outside"].as_array() {
                assert_eq!(growths.len(), 3);
                for growth in growths { bytes.extend_from_slice(&wide(growth)?.to_le_bytes()); }
            } else { bytes.extend_from_slice(&[0; 48]); }
        } else if !dynamic {
            bytes.extend_from_slice(&[0; 112]);
        }
    }
    if !dynamic {
        bytes.extend_from_slice(pool.as_ref());
    }
    Ok(bytes)
}

#[derive(Debug)]
pub struct Quote {
    pub amount_in: u64,
    pub amount_out: u64,
    pub next_pool: Value,
    pub audit: Value,
}

pub fn quote(pool: &Value, input: &str, a_to_b: bool) -> std::result::Result<Quote, String> {
    std::panic::catch_unwind(|| quote_inner(pool, input, a_to_b,true))
        .unwrap_or_else(|_| Err("original program panic (mutations discarded)".into()))
}
pub fn quote_exact_out(pool: &Value, output: &str, a_to_b: bool) -> std::result::Result<Quote,String> {
    std::panic::catch_unwind(|| quote_inner(pool,output,a_to_b,false)).unwrap_or_else(|_|Err("original program panic (mutations discarded)".into()))
}
fn quote_inner(pool: &Value, input: &str, a_to_b: bool, exact_in: bool) -> std::result::Result<Quote, String> {
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid input amount".into());
    }
    let amount = input.parse::<u64>().map_err(|_| "input exceeds u64")?;
    let mut state = whirlpool_state(pool)?;
    let s = &pool["orca_whirlpool"];
    let pool_key = pk(&pool["pool_id"])?;
    let owner = upstream::ID;
    let arrays = s["tick_arrays"]
        .as_array()
        .ok_or("missing complete arrays")?;
    if !(3..=7).contains(&arrays.len()) {
        return Err("profile requires 3..7 complete arrays".into());
    }
    let starts: Vec<i32> = arrays
        .iter()
        .map(|a| integer(&a["start_tick_index"]))
        .collect::<std::result::Result<_, _>>()?;
    let span = 88 * state.tick_spacing as i32;
    if starts.windows(2).any(|w| w[1] != w[0] + span) {
        return Err("array starts must be ordered and contiguous".into());
    }
    // Keep proof-window admission here; let the original directional builder below
    // decide which arrays the swap needs. After a left cross at a window's low edge,
    // tick may be start-1 while a B->A return correctly selects this same window.
    let mut bytes: Vec<Vec<u8>> = arrays
        .iter()
        .map(|a| array_bytes(a, &pool_key, state.tick_spacing))
        .collect::<std::result::Result<_, _>>()?;
    let keys: Vec<Pubkey> = arrays
        .iter()
        .map(|a| pk(&a["address"]))
        .collect::<std::result::Result<_, _>>()?;
    let mut lamports = vec![10_000_000u64; arrays.len()];
    let infos: Vec<AccountInfo> = keys
        .iter()
        .zip(lamports.iter_mut())
        .zip(bytes.iter_mut())
        .map(|((k, l), b)| AccountInfo::new(k, false, true, l, b, &owner, false, 0))
        .collect();
    let mut pool_bytes = Vec::new();
    state
        .try_serialize(&mut pool_bytes)
        .map_err(|e| e.to_string())?;
    let mut pool_lamports = 10_000_000u64;
    let pool_info = AccountInfo::new(
        &pool_key,
        false,
        true,
        &mut pool_lamports,
        &mut pool_bytes,
        &owner,
        false,
        0,
    );
    let account: Account<Whirlpool> = Account::try_from(&pool_info).map_err(|e| e.to_string())?;
    // This public original builder performs ordering, B->A shifted lookup and max-three selection.
    let builder = SparseSwapTickSequenceBuilder::new(infos.clone(), None);
    let mut sequence = builder
        .try_build(&account, a_to_b)
        .map_err(|e| format!("original builder: {e}"))?;
    let selected_starts: Vec<i32> = (0..3)
        .filter_map(|i| sequence.get_tick_offset(i, 0, state.tick_spacing).ok())
        .map(|offset| -(offset as i32) * state.tick_spacing as i32)
        .collect();
    let timestamp = num(&s["bank_unix_timestamp"])?;
    let update = swap_manager::swap(
        &state,
        &mut sequence,
        amount,
        0,
        exact_in,
        a_to_b,
        timestamp,
        &None,
    )
    .map_err(|e| format!("original manager: {e}"))?;
    let consumed = if a_to_b {
        update.amount_a
    } else {
        update.amount_b
    };
    let output = if a_to_b {
        update.amount_b
    } else {
        update.amount_a
    };
    // Exact-in domain requires the complete allocation to be consumed and a positive output.
    if (if exact_in {consumed}else{output}) != amount {
        return Err(format!(
            "partial fill rejected: consumed {consumed} of {amount}"
        ));
    }
    if output == 0 {
        return Err("zero output rejected".into());
    }
    let raw_in = num(&s[if a_to_b { "raw_vault0" } else { "raw_vault1" }])?;
    let raw_out = num(&s[if a_to_b { "raw_vault1" } else { "raw_vault0" }])?;
    let next_in = raw_in
        .checked_add(consumed)
        .ok_or("raw vault input overflow")?;
    let next_out = raw_out
        .checked_sub(output)
        .ok_or("raw vault output underflow")?;
    state.update_after_swap(
        update.next_liquidity,
        update.next_tick_index,
        update.next_sqrt_price,
        update.next_fee_growth_global,
        update.next_reward_infos,
        update.next_protocol_fee,
        a_to_b,
        timestamp,
    );
    drop(sequence);
    let mut next = pool.clone();
    let n = &mut next["orca_whirlpool"];
    n["sqrt_price_x64"] = json!(state.sqrt_price.to_string());
    n["liquidity"] = json!(state.liquidity.to_string());
    n["tick_current_index"] = json!(state.tick_current_index);
    n["fee_growth_global0"] = json!(state.fee_growth_global_a.to_string());
    n["fee_growth_global1"] = json!(state.fee_growth_global_b.to_string());
    n["protocol_fee_owed0"] = json!(state.protocol_fee_owed_a);
    n["protocol_fee_owed1"] = json!(state.protocol_fee_owed_b);
    if s["reward_infos"].is_array() {
        n["reward_infos"] = json!(state.reward_infos.iter().map(|r| json!({
            "mint":r.mint.to_string(), "vault":r.vault.to_string(), "extension":r.extension,
            "emissions_per_second_x64":r.emissions_per_second_x64.to_string(), "growth_global_x64":r.growth_global_x64.to_string()
        })).collect::<Vec<_>>());
    }
    n["reward_last_updated_timestamp"] = json!(state.reward_last_updated_timestamp);
    n[if a_to_b { "raw_vault0" } else { "raw_vault1" }] = json!(next_in);
    n[if a_to_b { "raw_vault1" } else { "raw_vault0" }] = json!(next_out);
    let mut crossed = Vec::new();
    for (i, info) in infos.iter().enumerate() {
        let loaded = load_tick_array(info, &pool_key).map_err(|e| e.to_string())?;
        for tick in n["tick_arrays"][i]["ticks"].as_array_mut().unwrap() {
            let tick_index = integer(&tick["tick"])?;
            let t = loaded
                .get_tick(tick_index, state.tick_spacing)
                .map_err(|e| e.to_string())?;
            let outside0 = t.fee_growth_outside_a;
            let outside1 = t.fee_growth_outside_b;
            if wide(&tick["fee_growth_outside0"])? != outside0
                || wide(&tick["fee_growth_outside1"])? != outside1
            {
                crossed.push(tick_index);
            }
            if tick["reward_growths_outside"].is_array() {
                tick["reward_growths_outside"] = json!({t.reward_growths_outside}.iter().map(|v|v.to_string()).collect::<Vec<_>>());
            }
            tick["fee_growth_outside0"] = json!(outside0.to_string());
            tick["fee_growth_outside1"] = json!(outside1.to_string());
        }
    }
    Ok(Quote {
        amount_in: consumed,
        amount_out: output,
        next_pool: next,
        audit: json!({"amount_consumed": consumed.to_string(), "lp_fee": update.lp_fee, "next_protocol_fee": update.next_protocol_fee,
        "crossed_ticks": crossed, "original_manager": true, "original_update_after_swap": true, "adaptive_fee_info": null, "selected_array_cap": 3, "selected_tick_array_starts": selected_starts}),
    })
}

pub fn fixture() -> Value { fixture_for(&base_pool()) }
pub fn fixture_for(pool: &Value) -> Value {
    let s = &pool["orca_whirlpool"];
    let state = whirlpool_state(&pool).unwrap();
    let pool_key = pk(&pool["pool_id"]).unwrap();
    let config_key = pk(&s["whirlpools_config"]).unwrap();
    let spacing = state.tick_spacing;
    let (fee_tier, tier_bump) = Pubkey::find_program_address(
        &[b"fee_tier", config_key.as_ref(), &spacing.to_le_bytes()],
        &upstream::ID,
    );
    let config = WhirlpoolsConfig {
        fee_authority: key("fee-authority"),
        collect_protocol_fees_authority: key("protocol-authority"),
        reward_emissions_super_authority: key("reward-authority"),
        default_protocol_fee_rate: state.protocol_fee_rate,
        feature_flags: 0,
    };
    let tier = FeeTier {
        whirlpools_config: config_key,
        tick_spacing: spacing,
        default_fee_rate: state.fee_rate,
    };
    let mut rpc = serde_json::Map::new();
    let mut add = |address: String, owner: &str, executable: bool, data: Vec<u8>| {
        rpc.insert(address, json!({"owner": owner,"executable": executable,"lamports": 10000000,"data_hex": hex(&data)}));
    };
    let mut data = Vec::new();
    state.try_serialize(&mut data).unwrap();
    add(pool_key.to_string(), &upstream::ID.to_string(), false, data);
    let mut data = Vec::new();
    config.try_serialize(&mut data).unwrap();
    add(
        config_key.to_string(),
        &upstream::ID.to_string(),
        false,
        data,
    );
    let mut data = Vec::new();
    tier.try_serialize(&mut data).unwrap();
    add(fee_tier.to_string(), &upstream::ID.to_string(), false, data);
    for side in 0..2 {
        let mint = pk(&pool[format!("token{side}")]).unwrap();
        let mut mint_data = vec![0u8; 82];
        mint_data[36..44].copy_from_slice(&u64::MAX.to_le_bytes());
        mint_data[44] = s[format!("mint{side}_decimals")].as_u64().unwrap() as u8;
        mint_data[45] = 1;
        add(mint.to_string(), TOKEN, false, mint_data);
        let mut token_data = vec![0u8; 165];
        token_data[..32].copy_from_slice(mint.as_ref());
        token_data[32..64].copy_from_slice(pool_key.as_ref());
        token_data[64..72].copy_from_slice(
            &s[format!("raw_vault{side}")]
                .as_u64()
                .unwrap()
                .to_le_bytes(),
        );
        token_data[108] = 1;
        add(
            s[format!("vault{side}")].as_str().unwrap().into(),
            TOKEN,
            false,
            token_data,
        );
    }
    for a in s["tick_arrays"].as_array().unwrap() {
        add(
            a["address"].as_str().unwrap().into(),
            &upstream::ID.to_string(),
            false,
            array_bytes(a, &pool_key, spacing).unwrap(),
        );
    }
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend_from_slice(pk(&s["programdata"]).unwrap().as_ref());
    add(upstream::ID.to_string(), LOADER, true, data);
    let mut data = 3u32.to_le_bytes().to_vec();
    data.extend_from_slice(&200u64.to_le_bytes());
    data.push(1);
    data.extend_from_slice(key("upgrade-authority").as_ref());
    data.extend_from_slice(PROGRAM_CODE.as_bytes());
    add(
        s["programdata"].as_str().unwrap().into(),
        LOADER,
        false,
        data,
    );
    add(
        TOKEN.into(),
        "BPFLoader2111111111111111111111111111111111",
        true,
        TOKEN_CODE.as_bytes().to_vec(),
    );
    let mut clock = Vec::new();
    for value in [123456i64, 1793050000, 987, 988, 1793059200] {
        clock.extend_from_slice(&value.to_le_bytes());
    }
    add(
        "SysvarC1ock11111111111111111111111111111111".into(),
        "Sysvar1111111111111111111111111111111111111",
        false,
        clock,
    );
    add(
        s["oracle"].as_str().unwrap().into(),
        "11111111111111111111111111111111",
        false,
        vec![],
    );
    json!({"schema_version":1,"pool":pool,"pool_config":{
        "id":pool["pool_id"],"protocol":pool["protocol"],"program_id":s["program_id"],"token0":pool["token0"],"token1":pool["token1"],"fee_numerator":pool["fee_numerator"],"fee_denominator":1000000,
        "accounts":[pool["pool_id"],s["vault0"],s["vault1"],s["whirlpools_config"],pool["token0"],pool["token1"]],
        "orca_whirlpool":{"expected_programdata":s["programdata"],"expected_program_code_sha256":s["program_code_sha256"],"expected_deployed_slot":s["deployed_slot"],"expected_token_program_code_sha256":s["token_program_code_sha256"],"tick_array_starts":[-5632,0,5632],"max_steps":8192}
    },"metadata":{
        "synthetic_proof_identity":true,"not_deployed_attestations":true,"revision":REVISION,"bank_commitment":"finalized","program_code_utf8":PROGRAM_CODE,"program_code_hex":hex(PROGRAM_CODE.as_bytes()),"program_code_sha256":s["program_code_sha256"],
        "token_program_code_utf8":TOKEN_CODE,"token_program_code_hex":hex(TOKEN_CODE.as_bytes()),"token_program_code_sha256":s["token_program_code_sha256"],"programdata":s["programdata"],"programdata_upgrade_authority":key("upgrade-authority").to_string(),"upgradeable_loader":LOADER,"immutable_loader":"BPFLoader2111111111111111111111111111111111","token_program":TOKEN,
        "clock":"SysvarC1ock11111111111111111111111111111111","clock_owner":"Sysvar1111111111111111111111111111111111111","pool_len":Whirlpool::LEN,"fixed_tick_array_len":FixedTickArray::LEN,"dynamic_tick_array_min_len":DynamicTickArray::MIN_LEN,"dynamic_tick_array_max_len":DynamicTickArray::MAX_LEN,
        "config":config_key.to_string(),"config_len":WhirlpoolsConfig::LEN,"config_feature_flags":0,"fee_authority":config.fee_authority.to_string(),"collect_protocol_fees_authority":config.collect_protocol_fees_authority.to_string(),"reward_emissions_super_authority":config.reward_emissions_super_authority.to_string(),
        "reward_authority_extension_hex":hex(&state.reward_infos[0].extension),"reward_control_flags":0,"fee_tier":fee_tier.to_string(),"fee_tier_bump":tier_bump,"fee_tier_len":FeeTier::LEN,"fee_tier_index_equals_spacing":true,
        "pool_mint_keys_sorted_by_bytes":true,"vault_derivation":"arbitrary on-curve signer public keys; InitializePool has no vault PDA seeds; token authority is Whirlpool",
        "pool_derivation":"find_program_address([whirlpool,config,mint0,mint1,tick_spacing.to_le_bytes()],program)","tick_array_derivation":"find_program_address([tick_array,pool,start_tick_index.to_string()],program)","oracle_derivation":"find_program_address([oracle,pool],program)","programdata_derivation":"find_program_address([program],upgradeable_loader)",
        "oracle_status":"uninitialized system-owned empty account; RPC null can substitute only this declared PDA","array_selection":"original SparseSwapTickSequenceBuilder: at most three selected from 3..7 complete accounts; B-to-A shifted first array",
        "limitations":["Synthetic code byte strings are fixture identities, not deployed program attestations","Native manager and account loaders only: no SBF, validator/bank, CPI, Token program, signatures, instruction account constraints, or RPC snapshot finality execution"]
    },"rpc_accounts":rpc})
}

pub fn tickmath_vectors() -> Value {
    let mut ticks = std::collections::BTreeSet::new();
    for t in MIN_TICK_INDEX..=MAX_TICK_INDEX {
        if t % 997 == 0 {
            ticks.insert(t);
        }
    }
    for t in [
        -443636, -443635, -327680, -262144, -131072, -65536, -32768, -16384, -8192, -4096, -2048,
        -1024, -512, -256, -128, -64, -2, -1, 0, 1, 2, 64, 128, 256, 512, 1024, 2048, 4096, 8192,
        16384, 32768, 65536, 131072, 262144, 327680, 443635, 443636,
    ] {
        for d in -2..=2 {
            if (MIN_TICK_INDEX..=MAX_TICK_INDEX).contains(&(t + d)) {
                ticks.insert(t + d);
            }
        }
    }
    let vectors: Vec<Value> = ticks.into_iter().map(|tick| {
        let price = math::sqrt_price_from_tick_index(tick);
        let below = if price > math::MIN_SQRT_PRICE_X64 { Some(math::tick_index_from_sqrt_price(&(price-1))) } else { None };
        let above = if price < math::MAX_SQRT_PRICE_X64 { Some(math::tick_index_from_sqrt_price(&(price+1))) } else { None };
        json!({"tick":tick,"sqrt_price_x64":price.to_string(),"inverse_tick":math::tick_index_from_sqrt_price(&price),"price_minus_one_inverse_tick":below,"price_plus_one_inverse_tick":above})
    }).collect();
    let mut all_hash = Sha256::new();
    for tick in MIN_TICK_INDEX..=MAX_TICK_INDEX {
        all_hash.update(tick.to_le_bytes());
        all_hash.update(math::sqrt_price_from_tick_index(tick).to_le_bytes());
    }
    json!({"schema_version":1,"reference":{"repository":"orca-so/whirlpools","revision":REVISION,"function":"original whirlpool::math::{sqrt_price_from_tick_index,tick_index_from_sqrt_price}","q_format":"Q64.64; no V3 shift","source":"upstream/programs/whirlpool/src/math/tick_math.rs","exhaustive_tick_count":887273,"exhaustive_tick_le_i32_price_le_u128_sha256":hex(&all_hash.finalize())},"min_tick":MIN_TICK_INDEX,"max_tick":MAX_TICK_INDEX,"min_sqrt_price_x64":math::MIN_SQRT_PRICE_X64.to_string(),"max_sqrt_price_x64":math::MAX_SQRT_PRICE_X64.to_string(),"vectors":vectors})
}

/// Kernel oracle supplements full swap observations with zero-liquidity and overflow cases.
pub fn reward_update(pool: &Value, outside: &[u128;3]) -> Value {
    let mut state=whirlpool_state(pool).unwrap();
    let rewards=upstream::manager::whirlpool_manager::next_whirlpool_reward_infos(&state, num(&pool["orca_whirlpool"]["bank_unix_timestamp"]).unwrap()).unwrap();
    let tick=Tick { reward_growths_outside:*outside, ..Tick::default() };
    let crossed=upstream::manager::tick_manager::next_tick_cross_update(&tick,0,0,&rewards).unwrap();
    state.reward_infos=rewards;
    json!({"reward_infos":rewards.iter().map(|r|json!({"mint":r.mint.to_string(),"vault":r.vault.to_string(),"extension":r.extension,"emissions_per_second_x64":r.emissions_per_second_x64.to_string(),"growth_global_x64":r.growth_global_x64.to_string()})).collect::<Vec<_>>(),
      "outside":crossed.reward_growths_outside.iter().map(|v|v.to_string()).collect::<Vec<_>>()})
}
