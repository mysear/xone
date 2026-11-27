use num_bigint::BigUint;
use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/managed-snapshot-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn check(c: &Value) -> Option<Pool> {
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let before = p.clone();
    let output: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let i = c["i"].as_u64().unwrap() as usize;
    let j = c["j"].as_u64().unwrap() as usize;
    let q = if let Some(s) = &p.multi_asset {
        xone_protocol_math::multi_asset::quote_exact_out_tokens(
            &p,
            &output,
            &s.assets[i].token,
            &s.assets[j].token,
        )
        .map(|q| (q.consumed_in, q.amount_out, q.next_pool))
    } else {
        xone_protocol_math::exact_out::quote_exact_out(&p, &output, i == 0)
            .map(|q| (q.amount_in, q.amount_out, q.next_pool))
    };
    assert_eq!(p, before);
    if c["expected"]["reverted"] == true {
        assert!(q.is_err(), "original reverted: {c}");
        return None;
    }
    let (input, out, next) =
        q.unwrap_or_else(|e| panic!("{} {i}->{j} target={output}: {}", p.protocol, e.0));
    assert_eq!(input.to_string(), c["expected"]["amount_in"]);
    assert_eq!(out, output);
    let mut expected = p.clone();
    let b = c["expected"]["next_balances"].as_array().unwrap();
    expected.reserve0 = b[0].as_str().unwrap().parse().unwrap();
    expected.reserve1 = b[1].as_str().unwrap().parse().unwrap();
    if let Some(s) = &mut expected.multi_asset {
        for (a, b) in s.assets.iter_mut().zip(b) {
            a.balance = b.as_str().unwrap().parse().unwrap();
        }
    }
    assert_eq!(next, expected, "entire physical state");
    Some(next)
}
#[test]
fn managed_output_matches_original_scheduled_callbacks_and_full_balances() {
    let f = corpus();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 180);
    assert_eq!(cases.iter().filter(|c| check(c).is_some()).count(), 108);
}
#[test]
fn managed_output_sequences_preserve_actual_weights_fee_and_all_assets() {
    for seq in corpus()["sequences"].as_array().unwrap() {
        let mut previous = None;
        for c in seq["steps"].as_array().unwrap() {
            if let Some(p) = previous {
                assert_eq!(
                    p,
                    serde_json::from_value::<Pool>(c["pool"].clone()).unwrap()
                );
            }
            previous = check(c);
            assert!(previous.is_some());
        }
    }
}
#[test]
fn managed_output_guards_and_compressed_weights_are_separate_from_static_admission() {
    use xone_protocol_math::balancer_multi::{
        quote_exact_out, quote_managed_snapshot_exact_out, ManagedSwapGuards,
    };
    let f = corpus();
    assert_eq!(f["snapshots"].as_array().unwrap().len(), 9);
    for snapshot in f["snapshots"].as_array().unwrap() {
        assert_ne!(snapshot["normalized_weight_sum"], "1000000000000000000");
    }
    let p: Pool = serde_json::from_value(f["cases"][0]["pool"].clone()).unwrap();
    let s = p.multi_asset.as_ref().unwrap();
    let b = s
        .assets
        .iter()
        .map(|a| a.balance.0.clone())
        .collect::<Vec<_>>();
    let w = s.weights.iter().map(|a| a.0.clone()).collect::<Vec<_>>();
    let scales = s
        .assets
        .iter()
        .map(|a| a.scaling.0.clone())
        .collect::<Vec<_>>();
    let fee = BigUint::from(p.fee_numerator);
    let target = BigUint::from(1000u32);
    let zero = BigUint::from(0u8);
    let one = BigUint::from(1u8);
    assert!(quote_exact_out(&b, &w, &scales, &fee, &target, 0, 1).is_err());
    let enabled = ManagedSwapGuards {
        swaps_enabled: true,
        protocol_swap_fee: zero.clone(),
        management_swap_fee: zero.clone(),
    };
    assert!(
        quote_managed_snapshot_exact_out(&b, &w, &scales, &fee, &target, 0, 1, &enabled).is_ok()
    );
    for guards in [
        ManagedSwapGuards {
            swaps_enabled: false,
            ..enabled.clone()
        },
        ManagedSwapGuards {
            protocol_swap_fee: one.clone(),
            ..enabled.clone()
        },
        ManagedSwapGuards {
            management_swap_fee: one.clone(),
            ..enabled.clone()
        },
    ] {
        assert!(
            quote_managed_snapshot_exact_out(&b, &w, &scales, &fee, &target, 0, 1, &guards)
                .is_err()
        );
    }
    for target in [zero.clone(), &b[1] * 3u8 / 10u8 + &one, &one << 256usize] {
        assert!(
            quote_managed_snapshot_exact_out(&b, &w, &scales, &fee, &target, 0, 1, &enabled)
                .is_err()
        );
    }
    let mut bad = w.clone();
    bad[0] = zero;
    assert!(
        quote_managed_snapshot_exact_out(&b, &bad, &scales, &fee, &target, 0, 1, &enabled).is_err()
    );
    assert_eq!(f["guards"].as_array().unwrap().len(), 3);
    for guard in f["guards"].as_array().unwrap() {
        assert_eq!(guard["expected"]["reverted"], true);
    }
    assert!(xone_protocol_math::exact_out::supports_protocol(
        "balancer-v2-managed-snapshot"
    ));
}
