use xone_domain::singleton::{SingletonAdvancedState, DYNAMIC_FEE_FLAG};
#[test]
fn frozen_advanced_identity_and_capability_boundary() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-quotes.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let s: SingletonAdvancedState = serde_json::from_value(case["snapshot"].clone()).unwrap();
        s.validate().unwrap();
        let mut stale = s.clone();
        stale.snapshot.lp_fee = (stale.snapshot.lp_fee + 1) % 1_000_001;
        assert_eq!(stale.validate().is_ok(), s.key_fee == DYNAMIC_FEE_FLAG);
        let mut callback = s.clone();
        if s.protocol == "uniswap-v4" {
            callback.snapshot.hooks = "0x0000000000000000000000000000000000010080".into();
        } else {
            callback.snapshot.parameters =
                "0x00000000000000000000000000000000000000000000000000000000003c0040".into();
        }
        assert!(callback.validate().is_err());
        let mut missing = s.clone();
        missing.snapshot_block_hash.clear();
        assert!(missing.validate().is_err());
        // Legacy validation must continue rejecting dynamic keys and native/nonzero hook identities.
        if s.key_fee == DYNAMIC_FEE_FLAG
            || s.currency0
                .ends_with("0000000000000000000000000000000000000000")
        {
            assert!(s
                .snapshot
                .validate(&s.protocol, &s.currency0, &s.currency1)
                .is_err());
        }
    }
}

#[test]
fn advanced_identity_rejects_unexecuted_callbacks_and_ambiguous_fee_flags() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-quotes.json"
    ))
    .unwrap();
    for protocol in ["uniswap-v4", "pancake-infinity-cl"] {
        let value = f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["snapshot"]["protocol"] == protocol
                    && c["snapshot"]["key_fee"] == DYNAMIC_FEE_FLAG
            })
            .unwrap();
        let base: SingletonAdvancedState =
            serde_json::from_value(value["snapshot"].clone()).unwrap();
        for flag in if protocol == "uniswap-v4" {
            vec![0x80u16, 0x40, 8, 4]
        } else {
            vec![0x40u16, 0x80, 0x400, 0x800]
        } {
            let mut bad = base.clone();
            if protocol == "uniswap-v4" {
                bad.snapshot.hooks = format!("0x{:040x}", 0x10000u32 + u32::from(flag));
            } else {
                bad.snapshot.parameters = format!("0x{:064x}", (60u32 << 16) | u32::from(flag));
            }
            bad.snapshot.pool_id = bad.key_id().unwrap();
            assert_eq!(
                bad.validate().unwrap_err().0,
                "SINGLETON_HOOK_EXECUTION_REQUIRED"
            );
        }
        for key_fee in [0x800001, 0x400000, 1_000_001] {
            let mut bad = base.clone();
            bad.key_fee = key_fee;
            assert!(bad.key_id().is_err());
        }
        let mut no_hook = base.clone();
        no_hook.snapshot.hooks = "0x0000000000000000000000000000000000000000".into();
        no_hook.snapshot.pool_id = no_hook.key_id().unwrap();
        assert!(no_hook.validate().is_err());
        let mut bad = base.clone();
        bad.snapshot.lp_fee = 1_000_001;
        assert!(bad.validate().is_err());
        let mut bad = base.clone();
        bad.snapshot_block_hash = format!("0x{}", "00".repeat(32));
        assert!(bad.validate().is_err());
    }
}

#[test]
fn advanced_proto_roundtrip_does_not_use_legacy_hook_validation() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-quotes.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let s: SingletonAdvancedState = serde_json::from_value(case["snapshot"].clone()).unwrap();
        let wire = s.to_proto().unwrap();
        assert_eq!(wire.snapshot_block_hash.len(), 32);
        assert_eq!(SingletonAdvancedState::from_proto(wire.clone()).unwrap(), s);
        let mut bad = wire.clone();
        bad.snapshot_block_hash.clear();
        assert!(SingletonAdvancedState::from_proto(bad).is_err());
        let mut bad = wire.clone();
        bad.snapshot = None;
        assert!(SingletonAdvancedState::from_proto(bad).is_err());
        let mut bad = wire;
        bad.snapshot.as_mut().unwrap().pool_id.clear();
        assert!(SingletonAdvancedState::from_proto(bad).is_err());
    }
}
