use crate::{
    err,
    protocols::{validate_address, validate_identity},
    wire, Amount, Pool, Result,
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "balancer-v2-managed-aum";
pub const ADAPTER_VERSION: &str = "balancer-v2/20230411-managed-aum-3";
// Observed runtime hashes are distinct from original templates; shape is not provenance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2Dependency {
    pub role: String,
    pub address: String,
    pub code_sha256: String,
    pub template_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2TokenInfo {
    pub vault_actual_balance: Amount,
    pub cash: Amount,
    pub managed: Amount,
    pub asset_manager: String,
    pub last_change_block: u64,
    pub circuit_breaker_enabled: bool,
}
/// Noncanonical computation clock; source-bank identity remains in the parent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2ExecutionContext {
    pub block_number: u64,
    pub timestamp: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BalancerManagedV2State {
    pub vault_bpt_actual_balance: Amount,
    pub timestamp: u64,
    pub block_number: u64,
    pub block_hash: String,
    pub factory: String,
    pub factory_code_sha256: String,
    pub pool_runtime_template_sha256: String,
    pub fee_provider: String,
    pub fee_provider_code_sha256: String,
    pub protocol_collector: String,
    pub protocol_collector_code_sha256: String,
    pub dependencies: Vec<ManagedV2Dependency>,
    pub total_supply: Amount,
    pub virtual_supply: Amount,
    pub actual_supply: Amount,
    pub pending_aum: Amount,
    pub vault_bpt_cash: Amount,
    pub vault_bpt_managed: Amount,
    pub bpt_index: u32,
    pub manager: String,
    pub manager_bpt_balance: Amount,
    pub protocol_collector_bpt_balance: Amount,
    /// Effective original getter value; recovery mode exposes zero.
    pub management_aum_fee: Amount,
    pub last_collection_timestamp: u32,
    pub provider_fee_id: Amount,
    pub live_provider_aum_percentage: Amount,
    pub cached_protocol_aum_percentage: Amount,
    pub swaps_enabled: bool,
    /// Pool own effective getPausedState flag at the current context.
    pub paused: bool,
    pub vault_paused: bool,
    pub vault_pause_window_end: u64,
    pub vault_buffer_period_end: u64,
    pub recovery_mode: bool,
    pub allowlist_enabled: bool,
    pub pause_window_end: u64,
    pub buffer_period_end: u64,
    pub gradual_weight_end: u64,
    pub gradual_fee_end: u64,
    pub token_info: Vec<ManagedV2TokenInfo>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub execution_context: Option<ManagedV2ExecutionContext>,
}

fn digest(v: &str) -> bool {
    v.len() == 64 && v == v.to_ascii_lowercase() && hex::decode(v).is_ok()
}
pub fn validate_dependencies(ds: &[ManagedV2Dependency]) -> Result<()> {
    let roles = [
        "circuit-breaker-lib",
        "managed-pool-add-remove-token-lib",
        "managed-pool-amm-lib",
        "external-weighted-math",
        "recovery-mode-helper",
    ];
    let mut seen = std::collections::BTreeSet::new();
    if ds.len() != roles.len() {
        return Err(err("MANAGED_V2_DEPENDENCIES"));
    }
    for d in ds {
        if !roles.contains(&d.role.as_str())
            || !seen.insert(&d.role)
            || validate_identity(&d.address, &d.code_sha256).is_err()
            || !digest(&d.template_sha256)
        {
            return Err(err("MANAGED_V2_DEPENDENCIES"));
        }
    }
    Ok(())
}
impl BalancerManagedV2State {
    pub fn validate(&self) -> Result<()> {
        let s = self;
        validate_identity(&s.factory, &s.factory_code_sha256)?;
        validate_identity(&s.fee_provider, &s.fee_provider_code_sha256)?;
        validate_identity(&s.protocol_collector, &s.protocol_collector_code_sha256)?;
        validate_address(&s.manager)?;
        if !digest(&s.pool_runtime_template_sha256)
            || s.block_hash.len() != 66
            || !s.block_hash.starts_with("0x")
            || !digest(&s.block_hash[2..])
            || s.block_hash[2..].bytes().all(|x| x == b'0')
            || s.timestamp.checked_mul(1000).is_none()
        {
            return Err(err("MANAGED_V2_IDENTITY"));
        }
        if let Some(c) = &s.execution_context {
            if c.block_number < s.block_number
                || c.timestamp < s.timestamp
                || c.timestamp < u64::from(s.last_collection_timestamp)
                || c.timestamp.checked_mul(1000).is_none()
                || s.token_info
                    .iter()
                    .any(|v| v.last_change_block > c.block_number)
            {
                return Err(err("MANAGED_V2_EXECUTION_CONTEXT"));
            }
        }
        validate_dependencies(&s.dependencies)?;
        for a in [
            &s.total_supply,
            &s.virtual_supply,
            &s.actual_supply,
            &s.pending_aum,
            &s.vault_bpt_cash,
            &s.vault_bpt_managed,
            &s.vault_bpt_actual_balance,
            &s.manager_bpt_balance,
            &s.protocol_collector_bpt_balance,
            &s.management_aum_fee,
            &s.provider_fee_id,
            &s.live_provider_aum_percentage,
            &s.cached_protocol_aum_percentage,
        ] {
            if a.0.bits() > 256 {
                return Err(err("AMOUNT_OVERFLOW"));
            }
        }

        if s.management_aum_fee.0 > BigUint::from(950_000_000_000_000_000u64)
            || s.live_provider_aum_percentage.0 > BigUint::from(1_000_000_000_000_000_000u64)
            || s.cached_protocol_aum_percentage.0 > BigUint::from(1_000_000_000_000_000_000u64)
        {
            return Err(err("MANAGED_V2_FEE"));
        }
        if s.vault_bpt_actual_balance.0 < s.vault_bpt_cash.0
            || s.vault_bpt_actual_balance.0 > s.total_supply.0
        {
            return Err(err("MANAGED_V2_ACTUAL_BPT_BALANCE"));
        }
        if s.bpt_index != 0 || s.vault_bpt_managed.0 != BigUint::from(0u8) {
            return Err(err("MANAGED_V2_BPT_REGISTRATION"));
        }
        if s.provider_fee_id.0.bits() > 8 {
            return Err(err("MANAGED_V2_PROVIDER_FEE_ID_WIDTH"));
        }
        if s.recovery_mode && s.management_aum_fee.0 != BigUint::from(0u8) {
            return Err(err("MANAGED_V2_RECOVERY_EFFECTIVE_FEE"));
        }
        if s.management_aum_fee.0 == BigUint::from(0u8) && s.pending_aum.0 != BigUint::from(0u8) {
            return Err(err("MANAGED_V2_ZERO_FEE_PENDING"));
        }
        let own = &s.vault_bpt_cash.0 + &s.vault_bpt_managed.0;
        if own.bits() > 112
            || &s.virtual_supply.0 + &own != s.total_supply.0
            || &s.virtual_supply.0 + &s.pending_aum.0 != s.actual_supply.0
            || s.manager_bpt_balance.0 > s.total_supply.0
            || s.protocol_collector_bpt_balance.0 > s.total_supply.0
        {
            return Err(err("MANAGED_V2_SUPPLY"));
        }
        if s.manager == s.protocol_collector {
            if s.manager_bpt_balance != s.protocol_collector_bpt_balance {
                return Err(err("MANAGED_V2_RECIPIENT_ALIAS"));
            }
        } else if &s.manager_bpt_balance.0 + &s.protocol_collector_bpt_balance.0 > s.total_supply.0
        {
            return Err(err("MANAGED_V2_LEDGER"));
        }
        if !(2..=50).contains(&s.token_info.len())
            || s.bpt_index as usize > s.token_info.len()
            || s.pause_window_end > s.buffer_period_end
            || s.vault_pause_window_end > s.vault_buffer_period_end
        {
            return Err(err("MANAGED_V2_PARAMETERS"));
        }
        // getPausedState is effective at the source or derived computation clock.
        let pause_timestamp = s
            .execution_context
            .as_ref()
            .map_or(s.timestamp, |c| c.timestamp);
        if (s.paused && pause_timestamp > s.buffer_period_end)
            || (s.vault_paused && pause_timestamp > s.vault_buffer_period_end)
        {
            return Err(err("MANAGED_V2_EFFECTIVE_PAUSE"));
        }
        if s.gradual_weight_end > s.timestamp || s.gradual_fee_end > s.timestamp {
            return Err(err("MANAGED_V2_ACTIVE_SCHEDULE_UNSUPPORTED"));
        }
        for v in &s.token_info {
            if v.last_change_block > u64::from(u32::MAX) {
                return Err(err("MANAGED_V2_VAULT_BLOCK_WIDTH"));
            }
            validate_address(&v.asset_manager)?;
            if v.managed.0 != BigUint::from(0u8)
                && v.asset_manager == "0x0000000000000000000000000000000000000000"
            {
                return Err(err("MANAGED_V2_TOKEN_ASSET_MANAGER"));
            }
            if v.vault_actual_balance.0.bits() > 256 || v.vault_actual_balance.0 < v.cash.0 {
                return Err(err("MANAGED_V2_ACTUAL_TOKEN_BALANCE"));
            }
            if v.cash.0.bits() > 256
                || v.managed.0.bits() > 256
                || (&v.cash.0 + &v.managed.0).bits() > 112
            {
                return Err(err("MANAGED_V2_TOKEN_INFO"));
            };
            if v.circuit_breaker_enabled {
                return Err(err("MANAGED_V2_CIRCUIT_BREAKER_UNSUPPORTED"));
            }
        }
        Ok(())
    }
}
impl Pool {
    pub(crate) fn validate_managed_v2(&self) -> Result<()> {
        let p = self;
        let s = p
            .multi_asset
            .as_ref()
            .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
        let m = s
            .managed_v2
            .as_ref()
            .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
        crate::validate_chain(&p.chain_key)?;
        validate_identity(&p.pool_id, &s.code_sha256)?;
        validate_identity(&s.vault, &s.vault_code_sha256)?;
        if s.managed_fees.is_some()
            || s.curve_crypto.is_some()
            || s.curve_meta.is_some()
            || s.curve_ng.is_some()
            || !p.chain_key.starts_with("evm:")
            || p.adapter_version != ADAPTER_VERSION
            || p.token_behavior != "standard"
            || p.fee_denominator != 1_000_000_000_000_000_000
            || !(1_000_000_000_000..=950_000_000_000_000_000).contains(&p.fee_numerator)
            || !(2..=50).contains(&s.assets.len())
            || s.weights.len() != s.assets.len()
            || m.token_info.len() != s.assets.len()
            || s.amplification.0 != BigUint::from(0u8)
            || s.admin_fee != 0
            || s.pool_id.len() != 66
            || !s.pool_id.starts_with(&format!("{}0001", p.pool_id))
            || !digest(&s.pool_id[2..])
        {
            return Err(err("MANAGED_V2_PARAMETERS"));
        }
        m.validate()?;
        let mut wallets = std::collections::BTreeMap::new();
        wallets.insert(s.vault.as_str(), &m.vault_bpt_actual_balance.0);
        for (address, balance) in [
            (&m.manager, &m.manager_bpt_balance),
            (&m.protocol_collector, &m.protocol_collector_bpt_balance),
        ] {
            if let Some(previous) = wallets.get(address.as_str()) {
                if *previous != &balance.0 {
                    return Err(err("MANAGED_V2_VAULT_WALLET_ALIAS"));
                }
            }
            wallets.insert(address.as_str(), &balance.0);
        }
        let observed = wallets
            .values()
            .fold(BigUint::from(0u8), |sum, balance| sum + *balance);
        if observed > m.total_supply.0 {
            return Err(err("MANAGED_V2_WALLET_SUPPLY"));
        }
        // A Vault recipient's ERC20 wallet may hold BPT registered in other pools.
        // Its balance is not the own registration used for virtual supply.
        if p.token0 != s.assets[0].token
            || p.token1 != s.assets[1].token
            || p.reserve0 != s.assets[0].balance
            || p.reserve1 != s.assets[1].balance
        {
            return Err(err("MULTI_ASSET_DUPLICATE_FIELDS"));
        }
        let mut total = BigUint::from(0u8);
        for (i, a) in s.assets.iter().enumerate() {
            validate_identity(&a.token, &a.code_sha256)?;
            if a.token == p.pool_id
                || a.token == s.vault
                || i > 0 && s.assets[i - 1].token >= a.token
                || !(18..=36).any(|n| a.scaling.0 == BigUint::from(10u8).pow(n))
                || a.balance.0.bits() > 256
            {
                return Err(err("MANAGED_V2_ASSET"));
            };
            if a.balance.0 != &m.token_info[i].cash.0 + &m.token_info[i].managed.0 {
                return Err(err("MANAGED_V2_BALANCE"));
            };
            if s.weights[i].0.bits() > 256
                || s.weights[i].0 < BigUint::from(10_000_000_000_000_000u64)
            {
                return Err(err("MANAGED_V2_WEIGHT"));
            };
            total += &s.weights[i].0;
        }
        if total != BigUint::from(1_000_000_000_000_000_000u64) {
            return Err(err("MANAGED_V2_WEIGHT_SUM"));
        };
        Ok(())
    }
}
fn amount(v: Option<wire::Amount>) -> Result<Amount> {
    Amount::from_be_bytes(
        &v.ok_or_else(|| err("MANAGED_V2_AMOUNT_REQUIRED"))?
            .uint256_be,
    )
}
fn hash(v: Vec<u8>, block: bool) -> Result<String> {
    if v.len() != 32 {
        return Err(err("MANAGED_V2_HASH_WIDTH"));
    };
    Ok(format!(
        "{}{}",
        if block { "0x" } else { "" },
        hex::encode(v)
    ))
}
fn raw(v: &str) -> Result<Vec<u8>> {
    let b =
        hex::decode(v.strip_prefix("0x").unwrap_or(v)).map_err(|_| err("MANAGED_V2_HASH_WIDTH"))?;
    if b.len() != 32 {
        return Err(err("MANAGED_V2_HASH_WIDTH"));
    };
    Ok(b)
}
impl ManagedV2Dependency {
    pub fn to_proto(&self) -> Result<wire::ManagedV2Dependency> {
        Ok(wire::ManagedV2Dependency {
            role: self.role.clone(),
            address: self.address.clone(),
            code_sha256: raw(&self.code_sha256)?,
            template_sha256: raw(&self.template_sha256)?,
        })
    }
    pub fn from_proto(w: wire::ManagedV2Dependency) -> Result<Self> {
        let s = Self {
            role: w.role,
            address: w.address,
            code_sha256: hash(w.code_sha256, false)?,
            template_sha256: hash(w.template_sha256, false)?,
        };
        Ok(s)
    }
}
impl ManagedV2TokenInfo {
    pub fn to_proto(&self) -> Result<wire::ManagedV2TokenInfo> {
        Ok(wire::ManagedV2TokenInfo {
            vault_actual_balance: Some(wire::Amount {
                uint256_be: self.vault_actual_balance.to_be_bytes()?.to_vec(),
            }),
            cash: Some(wire::Amount {
                uint256_be: self.cash.to_be_bytes()?.to_vec(),
            }),
            managed: Some(wire::Amount {
                uint256_be: self.managed.to_be_bytes()?.to_vec(),
            }),
            asset_manager: self.asset_manager.clone(),
            last_change_block: Some(self.last_change_block),
            circuit_breaker_enabled: Some(self.circuit_breaker_enabled),
        })
    }
    pub fn from_proto(w: wire::ManagedV2TokenInfo) -> Result<Self> {
        let s = Self {
            vault_actual_balance: amount(w.vault_actual_balance)?,
            cash: amount(w.cash)?,
            managed: amount(w.managed)?,
            asset_manager: w.asset_manager,
            last_change_block: w
                .last_change_block
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            circuit_breaker_enabled: w
                .circuit_breaker_enabled
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
        };
        Ok(s)
    }
}
impl BalancerManagedV2State {
    pub fn to_proto(&self) -> Result<wire::BalancerManagedV2State> {
        if self.execution_context.is_some() {
            return Err(err("MANAGED_V2_DERIVED_STATE_NOT_CANONICAL"));
        }
        self.validate()?;
        Ok(wire::BalancerManagedV2State {
            vault_bpt_actual_balance: Some(wire::Amount {
                uint256_be: self.vault_bpt_actual_balance.to_be_bytes()?.to_vec(),
            }),
            timestamp: Some(self.timestamp),
            block_number: Some(self.block_number),
            block_hash: raw(&self.block_hash)?,
            factory: self.factory.clone(),
            factory_code_sha256: raw(&self.factory_code_sha256)?,
            pool_runtime_template_sha256: raw(&self.pool_runtime_template_sha256)?,
            fee_provider: self.fee_provider.clone(),
            fee_provider_code_sha256: raw(&self.fee_provider_code_sha256)?,
            protocol_collector: self.protocol_collector.clone(),
            protocol_collector_code_sha256: raw(&self.protocol_collector_code_sha256)?,
            dependencies: self
                .dependencies
                .iter()
                .map(|v| v.to_proto())
                .collect::<Result<_>>()?,
            total_supply: Some(wire::Amount {
                uint256_be: self.total_supply.to_be_bytes()?.to_vec(),
            }),
            virtual_supply: Some(wire::Amount {
                uint256_be: self.virtual_supply.to_be_bytes()?.to_vec(),
            }),
            actual_supply: Some(wire::Amount {
                uint256_be: self.actual_supply.to_be_bytes()?.to_vec(),
            }),
            pending_aum: Some(wire::Amount {
                uint256_be: self.pending_aum.to_be_bytes()?.to_vec(),
            }),
            vault_bpt_cash: Some(wire::Amount {
                uint256_be: self.vault_bpt_cash.to_be_bytes()?.to_vec(),
            }),
            vault_bpt_managed: Some(wire::Amount {
                uint256_be: self.vault_bpt_managed.to_be_bytes()?.to_vec(),
            }),
            bpt_index: Some(u64::from(self.bpt_index)),
            manager: self.manager.clone(),
            manager_bpt_balance: Some(wire::Amount {
                uint256_be: self.manager_bpt_balance.to_be_bytes()?.to_vec(),
            }),
            protocol_collector_bpt_balance: Some(wire::Amount {
                uint256_be: self.protocol_collector_bpt_balance.to_be_bytes()?.to_vec(),
            }),
            management_aum_fee: Some(wire::Amount {
                uint256_be: self.management_aum_fee.to_be_bytes()?.to_vec(),
            }),
            last_collection_timestamp: Some(u64::from(self.last_collection_timestamp)),
            provider_fee_id: Some(wire::Amount {
                uint256_be: self.provider_fee_id.to_be_bytes()?.to_vec(),
            }),
            live_provider_aum_percentage: Some(wire::Amount {
                uint256_be: self.live_provider_aum_percentage.to_be_bytes()?.to_vec(),
            }),
            cached_protocol_aum_percentage: Some(wire::Amount {
                uint256_be: self.cached_protocol_aum_percentage.to_be_bytes()?.to_vec(),
            }),
            swaps_enabled: Some(self.swaps_enabled),
            paused: Some(self.paused),
            vault_paused: Some(self.vault_paused),
            vault_pause_window_end: Some(self.vault_pause_window_end),
            vault_buffer_period_end: Some(self.vault_buffer_period_end),
            recovery_mode: Some(self.recovery_mode),
            allowlist_enabled: Some(self.allowlist_enabled),
            pause_window_end: Some(self.pause_window_end),
            buffer_period_end: Some(self.buffer_period_end),
            gradual_weight_end: Some(self.gradual_weight_end),
            gradual_fee_end: Some(self.gradual_fee_end),
            token_info: self
                .token_info
                .iter()
                .map(|v| v.to_proto())
                .collect::<Result<_>>()?,
        })
    }
    pub fn from_proto(w: wire::BalancerManagedV2State) -> Result<Self> {
        let s = Self {
            vault_bpt_actual_balance: amount(w.vault_bpt_actual_balance)?,
            execution_context: None,
            timestamp: w
                .timestamp
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            block_number: w
                .block_number
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            block_hash: hash(w.block_hash, true)?,
            factory: w.factory,
            factory_code_sha256: hash(w.factory_code_sha256, false)?,
            pool_runtime_template_sha256: hash(w.pool_runtime_template_sha256, false)?,
            fee_provider: w.fee_provider,
            fee_provider_code_sha256: hash(w.fee_provider_code_sha256, false)?,
            protocol_collector: w.protocol_collector,
            protocol_collector_code_sha256: hash(w.protocol_collector_code_sha256, false)?,
            dependencies: w
                .dependencies
                .into_iter()
                .map(ManagedV2Dependency::from_proto)
                .collect::<Result<_>>()?,
            total_supply: amount(w.total_supply)?,
            virtual_supply: amount(w.virtual_supply)?,
            actual_supply: amount(w.actual_supply)?,
            pending_aum: amount(w.pending_aum)?,
            vault_bpt_cash: amount(w.vault_bpt_cash)?,
            vault_bpt_managed: amount(w.vault_bpt_managed)?,
            bpt_index: u32::try_from(
                w.bpt_index
                    .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            )
            .map_err(|_| err("MANAGED_V2_UINT32_WIDTH"))?,
            manager: w.manager,
            manager_bpt_balance: amount(w.manager_bpt_balance)?,
            protocol_collector_bpt_balance: amount(w.protocol_collector_bpt_balance)?,
            management_aum_fee: amount(w.management_aum_fee)?,
            last_collection_timestamp: u32::try_from(
                w.last_collection_timestamp
                    .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            )
            .map_err(|_| err("MANAGED_V2_UINT32_WIDTH"))?,
            provider_fee_id: amount(w.provider_fee_id)?,
            live_provider_aum_percentage: amount(w.live_provider_aum_percentage)?,
            cached_protocol_aum_percentage: amount(w.cached_protocol_aum_percentage)?,
            swaps_enabled: w
                .swaps_enabled
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            paused: w.paused.ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            vault_paused: w
                .vault_paused
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            vault_pause_window_end: w
                .vault_pause_window_end
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            vault_buffer_period_end: w
                .vault_buffer_period_end
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            recovery_mode: w
                .recovery_mode
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            allowlist_enabled: w
                .allowlist_enabled
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            pause_window_end: w
                .pause_window_end
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            buffer_period_end: w
                .buffer_period_end
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            gradual_weight_end: w
                .gradual_weight_end
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            gradual_fee_end: w
                .gradual_fee_end
                .ok_or_else(|| err("MANAGED_V2_SCALAR_REQUIRED"))?,
            token_info: w
                .token_info
                .into_iter()
                .map(ManagedV2TokenInfo::from_proto)
                .collect::<Result<_>>()?,
        };
        s.validate()?;
        Ok(s)
    }
}

// Missing optional extensions are permitted, an explicit JSON null is unknown.
pub fn deserialize_present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
