use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, BlockRef, Pool, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_quote_api::Api;

const AUTH: &str = "rayclmm-orders-test";

fn install(
    config: xone_config::ConfigSnapshot,
    pool: &Pool,
    hash_value: &str,
    parent: &str,
) -> Api {
    let chain = &config.config.chains[1];
    let state = pool.raydium_clmm.as_ref().unwrap();
    assert_eq!(chain.key, pool.chain_key);
    let snapshot = Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: pool.chain_key.clone(),
                number_or_slot: state.bank_slot,
                hash: hash_value.into(),
                parent_hash: parent.into(),
                timestamp_ms: state.bank_unix_timestamp * 1000,
            },
            canonical_epoch: 1,
            journal_seq: 1,
            schema_version: SCHEMA_VERSION,
            adapter_set_hash: hash(&[&pool.adapter_version]).unwrap(),
            online_generation: 1,
            scope_hash: scope_hash(std::slice::from_ref(pool)).unwrap(),
            writer_epoch: 1,
            owner_service: chain.owner_service.clone(),
            owner_assignment_version: chain.assignment_version,
        },
        pools: vec![pool.clone()],
        source: "CONTROLLED_ORIGINAL_RAYCLMM_LIMIT_ORDERS".into(),
        history_gaps: vec![],
    };
    let api = Api::new(config).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    api
}

async fn call(api: &Api, endpoint: &str, request: &Value) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::post(endpoint)
                .header("authorization", format!("Bearer {AUTH}"))
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status(),
        serde_json::from_slice(&to_bytes(response.into_body(), 128 << 10).await.unwrap()).unwrap(),
    )
}

fn quote_request(
    pool: &Pool,
    case: &Value,
    caller: &Value,
    recipient: &Value,
) -> (&'static str, Value) {
    let (sell, buy) = if case["left"] == true {
        (&pool.token0, &pool.token1)
    } else {
        (&pool.token1, &pool.token0)
    };
    let mut request = json!({"chainKey":pool.chain_key,"sellToken":sell,"buyToken":buy,
        "caller":caller,"recipient":recipient,"slippageBps":0,"verification":"FAST"});
    let endpoint = if case["exact_in"] == true {
        request["kind"] = "EXACT_IN".into();
        request["sellAmount"] = case["amount"].clone();
        "/v1/quote"
    } else {
        request["kind"] = "EXACT_OUT".into();
        request["buyAmount"] = case["amount"].clone();
        request["maxSellAmount"] = "100000".into();
        "/v1/quote-exact-out"
    };
    (endpoint, request)
}

#[tokio::test]
#[ignore = "requires the pinned native Raydium generate-orders corpus"]
async fn http_quotes_match_both_directions_amount_modes_and_fee_tokens() {
    let path = std::env::var("XONE_RAYCLMM_ORDER_CORPUS").unwrap();
    let corpus: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut tested = 0;
    for case in corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["order_only"] == true && case["amount"] == "2100")
    {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let mut config = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        config.token = AUTH.into();
        config.config.routing.max_splits = 1;
        let caller = json!("9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso");
        let recipient = json!("4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE");
        let api = install(
            config,
            &p,
            caller.as_str().unwrap(),
            recipient.as_str().unwrap(),
        );
        let (endpoint, request) = quote_request(&p, case, &caller, &recipient);
        let input_field = if case["exact_in"] == true {
            "consumedIn"
        } else {
            "requiredSellAmount"
        };
        let (status, body) = call(&api, endpoint, &request).await;
        assert_eq!(status, StatusCode::OK, "{request}: {body}");
        assert_eq!(body[input_field], case["expected"]["consumed"]);
        assert_eq!(body["grossBuyAmount"], case["expected"]["output"]);
        tested += 1;
    }
    assert_eq!(tested, 12);
}

struct RpcBank {
    accounts: Mutex<Value>,
    slot: u64,
    genesis: Value,
    blockhash: Value,
}

async fn rpc(State(bank): State<Arc<RpcBank>>, Json(call): Json<Value>) -> Json<Value> {
    let slot = bank.slot;
    let params = &call["params"];
    let result = match call["method"].as_str().unwrap() {
        "getGenesisHash" => bank.genesis.clone(),
        "getMultipleAccounts" => {
            let accounts = bank.accounts.lock().unwrap();
            let values: Vec<_> = params[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| &accounts[key.as_str().unwrap()])
                .collect();
            json!({"context":{"slot":slot},"value":values})
        }
        "getLatestBlockhash" => json!({"context":{"slot":slot},"value":{
            "blockhash":bank.blockhash,"lastValidBlockHeight":999999999}}),
        "getFeeForMessage" => json!({"context":{"slot":slot},"value":5000}),
        method => panic!("unexpected RPC method {method}"),
    };
    Json(json!({"jsonrpc":"2.0","id":call["id"],"result":result}))
}

