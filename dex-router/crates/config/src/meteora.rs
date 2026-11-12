use super::{MeteoraConfig, PoolConfig};
use xone_domain::{
    err,
    meteora::{self, pda},
    Result,
};
impl MeteoraConfig {
    pub fn validate(&self, p: &PoolConfig, chain: &str) -> Result<()> {
        if !chain.starts_with("solana:")
            || !meteora::supports_profile(&p.protocol)
            || p.program_id != meteora::PROGRAM
            || p.fee_numerator != 0
            || p.fee_denominator != 1_000_000_000
            || self.expected_program_code_sha256 != meteora::CODE
            || self.expected_deployed_slot == 0
            || self.expected_token_program_code_sha256 != meteora::TOKEN_CODE
            || self.expected_token_program_code_sha256.len() != 64
            || hex::decode(&self.expected_token_program_code_sha256).is_err()
            || !(1..=4096).contains(&self.max_steps)
            || self.bin_array_indices.is_empty()
            || self.bin_array_indices.len() > 32
            || p.accounts.len()
                != 6 + self.bin_array_indices.len() + usize::from(self.bitmap_extension.is_some())
            || p.accounts[0] != p.id
            || p.accounts[3] != p.token0
            || p.accounts[4] != p.token1
        {
            return Err(err("METEORA_CONFIG"));
        }
        if p.pump.is_some()
            || p.pancake_bsc_lm.is_some()
            || p.pancake_lm.is_some()
            || p.lfj.is_some()
            || p.slipstream_context.is_some()
            || p.infinity_bin.is_some()
            || p.singleton_advanced.is_some()
            || p.multi_asset.is_some()
            || p.weighted.is_some()
            || p.raydium_clmm.is_some()
            || p.orca_whirlpool.is_some()
            || p.raydium_cpmm.is_some()
            || p.singleton_cl.is_some()
            || p.stable.is_some()
            || p.v2.is_some()
            || p.v3.is_some()
        {
            return Err(err("METEORA_CONFIG_MODEL"));
        }
        let program = meteora::pubkey(meteora::PROGRAM)?;
        if pda(&[&program], "BPFLoaderUpgradeab1e11111111111111111111111")?
            != self.expected_programdata
        {
            return Err(err("METEORA_PROGRAMDATA"));
        }
        let key = meteora::pubkey(&p.id)?;
        if key.len() != 32 {
            return Err(err("METEORA_CONFIG"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for id in &p.accounts {
            xone_domain::raydium::validate_pubkey(id)?;
            if !ids.insert(id) {
                return Err(err("METEORA_ACCOUNT_ALIAS"));
            }
        }
        if (p.protocol == meteora::BITMAP_PROFILE) != self.bitmap_extension.is_some() {
            return Err(err("METEORA_BITMAP_PROFILE"));
        }
        if let Some(id) = &self.bitmap_extension {
            if id != &pda(&[b"bitmap", &key], meteora::PROGRAM)? || p.accounts.last() != Some(id) {
                return Err(err("METEORA_BITMAP_EXTENSION_IDENTITY"));
            }
        }
        if (p.protocol == xone_domain::meteora_token2022::PROFILE)
            != self.token2022_upgrade.is_some()
        {
            return Err(err("METEORA_TOKEN2022_CONFIG"));
        }
        if let Some(t) = &self.token2022_upgrade {
            if t.deployed_slot == 0
                || t.programdata
                    != pda(
                        &[&meteora::pubkey(xone_domain::meteora_token2022::PROGRAM)?],
                        "BPFLoaderUpgradeab1e11111111111111111111111",
                    )?
            {
                return Err(err("METEORA_TOKEN2022_CONFIG"));
            }
        }
        let mut previous = None;
        for (i, index) in self.bin_array_indices.iter().enumerate() {
            if !(if self.bitmap_extension.is_some() {
                -6656..=6655
            } else {
                -512..=511
            })
            .contains(index)
                || previous.is_some_and(|p| p >= *index)
                || pda(
                    &[b"bin_array", &key, &index.to_le_bytes()],
                    meteora::PROGRAM,
                )? != p.accounts[6 + i]
            {
                return Err(err("METEORA_ARRAY_IDENTITY"));
            }
            previous = Some(*index);
        }
        if let Some(upgrade) = &self.token_program_upgrade {
            if upgrade.programdata != super::CLASSIC_TOKEN_PROGRAMDATA || upgrade.deployed_slot == 0
            {
                return Err(err("TOKEN_PROGRAM_VERSION_UNSUPPORTED"));
            }
        }
        Ok(())
    }
}
