use std::collections::BTreeMap;
use xone_domain::Amount;
use xone_protocol_math::infinity_bin_multi::*;
#[test]
fn tree_outside_active_is_consumed_before_strict_tree_successor() {
    let a = 8388608;
    let n = |s: &str| s.parse::<Amount>().unwrap();
    let s = MultiBinState {
        active_id: a,
        bin_step: 1,
        lp_fee: 3000,
        protocol_fee0: 4000,
        protocol_fee1: 17,
        max_steps: 64,
        tree_min_id: 0,
        tree_max_id: 0xffffff,
        bins: BTreeMap::from([(
            a - 1,
            BinReserves {
                reserve0: n("1000000"),
                reserve1: n("1000000"),
            },
        )]),
    };
    let ghost = BinReserves {
        reserve0: n("1000"),
        reserve1: n("1000"),
    };
    let input = n("100");
    let q = quote_exact_in_with_active(&s, &input, true, Some(&ghost)).unwrap();
    assert_eq!(q.next_state.active_id, a, "untracked active was skipped");
    assert_eq!(q.amount_out, n("99"));
    assert_eq!(
        q.next_state.bins, s.bins,
        "ghost must not enter or change the tree"
    );
    let q = quote_exact_in_with_active(&s, &n("2000"), true, Some(&ghost)).unwrap();
    assert_eq!(q.next_state.active_id, a - 1);
    assert!(!q.next_state.bins.contains_key(&a));
}

#[test]
fn original_burn_locked_share_dust_and_following_swaps_match_full_state() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-detached.json"
    ))
    .unwrap();
    let amount = |v: &serde_json::Value| v.as_str().unwrap().parse::<Amount>().unwrap();
    let reserves = |v: &serde_json::Value| BinReserves {
        reserve0: amount(&v["reserve0"]),
        reserve1: amount(&v["reserve1"]),
    };
    let mut count = 0;
    let mut rejected = 0;
    for c in d["cases"].as_array().unwrap() {
        let post = &c["burn"]["expected"]["post"];
        assert_eq!(post["detached_active"]["total_shares"], "1000");
        assert!(post["detached_active"]["in_tree"] == false);
        for t in c["trades"].as_array().unwrap() {
            let s = &t["state"];
            let state = MultiBinState {
                active_id: s["active_id"].as_u64().unwrap() as u32,
                bin_step: s["bin_step"].as_u64().unwrap() as u16,
                lp_fee: s["lp_fee"].as_u64().unwrap() as u32,
                protocol_fee0: s["protocol_fee0"].as_u64().unwrap() as u32,
                protocol_fee1: s["protocol_fee1"].as_u64().unwrap() as u32,
                max_steps: 64,
                tree_min_id: 0,
                tree_max_id: 0xffffff,
                bins: s["bins"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(id, b)| (id.parse::<u32>().unwrap(), reserves(b)))
                    .collect(),
            };
            let active = if s["detached_active"].is_null() {
                None
            } else {
                Some(reserves(&s["detached_active"]))
            };
            let q = quote_exact_in_with_active(
                &state,
                &amount(&t["amount_in"]),
                t["zero_for_one"].as_bool().unwrap(),
                active.as_ref(),
            );
            let expected = &t["expected"];
            if expected["native_reverted"] == true {
                assert!(q.is_err(), "{} {}", c["name"], t["label"]);
                rejected += 1;
                continue;
            }
            let q = q.unwrap_or_else(|e| panic!("{} {} {e}", c["name"], t["label"]));
            assert_eq!(q.amount_out, amount(&expected["amount_out"]));
            assert_eq!(q.protocol_amount, amount(&expected["protocol_amount"]));
            let post = &expected["post"];
            assert_eq!(
                q.next_state.active_id,
                post["active_id"].as_u64().unwrap() as u32
            );
            let bins = post["bins"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, b)| (id.parse::<u32>().unwrap(), reserves(b)))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(q.next_state.bins, bins);
            let active = if post["detached_active"].is_null() {
                None
            } else {
                Some(reserves(&post["detached_active"]))
            };
            assert_eq!(q.detached_active, active);
            count += 1;
        }
    }
    assert_eq!(d["cases"].as_array().unwrap().len(), 16);
    assert!(count >= 20 && rejected >= 6, "{count}/{rejected}");
}
