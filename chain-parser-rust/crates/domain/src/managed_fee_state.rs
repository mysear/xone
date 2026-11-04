use crate::{err, protocols::validate_address, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};

/// Every BPT ledger entry changed by a pinned ManagedPool regular swap.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BalancerManagedState {
    pub total_supply: Amount,
    pub protocol_swap_fee: Amount,
    pub management_swap_fee: Amount,
    pub owner: String,
    pub protocol_collector: String,
    pub owner_balance: Amount,
    pub protocol_collector_balance: Amount,
    pub timestamp: u64,
    pub block_number: u64,
    pub block_hash: String,
}
impl Pool {
    pub(crate) fn validate_managed_fees(&self) -> Result<()> {
        let s = self
            .multi_asset
            .as_ref()
            .and_then(|s| s.managed_fees.as_ref())
            .ok_or_else(|| err("MANAGED_FEE_STATE_REQUIRED"))?;
        validate_address(&s.owner)?;
        validate_address(&s.protocol_collector)?;
        if s.protocol_collector == format!("0x{}", "0".repeat(40))
            || s.timestamp == 0
            || s.timestamp.checked_mul(1000).is_none()
            || s.block_hash.len() != 66
            || !s.block_hash.starts_with("0x")
            || s.block_hash != s.block_hash.to_ascii_lowercase()
            || hex::decode(&s.block_hash[2..]).is_err()
            || s.block_hash[2..].bytes().all(|b| b == b'0')
        {
            return Err(err("MANAGED_FEE_IDENTITY"));
        }
        for a in [
            &s.total_supply,
            &s.protocol_swap_fee,
            &s.management_swap_fee,
            &s.owner_balance,
            &s.protocol_collector_balance,
        ] {
            if a.0.bits() > 256 {
                return Err(err("AMOUNT_OVERFLOW"));
            }
        }
        let one = BigUint::from(1_000_000_000_000_000_000u64);
        if s.total_supply.0.is_zero()
            || s.protocol_swap_fee.0 > BigUint::from(500_000_000_000_000_000u64)
            || s.management_swap_fee.0 > one
            || s.owner_balance.0 > s.total_supply.0
            || s.protocol_collector_balance.0 > s.total_supply.0
        {
            return Err(err("MANAGED_FEE_ACCOUNTING"));
        }
        if s.owner == s.protocol_collector {
            if s.owner_balance != s.protocol_collector_balance {
                return Err(err("MANAGED_FEE_RECIPIENT_ALIAS"));
            }
        } else if &s.owner_balance.0 + &s.protocol_collector_balance.0 > s.total_supply.0 {
            return Err(err("MANAGED_FEE_ACCOUNTING"));
        }
        Ok(())
    }
}
impl BalancerManagedState {
    pub fn to_proto(&self) -> Result<wire::BalancerManagedState> {
        fn amount(a: &Amount) -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        }
        let hash = hex::decode(self.block_hash.trim_start_matches("0x"))
            .map_err(|_| err("MANAGED_FEE_BLOCK_HASH"))?;
        if hash.len() != 32 {
            return Err(err("MANAGED_FEE_BLOCK_HASH"));
        }
        Ok(wire::BalancerManagedState {
            total_supply: amount(&self.total_supply)?,
            protocol_swap_fee: amount(&self.protocol_swap_fee)?,
            management_swap_fee: amount(&self.management_swap_fee)?,
            owner: self.owner.clone(),
            protocol_collector: self.protocol_collector.clone(),
            owner_balance: amount(&self.owner_balance)?,
            protocol_collector_balance: amount(&self.protocol_collector_balance)?,
            timestamp: self.timestamp,
            block_number: self.block_number,
            block_hash: hash,
        })
    }
    pub fn from_proto(s: wire::BalancerManagedState) -> Result<Self> {
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(
                &a.ok_or_else(|| err("MANAGED_FEE_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        }
        if s.block_hash.len() != 32 {
            return Err(err("MANAGED_FEE_BLOCK_HASH"));
        }
        Ok(Self {
            total_supply: amount(s.total_supply)?,
            protocol_swap_fee: amount(s.protocol_swap_fee)?,
            management_swap_fee: amount(s.management_swap_fee)?,
            owner: s.owner,
            protocol_collector: s.protocol_collector,
            owner_balance: amount(s.owner_balance)?,
            protocol_collector_balance: amount(s.protocol_collector_balance)?,
            timestamp: s.timestamp,
            block_number: s.block_number,
            block_hash: format!("0x{}", hex::encode(s.block_hash)),
        })
    }
}
