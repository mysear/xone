use xone_protocol_math::orca_adaptive_swap::*;
fn validate_manifest(corpus: &serde_json::Value) {
    use sha2::{Digest, Sha256};
    let names = |key: &str| {
        corpus[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>()
    };
    let cases = names("cases");
    let failures = names("failures");
    let sequences = corpus["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap(),
                s["trades"].as_array().unwrap().len(),
            )
        })
        .collect::<Vec<_>>();
    let all = cases
        .iter()
        .copied()
        .chain(failures.iter().copied())
        .chain(sequences.iter().map(|s| s.0))
        .collect::<Vec<_>>();
    assert_eq!(
        all.len(),
        all.iter().collect::<std::collections::BTreeSet<_>>().len(),
        "duplicate native member"
    );
    let manifest = serde_json::json!({"cases":cases,"failures":failures,"sequences":sequences});
    assert_eq!(
        hex_digest(&Sha256::digest(serde_json::to_vec(&manifest).unwrap())),
        "13f4a65d41edb2cb764e852ae1e42bdaa62d65c958c1ed67cf46f400b8cf7591",
        "native member manifest changed"
    );
}
fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn projection(result: &AdaptiveSwapResult) -> serde_json::Value {
    let mut value = serde_json::to_value(result).unwrap();
    value.as_object_mut().unwrap().remove("steps");
    value
}

#[test]
fn original_swap_manager_corpus_matches_full_post_state() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-swap.json"
    ))
    .unwrap();
    assert_eq!(corpus["online_profile_enabled"], false);
    assert_eq!(
        corpus["identity"]["revision"],
        "cc683ec92acb5e7606caa3d8e514d9383e9c7c11"
    );
    assert_eq!(
        corpus["identity"]["execution"],
        "unchanged-original-native-swap-manager-and-post-update"
    );
    validate_manifest(&corpus);
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() >= 80);
    let mut high_fee_steps = 0;
    let mut crossed = 0;
    let mut skips = 0;
    for case in cases {
        let state: AdaptiveSwapState =
            serde_json::from_value(case["request"]["state"].clone()).unwrap();
        let params: AdaptiveExactInParams =
            serde_json::from_value(case["request"]["params"].clone()).unwrap();
        let original = state.clone();
        let quote =
            quote_exact_in(&state, params).unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
        assert_eq!(state, original, "input mutated");
        assert_eq!(projection(&quote), case["expected"], "{}", case["name"]);
        for step in &quote.steps {
            high_fee_steps += usize::from(step.fee_rate > 60_000);
            crossed += usize::from(step.crossed_initialized_tick);
            skips += usize::from(step.skip);
            assert!(step.fee_rate <= 100_000);
        }
        assert_eq!(quote.amount_in + quote.amount_remaining, params.amount);
        assert_eq!(
            quote.lp_fee + quote.protocol_fee,
            quote.steps.iter().map(|s| s.fee_amount).sum::<u64>()
        );
    }
    assert!(high_fee_steps > 0);
    assert!(crossed > 0);
    assert!(skips > 0);
    for sequence in corpus["sequences"].as_array().unwrap() {
        let mut state: AdaptiveSwapState =
            serde_json::from_value(sequence["initial_state"].clone()).unwrap();
        for trade in sequence["trades"].as_array().unwrap() {
            let params = serde_json::from_value(trade["params"].clone()).unwrap();
            let result = quote_exact_in(&state, params).unwrap();
            assert_eq!(
                projection(&result),
                trade["expected"],
                "sequence {}",
                sequence["name"]
            );
            state = result.next_state;
        }
    }
}
#[test]
fn rejects_unproven_or_unsupported_state_without_mutation() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-swap.json"
    ))
    .unwrap();
    let req = &corpus["cases"][0]["request"];
    let base: AdaptiveSwapState = serde_json::from_value(req["state"].clone()).unwrap();
    let params: AdaptiveExactInParams = serde_json::from_value(req["params"].clone()).unwrap();
    for mutation in 0..11 {
        let mut state = base.clone();
        let mut p = params;
        match mutation {
            0 => state.token_program = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb".into(),
            1 => state.reward_infos[0].mint = [1; 32],
            2 => state.tick_arrays[0].layout = "dynamic".into(),
            3 => state.oracle.whirlpool = [2; 32],
            4 => state.oracle.reserved.pop().map(|_| ()).unwrap(),
            5 => state.tick_arrays[0].initialized_bitmap ^= 1,
            6 => p.amount = 0,
            7 => p.timestamp = 0,
            8 => p.max_steps = 0,
            9 => state.tick_arrays.clear(),
            _ => p.sqrt_price_limit = state.sqrt_price_x64,
        }
        let original = state.clone();
        assert!(quote_exact_in(&state, p).is_err(), "mutation {mutation}");
        assert_eq!(state, original);
    }
    let mut p = params;
    p.amount = u64::MAX;
    p.sqrt_price_limit = 0;
    assert!(
        quote_exact_in(&base, p).is_err(),
        "array proof exhaustion must fail"
    );
    p = params;
    p.max_steps = 1;
    p.amount = 1_000_000_000;
    assert!(quote_exact_in(&base, p).is_err(), "step budget must fail");
}
#[test]
fn original_native_proof_exhaustion_is_an_atomic_error() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-swap.json"
    ))
    .unwrap();
    validate_manifest(&corpus);
    for case in corpus["failures"].as_array().unwrap() {
        assert!(case["expected"]["error"].is_string());
        let state = serde_json::from_value(case["request"]["state"].clone()).unwrap();
        let params = serde_json::from_value(case["request"]["params"].clone()).unwrap();
        assert!(quote_exact_in(&state, params).is_err(), "{}", case["name"]);
    }
}
