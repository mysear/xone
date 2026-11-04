use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
use xone_quote_api::Api;

#[tokio::test]
async fn simulation_authentication_precedes_body_collection_and_limits() {
    let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    config.token = "explicit-api-body-ordering-credential".into();
    config.config.api.max_body_bytes = 1024;
    let api = Api::new(config).unwrap();
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quotes/missing/simulate")
                .body(Body::from(vec![0; 2048]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(value["error"], "UNAUTHORIZED");
}

fn config() -> xone_config::ConfigSnapshot {
    let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    config.token = "explicit-api-body-ordering-credential".into();
    config
}

#[tokio::test]
async fn simulation_body_rejects_all_overrides_after_auth_and_rate_limit() {
    let api = Api::new(config()).unwrap();
    for body in [
        r#"{"caller":"override"}"#,
        r#"{"recipient":"override"}"#,
        r#"{"calldata":"0x"}"#,
        r#"{"state_file":"/tmp/other"}"#,
        r#"{"executor":"other"}"#,
        r#"{"transaction":{"to":"other","value":"1"}}"#,
        r#"{"transactionContext":{"nonce":999,"gas_limit":999}}"#,
        "[]",
        "null",
        "not json",
    ] {
        for authorized in [false, true] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/v1/quotes/missing/simulate");
            if authorized {
                request = request.header("authorization", format!("Bearer {}", api.config.token));
            }
            let response = api
                .handler()
                .oneshot(request.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["error"],
                if authorized {
                    "INVALID_SIMULATION_REQUEST"
                } else {
                    "UNAUTHORIZED"
                },
                "{body}"
            );
        }
    }
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quotes/missing/simulate")
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(value["error"], "INDEPENDENT_SIMULATION_UNAVAILABLE");
}

#[tokio::test]
async fn simulation_rate_limit_precedes_body_collection() {
    let mut config = config();
    config.config.api.requests_per_minute = 1;
    config.config.api.max_body_bytes = 1024;
    let api = Api::new(config).unwrap();
    for (body, expected) in [
        (Vec::new(), StatusCode::UNPROCESSABLE_ENTITY),
        (vec![0; 2048], StatusCode::TOO_MANY_REQUESTS),
    ] {
        let response = api
            .handler()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/quotes/missing/simulate")
                    .header("authorization", format!("Bearer {}", api.config.token))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
