//! Exact finalized account closure for the fixed Pump legacy classic/SOL profile.
use crate::{
    raydium::{digest, get, RpcAccount},
    rpc::TOKEN_PROGRAM,
};
use std::collections::{BTreeMap, BTreeSet};
use xone_config::{Chain, PoolConfig};
use xone_domain::{
    err,
    meteora::{key, pda, pubkey, u32_at, u64_at},
    pump::{self, *},
    Pool, Result,
};
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub fn required_accounts(p: &PoolConfig) -> Result<Vec<String>> {
    let c = p.pump.as_ref().ok_or_else(|| err("PUMP_CONFIG_REQUIRED"))?;
    c.validate(p, "solana:localnet")?;
    let mut ids = p.accounts.clone();
    ids.extend([
        p.program_id.clone(),
        c.expected_programdata.clone(),
        FEE_PROGRAM.into(),
        c.expected_fee_programdata.clone(),
        TOKEN_PROGRAM.into(),
        crate::raydium::CLOCK.into(),
    ]);
    crate::token_program::append_dependency(&mut ids, c.token_program_upgrade.as_ref())?;
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(err("PUMP_ACCOUNT_ALIAS"));
    }
    Ok(ids)
}
pub fn decode(
    ch: &Chain,
    p: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    slot: u64,
) -> Result<Pool> {
    required_accounts(p)?;
    if !ch.key.starts_with("solana:") || ch.confirmation_tag != "finalized" {
        return Err(err("PUMP_REQUIRES_FINALIZED"));
    }
    let c = p.pump.as_ref().unwrap();
    for (id, data, deploy, code) in [
        (
            p.program_id.as_str(),
            &c.expected_programdata,
            c.expected_deployed_slot,
            c.expected_program_code_sha256.as_str(),
        ),
        (
            FEE_PROGRAM,
            &c.expected_fee_programdata,
            c.expected_fee_deployed_slot,
            FEE_CODE,
        ),
    ] {
        let program = get(accounts, id)?;
        let d = get(accounts, data)?;
        if !program.executable
            || program.owner != LOADER
            || program.data.len() != 36
            || u32_at(&program.data, 0) != 2
            || key(&program.data, 4) != *data
            || d.executable
            || d.owner != LOADER
            || d.data.len() < 45
            || u32_at(&d.data, 0) != 3
            || u64_at(&d.data, 4) != deploy
            || deploy > slot
            || d.data[12] > 1
            || digest(&d.data[45..]) != code
        {
            return Err(err("PUMP_PROGRAM_VERSION"));
        }
    }
    crate::token_program::verify(
        accounts,
        &c.expected_token_program_code_sha256,
        c.token_program_upgrade.as_ref(),
        slot,
    )?;
    let clock = get(accounts, crate::raydium::CLOCK)?;
    if clock.executable
        || clock.owner != "Sysvar1111111111111111111111111111111111111"
        || clock.data.len() != 40
        || u64_at(&clock.data, 0) != slot
        || u64_at(&clock.data, 32) > i64::MAX as u64
    {
        return Err(err("PUMP_CLOCK"));
    }
    let raw = &get(accounts, &p.id)?.data;
    let amm = xone_domain::pump_amm::supports_profile(&p.protocol);
    if raw.len() != if amm { 287 } else { 166 } {
        return Err(err("PUMP_CURVE_LAYOUT"));
    }
    let vault = pda(
        &[&pubkey(&p.id)?, &pubkey(TOKEN)?, &pubkey(&p.token0)?],
        ASSOCIATED,
    )?;
    let global = if amm {
        xone_domain::pump_amm::global_id()?
    } else {
        PumpState::global_id()?
    };
    let g = &get(accounts, &global)?.data;
    if g.len() != if amm { 949 } else { 1088 } {
        return Err(err("PUMP_GLOBAL_LAYOUT"));
    }
    let records = p
        .accounts
        .iter()
        .map(|id| {
            let a = get(accounts, id)?;
            if a.executable {
                return Err(err("PUMP_ACCOUNT_EXECUTABLE"));
            }
            Ok(pump::Account {
                address: id.clone(),
                owner: a.owner.clone(),
                lamports: a.lamports,
                data: hex::encode(&a.data),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut state = PumpState {
        programdata: c.expected_programdata.clone(),
        deployed_slot: c.expected_deployed_slot,
        program_code_sha256: c.expected_program_code_sha256.clone(),
        fee_programdata: c.expected_fee_programdata.clone(),
        fee_deployed_slot: c.expected_fee_deployed_slot,
        fee_program_code_sha256: c.expected_fee_program_code_sha256.clone(),
        token_program_code_sha256: c.expected_token_program_code_sha256.clone(),
        slot,
        timestamp: u64_at(&clock.data, 32),
        user: c.user.clone(),
        vault,
        creator_vault: if amm {
            xone_domain::pump_amm::ata(
                xone_domain::pump_amm::WSOL,
                &pda(
                    &[b"creator_vault", &pubkey(&key(raw, 211))?],
                    xone_domain::pump_amm::PROGRAM,
                )?,
            )?
        } else {
            pda(&[b"creator-vault", &pubkey(&key(raw, 49))?], PROGRAM)?
        },
        fee_recipient: key(g, if amm { 57 } else { 41 }),
        buyback_recipient: key(g, if amm { 643 } else { 741 }),
        accounts: records,
        derived: false,
    };
    state.accounts.sort_by(|a, b| a.address.cmp(&b.address));
    let pool:Pool=serde_json::from_value(serde_json::json!({"chain_key":ch.key,"pool_id":p.id,"protocol":p.protocol,"adapter_version":if amm {xone_domain::pump_amm::adapter(&p.protocol)?}else if p.protocol==MIGRATION_PROFILE{MIGRATION_ADAPTER}else{ADAPTER},"token_behavior":"standard","token0":p.token0,"token1":p.token1,"reserve0":if amm {u64_at(&get(accounts,&state.vault)?.data,64)}else{u64_at(raw,24)}.to_string(),"reserve1":if amm {u64_at(&get(accounts,&key(raw,171))?.data,64)}else{u64_at(raw,32)}.to_string(),"fee_numerator":0,"fee_denominator":10000,"pump":state})).map_err(|_|err("PUMP_STATE_ENCODING"))?;
    pool.validate()?;
    Ok(pool)
}

/// Empty System-owned fee vaults are valid dependencies of native SOL Pump
/// instructions. The existing SPL decoders retain their nonzero-owner policy.
pub fn decode_account(v: &serde_json::Value) -> Result<RpcAccount> {
    if v["owner"].as_str() != Some(SYSTEM) {
        return RpcAccount::decode(v);
    }
    use base64::{engine::general_purpose::STANDARD, Engine};
    let encoding = v["data"]
        .as_array()
        .filter(|a| a.len() == 2 && a[1] == "base64")
        .and_then(|a| a[0].as_str())
        .ok_or_else(|| err("ACCOUNT_ENCODING"))?;
    let data = STANDARD
        .decode(encoding)
        .map_err(|_| err("ACCOUNT_ENCODING"))?;
    if !data.is_empty() || v["executable"].as_bool() != Some(false) {
        return Err(err("PUMP_FUND_ACCOUNT"));
    }
    Ok(RpcAccount {
        owner: SYSTEM.into(),
        data,
        executable: false,
        lamports: v["lamports"]
            .as_u64()
            .ok_or_else(|| err("ACCOUNT_LAMPORTS_MISSING"))?,
    })
}
