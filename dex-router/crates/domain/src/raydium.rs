use crate::{err, wire, Amount, Pool, Result};
use serde::{Deserialize, Serialize};
pub const PROGRAM_ID: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
pub const PROGRAM_DATA: &str = "DMawCQzbgNTmbzaESc7o6pvL1KAeetY8zA7jNpzntHhU";
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
pub fn validate_pubkey(s: &str) -> Result<[u8; 32]> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|_| err("SOLANA_PUBKEY_IDENTITY"))?;
    let out: [u8; 32] = bytes
        .try_into()
        .map_err(|_| err("SOLANA_PUBKEY_IDENTITY"))?;
    if out == [0u8; 32] || bs58::encode(out).into_string() != s {
        return Err(err("SOLANA_PUBKEY_IDENTITY"));
    }
    Ok(out)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCPMMState {
    pub program_id: String,
    pub programdata: String,
    pub program_code_sha256: String,
    pub deployed_slot: u64,
    pub token_program_code_sha256: String,
    pub authority: String,
    pub authority_bump: u32,
    pub amm_config: String,
    pub vault0: String,
    pub vault1: String,
    pub raw_vault0: u64,
    pub raw_vault1: u64,
    pub protocol_fee_rate: u64,
    pub fund_fee_rate: u64,
    pub creator_fee_rate: u64,
    pub creator_fee_share_rate: u64,
    pub creator_fee_on: u32,
    pub creator_fee_enabled: bool,
    pub protocol_fees0: u64,
    pub protocol_fees1: u64,
    pub fund_fees0: u64,
    pub fund_fees1: u64,
    pub creator_fees0: u64,
    pub creator_fees1: u64,
    pub bank_slot: u64,
    pub bank_unix_timestamp: u64,
    pub bank_epoch: u64,
    pub open_time: u64,
    pub recent_epoch: u64,
    pub mint0_decimals: u32,
    pub mint1_decimals: u32,
}
impl RaydiumCPMMState {
    pub fn net_reserves(&self) -> Result<(u64, u64)> {
        let net = |raw: u64, p: u64, f: u64, c: u64| -> Result<u64> {
            let fee = p
                .checked_add(f)
                .and_then(|x| x.checked_add(c))
                .ok_or_else(|| err("RAYDIUM_FEE_OVERFLOW"))?;
            raw.checked_sub(fee)
                .ok_or_else(|| err("RAYDIUM_RESERVE_UNDERFLOW"))
        };
        Ok((
            net(
                self.raw_vault0,
                self.protocol_fees0,
                self.fund_fees0,
                self.creator_fees0,
            )?,
            net(
                self.raw_vault1,
                self.protocol_fees1,
                self.fund_fees1,
                self.creator_fees1,
            )?,
        ))
    }
    fn validate_wire(&self) -> Result<()> {
        if self.program_id != PROGRAM_ID
            || self.programdata != PROGRAM_DATA
            || self.recent_epoch > self.bank_epoch
            || (self.authority != "GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL"
                || self.authority_bump != 253)
            || self.creator_fee_on > 2
            || self.mint0_decimals > 255
            || self.mint1_decimals > 255
            || self.deployed_slot > self.bank_slot
            || self.bank_unix_timestamp > i64::MAX as u64
            || self.open_time > self.bank_unix_timestamp
        {
            return Err(err("RAYDIUM_VERSION_OR_BANK"));
        }
        for key in [
            &self.programdata,
            &self.authority,
            &self.amm_config,
            &self.vault0,
            &self.vault1,
        ] {
            validate_pubkey(key)?;
        }
        for code in [&self.program_code_sha256, &self.token_program_code_sha256] {
            if code.len() != 64 || *code != code.to_ascii_lowercase() || hex::decode(code).is_err()
            {
                return Err(err("RAYDIUM_CODE_IDENTITY"));
            }
        }
        if self.protocol_fee_rate > 1000000
            || self.fund_fee_rate > 1000000
            || self.protocol_fee_rate + self.fund_fee_rate > 1000000
            || self.creator_fee_rate > 1000000
            || self.creator_fee_share_rate > 1000000
        {
            return Err(err("RAYDIUM_FEE_RANGE"));
        }
        self.net_reserves()?;
        Ok(())
    }
    pub fn validate(&self, p: &Pool) -> Result<()> {
        self.validate_wire()?;
        if !p.chain_key.starts_with("solana:")
            || crate::validate_chain(&p.chain_key).is_err()
            || p.protocol != "raydium-cpmm-creator"
            || p.adapter_version != "raydium-cpmm/f4b9b263-spl-1"
            || p.token_behavior != "standard"
            || p.fee_denominator != 1000000
            || p.fee_numerator >= 1000000
            || p.v2.is_some()
            || p.v3.is_some()
            || p.stable.is_some()
            || p.singleton_cl.is_some()
        {
            return Err(err("RAYDIUM_POOL_IDENTITY"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in [
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.program_id,
            &self.programdata,
            &self.authority,
            &self.amm_config,
            &self.vault0,
            &self.vault1,
            TOKEN_PROGRAM,
            CLOCK,
        ] {
            validate_pubkey(key)?;
            if !seen.insert(key) {
                return Err(err("RAYDIUM_ACCOUNT_ALIAS"));
            }
        }
        if validate_pubkey(&p.token0)? >= validate_pubkey(&p.token1)? {
            return Err(err("RAYDIUM_MINT_ORDER"));
        }
        let (a, b) = self.net_reserves()?;
        if p.reserve0 != Amount::checked(a.into())? || p.reserve1 != Amount::checked(b.into())? {
            return Err(err("RAYDIUM_RESERVE_MISMATCH"));
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::RaydiumCpmmState> {
        self.validate_wire()?;
        Ok(wire::RaydiumCpmmState {
            program_id: self.program_id.clone(),
            programdata: self.programdata.clone(),
            program_code_sha256: hex::decode(&self.program_code_sha256)
                .map_err(|_| err("RAYDIUM_DIGEST"))?,
            deployed_slot: self.deployed_slot,
            token_program_code_sha256: hex::decode(&self.token_program_code_sha256)
                .map_err(|_| err("RAYDIUM_DIGEST"))?,
            authority: self.authority.clone(),
            authority_bump: self.authority_bump,
            amm_config: self.amm_config.clone(),
            vault0: self.vault0.clone(),
            vault1: self.vault1.clone(),
            raw_vault0: self.raw_vault0,
            raw_vault1: self.raw_vault1,
            protocol_fee_rate: self.protocol_fee_rate,
            fund_fee_rate: self.fund_fee_rate,
            creator_fee_rate: self.creator_fee_rate,
            creator_fee_share_rate: self.creator_fee_share_rate,
            creator_fee_on: self.creator_fee_on,
            creator_fee_enabled: self.creator_fee_enabled,
            protocol_fees0: self.protocol_fees0,
            protocol_fees1: self.protocol_fees1,
            fund_fees0: self.fund_fees0,
            fund_fees1: self.fund_fees1,
            creator_fees0: self.creator_fees0,
            creator_fees1: self.creator_fees1,
            bank_slot: self.bank_slot,
            bank_unix_timestamp: self.bank_unix_timestamp,
            bank_epoch: self.bank_epoch,
            open_time: self.open_time,
            recent_epoch: self.recent_epoch,
            mint0_decimals: self.mint0_decimals,
            mint1_decimals: self.mint1_decimals,
        })
    }
    pub fn from_proto(s: wire::RaydiumCpmmState) -> Result<Self> {
        if s.program_code_sha256.len() != 32 || s.token_program_code_sha256.len() != 32 {
            return Err(err("RAYDIUM_DIGEST_WIDTH"));
        };
        let out = Self {
            program_id: s.program_id,
            programdata: s.programdata,
            program_code_sha256: hex::encode(s.program_code_sha256),
            deployed_slot: s.deployed_slot,
            token_program_code_sha256: hex::encode(s.token_program_code_sha256),
            authority: s.authority,
            authority_bump: s.authority_bump,
            amm_config: s.amm_config,
            vault0: s.vault0,
            vault1: s.vault1,
            raw_vault0: s.raw_vault0,
            raw_vault1: s.raw_vault1,
            protocol_fee_rate: s.protocol_fee_rate,
            fund_fee_rate: s.fund_fee_rate,
            creator_fee_rate: s.creator_fee_rate,
            creator_fee_share_rate: s.creator_fee_share_rate,
            creator_fee_on: s.creator_fee_on,
            creator_fee_enabled: s.creator_fee_enabled,
            protocol_fees0: s.protocol_fees0,
            protocol_fees1: s.protocol_fees1,
            fund_fees0: s.fund_fees0,
            fund_fees1: s.fund_fees1,
            creator_fees0: s.creator_fees0,
            creator_fees1: s.creator_fees1,
            bank_slot: s.bank_slot,
            bank_unix_timestamp: s.bank_unix_timestamp,
            bank_epoch: s.bank_epoch,
            open_time: s.open_time,
            recent_epoch: s.recent_epoch,
            mint0_decimals: s.mint0_decimals,
            mint1_decimals: s.mint1_decimals,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
