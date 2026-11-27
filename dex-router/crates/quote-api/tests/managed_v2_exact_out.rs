use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use prost::Message;
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{
    hash, scope_hash, wire::OnlineSnapshot, wire_conversion::snapshot_from_proto, BlockRef, Pool,
    Record, Snapshot, SnapshotRef, SCHEMA_VERSION,
};
use xone_execution_plan::exact_out::{verify, ExactOutPlan};
use xone_quote_api::Api;

fn api() -> Api {
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "managed-v2-exact-out-test".into();
    config.config.routing.max_state_age_ms = 315_360_000_000;
    config.config.routing.max_hops = 1;
    config.config.routing.max_splits = 1;
    Api::new(config).unwrap()
}

#[tokio::test]
async fn managed_v2_bpt_exact_out_quotes_include_the_pool_token() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../protocol-math/tests/fixtures/managed_v2_bpt_exact_out.json"
    ))
    .unwrap();
    for scenario in fixture["scenarios"].as_array().unwrap() {
        let actor = scenario["actor"].as_str().unwrap();
        let mut pool: Pool = serde_json::from_value(scenario["source"].clone()).unwrap();
        for (index, case) in scenario["cases"].as_array().unwrap().iter().enumerate() {
            let m = pool
                .multi_asset
                .as_mut()
                .unwrap()
                .managed_v2
                .as_mut()
                .unwrap();
            let clock = m.execution_context.take();
            if let Some(clock) = clock {
                m.block_number = clock.block_number;
                m.timestamp = clock.timestamp;
                m.block_hash = format!("0x{:064x}", m.block_number);
            }
            let reference = SnapshotRef {
                block: BlockRef {
                    chain_key: pool.chain_key.clone(),
                    number_or_slot: m.block_number,
                    hash: m.block_hash.clone(),
                    parent_hash: format!("0x{:064x}", m.block_number - 1),
                    timestamp_ms: m.timestamp * 1000,
                },
                schema_version: SCHEMA_VERSION,
                canonical_epoch: 1,
                journal_seq: index as u64 + 1,
                adapter_set_hash: hash(&[&pool.adapter_version]).unwrap(),
                scope_hash: scope_hash(std::slice::from_ref(&pool)).unwrap(),
                online_generation: 1,
                writer_epoch: 1,
                owner_service: "chain-parser-go".into(),
                owner_assignment_version: 1,
            };
            let api = api();
            api.replica
                .install_baseline(Snapshot {
                    reference,
                    pools: vec![pool.clone()],
                    source: "LOCAL_MANAGED_BPT_PROJECTION".into(),
                    history_gaps: vec![],
                })
                .unwrap();
            let assets = &pool.multi_asset.as_ref().unwrap().assets;
            let token = |key: &str| match case[key].as_i64().unwrap() {
                -1 => pool.pool_id.as_str(),
                index => assets[index as usize].token.as_str(),
            };
            let (sell, buy) = (token("index_in"), token("index_out"));
            let reverted = case["status"] == 0;
            let max_sell = if reverted {
                "10000000000000000000000"
            } else {
                case["raw_in"].as_str().unwrap()
            };
            let request = Request::post("/v1/quote-exact-out")
                .header("authorization", format!("Bearer {}", api.config.token))
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "chainKey": pool.chain_key, "kind": "EXACT_OUT",
                    "sellToken": sell, "buyToken": buy,
                    "buyAmount": case["target"], "maxSellAmount": max_sell,
                    "caller": actor, "recipient": actor, "slippageBps": 0, "verification": "FAST",
                }).to_string()))
                .unwrap();
            let response = api.handler().oneshot(request).await.unwrap();
            let status = response.status();
            let quote: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
                    .unwrap();
            if reverted {
                assert!(!status.is_success(), "{} {quote}", case["name"]);
                pool = serde_json::from_value(case["after"].clone()).unwrap();
                continue;
            }
            assert_eq!(status, StatusCode::OK, "{} {quote}", case["name"]);
            assert_eq!(quote["requiredSellAmount"], case["raw_in"]);
            assert_eq!(quote["grossBuyAmount"], case["target"]);
            let edge = &quote["routes"][0]["edges"][0];
            assert_eq!(edge["from_token"], sell);
            assert_eq!(edge["to"], buy);
            pool = serde_json::from_value(case["after"].clone()).unwrap();
        }
    }
}

