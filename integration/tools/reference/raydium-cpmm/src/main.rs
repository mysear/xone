use raydium_cpmm_reference::{exact_out_corpus, fixture, generate, quote, Pool};
use serde_json::{json, Value};
use std::io::{self, Read};
fn main() {
    // Expected upstream unwrap failures are represented in structured oracle results.
    std::panic::set_hook(Box::new(|_| {}));
    let command = std::env::args().nth(1).unwrap_or_else(|| "generate".into());
    let out = match command.as_str() {
        "generate" => generate(),
        "exact-out-corpus" => exact_out_corpus(),
        "fixture" => fixture(),
        "quote" => {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text).unwrap();
            let v: Value = serde_json::from_str(&text).expect("quote JSON");
            let p: Pool = serde_json::from_value(v["pool"].clone()).expect("PoolJSON");
            match quote(
                &p,
                v["input"].as_str().expect("input string"),
                v["zero_for_one"].as_bool().expect("direction"),
            ) {
                Ok(q) => {
                    json!({"amount_out":q.amount_out.to_string(),"reserve0":q.pool.reserve0,"reserve1":q.pool.reserve1,"raydium_cpmm":q.pool.raydium_cpmm,"raw_reference":q.fees})
                }
                Err(reason) => json!({"reverted":true,"reason":reason}),
            }
        }
        _ => panic!("usage: raydium-cpmm-reference [generate|fixture|quote]"),
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
