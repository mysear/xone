use crate::{
    err,
    orca::OrcaWhirlpoolState,
    spl_token_2022::{TransferFee, TransferFeeAccount},
    wire, Pool, Result,
};
use serde::{Deserialize, Serialize};

pub const PROFILE: &str = "orca-whirlpools-adaptive-token2022";
pub const ADAPTER: &str = "orca-whirlpools/cc683ec9-token2022-adaptive-1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrcaToken2022Asset {
    pub mint_data: String,
    pub vault_data: String,
}
impl OrcaToken2022Asset {
    pub fn transfer_fee(&self, epoch: u64) -> Result<TransferFee> {
        TransferFee::from_mint(&decode(&self.mint_data)?, epoch)
    }

    fn validate(&self, epoch: u64, decimals: u32, balance: u64) -> Result<Vec<u8>> {
        let mint = decode(&self.mint_data)?;
        TransferFee::from_mint(&mint, epoch)?;
        let vault = decode(&self.vault_data)?;
        TransferFeeAccount::unpack(&vault)?;
        if u32::from(mint[44]) != decimals
            || u32::from_le_bytes(mint[..4].try_into().unwrap()) > 1
            || u32::from_le_bytes(mint[46..50].try_into().unwrap()) > 1
            || u64::from_le_bytes(vault[64..72].try_into().unwrap()) != balance
            || vault[72..76] != [0; 4]
            || vault[108] != 1
            || vault[109..113] != [0; 4]
            || vault[121..133] != [0; 12]
        {
            return Err(err("ORCA_TOKEN2022_ACCOUNT_STATE"));
        }
        Ok(vault)
    }

    pub fn update_vault(&mut self, balance: u64, withheld_increment: u64) -> Result<()> {
        let mut vault = decode(&self.vault_data)?;
        TransferFeeAccount::increment_withheld(&mut vault, withheld_increment)?;
        vault[64..72].copy_from_slice(&balance.to_le_bytes());
        self.vault_data = hex::encode(vault);
        Ok(())
    }

    fn to_proto(&self) -> Result<wire::OrcaToken2022Asset> {
        Ok(wire::OrcaToken2022Asset {
            mint_data: decode(&self.mint_data)?,
            vault_data: decode(&self.vault_data)?,
        })
    }
    fn from_proto(value: wire::OrcaToken2022Asset) -> Self {
        Self {
            mint_data: hex::encode(value.mint_data),
            vault_data: hex::encode(value.vault_data),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrcaToken2022State {
    pub programdata: String,
    pub deployed_slot: u64,
    pub program_code_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token0: Option<OrcaToken2022Asset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token1: Option<OrcaToken2022Asset>,
}
impl OrcaToken2022State {
    pub fn validate(&self, state: &OrcaWhirlpoolState, pool: Option<&Pool>) -> Result<()> {
        if self.programdata != crate::spl_token_2022::PROGRAMDATA
            || self.deployed_slot == 0
            || self.deployed_slot > state.bank_slot
            || self.token0.is_none() && self.token1.is_none()
            || decode(&self.program_code_sha256)?.len() != 32
        {
            return Err(err("ORCA_TOKEN2022_DEPLOYMENT"));
        }
        for (asset, decimals, balance, mint) in [
            (
                self.token0.as_ref(),
                state.mint0_decimals,
                state.raw_vault0,
                pool.map(|p| &p.token0),
            ),
            (
                self.token1.as_ref(),
                state.mint1_decimals,
                state.raw_vault1,
                pool.map(|p| &p.token1),
            ),
        ] {
            let Some(asset) = asset else { continue };
            let vault = asset.validate(state.bank_epoch, decimals, balance)?;
            if let (Some(mint), Some(pool)) = (mint, pool) {
                if vault[..32] != crate::raydium::validate_pubkey(mint)?
                    || vault[32..64] != crate::raydium::validate_pubkey(&pool.pool_id)?
                {
                    return Err(err("ORCA_TOKEN2022_VAULT_BINDING"));
                }
            }
        }
        Ok(())
    }

    pub fn to_proto(&self) -> Result<wire::OrcaToken2022State> {
        Ok(wire::OrcaToken2022State {
            programdata: self.programdata.clone(),
            deployed_slot: self.deployed_slot,
            program_code_sha256: decode(&self.program_code_sha256)?,
            token0: self
                .token0
                .as_ref()
                .map(OrcaToken2022Asset::to_proto)
                .transpose()?,
            token1: self
                .token1
                .as_ref()
                .map(OrcaToken2022Asset::to_proto)
                .transpose()?,
        })
    }
    pub fn from_proto(value: wire::OrcaToken2022State) -> Self {
        Self {
            programdata: value.programdata,
            deployed_slot: value.deployed_slot,
            program_code_sha256: hex::encode(value.program_code_sha256),
            token0: value.token0.map(OrcaToken2022Asset::from_proto),
            token1: value.token1.map(OrcaToken2022Asset::from_proto),
        }
    }
}

fn decode(value: &str) -> Result<Vec<u8>> {
    if value.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err(err("ORCA_TOKEN2022_ENCODING"));
    }
    hex::decode(value).map_err(|_| err("ORCA_TOKEN2022_ENCODING"))
}
