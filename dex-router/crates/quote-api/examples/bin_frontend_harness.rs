//! Loopback-only live API fixture for immutable singleton CL output quotes.
use serde_json::{json, Value};
use xone_domain::{hash, scope_hash, Pool, Snapshot};
#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "info path and frontend origin required");
    let corpus: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/infinity-bin-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut info = Vec::new();
    for model in ["pancake-infinity-bin"] {
        let forward = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["pool"]["protocol"] == model
                    && c["zeroForOne"] == true
                    && c["passed"] == true
                    && c["target"] != "1"
            })
            .unwrap();
        let reverse = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["pool"] == forward["pool"]
                    && c["zeroForOne"] == false
                    && c["passed"] == true
                    && c["target"] != "1"
            })
            .unwrap();
        let pool: Pool = serde_json::from_value(forward["pool"].clone()).unwrap();
        let mut cfg = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        cfg.token = "bin-browser-test".into();
        cfg.config.routing.max_state_age_ms = 315360000000;
        cfg.config.routing.max_hops = 1;
        cfg.config.api.requests_per_minute = 10000;
        cfg.config.api.cors_allowed_origins = vec![args[2].clone()];
        let api = xone_quote_api::Api::new(cfg).unwrap();
        let mut s: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        let bank = pool.infinity_bin.as_ref().unwrap();
        s.reference.block.number_or_slot = bank.block_number;
        s.reference.block.hash = bank.block_hash.clone();
        s.reference.block.timestamp_ms = bank.timestamp * 1000;
        s.reference.adapter_set_hash = hash(&vec![pool.adapter_version.clone()]).unwrap();
        s.pools = vec![pool];
        s.reference.scope_hash = scope_hash(&s.pools).unwrap();
        s.source = "CONTROLLED_BIN_OUTPUT_CORE".into();
        api.replica.install_baseline(s).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        info.push(json!({"api":format!("http://{}",listener.local_addr().unwrap()),"cases":[forward,reverse]}));
        tokio::spawn(async move {
            axum::serve(listener, api.handler()).await.unwrap();
        });
    }
    std::fs::write(
        &args[1],
        serde_json::to_vec_pretty(&json!({"token":"bin-browser-test","services":info})).unwrap(),
    )
    .unwrap();
    std::future::pending::<()>().await;
}
