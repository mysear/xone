use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use xone_domain::{wire, wire_conversion as wc, Snapshot, SnapshotRef};
use xone_kafka_transport::{
    checkpoint::{self, Manifest, ManifestCursor},
    Cursor,
};
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn fixture() -> (Manifest, Vec<u8>) {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let payload = wire::OwnerBootstrap {
        snapshot: Some(wc::snapshot_to_proto(&snapshot).unwrap()),
        cursor: Some(wire::JournalCursor {
            topic: "canonical.evm-31337.v1".into(),
            partition: 0,
            next_offset: 9_007_199_254_741_009,
        }),
    }
    .encode_to_vec();
    (
        Manifest {
            schema_version: 2,
            codec: checkpoint::CODEC.into(),
            reference: snapshot.reference,
            cursor: ManifestCursor {
                topic: "canonical.evm-31337.v1".into(),
                partition: 0,
                next_offset: "9007199254741009".into(),
            },
            payload: "bootstrap.pb".into(),
            sha256: sha(&payload),
            size: payload.len() as u64,
        },
        payload,
    )
}
#[test]
fn checkpoint_roundtrip_preserves_wide_offset_and_source_reference() {
    let (m, data) = fixture();
    let raw = serde_json::to_vec(&m).unwrap();
    let boot = checkpoint::decode(&raw, &data, &sha(&raw), 8 << 20).unwrap();
    assert_eq!(boot.cursor.offset, 9_007_199_254_741_009);
    assert_eq!(boot.snapshot.reference, m.reference);
}
#[test]
fn checkpoint_limits_pins_and_noncanonical_metadata_cannot_expose_state() {
    let (m, payload) = fixture();
    let raw = serde_json::to_vec(&m).unwrap();
    for limit in [0, checkpoint::MAX_BYTES + 1, usize::MAX, payload.len() - 1] {
        assert!(checkpoint::decode(&raw, &payload, &sha(&raw), limit).is_err());
    }
    assert!(checkpoint::decode(&raw, &payload, &"a".repeat(64), 8 << 20).is_err());
    for mode in 0..10 {
        let mut changed = m.clone();
        let mut data = payload.clone();
        match mode {
            0 => changed.cursor.next_offset = "01".into(),
            1 => changed.cursor.partition = 1,
            2 => changed.cursor.topic = "../escape".into(),
            3 => changed.payload = "../escape".into(),
            4 => changed.reference.journal_seq += 1,
            5 => changed.codec = "unknown".into(),
            6 => {
                data[0] ^= 1;
            }
            7 => {
                data.extend([0x98, 0x01, 0x01]);
                changed.sha256 = sha(&data);
                changed.size = data.len() as u64;
            }
            8 => changed.cursor.next_offset = "9007199254741010".into(),
            _ => changed.sha256 = "0".repeat(64),
        };
        let manifest = serde_json::to_vec(&changed).unwrap();
        assert!(
            checkpoint::decode(&manifest, &data, &sha(&manifest), 8 << 20).is_err(),
            "{mode}"
        );
    }
    let mut whitespace = raw.clone();
    whitespace.push(b'\n');
    assert!(checkpoint::decode(&whitespace, &payload, &sha(&whitespace), 8 << 20).is_err());
    let mut duplicate = raw[..raw.len() - 1].to_vec();
    duplicate.extend_from_slice(format!(",\"size\":{}}}", payload.len()).as_bytes());
    assert!(checkpoint::decode(&duplicate, &payload, &sha(&duplicate), 8 << 20).is_err());
}
#[test]
#[ignore = "requires the current validation run's independently exported Go checkpoints"]
fn current_go_checkpoint_vectors_match_reference_cursor_and_full_width_amounts() {
    let directory = std::env::var("XONE_CHECKPOINT_VECTORS_OUTPUT")
        .expect("current Go export directory required");
    let document: Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(&directory).join("vectors.json")).unwrap(),
    )
    .unwrap();
    let vectors = document["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 3);
    for (i, v) in vectors.iter().enumerate() {
        let manifest = v["manifest"].as_str().unwrap().as_bytes();
        let payload = hex::decode(v["payload_hex"].as_str().unwrap()).unwrap();
        let boot = checkpoint::decode(
            manifest,
            &payload,
            v["manifest_sha256"].as_str().unwrap(),
            8 << 20,
        )
        .unwrap();
        let reference: SnapshotRef = serde_json::from_value(v["reference"].clone()).unwrap();
        let cursor: Cursor = serde_json::from_value(v["cursor"].clone()).unwrap();
        assert_eq!(boot.snapshot.reference, reference);
        assert_eq!(boot.cursor, cursor);
        if i == 1 {
            assert_eq!(boot.cursor.offset, i64::MAX - 1);
            assert_eq!(boot.snapshot.reference.journal_seq, (1u64 << 63) + 7);
        }
        if i == 2 {
            assert!(boot.snapshot.pools[0].reserve0.0.bits() > 128);
        }
        let mut changed = payload;
        changed[0] ^= 1;
        assert!(checkpoint::decode(
            manifest,
            &changed,
            v["manifest_sha256"].as_str().unwrap(),
            8 << 20
        )
        .is_err());
    }
}
