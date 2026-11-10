//! Whole Pool comparison against qualified original BPT execution observations.
use serde_json::Value;
use std::{path::Path, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_protocol_math::managed_v2_pool::{quote_bpt_exact_in_at, BptExecution};
fn data() -> &'static Value {
    static BANK: OnceLock<Value> = OnceLock::new();
    BANK.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../test/contracts/managed-v2-bpt-pools.json"),
            )
            .unwrap(),
        )
        .unwrap()
    })
}
#[test]
fn original_bpt_swap_amounts_and_every_retained_poststate_field_match() {
    let v = data();
    let mut successes = 0;
    let mut rejects = 0;
    let mut joins = 0;
    let mut exits = 0;
    for c in v["cases"].as_array().unwrap() {
        let bank = |key: &str| {
            v["banks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["id"] == c[key])
                .unwrap()
        };
        let entry = |key: &str| {
            bank(key)["pools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == c["pool_name"])
        };
        let (Some(before), Some(after)) = (entry("before"), entry("after")) else {
            continue;
        };
        let p: Pool = serde_json::from_value(before["pool"].clone()).unwrap();
        let mut expected: Pool = serde_json::from_value(after["pool"].clone()).unwrap();
        p.validate().unwrap();
        expected.validate().unwrap();
        let m = expected
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let result = if c["operation"] == "token_swap" {
            xone_protocol_math::managed_v2_pool::quote_at(
                &p,
                &c["amount_in"].as_str().unwrap().parse().unwrap(),
                c["token_in"].as_str().unwrap(),
                c["token_out"].as_str().unwrap(),
                m.block_number,
                m.timestamp,
            )
        } else {
            quote_bpt_exact_in_at(
                &p,
                &c["amount_in"].as_str().unwrap().parse::<Amount>().unwrap(),
                c["token_in"].as_str().unwrap(),
                c["token_out"].as_str().unwrap(),
                BptExecution {
                    sender: c["actor"].as_str().unwrap(),
                    recipient: c["actor"].as_str().unwrap(),
                    block_number: m.block_number,
                    timestamp: m.timestamp,
                },
            )
        };
        if c["expected_native"]["status"] == 0 {
            assert!(result.is_err(), "{} original revert accepted", c["id"]);
            rejects += 1;
            continue;
        }
        let swap = result.unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            swap.amount_out,
            c["expected_native"]["actual_amount_out"]
                .as_str()
                .unwrap()
                .parse::<Amount>()
                .unwrap(),
            "{} actual wallet credit",
            c["id"]
        );
        let source = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
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
        m.block_number = source.block_number;
        m.timestamp = source.timestamp;
        m.block_hash = source.block_hash.clone();
        assert_eq!(
            swap.next_pool, expected,
            "{} entire original Pool poststate",
            c["id"]
        );
        assert_eq!(
            swap.consumed_in,
            c["amount_in"].as_str().unwrap().parse::<Amount>().unwrap()
        );
        assert_eq!(swap.remaining_in, Amount::zero());
        if c["operation"] == "bpt_join" {
            joins += 1
        } else if c["operation"] == "bpt_exit" {
            exits += 1
        };
        successes += 1;
    }
    assert!(
        successes == 100 && joins == 47 && exits == 51 && rejects == 14,
        "{successes} successes {joins} joins {exits} exits {rejects} rejects"
    );
}

fn first_join() -> (Pool, Value) {
    let v = data();
    let c = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "settled/C0")
        .unwrap()
        .clone();
    let b = v["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == c["before"])
        .unwrap();
    let p = b["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == c["pool_name"])
        .unwrap();
    (serde_json::from_value(p["pool"].clone()).unwrap(), c)
}
#[test]
fn recipient_wallet_aliases_move_preminted_bpt_once_without_extra_minting() {
    use xone_protocol_math::managed_v2_pool::collect_at;
    let (p, c) = first_join();
    let original = p.clone();
    let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    let vault = p.multi_asset.as_ref().unwrap().vault.as_str();
    let collected = collect_at(&p, m.block_number, m.timestamp)
        .unwrap()
        .next_pool;
    let cm = collected
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    for recipient in [m.manager.as_str(), m.protocol_collector.as_str(), vault] {
        let s = quote_bpt_exact_in_at(
            &p,
            &c["amount_in"].as_str().unwrap().parse().unwrap(),
            c["token_in"].as_str().unwrap(),
            c["token_out"].as_str().unwrap(),
            BptExecution {
                sender: c["actor"].as_str().unwrap(),
                recipient,
                block_number: m.block_number,
                timestamp: m.timestamp,
            },
        )
        .unwrap();
        let n = s
            .next_pool
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(
            n.total_supply, cm.total_supply,
            "swap transfers preminted BPT"
        );
        assert_eq!(n.virtual_supply.0, &cm.virtual_supply.0 + &s.amount_out.0);
        assert_eq!(
            n.manager_bpt_balance.0,
            &cm.manager_bpt_balance.0
                + if recipient == m.manager {
                    s.amount_out.0.clone()
                } else {
                    0u8.into()
                }
        );
        assert_eq!(
            n.protocol_collector_bpt_balance.0,
            &cm.protocol_collector_bpt_balance.0
                + if recipient == m.protocol_collector {
                    s.amount_out.0.clone()
                } else {
                    0u8.into()
                }
        );
        assert_eq!(
            n.vault_bpt_actual_balance.0,
            &cm.vault_bpt_actual_balance.0
                - if recipient == vault {
                    0u8.into()
                } else {
                    s.amount_out.0.clone()
                }
        );
    }
    assert_eq!(p, original, "all derived transitions retain their input");
}
#[test]
fn failures_preserve_source_and_join_permission_is_bound_to_sender() {
    let (p, c) = first_join();
    let original = p.clone();
    let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    let actor = c["actor"].as_str().unwrap();
    let unknown = "0x0000000000000000000000000000000000000001";
    let run = |p: &Pool, sender: &str, recipient: &str, block, time| {
        quote_bpt_exact_in_at(
            p,
            &c["amount_in"].as_str().unwrap().parse().unwrap(),
            c["token_in"].as_str().unwrap(),
            c["token_out"].as_str().unwrap(),
            BptExecution {
                sender,
                recipient,
                block_number: block,
                timestamp: time,
            },
        )
    };
    assert!(run(&p, actor, actor, m.block_number - 1, m.timestamp).is_err());
    assert!(run(&p, actor, actor, m.block_number, m.timestamp - 1).is_err());
    assert!(run(
        &p,
        actor,
        "0x0000000000000000000000000000000000000000",
        m.block_number,
        m.timestamp
    )
    .is_err());
    let mut allowlisted = p.clone();
    let s = allowlisted
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap();
    s.allowlist_enabled = true;
    s.schedule.as_mut().unwrap().pool_state_word.0 |= num_bigint::BigUint::from(1u8) << 253usize;
    let member = s
        .bpt
        .as_mut()
        .unwrap()
        .allowlist_members
        .iter_mut()
        .find(|member| member.address == actor)
        .unwrap();
    member.allowed = true;
    allowlisted.validate().unwrap();
    assert!(
        run(&allowlisted, unknown, actor, m.block_number, m.timestamp).is_err(),
        "allowlisted recipient cannot authorize unknown sender"
    );
    assert!(
        run(&allowlisted, actor, unknown, m.block_number, m.timestamp).is_ok(),
        "known sender may choose another recipient"
    );
    assert_eq!(p, original);
    let mut short = p.clone();
    short
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .vault_bpt_cash = "0".parse().unwrap();
    assert!(run(&short, actor, actor, m.block_number, m.timestamp).is_err());
    assert_eq!(p, original);
}

#[test]
fn normal_token_dispatch_uses_bpt_profiles_exact_regular_kernel_but_rejects_bpt_without_funds_context(
) {
    let (p, c) = first_join();
    let s = p.multi_asset.as_ref().unwrap();
    let amount: Amount = "1000000000000000000".parse().unwrap();
    let expected = xone_protocol_math::managed_v2_pool::quote_exact_in_tokens(
        &p,
        &amount,
        &s.assets[0].token,
        &s.assets[1].token,
    )
    .unwrap();
    let actual = xone_protocol_math::multi_asset::quote_exact_in_tokens(
        &p,
        &amount,
        &s.assets[0].token,
        &s.assets[1].token,
    )
    .unwrap();
    assert_eq!(actual.amount_out, expected.amount_out);
    assert_eq!(actual.next_pool, expected.next_pool);
    assert!(xone_protocol_math::multi_asset::quote_exact_in_tokens(
        &p,
        &amount,
        c["token_in"].as_str().unwrap(),
        c["token_out"].as_str().unwrap()
    )
    .is_err());
}
