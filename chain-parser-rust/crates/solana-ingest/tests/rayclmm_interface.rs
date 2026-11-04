use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{Pool, Result};
use xone_solana_ingest::raydium::RpcAccount;
use xone_solana_ingest::raydium_clmm::{decode_raydium_clmm, required_accounts};

#[test]
fn decoder_exposes_the_authoritative_interface() {
    let _: fn(&PoolConfig) -> Result<Vec<String>> = required_accounts;
    let _: fn(&Chain, &PoolConfig, &BTreeMap<String, RpcAccount>, u64) -> Result<Pool> =
        decode_raydium_clmm;
}
