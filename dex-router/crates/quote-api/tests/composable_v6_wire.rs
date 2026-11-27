use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use prost::Message;
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{
    hash, wire::OnlineSnapshot, wire_conversion::snapshot_from_proto, Amount, Record,
};
use xone_protocol_math::multi_asset::{quote_exact_in_tokens, quote_exact_out_tokens};
use xone_quote_api::Api;

#[tokio::test]
async fn composable_v6_go_snapshots_drive_bpt_quotes() {
    let reference: Value = serde_json::from_str(include_str!(
        "../../protocol-math/tests/fixtures/composable-v6-supply.json"
    ))
    .unwrap();
    let snapshots = [
        include_bytes!("fixtures/composable_v6_snapshot_0.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_1.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_2.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_3.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_4.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_5.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_6.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_7.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_8.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_9.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_10.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_11.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_12.pb").as_slice(),
        include_bytes!("fixtures/composable_v6_snapshot_13.pb").as_slice(),
    ]
    .map(|bytes| snapshot_from_proto(OnlineSnapshot::decode(bytes).unwrap()).unwrap());
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "composable-v6-test".into();
    config.config.routing.max_state_age_ms = 315_360_000_000;
    config.config.routing.max_hops = 1;
    config.config.routing.max_splits = 1;
    let cases = reference["bpt_swaps"].as_array().unwrap();
    for (snapshots, cases) in snapshots[..10].chunks_exact(5).zip(cases.chunks_exact(4)) {
        test_pool(Api::new(config.clone()).unwrap(), snapshots, cases).await;
    }
    let cases = reference["refresh_wire_swaps"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for (snapshots, cases) in snapshots[10..].chunks_exact(2).zip(cases.chunks_exact(1)) {
        test_pool(Api::new(config.clone()).unwrap(), snapshots, cases).await;
    }
}

async fn test_pool(api: Api, snapshots: &[xone_domain::Snapshot], cases: &[Value]) {
    api.replica.install_baseline(snapshots[0].clone()).unwrap();

    for (index, case) in cases.iter().enumerate() {
        let previous = &snapshots[index];
        let pool = &previous.pools[0];
        let assets = &pool.multi_asset.as_ref().unwrap().assets;
        let sell = &assets[case["i"].as_u64().unwrap() as usize].token;
        let buy = &assets[case["j"].as_u64().unwrap() as usize].token;
        let amount: Amount = case["amount"].as_str().unwrap().parse().unwrap();
        let given_out = case["kind"] == 1;
        let mut modeled = if given_out {
            quote_exact_out_tokens(pool, &amount, sell, buy)
        } else {
            quote_exact_in_tokens(pool, &amount, sell, buy)
        }
        .unwrap();
        let observed = &snapshots[index + 1];
        let rates = modeled
            .next_pool
            .multi_asset
            .as_mut()
            .unwrap()
            .composable_v6
            .as_mut()
            .unwrap();
        if rates.rate_snapshot_timestamp.is_some() {
            let quote_time = previous.reference.block.timestamp_ms / 1000;
            let mined_time = observed.reference.block.timestamp_ms / 1000;
            assert_eq!(rates.rate_snapshot_timestamp, Some(quote_time));
            rates.rate_snapshot_timestamp = Some(mined_time);
            let previous_rates = &pool
                .multi_asset
                .as_ref()
                .unwrap()
                .composable_v6
                .as_ref()
                .unwrap()
                .rate_caches;
            for (next, prev) in rates.rate_caches.iter_mut().zip(previous_rates) {
                if let Some(refreshed) = &prev.observed_refresh_rate {
                    assert_eq!(&next.current, refreshed);
                    assert_eq!(
                        u64::from(next.expires),
                        quote_time + u64::from(prev.duration)
                    );
                    next.expires = u32::try_from(mined_time + u64::from(prev.duration)).unwrap();
                }
            }
        }
        assert_eq!(modeled.next_pool, observed.pools[0]);
        assert_eq!(json!(modeled.consumed_in), case["amount_in"]);
        assert_eq!(json!(modeled.amount_out), case["amount_out"]);

        if given_out {
            let request = Request::post("/v1/quote-exact-out")
                .header("authorization", format!("Bearer {}", api.config.token))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "chainKey": pool.chain_key, "kind": "EXACT_OUT",
                        "sellToken": sell, "buyToken": buy,
                        "buyAmount": case["amount_out"], "maxSellAmount": case["amount_in"],
                        "caller": "0x000000000000000000000000000000000000dead",
                        "recipient": "0x000000000000000000000000000000000000beef",
                        "slippageBps": 0, "verification": "FAST",
                    })
                    .to_string(),
                ))
                .unwrap();
            let response = api.handler().oneshot(request).await.unwrap();
            let status = response.status();
            let quote: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
                    .unwrap();
            assert_eq!(status, StatusCode::OK, "{quote}");
            assert_eq!(quote["requiredSellAmount"], case["amount_in"]);
            assert_eq!(quote["grossBuyAmount"], case["amount_out"]);
            assert_eq!(quote["snapshotRef"], json!(previous.reference));
        }
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
