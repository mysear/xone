use crate::{err, wire, Amount, BlockRef, Result, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SwapFact {
    pub event_id: String,
    pub tx_hash: String,
    pub tx_index: u64,
    pub log_index: u64,
    pub pool_id: String,
    pub token0: String,
    pub token1: String,
    pub amount0_in: Amount,
    pub amount1_in: Amount,
    pub amount0_out: Amount,
    pub amount1_out: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FactBlock {
    pub block: BlockRef,
    pub swaps: Vec<SwapFact>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FactBatch {
    pub schema_version: u32,
    pub decoder_version: String,
    pub blocks: Vec<FactBlock>,
    pub reverted_blocks: Vec<BlockRef>,
}
fn raw_hash(s: &str) -> bool {
    s.starts_with("0x")
        && s == s.to_ascii_lowercase()
        && hex::decode(&s[2..]).is_ok_and(|v| v.len() == 32)
}
fn valid_block(b: &BlockRef, chain: &str) -> bool {
    b.chain_key == chain
        && b.chain_key.starts_with("evm:")
        && raw_hash(&b.hash)
        && raw_hash(&b.parent_hash)
        && b.timestamp_ms > 0
}
impl FactBatch {
    pub fn validate(&self, kind: &str, snapshot: &Snapshot) -> Result<()> {
        let tip = &snapshot.reference.block;
        if self.schema_version != 1
            || self.decoder_version != "uniswap-v2/1"
            || self.blocks.len() + self.reverted_blocks.len() > 100000
        {
            return Err(err("FACT_SCHEMA_OR_LIMIT"));
        }
        if !["BLOCK_COMMIT", "REORG_COMMIT"].contains(&kind) {
            return Err(err("FACT_KIND"));
        }
        if kind == "BLOCK_COMMIT" && (self.blocks.len() != 1 || !self.reverted_blocks.is_empty()) {
            return Err(err("FACT_BLOCK_REQUIRED"));
        }
        let mut seen = BTreeSet::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if !valid_block(&b.block, &tip.chain_key) {
                return Err(err("FACT_BLOCK_IDENTITY"));
            }
            if i > 0 {
                let p = &self.blocks[i - 1].block;
                if b.block.number_or_slot.checked_sub(p.number_or_slot) != Some(1)
                    || b.block.parent_hash != p.hash
                {
                    return Err(err("FACT_BLOCK_SEQUENCE"));
                }
            }
            for (j, s) in b.swaps.iter().enumerate() {
                let id = format!(
                    "{}/{}/{}/{}",
                    b.block.chain_key, b.block.hash, s.tx_index, s.log_index
                );
                if s.event_id != id
                    || !seen.insert(&s.event_id)
                    || !raw_hash(&s.tx_hash)
                    || s.pool_id.is_empty()
                    || s.token0.is_empty()
                    || s.token1.is_empty()
                    || s.token0 == s.token1
                {
                    return Err(err("FACT_EVENT_IDENTITY"));
                }
                if j > 0 {
                    let p = &b.swaps[j - 1];
                    if s.log_index <= p.log_index || s.tx_index < p.tx_index {
                        return Err(err("FACT_EVENT_SEQUENCE"));
                    }
                }
                let pool = snapshot
                    .pools
                    .iter()
                    .find(|p| p.pool_id == s.pool_id)
                    .ok_or_else(|| err("FACT_POOL_SCOPE"))?;
                if pool.protocol != "uniswap-v2"
                    || pool.token0 != s.token0
                    || pool.token1 != s.token1
                {
                    return Err(err("FACT_POOL_SCOPE"));
                }
                for amount in [&s.amount0_in, &s.amount1_in, &s.amount0_out, &s.amount1_out] {
                    amount.to_be_bytes()?;
                }
            }
        }
        if self.blocks.last().is_some_and(|b| &b.block != tip) {
            return Err(err("FACT_TIP_MISMATCH"));
        }
        for (i, b) in self.reverted_blocks.iter().enumerate() {
            if !valid_block(b, &tip.chain_key) {
                return Err(err("FACT_REVERT_IDENTITY"));
            }
            if i > 0 {
                let p = &self.reverted_blocks[i - 1];
                if p.number_or_slot.checked_sub(b.number_or_slot) != Some(1)
                    || p.parent_hash != b.hash
                {
                    return Err(err("FACT_REVERT_SEQUENCE"));
                }
            }
        }
        if kind == "REORG_COMMIT" {
            let last = self
                .reverted_blocks
                .last()
                .ok_or_else(|| err("FACT_REVERT_REQUIRED"))?;
            if let Some(first) = self.blocks.first() {
                if first.block.number_or_slot != last.number_or_slot
                    || first.block.parent_hash != last.parent_hash
                {
                    return Err(err("FACT_REORG_ANCESTOR"));
                }
            } else if tip.hash != last.parent_hash
                || last.number_or_slot.checked_sub(tip.number_or_slot) != Some(1)
            {
                return Err(err("FACT_REORG_ANCESTOR"));
            }
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::FactBatch> {
        let blocks = self
            .blocks
            .iter()
            .map(|b| {
                Ok(wire::FactBlock {
                    block: Some(to_block(&b.block)?),
                    swaps: b
                        .swaps
                        .iter()
                        .map(|s| {
                            Ok(wire::SwapFact {
                                event_id: s.event_id.clone(),
                                tx_hash: hex::decode(s.tx_hash.trim_start_matches("0x"))
                                    .map_err(|_| err("FACT_TX_WIDTH"))?,
                                tx_index: s.tx_index,
                                log_index: s.log_index,
                                pool_id: s.pool_id.clone(),
                                token0: s.token0.clone(),
                                token1: s.token1.clone(),
                                amount0_in: Some(to_amount(&s.amount0_in)?),
                                amount1_in: Some(to_amount(&s.amount1_in)?),
                                amount0_out: Some(to_amount(&s.amount0_out)?),
                                amount1_out: Some(to_amount(&s.amount1_out)?),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(wire::FactBatch {
            schema_version: self.schema_version,
            decoder_version: self.decoder_version.clone(),
            blocks,
            reverted_blocks: self
                .reverted_blocks
                .iter()
                .map(to_block)
                .collect::<Result<Vec<_>>>()?,
        })
    }
    pub fn from_proto(f: wire::FactBatch) -> Result<Self> {
        let blocks = f
            .blocks
            .into_iter()
            .map(|b| {
                Ok(FactBlock {
                    block: from_block(b.block.ok_or_else(|| err("FACT_BLOCK_REQUIRED"))?)?,
                    swaps: b
                        .swaps
                        .into_iter()
                        .map(|s| {
                            if s.tx_hash.len() != 32 {
                                return Err(err("FACT_TX_WIDTH"));
                            }
                            Ok(SwapFact {
                                event_id: s.event_id,
                                tx_hash: format!("0x{}", hex::encode(s.tx_hash)),
                                tx_index: s.tx_index,
                                log_index: s.log_index,
                                pool_id: s.pool_id,
                                token0: s.token0,
                                token1: s.token1,
                                amount0_in: from_amount(s.amount0_in)?,
                                amount1_in: from_amount(s.amount1_in)?,
                                amount0_out: from_amount(s.amount0_out)?,
                                amount1_out: from_amount(s.amount1_out)?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            schema_version: f.schema_version,
            decoder_version: f.decoder_version,
            blocks,
            reverted_blocks: f
                .reverted_blocks
                .into_iter()
                .map(from_block)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}
fn to_amount(a: &Amount) -> Result<wire::Amount> {
    Ok(wire::Amount {
        uint256_be: a.to_be_bytes()?.to_vec(),
    })
}
fn from_amount(a: Option<wire::Amount>) -> Result<Amount> {
    Amount::from_be_bytes(&a.ok_or_else(|| err("FACT_AMOUNT_REQUIRED"))?.uint256_be)
}
fn to_block(b: &BlockRef) -> Result<wire::BlockRef> {
    Ok(wire::BlockRef {
        chain_key: b.chain_key.clone(),
        number_or_slot: b.number_or_slot,
        hash: hex::decode(b.hash.trim_start_matches("0x")).map_err(|_| err("FACT_HASH_WIDTH"))?,
        parent_hash: hex::decode(b.parent_hash.trim_start_matches("0x"))
            .map_err(|_| err("FACT_HASH_WIDTH"))?,
        timestamp_ms: b.timestamp_ms,
    })
}
fn from_block(b: wire::BlockRef) -> Result<BlockRef> {
    if b.hash.len() != 32 || b.parent_hash.len() != 32 {
        return Err(err("FACT_HASH_WIDTH"));
    }
    Ok(BlockRef {
        chain_key: b.chain_key,
        number_or_slot: b.number_or_slot,
        hash: format!("0x{}", hex::encode(b.hash)),
        parent_hash: format!("0x{}", hex::encode(b.parent_hash)),
        timestamp_ms: b.timestamp_ms,
    })
}
