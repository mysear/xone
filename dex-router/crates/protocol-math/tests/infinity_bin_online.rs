use xone_domain::{Amount, Pool};
#[test]
fn online_bin_state_reuses_original_sparse_tree_math_and_complete_balances() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-wire.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(f["snapshot"]["pools"][0].clone()).unwrap();
    let input: Amount = "1100000".parse().unwrap();
    for left in [true, false] {
        let s = p.infinity_bin.as_ref().unwrap();
        let state = xone_protocol_math::infinity_bin_multi::MultiBinState {
            active_id: s.active_id,
            bin_step: s.bin_step as u16,
            lp_fee: s.lp_fee,
            protocol_fee0: s.protocol_fee0,
            protocol_fee1: s.protocol_fee1,
            max_steps: s.max_steps,
            tree_min_id: 0,
            tree_max_id: 0xffffff,
            bins: s
                .bins
                .iter()
                .map(|b| {
                    (
                        b.id,
                        xone_protocol_math::infinity_bin_multi::BinReserves {
                            reserve0: b.reserve0.clone(),
                            reserve1: b.reserve1.clone(),
                        },
                    )
                })
                .collect(),
        };
        let expected =
            xone_protocol_math::infinity_bin_multi::quote_exact_in(&state, &input, left).unwrap();
        assert!(expected.bin_fills.len() > 1);
        let q = xone_protocol_math::quote_exact_in(&p, &input, left).unwrap();
        assert_eq!(q.amount_out, expected.amount_out);
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        let next = q.next_pool.infinity_bin.unwrap();
        assert_eq!(next.active_id, expected.next_state.active_id);
        for b in next.bins {
            assert_eq!(b.reserve0, expected.next_state.bins[&b.id].reserve0);
            assert_eq!(b.reserve1, expected.next_state.bins[&b.id].reserve1);
            assert_eq!(
                b.total_shares,
                s.bins
                    .iter()
                    .find(|old| old.id == b.id)
                    .unwrap()
                    .total_shares
            )
        }
        if left {
            assert_eq!(next.app_reserve0.0, &s.app_reserve0.0 + &input.0);
            assert_eq!(
                next.app_reserve1.0,
                &s.app_reserve1.0 - &expected.amount_out.0
            );
            assert_eq!(
                next.protocol_fees0.0,
                &s.protocol_fees0.0 + &expected.protocol_amount.0
            );
        } else {
            assert_eq!(next.app_reserve1.0, &s.app_reserve1.0 + &input.0);
            assert_eq!(
                next.app_reserve0.0,
                &s.app_reserve0.0 - &expected.amount_out.0
            );
            assert_eq!(
                next.protocol_fees1.0,
                &s.protocol_fees1.0 + &expected.protocol_amount.0
            );
        }
    }
    let huge: Amount = "100000000000000000000000000000".parse().unwrap();
    assert!(xone_protocol_math::quote_exact_in(&p, &huge, true).is_err());
}

#[test]
fn normal_bin_quotes_match_original_manager_and_cross_bin_poststates() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-pools.json"
    ))
    .unwrap();
    let banks = f["banks"].as_array().unwrap();
    for trade in f["trades"].as_array().unwrap() {
        let before = banks.iter().find(|b| b["id"] == trade["pre_bank"]).unwrap();
        let after = banks
            .iter()
            .find(|b| b["id"] == trade["post_bank"])
            .unwrap();
        let index = before["items"]
            .as_array()
            .unwrap()
            .iter()
            .position(|i| i["pool"]["infinity_bin"]["pool_id"] == trade["pool_id"])
            .unwrap();
        let p: Pool = serde_json::from_value(before["items"][index]["pool"].clone()).unwrap();
        let input: Amount = trade["amount_in"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::quote_exact_in(
            &p,
            &input,
            trade["zero_for_one"].as_bool().unwrap(),
        )
        .unwrap();
        let mut expected: Pool =
            serde_json::from_value(after["items"][index]["pool"].clone()).unwrap();
        let old = p.infinity_bin.as_ref().unwrap();
        let s = expected.infinity_bin.as_mut().unwrap();
        s.block_number = old.block_number;
        s.timestamp = old.timestamp;
        s.block_hash = old.block_hash.clone();
        assert_eq!(q.next_pool, expected, "{}", trade["id"]);
        assert_eq!(q.amount_out.to_string(), trade["expected"]["amount_out"]);
        assert_eq!(q.remaining_in, Amount::zero());
        assert_eq!(q.consumed_in, input);
    }
}

#[test]
fn online_bin_projection_is_bound_to_original_manager_corpus() {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!("../../../test/contracts/infinity-bin-service.json");
    let projected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-pools.json"
    ))
    .unwrap();
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(projected["source_corpus_sha256"], actual);
}

#[test]
fn detached_locked_active_keeps_or_drops_its_state_and_never_enters_tree() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-wire.json"
    ))
    .unwrap();
    let mut p: Pool = serde_json::from_value(f["snapshot"]["pools"][0].clone()).unwrap();
    let s = p.infinity_bin.as_mut().unwrap();
    let ghost = s.bins.iter_mut().find(|b| b.id == s.active_id).unwrap();
    ghost.reserve0 = "1000".parse().unwrap();
    ghost.reserve1 = "1000".parse().unwrap();
    ghost.total_shares = "1000".parse().unwrap();
    let ghost = ghost.clone();
    s.bins.retain(|b| b.id != s.active_id);
    s.detached_active = Some(ghost);
    s.tree_level2
        .iter_mut()
        .find(|w| w.index == s.active_id >> 8)
        .unwrap()
        .value = "0".parse().unwrap();
    s.tree_level2.retain(|w| w.value != Amount::zero());
    s.tree_level1
        .iter_mut()
        .find(|w| w.index == 128)
        .unwrap()
        .value = "2".parse().unwrap();
    p.validate().unwrap();
    let q = xone_protocol_math::quote_exact_in(&p, &"100".parse().unwrap(), true).unwrap();
    let b = q.next_pool.infinity_bin.unwrap();
    assert_eq!(b.active_id, 8388608);
    assert_eq!(
        b.detached_active.unwrap().reserve1,
        "901".parse::<Amount>().unwrap()
    );
    assert_eq!(b.bins, p.infinity_bin.as_ref().unwrap().bins);
    let q = xone_protocol_math::quote_exact_in(&p, &"2000".parse().unwrap(), true).unwrap();
    assert!(q
        .next_pool
        .infinity_bin
        .as_ref()
        .unwrap()
        .detached_active
        .is_none());
    assert!(
        xone_protocol_math::quote_exact_in(&q.next_pool, &"100".parse().unwrap(), false).is_ok()
    );
}
