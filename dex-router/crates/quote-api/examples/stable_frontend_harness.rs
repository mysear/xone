//! Loopback-only live API fixture for immutable StablePool output quotes.
use serde_json::{json, Value};
use xone_domain::{hash, scope_hash, Pool, Snapshot};
#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "info path and frontend origin required");
    let corpus: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/stable-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut info = Vec::new();
    {
        let model = "balancer-v2-stable2";
        let (i, j) = (0, 1);
        let forward = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["pool"]["protocol"] == model
                    && c["i"] == i
                    && c["j"] == j
                    && c["expected"]["reverted"] != true
                    && c["target"] != "1"
            })
            .unwrap();
        let reverse = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["pool"] == forward["pool"]
                    && c["i"] == j
                    && c["j"] == i
                    && c["expected"]["reverted"] != true
                    && c["target"] != "1"
            })
            .unwrap();
        let pool: Pool = serde_json::from_value(forward["pool"].clone()).unwrap();
        let mut cfg = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        cfg.token = "stable-browser-test".into();
        cfg.config.routing.max_state_age_ms = 315360000000;
        cfg.config.routing.max_hops = 1;
        cfg.config.api.requests_per_minute = 10000;
        cfg.config.api.cors_allowed_origins = vec![args[2].clone()];
        let api = xone_quote_api::Api::new(cfg).unwrap();
        let mut s: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        s.reference.adapter_set_hash = hash(&vec![pool.adapter_version.clone()]).unwrap();
        s.pools = vec![pool];
        s.reference.scope_hash = scope_hash(&s.pools).unwrap();
        s.source = "CONTROLLED_STABLE_SOLVER".into();
        api.replica.install_baseline(s).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        info.push(json!({"api":format!("http://{}",listener.local_addr().unwrap()),"cases":[forward,reverse]}));
        tokio::spawn(async move {
            axum::serve(listener, api.handler()).await.unwrap();
        });
    }
    std::fs::write(
        &args[1],
        serde_json::to_vec_pretty(&json!({"token":"stable-browser-test","services":info})).unwrap(),
    )
    .unwrap();
    std::future::pending::<()>().await;
}
