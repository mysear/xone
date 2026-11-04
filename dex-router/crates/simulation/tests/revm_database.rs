use revm::{
    primitives::{keccak256, AccountInfo, Address, Bytes, B256, KECCAK_EMPTY, U256},
    Database,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use xone_simulation::revm_backend::*;
fn address() -> Address {
    Address::repeat_byte(0x22)
}
fn data() -> ImageData {
    let mut data = ImageData::default();
    data.accounts.insert(
        address(),
        Some(AccountInfo {
            balance: U256::from(12),
            nonce: 3,
            code_hash: KECCAK_EMPTY,
            code: None,
        }),
    );
    data
}
fn db(data: ImageData, reads: u64, bytes: u64) -> StrictDB {
    StrictDB::new(
        Arc::new(StateImage::new(data).unwrap()),
        BoundReadLimits {
            max_reads: reads,
            max_bytes: bytes,
        },
        ExecutionControl::default(),
    )
}
#[test]
fn unknown_accounts_fail_and_declared_absence_is_none() {
    let a = Address::repeat_byte(0x44);
    let mut d = data();
    d.accounts.insert(a, None);
    let mut db = db(d, 10, 1000);
    assert_eq!(db.basic(a).unwrap(), None);
    assert_eq!(
        db.basic(Address::ZERO),
        Err(DbError::UnknownAccount(Address::ZERO))
    );
    assert_eq!(db.usage().basic, 2);
}
#[test]
fn explicit_zero_is_distinct_from_unknown_storage_even_for_absent_accounts() {
    let mut d = data();
    d.storage.insert((address(), U256::ZERO), U256::ZERO);
    d.accounts.insert(Address::ZERO, None);
    let mut db = db(d, 20, 5000);
    assert_eq!(db.storage(address(), U256::ZERO).unwrap(), U256::ZERO);
    for a in [address(), Address::ZERO, Address::repeat_byte(0x77)] {
        assert!(matches!(
            db.storage(a, U256::from(9)),
            Err(DbError::UnknownStorage { .. })
        ));
    }
}
#[test]
fn code_requires_declared_hash_and_verifies_keccak() {
    let mut d = data();
    let bytes = Bytes::from_static(&[0x60, 0x01, 0x00]);
    let hash = keccak256(&bytes);
    d.bytecodes.insert(hash, bytes.clone());
    let mut db = db(d, 10, 1000);
    assert_eq!(db.code_by_hash(hash).unwrap().original_bytes(), bytes);
    assert_eq!(
        db.code_by_hash(B256::ZERO),
        Err(DbError::UnknownCode(B256::ZERO))
    );
    assert_eq!(
        db.code_by_hash(KECCAK_EMPTY),
        Err(DbError::UnknownCode(KECCAK_EMPTY))
    );
}
#[test]
fn forged_code_hash_is_rejected_on_image_construction() {
    let mut d = data();
    d.bytecodes.insert(B256::ZERO, Bytes::from_static(&[0x00]));
    assert!(matches!(
        StateImage::new(d),
        Err(ImageError::CodeHashMismatch { .. })
    ));
}
#[test]
fn inline_code_cannot_bypass_hash_lookup_or_byte_meter() {
    let mut d = data();
    d.accounts
        .get_mut(&address())
        .unwrap()
        .as_mut()
        .unwrap()
        .code = Some(revm::primitives::Bytecode::default());
    assert!(matches!(
        StateImage::new(d),
        Err(ImageError::InlineCode { .. })
    ));
}
#[test]
fn undeclared_blockhash_fails_and_declared_zero_is_preserved() {
    let mut d = data();
    d.block_hashes.insert(5, B256::ZERO);
    let mut db = db(d, 10, 1000);
    assert_eq!(db.block_hash(5).unwrap(), B256::ZERO);
    assert_eq!(db.block_hash(4), Err(DbError::UnknownBlockHash(4)));
}
#[test]
fn every_attempt_counts_and_success_payload_bytes_are_exact() {
    let mut d = data();
    d.accounts.insert(Address::ZERO, None);
    d.storage.insert((address(), U256::ZERO), U256::ZERO);
    d.bytecodes.insert(KECCAK_EMPTY, Bytes::new());
    d.block_hashes.insert(5, B256::ZERO);
    let mut db = db(d, 10, 1000);
    db.basic(address()).unwrap(); // 20 key + 1 presence + 32 balance + 8 nonce + 32 hash
    db.basic(Address::ZERO).unwrap(); // 20 + 1
    db.code_by_hash(KECCAK_EMPTY).unwrap(); // 32 + 0
    db.storage(address(), U256::ZERO).unwrap(); // 20 + 32 + 32
    db.block_hash(5).unwrap(); // 8 + 32
    assert!(db.basic(Address::repeat_byte(0x99)).is_err()); // missing: key only
    assert_eq!(
        db.usage(),
        ReadUsage {
            reads: 6,
            bytes: 93 + 21 + 32 + 84 + 40 + 20,
            basic: 3,
            code: 1,
            storage: 1,
            block_hash: 1
        }
    );
}
#[test]
fn read_and_byte_limits_are_sticky_and_do_not_succeed_after_exhaustion() {
    let mut reads = db(data(), 1, 1000);
    reads.basic(address()).unwrap();
    assert_eq!(reads.basic(address()), Err(DbError::ReadLimit));
    assert_eq!(reads.block_hash(0), Err(DbError::ReadLimit));
    assert_eq!(reads.usage().reads, 2); // failing limit attempt counted, no further reads admitted
    let mut bytes = db(data(), 100, 92);
    assert_eq!(bytes.basic(address()), Err(DbError::ByteLimit));
    assert_eq!(bytes.code_by_hash(KECCAK_EMPTY), Err(DbError::ByteLimit));
    assert_eq!(bytes.usage().bytes, 93);
}
#[test]
fn db_cancellation_and_deadline_are_typed_errors() {
    let flag = Arc::new(AtomicBool::new(true));
    let image = Arc::new(StateImage::new(data()).unwrap());
    let limits = BoundReadLimits {
        max_reads: 10,
        max_bytes: 1000,
    };
    let mut cancelled = StrictDB::new(
        image.clone(),
        limits,
        ExecutionControl {
            cancellation: flag.clone(),
            deadline: None,
            progress: None,
        },
    );
    assert_eq!(cancelled.basic(address()), Err(DbError::Cancelled));
    flag.store(false, Ordering::SeqCst);
    assert_eq!(cancelled.basic(address()), Err(DbError::Cancelled));
    let mut expired = StrictDB::new(
        image,
        limits,
        ExecutionControl {
            deadline: Some(std::time::Instant::now()),
            ..ExecutionControl::default()
        },
    );
    assert_eq!(expired.block_hash(0), Err(DbError::Deadline));
}
#[test]
fn invalid_storage_owner_and_noncanonical_empty_hash_are_rejected() {
    let mut d = data();
    d.storage.insert((Address::ZERO, U256::ZERO), U256::ZERO);
    assert!(matches!(
        StateImage::new(d),
        Err(ImageError::StorageWithoutAccount { .. })
    ));
    let mut d = data();
    d.accounts
        .get_mut(&address())
        .unwrap()
        .as_mut()
        .unwrap()
        .code_hash = B256::ZERO;
    assert!(matches!(
        StateImage::new(d),
        Err(ImageError::NonCanonicalCodeHash { .. })
    ));
}
