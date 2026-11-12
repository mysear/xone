//! Fixed acquired Pump/Fee programs, classic token and legacy native SOL curves.
//! Native SOL is a distinct graph asset; it is never treated as wrapped SOL.
use crate::{
    err,
    meteora::{bytes, discriminator, key, pda, pubkey, u128_at, u32_at, u64_at},
    wire, Amount, Pool, Result,
};
use serde::{Deserialize, Serialize};
pub const MIGRATION_PROFILE: &str = "pump-bonding-sol-migration";
pub const MIGRATION_ADAPTER: &str = "pump-bonding/7a19affaa2c6-sol-migration-1";
pub fn supports_curve_profile(profile: &str) -> bool {
    matches!(profile, PROFILE | MIGRATION_PROFILE)
}
pub const PROFILE: &str = "pump-bonding-sol-classic";
pub const ADAPTER: &str = "pump-bonding/7a19affaa2c6-sol-1";
pub const PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const FEE_PROGRAM: &str = "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ";
pub const CODE: &str = "7a19affaa2c6187e04a5fe0ea298cee81fa5eb8c24cfcc30d85f615e89f7b02d";
pub const FEE_CODE: &str = "4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e";
pub const NATIVE_SOL: &str = "native:solana:sol";
pub const SYSTEM: &str = "11111111111111111111111111111111";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const ASSOCIATED: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub address: String,
    pub owner: String,
    pub lamports: u64,
    pub data: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PumpState {
    pub programdata: String,
    pub deployed_slot: u64,
    pub program_code_sha256: String,
    pub fee_programdata: String,
    pub fee_deployed_slot: u64,
    pub fee_program_code_sha256: String,
    pub token_program_code_sha256: String,
    pub slot: u64,
    pub timestamp: u64,
    pub user: String,
    pub vault: String,
    pub creator_vault: String,
    pub fee_recipient: String,
    pub buyback_recipient: String,
    pub accounts: Vec<Account>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub derived: bool,
}
impl PumpState {
    pub fn account(&self, address: &str) -> Result<&Account> {
        self.accounts
            .iter()
            .find(|a| a.address == address)
            .ok_or_else(|| err("PUMP_ACCOUNT_MISSING"))
    }
    pub fn account_mut(&mut self, address: &str) -> Result<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.address == address)
            .ok_or_else(|| err("PUMP_ACCOUNT_MISSING"))
    }
    pub fn data(&self, address: &str, size: usize) -> Result<Vec<u8>> {
        bytes(&self.account(address)?.data, size)
    }
    pub fn global_id() -> Result<String> {
        pda(&[b"global"], PROGRAM)
    }
    pub fn fee_id() -> Result<String> {
        pda(&[b"fee_config", &pubkey(PROGRAM)?], FEE_PROGRAM)
    }
    pub fn validate(&self, p: &Pool) -> Result<()> {
        if crate::pump_amm::supports_profile(&p.protocol) {
            return crate::pump_amm::validate(p, self);
        }
        if !supports_curve_profile(&p.protocol)
            || p.adapter_version
                != if p.protocol == MIGRATION_PROFILE {
                    MIGRATION_ADAPTER
                } else {
                    ADAPTER
                }
            || !p.chain_key.starts_with("solana:")
            || p.token_behavior != "standard"
            || p.token1 != NATIVE_SOL
            || p.fee_numerator != 0
            || p.fee_denominator != 10000
        {
            return Err(err("PUMP_PROFILE"));
        }
        let mint = pubkey(&p.token0)?;
        let curve = pubkey(&p.pool_id)?;
        pubkey(&self.user)?;
        if self.program_code_sha256 != CODE
            || self.fee_program_code_sha256 != FEE_CODE
            || self.token_program_code_sha256 != crate::meteora::TOKEN_CODE
            || self.deployed_slot == 0
            || self.deployed_slot > self.slot
            || self.fee_deployed_slot == 0
            || self.fee_deployed_slot > self.slot
            || self.timestamp > i64::MAX as u64
        {
            return Err(err("PUMP_PROGRAM_VERSION"));
        }
        let loader = "BPFLoaderUpgradeab1e11111111111111111111111";
        if pda(&[&pubkey(PROGRAM)?], loader)? != self.programdata
            || pda(&[&pubkey(FEE_PROGRAM)?], loader)? != self.fee_programdata
            || pda(&[b"bonding-curve", &mint], PROGRAM)? != p.pool_id
            || pda(&[&curve, &pubkey(TOKEN)?, &mint], ASSOCIATED)? != self.vault
        {
            return Err(err("PUMP_PDA"));
        }
        if self.accounts.len() != 10
            || self
                .accounts
                .windows(2)
                .any(|a| a[0].address >= a[1].address)
        {
            return Err(err("PUMP_ACCOUNT_SET"));
        }
        let global = Self::global_id()?;
        let fees = Self::fee_id()?;
        let user_volume = pda(&[b"user_volume_accumulator", &pubkey(&self.user)?], PROGRAM)?;
        let ids = [
            p.pool_id.clone(),
            p.token0.clone(),
            self.vault.clone(),
            self.creator_vault.clone(),
            self.fee_recipient.clone(),
            self.buyback_recipient.clone(),
            global.clone(),
            fees.clone(),
            user_volume.clone(),
            pda(&[b"global_volume_accumulator"], PROGRAM)?,
        ];
        if ids.iter().collect::<std::collections::BTreeSet<_>>().len() != 10
            || self.accounts.iter().any(|a| !ids.contains(&a.address))
        {
            return Err(err("PUMP_ACCOUNT_IDENTITY"));
        }
        for a in &self.accounts {
            pubkey(&a.address)?;
            if a.owner != SYSTEM {
                pubkey(&a.owner)?;
            }
        }
        let d = self.data(&p.pool_id, 166)?;
        let g = self.data(&global, 1088)?;
        let m = self.data(&p.token0, 82)?;
        let v = self.data(&self.vault, 165)?;
        if d[..8] != discriminator("BondingCurve")
            || g[..8] != discriminator("Global")
            || g[8] != 1
            || d[48] > 1
            || d[81..83] != [0, 0]
            || d[83..115].iter().any(|b| *b != 0)
            || d[115..142].iter().any(|b| *b != 0)
            || d[150..166].iter().any(|b| *b != 0)
            || u64_at(&d, 142) != u64_at(&g, 81)
        {
            return Err(err("PUMP_CURVE_POLICY"));
        }
        let creator = pubkey(&key(&d, 49))?;
        if pda(&[b"creator-vault", &creator], PROGRAM)? != self.creator_vault
            || key(&g, 41) != self.fee_recipient
            || !(0..8).any(|i| key(&g, 741 + i * 32) == self.buyback_recipient)
            || u64_at(&g, 997) > 10000
        {
            return Err(err("PUMP_FEE_IDENTITY"));
        }
        for id in [&p.pool_id, &global] {
            if self.account(id)?.owner != PROGRAM {
                return Err(err("PUMP_ACCOUNT_OWNER"));
            }
        }
        if self.account(&fees)?.owner != FEE_PROGRAM
            || self.account(&p.token0)?.owner != TOKEN
            || self.account(&self.vault)?.owner != TOKEN
        {
            return Err(err("PUMP_ACCOUNT_OWNER"));
        }
        for id in [
            &self.creator_vault,
            &self.fee_recipient,
            &self.buyback_recipient,
        ] {
            let a = self.account(id)?;
            if a.owner != SYSTEM || !a.data.is_empty() {
                return Err(err("PUMP_FUND_ACCOUNT"));
            }
        }
        if m[45] != 1
            || m[44] != 6
            || u32_at(&m, 0) > 1
            || u32_at(&m, 46) > 1
            || u64_at(&m, 36) != 1_000_000_000_000_000
            || u64_at(&d, 40) != u64_at(&m, 36)
            || v[108] != 1
            || key(&v, 0) != p.token0
            || key(&v, 32) != p.pool_id
            || u32_at(&v, 72) != 0
            || u32_at(&v, 109) != 0
            || u64_at(&v, 121) != 0
            || u32_at(&v, 129) != 0
        {
            return Err(err("PUMP_TOKEN_POLICY"));
        }
        let migrated = p.protocol == MIGRATION_PROFILE
            && d[48] == 1
            && [8, 16, 24, 32].into_iter().all(|o| u64_at(&d, o) == 0);
        let reserved = if p.protocol == MIGRATION_PROFILE && !migrated {
            u64_at(&m, 36)
                .checked_sub(u64_at(&g, 89))
                .ok_or_else(|| err("PUMP_RESERVE_ACCOUNTING"))?
        } else {
            0
        };
        if u64_at(&v, 64)
            != u64_at(&d, 24)
                .checked_add(reserved)
                .ok_or_else(|| err("PUMP_RESERVE_ACCOUNTING"))?
            || p.reserve0 != Amount(u64_at(&d, 24).into())
            || p.reserve1 != Amount(u64_at(&d, 32).into())
            || self.account(&p.pool_id)?.lamports.checked_sub(2_046_240) != Some(u64_at(&d, 32))
            || d[48] != u8::from(u64_at(&d, 24) == 0)
            || (!migrated
                && (u64_at(&d, 8).checked_sub(u64_at(&d, 24))
                    != u64_at(&g, 73).checked_sub(u64_at(&g, 89))
                    || u64_at(&d, 16).checked_sub(u64_at(&d, 32)) != Some(u64_at(&g, 81))))
        {
            return Err(err("PUMP_RESERVE_ACCOUNTING"));
        }
        let uv = self.data(&user_volume, 137)?;
        let gv = self.data(&ids[9], 640)?;
        if self.account(&user_volume)?.owner != PROGRAM
            || self.account(&ids[9])?.owner != PROGRAM
            || uv[..8] != discriminator("UserVolumeAccumulator")
            || key(&uv, 8) != self.user
            || uv[40..].iter().any(|b| *b != 0)
            || gv[..8] != discriminator("GlobalVolumeAccumulator")
            || gv[8..24].iter().any(|b| *b != 0)
        {
            return Err(err("PUMP_VOLUME_POLICY"));
        }
        if (d[48] != 0) != (u64_at(&d, 24) == 0)
            || (!migrated
                && (u64_at(&d, 8).checked_sub(u64_at(&d, 24))
                    != u64_at(&g, 73).checked_sub(u64_at(&g, 89))
                    || u64_at(&d, 16).checked_sub(u64_at(&d, 32)) != Some(u64_at(&g, 81))))
        {
            return Err(err("PUMP_CURVE_TRANSITION"));
        }
        self.fee_rates(p)?;
        if [&self.user].into_iter().any(|a| {
            ids.contains(a)
                || *a == self.programdata
                || *a == self.fee_programdata
                || *a == PROGRAM
                || *a == FEE_PROGRAM
                || *a == user_volume
        }) {
            return Err(err("PUMP_USER_IDENTITY"));
        }
        Ok(())
    }
    /// Fixed SOL market-cap fee schedule, selected from the complete original bytes.
    pub fn fee_rates(&self, p: &Pool) -> Result<(u64, u64)> {
        let id = Self::fee_id()?;
        let a = self.account(&id)?;
        let raw = bytes(&a.data, 4097)?;
        if raw[..8] != discriminator("FeeConfig") {
            return Err(err("PUMP_FEE_LAYOUT"));
        }
        let mut o = 65;
        let mut selected = None;
        let d = self.data(&p.pool_id, 166)?;
        let market = u128::from(1_000_000_000_000_000u64) * u128::from(u64_at(&d, 16))
            / u128::from(u64_at(&d, 8).max(1));
        for schedule in 0..2 {
            let count = u32_at(&raw, o) as usize;
            o += 4;
            if count == 0 || count > 50 || o + count * 40 + 24 > raw.len() {
                return Err(err("PUMP_FEE_TIERS"));
            }
            let mut previous = None;
            for _ in 0..count {
                let threshold = u128_at(&raw, o);
                let lp = u64_at(&raw, o + 16);
                let protocol = u64_at(&raw, o + 24);
                let creator = u64_at(&raw, o + 32);
                if previous.is_some_and(|x| x >= threshold)
                    || lp != 0
                    || protocol.checked_add(creator).is_none_or(|n| n >= 10000)
                {
                    return Err(err("PUMP_FEE_TIERS"));
                }
                previous = Some(threshold);
                if schedule == 0 && (selected.is_none() || threshold <= market) {
                    selected = Some((protocol, creator));
                }
                o += 40;
            }
        }
        if raw[o + 24..].iter().any(|b| *b != 0) {
            return Err(err("PUMP_FEE_TRAILING"));
        }
        selected.ok_or_else(|| err("PUMP_FEE_TIERS"))
    }
    pub fn to_proto(&self) -> Result<wire::PumpState> {
        Ok(wire::PumpState {
            programdata: self.programdata.clone(),
            deployed_slot: self.deployed_slot,
            program_code_sha256: hex::decode(&self.program_code_sha256)
                .map_err(|_| err("PUMP_CODE"))?,
            fee_programdata: self.fee_programdata.clone(),
            fee_deployed_slot: self.fee_deployed_slot,
            fee_program_code_sha256: hex::decode(&self.fee_program_code_sha256)
                .map_err(|_| err("PUMP_CODE"))?,
            token_program_code_sha256: hex::decode(&self.token_program_code_sha256)
                .map_err(|_| err("PUMP_CODE"))?,
            slot: self.slot,
            timestamp: self.timestamp,
            user: self.user.clone(),
            vault: self.vault.clone(),
            creator_vault: self.creator_vault.clone(),
            fee_recipient: self.fee_recipient.clone(),
            buyback_recipient: self.buyback_recipient.clone(),
            accounts: self
                .accounts
                .iter()
                .map(|a| {
                    Ok(wire::PumpAccount {
                        address: a.address.clone(),
                        owner: a.owner.clone(),
                        lamports: a.lamports,
                        data: bytes(&a.data, a.data.len() / 2)?,
                    })
                })
                .collect::<Result<_>>()?,
            derived: self.derived,
        })
    }
    pub fn from_proto(s: wire::PumpState) -> Result<Self> {
        Ok(Self {
            programdata: s.programdata,
            deployed_slot: s.deployed_slot,
            program_code_sha256: hex::encode(s.program_code_sha256),
            fee_programdata: s.fee_programdata,
            fee_deployed_slot: s.fee_deployed_slot,
            fee_program_code_sha256: hex::encode(s.fee_program_code_sha256),
            token_program_code_sha256: hex::encode(s.token_program_code_sha256),
            slot: s.slot,
            timestamp: s.timestamp,
            user: s.user,
            vault: s.vault,
            creator_vault: s.creator_vault,
            fee_recipient: s.fee_recipient,
            buyback_recipient: s.buyback_recipient,
            accounts: s
                .accounts
                .into_iter()
                .map(|a| Account {
                    address: a.address,
                    owner: a.owner,
                    lamports: a.lamports,
                    data: hex::encode(a.data),
                })
                .collect(),
            derived: s.derived,
        })
    }
}

