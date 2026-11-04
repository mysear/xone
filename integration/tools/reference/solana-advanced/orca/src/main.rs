use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn main() {
 let mut out=io::BufWriter::new(io::stdout().lock());
 for line in io::stdin().lock().lines() {
  let req:Value=serde_json::from_str(&line.unwrap()).unwrap();
  let result=match req["action"].as_str().unwrap_or("quote") {
   "reward_update" => solana_advanced_orca_reference::reward_update(&req["pool"], &req["outside"].as_array().unwrap().iter().map(|v|v.as_str().unwrap().parse::<u128>().unwrap()).collect::<Vec<_>>().try_into().unwrap()),
   "base" => solana_advanced_orca_reference::base_pool(),
   "fixture" => solana_advanced_orca_reference::fixture_for(&req["pool"]),
   "quote" => match solana_advanced_orca_reference::quote(&req["pool"],req["input"].as_str().unwrap(),req["zero_for_one"].as_bool().unwrap()) {
    Ok(q)=>json!({"amount_out":q.amount_out.to_string(),"next_pool":q.next_pool,"audit":q.audit}),Err(e)=>json!({"reverted":true,"reason":e})
   },
   _=>panic!("unknown action"),
  };
  serde_json::to_writer(&mut out,&result).unwrap();out.write_all(b"\n").unwrap();out.flush().unwrap();
 }
}
