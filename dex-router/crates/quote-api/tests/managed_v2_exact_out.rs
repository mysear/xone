use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, BlockRef, Pool, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_execution_plan::exact_out::{verify, ExactOutPlan};
use xone_quote_api::Api;

#[tokio::test]
async fn managed_v2_output_quote_commits_the_explicit_token_edge() {
    let pool: Pool = serde_json::from_str(include_str!(
        "../../protocol-math/tests/fixtures/managed_v2_aum.json"
    ))
    .unwrap();
    let m = pool
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    let reference = SnapshotRef {
        block: BlockRef {
            chain_key: pool.chain_key.clone(),
            number_or_slot: m.block_number,
            hash: m.block_hash.clone(),
            parent_hash: format!("0x{}", "b".repeat(64)),
            timestamp_ms: m.timestamp * 1000,
        },
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: SCHEMA_VERSION,
        adapter_set_hash: hash(&[&pool.adapter_version]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(std::slice::from_ref(&pool)).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "managed-v2-exact-out-test".into();
    config.config.routing.max_state_age_ms = 315_360_000_000;
    config.config.routing.max_hops = 1;
    config.config.routing.max_splits = 1;
    let api = Api::new(config).unwrap();
    api.replica
        .install_baseline(Snapshot {
            reference,
            pools: vec![pool.clone()],
            source: "CONTROLLED_MANAGED_V2_BASELINE".into(),
            history_gaps: vec![],
        })
        .unwrap();
    let target = "772779500379197940000";
    let body = json!({
        "chainKey": pool.chain_key, "kind": "EXACT_OUT",
        "sellToken": pool.token0, "buyToken": pool.token1,
        "buyAmount": target, "maxSellAmount": "101000000000000000000",
        "caller": "0x000000000000000000000000000000000000dead",
        "recipient": "0x000000000000000000000000000000000000beef",
        "slippageBps": 0, "verification": "FAST",
    });
    let request = Request::post("/v1/quote-exact-out")
        .header("authorization", format!("Bearer {}", api.config.token))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = api.handler().oneshot(request).await.unwrap();
    let status = response.status();
    let quote: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{quote}");
    assert_eq!(quote["requiredSellAmount"], "100000000000102010102");
    assert_eq!(quote["grossBuyAmount"], target);

    let id = quote["quoteId"].as_str().unwrap();
    let request = Request::get(format!("/internal/quotes/{id}/evidence"))
        .header("authorization", format!("Bearer {}", api.config.token))
        .body(Body::empty())
        .unwrap();
    let response = api.handler().oneshot(request).await.unwrap();
    let status = response.status();
    let evidence: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{evidence}");
    let plan: ExactOutPlan = serde_json::from_value(evidence["execution_plan"].clone()).unwrap();
    verify(&plan).unwrap();
    let edge = &plan.allocation.routes[0].edges[0];
    assert_eq!(edge.pool_id, pool.pool_id);
    assert_eq!(edge.from_token.as_deref(), Some(pool.token0.as_str()));
    assert_eq!(edge.to, pool.token1);
}
