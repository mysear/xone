//! Model projection of actual ExactOut API commitments. Original execution is separate.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
};
use xone_domain::{hash, Pool, Snapshot};
use xone_execution_plan::exact_out::{verify, ExactOutPlan, ENGINE};
use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
use xone_quote_api::ExactOutRequest;
use xone_routing::{exact_out::evaluate_path, Funds};
type Error = Box<dyn std::error::Error>;
fn require(ok: bool, reason: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(std::io::Error::other(reason).into())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema_version: u32,
    records: Vec<Record>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    case_index: Value,
    snapshot: Snapshot,
    request: ExactOutRequest,
    quote: Value,
    execution_plan: ExactOutPlan,
}
fn process(raw: &[u8]) -> Result<Value, Error> {
    let original: Value = serde_json::from_slice(raw)?;
    let input: Input = serde_json::from_slice(raw)?;
    require(
        input.schema_version == 1 && !input.records.is_empty() && input.records.len() <= 256,
        "EXACT_OUT_PROJECT_SCOPE",
    )?;
    let mut records = Vec::new();
    for (index, r) in input.records.into_iter().enumerate() {
        let s = &r.snapshot;
        let req = &r.request;
        let p = &r.execution_plan;
        let q = &r.quote;
        require(
            serde_json::to_value(s)? == original["records"][index]["snapshot"]
                && serde_json::to_value(p)? == original["records"][index]["execution_plan"],
            "EXACT_OUT_PROJECT_UNPARSED_FIELDS",
        )?;
        s.validate("chain-parser-go", 1)?;
        req.validate()?;
        verify(p)?;
        require(
            s.pools.len() == 1
                && s.pools
                    .iter()
                    .all(|p| p.protocol == xone_domain::pancake_lm::PROFILE),
            "EXACT_OUT_PROJECT_POOL_SCOPE",
        )?;
        require(
            p.snapshot == s.reference
                && q["snapshotRef"] == serde_json::to_value(&s.reference)?
                && p.context.chain_key == req.chain_key
                && p.context.sell_token == req.sell_token
                && p.context.buy_token == req.buy_token
                && p.context.caller == req.caller
                && p.context.recipient == req.recipient
                && p.context.slippage_bps == req.slippage_bps
                && p.allocation.input_budget == req.max_sell_amount
                && p.allocation.target_output == req.buy_amount,
            "EXACT_OUT_PROJECT_CONTEXT",
        )?;
        let request_hash = hash(
            &json!({"request":req,"view_id":q["quoteViewId"],"policy":q["economicPolicyHash"],"engine":ENGINE}),
        )?;
        let expires = q["validity"]["expiresAtMs"]
            .as_u64()
            .ok_or("EXACT_OUT_PROJECT_EXPIRY")?;
        require(
            q["requestHash"] == request_hash
                && p.context.deadline_ms == expires
                && q["economicPolicyHash"] == p.economic_policy_hash
                && q["engineVersion"] == ENGINE
                && q["quoteId"]
                    == format!(
                        "xo-{}",
                        hash(
                            &json!({"request_hash":request_hash,"plan_hash":p.plan_hash,"expires":expires})
                        )?
                    ),
            "EXACT_OUT_PROJECT_COMMITMENT",
        )?;
        for (key, expected) in [
            ("routes", serde_json::to_value(&p.allocation.routes)?),
            ("planHash", json!(p.plan_hash)),
            ("kind", json!("EXACT_OUT")),
            ("minBuyAmount", json!(p.min_buy_amount)),
            ("maxSellAmount", json!(p.max_sell_amount)),
            ("requiredSellAmount", json!(p.allocation.required_input)),
            ("consumedIn", json!(p.allocation.required_input)),
            ("buyAmount", json!(p.allocation.target_output)),
            ("grossBuyAmount", json!(p.allocation.target_output)),
            ("inputBudget", json!(p.allocation.input_budget)),
            ("unusedInputBudget", json!(p.allocation.unused_input_budget)),
            ("refund", json!("0")),
        ] {
            require(q[key] == expected, "EXACT_OUT_PROJECT_PUBLIC_ALLOCATION")?;
        }
        let funds = Funds {
            sender: &req.caller,
            recipient: &req.recipient,
        };
        let proposals = p
            .allocation
            .routes
            .iter()
            .map(|r| ExactOutProposal {
                edges: r.edges.clone(),
                amount_out: r.amount_out.clone(),
            })
            .collect::<Vec<_>>();
        let recomputed = evaluate_allocation(
            &s.pools,
            &proposals,
            &req.buy_amount,
            &req.max_sell_amount,
            funds,
        )?;
        require(
            serde_json::to_value(recomputed)? == serde_json::to_value(&p.allocation)?,
            "EXACT_OUT_PROJECT_RECOMPUTED_ALLOCATION",
        )?;
        let mut state = s
            .pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<String, Pool>>();
        let mut steps = Vec::new();
        for route in &p.allocation.routes {
            require(
                req.constraints
                    .as_ref()
                    .is_none_or(|c| route.edges.len() <= c.max_hops),
                "EXACT_OUT_PROJECT_HOPS",
            )?;
            let before = state
                .values()
                .next()
                .ok_or("LEGACY_LM_EXACT_OUT_SOURCE_REQUIRED")?
                .clone();
            let computed = evaluate_path(&mut state, &route.edges, &route.amount_out, funds)?;
            require(
                serde_json::to_value(computed)? == serde_json::to_value(route)?,
                "EXACT_OUT_PROJECT_ROUTE",
            )?;
            steps.push(
                json!({"route":route,"pool_id":route.edges[0].pool_id,"zero_for_one":route.edges[0].zero_for_one,"amount_in":route.amount_in,"amount_out":route.amount_out,"before":before,"after":state.values().next().ok_or("LEGACY_LM_EXACT_OUT_FINAL_REQUIRED")?}),
            );
        }
        records.push(json!({"case_index":r.case_index,"snapshot":s,"request":req,"quote_id":q["quoteId"],
            "plan_hash":p.plan_hash,"required_input":p.allocation.required_input,"target_output":p.allocation.target_output,"gross_buy_amount":p.allocation.target_output,
            "max_sell_amount":p.max_sell_amount,"steps":steps,"final_pools":state.values().collect::<Vec<_>>()}));
    }
    Ok(
        json!({"schema_version":1,"passed":true,"input_sha256":format!("{:x}",Sha256::digest(raw)),
        "authority":"model projection of actual normal ExactOut API commitments; original execution is separate",
        "records":records,"all_requested_features_complete":false,"production_ready":false}),
    )
}
fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    require(args.len() == 2, "EXACT_OUT_PROJECT_INPUT_OUTPUT")?;
    let mut raw = Vec::new();
    std::fs::File::open(&args[0])?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)?;
    require(raw.len() <= 16 * 1024 * 1024, "EXACT_OUT_PROJECT_BYTES")?;
    let value = process(&raw)?;
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    out.write_all(&serde_json::to_vec_pretty(&value)?)?;
    out.write_all(b"\n")?;
    println!(
        "ExactOut model records projected: {}",
        value["records"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}
