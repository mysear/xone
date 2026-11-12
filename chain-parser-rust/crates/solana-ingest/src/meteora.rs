//! Fixed-bank DLMM MM layout. All account and executable dependencies are explicit.
use crate::raydium::{get, RpcAccount};
use crate::rpc::TOKEN_PROGRAM;
use std::collections::{BTreeMap, BTreeSet};
use xone_config::{Chain, PoolConfig};
use xone_domain::{
    err,
    meteora::{self, *},
    Amount, Pool, Result,
};
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub fn required_accounts(p: &PoolConfig) -> Result<Vec<String>> {
    let cfg = p
        .meteora
        .as_ref()
        .ok_or_else(|| err("METEORA_CONFIG_REQUIRED"))?;
    cfg.validate(p, "solana:mainnet-beta")?;
    let mut ids = p.accounts.clone();
    ids.extend([
        PROGRAM.into(),
        cfg.expected_programdata.clone(),
        TOKEN_PROGRAM.into(),
        crate::raydium::CLOCK.into(),
    ]);
    crate::token_program::append_dependency(&mut ids, cfg.token_program_upgrade.as_ref())?;
    if cfg.token2022_upgrade.is_some() {
        ids.push(xone_domain::meteora_token2022::PROGRAM.into());
        crate::token_program::append_dependency_for(
            &mut ids,
            xone_domain::meteora_token2022::PROGRAM,
            cfg.token2022_upgrade.as_ref(),
        )?;
    }
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(err("METEORA_ACCOUNT_ALIAS"));
    }
    Ok(ids)
}
fn owned<'a>(
    accounts: &'a BTreeMap<String, RpcAccount>,
    id: &str,
    owner: &str,
    size: usize,
) -> Result<&'a [u8]> {
    let a = get(accounts, id)?;
    if a.executable || a.owner != owner || a.data.len() != size {
        return Err(err("METEORA_ACCOUNT_LAYOUT"));
    }
    Ok(&a.data)
}
pub fn decode(
    ch: &Chain,
    p: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    slot: u64,
) -> Result<Pool> {
    if !ch.key.starts_with("solana:") || ch.confirmation_tag != "finalized" {
        return Err(err("METEORA_REQUIRES_FINALIZED"));
    }
    let ids = required_accounts(p)?;
    if ids.iter().any(|id| !accounts.contains_key(id)) {
        return Err(err("ACCOUNT_MISSING"));
    }
    let cfg = p.meteora.as_ref().unwrap();
    let program = get(accounts, PROGRAM)?;
    let data = get(accounts, &cfg.expected_programdata)?;
    if !program.executable
        || program.owner != LOADER
        || program.data.len() != 36
        || u32_at(&program.data, 0) != 2
        || key(&program.data, 4) != cfg.expected_programdata
        || data.executable
        || data.owner != LOADER
        || data.data.len() <= 45
        || u32_at(&data.data, 0) != 3
        || data.data[12] > 1
        || u64_at(&data.data, 4) != cfg.expected_deployed_slot
        || cfg.expected_deployed_slot > slot
        || crate::raydium::digest(&data.data[45..]) != CODE
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    crate::token_program::verify(
        accounts,
        &cfg.expected_token_program_code_sha256,
        cfg.token_program_upgrade.as_ref(),
        slot,
    )?;
    if cfg.token2022_upgrade.is_some() {
        crate::token_program::verify_program(
            accounts,
            xone_domain::meteora_token2022::PROGRAM,
            xone_domain::meteora_token2022::CODE,
            cfg.token2022_upgrade.as_ref(),
            slot,
        )?;
    }
    let clock = owned(
        accounts,
        crate::raydium::CLOCK,
        "Sysvar1111111111111111111111111111111111111",
        40,
    )?;
    if u64_at(clock, 0) != slot || i64_at(clock, 32) < 0 {
        return Err(err("METEORA_CLOCK"));
    }
    let timestamp = u64_at(clock, 32);
    let pair = owned(accounts, &p.id, PROGRAM, PAIR_SIZE)?;
    if pair[..8] != discriminator("LbPair")
        || key(pair, 152) != p.accounts[1]
        || key(pair, 184) != p.accounts[2]
        || key(pair, 552) != p.accounts[5]
    {
        return Err(err("METEORA_DEPENDENCY_IDENTITY"));
    }
    let oracle = get(accounts, &p.accounts[5])?;
    if oracle.executable || oracle.owner != PROGRAM {
        return Err(err("METEORA_ACCOUNT_LAYOUT"));
    }
    let mut vaults = [0; 2];
    for (side, balance) in vaults.iter_mut().enumerate() {
        let mint_id = &p.accounts[3 + side];
        let vault_id = &p.accounts[1 + side];
        let token22 = side == 0 && cfg.token2022_upgrade.is_some();
        let owner = if token22 {
            xone_domain::meteora_token2022::PROGRAM
        } else {
            TOKEN_PROGRAM
        };
        let mint = owned(accounts, mint_id, owner, if token22 { 278 } else { 82 })?;
        let vault = owned(accounts, vault_id, owner, if token22 { 178 } else { 165 })?;
        if key(pair, 88 + side * 32) != *mint_id
            || key(vault, 0) != *mint_id
            || key(vault, 32) != p.id
            || mint[45] != 1
            || u32_at(mint, 0) > 1
            || u32_at(mint, 46) > 1
            || vault[108] != 1
            || u32_at(vault, 72) != 0
            || u64_at(vault, 121) != 0
            || u32_at(vault, 129) != 0
        {
            return Err(err("METEORA_TOKEN_POLICY"));
        }
        let raw = u64_at(vault, 64);
        *balance = raw;
        if mint_id == "So11111111111111111111111111111111111111112" {
            if mint[44] != 9
                || u32_at(vault, 109) != 1
                || get(accounts, vault_id)?
                    .lamports
                    .checked_sub(u64_at(vault, 113))
                    .is_none_or(|n| n < raw)
            {
                return Err(err("NATIVE_VAULT_BACKING"));
            }
        } else if u32_at(vault, 109) != 0 {
            return Err(err("METEORA_TOKEN_POLICY"));
        }
    }
    let bin_arrays = cfg
        .bin_array_indices
        .iter()
        .enumerate()
        .map(|(i, index)| {
            Ok(BinArray {
                address: p.accounts[6 + i].clone(),
                index: *index,
                data: hex::encode(owned(accounts, &p.accounts[6 + i], PROGRAM, ARRAY_SIZE)?),
            })
        })
        .collect::<Result<_>>()?;
    let state = MeteoraState {
        token2022: cfg
            .token2022_upgrade
            .as_ref()
            .map(|t| {
                Ok(xone_domain::meteora_token2022::Token2022State {
                    programdata: t.programdata.clone(),
                    deployed_slot: t.deployed_slot,
                    program_code_sha256: xone_domain::meteora_token2022::CODE.into(),
                    bank_epoch: u64_at(clock, 16),
                    mint_data: hex::encode(owned(
                        accounts,
                        &p.token0,
                        xone_domain::meteora_token2022::PROGRAM,
                        278,
                    )?),
                    vault_data: hex::encode(owned(
                        accounts,
                        &p.accounts[1],
                        xone_domain::meteora_token2022::PROGRAM,
                        178,
                    )?),
                })
            })
            .transpose()?,
        bitmap_extension: cfg
            .bitmap_extension
            .as_ref()
            .map(|id| {
                Ok(xone_domain::meteora::BitmapExtension {
                    address: id.clone(),
                    data: hex::encode(owned(
                        accounts,
                        id,
                        PROGRAM,
                        xone_domain::meteora::BITMAP_SIZE,
                    )?),
                })
            })
            .transpose()?,
        programdata: cfg.expected_programdata.clone(),
        program_code_sha256: cfg.expected_program_code_sha256.clone(),
        deployed_slot: cfg.expected_deployed_slot,
        token_program_code_sha256: cfg.expected_token_program_code_sha256.clone(),
        slot,
        timestamp,
        pair_data: hex::encode(pair),
        oracle_data: hex::encode(&oracle.data),
        bin_arrays,
        raw_vault0: vaults[0],
        raw_vault1: vaults[1],
        max_steps: cfg.max_steps,
        derived: false,
    };
    // Deserialization initializes every unrelated model to None without coupling
    // this independent decoder to the representation of the other DEX families.
    let pool:Pool=serde_json::from_value(serde_json::json!({"chain_key":ch.key,"pool_id":p.id,"protocol":p.protocol,"adapter_version":meteora::adapter(&p.protocol)?,"token_behavior":"standard","token0":p.token0,"token1":p.token1,"reserve0":Amount(vaults[0].into()),"reserve1":Amount(vaults[1].into()),"fee_numerator":0,"fee_denominator":1_000_000_000u64,"meteora":state})).map_err(|_|err("METEORA_STATE_ENCODING"))?;
    pool.validate()?;
    Ok(pool)
}
