use crate::{err, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub family: &'static str,
    pub adapter: &'static str,
    pub fee_numerator: u64,
    pub fee_denominator: u64,
    pub fee_protocol_bits: u8,
    pub require_identity: bool,
}
pub fn protocol(name: &str) -> Option<Profile> {
    let (family, adapter, n, d, bits, identity) = match name {
        crate::v2_swap::PROFILE => ("v2", crate::v2_swap::ADAPTER, 3, 1000, 0, true),
        "raydium-clmm-rewards" => (
            "rayclmm",
            "raydium-clmm/6d7a0f82-spl-rewards-1",
            0,
            1000000,
            0,
            true,
        ),
        "raydium-clmm-dynamic" => (
            "rayclmm",
            "raydium-clmm/6d7a0f82-spl-dynamic-1",
            0,
            1000000,
            0,
            true,
        ),
        "raydium-clmm-custom-seed" => (
            "rayclmm",
            "raydium-clmm/6d7a0f82-spl-custom-seed-1",
            0,
            1000000,
            0,
            true,
        ),
        "raydium-clmm-bitmap" => (
            "rayclmm",
            "raydium-clmm/6d7a0f82-spl-bitmap-1",
            0,
            1000000,
            0,
            true,
        ),
        "raydium-clmm-token-fee" => (
            "rayclmm",
            "raydium-clmm/6d7a0f82-spl-token-fee-1",
            0,
            1000000,
            0,
            true,
        ),
        "raydium-clmm-static" => (
            "rayclmm",
            "raydium-clmm/6d7a0f82-spl-static-1",
            0,
            1000000,
            0,
            true,
        ),
        "orca-whirlpools-adaptive-dynamic" => (
            "orca",
            "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1",
            0,
            1000000,
            0,
            true,
        ),
        "orca-whirlpools-adaptive-rewards" => (
            "orca",
            "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1",
            0,
            1000000,
            0,
            true,
        ),
        "orca-whirlpools-adaptive" => (
            "orca",
            "orca-whirlpools/cc683ec9-spl-adaptive-1",
            0,
            1000000,
            0,
            true,
        ),
        "orca-whirlpools-rewards" => (
            "orca",
            "orca-whirlpools/cc683ec9-spl-rewards-1",
            0,
            1000000,
            0,
            true,
        ),
        "orca-whirlpools-static" => (
            "orca",
            "orca-whirlpools/cc683ec9-spl-static-1",
            0,
            1000000,
            0,
            true,
        ),
        "raydium-cpmm-creator" => (
            "raydium",
            "raydium-cpmm/f4b9b263-spl-1",
            0,
            1000000,
            0,
            true,
        ),
        "pancake-infinity-bin" => (
            "infinity-bin",
            "pancake-infinity-bin/3c5ef1be-static-1",
            0,
            1000000,
            0,
            true,
        ),
        "uniswap-v4-advanced" => (
            "singleton-advanced",
            "uniswap-v4/core-advanced-1",
            0,
            1000000,
            0,
            true,
        ),
        "pancake-infinity-cl-advanced" => (
            "singleton-advanced",
            "pancake-infinity-cl/cl-advanced-1",
            0,
            1000000,
            0,
            true,
        ),
        "uniswap-v4" => ("singleton", "uniswap-v4/core-1", 0, 1000000, 0, true),
        "pancake-infinity-cl" => ("singleton", "pancake-infinity-cl/cl-1", 0, 1000000, 0, true),
        crate::slipstream_context::PROFILE => (
            "slipstream-context",
            crate::slipstream_context::ADAPTER,
            0,
            1000000,
            0,
            true,
        ),
        crate::meteora_token2022::PROFILE => (
            "meteora",
            crate::meteora_token2022::ADAPTER,
            0,
            1_000_000_000,
            0,
            true,
        ),
        crate::meteora::ORDERS_PROFILE => (
            "meteora",
            crate::meteora::ORDERS_ADAPTER,
            0,
            1_000_000_000,
            0,
            true,
        ),
        crate::meteora::REWARDS_PROFILE => (
            "meteora",
            crate::meteora::REWARDS_ADAPTER,
            0,
            1_000_000_000,
            0,
            true,
        ),
        crate::meteora::BITMAP_PROFILE => (
            "meteora",
            crate::meteora::BITMAP_ADAPTER,
            0,
            1_000_000_000,
            0,
            true,
        ),
        crate::meteora::ONLY_Y_PROFILE => (
            "meteora",
            crate::meteora::ONLY_Y_ADAPTER,
            0,
            1_000_000_000,
            0,
            true,
        ),
        crate::pump_amm::BOOST_PROFILE => (
            "pump-amm",
            crate::pump_amm::BOOST_ADAPTER,
            0,
            10000,
            0,
            true,
        ),
        crate::pump_amm::PROFILE => ("pump-amm", crate::pump_amm::ADAPTER, 0, 10000, 0, true),
        crate::pump::MIGRATION_PROFILE => {
            ("pump", crate::pump::MIGRATION_ADAPTER, 0, 10000, 0, true)
        }
        crate::pump::PROFILE => ("pump", crate::pump::ADAPTER, 0, 10000, 0, true),
        crate::meteora::PROFILE => (
            "meteora",
            crate::meteora::ADAPTER,
            0,
            1_000_000_000,
            0,
            true,
        ),
        crate::lfj::PROFILE => (
            "lfj",
            crate::lfj::ADAPTER,
            0,
            1_000_000_000_000_000_000,
            0,
            true,
        ),
        "aerodrome-slipstream" => ("v3", "aerodrome-slipstream/cl-1", 0, 1000000, 0, true),
        "velodrome-v2-volatile" => (
            "aero",
            "velodrome-v2-volatile/b3065d8b-1",
            0,
            10000,
            0,
            true,
        ),
        "velodrome-v2-stable" => ("aero", "velodrome-v2-stable/b3065d8b-1", 0, 10000, 0, true),
        "aerodrome-v2-volatile" => ("aero", "aerodrome-v2-volatile/pool-1", 0, 10000, 0, true),
        "aerodrome-v2-stable" => ("aero", "aerodrome-v2-stable/pool-1", 0, 10000, 0, true),
        "curve-plain2-basic" => (
            "curve",
            "curve-plain2-basic/factory-5",
            0,
            10000000000,
            0,
            true,
        ),
        "balancer-v2-managed-breaker" => (
            "multi",
            crate::managed_v2_circuit_state::ADAPTER_VERSION,
            0,
            1_000_000_000_000_000_000,
            0,
            true,
        ),
        "balancer-v2-managed-bpt" => (
            "multi",
            crate::managed_v2_bpt_state::ADAPTER_VERSION,
            0,
            1_000_000_000_000_000_000,
            0,
            true,
        ),
        "balancer-v2-managed-scheduled" => (
            "multi",
            crate::managed_v2_schedule::ADAPTER_VERSION,
            0,
            1_000_000_000_000_000_000,
            0,
            true,
        ),
        "balancer-v2-managed-aum" => (
            "multi",
            crate::managed_v2_state::ADAPTER_VERSION,
            0,
            1_000_000_000_000_000_000,
            0,
            true,
        ),
        "balancer-v2-managed-fees" => (
            "multi",
            "balancer-v2-managed-fees/e281ff40-bpt-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "balancer-v2-managed-snapshot" => (
            "multi",
            "balancer-v2-managed-snapshot/no-bpt-fees-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "balancer-v2-composable-regular" => (
            "multi",
            "balancer-v2-composable-regular/no-rates-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "balancer-v2-weighted-multi" => (
            "multi",
            "balancer-v2-weighted-multi/static-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "curve-crypto2-ramp" => (
            "multi",
            "curve-crypto2-ramp/d7d04cd9-ramp-1",
            0,
            10_000_000_000,
            0,
            true,
        ),
        "curve-crypto2-full" => (
            "multi",
            "curve-crypto2-full/d7d04cd9-standard-1",
            0,
            10_000_000_000,
            0,
            true,
        ),
        "curve-metausd-direct" => (
            "multi",
            "curve-metausd-direct/5272fd2b-fixed-base-1",
            0,
            10000000000,
            0,
            true,
        ),
        "curve-stableswap-ng" => (
            "multi",
            "curve-stableswap-ng/3332cd44-standard-1",
            0,
            10000000000,
            0,
            true,
        ),
        "curve-plain3-basic" => (
            "multi",
            "curve-plain3-basic/factory-1",
            0,
            10000000000,
            0,
            true,
        ),
        "curve-plain4-basic" => (
            "multi",
            "curve-plain4-basic/factory-1",
            0,
            10000000000,
            0,
            true,
        ),
        "balancer-v2-weighted2-static" => (
            "weighted",
            "balancer-v2-weighted2/static-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "balancer-v2-weighted2-equal" => (
            "weighted",
            "balancer-v2-weighted2/equal-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "balancer-v2-stable2" => (
            "balancer",
            "balancer-v2-stable2/stable-1",
            0,
            1000000000000000000,
            0,
            true,
        ),
        "uniswap-v2" => ("v2", "uniswap-v2/1", 0, 0, 0, false),
        "sushiswap-v2" => ("v2", "sushiswap-v2/core-1", 3, 1000, 0, true),
        "pancake-v1" => ("v2", "pancake-v1/core-1", 2, 1000, 0, true),
        "pancake-v2" => ("v2", "pancake-v2/core-1", 25, 10000, 0, true),
        "uniswap-v3" => ("v3", "uniswap-v3/core-1", 0, 1000000, 8, true),
        crate::pancake_bsc_lm::PROFILE => (
            "pancake-bsc4",
            crate::pancake_bsc_lm::ADAPTER,
            0,
            1000000,
            32,
            true,
        ),
        crate::pancake_lm::PROFILE => (
            "pancake-lm",
            crate::pancake_lm::ADAPTER,
            0,
            1000000,
            32,
            true,
        ),
        "pancake-v3" => ("v3", "pancake-v3/core-1", 0, 1000000, 32, true),
        _ => return None,
    };
    Some(Profile {
        family,
        adapter,
        fee_numerator: n,
        fee_denominator: d,
        fee_protocol_bits: bits,
        require_identity: identity,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V2State {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap: Option<crate::v2_swap::V2SwapState>,
    pub factory: String,
    pub code_sha256: String,
}
pub fn validate_identity(factory: &str, code: &str) -> Result<()> {
    if factory.len() != 42
        || !factory.starts_with("0x")
        || factory != factory.to_ascii_lowercase()
        || hex::decode(&factory[2..]).is_err()
    {
        return Err(err("POOL_FACTORY_IDENTITY"));
    }
    if code.len() != 64 || code != code.to_ascii_lowercase() || hex::decode(code).is_err() {
        return Err(err("POOL_CODE_IDENTITY"));
    }
    Ok(())
}

pub fn validate_address(address: &str) -> Result<()> {
    if address.len() != 42
        || !address.starts_with("0x")
        || address != address.to_ascii_lowercase()
        || hex::decode(&address[2..]).is_err()
    {
        return Err(err("POOL_ADDRESS_NOT_CANONICAL"));
    }
    Ok(())
}
