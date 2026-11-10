//! Complete control observations for the published single-token BPT adapter.
use crate::{err, protocols::validate_address, wire, Amount, Result};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "balancer-v2-managed-bpt";
pub const ADAPTER_VERSION: &str = "balancer-v2/20230411-managed-bpt-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2AllowlistMember {
    pub address: String,
    pub allowed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2BptState {
    pub join_exit_enabled: bool,
    pub minimum_bpt_locked: Amount,
    pub bpt_last_change_block: u64,
    pub allowlist_members: Vec<ManagedV2AllowlistMember>,
}
impl ManagedV2BptState {
    pub fn validate(&self) -> Result<()> {
        if self.minimum_bpt_locked != Amount(1_000_000u64.into())
            || self.bpt_last_change_block > u64::from(u32::MAX)
        {
            return Err(err("MANAGED_V2_BPT_CONTROL"));
        }
        let mut previous: Option<&str> = None;
        for member in &self.allowlist_members {
            validate_address(&member.address)?;
            if previous.is_some_and(|p| p >= member.address.as_str()) {
                return Err(err("MANAGED_V2_BPT_MEMBER_ORDER"));
            }
            previous = Some(&member.address);
        }
        Ok(())
    }
    /// Missing membership is unknown; callers cannot treat it as permission.
    pub fn member_allowed(&self, address: &str) -> Result<bool> {
        self.validate()?;
        validate_address(address)?;
        self.allowlist_members
            .binary_search_by(|m| m.address.as_str().cmp(address))
            .map(|i| self.allowlist_members[i].allowed)
            .map_err(|_| err("MANAGED_V2_BPT_MEMBER_UNKNOWN"))
    }
    pub fn to_proto(&self) -> Result<wire::ManagedV2BptState> {
        self.validate()?;
        Ok(wire::ManagedV2BptState {
            join_exit_enabled: Some(self.join_exit_enabled),
            minimum_bpt_locked: Some(wire::Amount {
                uint256_be: self.minimum_bpt_locked.to_be_bytes()?.to_vec(),
            }),
            bpt_last_change_block: Some(self.bpt_last_change_block),
            allowlist_members: self
                .allowlist_members
                .iter()
                .map(|m| wire::ManagedV2AllowlistMember {
                    address: m.address.clone(),
                    allowed: Some(m.allowed),
                })
                .collect(),
        })
    }
    pub fn from_proto(w: wire::ManagedV2BptState) -> Result<Self> {
        let s = Self {
            join_exit_enabled: w
                .join_exit_enabled
                .ok_or_else(|| err("MANAGED_V2_BPT_SCALAR_REQUIRED"))?,
            minimum_bpt_locked: Amount::from_be_bytes(
                &w.minimum_bpt_locked
                    .ok_or_else(|| err("MANAGED_V2_BPT_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )?,
            bpt_last_change_block: w
                .bpt_last_change_block
                .ok_or_else(|| err("MANAGED_V2_BPT_SCALAR_REQUIRED"))?,
            allowlist_members: w
                .allowlist_members
                .into_iter()
                .map(|m| {
                    Ok(ManagedV2AllowlistMember {
                        address: m.address,
                        allowed: m
                            .allowed
                            .ok_or_else(|| err("MANAGED_V2_BPT_SCALAR_REQUIRED"))?,
                    })
                })
                .collect::<Result<_>>()?,
        };
        s.validate()?;
        Ok(s)
    }
}
