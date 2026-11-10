//! Complete Pool differential; observed raw original wallet outputs are the oracle.
use serde_json::Value;
use std::{path::Path, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_protocol_math::managed_v2_pool::{quote_at, quote_bpt_exact_in_at, reclock, BptExecution};
fn bank() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../test/contracts/managed-v2-breaker-pools.json"),
            )
            .unwrap(),
        )
        .unwrap()
    })
}
fn pool(id: &Value, name: &Value) -> Pool {
    let b = bank()["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == *id)
        .unwrap();
    serde_json::from_value(
        b["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == *name)
            .unwrap()["pool"]
            .clone(),
    )
    .unwrap()
}
fn derive(mut expected: Pool, source: &Pool) -> Pool {
    let origin = source
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    let m = expected
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap();
    m.execution_context = Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
        block_number: m.block_number,
        timestamp: m.timestamp,
    });
    m.block_number = origin.block_number;
    m.timestamp = origin.timestamp;
    m.block_hash = origin.block_hash.clone();
    expected
}
#[test]
fn original_breaker_swaps_match_exact_amounts_complete_poststate_or_original_rejection() {
    let mut success = 0;
    let mut rejects = 0;
    let mut dust = 0;
    let mut recovered = false;
    for c in bank()["cases"].as_array().unwrap() {
        let source = pool(&c["before"], &c["pool_name"]);
        source.validate().unwrap();
        let expected = pool(&c["after"], &c["pool_name"]);
        expected.validate().unwrap();
        let m = expected
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        let token_in = c["token_in"].as_str().unwrap();
        let token_out = c["token_out"].as_str().unwrap();
        let result = if c["operation"] == "token_swap" {
            quote_at(
                &source,
                &input,
                token_in,
                token_out,
                m.block_number,
                m.timestamp,
            )
        } else {
            quote_bpt_exact_in_at(
                &source,
                &input,
                token_in,
                token_out,
                BptExecution {
                    sender: c["actor"].as_str().unwrap(),
                    recipient: c["actor"].as_str().unwrap(),
                    block_number: m.block_number,
                    timestamp: m.timestamp,
                },
            )
        };
        if c["expected_native"]["status"] == 0 {
            let error = result.unwrap_err();
            if c["expected_native"]["rejection_reason"] == "BAL#356" {
                assert_eq!(
                    error.0, "MANAGED_V2_CIRCUIT_BREAKER_TRIPPED",
                    "{} exact original reason",
                    c["id"]
                );
            }
            rejects += 1;
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{} {}: {e}", c["id"], c["label"]));
        assert_eq!(
            q.amount_out,
            c["expected_native"]["actual_amount_out"]
                .as_str()
                .unwrap()
                .parse::<Amount>()
                .unwrap(),
            "{} raw actual credit",
            c["id"]
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        assert_eq!(
            q.next_pool,
            derive(expected, &source),
            "{} complete after",
            c["id"]
        );
        success += 1;
        if q.amount_out == Amount::zero() {
            dust += 1;
        }
        if c["label"] == "subunit-upper-large-join-recovers" {
            recovered = true;
        }
    }
    assert!(
        success > 200 && rejects >= 12 && dust >= 4 && recovered,
        "{success}/{rejects}/{dust}/{recovered}"
    );
}
#[test]
fn every_original_clock_recomputes_dynamic_bounds_without_changing_raw_or_source() {
    let mut clocks = 0;
    let raw: Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/contracts/managed-v2-breaker.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for b in bank()["banks"].as_array().unwrap() {
        let ns = b["namespace"].as_str().unwrap();
        let is_clock = raw["checkpoints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["bank"] == b["id"]);
        if !is_clock {
            continue;
        }
        let rows = bank()["banks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|q| q["namespace"] == ns)
            .collect::<Vec<_>>();
        let index = rows.iter().position(|q| q["id"] == b["id"]).unwrap();
        let before = rows[index - 1];
        for row in b["pools"].as_array().unwrap() {
            let source = pool(&before["id"], &row["name"]);
            let expected: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
            let m = expected
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap();
            let q = reclock(&source, m.block_number, m.timestamp).unwrap();
            assert_eq!(q, derive(expected, &source));
            clocks += 1;
        }
    }
    assert_eq!(clocks, 26);
}

#[test]
fn future_execution_aum_can_trip_untouched_token_even_when_bpt_output_is_zero() {
    let source = pool(
        &Value::String("boundaries/B86".into()),
        &Value::String("B".into()),
    );
    let origin = source.clone();
    let s = source.multi_asset.as_ref().unwrap();
    let actor = s
        .managed_v2
        .as_ref()
        .unwrap()
        .bpt
        .as_ref()
        .unwrap()
        .allowlist_members[0]
        .address
        .as_str();
    let input: Amount = "1".parse().unwrap();
    let result = quote_bpt_exact_in_at(
        &source,
        &input,
        &s.assets[0].token,
        &source.pool_id,
        BptExecution {
            sender: actor,
            recipient: actor,
            block_number: 138,
            timestamp: 1767244804,
        },
    );
    assert_eq!(
        result.unwrap_err().0,
        "MANAGED_V2_CIRCUIT_BREAKER_TRIPPED",
        "current execution supply must check every untouched token bound"
    );
    assert_eq!(source, origin);
}
#[test]
fn explicit_reclock_and_future_context_bpt_quote_use_identical_views_in_both_directions() {
    let cases = bank()["cases"].as_array().unwrap();
    let mut count = 0;
    for c in cases
        .iter()
        .filter(|c| {
            c["namespace"] == "dynamic"
                && c["expected_native"]["status"] == 1
                && c["operation"] != "token_swap"
        })
        .take(12)
    {
        let source = pool(&c["before"], &c["pool_name"]);
        let m = source
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let block = m.block_number + 1;
        let timestamp = m.timestamp + 123;
        let current = reclock(&source, block, timestamp).unwrap();
        let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        let execution = BptExecution {
            sender: c["actor"].as_str().unwrap(),
            recipient: c["actor"].as_str().unwrap(),
            block_number: block,
            timestamp,
        };
        let a = quote_bpt_exact_in_at(
            &source,
            &input,
            c["token_in"].as_str().unwrap(),
            c["token_out"].as_str().unwrap(),
            execution,
        );
        let b = quote_bpt_exact_in_at(
            &current,
            &input,
            c["token_in"].as_str().unwrap(),
            c["token_out"].as_str().unwrap(),
            execution,
        );
        match (a, b) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.amount_out, b.amount_out);
                assert_eq!(a.next_pool, b.next_pool);
            }
            (Err(a), Err(b)) => assert_eq!(a.0, b.0),
            (a, b) => panic!("{} stale-vs-current result: {a:?}/{b:?}", c["id"]),
        };
        count += 1;
    }
    assert_eq!(count, 12);
}
