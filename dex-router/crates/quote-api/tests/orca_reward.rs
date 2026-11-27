use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use xone_domain::{
    hash, meteora, scope_hash, spl_token_2022, BlockRef, Pool, Snapshot, SnapshotRef,
    SCHEMA_VERSION,
};
use xone_execution_preparation::{orca_reward, SolanaRpcConfig};
use xone_quote_api::Api;
use xone_simulation::solana_sbf::Engine as SbfEngine;

const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const GENESIS: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
const AUTH: &str = "xone-orca-reward-test";

#[derive(Clone, Copy)]
enum BlockhashRpc {
    Valid,
    Expired,
    Unknown,
    Stale,
}

#[derive(Clone)]
struct BankRpc {
    case: Arc<Value>,
    blockhash: Arc<Mutex<BlockhashRpc>>,
    validator: Option<String>,
}

impl BankRpc {
    fn new(case: Value) -> Self {
        Self {
            case: Arc::new(case),
            blockhash: Arc::new(Mutex::new(BlockhashRpc::Valid)),
            validator: None,
        }
    }
}

fn api(pool: Pool, rpc: String, blockhash: &str, manifest: &str) -> Api {
    let block = bank_block(&pool, GENESIS, blockhash);
    configured_api(pool, rpc, GENESIS, block, manifest, None)
}

fn bank_block(pool: &Pool, hash: &str, parent_hash: &str) -> BlockRef {
    let state = pool.orca_whirlpool.as_ref().unwrap();
    BlockRef {
        chain_key: pool.chain_key.clone(),
        number_or_slot: state.bank_slot,
        hash: hash.into(),
        parent_hash: parent_hash.into(),
        timestamp_ms: state.bank_unix_timestamp * 1000,
    }
}

fn configured_api(
    pool: Pool,
    rpc: String,
    genesis: &str,
    block: BlockRef,
    manifest: &str,
    receipt_journal: Option<&std::path::Path>,
) -> Api {
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = AUTH.into();
    config.config.api.quote_timeout_ms = 15000;
    config.config.routing.max_state_age_ms = 315360000000;
    config.config.routing.quote_ttl_ms = 60000;
    config.config.api.solana_runtime_manifest = manifest.into();
    if let Some(journal) = receipt_journal {
        config.config.api.transaction_submission_enabled = true;
        config.config.api.transaction_submission_chains = vec![pool.chain_key.clone()];
        config.config.api.transaction_max_pending = 2;
        config.config.api.transaction_receipt_journal = journal.to_str().unwrap().into();
    }
    let chain = &mut config.config.chains[1];
    chain.key = pool.chain_key.clone();
    chain.genesis_hash = genesis.into();
    chain.rpc_urls = vec![rpc];
    let snapshot = Snapshot {
        reference: SnapshotRef {
            block,
            canonical_epoch: 1,
            journal_seq: 1,
            schema_version: SCHEMA_VERSION,
            adapter_set_hash: hash(&[&pool.adapter_version]).unwrap(),
            online_generation: 1,
            scope_hash: scope_hash(std::slice::from_ref(&pool)).unwrap(),
            writer_epoch: 1,
            owner_service: chain.owner_service.clone(),
            owner_assignment_version: chain.assignment_version,
        },
        pools: vec![pool],
        source: "CONTROLLED_ORIGINAL_SBF_EXECUTION_BANK".into(),
        history_gaps: vec![],
    };
    let api = Api::new(config).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    api
}

async fn claim(api: &Api, path: &str, request: &Value) -> (StatusCode, Value) {
    api_call(api, Request::post(path), Body::from(request.to_string())).await
}

