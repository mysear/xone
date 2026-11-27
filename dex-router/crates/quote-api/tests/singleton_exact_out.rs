use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, BlockRef, Pool, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_quote_api::Api;
const MAX: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/singleton-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn setup() -> (Api, Snapshot) {
    let mut c = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    c.token = "singleton-original-output-regression".into();
    c.config.routing.max_state_age_ms = 315360000000;
    c.config.routing.max_hops = 3;
    c.config.routing.max_splits = 2;
    c.config.routing.split_parts = 4;
    c.config.routing.max_candidates = 16;
    c.config.routing.max_expansions = 10000;
    c.config.api.requests_per_minute = 10000;
    c.config.api.quote_timeout_ms = 30000;
    let chain = c
        .config
        .chains
        .iter()
        .find(|chain| chain.key == "evm:31337")
        .unwrap();
    let snapshot = Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: chain.key.clone(),
                number_or_slot: 100,
                hash: format!("0x{}", "ab".repeat(32)),
                parent_hash: format!("0x{}", "cd".repeat(32)),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            },
            canonical_epoch: 1,
            journal_seq: 1,
            schema_version: SCHEMA_VERSION,
            adapter_set_hash: String::new(),
            online_generation: 1,
            scope_hash: String::new(),
            writer_epoch: 1,
            owner_service: chain.owner_service.clone(),
            owner_assignment_version: chain.assignment_version,
        },
        pools: Vec::new(),
        source: "CONTROLLED_SINGLETON_OUTPUT_CORE".into(),
        history_gaps: Vec::new(),
    };
    let api = Api::new(c).unwrap();
    (api, snapshot)
}
fn install(api: &Api, s: &mut Snapshot, pools: Vec<Pool>) {
    s.reference.online_generation += 1;
    s.reference.journal_seq += 1;
    s.reference.scope_hash = scope_hash(&pools).unwrap();
    s.reference.adapter_set_hash = hash(
        &pools
            .iter()
            .map(|p| p.adapter_version.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    s.pools = pools;
    s.source = "CONTROLLED_SINGLETON_OUTPUT_CORE".into();
    api.replica.install_baseline(s.clone()).unwrap();
}
async fn call(api: &Api, url: &str, body: Value) -> (StatusCode, Value) {
    let r = api
        .handler()
        .oneshot(
            Request::builder()
                .method(
                    if url.starts_with("/internal/") || url.ends_with("quote-options") {
                        "GET"
                    } else {
                        "POST"
                    },
                )
                .uri(url)
                .header("authorization", format!("Bearer {}", api.config.token))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        r.status(),
        serde_json::from_slice(&to_bytes(r.into_body(), 8 << 20).await.unwrap()).unwrap(),
    )
}
fn tokens(p: &Pool, i: usize, j: usize) -> Vec<String> {
    if let Some(s) = &p.multi_asset {
        vec![s.assets[i].token.clone(), s.assets[j].token.clone()]
    } else {
        let t = [&p.token0, &p.token1];
        vec![t[i].clone(), t[j].clone()]
    }
}
fn request(pair: &[String], target: &Value, budget: Value) -> Value {
    json!({"chainKey":"evm:31337","kind":"EXACT_OUT","sellToken":pair[0],"buyToken":pair[1],"buyAmount":target,"maxSellAmount":budget,"caller":"0x000000000000000000000000000000000000dead","recipient":"0x000000000000000000000000000000000000beef","slippageBps":0,"verification":"FAST"})
}
fn record(index: usize, value: Value) {
    if let Ok(dir) = std::env::var("XONE_SINGLETON_API_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("case-{index}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}
#[tokio::test]
async fn singleton_http_full_targets_plans_views_and_limits_match_original_core() {
    let f = corpus();
    let (api, mut snapshot) = setup();
    let mut seen = BTreeSet::new();
    let (mut success, mut rejected) = (0, 0);
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        install(&api, &mut snapshot, vec![p.clone()]);
        let i = usize::from(!c["zeroForOne"].as_bool().unwrap());
        let pair = tokens(&p, i, 1 - i);
        let req = request(&pair, &c["target"], json!(MAX));
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        if c["passed"] == false {
            assert_eq!(status, StatusCode::NOT_FOUND, "{q}");
            assert_eq!(q["error"], "NO_ROUTE");
            record(
                index,
                json!({"index":index,"passed":false,"request":req,"response":q,"status":status.as_u16()}),
            );
            rejected += 1;
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{}: {q}", p.protocol);
        assert_eq!(q["requiredSellAmount"], c["requiredInput"]);
        assert_eq!(q["grossBuyAmount"], c["target"]);
        assert_eq!(q["outputSurplus"], "0");
        let id = q["quoteId"].as_str().unwrap();
        let (_, e) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        let mut broken = plan.clone();
        broken.allocation.routes[0].edges[0].from_token = Some(pair[0].clone());
        assert!(xone_execution_plan::exact_out::build(
            &broken.context,
            &broken.allocation,
            &broken.snapshot,
            &broken.economic_policy_hash
        )
        .is_err());
        let mut other_manager = plan.clone();
        other_manager.allocation.routes[0].factory =
            "0x000000000000000000000000000000000000dead".into();
        assert!(xone_execution_plan::exact_out::build(
            &other_manager.context,
            &other_manager.allocation,
            &other_manager.snapshot,
            &other_manager.economic_policy_hash
        )
        .is_err());
        if seen.insert(p.protocol.clone()) {
            let (_, options) = call(&api, "/v1/quote-options", Value::Null).await;
            let chain = options["chains"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["chainKey"] == "evm:31337")
                .unwrap();
            let view = &chain["protocolViews"][0];
            assert!(view["capabilities"]["quoteModes"]
                .as_array()
                .unwrap()
                .contains(&json!("EXACT_OUT")));
            assert_eq!(view["capabilities"]["executionPreparation"], false);
            assert_eq!(
                q["protocolViewIds"][format!("{}@{}", p.protocol, p.adapter_version)],
                view["viewId"]
            );
            assert_ne!(
                call(&api, &format!("/v1/quotes/{id}/build"), json!({}))
                    .await
                    .0,
                StatusCode::OK
            );
        }
        success += 1;
        let mut low = req.clone();
        let n: xone_domain::Amount = c["requiredInput"].as_str().unwrap().parse().unwrap();
        low["maxSellAmount"] = (n.0 - 1u8).to_string().into();
        let (low_status, low_response) = call(&api, "/v1/quote-exact-out", low.clone()).await;
        assert_eq!(low_status, StatusCode::BAD_REQUEST);
        assert_eq!(low_response["error"], "EXACT_OUT_MAX_INPUT");
        record(
            index,
            json!({"index":index,"passed":true,"request":req,"response":q,"evidence":e,"status":status.as_u16(),"budgetControl":{"request":low,"status":low_status.as_u16(),"response":low_response}}),
        );
    }
    assert_eq!((success, rejected), (424, 220));
    assert_eq!(seen.len(), 2);
}

#[tokio::test]
async fn singleton_http_advanced_native_output_in_both_directions() {
    use xone_domain::singleton::{SingletonAdvancedState, DYNAMIC_FEE_FLAG, ZERO_ADDRESS};

    let (api, mut snapshot) = setup();
    let fixture = corpus();
    for protocol in ["uniswap-v4", "pancake-infinity-cl"] {
        for zero in [true, false] {
            let case = fixture["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| {
                    case["pool"]["protocol"] == protocol
                        && case["passed"] == true
                        && case["zeroForOne"] == zero
                })
                .unwrap();
            let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
            let mut core = pool.singleton_cl.take().unwrap();
            core.hooks = "0x0000000000000000000000000000000000010000".into();
            let mut advanced = SingletonAdvancedState {
                protocol: protocol.into(),
                currency0: ZERO_ADDRESS.into(),
                currency1: pool.token1.clone(),
                key_fee: DYNAMIC_FEE_FLAG,
                snapshot_block_hash: snapshot.reference.block.hash.clone(),
                hook_model: String::new(),
                snapshot: core,
            };
            advanced.snapshot.pool_id = advanced.key_id().unwrap();
            pool.token0 = advanced.currency0.clone();
            pool.pool_id = format!(
                "{}/{}",
                advanced.snapshot.manager, advanced.snapshot.pool_id
            );
            pool.protocol.push_str("-advanced");
            pool.adapter_version = xone_domain::protocols::protocol(&pool.protocol)
                .unwrap()
                .adapter
                .into();
            pool.singleton_advanced = Some(advanced);
            install(&api, &mut snapshot, vec![pool.clone()]);
            let index = usize::from(!zero);
            let request = request(
                &tokens(&pool, index, 1 - index),
                &case["target"],
                json!(MAX),
            );
            let (status, quote) = call(&api, "/v1/quote-exact-out", request).await;
            assert_eq!(status, StatusCode::OK, "{protocol} {zero}: {quote}");
            assert_eq!(quote["requiredSellAmount"], case["requiredInput"]);
            assert_eq!(quote["grossBuyAmount"], case["target"]);
        }
    }
}
#[tokio::test]
#[ignore = "requires the original Cancun PoolManager quote results"]
async fn v4_settlement_hook_http_matches_original_pool_manager() {
    use xone_domain::singleton::{SingletonAdvancedState, V4_SETTLEMENT_HOOK_MODEL};

    let path = std::env::var_os("XONE_V4_HOOK_QUOTES").unwrap();
    let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(cases.len(), 8);
    let (api, mut snapshot) = setup();
    for case in cases {
        let zero = case["zeroForOne"].as_bool().unwrap();
        let exact_in = case["exactIn"].as_bool().unwrap();
        let mut state: SingletonAdvancedState = serde_json::from_value(json!({
            "protocol": "uniswap-v4",
            "currency0": case["currency0"],
            "currency1": case["currency1"],
            "key_fee": 0x800000,
            "snapshot_block_hash": snapshot.reference.block.hash,
            "hook_model": V4_SETTLEMENT_HOOK_MODEL,
            "snapshot": {
                "manager": case["manager"],
                "manager_code_sha256": case["managerCodeSha256"],
                "pool_id": "",
                "lp_fee": 0,
                "protocol_fee0": 0,
                "protocol_fee1": 0,
                "hooks": case["hook"],
                "parameters": "",
                "core": {
                    "sqrt_price_x96": "79228162514264337593543950336",
                    "tick": 0, "liquidity": "1000000000000", "tick_spacing": 60, "max_steps": 8,
                    "tick_words": [{"word_pos": -1, "bitmap": "28948022309329048855892746252171976963317496166410141009864396001978282409984"}, {"word_pos": 0, "bitmap": "4"}],
                    "ticks": [{"tick": -120, "liquidity_gross": "1000000000000", "liquidity_net": "1000000000000"}, {"tick": 120, "liquidity_gross": "1000000000000", "liquidity_net": "-1000000000000"}],
                },
            },
        })).unwrap();
        state.snapshot.pool_id = state.key_id().unwrap();
        assert_eq!(
            SingletonAdvancedState::from_proto(state.to_proto().unwrap()).unwrap(),
            state
        );
        let protocol = "uniswap-v4-advanced";
        let pool: Pool = serde_json::from_value(json!({
            "chain_key": "evm:31337",
            "pool_id": format!("{}/{}", state.snapshot.manager, state.snapshot.pool_id),
            "protocol": protocol,
            "token0": state.currency0,
            "token1": state.currency1,
            "reserve0": "0", "reserve1": "0", "fee_numerator": 0, "fee_denominator": 1_000_000,
            "adapter_version": xone_domain::protocols::protocol(protocol).unwrap().adapter,
            "token_behavior": "standard",
            "singleton_advanced": state,
        }))
        .unwrap();
        let i = usize::from(!zero);
        let pair = tokens(&pool, i, 1 - i);
        let input = case["input"].as_str().unwrap().parse().unwrap();
        let output = case["output"].as_str().unwrap().parse().unwrap();
        let next = if exact_in {
            let quote = xone_protocol_math::quote_exact_in(&pool, &input, zero).unwrap();
            assert_eq!(quote.amount_out, output);
            quote.next_pool
        } else {
            let quote =
                xone_protocol_math::exact_out::quote_exact_out(&pool, &output, zero).unwrap();
            assert_eq!(quote.amount_in, input);
            quote.next_pool
        };
        let core = next.singleton_advanced.unwrap().snapshot.core;
        assert_eq!(core.sqrt_price_x96.to_string(), case["priceAfter"]);
        assert_eq!(i64::from(core.tick), case["tickAfter"]);
        install(&api, &mut snapshot, vec![pool]);
        let (status, quote) = if exact_in {
            call(&api, "/v1/quote", json!({
                "chainKey":"evm:31337","kind":"EXACT_IN","sellToken":pair[0],"buyToken":pair[1],
                "sellAmount":case["input"],"caller":"0x000000000000000000000000000000000000dead",
                "recipient":"0x000000000000000000000000000000000000beef","slippageBps":0,
                "verification":"FAST"
            })).await
        } else {
            call(
                &api,
                "/v1/quote-exact-out",
                request(&pair, &case["output"], json!(MAX)),
            )
            .await
        };
        assert_eq!(status, StatusCode::OK, "{case}: {quote}");
        assert_eq!(quote["grossBuyAmount"], case["output"]);
        let sell_field = if exact_in {
            "consumedIn"
        } else {
            "requiredSellAmount"
        };
        assert_eq!(quote[sell_field], case["input"]);
    }
}

#[tokio::test]
async fn singleton_http_capacity_grid_and_old_view_revocation() {
    let f = corpus();
    let (api, mut snapshot) = setup();
    for grid in f["grids"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(grid["pools"].clone()).unwrap();
        install(&api, &mut snapshot, pools.clone());
        let i = usize::from(!grid["zeroForOne"].as_bool().unwrap());
        let pair = tokens(&pools[0], i, 1 - i);
        let req = request(&pair, &grid["target"], json!(MAX));
        let (status, q) = call(&api, "/v1/quote-exact-out", req).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], grid["bestRequiredInput"]);
        let id = q["quoteId"].as_str().unwrap();
        install(&api, &mut snapshot, pools);
        let (_, e) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        assert_eq!(e["snapshot_valid"], false);
        let (_, old) = call(&api, &format!("/v1/quotes/{id}/build"), json!({})).await;
        assert_eq!(old["error"], "SNAPSHOT_REORGED");
    }
}
