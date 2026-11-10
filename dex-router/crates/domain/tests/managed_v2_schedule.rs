use num_bigint::BigUint;
use xone_domain::{
    managed_v2_schedule::{ManagedV2Schedule, ManagedV2ScheduledToken, ADAPTER_VERSION, PROTOCOL},
    Amount,
};

const ONE: u64 = 1_000_000_000_000_000_000;
const MIN: u64 = 10_000_000_000_000_000;
const FEE_MIN: u64 = 1_000_000_000_000;
const FEE_MAX: u64 = 950_000_000_000_000_000;
fn amount(v: u64) -> Amount {
    Amount(BigUint::from(v))
}
fn token(start: u64, end: u64, diff: u64, index: usize) -> ManagedV2ScheduledToken {
    ManagedV2ScheduledToken {
        token: format!("0x{index:040x}"),
        state_word: Amount(
            BigUint::from(start)
                | (BigUint::from(end) << 64usize)
                | (BigUint::from(diff) << 128usize),
        ),
    }
}
fn pool(ws: u64, we: u64, fs: u64, fe: u64, a: u64, b: u64, flags: u64) -> Amount {
    Amount(
        BigUint::from(ws)
            | (BigUint::from(we) << 32usize)
            | (BigUint::from(fs) << 64usize)
            | (BigUint::from(fe) << 96usize)
            | (BigUint::from(a) << 128usize)
            | (BigUint::from(b) << 190usize)
            | (BigUint::from(flags) << 252usize),
    )
}
fn schedule() -> ManagedV2Schedule {
    ManagedV2Schedule {
        pool_state_word: pool(100, 2_592_100, 100, 2_592_100, ONE / 100, 3 * ONE / 100, 9),
        tokens: vec![
            token(8 * ONE / 10, 2 * ONE / 10, 0, 1),
            token(2 * ONE / 10, 8 * ONE / 10, 12, 2),
        ],
    }
}
fn weights(s: &ManagedV2Schedule, t: u64) -> Vec<Amount> {
    s.at(t).unwrap().weights
}
fn reject(s: ManagedV2Schedule) {
    assert!(s.validate().is_err());
    assert!(s.at(101).is_err());
}

#[test]
fn api_and_json_required_nonnull_strict() {
    assert_eq!(PROTOCOL, "balancer-v2-managed-scheduled");
    assert_eq!(ADAPTER_VERSION, "balancer-v2/20230411-managed-scheduled-1");
    let s = schedule();
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 2);
    assert_eq!(
        serde_json::from_value::<ManagedV2Schedule>(v.clone()).unwrap(),
        s
    );
    for field in ["pool_state_word", "tokens"] {
        let mut missing = v.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ManagedV2Schedule>(missing).is_err());
        let mut null = v.clone();
        null[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ManagedV2Schedule>(null).is_err());
    }
    for field in ["token", "state_word"] {
        let mut missing = v.clone();
        missing["tokens"][0].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ManagedV2Schedule>(missing).is_err());
        let mut null = v.clone();
        null["tokens"][0][field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ManagedV2Schedule>(null).is_err());
    }
    for nested in [false, true] {
        let mut extra = v.clone();
        if nested {
            extra["tokens"][0]["extra"] = true.into();
        } else {
            extra["extra"] = true.into();
        }
        assert!(serde_json::from_value::<ManagedV2Schedule>(extra).is_err());
    }
    for invalid in [
        serde_json::json!(0),
        serde_json::json!("-1"),
        serde_json::json!("01"),
        serde_json::json!((BigUint::from(1u8) << 256usize).to_string()),
    ] {
        for nested in [false, true] {
            let mut bad = v.clone();
            if nested {
                bad["tokens"][0]["state_word"] = invalid.clone();
            } else {
                bad["pool_state_word"] = invalid.clone();
            }
            assert!(serde_json::from_value::<ManagedV2Schedule>(bad).is_err());
        }
    }
}

