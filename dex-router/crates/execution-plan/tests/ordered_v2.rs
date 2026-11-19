use serde_json::Value;
use std::{path::Path, process::Command, sync::OnceLock};
use xone_domain::{hash, scope_hash, Amount, Pool, SnapshotRef};
use xone_execution_plan::{build, exact_out, ordered_v2::*, Context};
use xone_optimizer::{evaluate_allocation, exact_out as optimizer_out, AllocatedRoute};
use xone_routing::{Edge, Funds, PathQuote};

fn fixture(i: usize) -> (SplitPlan, Vec<Pool>, SplitExecutor, Value) {
    static BANK: OnceLock<Value> = OnceLock::new();
    let bank = BANK.get_or_init(|| {
        let bytes = if let Some(path) = std::env::var_os("XONE_ORDERED_V2_REPORT") {
            std::fs::read(path).unwrap()
        } else {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let run = Command::new("node")
                .current_dir(root)
                .arg("integration/tools/ci/ordered-v2-split-replay.cjs")
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            run.stdout
        };
        let mut report: Value = serde_json::from_slice(&bytes).unwrap();
        report["binding"].take()
    });
    let pools: Vec<Pool> = serde_json::from_value(bank["pools"].clone()).unwrap();
    let c = &bank["cases"][i];
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for leg in c["legs"].as_array().unwrap() {
        let tokens: Vec<String> = serde_json::from_value(leg["tokens"].clone()).unwrap();
        let n: Amount = serde_json::from_value(leg["amount"].clone()).unwrap();
        let edges = tokens
            .windows(2)
            .map(|t| {
                let pool = pools
                    .iter()
                    .find(|p| {
                        (p.token0 == t[0] && p.token1 == t[1])
                            || (p.token0 == t[1] && p.token1 == t[0])
                    })
                    .unwrap();
                Edge {
                    pool_id: pool.pool_id.clone(),
                    protocol: pool.protocol.clone(),
                    zero_for_one: pool.token0 == t[0],
                    from_token: None,
                    to: t[1].clone(),
                }
            })
            .collect::<Vec<_>>();
        outputs.push(optimizer_out::ExactOutProposal {
            edges: edges.clone(),
            amount_out: n.clone(),
        });
        inputs.push(AllocatedRoute {
            path: PathQuote {
                edges,
                tokens,
                amount_in: n.clone(),
                amount_out: Amount::zero(),
            },
            amount_in: n,
            amount_out: Amount::zero(),
        });
    }
    let exact_out = c["exactOut"].as_bool().unwrap();
    let ctx = Context {
        chain_key: pools[0].chain_key.clone(),
        caller: bank["caller"].as_str().unwrap().into(),
        recipient: bank["recipient"].as_str().unwrap().into(),
        sell_token: inputs[0].path.tokens[0].clone(),
        buy_token: inputs[0].path.tokens.last().unwrap().clone(),
        slippage_bps: if exact_out { 50 } else { 0 },
        deadline_ms: bank["deadlineMs"].as_u64().unwrap(),
    };
    let snapshot = SnapshotRef {
        block: serde_json::from_value(bank["block"].clone()).unwrap(),
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: 1,
        adapter_set_hash: hash(&vec![&pools[0].adapter_version]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let plan = if exact_out {
        let a = optimizer_out::evaluate_allocation(
            &pools,
            &outputs,
            &serde_json::from_value(c["output"].clone()).unwrap(),
            &serde_json::from_value(c["budget"].clone()).unwrap(),
            Funds {
                sender: &ctx.caller,
                recipient: &ctx.recipient,
            },
        )
        .unwrap();
        assert_eq!(a.required_input.to_string(), c["input"]);
        SplitPlan::ExactOut(exact_out::build(&ctx, &a, &snapshot, "split-policy").unwrap())
    } else {
        let a = evaluate_allocation(&pools, &inputs, &Amount::zero()).unwrap();
        assert_eq!(a.gross_buy_amount.to_string(), c["output"]);
        SplitPlan::ExactIn(build(&ctx, &a, &snapshot, "split-policy").unwrap())
    };
    (
        plan,
        pools,
        serde_json::from_value(bank["executor"].clone()).unwrap(),
        c.clone(),
    )
}

#[test]
fn split_plans_match_mined_calldata_balances_and_pool_order() {
    for i in 0..4 {
        let (p, pools, e, c) = fixture(i);
        let allowed = [e.target.clone(), e.router.target.clone()];
        let b = bind(&p, &pools, &e, &allowed).unwrap();
        assert_eq!(b.transaction.data, c["data"]);
        assert_eq!(
            serde_json::to_value(b.leg_pools.concat()).unwrap(),
            c["poolOrder"]
        );
        assert_eq!(b.transaction.to, e.target);
        assert_eq!(b.transaction.value, Amount::zero());
        let mut reversed = pools.clone();
        reversed.reverse();
        validate(&b, &reversed, &e, &allowed).unwrap();
    }
}

#[test]
fn split_rejects_recommitted_stale_leg_order_and_wrong_amounts() {
    for i in [0, 2] {
        for reorder in [false, true] {
            let (mut plan, pools, e, _) = fixture(i);
            match &mut plan {
                SplitPlan::ExactIn(p) => {
                    if reorder {
                        p.allocation.routes.reverse()
                    } else {
                        p.allocation.routes[1].path.amount_out = Amount::zero()
                    }
                    *p = build(
                        &p.context,
                        &p.allocation,
                        &p.snapshot,
                        &p.economic_policy_hash,
                    )
                    .unwrap();
                }
                SplitPlan::ExactOut(p) => {
                    if reorder {
                        p.allocation.routes.reverse()
                    } else {
                        p.allocation.routes[0].amounts[1] = "1".parse().unwrap();
                    }
                    *p = exact_out::build(
                        &p.context,
                        &p.allocation,
                        &p.snapshot,
                        &p.economic_policy_hash,
                    )
                    .unwrap();
                }
            }
            assert_eq!(
                bind(
                    &plan,
                    &pools,
                    &e,
                    &[e.target.clone(), e.router.target.clone()]
                )
                .unwrap_err()
                .0,
                if i == 2 && reorder {
                    "EXACT_OUT_MAX_INPUT"
                } else {
                    "EXECUTION_ALLOCATION_MISMATCH"
                }
            );
        }
    }
}

#[test]
fn split_rebuild_rejects_substitution_and_missing_source() {
    for i in [0, 2] {
        let (p, mut pools, e, _) = fixture(i);
        let allowed = [e.target.clone(), e.router.target.clone()];
        let b = bind(&p, &pools, &e, &allowed).unwrap();
        for j in 0..5 {
            let mut bad = b.clone();
            match j {
                0 => bad.transaction.data.push_str("00"),
                1 => bad.leg_pools.reverse(),
                2 => bad.pool_state_hash = "00".repeat(32),
                3 => bad.binding_hash = "11".repeat(32),
                _ => bad.executor.code_sha256 = "ab".repeat(32),
            };
            assert!(validate(&bad, &pools, &e, &allowed).is_err());
        }
        assert!(bind(&p, &pools, &e, &allowed[..1]).is_err());
        assert!(bind(&p, &pools, &e, &allowed[1..]).is_err());
        pools[3]
            .v2
            .as_mut()
            .unwrap()
            .swap
            .as_mut()
            .unwrap()
            .bank_block_number += 1;
        assert!(bind(&p, &pools, &e, &allowed).is_err());
    }
}
