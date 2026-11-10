//! In-process HTTP quote scope; actual process/VM execution belongs to B5.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Snapshot};
use xone_quote_api::Api;
fn api_source(id: &str) -> (Api, Snapshot, Value) {
    let v: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-bpt-pools.json"
    ))
    .unwrap();
    let c = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone();
    let b = v["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == c["before"])
        .unwrap();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools = b["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| serde_json::from_value(p["pool"].clone()).unwrap())
        .collect();
    let number =
        |v: &Value| u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    s.reference.block.number_or_slot = number(&b["header"]["number"]);
    s.reference.block.timestamp_ms = number(&b["header"]["timestamp"]) * 1000;
    s.reference.block.hash = b["header"]["hash"].as_str().unwrap().into();
    s.reference.block.parent_hash = b["header"]["parentHash"].as_str().unwrap().into();
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let mut adapters = s
        .pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<Vec<_>>();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = hash(&adapters).unwrap();
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "local-bpt-scope-explicit-credential".into();
    config.config.routing.max_hops = 2;
    config.config.routing.max_splits = 2;
    config.config.routing.split_parts = 10;
    config.config.api.quote_timeout_ms = 10000;
    let api = Api::new(config).unwrap();
    api.replica.install_baseline(s.clone()).unwrap();
    (api, s, c)
}
async fn quote(api: &Api, c: &Value, caller: &str) -> (StatusCode, Value) {
    let response=api.handler().oneshot(Request::builder().method("POST").uri("/v1/quote").header("authorization",format!("Bearer {}",api.config.token)).header("content-type","application/json").body(Body::from(json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":c["token_in"],"buyToken":c["token_out"],"sellAmount":c["amount_in"],"caller":caller,"recipient":caller,"slippageBps":50,"verification":"FAST","constraints":{"maxHops":1}}).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let body =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    (status, body)
}
#[tokio::test]
async fn bpt_http_quotes_preserve_native_credit_and_indicative_boundary() {
    for id in ["settled/C0", "settled/C1"] {
        let (api, _, c) = api_source(id);
        let (status, body) = quote(&api, &c, c["actor"].as_str().unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["grossBuyAmount"],
            c["expected_native"]["actual_amount_out"]
        );
        assert_eq!(body["verification"]["level"], "INDICATIVE");
        assert_eq!(body["verification"]["success"], false);
    }
}
#[tokio::test]
async fn bpt_http_caller_membership_is_not_lost_between_search_and_optimizer() {
    let v: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-bpt-pools.json"
    ))
    .unwrap();
    let c = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"] == "join-allowlisted")
        .unwrap();
    let (api, _, c) = api_source(c["id"].as_str().unwrap());
    let (status, body) = quote(&api, &c, c["actor"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["grossBuyAmount"],
        c["expected_native"]["actual_amount_out"]
    );
    let (status, _) = quote(&api, &c, "0x0000000000000000000000000000000000000001").await;
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "writes explicit selected-API input for owned original VM replay"]
async fn actual_selected_bpt_api_report() {
    let v: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-bpt-pools.json"
    ))
    .unwrap();
    let output = std::path::PathBuf::from(
        std::env::var("XONE_BPT_ACTUAL_API_REPORT").expect("explicit output path"),
    );
    assert!(output.is_absolute());
    let mut rows = Vec::new();
    for id in ["settled/C0", "settled/C1", "scheduled/C0"] {
        let (api, _, c) = api_source(id);
        let caller = c["actor"].as_str().unwrap();
        let (status, q) = quote(&api, &c, caller).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let request = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":c["token_in"],"buyToken":c["token_out"],"sellAmount":c["amount_in"],"caller":caller,"recipient":caller,"slippageBps":50,"verification":"FAST","constraints":{"maxHops":1}});
        rows.push(json!({"id":format!("HTTP/{}",rows.len()),"namespace":c["namespace"],"source_bank":c["before"],"request":request,"quote":q}));
    }
    let (api, source, c) = api_source("settled/C1");
    let other = source
        .pools
        .iter()
        .find(|p| p.pool_id != c["token_in"].as_str().unwrap())
        .unwrap();
    let caller = c["actor"].as_str().unwrap();
    let request = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":c["token_in"],"buyToken":other.pool_id,"sellAmount":c["amount_in"],"caller":caller,"recipient":caller,"slippageBps":50,"verification":"FAST","constraints":{"maxHops":2}});
    let q = api
        .quote(serde_json::from_value(request.clone()).unwrap())
        .await
        .unwrap();
    assert_eq!(q["routes"][0]["path"]["edges"].as_array().unwrap().len(), 2);
    rows.push(json!({"id":format!("API/{}",rows.len()),"namespace":c["namespace"],"source_bank":c["before"],"request":request,"quote":q}));
    let (api, source, c) = api_source("settled/C0");
    let first = &source.pools[0];
    let other = &source.pools[1];
    let m = first
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    let native: Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/contracts/managed-v2-bpt.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let caller = native["branches"]["settled"]["roles"]["lp"]
        .as_str()
        .unwrap();
    let amount = xone_domain::Amount(&m.actual_supply.0 * 4u8 / 10u8).to_string();
    let request = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":first.pool_id,"buyToken":other.pool_id,"sellAmount":amount,"caller":caller,"recipient":caller,"slippageBps":50,"verification":"FAST","constraints":{"maxHops":2}});
    let q = api
        .quote(serde_json::from_value(request.clone()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        q["routes"].as_array().unwrap().len(),
        2,
        "actual physical shared capacity split"
    );
    rows.push(json!({"id":format!("API/{}",rows.len()),"namespace":c["namespace"],"source_bank":c["before"],"request":request,"quote":q}));
    let report = json!({"schema_version":1,"original_bpt_sha256":v["original_raw"]["sha256"],"authority":"actual-in-process-API-responses-for-independent-original-VM-replay","quotes":rows});
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(file, &report).unwrap();
}

#[tokio::test]
async fn managed_http_rejects_mixed_case_vault_funds_alias() {
    let (api, source, c) = api_source("settled/C0");
    let pool = &source.pools[0];
    let s = pool.multi_asset.as_ref().unwrap();
    let upper = format!("0x{}", s.vault[2..].to_ascii_uppercase());
    let request = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":s.assets[0].token,"buyToken":s.assets[1].token,"sellAmount":c["amount_in"],"caller":c["actor"],"recipient":upper,"slippageBps":50,"verification":"FAST","constraints":{"maxHops":1}});
    assert!(api
        .quote(serde_json::from_value(request).unwrap())
        .await
        .is_err());
}
