use super::{LfjConfig, PoolConfig};
use xone_domain::{err, lfj::LfjState, Amount, Pool, Result};
impl LfjConfig {
    pub(super) fn validate(&self, p: &PoolConfig, chain: &str) -> Result<()> {
        if !chain.starts_with("evm:")
            || !p.program_id.is_empty()
            || !p.accounts.is_empty()
            || self.token_policy != "standard"
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
            return Err(err("LFJ_CONFIG"));
        }
        let state = LfjState {
            code_sha256: self.expected_code_sha256.clone(),
            implementation: self.expected_implementation.clone(),
            implementation_code_sha256: self.expected_implementation_code_sha256.clone(),
            factory: self.expected_factory.clone(),
            factory_code_sha256: self.expected_factory_code_sha256.clone(),
            token0_code_sha256: self.expected_token0_code_sha256.clone(),
            token1_code_sha256: self.expected_token1_code_sha256.clone(),
            block_hash: format!("0x{}", "1".repeat(64)),
            block_number: 1,
            timestamp: 1,
            bin_step: self.bin_step,
            parameters: Amount::zero(),
            raw_reserve0: Amount::zero(),
            raw_reserve1: Amount::zero(),
            protocol_fees0: Amount::zero(),
            protocol_fees1: Amount::zero(),
            bins: vec![],
            oracle_samples: vec![],
            max_bins: self.max_bins,
            max_steps: self.max_steps,
        };
        let pool = Pool {
            meteora: None,
            pump: None,
            pancake_bsc_lm: None,
            pancake_lm: None,
            slipstream_context: None,
            lfj: Some(state),
            infinity_bin: None,
            singleton_advanced: None,
            multi_asset: None,
            weighted: None,
            raydium_clmm: None,
            orca_whirlpool: None,
            raydium_cpmm: None,
            singleton_cl: None,
            stable: None,
            v2: None,
            v3: None,
            chain_key: chain.into(),
            pool_id: p.id.clone(),
            protocol: p.protocol.clone(),
            token0: p.token0.clone(),
            token1: p.token1.clone(),
            reserve0: Amount::zero(),
            reserve1: Amount::zero(),
            fee_numerator: p.fee_numerator,
            fee_denominator: p.fee_denominator,
            adapter_version: xone_domain::lfj::ADAPTER.into(),
            token_behavior: "standard".into(),
        };
        pool.validate()
    }
}