/// Shared immutable observations must agree. Swaps also mutate fee lamports;
/// those shared balances need an explicit joint ledger before routing both pools.
pub fn validate_dependencies<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let mut observed = std::collections::BTreeMap::new();
    for p in pools {
        if let Some(s) = &p.pump {
            for a in &s.accounts {
                if observed
                    .insert(&a.address, a)
                    .is_some_and(|previous| previous != a)
                {
                    return Err(err("PUMP_SHARED_ACCOUNT_CONFLICT"));
                }
            }
        }
    }
    Ok(())
}
pub fn blocked_pools<'a>(
    pools: impl IntoIterator<Item = &'a Pool>,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut owners =
        std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
    for p in pools {
        if let Some(s) = &p.pump {
            p.validate()?;
            if supports_curve_profile(&p.protocol) && s.data(&p.pool_id, 166)?[48] != 0 {
                continue;
            }
            let mut ids = vec![p.pool_id.clone(), s.vault.clone(), s.creator_vault.clone()];
            if crate::pump_amm::supports_profile(&p.protocol) {
                let d = s.data(&p.pool_id, 287)?;
                ids.extend([
                    key(&d, 171),
                    crate::pump_amm::ata(crate::pump_amm::WSOL, &s.fee_recipient)?,
                    crate::pump_amm::ata(crate::pump_amm::WSOL, &s.buyback_recipient)?,
                ]);
            } else {
                ids.extend([s.fee_recipient.clone(), s.buyback_recipient.clone()]);
            }
            for id in ids {
                owners.entry(id).or_default().insert(p.pool_id.clone());
            }
        }
    }
    let mut blocked = std::collections::BTreeMap::new();
    for ids in owners.values().filter(|ids| ids.len() > 1) {
        for id in ids {
            blocked.insert(id.clone(), "PUMP_SHARED_FUND_ACCOUNT_NOT_MODELED".into());
        }
    }
    Ok(blocked)
}
