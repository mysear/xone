use raydium_clmm_reference::{
    bitmap_fixture, bitmap_vectors, fixture, fixture_from_bitmap, generate, generate_bitmap,
    generate_seed, generate_token_fee, quote, seed_fixture, seed_identities, tick_vectors, Pool,
};
use serde_json::{json, Value};
use std::io::{self, Read};
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "generate".into());
    let timestamp = std::env::args().skip(2).collect::<Vec<_>>();
    let timestamp = if timestamp.is_empty() {
        raydium_clmm_reference::DYNAMIC_TIME_ANCHOR
    } else {
        assert_eq!(timestamp.len(), 2);
        assert_eq!(timestamp[0], "--timestamp");
        timestamp[1].parse::<u64>().unwrap()
    };
    let out = match cmd.as_str() {
        "generate-dynamic" => raydium_clmm_reference::generate_dynamic(timestamp),
        "dynamic-fixture" => raydium_clmm_reference::dynamic_fixture(timestamp),
        "fixture" => fixture(),
        "generate-seed" => generate_seed(),
        "seed-fixture" => seed_fixture(),
        "seed-identities" => seed_identities(),
        "bitmap-fixture" | "fixture-bitmap" => bitmap_fixture(),
        "fixture-from-pool" => {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            let pool: Pool = serde_json::from_value(v.get("pool").unwrap_or(&v).clone()).unwrap();
            fixture_from_bitmap(&pool)
        },
        "bitmap-vectors" => bitmap_vectors(),
        "generate-bitmap" => generate_bitmap(),
        "generate" => generate(),
        "generate-token-fee" => generate_token_fee(),
        "tick-vectors" => tick_vectors(),
        "quote" => {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            let pool: Pool = serde_json::from_value(v["pool"].clone()).unwrap();
            match quote(
                &pool,
                v["input"].as_str().unwrap(),
                v["zero_for_one"].as_bool().unwrap(),
            ) {
                Ok(q) => json!({"amount_out":q.amount_out.to_string(),"next_pool":q.pool}),
                Err(reason) => json!({"reverted":true,"reason":reason}),
            }
        }
        _ => panic!("usage: raydium-clmm-reference [generate|generate-token-fee|generate-bitmap|generate-seed|seed-fixture|seed-identities|bitmap-fixture|fixture|tick-vectors|quote]"),
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
