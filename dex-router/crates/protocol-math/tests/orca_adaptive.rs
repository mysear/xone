//! Standalone until the owner exports the module from lib.rs.
#[path = "../src/orca_adaptive.rs"]
mod orca_adaptive;
use orca_adaptive::*;

fn constants() -> AdaptiveFeeConstants {
    AdaptiveFeeConstants {
        filter_period: 10,
        decay_period: 100,
        reduction_factor: 5000,
        adaptive_fee_control_factor: 1000,
        max_volatility_accumulator: 100_000,
        tick_group_size: 8,
        major_swap_threshold_ticks: 64,
        reserved: [0; 16],
    }
}
fn info() -> AdaptiveFeeInfo {
    AdaptiveFeeInfo {
        constants: constants(),
        variables: AdaptiveFeeVariables {
            last_reference_update_timestamp: 100,
            last_major_swap_timestamp: 100,
            volatility_reference: 20_000,
            tick_group_index_reference: 0,
            volatility_accumulator: 40_000,
            reserved: [0; 16],
        },
    }
}
fn context(tick: i32, time: u64, left: bool) -> AdaptiveFeeContext {
    AdaptiveFeeContext {
        a_to_b: left,
        current_tick_index: tick,
        timestamp: time,
        static_fee_rate: 3000,
        tick_spacing: 64,
        trade_enable_timestamp: 0,
    }
}
#[test]
fn floor_negative_group_and_fee_before_advance() {
    let mut m = FeeRateManager::begin(context(-1, 101, true), info()).unwrap();
    assert_eq!(m.tick_group_index(), -1);
    assert_eq!(m.get_total_fee_rate(), 3011); // accumulator is unchanged until first loop
    m.update_volatility_accumulator().unwrap();
    assert_eq!(
        m.get_next_adaptive_fee_info()
            .variables
            .volatility_accumulator,
        30_000
    );
    assert_eq!(m.get_total_fee_rate(), 3006);
    m.advance_tick_group().unwrap();
    assert_eq!(m.tick_group_index(), -2);
    assert_eq!(m.get_total_fee_rate(), 3006); // advancing doesn't accrue the next group
    m.update_volatility_accumulator().unwrap();
    assert_eq!(m.get_total_fee_rate(), 3011);
}
#[test]
fn reference_filter_decay_and_forced_reset_boundaries() {
    for (time, expected_ref, expected_tick, updated) in [
        (109, 20_000, 0, 100),
        (110, 20_000, -1, 110),
        (199, 20_000, -1, 199),
        (200, 0, -1, 200),
        (3700, 0, -1, 3700),
        (3701, 0, -1, 3701),
    ] {
        let m = FeeRateManager::begin(context(-1, time, true), info()).unwrap();
        let v = m.finish().variables;
        assert_eq!(
            (
                v.volatility_reference,
                v.tick_group_index_reference,
                v.last_reference_update_timestamp
            ),
            (expected_ref, expected_tick, updated)
        );
        assert_eq!(v.volatility_accumulator, 40_000);
    }
    let mut state = info();
    state.variables.last_major_swap_timestamp = 3699;
    let kept = FeeRateManager::begin(context(16, 3700, false), state)
        .unwrap()
        .finish();
    assert_eq!(kept.variables.last_reference_update_timestamp, 100);
    let reset = FeeRateManager::begin(context(16, 3701, false), state)
        .unwrap()
        .finish();
    assert_eq!(reset.variables.last_reference_update_timestamp, 3701);
    assert_eq!(reset.variables.volatility_reference, 0);
}
#[test]
fn rejects_invalid_constants_state_time_and_widths() {
    assert!(FeeRateManager::begin(context(0, 99, true), info()).is_err());
    let mut ctx = context(0, 101, true);
    ctx.trade_enable_timestamp = 102;
    assert!(FeeRateManager::begin(ctx, info()).is_err());
    for mutate in [0, 1, 2, 3, 4, 5, 6, 7] {
        let mut state = info();
        match mutate {
            0 => state.constants.filter_period = 0,
            1 => state.constants.decay_period = 10,
            2 => state.constants.reduction_factor = 10_000,
            3 => state.constants.adaptive_fee_control_factor = 100_000,
            4 => state.constants.tick_group_size = 0,
            5 => state.constants.tick_group_size = 7,
            6 => state.constants.major_swap_threshold_ticks = 0,
            _ => state.constants.max_volatility_accumulator = u32::MAX,
        }
        assert!(FeeRateManager::begin(context(0, 101, true), state).is_err());
    }
    let mut state = info();
    state.variables.volatility_reference = 100_001;
    assert!(FeeRateManager::begin(context(0, 101, true), state).is_err());
    state = info();
    state.variables.tick_group_index_reference = i32::MIN;
    assert!(FeeRateManager::begin(context(0, 101, true), state).is_err());
    assert!(FeeRateManager::begin(context(i32::MAX, 101, true), info()).is_err());
    let mut serialized = serde_json::to_value(info()).unwrap();
    serialized["constants"]["tick_group_size"] = serde_json::json!(65536);
    assert!(serde_json::from_value::<AdaptiveFeeInfo>(serialized).is_err());
}
fn snapshot(m: &FeeRateManager) -> serde_json::Value {
    let (lower, upper) = m.core_tick_group_range();
    let bound = |x: Option<(i32, u128)>| x.map(|(g, p)| serde_json::json!([g, p.to_string()]));
    serde_json::json!({"tick_group_index":m.tick_group_index(),"fee_rate":m.get_total_fee_rate(),
        "lower":bound(lower),"upper":bound(upper),"info":m.get_next_adaptive_fee_info()})
}
fn execute(req: &serde_json::Value) -> serde_json::Value {
    let ctx: AdaptiveFeeContext = serde_json::from_value(req["context"].clone()).unwrap();
    let state: AdaptiveFeeInfo = serde_json::from_value(req["info"].clone()).unwrap();
    let mut m = match FeeRateManager::begin(ctx, state) {
        Ok(m) => m,
        Err(e) => {
            return serde_json::json!({"error":match e.0.as_str() {
            "ORCA_ADAPTIVE_INVALID_CONSTANTS"=>"constants", "ORCA_ADAPTIVE_INVALID_TIMESTAMP"=>"timestamp",
            "ORCA_ADAPTIVE_TRADE_NOT_ENABLED"=>"trade-not-enabled",_=>panic!("unexpected error {e}")}});
        }
    };
    let mut trace = vec![snapshot(&m)];
    for op in req["operations"].as_array().unwrap() {
        let p = |k: &str| op[k].as_str().unwrap().parse::<u128>().unwrap();
        let mut target = None;
        match op["op"].as_str().unwrap() {
            "update" => m.update_volatility_accumulator().unwrap(),
            "advance" => m.advance_tick_group().unwrap(),
            "skip" => m
                .advance_tick_group_after_skip(
                    p("price"),
                    p("next_tick_price"),
                    i32::try_from(op["next_tick_index"].as_i64().unwrap()).unwrap(),
                )
                .unwrap(),
            "target" => {
                let (price, skip) = m
                    .get_bounded_sqrt_price_target(p("target"), p("liquidity"))
                    .unwrap();
                target = Some(serde_json::json!({"price":price.to_string(),"skip":skip}));
            }
            "major" => m.update_major_swap_timestamp(p("pre"), p("post")).unwrap(),
            _ => panic!("unknown operation"),
        }
        let mut state = snapshot(&m);
        if let Some(t) = target {
            state["target"] = t;
        }
        trace.push(state);
    }
    serde_json::json!({"trace":trace,"next_info":m.finish()})
}
#[test]
fn original_native_corpus_matches_every_state_and_group() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/orca-adaptive.json")).unwrap();
    assert_eq!(fixture["online_profile_enabled"], false);
    assert_eq!(
        fixture["identity"]["revision"],
        "cc683ec92acb5e7606caa3d8e514d9383e9c7c11"
    );
    assert!(
        fixture["identity"]["source_files_verified"]
            .as_u64()
            .unwrap()
            > 100
    );
    assert_eq!(
        fixture["identity"]["executed_binary_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 100, "corpus must exercise many transitions");
    for case in cases {
        assert_eq!(
            execute(&case["request"]),
            case["expected"],
            "native case {}",
            case["name"]
        );
    }
    let sequences = fixture["sequences"].as_array().unwrap();
    assert!(sequences.len() >= 2);
    for sequence in sequences {
        let mut state = sequence["initial_info"].clone();
        for trade in sequence["trades"].as_array().unwrap() {
            let mut req = trade["request"].clone();
            req["info"] = state;
            let actual = execute(&req);
            assert_eq!(actual, trade["expected"], "sequence {}", sequence["name"]);
            state = actual["next_info"].clone();
        }
    }
}
#[test]
fn invalid_external_price_proofs_do_not_mutate_state() {
    let mut m = FeeRateManager::begin(context(0, 101, false), info()).unwrap();
    let before = snapshot(&m);
    assert!(m
        .advance_tick_group_after_skip(1u128 << 64, 1u128 << 64, 8)
        .is_err());
    assert_eq!(snapshot(&m), before);
    assert!(m.get_bounded_sqrt_price_target(0, 10).is_err());
    assert!(m.update_major_swap_timestamp(0, 1u128 << 64).is_err());
    assert_eq!(snapshot(&m), before);
}

#[test]
fn shifted_min_tick_remains_a_valid_native_start_state() {
    let mut state = info();
    state.constants.tick_group_size = 1;
    state.variables.tick_group_index_reference = -443637;
    let mut manager = FeeRateManager::begin(context(-443637, 101, false), state).unwrap();
    manager.update_volatility_accumulator().unwrap();
    assert_eq!(manager.tick_group_index(), -443637);
    assert_eq!(
        manager
            .get_bounded_sqrt_price_target(1u128 << 64, 1)
            .unwrap(),
        (4295048016, false)
    );
}
