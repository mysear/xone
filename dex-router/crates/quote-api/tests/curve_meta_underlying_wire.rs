use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use prost::Message;
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{
    hash, wire::OnlineSnapshot, wire_conversion::snapshot_from_proto, Amount, Record,
};
use xone_protocol_math::multi_asset::quote_exact_in_tokens;
use xone_quote_api::Api;

#[derive(Deserialize)]
struct Swap {
    sell: String,
    buy: String,
    amount_in: Amount,
    amount_out: Amount,
}

#[tokio::test]
async fn go_curve_meta_underlying_snapshots_quote_and_restore() {
    let snapshots = [
        include_bytes!("fixtures/curve_meta_underlying_0.pb").as_slice(),
        include_bytes!("fixtures/curve_meta_underlying_1.pb").as_slice(),
        include_bytes!("fixtures/curve_meta_underlying_2.pb").as_slice(),
        include_bytes!("fixtures/curve_meta_underlying_3.pb").as_slice(),
    ]
    .map(|bytes| snapshot_from_proto(OnlineSnapshot::decode(bytes).unwrap()).unwrap());
    let swaps: [Swap; 3] =
        serde_json::from_str(include_str!("fixtures/curve_meta_underlying_swaps.json")).unwrap();
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "curve-meta-test".into();
    config
        .config
        .chains
        .iter_mut()
        .find(|chain| chain.key == "evm:31337")
        .unwrap()
        .key = "evm:1".into();
    config.config.routing.max_state_age_ms = 315_360_000_000;
    config.config.routing.max_hops = 1;
    config.config.routing.max_splits = 1;
    let api = Api::new(config.clone()).unwrap();
    api.replica.install_baseline(snapshots[0].clone()).unwrap();
    for (index, swap) in swaps.iter().enumerate() {
        let previous = &snapshots[index];
        let next = &snapshots[index + 1];
        let pool = &previous.pools[0];
        let calculated =
            quote_exact_in_tokens(pool, &swap.amount_in, &swap.sell, &swap.buy).unwrap();
        assert_eq!(calculated.amount_out, swap.amount_out);
        let mut post = calculated.next_pool;
        let meta = post
            .multi_asset
            .as_mut()
            .unwrap()
            .curve_meta
            .as_mut()
            .unwrap();
        meta.block_hash.clone_from(&next.reference.block.hash);
        meta.block_number = next.reference.block.number_or_slot;
        meta.timestamp = next.reference.block.timestamp_ms / 1000;
        assert_eq!(post, next.pools[0]);

        for exact_out in [false, true] {
            let mut body = json!({
                "chainKey": "evm:1", "sellToken": swap.sell, "buyToken": swap.buy,
                "caller": "0x000000000000000000000000000000000000dead",
                "recipient": "0x000000000000000000000000000000000000beef",
                "slippageBps": 0, "verification": "FAST",
            });
            let path = if exact_out {
                body["kind"] = json!("EXACT_OUT");
                body["buyAmount"] = json!(swap.amount_out);
                body["maxSellAmount"] = json!(swap.amount_in);
                "/v1/quote-exact-out"
            } else {
                body["kind"] = json!("EXACT_IN");
                body["sellAmount"] = json!(swap.amount_in);
                "/v1/quote"
            };
            let request = Request::post(path)
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
            assert_eq!(quote["grossBuyAmount"], json!(swap.amount_out));
            assert_eq!(quote["snapshotRef"], json!(previous.reference));
            if exact_out {
                let required: Amount =
                    serde_json::from_value(quote["requiredSellAmount"].clone()).unwrap();
                assert!(required.0 <= swap.amount_in.0);
            }
        }
        api.replica
            .apply(Record {
                reference: next.reference.clone(),
                kind: "BLOCK_COMMIT".into(),
                record_id: hash(&next.reference).unwrap(),
                payload_hash: hash(&next.pools).unwrap(),
                pools: next.pools.clone(),
                normalized_facts: None,
            })
            .unwrap();
    }
    let recovered = Api::new(config).unwrap();
    let checkpoint = &snapshots[3];
    recovered
        .replica
        .restore_owner_checkpoint(checkpoint.clone())
        .unwrap();
    let view = recovered.replica.view("evm:1").unwrap();
    assert_eq!(view.reference, checkpoint.reference);
    assert_eq!(&*view.pools, &checkpoint.pools);
    assert!(view.graph.adjacency.contains_key(&swaps[0].sell));
}
