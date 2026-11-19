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
    fixture_from_bank(bank, i)
}

fn fixture_from_bank(bank: &Value, i: usize) -> (SplitPlan, Vec<Pool>, SplitExecutor, Value) {
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

#[test]
#[ignore = "requires original split bank and built stdio worker"]
fn split_worker_replays_four_mined_plans_and_rejects_inconsistent_sources() {
    use serde_json::json;
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_ORDERED_V2_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let bank = &report["executionBank"];
    let worker = std::env::var("XONE_SPLIT_WORKER").unwrap();
    let invoke = |request: &Value| invoke(&worker, request);
    for i in 0..4 {
        let (p, pools, e, c) = fixture(i);
        let allowed = [e.target.clone(), e.router.target.clone()];
        let b = bind(&p, &pools, &e, &allowed).unwrap();
        let r = &bank["runs"][2 * i];
        let request = json!({"protocol":"xone-local-evm-worker/1","split":{"bound":b,"expected_executor":e},"pools":pools,"allowed_targets":allowed,
            "fixture":{"schema_version":1,"state_anchor":bank["state_anchor"],"execution_environment":r["execution_environment"],"accounts":bank["accounts"],"block_hashes":[]},
            "expected_genesis_hash":bank["state_anchor"]["genesis_hash"],"transaction_context":r["transaction_context"],"timeout_ms":15000,
            "budget":{"max_fixture_json_bytes":16000000,"max_accounts":256,"max_storage_slots":65536,"max_code_bytes":8000000,"max_image_bytes":16000000,"max_reads":32000,"max_read_bytes":32000000,"max_instructions":4000000,"max_input_bytes":131072,"max_total_input_bytes":1000000,"max_transaction_gas":15000000,"probe_gas_limit":200000,"max_total_gas":15000000,"max_memory_bytes":64000000,"max_output_bytes":1000000}});
        let response = invoke(&request);
        assert!(response["error"].is_null(), "{}", response["error"]);
        let outcome = &response["outcome"];
        assert_eq!(outcome["matches_plan"], true);
        assert_eq!(outcome["verified"], false);
        assert_eq!(outcome["actual_input_consumed"], c["input"]);
        assert_eq!(outcome["actual_buy_amount"], c["output"]);
        assert_eq!(outcome["execution"]["gas_used"].to_string(), r["gasUsed"]);
        assert_eq!(
            outcome["native_fees"]["effective_gas_price"],
            r["effectiveGasPrice"]
        );
        assert_eq!(
            outcome["native_fees"]["execution_fee"],
            report["cases"][i]["nativeExecutionFee"]
        );
        if i == 0 {
            for phase in ["code", "router()", "split_caller_allowance_before"] {
                let mut bad = request.clone();
                let accounts = bad["fixture"]["accounts"].as_array_mut().unwrap();
                if phase == "split_caller_allowance_before" {
                    let sell = c["legs"][0]["tokens"][0].as_str().unwrap();
                    // Only the caller->batch allowance has MaxUint256 among the two captured allowance slots.
                    accounts.iter_mut().find(|a| a["address"] == sell).unwrap()["storage"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|s| s["value"] != format!("0x{}", "ff".repeat(32)));
                } else {
                    let account = accounts
                        .iter_mut()
                        .find(|a| a["address"] == e.target)
                        .unwrap();
                    let code = account["code"]
                        .as_str()
                        .unwrap()
                        .replace(&e.router.target[2..], &e.router.factory[2..]);
                    assert_ne!(account["code"], code);
                    account["code"] = json!(code);
                    if phase == "router()" {
                        use sha2::{Digest, Sha256};
                        let mut changed = e.clone();
                        changed.code_sha256 =
                            hex::encode(Sha256::digest(hex::decode(&code[2..]).unwrap()));
                        bad["split"] = json!({"bound":bind(&p,&pools,&changed,&allowed).unwrap(),"expected_executor":changed});
                    }
                }
                let rejected = invoke(&bad);
                assert!(rejected["outcome"].is_null());
                if phase == "code" {
                    assert_eq!(rejected["error"]["code"], "SIMULATION_CODE_MISMATCH");
                } else {
                    assert_eq!(rejected["error"]["phase"], phase, "{}", rejected["error"]);
                }
            }
        }
    }
}

#[test]
#[ignore = "requires independently mined caller approvals"]
fn exact_approval_calldata_matches_both_mined_sequences() {
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_ORDERED_V2_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let sequences = report["executionBank"]["approvalReplay"]["sequences"]
        .as_array()
        .unwrap();
    assert_eq!(sequences.len(), 2);
    for sequence in sequences {
        let i = sequence["caseIndex"].as_u64().unwrap() as usize;
        let (plan, pools, executor, case) = fixture(i);
        let allowed = [executor.target.clone(), executor.router.target.clone()];
        let mut bound = bind(&plan, &pools, &executor, &allowed).unwrap();
        let receipt = &sequence["runs"][0];
        let approval = caller_approval(&bound, &pools, &executor, &allowed, &Amount::zero())
            .unwrap()
            .unwrap();
        assert_eq!(approval.data, receipt["data"]);
        assert_eq!(approval.to, receipt["to"]);
        assert_eq!(approval.from, receipt["from"]);
        assert_eq!(approval.value.to_string(), receipt["value"]);
        assert!(caller_approval(
            &bound,
            &pools,
            &executor,
            &allowed,
            &serde_json::from_value(case["input"].clone()).unwrap()
        )
        .unwrap()
        .is_none());
        assert!(
            caller_approval(&bound, &pools, &executor, &allowed, &"1".parse().unwrap()).is_err()
        );
        bound.transaction.to = executor.router.target.clone();
        assert!(caller_approval(&bound, &pools, &executor, &allowed, &Amount::zero()).is_err());
    }
}

#[test]
#[ignore = "requires mined reset-then-exact sequences"]
fn reset_approval_policy_matches_both_mined_sequences() {
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_ORDERED_V2_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let bank = &report["executionBank"]["resetApprovalReplay"];
    let mut source = report["binding"].clone();
    set_bank_anchor(&mut source, &bank["state_anchor"]);
    for seq in bank["sequences"].as_array().unwrap() {
        let (plan, pools, ex, case) =
            fixture_from_bank(&source, seq["caseIndex"].as_u64().unwrap() as usize);
        let allowed = [ex.target.clone(), ex.router.target.clone()];
        let mut bound = bind(&plan, &pools, &ex, &allowed).unwrap();
        let one: Amount = "1".parse().unwrap();
        let required: Amount = serde_json::from_value(case["input"].clone()).unwrap();
        let ops = caller_approvals(
            &bound,
            &pools,
            &ex,
            &allowed,
            &one,
            ApprovalPolicy::ResetThenExact,
        )
        .unwrap();
        assert_eq!(ops.len(), 2);
        for (operation, mined) in ops.iter().zip(seq["runs"].as_array().unwrap()) {
            assert_eq!(operation.from, mined["from"]);
            assert_eq!(operation.to, mined["to"]);
            assert_eq!(operation.data, mined["data"]);
            assert_eq!(operation.value.to_string(), mined["value"]);
        }
        assert!(caller_approvals(
            &bound,
            &pools,
            &ex,
            &allowed,
            &required,
            ApprovalPolicy::ResetThenExact
        )
        .unwrap()
        .is_empty());
        let from_zero = caller_approvals(
            &bound,
            &pools,
            &ex,
            &allowed,
            &Amount::zero(),
            ApprovalPolicy::ResetThenExact,
        )
        .unwrap();
        assert_eq!(hash(&from_zero).unwrap(), hash(&ops[1..].to_vec()).unwrap());
        assert!(caller_approval(&bound, &pools, &ex, &allowed, &one).is_err());
        bound.transaction.to = ex.router.target.clone();
        assert!(caller_approvals(
            &bound,
            &pools,
            &ex,
            &allowed,
            &one,
            ApprovalPolicy::ResetThenExact
        )
        .is_err());
    }
}

fn invoke(worker: &str, request: &Value) -> Value {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(worker)
        .arg("--evm-simulation-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice::<Value>(&out.stdout).unwrap()
}

#[test]
#[ignore = "requires mined approval sequences and a built stdio worker"]
fn worker_audits_sequential_approval_and_swap_fees() {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_ORDERED_V2_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let bank = &report["executionBank"]["approvalReplay"];
    let anchor = &bank["state_anchor"];
    let mut source = report["binding"].clone();
    set_bank_anchor(&mut source, anchor);
    let worker = std::env::var("XONE_SPLIT_WORKER").unwrap();
    for seq in bank["sequences"].as_array().unwrap() {
        let i = seq["caseIndex"].as_u64().unwrap() as usize;
        let (plan, pools, ex, c) = fixture_from_bank(&source, i);
        let allowed = [ex.target.clone(), ex.router.target.clone()];
        let b = bind(&plan, &pools, &ex, &allowed).unwrap();
        let a = &seq["runs"][0];
        let swap = &seq["runs"][1];
        let code = bank["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["address"] == a["to"])
            .unwrap()["code"]
            .as_str()
            .unwrap();
        let request = json!({"protocol":"xone-local-evm-worker/1","split":{"bound":b,"expected_executor":ex,"approval_sequence":{"token_code_sha256":hex::encode(Sha256::digest(hex::decode(&code[2..]).unwrap())),"approval_context":a["transaction_context"],"swap_environment":swap["execution_environment"]}},"pools":pools,"allowed_targets":allowed,"expected_genesis_hash":anchor["genesis_hash"],"fixture":{"schema_version":1,"state_anchor":anchor,"accounts":bank["accounts"],"block_hashes":[],"execution_environment":a["execution_environment"]},"transaction_context":swap["transaction_context"],"timeout_ms":15000,"budget":{"max_fixture_json_bytes":16000000,"max_accounts":256,"max_storage_slots":65536,"max_code_bytes":8000000,"max_image_bytes":16000000,"max_reads":32000,"max_read_bytes":32000000,"max_instructions":4000000,"max_input_bytes":131072,"max_total_input_bytes":1000000,"max_transaction_gas":15000000,"probe_gas_limit":200000,"max_total_gas":15000000,"max_memory_bytes":64000000,"max_output_bytes":1000000}});
        let reply = invoke(&worker, &request);
        let out = &reply["outcome"];
        assert_eq!(out["matches_plan"], true, "{}", reply["error"]);
        assert_eq!(out["actual_input_consumed"], c["input"]);
        assert_eq!(out["actual_buy_amount"], c["output"]);
        let fee = |r: &Value| {
            r["gasUsed"].as_str().unwrap().parse::<u128>().unwrap()
                * r["effectiveGasPrice"]
                    .as_str()
                    .unwrap()
                    .parse::<u128>()
                    .unwrap()
        };
        assert_eq!(out["approval"]["execution_fee"], fee(a).to_string());
        assert_eq!(
            out["approval"]["payer_after"],
            out["native_fees"]["caller_before"]
        );
        assert_eq!(out["native_fees"]["execution_fee"], fee(swap).to_string());
        assert_eq!(out["approval"]["transaction"]["data"], a["data"]);
        if i == 0 {
            for (pointer, value) in [
                (
                    "/split/approval_sequence/swap_environment/parent_hash",
                    json!("ab".repeat(32)),
                ),
                (
                    "/split/approval_sequence/token_code_sha256",
                    json!("ab".repeat(32)),
                ),
                (
                    "/transaction_context/nonce",
                    a["transaction_context"]["nonce"].clone(),
                ),
            ] {
                let mut changed = request.clone();
                *changed.pointer_mut(pointer).unwrap() = value;
                assert!(!invoke(&worker, &changed)["error"].is_null(), "{pointer}");
            }
        }
    }
}

fn set_bank_anchor(source: &mut Value, anchor: &Value) {
    use serde_json::json;
    source["block"] = json!({"chain_key":anchor["chain_key"],"number_or_slot":anchor["block_number"],"hash":anchor["block_hash"],"parent_hash":anchor["parent_hash"],"timestamp_ms":anchor["timestamp"].as_u64().unwrap()*1000});
    // Only the caller's allowance changed between the two harvested snapshots.
    for pool in source["pools"].as_array_mut().unwrap() {
        let state = &mut pool["v2"]["swap"];
        state["bank_block_number"] = anchor["block_number"].clone();
        state["bank_block_hash"] = anchor["block_hash"].clone();
        state["bank_unix_timestamp"] = anchor["timestamp"].clone();
    }
}
