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
    let api = Api::new(config).unwrap();
    api.replica.install_baseline(snapshots[0].clone()).unwrap();

    for (index, case) in reference["bpt_swaps"].as_array().unwrap()[..4]
        .iter()
        .enumerate()
    {
        let previous = &snapshots[index];
        let pool = &previous.pools[0];
        let assets = &pool.multi_asset.as_ref().unwrap().assets;
        let sell = &assets[case["i"].as_u64().unwrap() as usize].token;
        let buy = &assets[case["j"].as_u64().unwrap() as usize].token;
        let amount: Amount = case["amount"].as_str().unwrap().parse().unwrap();
        let given_out = case["kind"] == 1;
        let modeled = if given_out {
            quote_exact_out_tokens(pool, &amount, sell, buy)
        } else {
            quote_exact_in_tokens(pool, &amount, sell, buy)
        }
        .unwrap();
        assert_eq!(modeled.next_pool, snapshots[index + 1].pools[0]);
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
        let observed = &snapshots[index + 1];
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