async fn deployed_api(
    bank: &Value,
    case: &Value,
    pool: &Pool,
    manifest: &str,
) -> (Api, Arc<RpcBank>, tokio::task::JoinHandle<()>) {
    let mut accounts = case["accounts"].clone();
    accounts
        .as_object_mut()
        .unwrap()
        .extend(bank["shared_accounts"].as_object().unwrap().clone());
    let rpc_bank = Arc::new(RpcBank {
        accounts: Mutex::new(accounts),
        slot: pool.raydium_clmm.as_ref().unwrap().bank_slot,
        genesis: bank["genesis"].clone(),
        blockhash: bank["blockhash"].clone(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = Router::new()
        .route("/", post(rpc))
        .with_state(rpc_bank.clone());
    let task = tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = AUTH.into();
    config.config.routing.max_splits = 1;
    config.config.routing.max_state_age_ms = 315360000000;
    config.config.chains[1].key = pool.chain_key.clone();
    config.config.chains[1].rpc_urls = vec![format!("http://{address}")];
    config.config.chains[1].genesis_hash = bank["genesis"].as_str().unwrap().into();
    config.config.api.solana_runtime_manifest = manifest.into();
    let api = install(
        config,
        pool,
        bank["genesis"].as_str().unwrap(),
        bank["blockhash"].as_str().unwrap(),
    );
    (api, rpc_bank, task)
}

fn run_worker(manifest: &Value, input: &Value) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(manifest["python_executable"]["path"].as_str().unwrap())
        .args(["-I", "-S", "-B"])
        .arg(root.join("../simulation/workers/solana_sbf.py"))
        .arg(manifest["runtime_directory"].as_str().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(input).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
#[ignore = "requires the controlled deployed Raydium bank and pinned SBF runtime"]
async fn http_preparation_and_independent_deployed_swap_v2() {
    let bank: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_RAYCLMM_ORDER_BANK").unwrap()).unwrap(),
    )
    .unwrap();
    let manifest = std::env::var("XONE_RAYCLMM_ORDER_MANIFEST").unwrap();
    let runtime: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    for case in bank["cases"].as_array().unwrap() {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let state = pool.raydium_clmm.as_ref().unwrap();
        let (api, rpc_bank, task) = deployed_api(&bank, case, &pool, &manifest).await;
        let (endpoint, request) = quote_request(&pool, case, &bank["caller"], &bank["recipient"]);
        let (status, quote) = call(&api, endpoint, &request).await;
        assert_eq!(status, StatusCode::OK, "{quote}");
        let id = quote["quoteId"].as_str().unwrap();
        let prepare_path = format!("/v1/quotes/{id}/prepare");
        let options = json!({"sourceTokenAccount":bank["source"],"destinationTokenAccount":bank["destination"]});
        let (status, mut prepared) = call(&api, &prepare_path, &options).await;
        assert_eq!(status, StatusCode::OK, "{prepared}");
        assert_eq!(prepared["ready"], true, "{prepared}");
        let swap = &prepared["instructions"][1];
        let historical = if case["left"] == true {
            state.tick_arrays.first().unwrap()
        } else {
            state.tick_arrays.last().unwrap()
        };
        assert!(swap["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["pubkey"] != historical.address));
        let order_array = state
            .tick_arrays
            .iter()
            .find(|a| a.start_tick_index == 0)
            .unwrap();
        let independent_path = format!(
            "/v1/quotes/{id}/preparations/{}/simulate-independent",
            prepared["preparationHash"].as_str().unwrap()
        );
        let (status, independent) = call(&api, &independent_path, &Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{independent}");
        assert_eq!(independent["actualInput"], case["expected"]["consumed"]);
        assert_eq!(independent["actualOutput"], case["expected"]["output"]);
        assert_eq!(
            independent["bankSha256"],
            prepared["accountSnapshot"]["snapshotHash"]
        );
        assert_eq!(independent["sourceBankUnchanged"], true);
        for (address, hash) in case["post_hashes"].as_object().unwrap() {
            let post = independent["postAccounts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|post| post["address"] == *address)
                .unwrap();
            assert_eq!(&post["dataSha256"], hash, "post-state for {address}");
        }
        let data = STANDARD
            .decode(swap["dataBase64"].as_str().unwrap())
            .unwrap();
        let mut packet = STANDARD
            .decode(prepared["unsignedTransaction"].as_str().unwrap())
            .unwrap();
        let message = &mut packet[65..];
        let offset = message
            .windows(data.len())
            .position(|part| part == data)
            .unwrap();
        let exact_in = case["exact_in"] == true;
        let bad_limit = if exact_in {
            case["expected"]["output"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                + 1
        } else {
            case["expected"]["consumed"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                - 1
        };
        message[offset + 16..offset + 24].copy_from_slice(&bad_limit.to_le_bytes());
        prepared["messageBase64"] = STANDARD.encode(message).into();
        prepared["unsignedTransaction"] = STANDARD.encode(packet).into();
        let failed = run_worker(
            &runtime,
            &json!({
                "protocol":"xone-local-sbf/1",
                "bank":{"pool":pool,"accounts":rpc_bank.accounts.lock().unwrap().clone(),
                    "absentAccounts":[],"rpcCapture":true},
                "preparation":prepared,"caller":bank["caller"],"recipient":bank["recipient"],
                "sellToken":request["sellToken"],"buyToken":request["buyToken"],
                "maxComputeUnits":1_400_000
            }),
        );
        assert_eq!(failed["error"], "SBF_TRANSACTION_FAILED", "{failed}");
        let expected_error = if exact_in { 6018 } else { 6019 };
        assert_eq!(
            failed["nativeError"],
            json!({"InstructionError":[1,{"Custom":expected_error}]})
        );
        assert_eq!(failed["sourceBankUnchanged"], true);
        {
            let mut accounts = rpc_bank.accounts.lock().unwrap();
            let data = &mut accounts[&order_array.address]["data"][0];
            let mut bytes = STANDARD.decode(data.as_str().unwrap()).unwrap();
            bytes[44 + 124] ^= 1;
            *data = STANDARD.encode(bytes).into();
        }
        let (status, rejected) = call(&api, &prepare_path, &options).await;
        assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
        assert_eq!(rejected["error"], "QUOTE_STATE_CHANGED");
        task.abort();
    }
}

#[tokio::test]
#[ignore = "requires the controlled deployed Raydium reward bank and pinned SBF runtime"]
async fn http_reward_settlement_and_independent_deployed_decrease_v2() {
    let bank: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_RAYCLMM_REWARD_BANK").unwrap()).unwrap(),
    )
    .unwrap();
    let manifest = std::env::var("XONE_RAYCLMM_ORDER_MANIFEST").unwrap();
    let runtime: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    for case in bank["cases"].as_array().unwrap() {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let (api, rpc_bank, task) = deployed_api(&bank, case, &pool, &manifest).await;
        let path = format!(
            "/v1/chains/{}/pools/{}/prepare-rayclmm-reward",
            pool.chain_key, pool.pool_id
        );
        let request = json!({"claim":case["claim"],"simulate":true});
        let (status, result) = call(&api, &path, &request).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        let prepared = &result["preparation"];
        let independent = &result["independentSimulation"];
        for name in ["fees", "rewards"] {
            let actual = independent[name].as_array().unwrap();
            let expected = case["expected"][name].as_array().unwrap();
            assert_eq!(actual.len(), expected.len());
            for (transfer, amount) in actual.iter().zip(expected) {
                assert_eq!(transfer["gross"], amount.to_string());
            }
        }
        assert_eq!(
            independent["rewards"][1]["transferFee"],
            case["expected"]["fee_on_reward1"].to_string()
        );
        assert_eq!(
            independent["bankSha256"],
            prepared["accountSnapshot"]["snapshotHash"]
        );
        assert_eq!(independent["sourceBankUnchanged"], true);
        assert_eq!(
            independent["postAccounts"],
            prepared["expectedPostAccounts"]
        );
        let mut invalid = request.clone();
        invalid["claim"]["rewardAccounts"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        let (status, _) = call(&api, &path, &invalid).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let mut invalid = request.clone();
        invalid["claim"]["positionTokenAccount"] = case["claim"]["feeAccounts"][0].clone();
        let (status, rejected) = call(&api, &path, &invalid).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{rejected}");
        assert_eq!(rejected["error"], "RAYCLMM_POSITION_INVALID");
        if case["expected"]["fee_on_reward1"] != 0 {
            let mut accounts = rpc_bank.accounts.lock().unwrap().clone();
            let destination = case["claim"]["rewardAccounts"][1].as_str().unwrap();
            let data = &mut accounts[destination]["data"][0];
            let mut bytes = STANDARD.decode(data.as_str().unwrap()).unwrap();
            bytes[108] = 2;
            *data = STANDARD.encode(bytes).into();
            let failed = run_worker(
                &runtime,
                &json!({
                    "protocol":"xone-local-sbf/1",
                    "bank":{"pool":pool,"accounts":accounts,"absentAccounts":[],"rpcCapture":true},
                    "preparation":prepared,"caller":bank["caller"],"recipient":bank["recipient"],
                    "maxComputeUnits":1_400_000
                }),
            );
            assert_eq!(failed["error"], "SBF_TRANSACTION_FAILED", "{failed}");
            assert_eq!(
                failed["nativeError"],
                json!({"InstructionError":[1,{"Custom":17}]})
            );
            assert_eq!(failed["sourceBankUnchanged"], true);
        }
        task.abort();
    }
}
