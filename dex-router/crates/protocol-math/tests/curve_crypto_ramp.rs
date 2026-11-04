use num_bigint::BigUint;
use xone_domain::{Amount, Pool};
use xone_protocol_math::multi_asset::quote_exact_in_tokens;

#[test]
fn completed_ramp_sentinels_preserve_origin_and_match_standard_ledger() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-service.json"
    ))
    .unwrap();
    for sentinel in [0, 1] {
        let mut standard: Pool = serde_json::from_value(f["cases"][0]["pool"].clone()).unwrap();
        standard
            .multi_asset
            .as_mut()
            .unwrap()
            .curve_crypto
            .as_mut()
            .unwrap()
            .future_a_gamma_time = sentinel;
        let mut ramp = standard.clone();
        ramp.protocol = "curve-crypto2-ramp".into();
        ramp.adapter_version = "curve-crypto2-ramp/d7d04cd9-ramp-1".into();
        let s = ramp.multi_asset.as_mut().unwrap();
        let c = s.curve_crypto.as_mut().unwrap();
        c.ramp = Some(xone_domain::curve_crypto_state::CurveCryptoRamp {
            initial_amp: s.amplification.clone(),
            initial_gamma: c.gamma.clone(),
            initial_time: 0,
        });
        let origin = c.ramp.clone();
        for (i, j) in [(0, 1), (1, 0)] {
            let s = standard.multi_asset.as_ref().unwrap();
            let input: Amount = "1000000000000000000".parse().unwrap();
            let expected =
                quote_exact_in_tokens(&standard, &input, &s.assets[i].token, &s.assets[j].token)
                    .unwrap();
            let mut actual =
                quote_exact_in_tokens(&ramp, &input, &s.assets[i].token, &s.assets[j].token)
                    .unwrap();
            assert_eq!(actual.amount_out, expected.amount_out);
            assert_eq!(actual.consumed_in, input);
            assert_eq!(actual.remaining_in, Amount::zero());
            actual.next_pool.validate().unwrap();
            assert_eq!(
                actual
                    .next_pool
                    .multi_asset
                    .as_ref()
                    .unwrap()
                    .curve_crypto
                    .as_ref()
                    .unwrap()
                    .ramp,
                origin
            );
            actual.next_pool.protocol = standard.protocol.clone();
            actual.next_pool.adapter_version = standard.adapter_version.clone();
            actual
                .next_pool
                .multi_asset
                .as_mut()
                .unwrap()
                .curve_crypto
                .as_mut()
                .unwrap()
                .ramp = None;
            assert_eq!(actual.next_pool, expected.next_pool);
        }
    }
}

fn assert_original_poststate(next: &Pool, expected: &serde_json::Value, name: &str) {
    let s = next.multi_asset.as_ref().unwrap();
    let c = s.curve_crypto.as_ref().unwrap();
    for k in 0..2 {
        assert_eq!(
            s.assets[k].balance.to_string(),
            expected["balances"][k].as_str().unwrap(),
            "balance {name}/{k}"
        );
        assert_eq!(
            c.raw_balances[k].to_string(),
            expected["raw_balances"][k].as_str().unwrap(),
            "raw {name}/{k}"
        );
    }
    for (actual, key) in [
        (&c.invariant, "D"),
        (&c.price_scale, "price_scale"),
        (&c.price_oracle, "_price_oracle"),
        (&c.last_prices, "last_prices"),
        (&c.xcp_profit, "xcp_profit"),
        (&c.xcp_profit_a, "xcp_profit_a"),
        (&c.virtual_price, "virtual_price"),
    ] {
        assert_eq!(
            actual.to_string(),
            expected["state"][key].as_str().unwrap(),
            "{key} {name}"
        );
    }
    assert_eq!(
        c.last_prices_timestamp.to_string(),
        expected["state"]["last_prices_timestamp"].as_str().unwrap(),
        "oracle time {name}"
    );
    assert_eq!(
        c.future_a_gamma_time.to_string(),
        expected["state"]["future_A_gamma_time"].as_str().unwrap(),
        "sentinel {name}"
    );
    assert_eq!(
        c.not_adjusted,
        expected["state"]["not_adjusted"] == "1",
        "adjustment {name}"
    );
    assert_eq!(
        c.total_supply.to_string(),
        expected["total_supply"].as_str().unwrap(),
        "supply {name}"
    );
    assert_eq!(
        c.admin_receiver_balance.to_string(),
        expected["receiver_balance"].as_str().unwrap(),
        "receiver {name}"
    );
    assert_eq!(next.reserve0, s.assets[0].balance);
    assert_eq!(next.reserve1, s.assets[1].balance);
    next.validate().unwrap();
}

