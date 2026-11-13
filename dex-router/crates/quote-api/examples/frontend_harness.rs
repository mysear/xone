//! Explicit local integration harness; never used by the production daemon.
#[path = "../tests/support/execution_bank.rs"]
mod execution_bank;
use axum::{
    extract::State,
    middleware::{self, Next},
    response::Response,
    routing::post,
    Json, Router,
};
use execution_bank::Bank;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use xone_domain::Snapshot;
#[derive(Clone)]
struct Control {
    bank: Bank,
    raydium: Bank,
    pump: Bank,
    curve: Bank,
    curve_ata: Bank,
    funding: Vec<Bank>,
    api: xone_quote_api::Api,
    snapshot: Arc<Mutex<Snapshot>>,
}
#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "frontend-harness INFO_FILE FRONTEND_ORIGIN");
    let runtime = std::env::var("XONE_TEST_SBF_MANIFEST_DIR").ok();
    let runtime_path = |name: &str| {
        runtime.as_ref().map(|r| {
            format!(
                "{r}/{}.json",
                if std::env::var_os("XONE_TEST_RPC_CAPTURE").is_some() {
                    "rpc-capture"
                } else {
                    name
                }
            )
        })
    };
    let curve = Bank::pump_curve(4);
    let (curve_rpc, curve_rpc_task) = curve.serve().await;
    let (mut curve_api, _) =
        curve.api_with_manifest(&curve_rpc, runtime_path("rpc-capture").as_deref());
    Arc::make_mut(&mut curve_api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let curve_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let curve_addr = curve_listener.local_addr().unwrap();
    let curve_task = tokio::spawn(async move {
        axum::serve(curve_listener, curve_api.handler())
            .await
            .unwrap();
    });
    let curve_ata = Bank::pump_curve(6);
    let (curve_ata_rpc, curve_ata_rpc_task) = curve_ata.serve().await;
    let (mut curve_ata_api, _) =
        curve_ata.api_with_manifest(&curve_ata_rpc, runtime_path("rpc-capture").as_deref());
    Arc::make_mut(&mut curve_ata_api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let curve_ata_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let curve_ata_addr = curve_ata_listener.local_addr().unwrap();
    let curve_ata_task = tokio::spawn(async move {
        axum::serve(curve_ata_listener, curve_ata_api.handler())
            .await
            .unwrap();
    });
    let pump = Bank::pump_amm(8);
    let (pump_rpc, pump_rpc_task) = pump.serve().await;
    let (mut pump_api, _) =
        pump.api_with_manifest(&pump_rpc, runtime_path("rpc-capture").as_deref());
    Arc::make_mut(&mut pump_api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let pump_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pump_addr = pump_listener.local_addr().unwrap();
    let pump_task = tokio::spawn(async move {
        axum::serve(pump_listener, pump_api.handler())
            .await
            .unwrap();
    });
    let token2022 = Bank::token2022(1);
    let (token2022_rpc, token2022_rpc_task) = token2022.serve().await;
    let (mut token2022_api, _) =
        token2022.api_with_manifest(&token2022_rpc, runtime_path("token2022").as_deref());
    Arc::make_mut(&mut token2022_api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let token2022_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let token2022_addr = token2022_listener.local_addr().unwrap();
    let token2022_task = tokio::spawn(async move {
        axum::serve(token2022_listener, token2022_api.handler())
            .await
            .unwrap();
    });
    let raydium = Bank::raydium(0);
    let (raydium_rpc, raydium_rpc_task) = raydium.serve().await;
    let (mut raydium_api, _) =
        raydium.api_with_manifest(&raydium_rpc, runtime_path("raydium").as_deref());
    Arc::make_mut(&mut raydium_api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let raydium_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let raydium_addr = raydium_listener.local_addr().unwrap();
    let raydium_task = tokio::spawn(async move {
        axum::serve(raydium_listener, raydium_api.handler())
            .await
            .unwrap();
    });
    let mut funding = Vec::new();
    let mut funding_info = Vec::new();
    let mut funding_tasks = Vec::new();
    for index in [9, 21, 11, 12] {
        let bank = Bank::pump_amm(index);
        let (rpc, rpc_task) = bank.serve().await;
        let (mut api, _) = bank.api_with_manifest(&rpc, runtime_path("rpc-capture").as_deref());
        Arc::make_mut(&mut api.config)
            .config
            .api
            .cors_allowed_origins = vec![args[2].clone()];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        funding_info.push(json!({"index":index,"api":format!("http://{}",listener.local_addr().unwrap()),"bank":&*bank.data}));
        funding_tasks.push(rpc_task);
        funding_tasks.push(tokio::spawn(async move {
            axum::serve(listener, api.handler()).await.unwrap();
        }));
        funding.push(bank);
    }
    let bank = Bank::new();
    let wsol = Bank::wsol(None);
    let (wsol_rpc, wsol_rpc_task) = wsol.serve().await;
    let (mut wsol_api, _) = wsol.api_with_manifest(&wsol_rpc, runtime_path("wsol").as_deref());
    Arc::make_mut(&mut wsol_api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let wsol_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wsol_addr = wsol_listener.local_addr().unwrap();
    let wsol_task = tokio::spawn(async move {
        axum::serve(wsol_listener, wsol_api.handler())
            .await
            .unwrap();
    });
    let (rpc, rpc_task) = bank.serve().await;
    let (mut api, snapshot) = bank.api_with_manifest(&rpc, runtime_path("meteora").as_deref());
    Arc::make_mut(&mut api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let orca_corpus: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/orca-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut orca_info = Vec::new();
    let mut orca_tasks = Vec::new();
    for protocol in [
        "orca-whirlpools-static",
        "orca-whirlpools-rewards",
        "orca-whirlpools-adaptive",
        "orca-whirlpools-adaptive-rewards",
        "orca-whirlpools-adaptive-dynamic",
    ] {
        let c = orca_corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["profile"] == protocol
                    && c["passed"] == true
                    && c["zeroForOne"] == true
                    && c["target"] == "1000"
            })
            .unwrap();
        let reverse = orca_corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r["pool"] == c["pool"]
                    && r["passed"] == true
                    && r["zeroForOne"] == false
                    && r["target"] == "1000"
            })
            .unwrap();
        let pool: xone_domain::Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let mut cfg = api.config.as_ref().clone();
        cfg.config.api.transaction_submission_enabled = false;
        cfg.config.api.transaction_submission_chains.clear();
        cfg.config.api.transaction_receipt_journal.clear();
        cfg.config.api.transaction_max_pending = 0;
        cfg.config.api.solana_runtime_manifest.clear();
        let orca = xone_quote_api::Api::new(cfg).unwrap();
        let mut baseline = snapshot.clone();
        let state = pool.orca_whirlpool.as_ref().unwrap();
        baseline.reference.block.number_or_slot = state.bank_slot;
        baseline.reference.block.timestamp_ms = state.bank_unix_timestamp * 1000;
        baseline.reference.online_generation += 1;
        baseline.reference.journal_seq += 1;
        baseline.reference.adapter_set_hash =
            xone_domain::hash(&vec![pool.adapter_version.clone()]).unwrap();
        baseline.pools = vec![pool];
        baseline.reference.scope_hash = xone_domain::scope_hash(&baseline.pools).unwrap();
        baseline.source = "CONTROLLED_ORIGINAL_ORCA_EXACT_OUT_NATIVE".into();
        orca.replica.install_baseline(baseline).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        orca_info.push(
            json!({"api":format!("http://{}",listener.local_addr().unwrap()),"cases":[c,reverse]}),
        );
        orca_tasks.push(tokio::spawn(async move {
            axum::serve(listener, orca.handler()).await.unwrap();
        }));
    }
    let control = Control {
        bank: bank.clone(),
        raydium: raydium.clone(),
        pump: pump.clone(),
        curve: curve.clone(),
        curve_ata: curve_ata.clone(),
        funding,
        api: api.clone(),
        snapshot: Arc::new(Mutex::new(snapshot)),
    };
    let admin = Router::new()
        .route("/test/mode", post(mode))
        .route("/test/sign", post(test_sign))
        .route("/test/wallet-stats", post(wallet_stats))
        .with_state(control.clone())
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(args[2].parse::<axum::http::HeaderValue>().unwrap())
                .allow_methods([axum::http::Method::POST])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        );
    let routes = api
        .handler()
        .merge(admin)
        .layer(middleware::from_fn_with_state(control, delay));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::fs::write(&args[1],serde_json::to_vec_pretty(&json!({"api":format!("http://{}",listener.local_addr().unwrap()),"token":execution_bank::AUTH,"bank":&*bank.data,"wsolApi":format!("http://{wsol_addr}"),"wsolBank":&*wsol.data,"token2022Api":format!("http://{token2022_addr}"),"token2022Bank":&*token2022.data,"raydiumApi":format!("http://{raydium_addr}"),"raydiumBank":&*raydium.data,"pumpApi":format!("http://{pump_addr}"),"pumpBank":&*pump.data,"curveApi":format!("http://{curve_addr}"),"curveBank":&*curve.data,"curveAtaApi":format!("http://{curve_ata_addr}"),"curveAtaBank":&*curve_ata.data,"funding":funding_info,"orcaQuotes":orca_info,"amm":serde_json::from_slice::<Value>(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),"/../../test/contracts/pump-amm-sell-target.json")).unwrap()).unwrap()["cases"][33]})).unwrap()).unwrap();
    axum::serve(listener, routes)
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            {
                let mut signal =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .unwrap();
                tokio::select! {_=signal.recv()=>{},_=tokio::signal::ctrl_c()=>{}}
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    for task in orca_tasks {
        task.abort();
    }
    for task in funding_tasks {
        task.abort();
    }
    rpc_task.abort();
    wsol_rpc_task.abort();
    wsol_task.abort();
    token2022_rpc_task.abort();
    token2022_task.abort();
    raydium_rpc_task.abort();
    curve_ata_rpc_task.abort();
    curve_ata_task.abort();
    let _ = curve_ata_task.await;
    curve_rpc_task.abort();
    curve_task.abort();
    let _ = curve_task.await;
    pump_rpc_task.abort();
    pump_task.abort();
    let _ = pump_task.await;
    raydium_task.abort();
    let _ = wsol_task.await;
    let _ = token2022_task.await;
    let _ = raydium_task.await;
}
async fn delay(State(c): State<Control>, req: axum::extract::Request, next: Next) -> Response {
    let mode = c.bank.mode.lock().unwrap().clone();
    let slow = (mode == "delay_quotes"
        || (mode == "delay_preparations" && req.uri().path().ends_with("/prepare")))
        && req.uri().path().starts_with("/v1/quote")
        && req.method() == "POST";
    if slow {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }
    next.run(req).await
}
async fn mode(State(c): State<Control>, Json(body): Json<Value>) -> Json<Value> {
    let mode = body["mode"].as_str().unwrap();
    *c.bank.mode.lock().unwrap() = mode.into();
    let mut s = c.snapshot.lock().unwrap();
    if mode == "suspend" {
        c.api.replica.suspend(&s.reference.block.chain_key);
    }
    if mode == "restore" {
        s.reference.online_generation += 1;
        s.reference.journal_seq += 1;
        c.api.replica.install_baseline(s.clone()).unwrap();
    }
    if mode == "advance" {
        s.reference.online_generation += 1;
        s.reference.journal_seq += 1;
        s.reference.block.number_or_slot += 1;
        s.pools[0].meteora.as_mut().unwrap().slot += 1;
        s.reference.block.parent_hash = s.reference.block.hash.clone();
        s.reference.block.hash = c.bank.data["blockhash"].as_str().unwrap().into();
        c.api.replica.install_baseline(s.clone()).unwrap();
    }
    Json(json!({"mode":mode}))
}

async fn test_sign(State(c): State<Control>, Json(body): Json<Value>) -> Json<Value> {
    Json(
        execution_bank::wallet_worker(
            &c.bank,
            "sign",
            body["transaction"].as_str().unwrap(),
            "ready",
        )
        .await,
    )
}
async fn wallet_stats(State(c): State<Control>) -> Json<Value> {
    Json(
        json!({"raydiumSendCalls":c.raydium.calls.lock().unwrap().iter().filter(|v|v.as_str()=="sendTransaction").count(),"raydiumTransactionCount":c.raydium.transactions.lock().unwrap().len(),"pumpSendCalls":c.pump.calls.lock().unwrap().iter().filter(|v|v.as_str()=="sendTransaction").count(),"curveSendCalls":c.curve.calls.lock().unwrap().iter().filter(|v|v.as_str()=="sendTransaction").count(),"curveAtaSendCalls":c.curve_ata.calls.lock().unwrap().iter().filter(|v|v.as_str()=="sendTransaction").count(),"fundingSendCalls":c.funding.iter().map(|b|b.calls.lock().unwrap().iter().filter(|v|v.as_str()=="sendTransaction").count()).collect::<Vec<_>>()}),
    )
}
