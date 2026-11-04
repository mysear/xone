use num_bigint::BigUint;
use serde_json::{json, Value};
use std::sync::OnceLock;
use xone_domain::{
    v2_swap::{ADAPTER, FACTORY_CODE_SHA256, PAIR_CODE_SHA256, PROFILE},
    Amount, Pool,
};
use xone_protocol_math::quote_exact_in;
fn amount(n: u64) -> Amount {
    n.to_string().parse().unwrap()
}
fn pool(donate0: u64, donate1: u64) -> Pool {
    serde_json::from_value(json!({"chain_key":"evm:31337","pool_id":format!("0x{}","1".repeat(40)),"protocol":PROFILE,"token0":format!("0x{}","2".repeat(40)),"token1":format!("0x{}","3".repeat(40)),"reserve0":"1000","reserve1":"2000","fee_numerator":3,"fee_denominator":1000,"adapter_version":ADAPTER,"token_behavior":"standard","v2":{"factory":format!("0x{}","4".repeat(40)),"code_sha256":PAIR_CODE_SHA256,"swap":{"raw_balance0":(1000+donate0).to_string(),"raw_balance1":(2000+donate1).to_string(),"block_timestamp_last":1790812700,"price0_cumulative_last":"0","price1_cumulative_last":"0","unlocked":true,"factory_code_sha256":FACTORY_CODE_SHA256,"bank_block_number":12,"bank_block_hash":format!("0x{}","a".repeat(64)),"bank_unix_timestamp":1790812800}}})).unwrap()
}
fn native() -> &'static Value {
    static NATIVE: OnceLock<Value> = OnceLock::new();
    NATIVE.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/v2-complete.json"
            ))
            .unwrap(),
        )
        .unwrap()
    })
}
#[test]
fn complete_swap_uses_raw_balances_for_both_directions_and_keeps_legacy_math() {
    for forward in [true, false] {
        let p = pool(50, 50);
        let original = p.clone();
        let result = quote_exact_in(&p, &amount(100), forward).unwrap();
        assert_eq!(result.amount_out, amount(if forward { 181 } else { 47 }));
        assert_eq!(
            result.next_pool.reserve0,
            amount(if forward { 1150 } else { 1003 })
        );
        assert_eq!(
            result.next_pool.reserve1,
            amount(if forward { 1869 } else { 2150 })
        );
        let s = result.next_pool.v2.as_ref().unwrap().swap.as_ref().unwrap();
        assert_eq!(s.raw_balance0, result.next_pool.reserve0);
        assert_eq!(s.raw_balance1, result.next_pool.reserve1);
        assert_eq!(s.block_timestamp_last, 1790812800);
        assert_eq!(
            s.price0_cumulative_last,
            Amount(BigUint::from(200u16) << 112usize)
        );
        assert_eq!(
            s.price1_cumulative_last,
            Amount(BigUint::from(50u8) << 112usize)
        );
        assert_eq!(p, original);
        assert_eq!(result.consumed_in, amount(100));
        assert_eq!(result.remaining_in, Amount::zero());
        let mut legacy = p.clone();
        legacy.protocol = "uniswap-v2".into();
        legacy.adapter_version = "uniswap-v2/1".into();
        legacy.v2.as_mut().unwrap().swap = None;
        assert_eq!(
            quote_exact_in(&legacy, &amount(100), forward)
                .unwrap()
                .amount_out,
            result.amount_out
        );
    }
}
#[test]
fn complete_swap_rejects_deficit_k_overflow_and_zero_output_without_partial_fill() {
    for bad in 0..7 {
        let mut p = pool(0, 0);
        let mut input = amount(100);
        match bad {
            0 => p.v2.as_mut().unwrap().swap.as_mut().unwrap().raw_balance0 = amount(999),
            1 => p.v2.as_mut().unwrap().swap.as_mut().unwrap().raw_balance1 = amount(1999),
            2 => {
                p.v2.as_mut().unwrap().swap.as_mut().unwrap().raw_balance0 =
                    Amount((BigUint::from(1u8) << 112usize) - BigUint::from(1u8))
            }
            3 => {
                p.v2.as_mut().unwrap().swap.as_mut().unwrap().raw_balance0 =
                    Amount((BigUint::from(1u8) << 256usize) - BigUint::from(1u8))
            }
            4 => input = Amount::zero(),
            5 => {
                p.reserve1 = Amount::zero();
                p.v2.as_mut().unwrap().swap.as_mut().unwrap().raw_balance1 = Amount::zero();
            }
            _ => input = Amount((BigUint::from(1u8) << 256usize) - BigUint::from(1u8)),
        };
        assert!(quote_exact_in(&p, &input, true).is_err(), "bad={bad}");
    }
}
#[test]
fn shared_steps_carry_the_donation_once_and_update_cumulatives_once_per_clock() {
    let original = pool(50, 0);
    let first = quote_exact_in(&original, &amount(100), true).unwrap();
    let second = quote_exact_in(&first.next_pool, &amount(100), true).unwrap();
    assert_eq!(first.amount_out, amount(181));
    assert_eq!(second.amount_out, amount(145));
    assert_eq!(second.next_pool.reserve0, amount(1250));
    assert_eq!(second.next_pool.reserve1, amount(1674));
    let a = first.next_pool.v2.unwrap().swap.unwrap();
    let b = second.next_pool.v2.unwrap().swap.unwrap();
    assert_eq!(a.price0_cumulative_last, b.price0_cumulative_last);
    assert_eq!(a.price1_cumulative_last, b.price1_cumulative_last);
    assert_eq!(b.bank_block_number, 12);
}
#[test]
fn original_seventeen_aligned_clock_swaps_match_complete_normal_dispatch_poststate() {
    let mut seen = 0;
    for c in native()["cases"].as_array().unwrap() {
        if c["expected"]["status"] != "success"
            || c["execution_timestamp"] != c["pool"]["v2"]["swap"]["bank_unix_timestamp"]
        {
            continue;
        }
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        let q = quote_exact_in(&p, &input, c["direction"] == "token0-to-token1").unwrap();
        assert_eq!(
            q.amount_out.to_string(),
            c["expected"]["amount_out"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        let expected: Pool = serde_json::from_value(c["expected"]["pool_after"].clone()).unwrap();
        assert_eq!(q.next_pool, expected, "{}", c["name"]);
        seen += 1;
    }
    assert_eq!(seen, 17);
}

#[test]
fn original_twenty_swaps_and_four_reverts_match_explicit_clock_settlement() {
    use xone_protocol_math::v2_complete::{quote_transition, settle_swap};
    let (mut success, mut rejected) = (0, 0);
    for c in native()["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let state = p.v2.as_ref().unwrap().swap.as_ref().unwrap();
        let time = c["execution_timestamp"].as_u64().unwrap();
        let forward = c["direction"] == "token0-to-token1";
        let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        if c["expected"]["status"] == "success" {
            let t =
                quote_transition([&p.reserve0, &p.reserve1], state, &input, forward, time).unwrap();
            let mut post = p.clone();
            post.reserve0 = t.reserve0.clone();
            post.reserve1 = t.reserve1.clone();
            post.v2.as_mut().unwrap().swap = Some(t.state.clone());
            let expected: Pool =
                serde_json::from_value(c["expected"]["pool_after"].clone()).unwrap();
            assert_eq!(post, expected, "{}", c["name"]);
            for (actual, key) in [
                (&t.amount0_in, "amount0_in"),
                (&t.amount1_in, "amount1_in"),
                (&t.amount0_out, "amount0_out"),
                (&t.amount1_out, "amount1_out"),
            ] {
                assert_eq!(
                    actual.to_string(),
                    c["expected"]["swap_event"][key].as_str().unwrap(),
                    "{} {key}",
                    c["name"]
                );
            }
            success += 1;
        } else {
            let data = c["transaction"]["data"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x");
            if &data[..8] == "022c0d9f" {
                let out0 = Amount(BigUint::parse_bytes(&data.as_bytes()[8..72], 16).unwrap());
                let out1 = Amount(BigUint::parse_bytes(&data.as_bytes()[72..136], 16).unwrap());
                let raw0 = Amount(&state.raw_balance0.0 - &out0.0);
                let raw1 = Amount(&state.raw_balance1.0 - &out1.0);
                assert_eq!(
                    settle_swap(
                        [&p.reserve0, &p.reserve1],
                        state,
                        [&out0, &out1],
                        [&raw0, &raw1],
                        time
                    )
                    .unwrap_err()
                    .0,
                    "V2_SWAP_INVARIANT",
                    "{}",
                    c["name"]
                );
            } else {
                assert_eq!(&data[..8], "38ed1739");
                assert_eq!(
                    quote_transition([&p.reserve0, &p.reserve1], state, &input, forward, time)
                        .unwrap_err()
                        .0,
                    "V2_SWAP_RESERVE_OVERFLOW",
                    "{}",
                    c["name"]
                );
            }
            rejected += 1;
        }
    }
    assert_eq!((success, rejected), (20, 4));
}
#[test]
fn explicit_clock_wraps_uint32_and_uint256_without_changing_source_identity() {
    use xone_protocol_math::v2_complete::quote_transition;
    let mut p = pool(0, 0);
    let source_time = (1u64 << 32) + 5;
    let state = p.v2.as_mut().unwrap().swap.as_mut().unwrap();
    state.bank_unix_timestamp = source_time;
    state.block_timestamp_last = u32::MAX - 4;
    state.price0_cumulative_last = Amount((BigUint::from(1u8) << 256usize) - BigUint::from(1u8));
    let t = quote_transition(
        [&p.reserve0, &p.reserve1],
        state,
        &amount(100),
        true,
        source_time,
    )
    .unwrap();
    assert_eq!(t.state.block_timestamp_last, 5);
    assert_eq!(
        t.state.price0_cumulative_last,
        Amount((BigUint::from(20u8) << 112usize) - BigUint::from(1u8))
    );
    assert_eq!(t.state.bank_unix_timestamp, source_time);
    assert_eq!(t.state.bank_block_hash, state.bank_block_hash);
    assert!(quote_transition(
        [&p.reserve0, &p.reserve1],
        state,
        &amount(100),
        true,
        source_time - 1
    )
    .is_err());
    assert!(quote_transition(
        [&p.reserve0, &p.reserve1],
        state,
        &amount(100),
        true,
        u64::MAX
    )
    .is_err());
}
