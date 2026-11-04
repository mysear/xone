//! Pinned CP-Swap creator layout with classic SPL Token and same-bank identity proofs.
use crate::rpc::{CPMM_PROGRAM, TOKEN_PROGRAM};
use base64::{engine::general_purpose::STANDARD, Engine};
use curve25519_dalek::edwards::CompressedEdwardsY;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use xone_config::{Chain, PoolConfig};
use xone_domain::{err, raydium::RaydiumCPMMState, Amount, Pool, Result};

pub const CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
const SYSVAR_OWNER: &str = "Sysvar1111111111111111111111111111111111111";
const UPGRADEABLE_LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const IMMUTABLE_LOADER: &str = "BPFLoader2111111111111111111111111111111111";
const NATIVE_MINT: &str = "So11111111111111111111111111111111111111112";
// raydium-io/raydium-cp-swap f4b9b263, src/lib.rs::AUTH_SEED.
const AUTH_SEED: &[u8] = b"vault_and_lp_mint_auth_seed";
const M: u64 = 1_000_000;

#[derive(Clone, Debug)]
pub struct RpcAccount {
    pub owner: String,
    pub data: Vec<u8>,
    pub executable: bool,
    pub lamports: u64,
}
impl RpcAccount {
    pub fn decode(v: &Value) -> Result<Self> {
        let owner = v["owner"].as_str().ok_or_else(|| err("ACCOUNT_MISSING"))?;
        pubkey(owner)?;
        let executable = v["executable"]
            .as_bool()
            .ok_or_else(|| err("ACCOUNT_EXECUTABLE_MISSING"))?;
        let lamports = v["lamports"]
            .as_u64()
            .ok_or_else(|| err("ACCOUNT_LAMPORTS_MISSING"))?;
        let encoded = v["data"]
            .as_array()
            .filter(|a| a.len() == 2 && a[1] == "base64")
            .and_then(|a| a[0].as_str())
            .ok_or_else(|| err("ACCOUNT_ENCODING"))?;
        let data = STANDARD
            .decode(encoded)
            .map_err(|_| err("ACCOUNT_ENCODING"))?;
        Ok(Self {
            owner: owner.into(),
            data,
            executable,
            lamports,
        })
    }
}
pub(crate) fn pubkey(s: &str) -> Result<[u8; 32]> {
    let bytes: [u8; 32] = bs58::decode(s)
        .into_vec()
        .map_err(|_| err("SOLANA_PUBKEY_IDENTITY"))?
        .try_into()
        .map_err(|_| err("SOLANA_PUBKEY_IDENTITY"))?;
    if bs58::encode(bytes).into_string() != s || bytes == [0; 32] {
        return Err(err("SOLANA_PUBKEY_IDENTITY"));
    }
    Ok(bytes)
}
pub(crate) fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub(crate) fn check_digest(s: &str) -> Result<()> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(err("RAYDIUM_CODE_IDENTITY"));
    }
    Ok(())
}
/// Solana's create_program_address hash and Edwards decompression rejection,
/// searching canonical bumps from 255 down to zero (all seeds here are <=32 bytes).
pub(crate) fn pda(seeds: &[&[u8]], program: &[u8; 32]) -> Result<(String, u8)> {
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        for seed in seeds {
            hash.update(seed);
        }
        hash.update([bump]);
        hash.update(program);
        hash.update(b"ProgramDerivedAddress");
        let bytes: [u8; 32] = hash.finalize().into();
        if CompressedEdwardsY(bytes).decompress().is_none() {
            return Ok((bs58::encode(bytes).into_string(), bump));
        }
    }
    Err(err("SOLANA_PDA_DERIVATION"))
}
pub(crate) fn u64_at(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}
pub(crate) fn tag_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
pub(crate) fn key_at(d: &[u8], o: usize) -> String {
    bs58::encode(&d[o..o + 32]).into_string()
}
pub(crate) fn get<'a>(
    accounts: &'a BTreeMap<String, RpcAccount>,
    id: &str,
) -> Result<&'a RpcAccount> {
    accounts
        .get(id)
        .ok_or_else(|| err("ACCOUNT_DEPENDENCY_MISSING"))
}
pub(crate) fn owned_data<'a>(
    accounts: &'a BTreeMap<String, RpcAccount>,
    id: &str,
    owner: &str,
    len: usize,
) -> Result<&'a [u8]> {
    let a = get(accounts, id)?;
    if a.executable || a.owner != owner || a.data.len() != len {
        return Err(err("ACCOUNT_OWNER_OR_LAYOUT"));
    }
    Ok(&a.data)
}

