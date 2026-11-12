//! Offline model projection of real API plans; original-program authority is separate.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};
use xone_domain::{hash, Amount, Pool, Snapshot};
use xone_execution_plan::{verify_plan_hash, Plan};
use xone_quote_api::QuoteRequest;
use xone_routing::{evaluate_path_with_funds, Funds};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema_version: u32,
    records: Vec<Selected>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Selected {
    case_index: usize,
    snapshot: Snapshot,
    request: QuoteRequest,
    quote: Value,
    execution_plan: Plan,
}
type Error = Box<dyn std::error::Error>;
fn require(condition: bool, message: &str) -> Result<(), Error> {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message).into())
    }
}
fn process(raw: &[u8]) -> Result<Value, Error> {
    let value: Value = serde_json::from_slice(raw)?;
    let input: Input = serde_json::from_slice(raw)?;
    require(
        input.schema_version == 1 && !input.records.is_empty() && input.records.len() <= 256,
        "METEORA_PROJECT_INPUT_SCOPE",
    )?;
    let mut records = Vec::new();
    for (index, record) in input.records.into_iter().enumerate() {
        let snapshot = &record.snapshot;
        let request = &record.request;
        let q = &record.quote;
        let plan = &record.execution_plan;
        require(
            serde_json::to_value(snapshot)? == value["records"][index]["snapshot"],
            "METEORA_PROJECT_UNPARSED_SNAPSHOT_FIELDS",
        )?;
        require(
            serde_json::to_value(plan)? == value["records"][index]["execution_plan"],
            "METEORA_PROJECT_UNPARSED_PLAN_FIELDS",
        )?;
        snapshot.validate("chain-parser-rust", 1)?;
        request.validate()?;
        require(
            request
                .constraints
                .as_ref()
                .is_none_or(|c| (1..=4).contains(&c.max_hops)),
            "METEORA_PROJECT_HOP_CONSTRAINT",
        )?;
        verify_plan_hash(plan)?;
        require(
            snapshot.pools.len() == 1
                && snapshot
                    .pools
                    .iter()
                    .all(|p| p.protocol == xone_domain::meteora::PROFILE),
            "METEORA_PROJECT_POOL_SCOPE",
        )?;
        require(
            request.verification == "FAST" && plan.snapshot == snapshot.reference,
            "METEORA_PROJECT_REQUEST_SNAPSHOT",
        )?;
        let context = &plan.context;
        require(
            context.chain_key == request.chain_key
                && context.caller == request.caller
                && context.recipient == request.recipient
                && context.sell_token == request.sell_token
                && context.buy_token == request.buy_token
                && context.slippage_bps == request.slippage_bps,
            "METEORA_PROJECT_CONTEXT",
        )?;
        require(
            q["engineVersion"] == "xone-router/0.1"
                && q["chainKey"] == request.chain_key
                && q["snapshotRef"] == serde_json::to_value(&snapshot.reference)?,
            "METEORA_PROJECT_QUOTE_IDENTITY",
        )?;
        require(
            q["verification"]["level"] == "INDICATIVE"
                && q["verification"]["success"] == false
                && q.get("netBuyAmountEstimate") == Some(&Value::Null),
            "METEORA_PROJECT_VERIFICATION_BOUNDARY",
        )?;
        require(
            q["costs"]["unavailableReason"] == "CHAIN_FEE_MODEL_NOT_IMPLEMENTED",
            "METEORA_PROJECT_FEE_BOUNDARY",
        )?;
        require(
            q["planHash"] == plan.plan_hash && q["economicPolicyHash"] == plan.economic_policy_hash,
            "METEORA_PROJECT_PLAN_BINDING",
        )?;
        let request_hash = hash(
            &json!({"request":request,"view_id":q["quoteViewId"],"policy":q["economicPolicyHash"],"engine":"xone-router/0.1"}),
        )?;
        require(
            q["requestHash"] == request_hash,
            "METEORA_PROJECT_REQUEST_HASH",
        )?;
        let expires = q["validity"]["expiresAtMs"]
            .as_u64()
            .ok_or("METEORA_PROJECT_EXPIRY_REQUIRED")?;
        require(
            expires == context.deadline_ms
                && q["quoteId"]
                    == hash(
                        &json!({"request_hash":request_hash,"plan_hash":plan.plan_hash,"expires":expires}),
                    )?,
            "METEORA_PROJECT_QUOTE_HASH",
        )?;
        require(
            plan.allocation.sell_amount == request.sell_amount
                && q["sellAmount"] == serde_json::to_value(&request.sell_amount)?,
            "METEORA_PROJECT_INPUT_CONSERVATION",
        )?;
        require(
            q["routes"] == serde_json::to_value(&plan.allocation.routes)?
                && q["grossBuyAmount"] == serde_json::to_value(&plan.allocation.gross_buy_amount)?
                && q["minBuyAmount"] == serde_json::to_value(&plan.min_buy_amount)?
                && q["consumedIn"] == serde_json::to_value(&request.sell_amount)?
                && q["refund"] == "0",
            "METEORA_PROJECT_PUBLIC_ALLOCATION",
        )?;
        require(
            plan.allocation.routes.len() == 1,
            "METEORA_PROJECT_ROUTE_SCOPE",
        )?;
        let cost: Amount = q["costs"]["proposalActivationCostBuyToken"]
            .as_str()
            .ok_or("METEORA_PROJECT_COST_REQUIRED")?
            .parse()?;
        let funds = Funds {
            sender: &request.caller,
            recipient: &request.recipient,
        };
        let allocation = xone_optimizer::evaluate_allocation_with_funds(
            &snapshot.pools,
            &plan.allocation.routes,
            &cost,
            funds,
        )?;
        require(
            serde_json::to_value(&allocation)? == serde_json::to_value(&plan.allocation)?,
            "METEORA_PROJECT_RECOMPUTED_ALLOCATION",
        )?;
        let mut state: BTreeMap<String, Pool> = snapshot
            .pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect();
        let mut used = BTreeSet::new();
        let mut steps = Vec::new();
        for route in &plan.allocation.routes {
            require(
                route.path.edges.len() == 1
                    && route.path.tokens
                        == vec![request.sell_token.clone(), request.buy_token.clone()],
                "METEORA_PROJECT_ONE_HOP",
            )?;
            let edge = &route.path.edges[0];
            require(
                used.insert(edge.pool_id.clone()),
                "METEORA_PROJECT_DUPLICATE_PHYSICAL_POOL",
            )?;
            let before = state
                .get(&edge.pool_id)
                .ok_or("METEORA_PROJECT_MISSING_POOL")?
                .clone();
            require(
                edge.protocol == xone_domain::meteora::PROFILE && edge.from_token.is_none(),
                "METEORA_PROJECT_EDGE_SCOPE",
            )?;
            let out =
                evaluate_path_with_funds(&mut state, &route.path.edges, &route.amount_in, funds)?;
            require(
                out == route.amount_out
                    && route.path.amount_in == route.amount_in
                    && route.path.amount_out == out,
                "METEORA_PROJECT_STEP_AMOUNTS",
            )?;
            steps.push(json!({"pool_id":edge.pool_id,"zero_for_one":edge.zero_for_one,"amount_in":route.amount_in,"amount_out":out,"before":before,"after":state[&edge.pool_id]}));
        }
        records.push(json!({"case_index":record.case_index,"snapshot":snapshot,"request":request,"quote_id":q["quoteId"],"plan_hash":plan.plan_hash,"gross_buy_amount":plan.allocation.gross_buy_amount,"steps":steps}));
    }
    Ok(
        json!({"schema_version":1,"passed":true,"authority":"production model projection of captured normal API plans; not original-program execution","input_sha256":format!("{:x}",Sha256::digest(raw)),"records":records,"original_program_execution_claimed":false,"production_ready":false}),
    )
}
fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    require(args.len() == 2, "METEORA_PROJECT_INPUT_OUTPUT_REQUIRED")?;
    let mut raw = Vec::new();
    std::fs::File::open(&args[0])?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)?;
    require(raw.len() <= 16 * 1024 * 1024, "METEORA_PROJECT_INPUT_BYTES")?;
    let value = process(&raw)?;
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(&args[1]))?;
    out.write_all(&serde_json::to_vec_pretty(&value)?)?;
    out.write_all(b"\n")?;
    println!(
        "METEORA model records projected: {}",
        value["records"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}
