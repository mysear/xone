//! This executable calls the unchanged upstream implementation, never the port.
#[path = "../identity.rs"]
mod identity;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, Write};
use whirlpool::{
    manager::fee_rate_manager::FeeRateManager,
    math::sqrt_price_from_tick_index,
    state::{AdaptiveFeeConstants, AdaptiveFeeInfo, AdaptiveFeeVariables},
};
fn n(v: &Value, k: &str) -> u64 {
    v[k].as_u64().unwrap()
}
fn signed(v: &Value, k: &str) -> i32 {
    i32::try_from(v[k].as_i64().unwrap()).unwrap()
}
fn p(v: &Value, k: &str) -> u128 {
    v[k].as_str().unwrap().parse().unwrap()
}
fn reserved(v: &Value) -> [u8; 16] {
    v.get("reserved")
        .map(|x| serde_json::from_value(x.clone()).unwrap())
        .unwrap_or([0; 16])
}
fn info(v: &Value) -> AdaptiveFeeInfo {
    let c = &v["constants"];
    let a = &v["variables"];
    AdaptiveFeeInfo {
        constants: AdaptiveFeeConstants {
            filter_period: n(c, "filter_period").try_into().unwrap(),
            decay_period: n(c, "decay_period").try_into().unwrap(),
            reduction_factor: n(c, "reduction_factor").try_into().unwrap(),
            adaptive_fee_control_factor: n(c, "adaptive_fee_control_factor").try_into().unwrap(),
            max_volatility_accumulator: n(c, "max_volatility_accumulator").try_into().unwrap(),
            tick_group_size: n(c, "tick_group_size").try_into().unwrap(),
            major_swap_threshold_ticks: n(c, "major_swap_threshold_ticks").try_into().unwrap(),
            reserved: reserved(c),
        },
        variables: AdaptiveFeeVariables {
            last_reference_update_timestamp: n(a, "last_reference_update_timestamp"),
            last_major_swap_timestamp: n(a, "last_major_swap_timestamp"),
            volatility_reference: n(a, "volatility_reference").try_into().unwrap(),
            tick_group_index_reference: signed(a, "tick_group_index_reference"),
            volatility_accumulator: n(a, "volatility_accumulator").try_into().unwrap(),
            reserved: reserved(a),
        },
    }
}
fn snapshot(m: &FeeRateManager) -> Value {
    match m {
        FeeRateManager::Adaptive {
            tick_group_index,
            adaptive_fee_constants: c,
            adaptive_fee_variables: v,
            core_tick_group_range_lower_bound: lower,
            core_tick_group_range_upper_bound: upper,
            ..
        } => {
            // Copy packed fields before serializing (never form unaligned references).
            let (fp, dp, rf, cf, mv, gs, mt, cr) = (
                c.filter_period,
                c.decay_period,
                c.reduction_factor,
                c.adaptive_fee_control_factor,
                c.max_volatility_accumulator,
                c.tick_group_size,
                c.major_swap_threshold_ticks,
                c.reserved,
            );
            let (rt, mtm, vr, gr, va, r) = (
                v.last_reference_update_timestamp,
                v.last_major_swap_timestamp,
                v.volatility_reference,
                v.tick_group_index_reference,
                v.volatility_accumulator,
                v.reserved,
            );
            let bound = |x: &Option<(i32, u128)>| x.map(|(g, p)| json!([g, p.to_string()]));
            json!({"tick_group_index":tick_group_index,"fee_rate":m.get_total_fee_rate(),"lower":bound(lower),"upper":bound(upper),
                "info":{"constants":{"filter_period":fp,"decay_period":dp,"reduction_factor":rf,"adaptive_fee_control_factor":cf,"max_volatility_accumulator":mv,"tick_group_size":gs,"major_swap_threshold_ticks":mt,"reserved":cr},
                "variables":{"last_reference_update_timestamp":rt,"last_major_swap_timestamp":mtm,"volatility_reference":vr,"tick_group_index_reference":gr,"volatility_accumulator":va,"reserved":r}}})
        }
        _ => unreachable!(),
    }
}
fn run(req: &Value) -> Value {
    if req["action"] == "prices" {
        return json!(req["ticks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let t = i32::try_from(t.as_i64().unwrap()).unwrap();
                assert!((-443636..=443636).contains(&t));
                sqrt_price_from_tick_index(t).to_string()
            })
            .collect::<Vec<_>>());
    }
    let ctx = &req["context"];
    let state = info(&req["info"]);
    let c = state.constants;
    if !AdaptiveFeeConstants::validate_constants(
        n(ctx, "tick_spacing").try_into().unwrap(),
        c.filter_period,
        c.decay_period,
        c.reduction_factor,
        c.adaptive_fee_control_factor,
        c.max_volatility_accumulator,
        c.tick_group_size,
        c.major_swap_threshold_ticks,
    ) {
        return json!({"error":"constants"});
    }
    if n(ctx, "timestamp") < n(ctx, "trade_enable_timestamp") {
        return json!({"error":"trade-not-enabled"});
    }
    let mut m = match FeeRateManager::new(
        ctx["a_to_b"].as_bool().unwrap(),
        signed(ctx, "current_tick_index"),
        n(ctx, "timestamp"),
        n(ctx, "static_fee_rate").try_into().unwrap(),
        &Some(state),
    ) {
        Ok(m) => m,
        Err(_) => return json!({"error":"timestamp"}),
    };
    let mut trace = vec![snapshot(&m)];
    for op in req["operations"].as_array().unwrap() {
        let mut target = None;
        match op["op"].as_str().unwrap() {
            "update" => m.update_volatility_accumulator().unwrap(),
            "advance" => m.advance_tick_group(),
            "skip" => m
                .advance_tick_group_after_skip(
                    p(op, "price"),
                    p(op, "next_tick_price"),
                    signed(op, "next_tick_index"),
                )
                .unwrap(),
            "target" => {
                let (price, skip) =
                    m.get_bounded_sqrt_price_target(p(op, "target"), p(op, "liquidity"));
                target = Some(json!({"price":price.to_string(),"skip":skip}));
            }
            "major" => m
                .update_major_swap_timestamp(n(ctx, "timestamp"), p(op, "pre"), p(op, "post"))
                .unwrap(),
            _ => panic!("unknown op"),
        }
        let mut s = snapshot(&m);
        if let Some(t) = target {
            s["target"] = t;
        }
        trace.push(s);
    }
    json!({"trace":trace,"next_info":snapshot(&m)["info"]})
}
fn main() {
    let mut observed = identity::verify(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), false);
    let compiled: Value = serde_json::from_str(env!("ORCA_REFERENCE_CLOSURE")).unwrap();
    observed["compiler"] = compiled["compiler"].clone();
    assert_eq!(
        observed, compiled,
        "reference closure changed after compilation"
    );
    let binary = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    observed["executed_binary_sha256"] = json!(format!("{:x}", Sha256::digest(binary)));
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let req: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let result = if req["action"] == "identity" {
            observed.clone()
        } else {
            run(&req)
        };
        serde_json::to_writer(&mut out, &result).unwrap();
        out.write_all(b"\n").unwrap();
        out.flush().unwrap();
    }
}