#[test]
fn original_two_token_exact_rounding_and_clamps() {
    let s = schedule();
    s.validate().unwrap();
    assert_eq!(
        weights(&s, 99),
        vec![amount(8 * ONE / 10), amount(2 * ONE / 10)]
    );
    assert_eq!(weights(&s, 100), weights(&s, 99));
    let v = s.at(101).unwrap();
    assert_eq!(
        v.weights,
        vec![amount(799999768518518519), amount(200000231481481481)]
    );
    assert_eq!(v.swap_fee, amount(10000007716049382));
    let v = s.at(2_592_099).unwrap();
    assert_eq!(
        v.weights,
        vec![amount(200000231481481482), amount(799999768518518518)]
    );
    assert_eq!(v.swap_fee, amount(29999992283950617));
    for t in [
        2_592_100,
        2_592_101,
        u64::from(u32::MAX) + 1,
        u64::MAX / 1000,
    ] {
        let v = s.at(t).unwrap();
        assert_eq!(v.weights, vec![amount(2 * ONE / 10), amount(8 * ONE / 10)]);
        assert_eq!(v.swap_fee, amount(3 * ONE / 100));
    }
    assert!(s.at(u64::MAX / 1000 + 1).is_err());
    assert!(s.at(u64::MAX).is_err());
}

#[test]
fn flags_scaling_metadata_and_raw_input_immutable() {
    for flags in 0..16 {
        let mut s = schedule();
        s.pool_state_word = pool(10, 20, 30, 40, FEE_MIN, FEE_MAX, flags);
        let before = serde_json::to_vec(&s).unwrap();
        let v = s.at(15).unwrap();
        assert_eq!(
            (v.weight_start, v.weight_end, v.fee_start, v.fee_end),
            (10, 20, 30, 40)
        );
        assert_eq!(v.swaps_enabled, flags & 1 != 0);
        assert_eq!(v.must_allowlist_lps, flags & 2 != 0);
        assert_eq!(v.recovery_mode, flags & 4 != 0);
        assert_eq!(v.join_exit_enabled, flags & 8 != 0);
        assert_eq!(
            v.scaling,
            vec![
                amount(ONE),
                Amount(BigUint::from(ONE) * BigUint::from(10u8).pow(12))
            ]
        );
        assert_eq!(v.swap_fee, amount(FEE_MIN));
        assert_eq!(s.at(35).unwrap().swap_fee, amount(475000500000000000));
        s.at(100).unwrap();
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
    }
    for diff in 0..=18 {
        let mut s = schedule();
        s.tokens[0] = token(8 * ONE / 10, 2 * ONE / 10, diff, 1);
        assert_eq!(
            s.at(101).unwrap().scaling[0],
            Amount(BigUint::from(ONE) * BigUint::from(10u8).pow(diff as u32))
        );
    }
}

#[test]
fn zero_times_direct_jump_and_u32_max() {
    for t in [0, 7, u64::from(u32::MAX)] {
        let mut s = schedule();
        s.pool_state_word = pool(t, t, t, t, FEE_MAX, FEE_MIN, 15);
        if t > 0 {
            assert_eq!(
                weights(&s, t - 1),
                vec![amount(8 * ONE / 10), amount(2 * ONE / 10)]
            );
        }
        let v = s.at(t).unwrap();
        assert_eq!(v.weights, vec![amount(2 * ONE / 10), amount(8 * ONE / 10)]);
        assert_eq!(v.swap_fee, amount(FEE_MIN));
    }
    let mut s = schedule();
    s.pool_state_word = pool(0, u32::MAX as u64, 0, u32::MAX as u64, FEE_MAX, FEE_MIN, 0);
    assert_eq!(
        weights(&s, 0),
        vec![amount(8 * ONE / 10), amount(2 * ONE / 10)]
    );
    assert_eq!(s.at(u32::MAX as u64).unwrap().swap_fee, amount(FEE_MIN));
}

