use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_quote_api::Api;

fn config() -> xone_config::ConfigSnapshot {
    let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    config.token = "explicit-local-simulation-api-credential".into();
    config
}

async fn simulate(api: &Api, authenticated: bool) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/v1/quotes/missing/simulate");
    if authenticated {
        request = request.header("authorization", format!("Bearer {}", api.config.token));
    }
    let response = api
        .handler()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn local_simulation_route_authenticates_before_backend_or_quote_lookup() {
    let api = Api::new(config()).unwrap();
    let (status, body) = simulate(&api, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({"error":"UNAUTHORIZED"}));
}

#[tokio::test]
async fn disabled_backend_remains_closed_and_never_reads_a_state_file() {
    let api = Api::new(config()).unwrap();
    let (status, body) = simulate(&api, true).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "INDEPENDENT_SIMULATION_UNAVAILABLE");
}

struct StateFile(std::path::PathBuf);
impl StateFile {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "xone-api-sim-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}
impl Drop for StateFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn bundle() -> Value {
    let corpus: Value =
        serde_json::from_str(include_str!("../../../test/contracts/evm-execution.json")).unwrap();
    let c = &corpus["cases"][0];
    json!({"schema_version":1,"fixture":{"schema_version":1,"state_anchor":c["state_anchor"],"execution_environment":c["execution_environment"],"accounts":c["accounts"],"block_hashes":c["block_hashes"]},"executor":c["executor"],"transaction_context":{"nonce":c["transaction_context"]["nonce"],"gas_limit":c["transaction_context"]["gas_limit"].as_u64().expect("native fixture schema: transaction_context.gas_limit must be u64"),"gas_price":c["transaction_context"]["gas_price"],"chain_id":c["transaction_context"]["chain_id"]}})
}
fn fixture_config(file: &StateFile) -> xone_config::ConfigSnapshot {
    let mut config = config();
    let bundle = bundle();
    let sim = &mut config.config.simulation;
    sim.backend = "revm-fixture".into();
    sim.rpc_url.clear();
    sim.state_file = file.0.to_string_lossy().into_owned();
    sim.max_state_bytes = 4 << 20;
    sim.max_instructions = 1_000_000;
    sim.max_memory_bytes = 4 << 20;
    sim.concurrency = 1;
    sim.timeout_ms = 10_000;
    sim.allowed_targets = vec![bundle["executor"]["target"].as_str().unwrap().into()];
    config
        .config
        .chains
        .iter_mut()
        .find(|c| c.key == "evm:31337")
        .unwrap()
        .genesis_hash = bundle["fixture"]["state_anchor"]["genesis_hash"]
        .as_str()
        .unwrap()
        .into();
    config
}

#[test]
fn configured_bundle_must_exist_be_bounded_and_have_valid_schema() {
    for case in [
        "missing",
        "oversize",
        "malformed",
        "schema",
        "image",
        "chain",
        "genesis",
    ] {
        let mut value = bundle();
        if case == "schema" {
            value["schema_version"] = 2.into();
        }
        if case == "image" {
            value["fixture"]["accounts"][0]["code"] = "0xzz".into();
        }
        if case == "chain" {
            value["fixture"]["state_anchor"]["chain_key"] = "evm:99999".into();
        }
        if case == "genesis" {
            value["fixture"]["state_anchor"]["genesis_hash"] =
                format!("0x{}", "ab".repeat(32)).into();
        }
        let file = StateFile::new(
            if case == "malformed" {
                b"not json".to_vec()
            } else {
                serde_json::to_vec(&value).unwrap()
            }
            .as_slice(),
        );
        let mut config = fixture_config(&file);
        if case == "missing" {
            std::fs::remove_file(&file.0).unwrap();
        }
        if case == "oversize" {
            config.config.simulation.max_state_bytes = 1024;
        }
        assert!(Api::new(config).is_err(), "{case}");
    }
}

#[tokio::test]
async fn configured_backend_rejects_unknown_quotes_and_respects_rate_limit() {
    let file = StateFile::new(&serde_json::to_vec(&bundle()).unwrap());
    let mut config = fixture_config(&file);
    config.config.api.requests_per_minute = 1;
    let api = Api::new(config).unwrap();
    let (status, body) = simulate(&api, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "UNKNOWN_QUOTE");
    assert_eq!(simulate(&api, true).await.0, StatusCode::TOO_MANY_REQUESTS);
}

fn snapshot() -> xone_domain::Snapshot {
    let corpus: Value =
        serde_json::from_str(include_str!("../../../test/contracts/evm-execution.json")).unwrap();
    let c = &corpus["cases"][0];
    let mut snapshot: xone_domain::Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![serde_json::from_value(c["pool"].clone()).unwrap()];
    let block = &mut snapshot.reference.block;
    block.chain_key = c["state_anchor"]["chain_key"].as_str().unwrap().into();
    block.number_or_slot = c["state_anchor"]["block_number"].as_u64().unwrap();
    block.hash = c["state_anchor"]["block_hash"].as_str().unwrap().into();
    block.parent_hash = c["state_anchor"]["parent_hash"].as_str().unwrap().into();
    block.timestamp_ms = c["state_anchor"]["timestamp"].as_u64().unwrap() * 1000;
    snapshot.reference.scope_hash = xone_domain::scope_hash(&snapshot.pools).unwrap();
    snapshot.reference.adapter_set_hash = xone_domain::hash(&vec!["uniswap-v2/1"]).unwrap();
    snapshot
}
async fn cached_quote(api: &Api) -> String {
    let corpus: Value =
        serde_json::from_str(include_str!("../../../test/contracts/evm-execution.json")).unwrap();
    let c = &corpus["cases"][0];
    let response = api.quote(serde_json::from_value(json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":c["token_in"],"buyToken":c["token_out"],"sellAmount":c["amount_in"],"caller":c["transaction"]["from"],"recipient":c["recipient"],"slippageBps":50,"verification":"FAST"})).unwrap()).await.unwrap();
    response["quoteId"].as_str().unwrap().into()
}
async fn simulate_id(api: &Api, id: &str) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/quotes/{id}/simulate"))
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn simulation_rejects_expired_invalidated_and_mismatched_source_quotes() {
    for case in ["expired", "invalidated", "source_block"] {
        let file = StateFile::new(&serde_json::to_vec(&bundle()).unwrap());
        let mut config = fixture_config(&file);
        config.config.routing.max_state_age_ms = 315_360_000_000;
        config.config.routing.max_splits = 1;
        if case == "expired" {
            config.config.routing.quote_ttl_ms = 10;
        }
        let api = Api::new(config).unwrap();
        let mut snapshot = snapshot();
        if case == "source_block" {
            snapshot.reference.block.hash = format!("0x{}", "aa".repeat(32));
        }
        api.replica.install_baseline(snapshot).unwrap();
        let id = cached_quote(&api).await;
        if case == "expired" {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        if case == "invalidated" {
            api.replica.suspend("evm:31337");
        }
        let (status, body) = simulate_id(&api, &id).await;
        assert_ne!(status, StatusCode::OK, "{case}");
        assert_eq!(
            body["error"],
            match case {
                "expired" => "QUOTE_EXPIRED",
                "invalidated" => "SNAPSHOT_REORGED",
                _ => "SIMULATION_ANCHOR_MISMATCH",
            },
            "{case}"
        );
    }
}
