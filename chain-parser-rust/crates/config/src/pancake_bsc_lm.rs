use super::{PancakeLmConfig, PoolConfig};
use serde::{Deserialize, Serialize};
use xone_domain::{err, Result};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BscPredecessorConfig {
    pub address: String,
    pub code_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PancakeBscLmConfig {
    pub ledger: PancakeLmConfig,
    pub predecessors: Vec<BscPredecessorConfig>,
}
impl PancakeBscLmConfig {
    pub(super) fn validate(&self, p: &PoolConfig, chain: &str) -> Result<()> {
        if p.protocol != xone_domain::pancake_bsc_lm::PROFILE
            || self.predecessors.len() != 3
            || p.pancake_lm.is_some()
        {
            return Err(err("BSC4_CONFIG"));
        }
        let mut projected = p.clone();
        projected.protocol = xone_domain::pancake_lm::PROFILE.into();
        projected.pancake_bsc_lm = None;
        projected.pancake_lm = Some(self.ledger.clone());
        self.ledger.validate(&projected, chain)?;
        let mut seen = std::collections::BTreeSet::from([
            p.id.as_str(),
            p.token0.as_str(),
            p.token1.as_str(),
            self.ledger.expected_lm_pool.as_str(),
            self.ledger.expected_master_chef.as_str(),
            self.ledger.expected_factory.as_str(),
        ]);
        for x in &self.predecessors {
            xone_domain::protocols::validate_identity(&x.address, &x.code_sha256)?;
            if x.address == xone_domain::singleton::ZERO_ADDRESS || !seen.insert(&x.address) {
                return Err(err("BSC4_PREDECESSOR_CONFIG_ALIAS"));
            }
        }
        Ok(())
    }
}