#[test]
fn start_sum_is_unrestricted_end_sum_is_exact() {
    for delta in [-2i64, -1, 0, 1, 2] {
        let mut s = schedule();
        s.tokens[0] = token((8 * ONE / 10) - 2 + (delta + 2) as u64, 2 * ONE / 10, 0, 1);
        s.validate().unwrap();
        assert_eq!(
            weights(&s, 100)[0],
            amount((8 * ONE / 10) - 2 + (delta + 2) as u64)
        );
        s.at(101).unwrap();
        if delta != 0 {
            let mut bad = schedule();
            bad.tokens[0] = token(8 * ONE / 10, 2 * ONE / 10 - 2 + (delta + 2) as u64, 0, 1);
            reject(bad);
        }
    }
    let mut s = schedule();
    s.tokens[0] = token(ONE, 2 * ONE / 10, 0, 1);
    s.tokens[1] = token(ONE, 8 * ONE / 10, 0, 2);
    s.validate().unwrap();
    assert_eq!(weights(&s, 100), vec![amount(ONE), amount(ONE)]);
    s.tokens[0] = token(MIN, 2 * ONE / 10, 0, 1);
    s.tokens[1] = token(MIN, 8 * ONE / 10, 0, 2);
    s.validate().unwrap();
}

#[test]
fn raw_shapes_and_ranges_fail_closed() {
    for word in [
        Amount(BigUint::from(1u8) << 256usize),
        Amount((BigUint::from(1u8) << 300usize) + BigUint::from(1u8)),
    ] {
        let mut s = schedule();
        s.pool_state_word = word.clone();
        reject(s);
        let mut s = schedule();
        s.tokens[0].state_word = word;
        reject(s);
    }
    for bit in 133usize..=255 {
        let mut s = schedule();
        s.tokens[0].state_word.0 |= BigUint::from(1u8) << bit;
        reject(s);
    }
    for diff in [19, 31] {
        let mut s = schedule();
        s.tokens[0] = token(8 * ONE / 10, 2 * ONE / 10, diff, 1);
        reject(s);
    }
    for weight in [0, MIN - 1, ONE + 1, u64::MAX] {
        for end in [false, true] {
            let mut s = schedule();
            s.tokens[0] = if end {
                token(8 * ONE / 10, weight, 0, 1)
            } else {
                token(weight, 2 * ONE / 10, 0, 1)
            };
            reject(s);
        }
    }
    for fee in [0, FEE_MIN - 1, FEE_MAX + 1, (1u64 << 61), (1u64 << 62) - 1] {
        for end in [false, true] {
            let mut s = schedule();
            s.pool_state_word = if end {
                pool(100, 200, 100, 200, FEE_MIN, fee, 0)
            } else {
                pool(100, 200, 100, 200, fee, FEE_MIN, 0)
            };
            reject(s);
        }
    }
    for times in [(20, 10, 0, 0), (0, 0, 20, 10)] {
        let mut s = schedule();
        s.pool_state_word = pool(times.0, times.1, times.2, times.3, FEE_MIN, FEE_MAX, 0);
        reject(s);
    }
}

#[test]
fn token_count_order_identity() {
    for count in [0usize, 1, 51] {
        let mut s = schedule();
        s.tokens = (0..count)
            .map(|i| token(ONE / 2, ONE / 2, 0, i + 1))
            .collect();
        reject(s);
    }
    let mut s = schedule();
    s.tokens = (1..=50).map(|i| token(ONE / 50, ONE / 50, 0, i)).collect();
    s.validate().unwrap();
    let mut s = schedule();
    s.tokens.swap(0, 1);
    reject(s);
    let mut s = schedule();
    s.tokens[1].token = s.tokens[0].token.clone();
    reject(s);
    for address in [
        "0x1",
        "0X0000000000000000000000000000000000000001",
        "0x000000000000000000000000000000000000000A",
        "0x000000000000000000000000000000000000000g",
    ] {
        let mut s = schedule();
        s.tokens[0].token = address.into();
        reject(s);
    }
    // Zero-address and own-pool exclusions require the parent's identity policy.
    let mut s = schedule();
    s.tokens[0].token = format!("0x{:040x}", 0);
    s.validate().unwrap();
}