#[tokio::test]
async fn managed_v2_go_snapshots_drive_continuous_exact_out_quotes() {
    let original: Value = serde_json::from_str(include_str!(
        "../../protocol-math/tests/fixtures/managed_v2_vault_exact_out.json"
    ))
    .unwrap();
    let snapshots = [
        include_bytes!("fixtures/managed_v2_vault_snapshot_0.pb").as_slice(),
        include_bytes!("fixtures/managed_v2_vault_snapshot_1.pb").as_slice(),
        include_bytes!("fixtures/managed_v2_vault_snapshot_2.pb").as_slice(),
    ]
    .map(|bytes| snapshot_from_proto(OnlineSnapshot::decode(bytes).unwrap()).unwrap());
    let expected_source: Pool = serde_json::from_value(original["source"].clone()).unwrap();
    assert_eq!(snapshots[0].pools, [expected_source]);

    let api = api();
    api.replica.install_baseline(snapshots[0].clone()).unwrap();

    for (index, case) in original["cases"].as_array().unwrap().iter().enumerate() {
        let previous = &snapshots[index];
        let pool = &previous.pools[0];
        let assets = &pool.multi_asset.as_ref().unwrap().assets;
        let sell = &assets[case["index_in"].as_u64().unwrap() as usize].token;
        let buy = &assets[case["index_out"].as_u64().unwrap() as usize].token;
        let target = case["output"].as_str().unwrap();
        let required = case["input"].as_str().unwrap();
        let body = json!({
            "chainKey": pool.chain_key, "kind": "EXACT_OUT",
            "sellToken": sell, "buyToken": buy,
            "buyAmount": target, "maxSellAmount": required,
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
            serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
                .unwrap();
        assert_eq!(status, StatusCode::OK, "{quote}");
        assert_eq!(quote["requiredSellAmount"], required);
        assert_eq!(quote["grossBuyAmount"], target);
        assert_eq!(quote["snapshotRef"], json!(previous.reference));

        let id = quote["quoteId"].as_str().unwrap();
        let request = Request::get(format!("/internal/quotes/{id}/evidence"))
            .header("authorization", format!("Bearer {}", api.config.token))
            .body(Body::empty())
            .unwrap();
        let response = api.handler().oneshot(request).await.unwrap();
        let status = response.status();
        let evidence: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
                .unwrap();
        assert_eq!(status, StatusCode::OK, "{evidence}");
        let plan: ExactOutPlan =
            serde_json::from_value(evidence["execution_plan"].clone()).unwrap();
        verify(&plan).unwrap();
        let edge = &plan.allocation.routes[0].edges[0];
        assert_eq!(edge.pool_id, pool.pool_id);
        assert_eq!(edge.from_token.as_deref(), Some(sell.as_str()));
        assert_eq!(&edge.to, buy);

        let observed = &snapshots[index + 1];
        let mut expected: Pool = serde_json::from_value(case["after"].clone()).unwrap();
        let m = expected
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        let clock = m.execution_context.take().unwrap();
        assert_eq!(clock.block_number, observed.reference.block.number_or_slot);
        assert_eq!(
            clock.timestamp * 1000,
            observed.reference.block.timestamp_ms
        );
        m.block_number = clock.block_number;
        m.timestamp = clock.timestamp;
        m.block_hash.clone_from(&observed.reference.block.hash);
        assert_eq!(observed.pools, [expected]);
        api.replica
            .apply(Record {
                reference: observed.reference.clone(),
                kind: "BLOCK_COMMIT".into(),
                record_id: hash(&observed.reference).unwrap(),
                payload_hash: hash(&observed.pools).unwrap(),
                pools: observed.pools.clone(),
                normalized_facts: None,
            })
            .unwrap();
    }
}
