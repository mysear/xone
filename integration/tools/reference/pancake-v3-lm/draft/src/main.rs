use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};
use serde_json::{json, Value};
use std::{error::Error, fs};
use xone_domain::{Amount, Pool};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn n(value: &Value) -> BigUint {
    value.as_str().unwrap().parse().unwrap()
}
fn mask(bits: usize) -> BigUint {
    (BigUint::one() << bits) - 1u8
}
fn add(a: &BigUint, b: &BigUint, bits: usize) -> BigUint {
    (a + b) & mask(bits)
}
fn sub(a: &BigUint, b: &BigUint, bits: usize) -> BigUint {
    ((a + (BigUint::one() << bits)) - b) & mask(bits)
}
fn put(value: &mut Value, key: &str, number: BigUint) {
    value[key] = json!(number.to_string());
}
fn signed(value: &Value) -> i128 {
    value.as_str().unwrap().parse().unwrap()
}
fn wrap(value: i128, bits: u32) -> i128 {
    let n = value & ((1i128 << bits) - 1);
    if n & (1i128 << (bits - 1)) != 0 {
        n - (1i128 << bits)
    } else {
        n
    }
}
fn transform(last: &Value, time: u32, tick: i32, liquidity: &BigUint) -> Value {
    let dt = time.wrapping_sub(n(&last["blockTimestamp"]).to_u32().unwrap());
    json!({"blockTimestamp":time.to_string(),"tickCumulative":wrap(signed(&last["tickCumulative"])+i128::from(tick)*i128::from(dt),56).to_string(),"secondsPerLiquidityCumulativeX128":add(&n(&last["secondsPerLiquidityCumulativeX128"]),&((BigUint::from(dt)<<128usize)/liquidity.clone().max(BigUint::one())),160).to_string(),"initialized":true})
}
fn quote(reference: &Value, before: &Value, zero: bool, input: &str) -> Result<(Value, String)> {
    let dep = &reference["deployment"];
    let chef = &before["masterChef"];
    let total = n(&chef["totalAllocPoint"]);
    let rate = if total.is_zero() {
        BigUint::zero()
    } else {
        let product = n(&chef["latestPeriodCakePerSecond"]) * n(&chef["poolInfo"]["allocPoint"]);
        if product.bits() > 256 {
            return Err("PANCAKE_MASTER_CHEF_CHECKED_PRODUCT".into());
        }
        product / total
    };
    if rate != n(&before["period"]["rewardPerSecond"])
        || chef["latestPeriodEndTime"] != before["period"]["endTime"]
        || n(&chef["pid"]).is_zero()
    {
        return Err("PANCAKE_MASTER_CHEF_VIEW".into());
    }

    let ticks:Vec<Value>=before["ticks"].as_array().unwrap().iter().map(|t|json!({"tick":t["index"],"liquidity_gross":t["liquidityGross"],"liquidity_net":t["liquidityNet"]})).collect();
    let words: Vec<Value> = before["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| json!({"word_pos":w["index"],"bitmap":w["bitmap"]}))
        .collect();
    let parent: Pool = serde_json::from_value(
        json!({"chain_key":"evm:31337","pool_id":dep["pool"]["address"],"protocol":"pancake-v3","adapter_version":"pancake-v3/core-1","token0":dep["token0"]["address"],"token1":dep["token1"]["address"],"reserve0":"0","reserve1":"0","token_behavior":"standard","fee_numerator":500,"fee_denominator":1000000,"v3":{"sqrt_price_x96":before["slot0"]["sqrtPriceX96"],"tick":signed(&before["slot0"]["tick"]),"liquidity":before["liquidity"],"tick_spacing":10,"factory":dep["factory"]["address"],"code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","max_steps":8192,"fee_protocol":n(&before["slot0"]["feeProtocol"]).to_u32().unwrap(),"tick_words":words,"ticks":ticks}}),
    )?;
    let time = reference["fixed_timestamp"].as_u64().unwrap() as u32;
    let mut next = before.clone();
    // Original MasterChef output is observed before this isolated mathematical projection.
    let lm = &mut next["lm"];
    let last = n(&lm["lastRewardTimestamp"]).to_u32().unwrap();
    if time > last {
        let liquidity = n(&lm["liquidity"]);
        if !liquidity.is_zero() {
            let end = (&n(&before["period"]["endTime"]) & mask(32))
                .to_u32()
                .unwrap();
            let duration = if end > time {
                time - last
            } else if end > last {
                end - last
            } else {
                0
            };
            if duration != 0 {
                let rate = n(&before["period"]["rewardPerSecond"]);
                let scaled = (&rate << 128usize) / BigUint::from(1_000_000_000_000u64);
                if scaled.bits() > 256 {
                    return Err("PANCAKE_REWARD_FULLMATH_WIDTH".into());
                }
                let growth = BigUint::from(duration) * scaled / liquidity;
                if growth.bits() > 256 {
                    return Err("PANCAKE_REWARD_GROWTH_WIDTH".into());
                }
                let updated = add(&n(&lm["rewardGrowthGlobalX128"]), &growth, 256);
                put(lm, "rewardGrowthGlobalX128", updated);
            }
        }
        lm["lastRewardTimestamp"] = json!(time.to_string());
    }
    let (swap, trace) =
        xone_protocol_math::v3::quote_exact_in_with_trace(&parent, &Amount(input.parse()?), zero)?;
    let core = swap.next_pool.v3.as_ref().unwrap();
    let liquidity = n(&before["liquidity"]);
    let tick = signed(&before["slot0"]["tick"]) as i32;
    let oi = n(&before["slot0"]["observationIndex"]).to_usize().unwrap();
    let latest = transform(&before["observations"][oi], time, tick, &liquidity);
    let fee_field = if zero {
        "feeGrowthGlobal0X128"
    } else {
        "feeGrowthGlobal1X128"
    };
    let protocol_field = if zero { "token0" } else { "token1" };
    let rate = n(&before["slot0"]["feeProtocol"]).to_u32().unwrap();
    let rate = if zero { rate % 65536 } else { rate / 65536 };
    let mut global = n(&next[fee_field]);
    let mut protocol = BigUint::zero();
    for step in trace {
        let protocol_delta = &step.fee_amount * rate / 10000u32;
        protocol = add(&protocol, &(&protocol_delta & mask(128)), 128);
        let lp = &step.fee_amount - protocol_delta;
        if !step.liquidity.is_zero() {
            global = add(&global, &((lp << 128usize) / &step.liquidity), 256);
        }
        if let Some(index) = step.crossed_tick {
            let info = next["ticks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|t| t["index"] == index)
                .unwrap();
            for key in ["feeGrowthOutside0X128", "feeGrowthOutside1X128"] {
                let g = if key
                    == if zero {
                        "feeGrowthOutside0X128"
                    } else {
                        "feeGrowthOutside1X128"
                    } {
                    global.clone()
                } else {
                    n(&before[if key == "feeGrowthOutside0X128" {
                        "feeGrowthGlobal0X128"
                    } else {
                        "feeGrowthGlobal1X128"
                    }])
                };
                let value = sub(&g, &n(&info[key]), 256);
                put(info, key, value);
            }
            info["tickCumulativeOutside"] = json!(wrap(
                signed(&latest["tickCumulative"]) - signed(&info["tickCumulativeOutside"]),
                56
            )
            .to_string());
            let value = sub(
                &n(&latest["secondsPerLiquidityCumulativeX128"]),
                &n(&info["secondsPerLiquidityOutsideX128"]),
                160,
            );
            put(info, "secondsPerLiquidityOutsideX128", value);
            let value = time.wrapping_sub(n(&info["secondsOutside"]).to_u32().unwrap());
            info["secondsOutside"] = json!(value.to_string());
            let lm = &mut next["lm"];
            let reward = n(&lm["rewardGrowthGlobalX128"]);
            let mut delta = 0i128;
            if let Some(info) = lm["ticks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|t| t["index"] == index)
            {
                if !n(&info["liquidityGross"]).is_zero() {
                    let value = sub(&reward, &n(&info["rewardGrowthOutsideX128"]), 256);
                    put(info, "rewardGrowthOutsideX128", value);
                    delta = signed(&info["liquidityNet"]);
                }
            }
            let magnitude = BigUint::from(delta.unsigned_abs());
            let current = n(&lm["liquidity"]);
            let updated = if (delta < 0) ^ zero {
                if current < magnitude {
                    return Err("PANCAKE_LM_LIQUIDITY_UNDERFLOW".into());
                }
                current - magnitude
            } else {
                current + magnitude
            };
            if updated.bits() > 128 {
                return Err("PANCAKE_LM_LIQUIDITY_OVERFLOW".into());
            }
            put(lm, "liquidity", updated);
        }
    }
    put(&mut next, fee_field, global);
    let pf = add(&n(&before["protocolFees"][protocol_field]), &protocol, 128);
    put(&mut next["protocolFees"], protocol_field, pf);
    if core.tick != tick && n(&before["observations"][oi]["blockTimestamp"]) != BigUint::from(time)
    {
        let cardinality = n(&before["slot0"]["observationCardinality"])
            .to_usize()
            .unwrap();
        let cardinality_next = n(&before["slot0"]["observationCardinalityNext"])
            .to_usize()
            .unwrap();
        let cardinality = if cardinality_next > cardinality && oi == cardinality - 1 {
            cardinality_next
        } else {
            cardinality
        };
        let index = (oi + 1) % cardinality;
        let mut latest = latest;
        latest["index"] = json!(index);
        next["observations"][index] = latest;
        next["slot0"]["observationIndex"] = json!(index.to_string());
        next["slot0"]["observationCardinality"] = json!(cardinality.to_string());
    }
    next["slot0"]["sqrtPriceX96"] = json!(core.sqrt_price_x96.to_string());
    next["slot0"]["tick"] = json!(core.tick.to_string());
    next["liquidity"] = json!(core.liquidity.to_string());
    let raw_in = if zero { "raw0" } else { "raw1" };
    let raw_out = if zero { "raw1" } else { "raw0" };
    let in_after = n(&before[raw_in]) + input.parse::<BigUint>()?;
    let out_before = n(&before[raw_out]);
    if out_before < swap.amount_out.0 {
        return Err("PANCAKE_RAW_BALANCE".into());
    }
    let out_after = out_before - &swap.amount_out.0;
    put(&mut next, raw_in, in_after);
    put(&mut next, raw_out, out_after);
    Ok((next, swap.amount_out.to_string()))
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let reference: Value = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let mut full = 0;
    let mut rejected = 0;
    let mut steps = 0;
    for case in reference["cases"].as_array().unwrap().iter().chain(
        reference["invalid_bank_cases"]
            .as_array()
            .into_iter()
            .flatten(),
    ) {
        let result = quote(
            &reference,
            &case["before"],
            case["zero_for_one"].as_bool().unwrap(),
            case["amount_in"].as_str().unwrap(),
        );
        if case["expected"]["reverted"] == true
            || case["expected"]["consumed_in"] != case["amount_in"]
        {
            assert!(result.is_err(), "partial/revert accepted {}", case["index"]);
            rejected += 1;
            continue;
        }
        let (next, output) = result.map_err(|e| format!("case {}: {}", case["index"], e))?;
        if output != case["expected"]["amount_out"].as_str().unwrap()
            || next != case["expected"]["after"]
        {
            fs::write(
                std::path::Path::new(&args[2]).with_extension("mismatch.json"),
                serde_json::to_string_pretty(
                    &json!({"index":case["index"],"actual":next,"expected":case["expected"]["after"],"output":output}),
                )?,
            )?;
            return Err(format!("PANCAKE_FULL_STATE_MISMATCH:{}", case["index"]).into());
        }
        full += 1;
    }
    for sequence in reference["sequences"].as_array().unwrap() {
        let mut carried = sequence["steps"][0]["before"].clone();
        for step in sequence["steps"].as_array().unwrap() {
            assert_eq!(carried, step["before"]);
            let (next, output) = quote(
                &reference,
                &carried,
                step["zero_for_one"].as_bool().unwrap(),
                step["amount_in"].as_str().unwrap(),
            )?;
            assert_eq!(next, step["expected"]["after"]);
            assert_eq!(output, step["expected"]["amount_out"].as_str().unwrap());
            carried = next;
            steps += 1;
        }
    }
    let report = json!({"passed":true,"full_output_and_poststate_matches":full,"partial_or_revert_rejected":rejected,"sequence_steps":steps,"authority":"isolated mathematical draft versus unmodified original Pool/LM/MasterChef execution; normal profile is not implemented","normal_profile_implemented":false,"all_requested_features_complete":false,"production_ready":false});
    fs::write(&args[2], serde_json::to_string_pretty(&report)?)?;
    println!("{}", report);
    Ok(())
}