// Literal packed storage and getters copied from the original VM bank (not a
// runtime dependency): /tmp/xone-managed-v2-three-schedule/bank.json.
// CP0..CP9; duration 1001; original 18/6/8-decimal sorted tokens.
#[test]
fn original_three_token_packed_checkpoints() {
    let cases = [
        (
            "CP0",
            1767225660,
            "9008e1bc9bf04000002386f26fc100006955bd256955b93c6955bd256955b93c",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "00000000000000000000000000000000016345785d8a000006f05b59d3b20000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0b1a2bc2ec5000000429d069189e0000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a016345785d8a000002c68af0bb140000",
                ),
            ],
            [
                "500000000000000000",
                "300000000000000000",
                "200000000000000000",
            ],
        ),
        (
            "CP1",
            1767225661,
            "9008e1bc9bf04000002386f26fc100006955bd256955b93c6955bd256955b93c",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "00000000000000000000000000000000016345785d8a000006f05b59d3b20000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0b1a2bc2ec5000000429d069189e0000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a016345785d8a000002c68af0bb140000",
                ),
            ],
            [
                "499600399600399601",
                "300499500499500499",
                "199900099900099901",
            ],
        ),
        (
            "CP2",
            1767225993,
            "9008e1bc9bf04000002386f26fc100006955bd256955b93c6955bd256955b93c",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "00000000000000000000000000000000016345785d8a000006f05b59d3b20000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0b1a2bc2ec5000000429d069189e0000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a016345785d8a000002c68af0bb140000",
                ),
            ],
            [
                "366933066933066934",
                "466333666333666333",
                "166733266733266734",
            ],
        ),
        (
            "CP3",
            1767226160,
            "9008e1bc9bf04000002386f26fc100006955bd256955b93c6955bd256955b93c",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "00000000000000000000000000000000016345785d8a000006f05b59d3b20000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0b1a2bc2ec5000000429d069189e0000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a016345785d8a000002c68af0bb140000",
                ),
            ],
            [
                "300199800199800200",
                "549750249750249750",
                "150049950049950050",
            ],
        ),
        (
            "CP4",
            1767226661,
            "9008e1bc9bf04000002386f26fc100006955bd256955b93c6955bd256955b93c",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "00000000000000000000000000000000016345785d8a000006f05b59d3b20000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0b1a2bc2ec5000000429d069189e0000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a016345785d8a000002c68af0bb140000",
                ),
            ],
            [
                "100000000000000000",
                "800000000000000000",
                "100000000000000000",
            ],
        ),
        (
            "CP5",
            1767226721,
            "9008e1bc9bf04000002386f26fc100006955c14a6955bd616955c14a6955bd61",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "0000000000000000000000000000000006f05b59d3b20000016345785d8a0000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0429d069189e00000b1a2bc2ec500000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a02c68af0bb140000016345785d8a0000",
                ),
            ],
            [
                "100000000000000000",
                "800000000000000000",
                "100000000000000000",
            ],
        ),
        (
            "CP6",
            1767226722,
            "9008e1bc9bf04000002386f26fc100006955c14a6955bd616955c14a6955bd61",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "0000000000000000000000000000000006f05b59d3b20000016345785d8a0000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0429d069189e00000b1a2bc2ec500000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a02c68af0bb140000016345785d8a0000",
                ),
            ],
            [
                "100399600399600399",
                "799500499500499501",
                "100099900099900099",
            ],
        ),
        (
            "CP7",
            1767227054,
            "9008e1bc9bf04000002386f26fc100006955c14a6955bd616955c14a6955bd61",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "0000000000000000000000000000000006f05b59d3b20000016345785d8a0000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0429d069189e00000b1a2bc2ec500000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a02c68af0bb140000016345785d8a0000",
                ),
            ],
            [
                "233066933066933066",
                "633666333666333667",
                "133266733266733266",
            ],
        ),
        (
            "CP8",
            1767227221,
            "9008e1bc9bf04000002386f26fc100006955c14a6955bd616955c14a6955bd61",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "0000000000000000000000000000000006f05b59d3b20000016345785d8a0000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0429d069189e00000b1a2bc2ec500000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a02c68af0bb140000016345785d8a0000",
                ),
            ],
            [
                "299800199800199800",
                "550249750249750250",
                "149950049950049950",
            ],
        ),
        (
            "CP9",
            1767227722,
            "9008e1bc9bf04000002386f26fc100006955c14a6955bd616955c14a6955bd61",
            [
                (
                    "0x6595b34ed0a270b10a586fc1ea22030a95386f1e",
                    "0000000000000000000000000000000006f05b59d3b20000016345785d8a0000",
                ),
                (
                    "0x91eb86019fd8d7c5a9e31143d422850a13f670a3",
                    "0000000000000000000000000000000c0429d069189e00000b1a2bc2ec500000",
                ),
                (
                    "0xc705034ded85e817b9e56c977e61a2098362898b",
                    "0000000000000000000000000000000a02c68af0bb140000016345785d8a0000",
                ),
            ],
            [
                "500000000000000000",
                "300000000000000000",
                "200000000000000000",
            ],
        ),
    ];
    for (id, timestamp, pool_hex, tokens, expected) in cases {
        let schedule = ManagedV2Schedule {
            pool_state_word: Amount(BigUint::parse_bytes(pool_hex.as_bytes(), 16).unwrap()),
            tokens: tokens
                .into_iter()
                .map(|(token, word)| ManagedV2ScheduledToken {
                    token: token.into(),
                    state_word: Amount(BigUint::parse_bytes(word.as_bytes(), 16).unwrap()),
                })
                .collect(),
        };
        let before = serde_json::to_vec(&schedule).unwrap();
        let v = schedule.at(timestamp).unwrap();
        assert_eq!(
            v.weights,
            expected.map(|w| w.parse::<Amount>().unwrap()).to_vec(),
            "{id}"
        );
        assert_eq!(v.swap_fee, amount(ONE / 100), "{id}");
        assert_eq!(
            v.scaling,
            [
                "1000000000000000000",
                "1000000000000000000000000000000",
                "10000000000000000000000000000"
            ]
            .map(|s| s.parse::<Amount>().unwrap())
            .to_vec()
        );
        assert_eq!(serde_json::to_vec(&schedule).unwrap(), before);
    }
}

