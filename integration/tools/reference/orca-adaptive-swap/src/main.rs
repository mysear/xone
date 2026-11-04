//! Calls unchanged original swap_manager, tick managers, Oracle and Whirlpool post-update.
#[path = "../identity.rs"]
mod identity;
use anchor_lang::prelude::Pubkey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::{RefCell, RefMut},
    io::{self, BufRead, Write},
};
use whirlpool::{
    manager::swap_manager,
    math::sqrt_price_from_tick_index,
    state::{
        AdaptiveFeeConstants, AdaptiveFeeInfo, AdaptiveFeeVariables, FixedTickArray,
        LoadedTickArrayMut, Oracle, Tick, TickArrayType, Whirlpool, WhirlpoolRewardInfo,
    },
    util::SwapTickSequence,
};
fn n(v: &Value, k: &str) -> u64 {
    v[k].as_u64().unwrap()
}
fn i(v: &Value, k: &str) -> i32 {
    v[k].as_i64().unwrap().try_into().unwrap()
}
fn u(v: &Value, k: &str) -> u128 {
    v[k].as_str().unwrap().parse().unwrap()
}
fn key(v: &Value) -> Pubkey {
    Pubkey::new_from_array(serde_json::from_value(v.clone()).unwrap())
}
fn adaptive(v: &Value) -> AdaptiveFeeInfo {
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
            reserved: serde_json::from_value(c["reserved"].clone()).unwrap(),
        },
        variables: AdaptiveFeeVariables {
            last_reference_update_timestamp: n(a, "last_reference_update_timestamp"),
            last_major_swap_timestamp: n(a, "last_major_swap_timestamp"),
            volatility_reference: n(a, "volatility_reference").try_into().unwrap(),
            tick_group_index_reference: i(a, "tick_group_index_reference"),
            volatility_accumulator: n(a, "volatility_accumulator").try_into().unwrap(),
            reserved: serde_json::from_value(a["reserved"].clone()).unwrap(),
        },
    }
}
fn variables(v: AdaptiveFeeVariables) -> Value {
    let (rt, mt, vr, gr, va, r) = (
        v.last_reference_update_timestamp,
        v.last_major_swap_timestamp,
        v.volatility_reference,
        v.tick_group_index_reference,
        v.volatility_accumulator,
        v.reserved,
    );
    json!({"last_reference_update_timestamp":rt,"last_major_swap_timestamp":mt,"volatility_reference":vr,"tick_group_index_reference":gr,"volatility_accumulator":va,"reserved":r})
}
fn load(cell: &RefCell<FixedTickArray>) -> LoadedTickArrayMut<'_> {
    RefMut::map(cell.borrow_mut(), |a| a as &mut dyn TickArrayType)
}
fn run(req: &Value) -> Value {
    if req["action"] == "prices" {
        return json!(req["ticks"]
            .as_array()
            .unwrap()
            .iter()
            .map(
                |t| sqrt_price_from_tick_index(t.as_i64().unwrap().try_into().unwrap()).to_string()
            )
            .collect::<Vec<_>>());
    }
    let s = &req["state"];
    let p = &req["params"];
    let left = p["a_to_b"].as_bool().unwrap();
    let mut pool = Whirlpool {
        tick_spacing: n(s, "tick_spacing").try_into().unwrap(),
        fee_rate: n(s, "fee_rate").try_into().unwrap(),
        protocol_fee_rate: n(s, "protocol_fee_rate").try_into().unwrap(),
        liquidity: u(s, "liquidity"),
        sqrt_price: u(s, "sqrt_price_x64"),
        tick_current_index: i(s, "tick_current_index"),
        protocol_fee_owed_a: n(s, "protocol_fee_owed_a"),
        protocol_fee_owed_b: n(s, "protocol_fee_owed_b"),
        fee_growth_global_a: u(s, "fee_growth_global_a"),
        fee_growth_global_b: u(s, "fee_growth_global_b"),
        reward_last_updated_timestamp: n(s, "reward_last_updated_timestamp"),
        ..Default::default()
    };
    for (out, r) in pool
        .reward_infos
        .iter_mut()
        .zip(s["reward_infos"].as_array().unwrap())
    {
        *out = WhirlpoolRewardInfo {
            mint: key(&r["mint"]),
            vault: key(&r["vault"]),
            extension: serde_json::from_value(r["extension"].clone()).unwrap(),
            emissions_per_second_x64: u(r, "emissions_per_second_x64"),
            growth_global_x64: u(r, "growth_global_x64"),
        };
    }
    let info = adaptive(&s["oracle"]["adaptive_fee_info"]);
    let mut oracle = Oracle {
        whirlpool: key(&s["oracle"]["whirlpool"]),
        trade_enable_timestamp: n(&s["oracle"], "trade_enable_timestamp"),
        adaptive_fee_constants: info.constants,
        adaptive_fee_variables: info.variables,
        reserved: serde_json::from_value::<Vec<u8>>(s["oracle"]["reserved"].clone())
            .unwrap()
            .try_into()
            .unwrap(),
    };
    let arrays = s["tick_arrays"].as_array().unwrap();
    let mut cells = Vec::new();
    for a in arrays {
        let start = i(a, "start_tick_index");
        let mut ticks = [Tick::default(); 88];
        for t in a["ticks"].as_array().unwrap() {
            let slot = ((i(t, "tick") - start) / i32::from(pool.tick_spacing)) as usize;
            ticks[slot] = Tick {
                initialized: true,
                liquidity_gross: u(t, "liquidity_gross"),
                liquidity_net: t["liquidity_net"].as_str().unwrap().parse().unwrap(),
                fee_growth_outside_a: u(t, "fee_growth_outside_a"),
                fee_growth_outside_b: u(t, "fee_growth_outside_b"),
                reward_growths_outside: std::array::from_fn(|idx| {
                    t["reward_growths_outside"][idx]
                        .as_str()
                        .unwrap()
                        .parse()
                        .unwrap()
                }),
            };
        }
        cells.push(RefCell::new(FixedTickArray {
            start_tick_index: start,
            ticks,
            whirlpool: key(&a["whirlpool"]),
        }));
    }
    let span = 88 * i32::from(pool.tick_spacing);
    let shift = if left {
        0
    } else {
        i32::from(pool.tick_spacing)
    };
    let first = arrays
        .iter()
        .position(|a| {
            pool.tick_current_index >= i(a, "start_tick_index") - shift
                && pool.tick_current_index < i(a, "start_tick_index") + span - shift
        })
        .unwrap();
    let order: Vec<usize> = if left {
        (0..=first).rev().collect()
    } else {
        (first..arrays.len()).collect()
    };
    let mut sequence = SwapTickSequence::new(
        load(&cells[order[0]]),
        order.get(1).map(|j| load(&cells[*j])),
        order.get(2).map(|j| load(&cells[*j])),
    );
    let update = match swap_manager::swap(
        &pool,
        &mut sequence,
        n(p, "amount"),
        u(p, "sqrt_price_limit"),
        true,
        left,
        n(p, "timestamp"),
        &Some(info),
    ) {
        Ok(v) => v,
        Err(e) => return json!({"error":e.to_string()}),
    };
    let (input, output) = if left {
        (update.amount_a, update.amount_b)
    } else {
        (update.amount_b, update.amount_a)
    };
    pool.update_after_swap(
        update.next_liquidity,
        update.next_tick_index,
        update.next_sqrt_price,
        update.next_fee_growth_global,
        update.next_reward_infos,
        update.next_protocol_fee,
        left,
        n(p, "timestamp"),
    );
    oracle.update_adaptive_fee_variables(update.next_adaptive_fee_info.as_ref().unwrap().variables);
    drop(sequence);
    let mut next = s.clone();
    next["sqrt_price_x64"] = json!(pool.sqrt_price.to_string());
    next["tick_current_index"] = json!(pool.tick_current_index);
    next["liquidity"] = json!(pool.liquidity.to_string());
    next["fee_growth_global_a"] = json!(pool.fee_growth_global_a.to_string());
    next["fee_growth_global_b"] = json!(pool.fee_growth_global_b.to_string());
    next["protocol_fee_owed_a"] = json!(pool.protocol_fee_owed_a);
    next["protocol_fee_owed_b"] = json!(pool.protocol_fee_owed_b);
    next["reward_last_updated_timestamp"] = json!(pool.reward_last_updated_timestamp);
    for (out, r) in next["reward_infos"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(pool.reward_infos)
    {
        out["growth_global_x64"] = json!(r.growth_global_x64.to_string());
    }
    next["oracle"]["adaptive_fee_info"]["variables"] = variables(oracle.adaptive_fee_variables);
    for (a, cell) in next["tick_arrays"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(cells)
    {
        let array = cell.into_inner();
        for t in a["ticks"].as_array_mut().unwrap() {
            let original = array.ticks
                [((i(t, "tick") - array.start_tick_index) / i32::from(pool.tick_spacing)) as usize];
            let (a, b, r) = (
                original.fee_growth_outside_a,
                original.fee_growth_outside_b,
                original.reward_growths_outside,
            );
            t["fee_growth_outside_a"] = json!(a.to_string());
            t["fee_growth_outside_b"] = json!(b.to_string());
            t["reward_growths_outside"] = json!(r.map(|v| v.to_string()));
        }
    }
    json!({"amount_in":input,"amount_out":output,"amount_remaining":n(p,"amount")-input,
        "lp_fee":update.lp_fee,"protocol_fee":update.next_protocol_fee,"next_state":next})
}
fn main() {
    let mut observed = identity::verify(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), false);
    let compiled: Value = serde_json::from_str(env!("ORCA_REFERENCE_CLOSURE")).unwrap();
    observed["compiler"] = compiled["compiler"].clone();
    assert_eq!(
        observed, compiled,
        "reference closure changed after compilation"
    );
    observed["executed_binary_sha256"] = json!(format!(
        "{:x}",
        Sha256::digest(std::fs::read(std::env::current_exe().unwrap()).unwrap())
    ));
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
