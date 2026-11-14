use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn main() {
 let mut out=io::BufWriter::new(io::stdout().lock());
 for line in io::stdin().lock().lines() {
  let req:Value=serde_json::from_str(&line.unwrap()).unwrap();
  let result=match req["action"].as_str().unwrap_or("quote") {
   "reward_update" => solana_advanced_raydium_clmm_reference::reward_update(&serde_json::from_value(req["pool"].clone()).unwrap(), &req["outside"].as_array().unwrap().iter().map(|v|v.as_str().unwrap().parse::<u128>().unwrap()).collect::<Vec<_>>().try_into().unwrap()),
   "base" => serde_json::to_value(solana_advanced_raydium_clmm_reference::base_pool()).unwrap(),
   "fixture" => solana_advanced_raydium_clmm_reference::fixture_for(&serde_json::from_value(req["pool"].clone()).unwrap()),
   "quote" => match solana_advanced_raydium_clmm_reference::quote(&serde_json::from_value(req["pool"].clone()).unwrap(),req["input"].as_str().unwrap(),req["zero_for_one"].as_bool().unwrap()) {
    Ok(q)=>json!({"amount_out":q.amount_out.to_string(),"next_pool":q.pool}),Err(e)=>json!({"reverted":true,"reason":e})
   },
   "quote_exact_out" => match solana_advanced_raydium_clmm_reference::quote_exact_out(&serde_json::from_value(req["pool"].clone()).unwrap(),req["output"].as_str().unwrap(),req["zero_for_one"].as_bool().unwrap()) {
    Ok(q)=>json!({"amount_in":q.amount_in.to_string(),"amount_out":q.amount_out.to_string(),"next_pool":q.pool}),Err(e)=>json!({"reverted":true,"reason":e})
   },
   "step_exact_out" => {
    let result=std::panic::catch_unwind(||raydium_clmm::libraries::swap_math::compute_swap(req["price"].as_str().unwrap().parse().unwrap(),req["target_price"].as_str().unwrap().parse().unwrap(),req["liquidity"].as_str().unwrap().parse().unwrap(),req["output"].as_str().unwrap().parse().unwrap(),req["fee"].as_u64().unwrap()as u32,false,req["zero_for_one"].as_bool().unwrap(),req["fee_on_input"].as_bool().unwrap()));
    match result {Ok(Ok(q))=>json!({"amount_in":q.amount_in.to_string(),"amount_out":q.amount_out.to_string(),"next_price":q.sqrt_price_next_x64.to_string(),"fee_amount":q.fee_amount.to_string()}),Ok(Err(e))=>json!({"reverted":true,"reason":e.to_string()}),Err(_)=>json!({"reverted":true,"reason":"original step panic"})}
   },
   "target_peer" => serde_json::to_value(solana_advanced_raydium_clmm_reference::target_peer(&serde_json::from_value(req["pool"].clone()).unwrap(),req["seed"].as_u64().unwrap()as u32)).unwrap(),
   "price" => json!({"sqrt_price_x64":raydium_clmm::libraries::tick_math::get_sqrt_price_at_tick(req["tick"].as_i64().unwrap()as i32).unwrap().to_string()}),
   "fixture_target" => {let p:solana_advanced_raydium_clmm_reference::Pool=serde_json::from_value(req["pool"].clone()).unwrap();if p.raydium_clmm.bitmap_extension.is_some(){solana_advanced_raydium_clmm_reference::fixture_from_bitmap(&p)}else{solana_advanced_raydium_clmm_reference::fixture_for(&p)}},
   _=>panic!("unknown action"),
  };
  serde_json::to_writer(&mut out,&result).unwrap();out.write_all(b"\n").unwrap();out.flush().unwrap();
 }
}
