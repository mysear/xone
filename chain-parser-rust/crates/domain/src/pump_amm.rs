//! Distinct canonical PumpSwap identity, classic SPL / backed WSOL v1 templates.
use crate::{
    err,
    meteora::*,
    pump::{PumpState, ASSOCIATED, FEE_CODE, FEE_PROGRAM, TOKEN},
    Amount, Pool, Result,
};
pub const BOOST_PROFILE: &str = "pump-amm-classic-wsol-boost";
pub const BOOST_ADAPTER: &str = "pump-amm/6a7ad34596d8-classic-wsol-boost-1";
pub fn supports_profile(profile: &str) -> bool {
    matches!(profile, PROFILE | BOOST_PROFILE)
}
pub fn adapter(profile: &str) -> Result<&'static str> {
    match profile {
        PROFILE => Ok(ADAPTER),
        BOOST_PROFILE => Ok(BOOST_ADAPTER),
        _ => Err(err("PUMP_AMM_PROFILE")),
    }
}
pub const PROFILE: &str = "pump-amm-classic-wsol";
pub const ADAPTER: &str = "pump-amm/6a7ad34596d8-classic-wsol-1";
pub const PROGRAM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
pub const CODE: &str = "6a7ad34596d83b4c435b32f458a56e718e9f82eb4ad38e4316616d15d24d8ff7";
pub const WSOL: &str = "So11111111111111111111111111111111111111112";
pub fn global_id() -> Result<String> {
    pda(&[b"global_config"], PROGRAM)
}
pub fn fee_id() -> Result<String> {
    pda(&[b"fee_config", &pubkey(PROGRAM)?], FEE_PROGRAM)
}
pub fn ata(mint: &str, owner: &str) -> Result<String> {
    pda(
        &[&pubkey(owner)?, &pubkey(TOKEN)?, &pubkey(mint)?],
        ASSOCIATED,
    )
}
pub fn validate(p: &Pool, s: &PumpState) -> Result<()> {
    if !supports_profile(&p.protocol)
        || p.adapter_version != adapter(&p.protocol)?
        || !p.chain_key.starts_with("solana:")
        || p.token_behavior != "standard"
        || p.token1 != WSOL
        || p.token0 == WSOL
        || p.fee_numerator != 0
        || p.fee_denominator != 10000
    {
        return Err(err("PUMP_AMM_PROFILE"));
    }
    if s.program_code_sha256 != CODE
        || s.fee_program_code_sha256 != FEE_CODE
        || s.token_program_code_sha256 != TOKEN_CODE
        || s.deployed_slot == 0
        || s.deployed_slot > s.slot
        || s.fee_deployed_slot == 0
        || s.fee_deployed_slot > s.slot
        || s.timestamp > i64::MAX as u64
    {
        return Err(err("PUMP_AMM_VERSION"));
    }
    for (program, data) in [(PROGRAM, &s.programdata), (FEE_PROGRAM, &s.fee_programdata)] {
        if pda(
            &[&pubkey(program)?],
            "BPFLoaderUpgradeab1e11111111111111111111111",
        )? != *data
        {
            return Err(err("PUMP_AMM_PROGRAMDATA"));
        }
    }
    pubkey(&s.user)?;
    let d = s.data(&p.pool_id, 287)?;
    let g = s.data(&global_id()?, 949)?;
    if d[..8] != discriminator("Pool")
        || g[..8] != discriminator("GlobalConfig")
        || d[9..11] != [0, 0]
        || d[243..245].iter().any(|b| *b != 0)
        || d[261..287].iter().any(|b| *b != 0)
        || g[56] != 0
        || u64_at(&g, 899) > 10000
    {
        return Err(err("PUMP_AMM_POLICY"));
    }
    let virtual_quote = virtual_quote(s, p)?;
    if (p.protocol == BOOST_PROFILE && virtual_quote == 0)
        || u64_at(&s.data(&ata(WSOL, &p.pool_id)?, 165)?, 64)
            .checked_add(virtual_quote)
            .is_none()
    {
        return Err(err("PUMP_AMM_VIRTUAL_QUOTE"));
    }
    let creator = pda(
        &[b"pool-authority", &pubkey(&p.token0)?],
        crate::pump::PROGRAM,
    )?;
    if key(&d, 11) != creator
        || key(&d, 43) != p.token0
        || key(&d, 75) != p.token1
        || key(&d, 107) != pda(&[b"pool_lp_mint", &pubkey(&p.pool_id)?], PROGRAM)?
        || pda(
            &[
                b"pool",
                &[0, 0],
                &pubkey(&creator)?,
                &pubkey(&p.token0)?,
                &pubkey(WSOL)?,
            ],
            PROGRAM,
        )? != p.pool_id
        || key(&d, 139) != s.vault
        || s.vault != ata(&p.token0, &p.pool_id)?
        || key(&d, 171) != ata(WSOL, &p.pool_id)?
    {
        return Err(err("PUMP_AMM_IDENTITY"));
    }
    // Bind the stored bump independently of a unique canonical address match.
    {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for seed in [
            &b"pool"[..],
            &[0u8, 0],
            &pubkey(&creator)?,
            &pubkey(&p.token0)?,
            &pubkey(WSOL)?,
            &d[8..9],
        ] {
            h.update(seed);
        }
        h.update(pubkey(PROGRAM)?);
        h.update(b"ProgramDerivedAddress");
        if h.finalize()[..] != pubkey(&p.pool_id)? {
            return Err(err("PUMP_AMM_BUMP"));
        }
    }
    let coin = key(&d, 211);
    if coin == crate::pump::SYSTEM {
        return Err(err("PUMP_AMM_CREATOR_REQUIRED"));
    }
    let authority = pda(&[b"creator_vault", &pubkey(&coin)?], PROGRAM)?;
    if s.creator_vault != ata(WSOL, &authority)?
        || s.fee_recipient != key(&g, 57)
        || s.buyback_recipient != key(&g, 643)
    {
        return Err(err("PUMP_AMM_FEE_IDENTITY"));
    }
    let gv = pda(&[b"global_volume_accumulator"], PROGRAM)?;
    let uv = pda(&[b"user_volume_accumulator", &pubkey(&s.user)?], PROGRAM)?;
    let expected = [
        p.pool_id.clone(),
        p.token0.clone(),
        p.token1.clone(),
        s.vault.clone(),
        key(&d, 171),
        global_id()?,
        fee_id()?,
        s.creator_vault.clone(),
        ata(WSOL, &s.fee_recipient)?,
        ata(WSOL, &s.buyback_recipient)?,
        gv.clone(),
        uv.clone(),
    ];
    if s.accounts.len() != expected.len()
        || expected
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != expected.len()
        || s.accounts.windows(2).any(|a| a[0].address >= a[1].address)
        || s.accounts.iter().any(|a| !expected.contains(&a.address))
        || [&s.fee_recipient, &s.buyback_recipient, &authority].contains(&&s.user)
        || expected.contains(&s.user)
        || [PROGRAM, FEE_PROGRAM, &s.programdata, &s.fee_programdata].contains(&s.user.as_str())
    {
        return Err(err("PUMP_AMM_ACCOUNT_SET"));
    }
    for (id, owner) in [
        (&p.pool_id, PROGRAM),
        (&global_id()?, PROGRAM),
        (&fee_id()?, FEE_PROGRAM),
        (&gv, PROGRAM),
        (&uv, PROGRAM),
    ] {
        if s.account(id)?.owner != owner {
            return Err(err("PUMP_AMM_ACCOUNT_OWNER"));
        }
    }
    for mint in [&p.token0, &p.token1] {
        let a = s.account(mint)?;
        let raw = s.data(mint, 82)?;
        if a.owner != TOKEN
            || raw[45] != 1
            || u32_at(&raw, 0) > 1
            || u32_at(&raw, 46) > 1
            || (mint == &p.token1 && raw[44] != 9)
        {
            return Err(err("PUMP_AMM_MINT"));
        }
    }
    if u64_at(&s.data(&p.token0, 82)?, 36) == 0 {
        return Err(err("PUMP_AMM_MINT_SUPPLY"));
    }
    for (id, mint, owner) in [
        (&s.vault, &p.token0, &p.pool_id),
        (&key(&d, 171), &p.token1, &p.pool_id),
        (&s.creator_vault, &p.token1, &authority),
        (&ata(WSOL, &s.fee_recipient)?, &p.token1, &s.fee_recipient),
        (
            &ata(WSOL, &s.buyback_recipient)?,
            &p.token1,
            &s.buyback_recipient,
        ),
    ] {
        let a = s.account(id)?;
        let raw = s.data(id, 165)?;
        if a.owner != TOKEN
            || key(&raw, 0) != *mint
            || key(&raw, 32) != *owner
            || raw[108] != 1
            || u32_at(&raw, 72) != 0
            || u64_at(&raw, 121) != 0
            || u32_at(&raw, 129) != 0
        {
            return Err(err("PUMP_AMM_TOKEN_ACCOUNT"));
        }
        if mint == &p.token1 {
            if u32_at(&raw, 109) != 1
                || a.lamports.checked_sub(u64_at(&raw, 113)) != Some(u64_at(&raw, 64))
            {
                return Err(err("PUMP_AMM_NATIVE_BACKING"));
            }
        } else if u32_at(&raw, 109) != 0 {
            return Err(err("PUMP_AMM_TOKEN_ACCOUNT"));
        }
    }
    if p.reserve0 != Amount(u64_at(&s.data(&s.vault, 165)?, 64).into())
        || p.reserve1 != Amount(u64_at(&s.data(&key(&d, 171), 165)?, 64).into())
    {
        return Err(err("PUMP_AMM_RESERVES"));
    }
    let global_volume = s.data(&gv, 640)?;
    let user_volume = s.data(&uv, 137)?;
    if global_volume[..8] != discriminator("GlobalVolumeAccumulator")
        || global_volume[8..24].iter().any(|b| *b != 0)
        || global_volume[32..].iter().any(|b| *b != 0)
        || user_volume[..8] != discriminator("UserVolumeAccumulator")
        || key(&user_volume, 8) != s.user
        || user_volume[40..].iter().any(|b| *b != 0)
    {
        return Err(err("PUMP_AMM_VOLUME_POLICY"));
    }
    fee_rates(p, s)?;
    Ok(())
}
pub fn fee_rates(p: &Pool, s: &PumpState) -> Result<(u64, u64, u64)> {
    let raw = s.data(&fee_id()?, 4097)?;
    if raw[..8] != discriminator("FeeConfig") {
        return Err(err("PUMP_AMM_FEE_LAYOUT"));
    }
    if p.reserve0 == Amount::zero() {
        return Err(err("PUMP_AMM_EMPTY"));
    }
    let market = u128::from(u64_at(&s.data(&p.token0, 82)?, 36))
        * u128::from(
            u64_at(&s.data(&ata(WSOL, &p.pool_id)?, 165)?, 64)
                .checked_add(virtual_quote(s, p)?)
                .ok_or_else(|| err("PUMP_AMM_VIRTUAL_QUOTE"))?,
        )
        / u128::from(u64_at(&s.data(&s.vault, 165)?, 64));
    let mut o = 65;
    let mut selected = None;
    for schedule in 0..2 {
        let count = u32_at(&raw, o) as usize;
        o += 4;
        if count == 0 || count > 50 || o + count * 40 + 24 > raw.len() {
            return Err(err("PUMP_AMM_FEE_TIERS"));
        }
        let mut previous = None;
        for _ in 0..count {
            let threshold = u128_at(&raw, o);
            let lp = u64_at(&raw, o + 16);
            let protocol = u64_at(&raw, o + 24);
            let creator = u64_at(&raw, o + 32);
            if previous.is_some_and(|v| v >= threshold)
                || lp
                    .checked_add(protocol)
                    .and_then(|n| n.checked_add(creator))
                    .is_none_or(|n| n >= 10000)
            {
                return Err(err("PUMP_AMM_FEE_TIERS"));
            }
            previous = Some(threshold);
            if schedule == 0 && (selected.is_none() || threshold <= market) {
                selected = Some((lp, protocol, creator));
            }
            o += 40;
        }
    }
    if raw[o + 24..].iter().any(|b| *b != 0) {
        return Err(err("PUMP_AMM_FEE_TRAILING"));
    }
    selected.ok_or_else(|| err("PUMP_AMM_FEE_TIERS"))
}