#[test]
fn public_original_ramp_and_stop_normal_dispatch_and_carried_sequences() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-ramp-service.json"
    ))
    .unwrap();
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-ramp-native.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    let originals = native["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 28);
    assert_eq!(originals.len(), cases.len());
    let mut carried = std::collections::BTreeMap::<String, Pool>::new();
    let mut transitions = 0;
    let mut lanes = std::collections::BTreeMap::<String, Pool>::new();
    let mut schedule_transitions = 0;
    for (row, original) in cases.iter().zip(originals) {
        let name = row["name"].as_str().unwrap();
        assert_eq!(row["name"], original["name"]);
        assert_eq!(
            row["expected"], original["expected"],
            "original projection {name}"
        );
        let recorded: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
        recorded.validate().unwrap();
        let seq = row["sequence"].as_str().unwrap();
        let lane = seq.split_once('-').unwrap().1;
        let current = if let Some(mut prior) = carried.remove(seq) {
            prior
                .multi_asset
                .as_mut()
                .unwrap()
                .curve_crypto
                .as_mut()
                .unwrap()
                .timestamp = recorded
                .multi_asset
                .as_ref()
                .unwrap()
                .curve_crypto
                .as_ref()
                .unwrap()
                .timestamp;
            assert_eq!(prior, recorded, "carried state is complete before {name}");
            transitions += 1;
            prior
        } else {
            assert_eq!(row["step"], 0);
            if let Some(mut prior) = lanes.remove(lane) {
                // Public ramp/stop administration changes only the schedule.
                let source = recorded.multi_asset.as_ref().unwrap();
                let target = prior.multi_asset.as_mut().unwrap();
                target.amplification = source.amplification.clone();
                let src = source.curve_crypto.as_ref().unwrap();
                let dst = target.curve_crypto.as_mut().unwrap();
                dst.gamma = src.gamma.clone();
                dst.ramp = src.ramp.clone();
                dst.future_a_gamma_time = src.future_a_gamma_time;
                dst.timestamp = src.timestamp;
                assert_eq!(
                    prior, recorded,
                    "state carried across public schedule administration before {name}"
                );
                schedule_transitions += 1;
                prior
            } else {
                recorded
            }
        };
        let before = current.clone();
        let s = current.multi_asset.as_ref().unwrap();
        let c = s.curve_crypto.as_ref().unwrap();
        assert_eq!(
            serde_json::to_value(c.ramp.as_ref().unwrap()).unwrap(),
            original["ramp"]
        );
        assert_eq!(
            s.amplification.to_string(),
            original["params"]["amp"].as_str().unwrap()
        );
        assert_eq!(
            c.gamma.to_string(),
            original["params"]["gamma"].as_str().unwrap()
        );
        let input: Amount = row["input"].as_str().unwrap().parse().unwrap();
        let i = row["index_in"].as_u64().unwrap() as usize;
        let j = row["index_out"].as_u64().unwrap() as usize;
        let q = quote_exact_in_tokens(&current, &input, &s.assets[i].token, &s.assets[j].token)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(
            q.amount_out.to_string(),
            row["expected"]["amount_out"].as_str().unwrap(),
            "amount {name}"
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        let next = q.next_pool.multi_asset.as_ref().unwrap();
        let n = next.curve_crypto.as_ref().unwrap();
        assert_eq!(n.ramp, c.ramp, "origin survives sentinel clear {name}");
        assert_eq!(next.amplification, s.amplification);
        assert_eq!(n.gamma, c.gamma);
        assert_original_poststate(&q.next_pool, &row["expected"], name);
        assert_eq!(current, before, "quote mutated source {name}");
        lanes.insert(lane.into(), q.next_pool.clone());
        carried.insert(seq.into(), q.next_pool);
    }
    assert_eq!(carried.len(), 6);
    assert_eq!(transitions, 22);
    assert_eq!(lanes.len(), 2);
    assert_eq!(schedule_transitions, 4);
}

#[test]
fn active_ramp_refreshes_cached_invariant_from_preswap_balances() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-ramp-service.json"
    ))
    .unwrap();
    // Use an imbalanced state so that interpolated A/gamma changes D.
    let row = &f["cases"][2];
    let p: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
    let s = p.multi_asset.as_ref().unwrap();
    let c = s.curve_crypto.as_ref().unwrap();
    assert!(c.timestamp < c.future_a_gamma_time);
    assert_ne!(
        s.assets[0].balance.0,
        &s.assets[1].balance.0
            * (&s.assets[1].scaling.0 / BigUint::from(1_000_000_000_000_000_000u64))
    );
    let input: Amount = row["input"].as_str().unwrap().parse().unwrap();
    let i = row["index_in"].as_u64().unwrap() as usize;
    let j = row["index_out"].as_u64().unwrap() as usize;
    let expected =
        quote_exact_in_tokens(&p, &input, &s.assets[i].token, &s.assets[j].token).unwrap();
    let mut stale = p.clone();
    stale
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap()
        .invariant = "1".parse().unwrap();
    stale.validate().unwrap();
    let got =
        quote_exact_in_tokens(&stale, &input, &s.assets[i].token, &s.assets[j].token).unwrap();
    assert_eq!(got.amount_out, expected.amount_out);
    assert_eq!(got.next_pool, expected.next_pool);
    assert_eq!(got.consumed_in, expected.consumed_in);
    assert_eq!(got.remaining_in, expected.remaining_in);
}
