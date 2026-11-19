#[path = "support/evm_bank.rs"]
mod bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use bank::{setup, source};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_quote_api::Api;
async fn call(api: &Api, id: &str, payload: Value, authorized: bool) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/v1/quotes/{id}/prepare-evm"))
        .header("content-type", "application/json");
    if authorized {
        request = request.header("authorization", format!("Bearer {}", api.config.token));
    }
    let response = api
        .handler()
        .oneshot(request.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let json = serde_json::from_slice(
        &to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    (status, json)
}
#[tokio::test]
async fn explicit_evm_preparation_binds_quote_view_and_downloadable_wire() {
    let (api, _, request, observations, task) = setup("ready").await;
    let q = api
        .quote(serde_json::from_value(request).unwrap())
        .await
        .unwrap();
    let payload = serde_json::to_value(source::request()).unwrap();
    let (status, p) = call(&api, q["quoteId"].as_str().unwrap(), payload, true).await;
    task.abort();
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(p["ready"], true);
    assert_eq!(p["callerApproval"]["status"], "SUFFICIENT");
    assert_eq!(p["callerApproval"]["additionalApprovalNative"], "0");
    assert_eq!(
        p["callerApproval"]["bindingHash"],
        p["component"]["binding_hash"]
    );
    assert_eq!(p["quoteId"], q["quoteId"]);
    assert_eq!(p["quoteViewId"], q["quoteViewId"]);
    assert_eq!(p["snapshotRef"], q["snapshotRef"]);
    assert_eq!(p["signed"], false);
    assert_eq!(p["submitted"], false);
    assert_eq!(p["independentExecutionVerified"], false);
    assert!(p["unsignedTransaction"]
        .as_str()
        .unwrap()
        .starts_with("0x02"));
    assert_eq!(p["transaction"]["chainId"], "0x7a69");
    let _: xone_execution_preparation::evm::Preparation =
        serde_json::from_value(p["component"].clone()).unwrap();
    let mut component = p["component"].clone();
    let committed = component
        .as_object_mut()
        .unwrap()
        .remove("preparation_hash")
        .unwrap();
    assert_eq!(committed, xone_domain::hash(&component).unwrap());
    let mut rpc = p["rpcPreparation"].clone();
    let rpc_hash = rpc
        .as_object_mut()
        .unwrap()
        .remove("rpc_preparation_hash")
        .unwrap();
    assert_eq!(rpc_hash, xone_domain::hash(&rpc).unwrap());
    assert_eq!(rpc["component"], p["component"]);

    let mut expected = p.clone();
    let h = expected
        .as_object_mut()
        .unwrap()
        .remove("preparationHash")
        .unwrap();
    assert_eq!(h, xone_domain::hash(&expected).unwrap());
    assert!(observations.lock().unwrap().len() > 25);
}
#[tokio::test]
async fn evm_prepare_auth_unknown_context_and_reorg_do_not_call_rpc() {
    let (api, mut snapshot, request, observations, task) = setup("ready").await;
    let q = api
        .quote(serde_json::from_value(request).unwrap())
        .await
        .unwrap();
    let id = q["quoteId"].as_str().unwrap();
    let payload = serde_json::to_value(source::request()).unwrap();
    let (status, _) = call(&api, id, payload.clone(), false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(&api, "missing", payload.clone(), true).await;
    assert_ne!(status, StatusCode::OK);
    let mut bad = payload.clone();
    bad["gas_limit"] = json!(700000);
    let (status, _) = call(&api, id, bad, true).await;
    assert_ne!(status, StatusCode::OK);
    snapshot.reference.online_generation += 1;
    snapshot.reference.journal_seq += 1;
    api.replica.install_baseline(snapshot).unwrap();
    let (status, p) = call(&api, id, payload, true).await;
    assert_ne!(status, StatusCode::OK);
    assert!(matches!(
        p["error"].as_str(),
        Some("SNAPSHOT_REORGED" | "QUOTE_STATE_CHANGED")
    ));
    assert!(observations.lock().unwrap().is_empty());
    task.abort();
}
#[tokio::test]
async fn evm_rpc_rejection_and_funding_blockers_preserve_api_semantics() {
    for mode in [
        "wrong_chain",
        "reserves",
        "nonce_changed",
        "funding",
        "allowance",
    ] {
        let (api, _, request, _, task) = setup(mode).await;
        let q = api
            .quote(serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        let (status, p) = call(
            &api,
            q["quoteId"].as_str().unwrap(),
            serde_json::to_value(source::request()).unwrap(),
            true,
        )
        .await;
        task.abort();
        if matches!(mode, "funding" | "allowance") {
            assert_eq!(status, StatusCode::OK, "{p}");
            assert_eq!(p["ready"], false);
            if mode == "allowance" {
                assert_eq!(p["callerApproval"]["status"], "APPROVAL_REQUIRED");
                assert_eq!(p["callerApproval"]["observedAllowance"], "0");
                assert!(p["callerApproval"]["additionalApprovalNative"].is_null());
            }
            assert!(p["unsignedTransaction"].is_null());
            assert!(p["transaction"].is_null());
        } else {
            assert_ne!(status, StatusCode::OK, "{p}");
            assert!(p["unsignedTransaction"].is_null());
        }
    }
}
#[tokio::test]
async fn exact_output_preparation_uses_its_own_target_and_maximum_input() {
    let (api, _, mut request, _, task) = setup("ready").await;
    let q = api
        .quote(serde_json::from_value(request.clone()).unwrap())
        .await
        .unwrap();
    request["kind"] = "EXACT_OUT".into();
    request["buyAmount"] = q["grossBuyAmount"].clone();
    request["maxSellAmount"] = q["sellAmount"].clone();
    request.as_object_mut().unwrap().remove("sellAmount");
    let quote = api
        .quote_exact_out(serde_json::from_value(request).unwrap())
        .await
        .unwrap();
    let (status, p) = call(
        &api,
        quote["quoteId"].as_str().unwrap(),
        serde_json::to_value(source::request()).unwrap(),
        true,
    )
    .await;
    task.abort();
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(p["quoteKind"], "EXACT_OUT");
    assert_eq!(
        p["callerApproval"]["requiredInput"],
        quote["requiredSellAmount"]
    );
    let _: xone_execution_preparation::evm::ExactOutPreparation =
        serde_json::from_value(p["component"].clone()).unwrap();
    let mut component = p["component"].clone();
    let committed = component
        .as_object_mut()
        .unwrap()
        .remove("exact_out_preparation_hash")
        .unwrap();
    assert_eq!(committed, xone_domain::hash(&component).unwrap());

    assert_eq!(p["component"]["target_output"], quote["grossBuyAmount"]);
    assert_eq!(p["requiredInput"], quote["requiredSellAmount"]);
    assert!(p["transaction"]["data"]
        .as_str()
        .unwrap()
        .starts_with("0x8803dbee"));
}