/// Deterministic destination identity only. A quote still requires the observed
/// target pool and all of its actual accounts in the same committed source bank.
pub fn migration_target(curve: &Pool) -> Result<String> {
    if !crate::pump::supports_curve_profile(&curve.protocol) {
        return Err(err("PUMP_MIGRATION_SOURCE_PROFILE"));
    }
    curve.validate()?;
    let state = curve
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_STATE_REQUIRED"))?;
    if state.data(&curve.pool_id, 166)?[48] != 1 {
        return Err(err("PUMP_MIGRATION_NOT_COMPLETE"));
    }
    let mint = pubkey(&curve.token0)?;
    let creator = pda(&[b"pool-authority", &mint], crate::pump::PROGRAM)?;
    pda(
        &[b"pool", &[0, 0], &pubkey(&creator)?, &mint, &pubkey(WSOL)?],
        PROGRAM,
    )
}

/// Positive pricing inventory is explicitly distinct from spendable quote vaults.
pub fn virtual_quote(s: &PumpState, p: &Pool) -> Result<u64> {
    let d = s.data(&p.pool_id, 287)?;
    u64::try_from(i128::from_le_bytes(d[245..261].try_into().unwrap()))
        .map_err(|_| err("PUMP_AMM_VIRTUAL_QUOTE"))
}
