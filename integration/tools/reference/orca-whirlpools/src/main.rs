use anchor_lang::prelude::Pubkey;
use orca_whirlpools_reference::{base_pool, fixture, quote, tick_array_address, tickmath_vectors};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    str::FromStr,
};
fn main() {
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let result = std::panic::catch_unwind(|| {
            let req: Value = serde_json::from_str(&line).expect("JSON request");
            match req["action"].as_str().unwrap_or("quote") {
                "base" => base_pool(), "fixture" => fixture(), "tickmath" => tickmath_vectors(),
                "price" => json!({"sqrt_price_x64":whirlpool::math::sqrt_price_from_tick_index(req["tick"].as_i64().unwrap() as i32).to_string()}),
                "array_address" => { let (p,b) = tick_array_address(&Pubkey::from_str(req["pool"].as_str().unwrap()).unwrap(), req["start"].as_i64().unwrap() as i32); json!({"address":p.to_string(),"bump":b}) },
                "quote" => match quote(&req["pool"], req["input"].as_str().unwrap(), req["zero_for_one"].as_bool().unwrap()) {
                    Ok(q) => json!({"expected":{"amount_out":q.amount_out.to_string(),"next_pool":q.next_pool},"raw_reference":q.audit}),
                    Err(e) => json!({"expected":{"reverted":true},"revert_reason":e})
                }, _ => panic!("unknown action")
            }
        }).unwrap_or_else(|_| json!({"fatal":"invalid request"}));
        serde_json::to_writer(&mut out, &result).unwrap();
        out.write_all(b"\n").unwrap();
        out.flush().unwrap();
    }
}