pub fn required_accounts(p: &PoolConfig) -> Result<Vec<String>> {
    if p.protocol != "raydium-cpmm-creator"
        || p.program_id != CPMM_PROGRAM
        || p.accounts.len() != 6
        || p.accounts[0] != p.id
        || p.accounts[4] != p.token0
        || p.accounts[5] != p.token1
        || p.v2.is_some()
        || p.v3.is_some()
        || p.stable.is_some()
        || p.singleton_cl.is_some()
    {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    let cfg = p
        .raydium_cpmm
        .as_ref()
        .ok_or_else(|| err("RAYDIUM_CONFIG_REQUIRED"))?;
    check_digest(&cfg.expected_program_code_sha256)?;
    check_digest(&cfg.expected_token_program_code_sha256)?;
    if pubkey(&p.token0)? >= pubkey(&p.token1)? {
        return Err(err("RAYDIUM_MINT_ORDER"));
    }
    let program = pubkey(CPMM_PROGRAM)?;
    if cfg.expected_programdata != pda(&[&program], &pubkey(UPGRADEABLE_LOADER)?)?.0 {
        return Err(err("RAYDIUM_PROGRAMDATA_IDENTITY"));
    }
    let mut ids = p.accounts.clone();
    ids.extend([
        CPMM_PROGRAM.into(),
        cfg.expected_programdata.clone(),
        TOKEN_PROGRAM.into(),
        CLOCK.into(),
    ]);
    let mut seen = BTreeSet::new();
    for id in &ids {
        pubkey(id)?;
        if !seen.insert(id) {
            return Err(err("RAYDIUM_ACCOUNT_ALIAS"));
        }
    }
    Ok(ids)
}

pub fn decode_raydium_cpmm(
    ch: &Chain,
    p: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    bank_slot: u64,
) -> Result<Pool> {
    if !ch.key.starts_with("solana:") {
        return Err(err("PROTOCOL_CHAIN_FAMILY_MISMATCH"));
    }
    if ch.confirmation_tag != "finalized" {
        return Err(err("RAYDIUM_REQUIRES_FINALIZED"));
    }
    required_accounts(p)?;
    let identity = p
        .raydium_cpmm
        .as_ref()
        .ok_or_else(|| err("RAYDIUM_CONFIG_REQUIRED"))?;
    let program_key = pubkey(CPMM_PROGRAM)?;
    let program = get(accounts, CPMM_PROGRAM)?;
    if !program.executable
        || program.owner != UPGRADEABLE_LOADER
        || program.data.len() != 36
        || tag_at(&program.data, 0) != 2
        || key_at(&program.data, 4) != identity.expected_programdata
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    let programdata = get(accounts, &identity.expected_programdata)?;
    if programdata.executable
        || programdata.owner != UPGRADEABLE_LOADER
        || programdata.data.len() <= 45
        || tag_at(&programdata.data, 0) != 3
        || programdata.data[12] > 1
        || u64_at(&programdata.data, 4) != identity.expected_deployed_slot
        || identity.expected_deployed_slot > bank_slot
        || digest(&programdata.data[45..]) != identity.expected_program_code_sha256
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    let token_program = get(accounts, TOKEN_PROGRAM)?;
    if !token_program.executable
        || token_program.owner != IMMUTABLE_LOADER
        || token_program.data.is_empty()
        || digest(&token_program.data) != identity.expected_token_program_code_sha256
    {
        return Err(err("TOKEN_PROGRAM_VERSION_UNSUPPORTED"));
    }
    let clock = owned_data(accounts, CLOCK, SYSVAR_OWNER, 40)?;
    if u64_at(clock, 0) != bank_slot {
        return Err(err("CLOCK_CONTEXT_MISMATCH"));
    }
    let bank_unix_timestamp = i64::from_le_bytes(clock[32..40].try_into().unwrap());
    let bank_unix_timestamp: u64 = bank_unix_timestamp
        .try_into()
        .map_err(|_| err("CLOCK_TIMESTAMP_INVALID"))?;
    let bank_epoch = u64_at(clock, 16);
    let state = owned_data(accounts, &p.accounts[0], CPMM_PROGRAM, 637)?;
    let cfg = owned_data(accounts, &p.accounts[3], CPMM_PROGRAM, 236)?;
    if state[..8] != [247, 237, 227, 245, 215, 195, 222, 70]
        || cfg[..8] != [218, 244, 33, 104, 203, 203, 43, 111]
        || state[389] > 2
        || state[390] > 1
        || cfg[9] > 1
        || state[391..397]
            .iter()
            .chain(state[413..].iter())
            .any(|b| *b != 0)
        || cfg[124..].iter().any(|b| *b != 0)
        || state[329] & !7 != 0
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    if state[329] & 4 != 0 {
        return Err(err("POOL_SWAP_DISABLED"));
    }
    if u64_at(state, 373) > bank_unix_timestamp {
        return Err(err("POOL_NOT_OPEN"));
    }
    if u64_at(state, 381) > bank_epoch {
        return Err(err("POOL_EPOCH_INVALID"));
    }
    let (authority, authority_bump) = pda(&[AUTH_SEED], &program_key)?;
    let pool_key = pubkey(&p.id)?;
    let index = u16::from_le_bytes(cfg[10..12].try_into().unwrap()).to_be_bytes();
    let (config_key, config_bump) = pda(&[b"amm_config", &index], &program_key)?;
    if config_key != p.accounts[3]
        || cfg[8] != config_bump
        || state[328] != authority_bump
        || key_at(state, 8) != p.accounts[3]
        || state[330] != 9
        || key_at(state, 136) != pda(&[b"pool_lp_mint", &pool_key], &program_key)?.0
        || key_at(state, 296) != pda(&[b"observation", &pool_key], &program_key)?.0
    {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    let mut raw = [0; 2];
    for i in 0..2 {
        let mint_key = &p.accounts[4 + i];
        let mint = owned_data(accounts, mint_key, TOKEN_PROGRAM, 82)?;
        let vault = owned_data(accounts, &p.accounts[1 + i], TOKEN_PROGRAM, 165)?;
        if key_at(state, 72 + 32 * i) != p.accounts[1 + i]
            || key_at(state, 168 + 32 * i) != *mint_key
            || key_at(state, 232 + 32 * i) != TOKEN_PROGRAM
            || p.accounts[1 + i]
                != pda(
                    &[b"pool_vault", &pool_key, &pubkey(mint_key)?],
                    &program_key,
                )?
                .0
            || key_at(vault, 0) != *mint_key
            || key_at(vault, 32) != authority
        {
            return Err(err("POOL_ACCOUNT_IDENTITY"));
        }
        // COption<Pubkey> uses a u32 discriminant, not an Anchor u8 option.
        // None payload bytes may be stale after SPL pack(), so only flags are constrained.
        if mint[45] != 1
            || mint[44] != state[331 + i]
            || tag_at(mint, 0) > 1
            || tag_at(mint, 46) > 1
            || vault[108] != 1
            || tag_at(vault, 72) != 0
            || u64_at(vault, 121) != 0
            || tag_at(vault, 129) != 0
        {
            return Err(err("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED"));
        }
        raw[i] = u64_at(vault, 64);
        if mint_key == NATIVE_MINT {
            let lamports = get(accounts, &p.accounts[1 + i])?.lamports;
            if mint[44] != 9
                || tag_at(vault, 109) != 1
                || lamports
                    .checked_sub(u64_at(vault, 113))
                    .is_none_or(|backing| backing < raw[i])
            {
                return Err(err("NATIVE_VAULT_BACKING"));
            }
        } else if tag_at(vault, 109) != 0 {
            return Err(err("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED"));
        }
    }
    let trade_fee = u64_at(cfg, 12);
    let protocol_fee = u64_at(cfg, 20);
    let fund_fee = u64_at(cfg, 28);
    let creator_fee = u64_at(cfg, 108);
    // Bound snapshot metadata independently; combined trade/creator arithmetic
    // depends on the enabled swap branch and belongs to the quote calculation.
    if trade_fee >= M
        || creator_fee > M
        || protocol_fee.checked_add(fund_fee).is_none_or(|fee| fee > M)
        || u64_at(cfg, 116) > M
    {
        return Err(err("RAYDIUM_FEE_RANGE"));
    }
    let s = RaydiumCPMMState {
        program_id: CPMM_PROGRAM.into(),
        programdata: identity.expected_programdata.clone(),
        program_code_sha256: identity.expected_program_code_sha256.clone(),
        deployed_slot: identity.expected_deployed_slot,
        token_program_code_sha256: identity.expected_token_program_code_sha256.clone(),
        authority,
        authority_bump: authority_bump.into(),
        amm_config: p.accounts[3].clone(),
        vault0: p.accounts[1].clone(),
        vault1: p.accounts[2].clone(),
        raw_vault0: raw[0],
        raw_vault1: raw[1],
        protocol_fee_rate: protocol_fee,
        fund_fee_rate: fund_fee,
        creator_fee_rate: creator_fee,
        creator_fee_share_rate: u64_at(cfg, 116),
        creator_fee_on: state[389].into(),
        creator_fee_enabled: state[390] == 1,
        protocol_fees0: u64_at(state, 341),
        protocol_fees1: u64_at(state, 349),
        fund_fees0: u64_at(state, 357),
        fund_fees1: u64_at(state, 365),
        creator_fees0: u64_at(state, 397),
        creator_fees1: u64_at(state, 405),
        bank_slot,
        bank_unix_timestamp,
        bank_epoch,
        open_time: u64_at(state, 373),
        recent_epoch: u64_at(state, 381),
        mint0_decimals: state[331].into(),
        mint1_decimals: state[332].into(),
    };
    let (reserve0, reserve1) = s.net_reserves()?;
    if reserve0 == 0 || reserve1 == 0 {
        return Err(err("POOL_EMPTY_RESERVES"));
    }
    let pool = Pool {
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
        raydium_cpmm: Some(s),
        chain_key: ch.key.clone(),
        pool_id: p.id.clone(),
        protocol: "raydium-cpmm-creator".into(),
        token0: p.token0.clone(),
        token1: p.token1.clone(),
        reserve0: Amount::checked(reserve0.into())?,
        reserve1: Amount::checked(reserve1.into())?,
        fee_numerator: trade_fee,
        fee_denominator: M,
        adapter_version: "raydium-cpmm/f4b9b263-spl-1".into(),
        token_behavior: "standard".into(),
    };
    pool.validate()?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_authority_matches_independently_derived_pin() {
        assert_eq!(
            pda(&[AUTH_SEED], &pubkey(CPMM_PROGRAM).unwrap()).unwrap(),
            ("GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL".into(), 253)
        );
    }
}