#[test]
fn synthetic_rescheduled_start_drift_can_become_two_units() {
    // Synthetic continuity cases, distinct from the literal VM checkpoints.
    let cases = [
        (
            [500000000000000001, 300000000000000000, 200000000000000000],
            [100000000000000000, 800000000000000000, 100000000000000000],
            [499600399600399602, 300499500499500499, 199900099900099901],
            ONE + 2,
        ),
        (
            [99999999999999999, 800000000000000000, 100000000000000000],
            [500000000000000000, 300000000000000000, 200000000000000000],
            [100399600399600398, 799500499500499501, 100099900099900099],
            ONE - 2,
        ),
    ];
    for (start, end, expected, sum) in cases {
        let s = ManagedV2Schedule {
            pool_state_word: pool(100, 1101, 100, 1101, FEE_MAX, FEE_MIN, 0),
            tokens: (0..3).map(|i| token(start[i], end[i], 0, i + 1)).collect(),
        };
        s.validate().unwrap();
        let v = s.at(101).unwrap();
        assert_eq!(v.weights, expected.map(amount).to_vec());
        assert_eq!(
            v.weights.iter().map(|a| a.0.clone()).sum::<BigUint>(),
            BigUint::from(sum)
        );
        assert_eq!(s.at(1101).unwrap().weights, end.map(amount).to_vec());
    }
}

#[test]
fn descending_fee_rounding_uses_its_own_clock() {
    let mut s = schedule();
    s.pool_state_word = pool(100, 2_592_100, 200, 2_592_200, 3 * ONE / 100, ONE / 100, 0);
    assert_eq!(s.at(200).unwrap().swap_fee, amount(3 * ONE / 100));
    assert_eq!(s.at(201).unwrap().swap_fee, amount(29999992283950618));
    assert_eq!(s.at(2_592_199).unwrap().swap_fee, amount(10000007716049383));
    assert_eq!(s.at(2_592_200).unwrap().swap_fee, amount(ONE / 100));
}

#[test]
fn view_json_is_strict_computed_data() {
    use xone_domain::managed_v2_schedule::ManagedV2ScheduleView;
    let view = schedule().at(101).unwrap();
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(
        serde_json::from_value::<ManagedV2ScheduleView>(value.clone()).unwrap(),
        view
    );
    assert_eq!(value.as_object().unwrap().len(), 11);
    for field in value.as_object().unwrap().keys() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ManagedV2ScheduleView>(missing).is_err());
        let mut null = value.clone();
        null[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ManagedV2ScheduleView>(null).is_err());
    }
    let mut unknown = value;
    unknown["canonical_h"] = serde_json::json!("unexpected");
    assert!(serde_json::from_value::<ManagedV2ScheduleView>(unknown).is_err());
}
