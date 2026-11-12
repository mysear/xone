use super::{PoolConfig, PumpConfig};
use xone_domain::{
    err,
    meteora::{pda, pubkey},
    pump, Result,
};
impl PumpConfig {
    pub fn validate(&self, p: &PoolConfig, chain: &str) -> Result<()> {
        let amm = xone_domain::pump_amm::supports_profile(&p.protocol);
        let program = if amm {
            xone_domain::pump_amm::PROGRAM
        } else {
            pump::PROGRAM
        };
        if !chain.starts_with("solana:")
            || !matches!(
                p.protocol.as_str(),
                pump::PROFILE
                    | pump::MIGRATION_PROFILE
                    | xone_domain::pump_amm::PROFILE
                    | xone_domain::pump_amm::BOOST_PROFILE
            )
            || p.program_id != program
            || p.token1
                != if amm {
                    xone_domain::pump_amm::WSOL
                } else {
                    pump::NATIVE_SOL
                }
            || p.fee_numerator != 0
            || p.fee_denominator != 10000
            || self.expected_program_code_sha256
                != if amm {
                    xone_domain::pump_amm::CODE
                } else {
                    pump::CODE
                }
            || self.expected_fee_program_code_sha256 != pump::FEE_CODE
            || self.expected_token_program_code_sha256 != xone_domain::meteora::TOKEN_CODE
            || self.expected_deployed_slot == 0
            || self.expected_fee_deployed_slot == 0
            || p.accounts.len() != if amm { 12 } else { 10 }
        {
            return Err(err("PUMP_CONFIG"));
        }
        if p.meteora.is_some()
            || p.lfj.is_some()
            || p.pancake_lm.is_some()
            || p.pancake_bsc_lm.is_some()
            || p.slipstream_context.is_some()
            || p.multi_asset.is_some()
            || p.v2.is_some()
            || p.v3.is_some()
            || p.stable.is_some()
            || p.weighted.is_some()
            || p.singleton_cl.is_some()
            || p.singleton_advanced.is_some()
            || p.infinity_bin.is_some()
            || p.orca_whirlpool.is_some()
            || p.raydium_cpmm.is_some()
            || p.raydium_clmm.is_some()
        {
            return Err(err("PUMP_CONFIG_MODEL"));
        }
        pubkey(&self.user)?;
        let mint = pubkey(&p.token0)?;
        if !amm && pda(&[b"bonding-curve", &mint], pump::PROGRAM)? != p.id {
            return Err(err("PUMP_PDA"));
        }
        if amm {
            let creator = pda(&[b"pool-authority", &mint], pump::PROGRAM)?;
            if pda(
                &[
                    b"pool",
                    &[0, 0],
                    &pubkey(&creator)?,
                    &mint,
                    &pubkey(&p.token1)?,
                ],
                program,
            )? != p.id
            {
                return Err(err("PUMP_AMM_IDENTITY"));
            }
        }
        for (program, data) in [
            (program, &self.expected_programdata),
            (pump::FEE_PROGRAM, &self.expected_fee_programdata),
        ] {
            if pda(
                &[&pubkey(program)?],
                "BPFLoaderUpgradeab1e11111111111111111111111",
            )? != *data
            {
                return Err(err("PUMP_PROGRAMDATA"));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for id in &p.accounts {
            pubkey(id)?;
            if !seen.insert(id) {
                return Err(err("PUMP_ACCOUNT_ALIAS"));
            }
        }
        if !p.accounts.contains(&p.id) || !p.accounts.contains(&p.token0) {
            return Err(err("PUMP_CONFIG_ACCOUNTS"));
        }
        if let Some(t) = &self.token_program_upgrade {
            if t.programdata != super::CLASSIC_TOKEN_PROGRAMDATA || t.deployed_slot == 0 {
                return Err(err("TOKEN_PROGRAM_VERSION_UNSUPPORTED"));
            }
        }
        Ok(())
    }
}