async fn api_call(
    api: &Api,
    request: axum::http::request::Builder,
    body: Body,
) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            request
                .header("authorization", format!("Bearer {AUTH}"))
                .header("content-type", "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 64 << 10).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

fn token_account(mint: &str, owner: &str, amount: u64, program: &str, rent: &Value) -> Value {
    let mut data = vec![0; 165];
    data[..32].copy_from_slice(&meteora::pubkey(mint).unwrap());
    data[32..64].copy_from_slice(&meteora::pubkey(owner).unwrap());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1;
    if program == spl_token_2022::PROGRAM {
        data.extend([2, 2, 0, 8, 0]);
        data.extend(13u64.to_le_bytes());
    }
    json!({"owner":program,"executable":false,"lamports":rent[data.len().to_string()],"data":[STANDARD.encode(data),"base64"]})
}

fn sign(preparation: &Value) -> String {
    let python = std::env::var("XONE_SBF_PYTHON").unwrap_or_else(|_| "python3.11".into());
    let signer = std::process::Command::new(python)
        .env("PYTHONPATH", concat!(env!("CARGO_MANIFEST_DIR"), "/../../../integration/artifacts/local/r01-solana-rebuild/python"))
        .arg("-c")
        .arg("import base64,hashlib,sys; from solders.keypair import Keypair; from solders.transaction import Transaction; tx=Transaction.from_bytes(base64.b64decode(sys.argv[1])); key=Keypair.from_seed(hashlib.sha256(b'XONE Orca SwapV2 execution test').digest()); tx.sign([key],tx.message.recent_blockhash); print(base64.b64encode(bytes(tx)).decode())")
        .arg(preparation["unsignedTransaction"].as_str().unwrap())
        .output()
        .unwrap();
    assert!(
        signer.status.success(),
        "{}",
        String::from_utf8_lossy(&signer.stderr)
    );
    String::from_utf8(signer.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
#[ignore = "requires a deployed-program capture generated by orca-token2022-execution-bank.py"]
async fn orca_dual_token2022_wallet_wire_matches_independent_http_simulation() {
    let bank_path = std::env::var("XONE_ORCA_TOKEN2022_BANK").unwrap();
    let manifest = std::env::var("XONE_ORCA_SBF_MANIFEST").unwrap();
    let bank: Value = serde_json::from_slice(&std::fs::read(bank_path).unwrap()).unwrap();
    let ata = |owner: &str, mint: &str| {
        meteora::pda(
            &[
                &meteora::pubkey(owner).unwrap(),
                &meteora::pubkey(spl_token_2022::PROGRAM).unwrap(),
                &meteora::pubkey(mint).unwrap(),
            ],
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        )
        .unwrap()
    };
    for (zero_for_one, exact_in) in [(true, true), (false, false)] {
        let mut case = bank.clone();
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let state = pool.orca_whirlpool.as_ref().unwrap();
        let token_2022 = state.token_2022.as_ref().unwrap();
        for (mint, vault, token) in [
            (
                &pool.token0,
                &state.vault0,
                token_2022.token0.as_ref().unwrap(),
            ),
            (
                &pool.token1,
                &state.vault1,
                token_2022.token1.as_ref().unwrap(),
            ),
        ] {
            for (key, data) in [(mint, &token.mint_data), (vault, &token.vault_data)] {
                case["accounts"][key] = json!({"owner":spl_token_2022::PROGRAM,"executable":false,
                    "lamports":case["rent"]["182"],"data":[STANDARD.encode(hex::decode(data).unwrap()),"base64"]});
            }
        }
        let caller = bank["caller"].as_str().unwrap();
        let recipient = bank["recipient"].as_str().unwrap();
        let (sell, buy) = if zero_for_one {
            (&pool.token0, &pool.token1)
        } else {
            (&pool.token1, &pool.token0)
        };
        case["accounts"][ata(caller, sell)] =
            token_account(sell, caller, 100000, spl_token_2022::PROGRAM, &case["rent"]);
        case["accounts"][ata(recipient, buy)] =
            token_account(buy, recipient, 155, spl_token_2022::PROGRAM, &case["rent"]);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let bank_rpc = BankRpc::new(case);
        let mock = Router::new()
            .route("/", post(rpc))
            .with_state(bank_rpc.clone());
        let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
        let api = api(
            pool.clone(),
            endpoint,
            bank["blockhash"].as_str().unwrap(),
            &manifest,
        );
        let mut request = json!({"chainKey":pool.chain_key,"sellToken":sell,"buyToken":buy,
            "caller":caller,"recipient":recipient,"slippageBps":50,"verification":"FAST"});
        let quote_path = if exact_in {
            request["kind"] = "EXACT_IN".into();
            request["sellAmount"] = "5000".into();
            "/v1/quote"
        } else {
            request["kind"] = "EXACT_OUT".into();
            request["buyAmount"] = "3000".into();
            request["maxSellAmount"] = "100000".into();
            "/v1/quote-exact-out"
        };
        let (status, quote) = claim(&api, quote_path, &request).await;
        assert_eq!(status, 200, "{quote}");
        let id = quote["quoteId"].as_str().unwrap();
        let (status, prep) = claim(&api, &format!("/v1/quotes/{id}/prepare"), &json!({})).await;
        assert_eq!(status, 200, "{prep}");
        assert_eq!(prep["ready"], true, "{prep}");
        let key = prep["preparationHash"].as_str().unwrap();
        let path = format!("/v1/quotes/{id}/preparations/{key}/simulate-independent");
        let (status, unsigned) = claim(&api, &path, &Value::Null).await;
        assert_eq!(status, 200, "{unsigned}");
        assert_eq!(unsigned["signed"], false);
        assert!(unsigned.get("blockhashValidity").is_none());
        let signed_wire = sign(&prep);
        let (status, signed) = claim(
            &api,
            &path,
            &json!({"signedTransaction":signed_wire.trim()}),
        )
        .await;
        assert_eq!(status, 200, "{signed}");
        assert_eq!(signed["signed"], true);
        let slot = prep["costs"]["feeObservedSlot"].as_u64().unwrap();
        assert_eq!(
            signed["blockhashValidity"],
            json!({"authority":"CONFIRMED_RPC_OBSERVATION","recentBlockhash":prep["recentBlockhash"],
                "minimumContextSlot":slot,"observedSlot":slot+1,"blockHeight":1000,"lastValidBlockHeight":1000})
        );
        for key in [
            "actualInput",
            "actualOutput",
            "feeLamports",
            "postAccountsHash",
            "bankSha256",
            "messageAccountLocks",
        ] {
            assert_eq!(signed[key], unsigned[key], "{key}");
        }
        let mut account_metas = std::collections::BTreeMap::from([(caller, true)]);
        for instruction in prep["instructions"].as_array().unwrap() {
            let program = instruction["programId"].as_str().unwrap();
            account_metas.entry(program).or_insert(false);
            for meta in instruction["accounts"].as_array().unwrap() {
                let writable = account_metas
                    .entry(meta["pubkey"].as_str().unwrap())
                    .or_insert(false);
                *writable |= meta["isWritable"].as_bool().unwrap();
            }
        }
        let mut writable = vec![caller];
        let mut readonly = Vec::new();
        for (key, writes) in account_metas {
            if !writes {
                readonly.push(key);
            } else if key != caller {
                writable.push(key);
            }
        }
        assert_eq!(
            signed["messageAccountLocks"],
            json!({"writable":writable,"readonly":readonly})
        );
        let mut tampered = STANDARD.decode(signed_wire.trim()).unwrap();
        tampered[1] ^= 1;
        let (status, error) = claim(
            &api,
            &path,
            &json!({"signedTransaction":STANDARD.encode(tampered)}),
        )
        .await;
        assert_ne!(status, 200);
        assert_eq!(error["error"], "WALLET_SIGNATURE_INVALID", "{error}");
        for (mode, expected) in [
            (BlockhashRpc::Expired, "PREPARATION_BLOCKHASH_EXPIRED"),
            (BlockhashRpc::Unknown, "PREPARATION_BLOCKHASH_EXPIRED"),
            (BlockhashRpc::Stale, "PREPARATION_STALE_RPC"),
        ] {
            *bank_rpc.blockhash.lock().unwrap() = mode;
            let (status, error) = claim(
                &api,
                &path,
                &json!({"signedTransaction":signed_wire.trim()}),
            )
            .await;
            assert_ne!(status, 200);
            assert_eq!(error["error"], expected, "{error}");
        }
        server.abort();
    }
}

async fn rpc(State(bank): State<BankRpc>, Json(call): Json<Value>) -> Json<Value> {
    let case = &bank.case;
    let params = &call["params"];
    let slot = case["pool"]["orca_whirlpool"]["bank_slot"]
        .as_u64()
        .unwrap();
    let blockhash = *bank.blockhash.lock().unwrap();
    let method = call["method"].as_str().unwrap();
    if let Some(url) = &bank.validator {
        if method != "getMultipleAccounts" || params[1]["commitment"] != "finalized" {
            let mut response = validator_rpc(url, &call).await;
            if method == "getFeeForMessage"
                && params[1]["commitment"] == "confirmed"
                && matches!(blockhash, BlockhashRpc::Stale)
            {
                let floor = params[1]["minContextSlot"].as_u64().unwrap();
                response["result"]["context"]["slot"] = (floor - 1).into();
            }
            if method == "getSignatureStatuses" && matches!(blockhash, BlockhashRpc::Stale) {
                response["result"]["context"]["slot"] = 0.into();
            }
            return Json(response);
        }
    }
    let result = match method {
        "getGenesisHash" => json!(GENESIS),
        "getBlockHeight" => {
            assert_eq!(params[0]["minContextSlot"], slot);
            json!(if matches!(blockhash, BlockhashRpc::Expired) {
                1001
            } else {
                1000
            })
        }
        "isBlockhashValid" => {
            assert_eq!(params[0], case["blockhash"]);
            assert_eq!(params[1]["minContextSlot"], slot);
            let observed_slot = if matches!(blockhash, BlockhashRpc::Stale) {
                slot - 1
            } else {
                slot + 1
            };
            json!({"context":{"slot":observed_slot},"value":!matches!(blockhash, BlockhashRpc::Unknown)})
        }
        "getMultipleAccounts" => {
            let values: Vec<_> = params[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| &case["accounts"][key.as_str().unwrap()])
                .collect();
            json!({"context":{"slot":slot},"value":values})
        }
        "getLatestBlockhash" => {
            json!({"context":{"slot":slot},"value":{"blockhash":case["blockhash"],"lastValidBlockHeight":1000}})
        }
        "getFeeForMessage" => json!({"context":{"slot":slot},"value":5000}),
        "getMinimumBalanceForRentExemption" => case["rent"][params[0].to_string()].clone(),
        other => panic!("unexpected RPC method {other}"),
    };
    Json(json!({"jsonrpc":"2.0","id":call["id"],"result":result}))
}

async fn validator_rpc(url: &str, call: &Value) -> Value {
    reqwest::Client::new()
        .post(url)
        .json(call)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires a local Agave validator seeded with the Orca classic SPL bank"]
async fn orca_signed_swap_finalizes_on_agave_and_matches_sbf() {
    let validator = std::env::var("XONE_ORCA_VALIDATOR_URL").unwrap();
    let url = reqwest::Url::parse(&validator).unwrap();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let path = std::env::var("XONE_ORCA_VALIDATOR_BANK").unwrap();
    let case: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let manifest = std::env::var("XONE_ORCA_VALIDATOR_MANIFEST").unwrap();
    let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
    let journal = tempfile::tempdir().unwrap();
    let mut bank_rpc = BankRpc::new(case);
    bank_rpc.validator = Some(validator.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let mock = Router::new()
        .route("/", post(rpc))
        .with_state(bank_rpc.clone());
    let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let case = &bank_rpc.case;
    let genesis = case["genesis"].as_str().unwrap();
    let head = bank_block(
        &pool,
        case["blockhash"].as_str().unwrap(),
        case["previousBlockhash"].as_str().unwrap(),
    );
    let journal_path = journal.path().join("receipts.ndjson");
    let api = configured_api(
        pool.clone(),
        endpoint.clone(),
        genesis,
        head.clone(),
        &manifest,
        Some(&journal_path),
    );
    let (status, quote) = claim(
        &api,
        "/v1/quote",
        &json!({
            "chainKey":pool.chain_key,"sellToken":pool.token0,"buyToken":pool.token1,
            "caller":case["caller"],"recipient":case["recipient"],"sellAmount":"5000",
            "slippageBps":50,"verification":"FAST","kind":"EXACT_IN"
        }),
    )
    .await;
    assert_eq!(status, 200, "{quote}");
    let id = quote["quoteId"].as_str().unwrap();
    let (status, prep) = claim(&api, &format!("/v1/quotes/{id}/prepare"), &json!({})).await;
    assert_eq!(status, 200, "{prep}");
    assert_eq!(prep["ready"], true, "{prep}");
    let key = prep["preparationHash"].as_str().unwrap();
    let prefix = format!("/v1/quotes/{id}/preparations/{key}");
    let simulation = format!("{prefix}/simulate-independent");
    let (status, unsigned) = claim(&api, &simulation, &Value::Null).await;
    assert_eq!(status, 200, "{unsigned}");
    let wire = sign(&prep);
    let (status, signed) = claim(&api, &simulation, &json!({"signedTransaction":wire})).await;
    assert_eq!(status, 200, "{signed}");
    let submit = format!("{prefix}/submit");
    let mut payload = json!({
        "signedTransaction":wire,"evidenceHash":signed["evidenceHash"],"confirm":true
    });
    let (status, rejected) = claim(&api, &submit, &payload).await;
    assert_ne!(status, 200);
    assert_eq!(rejected["error"], "INDEPENDENT_SIMULATION_REQUIRED");
    payload["evidenceHash"] = unsigned["evidenceHash"].clone();
    *bank_rpc.blockhash.lock().unwrap() = BlockhashRpc::Stale;
    let (status, rejected) = claim(&api, &submit, &payload).await;
    assert_ne!(status, 200);
    assert_eq!(rejected["error"], "PREPARATION_STALE_RPC");
    assert_eq!(std::fs::metadata(&journal_path).unwrap().len(), 0);
    *bank_rpc.blockhash.lock().unwrap() = BlockhashRpc::Valid;
    let (status, submission) = claim(&api, &submit, &payload).await;
    assert_eq!(status, 200, "{submission}");
    assert_eq!(submission["state"], "SUBMITTED", "{submission}");
    assert_eq!(submission["signature"], signed["walletSignature"]);
    let fee = &submission["feePreflight"];
    let slots = [
        &fee["preparationMinimumContextSlot"],
        &fee["blockhashValidity"]["observedSlot"],
        &fee["feeObservedSlot"],
        &fee["accountObservedSlot"],
        &fee["simulationObservedSlot"],
        &fee["minimumSendContextSlot"],
    ];
    assert!(slots
        .windows(2)
        .all(|pair| pair[0].as_u64().unwrap() <= pair[1].as_u64().unwrap()));
    let path = format!(
        "/v1/transactions/{}",
        submission["signature"].as_str().unwrap()
    );
    let journal_before = std::fs::read(&journal_path).unwrap();
    *bank_rpc.blockhash.lock().unwrap() = BlockhashRpc::Stale;
    let (status, rejection) = api_call(&api, Request::get(&path), Body::empty()).await;
    assert_ne!(status, 200);
    assert_eq!(rejection["error"], "RECEIPT_STALE_RPC");
    assert_eq!(std::fs::read(&journal_path).unwrap(), journal_before);
    *bank_rpc.blockhash.lock().unwrap() = BlockhashRpc::Valid;
    let receipt = tokio::time::timeout(std::time::Duration::from_secs(45), async {
        loop {
            let (status, receipt) = api_call(&api, Request::get(&path), Body::empty()).await;
            assert_eq!(status, 200, "{receipt}");
            match receipt["state"].as_str().unwrap() {
                "FINALIZED" => break receipt,
                "UNKNOWN" | "PROCESSED" | "CONFIRMED" | "RECEIPT_PENDING" => {}
                _ => panic!("{receipt}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    })
    .await
    .unwrap();
    assert!(receipt["slot"].as_u64().unwrap() >= fee["minimumSendContextSlot"].as_u64().unwrap());
    for key in [
        "actualInput",
        "actualOutput",
        "feeLamports",
        "payerLamportDelta",
        "transactionSha256",
    ] {
        assert_eq!(receipt["settlement"][key], signed[key], "{key}: {receipt}");
    }
    let block = validator_rpc(
        &validator,
        &json!({"jsonrpc":"2.0","id":1,"method":"getBlock",
            "params":[receipt["slot"],{"commitment":"finalized","transactionDetails":"signatures"}]
        }),
    )
    .await;
    assert!(
        block["result"]["signatures"]
            .as_array()
            .unwrap()
            .contains(&receipt["signature"]),
        "{block}"
    );
    drop(api);
    let reopened = configured_api(
        pool,
        endpoint,
        genesis,
        head,
        &manifest,
        Some(&journal_path),
    );
    let (status, recovered) = api_call(&reopened, Request::get(&path), Body::empty()).await;
    assert_eq!(status, 200, "{recovered}");
    assert_eq!(recovered, receipt);
    server.abort();
}

#[tokio::test]
#[ignore = "requires a deployed-program capture generated by orca-token2022-execution-bank.py"]
async fn orca_reward_preparation_and_deployed_program_cpi() {
    replay_rewards(false).await;
}

#[tokio::test]
#[ignore = "requires a deployed-program capture generated by orca-token2022-execution-bank.py"]
async fn orca_reward_update_before_collection_and_deployed_program_cpi() {
    replay_rewards(true).await;
}

async fn replay_rewards(update: bool) {
    let bank_path = std::env::var("XONE_ORCA_TOKEN2022_BANK").unwrap();
    let manifest = std::env::var("XONE_ORCA_SBF_MANIFEST").unwrap();
    let bank: Value = serde_json::from_slice(&std::fs::read(bank_path).unwrap()).unwrap();
    let caller = bank["caller"].as_str().unwrap();
    let recipient = bank["recipient"].as_str().unwrap();
    let key = |label: &str| meteora::key(&Sha256::digest(label.as_bytes()), 0);
    let position_mint = key("Orca claim fixture position mint");
    let position = meteora::pda(
        &[b"position", &meteora::pubkey(&position_mint).unwrap()],
        xone_domain::orca::PROGRAM_ID,
    )
    .unwrap();
    let nft = key("Orca claim fixture position token account");
    let invalid_delegate = key("Orca claim fixture invalid delegate account");
    let vault = key("Orca claim fixture reward vault");
    let destination = key("Orca claim fixture reward recipient");
    let data = |case: &Value, address: &str| {
        STANDARD
            .decode(case["accounts"][address]["data"][0].as_str().unwrap())
            .unwrap()
    };
    for program in [TOKEN, spl_token_2022::PROGRAM] {
        for (index, balance, delegated) in [(0, 2000, false), (1, 100, true), (2, 0, true)] {
            let mut case = bank.clone();
            let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
            let mint = &pool.token0;
            let state = pool.orca_whirlpool.as_mut().unwrap();
            let reward = &mut state.reward_infos[index];
            reward.mint = mint.clone();
            reward.vault = vault.clone();
            let growth = 4u128 << 64;
            let emissions = 125_000u128 << 64;
            if update {
                reward.growth_global_x64 = growth.to_string().parse().unwrap();
                reward.emissions_per_second_x64 = emissions.to_string().parse().unwrap();
            }
            if program == spl_token_2022::PROGRAM {
                let mint_data = &state
                    .token_2022
                    .as_ref()
                    .unwrap()
                    .token0
                    .as_ref()
                    .unwrap()
                    .mint_data;
                case["accounts"][mint] = json!({"owner":program,"executable":false,"lamports":case["rent"]["182"],"data":[STANDARD.encode(hex::decode(mint_data).unwrap()),"base64"]});
            }
            let mut pool_data = data(&case, &pool.pool_id);
            let start = 269 + 128 * index;
            pool_data[start..start + 32].copy_from_slice(&meteora::pubkey(mint).unwrap());
            pool_data[start + 32..start + 64].copy_from_slice(&meteora::pubkey(&vault).unwrap());
            if update {
                pool_data[start + 96..start + 112].copy_from_slice(&emissions.to_le_bytes());
                pool_data[start + 112..start + 128].copy_from_slice(&growth.to_le_bytes());
                if program == TOKEN {
                    // Exercise both Orca tick array layouts with the same initial tick state.
                    let lower = &mut state.tick_arrays[0];
                    let dynamic = data(&case, &lower.address);
                    let bitmap = u128::from_le_bytes(dynamic[44..60].try_into().unwrap());
                    let mut fixed = meteora::discriminator("TickArray").to_vec();
                    fixed.extend_from_slice(&dynamic[8..12]);
                    let mut offset = 60;
                    for index in 0..88 {
                        if bitmap & (1u128 << index) != 0 {
                            fixed.extend_from_slice(&dynamic[offset..offset + 113]);
                            offset += 113;
                        } else {
                            fixed.extend_from_slice(&[0; 113]);
                            offset += 1;
                        }
                    }
                    fixed.extend_from_slice(&meteora::pubkey(&pool.pool_id).unwrap());
                    lower.layout = "fixed".into();
                    case["accounts"][&lower.address]["data"][0] = STANDARD.encode(fixed).into();
                    case["accounts"][&lower.address]["lamports"] = json!(100_000_000);
                }
                // The unallocated third array stays in the quote view but is not an update account.
                let sparse = state.tick_arrays.last_mut().unwrap();
                assert!(sparse.ticks.is_empty());
                sparse.layout = "sparse".into();
                case["accounts"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&sparse.address);
            }
            case["accounts"][&pool.pool_id]["data"][0] = STANDARD.encode(pool_data).into();
            case["accounts"][&vault] =
                token_account(mint, &pool.pool_id, balance, program, &case["rent"]);
            case["accounts"][&destination] =
                token_account(mint, recipient, 200, program, &case["rent"]);
            case["accounts"][recipient] = case["accounts"][caller].clone();
            let mut position_data = vec![0; 216];
            position_data[..8].copy_from_slice(&meteora::discriminator("Position"));
            position_data[8..40].copy_from_slice(&meteora::pubkey(&pool.pool_id).unwrap());
            position_data[40..72].copy_from_slice(&meteora::pubkey(&position_mint).unwrap());
            if update {
                position_data[72..88].copy_from_slice(&100u128.to_le_bytes());
                position_data[88..92].copy_from_slice(&(-64i32).to_le_bytes());
                position_data[92..96].copy_from_slice(&64i32.to_le_bytes());
            }
            let owed = 1000u64;
            let offset = 160 + 24 * index;
            position_data[offset..offset + 8].copy_from_slice(&owed.to_le_bytes());
            case["accounts"][&position] = json!({"owner":xone_domain::orca::PROGRAM_ID,"executable":false,"lamports":10_000_000,"data":[STANDARD.encode(position_data),"base64"]});
            case["accounts"][&nft] = token_account(
                &position_mint,
                if delegated { recipient } else { caller },
                1,
                TOKEN,
                &case["rent"],
            );
            if delegated {
                let mut account = data(&case, &nft);
                account[72..76].copy_from_slice(&1u32.to_le_bytes());
                account[76..108].copy_from_slice(&meteora::pubkey(caller).unwrap());
                account[121..129].copy_from_slice(&1u64.to_le_bytes());
                case["accounts"][&nft]["data"][0] = STANDARD.encode(&account).into();
                let mut invalid = case["accounts"][&nft].clone();
                account[121..129].fill(0);
                invalid["data"][0] = STANDARD.encode(account).into();
                case["accounts"][&invalid_delegate] = invalid;
            }
            case["pool"] = serde_json::to_value(&pool).unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let app = Router::new()
                .route("/", post(rpc))
                .with_state(BankRpc::new(case));
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let path = format!(
                "/v1/chains/{}/pools/{}/prepare-orca-reward",
                pool.chain_key, pool.pool_id
            );
            let api = api(
                pool.clone(),
                url.clone(),
                bank["blockhash"].as_str().unwrap(),
                &manifest,
            );
            let mut request = json!({"simulate":true,"claim":{
                "authority":caller,"position":position,"positionTokenAccount":nft,
                "recipient":recipient,"rewardAccount":destination,"rewardIndex":index,
                "rewardTokenProgram":program,
            }});
            if update {
                request["claim"]["updateBeforeCollect"] = true.into();
            }
            let (status, result) = claim(&api, &path, &request).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "program={program} index={index}: {result}"
            );
            let prepared = &result["preparation"];
            let actual = &result["independentSimulation"];
            let accrued = if update { 537 } else { 0 };
            let owed = owed + accrued;
            let gross = owed.min(balance);
            let fee = if program == TOKEN {
                0
            } else {
                gross.div_ceil(100).min(20)
            };
            assert_eq!(prepared["grossReward"], gross.to_string());
            assert_eq!(prepared["netReward"], (gross - fee).to_string());
            assert_eq!(prepared["remainingRewardOwed"], (owed - gross).to_string());
            assert_eq!(
                actual["success"], true,
                "program={program} index={index}: {actual}"
            );
            assert_eq!(actual["transferFee"], fee.to_string());
            assert_eq!(actual["sourceBankUnchanged"], true);
            assert_eq!(
                actual["bankSha256"],
                prepared["accountSnapshot"]["snapshotHash"]
            );
            assert_eq!(actual["postAccounts"], prepared["expectedPostAccounts"]);
            assert_eq!(actual["preparationHash"], prepared["preparationHash"]);
            if update {
                let arrays = &pool.orca_whirlpool.as_ref().unwrap().tick_arrays;
                assert_eq!(
                    prepared["rewardUpdate"],
                    json!({
                        "lowerTickArray":arrays[0].address,"upperTickArray":arrays[1].address,
                        "accruedReward":accrued.to_string()
                    })
                );
                assert_eq!(prepared["expectedPostAccounts"][3]["address"], pool.pool_id);
            }
            for (field, value, error) in [
                ("rewardIndex", json!(3), "ORCA_REWARD_NOT_INITIALIZED"),
                (
                    "recipient",
                    json!(caller),
                    "ORCA_REWARD_DESTINATION_INVALID",
                ),
                (
                    "positionTokenAccount",
                    json!(vault),
                    "ORCA_REWARD_POSITION_INVALID",
                ),
            ] {
                let mut invalid = request.clone();
                invalid["claim"][field] = value;
                assert_eq!(
                    claim(&api, &path, &invalid).await,
                    (StatusCode::BAD_REQUEST, json!({"error":error}))
                );
            }
            let mut invalid = request.clone();
            if delegated {
                invalid["claim"]["positionTokenAccount"] = invalid_delegate.clone().into();
            } else {
                invalid["claim"]["authority"] = recipient.into();
            }
            assert_eq!(
                claim(&api, &path, &invalid).await,
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error":"ORCA_REWARD_AUTHORITY_INVALID"})
                )
            );
            if index == 0 {
                let original = orca_reward::prepare(
                    &SolanaRpcConfig {
                        url,
                        genesis_hash: GENESIS.into(),
                    },
                    &pool,
                    &serde_json::from_value(request["claim"].clone()).unwrap(),
                )
                .await
                .unwrap();
                let mut capture = serde_json::to_value(original.accounts.unwrap()).unwrap();
                // Freeze after preparation so the original token CPI must reject the transfer.
                let entry = &mut capture["accounts"][&destination]["data"][0];
                let mut bytes = STANDARD.decode(entry.as_str().unwrap()).unwrap();
                bytes[108] = 2;
                *entry = STANDARD.encode(bytes).into();
                let mut prepared = original.result;
                prepared["accountSnapshot"]["snapshotHash"] = hash(&capture).unwrap().into();
                prepared.as_object_mut().unwrap().remove("preparationHash");
                prepared["preparationHash"] = hash(&prepared).unwrap().into();
                let mint = prepared["rewardMint"].as_str().unwrap();
                let engine = SbfEngine::load(std::path::Path::new(&manifest)).unwrap();
                let error = engine
                    .simulate_captured(&pool, caller, recipient, mint, mint, &prepared, &capture)
                    .await
                    .unwrap_err();
                assert_eq!(error.0, "SBF_TRANSACTION_FAILED");
            }
            server.abort();
        }
    }
}
