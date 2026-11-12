//! Fixed Token-2022 executable, TransferFeeConfig and TransferFeeAmount only.
use crate::{
    err,
    meteora::{bytes, pda, pubkey, u16_at, u64_at, MeteoraState},
    wire, Result,
};
use serde::{Deserialize, Serialize};
pub const PROFILE: &str = "meteora-dlmm-token2022";
pub const ADAPTER: &str = "meteora-dlmm/1532e540-token2022-fee-1";
pub const PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const CODE: &str = "495e9d7680dd555cb126a6a8e5464af5be9b01f02f2cd70634352722d22e3cad";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Token2022State {
    pub programdata: String,
    pub deployed_slot: u64,
    pub program_code_sha256: String,
    pub bank_epoch: u64,
    pub mint_data: String,
    pub vault_data: String,
}
impl Token2022State {
    pub fn validate(&self, s: &MeteoraState) -> Result<()> {
        if self.program_code_sha256 != CODE
            || self.deployed_slot == 0
            || self.deployed_slot > s.slot
            || self.programdata
                != pda(
                    &[&pubkey(PROGRAM)?],
                    "BPFLoaderUpgradeab1e11111111111111111111111",
                )?
        {
            return Err(err("METEORA_TOKEN2022_VERSION"));
        }
        let m = bytes(&self.mint_data, 278)?;
        let v = bytes(&self.vault_data, 178)?;
        if m[45] != 1
            || m[82..165].iter().any(|b| *b != 0)
            || m[165] != 1
            || u16_at(&m, 166) != 1
            || u16_at(&m, 168) != 108
            || u16_at(&m, 258) > 10000
            || u16_at(&m, 276) > 10000
            || u64_at(&m, 242) > u64_at(&m, 260)
            || v[165] != 2
            || u16_at(&v, 166) != 2
            || u16_at(&v, 168) != 8
            || u64_at(&v, 64) != s.raw_vault0
        {
            return Err(err("METEORA_TOKEN2022_TLV"));
        }
        Ok(())
    }
    pub fn fee(&self, amount: u64) -> Result<u64> {
        let m = bytes(&self.mint_data, 278)?;
        let o = if self.bank_epoch >= u64_at(&m, 260) {
            260
        } else {
            242
        };
        let bps = u16_at(&m, o + 16);
        let cap = u64_at(&m, o + 8);
        u64::try_from(
            (u128::from(amount) * u128::from(bps))
                .div_ceil(10000)
                .min(u128::from(cap)),
        )
        .map_err(|_| err("METEORA_TOKEN2022_FEE"))
    }
    pub fn gross(&self, net: u64) -> Result<u64> {
        let m = bytes(&self.mint_data, 278)?;
        let o = if self.bank_epoch >= u64_at(&m, 260) {
            260
        } else {
            242
        };
        let bps = u16_at(&m, o + 16);
        let cap = u64_at(&m, o + 8);
        let fee = if bps == 10000 {
            u128::from(cap)
        } else {
            (u128::from(net) * u128::from(bps))
                .div_ceil(10000 - u128::from(bps))
                .min(u128::from(cap))
        };
        let gross =
            u64::try_from(u128::from(net) + fee).map_err(|_| err("METEORA_TOKEN2022_FEE"))?;
        if gross.checked_sub(self.fee(gross)?) != Some(net) {
            return Err(err("METEORA_TOKEN2022_FEE_INVERSE"));
        }
        Ok(gross)
    }
    pub fn to_proto(&self) -> Result<wire::MeteoraToken2022State> {
        Ok(wire::MeteoraToken2022State {
            programdata: self.programdata.clone(),
            deployed_slot: self.deployed_slot,
            program_code_sha256: hex::decode(&self.program_code_sha256)
                .map_err(|_| err("METEORA_TOKEN2022_VERSION"))?,
            bank_epoch: self.bank_epoch,
            mint_data: bytes(&self.mint_data, 278)?,
            vault_data: bytes(&self.vault_data, 178)?,
        })
    }
    pub fn from_proto(s: wire::MeteoraToken2022State) -> Self {
        Self {
            programdata: s.programdata,
            deployed_slot: s.deployed_slot,
            program_code_sha256: hex::encode(s.program_code_sha256),
            bank_epoch: s.bank_epoch,
            mint_data: hex::encode(s.mint_data),
            vault_data: hex::encode(s.vault_data),
        }
    }
}
