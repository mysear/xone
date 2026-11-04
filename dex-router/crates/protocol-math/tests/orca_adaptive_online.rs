use xone_domain::{Amount, Pool};
fn corpus() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-swap.json"
    ))
    .unwrap()
}
fn model(native: &serde_json::Value, ts: u64) -> Pool {
    let mut p: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap();
    p["protocol"] = "orca-whirlpools-adaptive".into();
    p["adapter_version"] = "orca-whirlpools/cc683ec9-spl-adaptive-1".into();
    p["fee_numerator"] = native["fee_rate"].clone();
    let id = p["pool_id"].clone();
    let s = &mut p["orca_whirlpool"];
    s["fee_tier_index"] = 65.into();
    s["bank_unix_timestamp"] = ts.into();
    s["fee_rate"] = native["fee_rate"].clone();
    s["protocol_fee_rate"] = native["protocol_fee_rate"].clone();
    for (domain, key) in [
        ("sqrt_price_x64", "sqrt_price_x64"),
        ("tick_current_index", "tick_current_index"),
        ("liquidity", "liquidity"),
        ("fee_growth_global0", "fee_growth_global_a"),
        ("fee_growth_global1", "fee_growth_global_b"),
        ("protocol_fee_owed0", "protocol_fee_owed_a"),
        ("protocol_fee_owed1", "protocol_fee_owed_b"),
        (
            "reward_last_updated_timestamp",
            "reward_last_updated_timestamp",
        ),
    ] {
        s[domain] = native[key].clone()
    }
    s["reward_infos"]=serde_json::json!((0..3).map(|i|serde_json::json!({"mint":"11111111111111111111111111111111","vault":"11111111111111111111111111111111","extension":native["reward_infos"][i]["extension"],"emissions_per_second_x64":"0","growth_global_x64":"0"})).collect::<Vec<_>>());
    let oracle = &native["oracle"];
    let constants = &oracle["adaptive_fee_info"]["constants"];
    let variables = &oracle["adaptive_fee_info"]["variables"];
    let mut a = constants.clone();
    a.as_object_mut().unwrap().remove("reserved");
    a["constants_reserved"] = constants["reserved"].clone();
    for (key, v) in variables.as_object().unwrap() {
        if key != "reserved" {
            a[key] = v.clone()
        }
    }
    a["variables_reserved"] = variables["reserved"].clone();
    a["whirlpool"] = id;
    a["trade_enable_timestamp"] = oracle["trade_enable_timestamp"].clone();
    a["reserved"] = oracle["reserved"].clone();
    s["adaptive"] = a;
    for (a, source) in s["tick_arrays"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(native["tick_arrays"].as_array().unwrap())
    {
        a["start_tick_index"] = source["start_tick_index"].clone();
        a["layout"] = source["layout"].clone();
        a["initialized_bitmap"] = source["initialized_bitmap"].clone();
        a["ticks"]=serde_json::json!(source["ticks"].as_array().unwrap().iter().map(|t|serde_json::json!({"tick":t["tick"],"liquidity_gross":t["liquidity_gross"],"liquidity_net":t["liquidity_net"],"fee_growth_outside0":t["fee_growth_outside_a"],"fee_growth_outside1":t["fee_growth_outside_b"],"reward_growths_outside":t["reward_growths_outside"]})).collect::<Vec<_>>());
    }
    serde_json::from_value(p).unwrap()
}
#[test]
fn adaptive_normal_dispatch_uses_native_complete_swap_and_all_poststate() {
    let d = corpus();
    let mut count = 0;
    for c in d["cases"].as_array().unwrap() {
        let state = &c["request"]["state"];
        if state["tick_spacing"] != 64 || state["tick_arrays"].as_array().unwrap().len() != 3 {
            continue;
        }
        let time = c["request"]["params"]["timestamp"].as_u64().unwrap();
        let p = model(state, time);
        if p.validate().is_err() {
            continue;
        }
        let params = &c["request"]["params"];
        if c["expected"]["amount_remaining"] != 0 {
            continue;
        }
        let amount: Amount = params["amount"]
            .as_u64()
            .unwrap()
            .to_string()
            .parse()
            .unwrap();
        let left = params["a_to_b"].as_bool().unwrap();
        let result = xone_protocol_math::quote_exact_in(&p, &amount, left);
        if c["expected"]["amount_out"] == 0 {
            assert!(result.is_err(), "native zero output returned a quote");
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{} {e}", c["name"]));
        let e = &c["expected"];
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_u64().unwrap().to_string(),
            "{}",
            c["name"]
        );
        let native = &e["next_state"];
        let expected = model(native, time);
        let s = q.next_pool.orca_whirlpool.as_ref().unwrap();
        let ns = expected.orca_whirlpool.as_ref().unwrap();
        assert_eq!(s.adaptive, ns.adaptive);
        assert_eq!(s.tick_arrays, ns.tick_arrays);
        assert_eq!(s.liquidity, ns.liquidity);
        assert_eq!(s.sqrt_price_x64, ns.sqrt_price_x64);
        assert_eq!(s.tick_current_index, ns.tick_current_index);
        assert_eq!(s.fee_growth_global0, ns.fee_growth_global0);
        assert_eq!(s.fee_growth_global1, ns.fee_growth_global1);
        assert_eq!(s.protocol_fee_owed0, ns.protocol_fee_owed0);
        assert_eq!(s.protocol_fee_owed1, ns.protocol_fee_owed1);
        assert_eq!(
            s.reward_last_updated_timestamp,
            ns.reward_last_updated_timestamp
        );
        assert_eq!(q.consumed_in, amount);
        assert_eq!(q.remaining_in, Amount::zero());
        let old = p.orca_whirlpool.as_ref().unwrap();
        if left {
            assert_eq!(
                s.raw_vault0,
                old.raw_vault0 + params["amount"].as_u64().unwrap()
            );
            assert_eq!(
                s.raw_vault1,
                old.raw_vault1 - e["amount_out"].as_u64().unwrap()
            )
        } else {
            assert_eq!(
                s.raw_vault1,
                old.raw_vault1 + params["amount"].as_u64().unwrap()
            );
            assert_eq!(
                s.raw_vault0,
                old.raw_vault0 - e["amount_out"].as_u64().unwrap()
            )
        }
        count += 1;
    }
    assert!(count >= 20, "insufficient native complete cases {count}");
}

#[test]
fn normal_adaptive_quotes_match_original_packed_program_banks() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-pools.json"
    ))
    .unwrap();
    let mut good = 0;
    let mut rejected = 0;
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let before = p.clone();
        let input: Amount = c["params"]["amount"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::quote_exact_in(
            &p,
            &input,
            c["params"]["a_to_b"].as_bool().unwrap(),
        );
        assert_eq!(before, p, "input mutated");
        if c["expected"]["status"] == "native_rejected" {
            assert!(q.is_err(), "expected native rejection {}", c["name"]);
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        let next: Pool = serde_json::from_value(c["expected"]["post_pool"].clone()).unwrap();
        assert_eq!(q.next_pool, next, "{}", c["name"]);
        assert_eq!(q.amount_out.to_string(), c["expected"]["amount_out"]);
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        good += 1;
    }
    assert_eq!((good, rejected), (16, 4));
}

#[test]
fn native_service_observations_are_bound_to_the_original_program_fixture() {
    use sha2::{Digest, Sha256};
    let original = include_bytes!("../../../test/contracts/orca-adaptive-service.json");
    let projected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-pools.json"
    ))
    .unwrap();
    let actual: String = Sha256::digest(original)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(projected["source_corpus_sha256"], actual);
    let raw: serde_json::Value = serde_json::from_slice(original).unwrap();
    assert_eq!(
        raw["identity"]["revision"],
        "cc683ec92acb5e7606caa3d8e514d9383e9c7c11"
    );
    assert_eq!(raw["identity"]["sbf"], false);
    assert_eq!(raw["scenarios"].as_array().unwrap().len(), 4);
    assert!(raw["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["banks"].as_array().unwrap().len() == 5));
}
