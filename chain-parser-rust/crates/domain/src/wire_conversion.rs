use crate::{err, wire, Amount, BlockRef, Gap, Pool, Result, Snapshot, SnapshotRef};
use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
fn digest(s: &str) -> Result<Vec<u8>> {
    let raw =
        hex::decode(s.strip_prefix("0x").unwrap_or(s)).map_err(|_| err("WIRE_DIGEST_ENCODING"))?;
    if raw.len() != 32 {
        return Err(err("WIRE_DIGEST_WIDTH"));
    }
    Ok(raw)
}
fn decode58(s: &str) -> Result<Vec<u8>> {
    if !(32..=44).contains(&s.len()) {
        return Err(err("BLOCK_HASH_WIDTH"));
    }
    let mut n = BigUint::zero();
    for ch in s.bytes() {
        let digit = B58
            .iter()
            .position(|v| *v == ch)
            .ok_or_else(|| err("BLOCK_HASH_ENCODING"))?;
        n = n * 58u32 + BigUint::from(digit);
    }
    let zeros = s.bytes().take_while(|v| *v == b'1').count();
    let raw = if n.is_zero() {
        Vec::new()
    } else {
        n.to_bytes_be()
    };
    if zeros + raw.len() != 32 {
        return Err(err("BLOCK_HASH_WIDTH"));
    }
    let mut bytes = vec![0; zeros];
    bytes.extend(raw);
    Ok(bytes)
}
fn encode58(raw: &[u8]) -> Result<String> {
    let mut n = BigUint::from_bytes_be(raw);
    let mut reversed = Vec::new();
    while !n.is_zero() {
        let digit = (&n % 58u32)
            .to_usize()
            .ok_or_else(|| err("BLOCK_HASH_ENCODING"))?;
        reversed.push(B58[digit]);
        n /= 58u32;
    }
    reversed.extend(std::iter::repeat_n(
        b'1',
        raw.iter().take_while(|v| **v == 0).count(),
    ));
    reversed.reverse();
    String::from_utf8(reversed).map_err(|_| err("BLOCK_HASH_ENCODING"))
}
fn block_hash(chain: &str, s: &str) -> Result<Vec<u8>> {
    if chain.starts_with("evm:") {
        digest(s)
    } else if chain.starts_with("solana:") {
        decode58(s)
    } else {
        Err(err("INVALID_CHAIN_KEY"))
    }
}
pub fn ref_to_proto(r: &SnapshotRef) -> Result<wire::SnapshotRef> {
    Ok(wire::SnapshotRef {
        block: Some(wire::BlockRef {
            chain_key: r.block.chain_key.clone(),
            number_or_slot: r.block.number_or_slot,
            hash: block_hash(&r.block.chain_key, &r.block.hash)?,
            parent_hash: block_hash(&r.block.chain_key, &r.block.parent_hash)?,
            timestamp_ms: r.block.timestamp_ms,
        }),
        canonical_epoch: r.canonical_epoch,
        journal_seq: r.journal_seq,
        schema_version: r.schema_version,
        adapter_set_hash: digest(&r.adapter_set_hash)?,
        online_generation: r.online_generation,
        scope_hash: digest(&r.scope_hash)?,
        writer_epoch: r.writer_epoch,
        owner_service: r.owner_service.clone(),
        owner_assignment_version: r.owner_assignment_version,
    })
}
pub fn ref_from_proto(r: wire::SnapshotRef) -> Result<SnapshotRef> {
    let b = r.block.ok_or_else(|| err("WIRE_REFERENCE_REQUIRED"))?;
    if b.hash.len() != 32
        || b.parent_hash.len() != 32
        || r.adapter_set_hash.len() != 32
        || r.scope_hash.len() != 32
    {
        return Err(err("WIRE_DIGEST_WIDTH"));
    }
    crate::validate_chain(&b.chain_key)?;
    let (hash, parent_hash) = if b.chain_key.starts_with("evm:") {
        (
            format!("0x{}", hex::encode(b.hash)),
            format!("0x{}", hex::encode(b.parent_hash)),
        )
    } else {
        (encode58(&b.hash)?, encode58(&b.parent_hash)?)
    };
    Ok(SnapshotRef {
        block: BlockRef {
            chain_key: b.chain_key,
            number_or_slot: b.number_or_slot,
            hash,
            parent_hash,
            timestamp_ms: b.timestamp_ms,
        },
        canonical_epoch: r.canonical_epoch,
        journal_seq: r.journal_seq,
        schema_version: r.schema_version,
        adapter_set_hash: hex::encode(r.adapter_set_hash),
        online_generation: r.online_generation,
        scope_hash: hex::encode(r.scope_hash),
        writer_epoch: r.writer_epoch,
        owner_service: r.owner_service,
        owner_assignment_version: r.owner_assignment_version,
    })
}
pub fn snapshot_to_proto(s: &Snapshot) -> Result<wire::OnlineSnapshot> {
    let pools = s
        .pools
        .iter()
        .map(|p| {
            Ok(wire::PoolState {
                pancake_lm: p.pancake_lm.as_ref().map(|s| s.to_proto()).transpose()?,
                pancake_bsc_lm: p
                    .pancake_bsc_lm
                    .as_ref()
                    .map(|s| s.to_proto())
                    .transpose()?,
                slipstream_context: p
                    .slipstream_context
                    .as_ref()
                    .map(|s| s.to_proto())
                    .transpose()?,
                pump: p.pump.as_ref().map(|s| s.to_proto()).transpose()?,
                meteora: p.meteora.as_ref().map(|s| s.to_proto()).transpose()?,
                lfj: p.lfj.as_ref().map(|s| s.to_proto()).transpose()?,
                infinity_bin: p.infinity_bin.as_ref().map(|b| b.to_proto()).transpose()?,
                raydium_clmm: p.raydium_clmm.as_ref().map(|s| s.to_proto()).transpose()?,
                orca_whirlpool: p
                    .orca_whirlpool
                    .as_ref()
                    .map(|s| s.to_proto())
                    .transpose()?,
                raydium_cpmm: p.raydium_cpmm.as_ref().map(|s| s.to_proto()).transpose()?,
                singleton_cl: p.singleton_cl.as_ref().map(|v| v.to_proto()).transpose()?,
                singleton_advanced: p
                    .singleton_advanced
                    .as_ref()
                    .map(|s| s.to_proto())
                    .transpose()?,
                multi_asset: p.multi_asset.as_ref().map(|s| s.to_proto()).transpose()?,
                weighted: p.weighted.as_ref().map(|s| s.to_proto()).transpose()?,
                stable: p.stable.as_ref().map(|s| s.to_proto()).transpose()?,
                v2: p
                    .v2
                    .as_ref()
                    .map(|v| {
                        Ok(wire::V2State {
                            swap: v
                                .swap
                                .as_ref()
                                .map(crate::v2_swap::V2SwapState::to_proto)
                                .transpose()?,
                            factory: v.factory.clone(),
                            code_sha256: hex::decode(&v.code_sha256)
                                .map_err(|_| err("POOL_CODE_IDENTITY"))?,
                        })
                    })
                    .transpose()?,
                v3: p.v3.as_ref().map(|v| v.to_proto()).transpose()?,
                chain_key: p.chain_key.clone(),
                pool_id: p.pool_id.clone(),
                protocol: p.protocol.clone(),
                token0: p.token0.clone(),
                token1: p.token1.clone(),
                reserve0: Some(wire::Amount {
                    uint256_be: p.reserve0.to_be_bytes()?.to_vec(),
                }),
                reserve1: Some(wire::Amount {
                    uint256_be: p.reserve1.to_be_bytes()?.to_vec(),
                }),
                fee_numerator: p.fee_numerator,
                fee_denominator: p.fee_denominator,
                adapter_version: p.adapter_version.clone(),
                token_behavior: p.token_behavior.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(wire::OnlineSnapshot {
        reference: Some(ref_to_proto(&s.reference)?),
        pools,
        source: s.source.clone(),
        history_gaps: s
            .history_gaps
            .iter()
            .map(|g| wire::HistoryGap {
                from: g.from,
                to: g.to,
                reason: g.reason.clone(),
            })
            .collect(),
    })
}
pub fn snapshot_from_proto(s: wire::OnlineSnapshot) -> Result<Snapshot> {
    let reference = ref_from_proto(s.reference.ok_or_else(|| err("WIRE_REFERENCE_REQUIRED"))?)?;
    let pools = s
        .pools
        .into_iter()
        .map(|p| {
            Ok(Pool {
                pancake_bsc_lm: p
                    .pancake_bsc_lm
                    .map(crate::pancake_bsc_lm::PancakeBscLmState::from_proto)
                    .transpose()?,
                pancake_lm: p
                    .pancake_lm
                    .map(crate::pancake_lm::PancakeLmState::from_proto)
                    .transpose()?,
                slipstream_context: p
                    .slipstream_context
                    .map(crate::slipstream_context::SlipstreamState::from_proto)
                    .transpose()?,
                pump: p.pump.map(crate::pump::PumpState::from_proto).transpose()?,
                meteora: p
                    .meteora
                    .map(crate::meteora::MeteoraState::from_proto)
                    .transpose()?,
                lfj: p.lfj.map(crate::lfj::LfjState::from_proto).transpose()?,
                infinity_bin: p
                    .infinity_bin
                    .map(crate::infinity_bin::InfinityBinState::from_proto)
                    .transpose()?,
                raydium_clmm: p
                    .raydium_clmm
                    .map(crate::raydium_clmm::RaydiumCLMMState::from_proto)
                    .transpose()?,
                orca_whirlpool: p
                    .orca_whirlpool
                    .map(crate::orca::OrcaWhirlpoolState::from_proto)
                    .transpose()?,
                raydium_cpmm: p
                    .raydium_cpmm
                    .map(crate::raydium::RaydiumCPMMState::from_proto)
                    .transpose()?,
                singleton_cl: p
                    .singleton_cl
                    .map(crate::singleton::SingletonCLState::from_proto)
                    .transpose()?,
                singleton_advanced: p
                    .singleton_advanced
                    .map(crate::singleton::SingletonAdvancedState::from_proto)
                    .transpose()?,
                multi_asset: p
                    .multi_asset
                    .map(crate::multi_asset::MultiAssetState::from_proto)
                    .transpose()?,
                weighted: p
                    .weighted
                    .map(crate::weighted::WeightedState::from_proto)
                    .transpose()?,
                stable: p
                    .stable
                    .map(crate::stable::StableState::from_proto)
                    .transpose()?,
                v2: p
                    .v2
                    .map(|v| {
                        if v.code_sha256.len() != 32 {
                            return Err(err("POOL_CODE_IDENTITY"));
                        }
                        Ok(crate::protocols::V2State {
                            swap: v
                                .swap
                                .map(crate::v2_swap::V2SwapState::from_proto)
                                .transpose()?,
                            factory: v.factory,
                            code_sha256: hex::encode(v.code_sha256),
                        })
                    })
                    .transpose()?,
                v3: p.v3.map(crate::v3::V3State::from_proto).transpose()?,
                chain_key: p.chain_key,
                pool_id: p.pool_id,
                protocol: p.protocol,
                token0: p.token0,
                token1: p.token1,
                reserve0: Amount::from_be_bytes(
                    &p.reserve0
                        .ok_or_else(|| err("WIRE_AMOUNT_REQUIRED"))?
                        .uint256_be,
                )?,
                reserve1: Amount::from_be_bytes(
                    &p.reserve1
                        .ok_or_else(|| err("WIRE_AMOUNT_REQUIRED"))?
                        .uint256_be,
                )?,
                fee_numerator: p.fee_numerator,
                fee_denominator: p.fee_denominator,
                adapter_version: p.adapter_version,
                token_behavior: p.token_behavior,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let snapshot = Snapshot {
        reference,
        pools,
        source: s.source,
        history_gaps: s
            .history_gaps
            .into_iter()
            .map(|g| Gap {
                from: g.from,
                to: g.to,
                reason: g.reason,
            })
            .collect(),
    };
    snapshot.validate(
        &snapshot.reference.owner_service,
        snapshot.reference.owner_assignment_version,
    )?;
    Ok(snapshot)
}
