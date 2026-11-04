use crate::{err, Pool, Result};
impl Pool {
    pub(crate) fn validate_singleton_advanced(&self) -> Result<()> {
        let s = self
            .singleton_advanced
            .as_ref()
            .ok_or_else(|| err("SINGLETON_ADVANCED_STATE_REQUIRED"))?;
        if self.multi_asset.is_some()
            || self.weighted.is_some()
            || self.singleton_cl.is_some()
            || self.stable.is_some()
            || self.v2.is_some()
            || self.v3.is_some()
            || self.orca_whirlpool.is_some()
            || self.raydium_clmm.is_some()
            || self.raydium_cpmm.is_some()
        {
            return Err(err("SINGLETON_ADVANCED_STATE_REQUIRED"));
        }
        let profile = crate::protocols::protocol(&self.protocol)
            .ok_or_else(|| err("SINGLETON_ADVANCED_PROFILE"))?;
        crate::validate_chain(&self.chain_key)?;
        if !self.chain_key.starts_with("evm:")
            || format!("{}-advanced", s.protocol) != self.protocol
            || self.pool_id != format!("{}/{}", s.snapshot.manager, s.snapshot.pool_id)
            || self.token0 != s.currency0
            || self.token1 != s.currency1
            || self.reserve0 != crate::Amount::zero()
            || self.reserve1 != crate::Amount::zero()
            || self.adapter_version != profile.adapter
            || self.token_behavior != "standard"
            || self.fee_denominator != 1000000
            || self.fee_numerator != u64::from(s.snapshot.lp_fee)
        {
            return Err(err("SINGLETON_ADVANCED_IDENTITY"));
        }
        s.validate()
    }
}
