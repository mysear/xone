use crate::{err, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tiny_keccak::{Hasher, Keccak};
pub const PROFILE: &str = "pancake-infinity-bin";
pub const ADAPTER: &str = "pancake-infinity-bin/3c5ef1be-static-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InfinityBinTreeWord {
    pub index: u32,
    pub value: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InfinityBinEntry {
    pub id: u32,
    pub reserve0: Amount,
    pub reserve1: Amount,
    pub total_shares: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InfinityBinState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached_active: Option<InfinityBinEntry>,
    pub manager: String,
    pub manager_code_sha256: String,
    pub vault: String,
    pub vault_code_sha256: String,
    pub pool_id: String,
    pub hooks: String,
    pub parameters: String,
    pub token0_code_sha256: String,
    pub token1_code_sha256: String,
    pub block_hash: String,
    pub bin_step: u32,
    pub active_id: u32,
    pub lp_fee: u32,
    pub protocol_fee0: u32,
    pub protocol_fee1: u32,
    pub max_bins: u32,
    pub max_tree_words: u32,
    pub max_steps: u32,
    pub tree_level0: Amount,
    pub app_reserve0: Amount,
    pub app_reserve1: Amount,
    pub protocol_fees0: Amount,
    pub protocol_fees1: Amount,
    pub tree_level1: Vec<InfinityBinTreeWord>,
    pub tree_level2: Vec<InfinityBinTreeWord>,
    pub bins: Vec<InfinityBinEntry>,
    pub block_number: u64,
    pub timestamp: u64,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
fn nonzero_address(s: &str) -> bool {
    crate::protocols::validate_address(s).is_ok() && s != crate::singleton::ZERO_ADDRESS
}
impl InfinityBinState {
    pub fn key_id(&self, token0: &str, token1: &str) -> Result<String> {
        let word = |v: &str| -> Result<Vec<u8>> {
            let b = hex::decode(
                v.strip_prefix("0x")
                    .ok_or_else(|| err("INFINITY_BIN_KEY"))?,
            )
            .map_err(|_| err("INFINITY_BIN_KEY"))?;
            if b.len() > 32 {
                return Err(err("INFINITY_BIN_KEY"));
            }
            let mut out = vec![0; 32 - b.len()];
            out.extend(b);
            Ok(out)
        };
        let mut encoded = word(token0)?;
        encoded.extend(word(token1)?);
        encoded.extend(word(&self.hooks)?);
        encoded.extend(word(&self.manager)?);
        let mut fee = [0; 32];
        fee[28..].copy_from_slice(&self.lp_fee.to_be_bytes());
        encoded.extend(fee);
        encoded.extend(word(&self.parameters)?);
        let mut output = [0; 32];
        let mut hash = Keccak::v256();
        hash.update(&encoded);
        hash.finalize(&mut output);
        Ok(format!("0x{}", hex::encode(output)))
    }
    pub fn validate(&self, p: &Pool) -> Result<()> {
        if p.protocol != PROFILE
            || p.adapter_version != ADAPTER
            || p.token_behavior != "standard"
            || p.fee_denominator != 1_000_000
            || p.fee_numerator != u64::from(self.lp_fee)
            || !p.reserve0.0.is_zero()
            || !p.reserve1.0.is_zero()
            || !p.chain_key.starts_with("evm:")
            || crate::validate_chain(&p.chain_key).is_err()
        {
            return Err(err("INFINITY_BIN_PROFILE"));
        }
        if !nonzero_address(&self.manager)
            || !nonzero_address(&self.vault)
            || self.manager == self.vault
            || !nonzero_address(&p.token0)
            || !nonzero_address(&p.token1)
            || p.token0 >= p.token1
            || [&self.manager, &self.vault]
                .iter()
                .any(|a| **a == p.token0 || **a == p.token1)
            || self.hooks != crate::singleton::ZERO_ADDRESS
        {
            return Err(err("INFINITY_BIN_IDENTITY"));
        }
        for d in [
            &self.manager_code_sha256,
            &self.vault_code_sha256,
            &self.token0_code_sha256,
            &self.token1_code_sha256,
        ] {
            if !digest(d) {
                return Err(err("INFINITY_BIN_CODE_DIGEST"));
            }
        }
        if !(1..=100).contains(&self.bin_step)
            || self.active_id == 0
            || self.active_id >= 0xffffff
            || self.lp_fee > 100_000
            || self.protocol_fee0 > 4000
            || self.protocol_fee1 > 4000
            || !(1..=1024).contains(&self.max_bins)
            || !(1..=1024).contains(&self.max_tree_words)
            || !(1..=4096).contains(&self.max_steps)
        {
            return Err(err("INFINITY_BIN_PARAMETERS"));
        }
        if self.parameters != format!("0x{:064x}", self.bin_step << 16)
            || self.key_id(&p.token0, &p.token1)? != self.pool_id
            || p.pool_id != format!("{}/{}", self.manager, self.pool_id)
        {
            return Err(err("INFINITY_BIN_KEY"));
        }
        if self.timestamp == 0
            || self.timestamp.checked_mul(1000).is_none()
            || self.block_hash.len() != 66
            || !self.block_hash.starts_with("0x")
            || !digest(&self.block_hash[2..])
            || self.block_hash[2..].bytes().all(|b| b == b'0')
        {
            return Err(err("INFINITY_BIN_SOURCE"));
        }
        for a in [
            &self.tree_level0,
            &self.app_reserve0,
            &self.app_reserve1,
            &self.protocol_fees0,
            &self.protocol_fees1,
        ] {
            if a.0.bits() > 256 {
                return Err(err("INFINITY_BIN_U256"));
            }
        }
        if self.bins.is_empty() && self.detached_active.is_none()
            || self.bins.len() + usize::from(self.detached_active.is_some())
                > self.max_bins as usize
            || 1 + self.tree_level1.len() + self.tree_level2.len() > self.max_tree_words as usize
        {
            return Err(err("INFINITY_BIN_TREE_BUDGET"));
        }
        if let Some(b) = &self.detached_active {
            if b.id != self.active_id
                || self.bins.iter().any(|x| x.id == b.id)
                || b.reserve0.0.bits() > 128
                || b.reserve1.0.bits() > 128
                || b.total_shares.0 != BigUint::from(1000u32)
            {
                return Err(err("INFINITY_BIN_DETACHED_ACTIVE"));
            }
        }
        let mut root = BigUint::zero();
        let mut l1 = BTreeMap::<u32, BigUint>::new();
        let mut l2 = BTreeMap::<u32, BigUint>::new();
        let mut previous = 0;
        for b in &self.bins {
            if b.id <= previous
                || b.id >= 0xffffff
                || b.reserve0.0.bits() > 128
                || b.reserve1.0.bits() > 128
                || b.total_shares.0.is_zero()
                || b.total_shares.0.bits() > 256
            {
                return Err(err("INFINITY_BIN_ENTRY"));
            }
            previous = b.id;
            root |= BigUint::one() << (b.id >> 16) as usize;
            *l1.entry(b.id >> 16).or_default() |= BigUint::one() << ((b.id >> 8) & 255) as usize;
            *l2.entry(b.id >> 8).or_default() |= BigUint::one() << (b.id & 255) as usize;
        }
        fn same(rows: &[InfinityBinTreeWord], expected: &BTreeMap<u32, BigUint>) -> bool {
            rows.len() == expected.len()
                && rows
                    .iter()
                    .zip(expected)
                    .all(|(r, (i, v))| r.index == *i && r.value.0 == *v)
        }
        if self.tree_level0.0 != root
            || !same(&self.tree_level1, &l1)
            || !same(&self.tree_level2, &l2)
        {
            return Err(err("INFINITY_BIN_TREE_INCOMPLETE"));
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::InfinityBinState> {
        fn raw(s: &str) -> Result<Vec<u8>> {
            let b =
                hex::decode(s.trim_start_matches("0x")).map_err(|_| err("INFINITY_BIN_BYTES"))?;
            if b.len() != 32 {
                return Err(err("INFINITY_BIN_BYTES"));
            }
            Ok(b)
        }
        fn amount(a: &Amount) -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        }
        let words = |v: &[InfinityBinTreeWord]| {
            v.iter()
                .map(|w| {
                    Ok(wire::InfinityBinTreeWord {
                        index: w.index,
                        value: amount(&w.value)?,
                    })
                })
                .collect::<Result<Vec<_>>>()
        };
        Ok(wire::InfinityBinState {
            detached_active: self
                .detached_active
                .as_ref()
                .map(|b| {
                    Ok(wire::InfinityBinEntry {
                        id: b.id,
                        reserve0: amount(&b.reserve0)?,
                        reserve1: amount(&b.reserve1)?,
                        total_shares: amount(&b.total_shares)?,
                    })
                })
                .transpose()?,
            manager: self.manager.clone(),
            manager_code_sha256: raw(&self.manager_code_sha256)?,
            vault: self.vault.clone(),
            vault_code_sha256: raw(&self.vault_code_sha256)?,
            pool_id: raw(&self.pool_id)?,
            hooks: self.hooks.clone(),
            parameters: raw(&self.parameters)?,
            token0_code_sha256: raw(&self.token0_code_sha256)?,
            token1_code_sha256: raw(&self.token1_code_sha256)?,
            block_hash: raw(&self.block_hash)?,
            bin_step: self.bin_step,
            active_id: self.active_id,
            lp_fee: self.lp_fee,
            protocol_fee0: self.protocol_fee0,
            protocol_fee1: self.protocol_fee1,
            max_bins: self.max_bins,
            max_tree_words: self.max_tree_words,
            max_steps: self.max_steps,
            tree_level0: amount(&self.tree_level0)?,
            app_reserve0: amount(&self.app_reserve0)?,
            app_reserve1: amount(&self.app_reserve1)?,
            protocol_fees0: amount(&self.protocol_fees0)?,
            protocol_fees1: amount(&self.protocol_fees1)?,
            tree_level1: words(&self.tree_level1)?,
            tree_level2: words(&self.tree_level2)?,
            bins: self
                .bins
                .iter()
                .map(|b| {
                    Ok(wire::InfinityBinEntry {
                        id: b.id,
                        reserve0: amount(&b.reserve0)?,
                        reserve1: amount(&b.reserve1)?,
                        total_shares: amount(&b.total_shares)?,
                    })
                })
                .collect::<Result<_>>()?,
            block_number: self.block_number,
            timestamp: self.timestamp,
        })
    }
    pub fn from_proto(s: wire::InfinityBinState) -> Result<Self> {
        fn raw(b: Vec<u8>, prefix: bool) -> Result<String> {
            if b.len() != 32 {
                return Err(err("INFINITY_BIN_BYTES"));
            }
            Ok(format!(
                "{}{}",
                if prefix { "0x" } else { "" },
                hex::encode(b)
            ))
        }
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(
                &a.ok_or_else(|| err("INFINITY_BIN_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        }
        let words = |v: Vec<wire::InfinityBinTreeWord>| {
            v.into_iter()
                .map(|w| {
                    Ok(InfinityBinTreeWord {
                        index: w.index,
                        value: amount(w.value)?,
                    })
                })
                .collect::<Result<Vec<_>>>()
        };
        Ok(Self {
            detached_active: s
                .detached_active
                .map(|b| {
                    Ok(InfinityBinEntry {
                        id: b.id,
                        reserve0: amount(b.reserve0)?,
                        reserve1: amount(b.reserve1)?,
                        total_shares: amount(b.total_shares)?,
                    })
                })
                .transpose()?,
            manager: s.manager,
            manager_code_sha256: raw(s.manager_code_sha256, false)?,
            vault: s.vault,
            vault_code_sha256: raw(s.vault_code_sha256, false)?,
            pool_id: raw(s.pool_id, true)?,
            hooks: s.hooks,
            parameters: raw(s.parameters, true)?,
            token0_code_sha256: raw(s.token0_code_sha256, false)?,
            token1_code_sha256: raw(s.token1_code_sha256, false)?,
            block_hash: raw(s.block_hash, true)?,
            bin_step: s.bin_step,
            active_id: s.active_id,
            lp_fee: s.lp_fee,
            protocol_fee0: s.protocol_fee0,
            protocol_fee1: s.protocol_fee1,
            max_bins: s.max_bins,
            max_tree_words: s.max_tree_words,
            max_steps: s.max_steps,
            tree_level0: amount(s.tree_level0)?,
            app_reserve0: amount(s.app_reserve0)?,
            app_reserve1: amount(s.app_reserve1)?,
            protocol_fees0: amount(s.protocol_fees0)?,
            protocol_fees1: amount(s.protocol_fees1)?,
            tree_level1: words(s.tree_level1)?,
            tree_level2: words(s.tree_level2)?,
            bins: s
                .bins
                .into_iter()
                .map(|b| {
                    Ok(InfinityBinEntry {
                        id: b.id,
                        reserve0: amount(b.reserve0)?,
                        reserve1: amount(b.reserve1)?,
                        total_shares: amount(b.total_shares)?,
                    })
                })
                .collect::<Result<_>>()?,
            block_number: s.block_number,
            timestamp: s.timestamp,
        })
    }
}

/// Manager accounting is currency-scoped and shared by every physical Bin pool.
pub fn validate_dependencies<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let mut managers = BTreeMap::new();
    let mut currencies = BTreeMap::new();
    for p in pools {
        let Some(s) = &p.infinity_bin else { continue };
        let identity = (
            &s.manager_code_sha256,
            &s.vault,
            &s.vault_code_sha256,
            s.block_number,
            &s.block_hash,
            s.timestamp,
        );
        if managers
            .insert(&s.manager, identity)
            .is_some_and(|old| old != identity)
        {
            return Err(err("INFINITY_BIN_MANAGER_DEPENDENCY_CONFLICT"));
        }
        for (token, code, reserve, fee) in [
            (
                &p.token0,
                &s.token0_code_sha256,
                &s.app_reserve0,
                &s.protocol_fees0,
            ),
            (
                &p.token1,
                &s.token1_code_sha256,
                &s.app_reserve1,
                &s.protocol_fees1,
            ),
        ] {
            let observed = (code, reserve, fee);
            if currencies
                .insert((&s.manager, token), observed)
                .is_some_and(|old| old != observed)
            {
                return Err(err("INFINITY_BIN_CURRENCY_DEPENDENCY_CONFLICT"));
            }
        }
    }
    Ok(())
}
