use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, BlockRef, Pool, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_quote_api::Api;

#[tokio::test]
#[ignore = "requires the pinned native Raydium generate-orders corpus"]
async fn http_quotes_match_both_directions_amount_modes_and_fee_tokens() {
    let path = std::env::var("XONE_RAYCLMM_ORDER_CORPUS").unwrap();
    let corpus: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut tested = 0;
    for case in corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["order_only"] == true && case["amount"] == "2100")
    {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let state = p.raydium_clmm.as_ref().unwrap();
        let mut config = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        config.token = "rayclmm-orders-test".into();
        config.config.routing.max_splits = 1;
        let chain = &config.config.chains[1];
        assert_eq!(chain.key, p.chain_key);
        let snapshot = Snapshot {
            reference: SnapshotRef {
                block: BlockRef {
                    chain_key: p.chain_key.clone(),
                    number_or_slot: state.bank_slot,
                    hash: "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso".into(),
                    parent_hash: "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE".into(),
                    timestamp_ms: state.bank_unix_timestamp * 1000,
                },
                canonical_epoch: 1,
                journal_seq: 1,
                schema_version: SCHEMA_VERSION,
                adapter_set_hash: hash(&[&p.adapter_version]).unwrap(),
                online_generation: 1,
                scope_hash: scope_hash(std::slice::from_ref(&p)).unwrap(),
                writer_epoch: 1,
                owner_service: chain.owner_service.clone(),
                owner_assignment_version: chain.assignment_version,
            },
            pools: vec![p.clone()],
            source: "CONTROLLED_ORIGINAL_RAYCLMM_LIMIT_ORDERS".into(),
            history_gaps: vec![],
        };
        let api = Api::new(config).unwrap();
        api.replica.install_baseline(snapshot).unwrap();
        let (sell, buy) = if case["left"] == true {
            (&p.token0, &p.token1)
        } else {
            (&p.token1, &p.token0)
        };
        let mut request = json!({"chainKey":p.chain_key,"sellToken":sell,"buyToken":buy,
            "caller":"9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso",
            "recipient":"4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE",
            "slippageBps":0,"verification":"FAST"});
        let (endpoint, input_field) = if case["exact_in"] == true {
            request["kind"] = "EXACT_IN".into();
            request["sellAmount"] = case["amount"].clone();
            ("/v1/quote", "consumedIn")
        } else {
            request["kind"] = "EXACT_OUT".into();
            request["buyAmount"] = case["amount"].clone();
            request["maxSellAmount"] = "100000".into();
            ("/v1/quote-exact-out", "requiredSellAmount")
        };
        let response = api
            .handler()
            .oneshot(
                Request::post(endpoint)
                    .header("authorization", "Bearer rayclmm-orders-test")
                    .header("content-type", "application/json")
                    .body(Body::from(request.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 128 << 10).await.unwrap())
                .unwrap();
        assert_eq!(status, StatusCode::OK, "{request}: {body}");
        assert_eq!(body[input_field], case["expected"]["consumed"]);
        assert_eq!(body["grossBuyAmount"], case["expected"]["output"]);
        tested += 1;
    }
    assert_eq!(tested, 12);
}
