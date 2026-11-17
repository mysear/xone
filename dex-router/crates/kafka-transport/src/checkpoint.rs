//! Integrity-checked public checkpoint format. A pinned digest is not owner
//! authorization, freshness, or a Kafka topic-lifecycle proof; callers enforce those.
use crate::{decode_bootstrap, Bootstrap};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use xone_domain::{err, wire, wire_conversion as wc, Result, SnapshotRef};

pub const CODEC: &str = "dex.v1.OwnerBootstrap/protobuf";
pub const MAX_BYTES: usize = 64 << 20;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCursor {
    pub topic: String,
    pub partition: i32,
    pub next_offset: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub codec: String,
    pub reference: SnapshotRef,
    pub cursor: ManifestCursor,
    pub payload: String,
    pub sha256: String,
    pub size: u64,
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && value.bytes().any(|c| c != b'0')
}
/// Authenticate metadata and size bounds before a caller downloads the payload.
pub fn parse_manifest(
    manifest: &[u8],
    expected_manifest: &str,
    max_bytes: usize,
) -> Result<Manifest> {
    if !(1..=MAX_BYTES).contains(&max_bytes) || !digest(expected_manifest) || manifest.len() > 65536
    {
        return Err(err("CHECKPOINT_SIZE_OR_PIN"));
    }
    if hex::encode(Sha256::digest(manifest)) != expected_manifest {
        return Err(err("CHECKPOINT_MANIFEST_CHECKSUM"));
    }
    let m: Manifest =
        serde_json::from_slice(manifest).map_err(|_| err("CHECKPOINT_MANIFEST_INVALID"))?;
    if serde_json::to_vec(&m).map_err(|_| err("CHECKPOINT_MANIFEST_INVALID"))? != manifest
        || m.schema_version != 2
        || m.codec != CODEC
        || m.payload != "bootstrap.pb"
        || !digest(&m.sha256)
        || m.size == 0
        || m.size > max_bytes as u64
    {
        return Err(err("CHECKPOINT_MANIFEST_INVALID"));
    }
    let offset = m
        .cursor
        .next_offset
        .parse::<i64>()
        .map_err(|_| err("CHECKPOINT_CURSOR"))?;
    if offset < 1
        || offset.to_string() != m.cursor.next_offset
        || m.cursor.partition != 0
        || m.cursor.topic.is_empty()
        || m.cursor.topic.len() > 249
        || [".", ".."].contains(&m.cursor.topic.as_str())
        || !m
            .cursor
            .topic
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(err("CHECKPOINT_CURSOR"));
    }
    Ok(m)
}

pub fn decode(
    manifest: &[u8],
    payload: &[u8],
    expected_manifest: &str,
    max_bytes: usize,
) -> Result<Bootstrap> {
    if payload.len() > max_bytes {
        return Err(err("CHECKPOINT_SIZE_OR_PIN"));
    }
    let m = parse_manifest(manifest, expected_manifest, max_bytes)?;
    let offset = m
        .cursor
        .next_offset
        .parse::<i64>()
        .map_err(|_| err("CHECKPOINT_CURSOR"))?;
    if m.size != payload.len() as u64 || hex::encode(Sha256::digest(payload)) != m.sha256 {
        return Err(err("CHECKPOINT_PAYLOAD_CHECKSUM"));
    }
    let boot = decode_bootstrap(payload)?;
    if boot.snapshot.reference != m.reference
        || boot.cursor.topic != m.cursor.topic
        || boot.cursor.partition != m.cursor.partition
        || boot.cursor.offset != offset
    {
        return Err(err("CHECKPOINT_IDENTITY"));
    }
    let canonical = wire::OwnerBootstrap {
        snapshot: Some(wc::snapshot_to_proto(&boot.snapshot)?),
        cursor: Some(wire::JournalCursor {
            topic: boot.cursor.topic.clone(),
            partition: boot.cursor.partition,
            next_offset: boot.cursor.offset,
        }),
    }
    .encode_to_vec();
    if canonical != payload {
        return Err(err("CHECKPOINT_PROTOBUF_NONCANONICAL"));
    }
    Ok(boot)
}
