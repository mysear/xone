use serde_json::json;
use xone_config::Chain;
use xone_solana_ingest::rpc::Rpc;

#[tokio::test]
async fn creator_profile_requires_finalized_even_before_provider_selection() {
    let chain: Chain = serde_json::from_value(json!({
        "key": "solana:mainnet", "owner_service": "test-parser", "assignment_version": 1,
        "genesis_hash": "genesis", "rpc_urls": [], "auto_reorg_window": 32,
        "confirmation_tag": "confirmed", "pools": [{
            "id": "pool", "protocol": "raydium-cpmm-creator", "token0": "mint0",
            "token1": "mint1", "fee_numerator": 2500, "fee_denominator": 1000000,
            "program_id": "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C", "accounts": []
        }]
    }))
    .unwrap();
    let rpc = Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    let error = rpc.current_baseline(&chain).await.unwrap_err();
    assert_eq!(error.0, "RAYDIUM_REQUIRES_FINALIZED");
}
