use xone_solana_ingest::assembly::*;
#[test]
fn source_sessions_and_dependency_barriers() {
    let mut a = Assembler::new(8);
    a.begin(10, "bank-a", vec!["pool".into(), "vault".into()])
        .unwrap();
    assert!(a
        .update(Update {
            provider: "p".into(),
            session: "s1".into(),
            slot: 10,
            bank: "bank-a".into(),
            pubkey: "pool".into(),
            write_version: 99,
            data: vec![1]
        })
        .unwrap());
    assert!(!a.ready());
    assert!(a
        .update(Update {
            provider: "p".into(),
            session: "s2".into(),
            slot: 10,
            bank: "bank-a".into(),
            pubkey: "vault".into(),
            write_version: 1,
            data: vec![2]
        })
        .is_err());
    a.disconnect();
    assert!(!a.ready());
    a.begin(11, "bank-b", vec!["pool".into()]).unwrap();
    a.update(Update {
        provider: "p".into(),
        session: "s2".into(),
        slot: 11,
        bank: "bank-b".into(),
        pubkey: "pool".into(),
        write_version: 1,
        data: vec![1],
    })
    .unwrap();
    assert!(!a.ready());
    a.complete_accounts(11, "bank-b").unwrap();
    assert!(a.ready());
}
