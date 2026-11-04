#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!(
            "{}",
            serde_json::json!({"timestamp_ms":xone_observability::now_ms(),"service":"dex-router","role":"startup","level":"error","event":"service_start_failed","config_release":"unavailable","config_public_hash":"","fields":{"error_code":e.0}})
        );
        std::process::exit(1);
    }
}
async fn run() -> xone_domain::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 2 && args[1] == "--evm-simulation-worker" {
        return xone_simulation::worker::run_stdio();
    }
    let (path, check) = xone_config::cli_args()?;
    let config = xone_config::load(path, "dex-router")?;
    if check {
        println!(
            "{}",
            serde_json::json!({"service":"dex-router","valid":true,"public_hash":config.public_hash,"economic_hash":config.economic_hash})
        );
        return Ok(());
    }
    xone_quote_api::run(config).await
}
