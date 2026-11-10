//! T34.9: in-process HTTP API, real Replica and optimizer; no VM execution claim.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;

fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-shared-vault.json"
    ))
    .unwrap()
}
fn source() -> Snapshot {
    let bank = raw();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools = bank["banks"]["H38"]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["pool"].clone()).unwrap())
        .collect();
    let h = &bank["banks"]["H38"]["header"];
    let hex =
        |v: &Value| u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    s.reference.block.number_or_slot = hex(&h["number"]);
    s.reference.block.timestamp_ms = hex(&h["timestamp"]) * 1000;
    s.reference.block.hash = h["hash"].as_str().unwrap().into();
    s.reference.block.parent_hash = h["parentHash"].as_str().unwrap().into();
    seal(&mut s);
    s
}
fn seal(s: &mut Snapshot) {
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let mut adapters: Vec<_> = s.pools.iter().map(|p| p.adapter_version.clone()).collect();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = hash(&adapters).unwrap();
}
fn ordinary() -> Pool {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools[0].clone()
}
fn weighted(managed: &Pool, same_vault: bool) -> Pool {
    // A legal, old three-underlying static weighted profile, not a Managed
    // object with an incompatible test model attached.
    let d: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let mut p: Pool = serde_json::from_value(d["pools"][0].clone()).unwrap();
    if same_vault {
        let observed = managed.multi_asset.as_ref().unwrap();
        let s = p.multi_asset.as_mut().unwrap();
        s.vault = observed.vault.clone();
        s.vault_code_sha256 = observed.vault_code_sha256.clone();
    }
    p.validate().unwrap();
    p
}
fn mixed() -> Snapshot {
    let mut s = source();
    s.pools.push(weighted(&s.pools[0], true));
    s.pools.push(ordinary());
    seal(&mut s);
    s
}

fn api() -> Api {
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "t349-local-scope-regression-credential".into();
    config.config.routing.max_hops = 1;
    config.config.routing.max_splits = 1;
    config.config.api.quote_timeout_ms = 10_000;
    // A second EVM owner is local test configuration, with no external journal.
    let mut other = config.config.chains[0].clone();
    other.key = "evm:31338".into();
    config.config.chains.push(other);
    Api::new(config).unwrap()
}
fn request(chain: &str, p: &Pool, reverse: bool, amount: &str) -> Value {
    let (sell, buy) = if reverse {
        (&p.token1, &p.token0)
    } else {
        (&p.token0, &p.token1)
    };
    json!({"chainKey":chain,"kind":"EXACT_IN","sellToken":sell,"buyToken":buy,
        "sellAmount":amount,"caller":"0x0000000000000000000000000000000000000001",
        "recipient":"0x0000000000000000000000000000000000000002",
        "slippageBps":50,"verification":"FAST","constraints":{"maxHops":1}})
}
async fn post(api: &Api, body: Value) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .header("authorization", format!("Bearer {}", api.config.token))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn indicative(body: &Value) {
    assert_eq!(body["verification"]["level"], "INDICATIVE");
    assert_eq!(body["verification"]["success"], false);
    assert_eq!(
        body["verification"]["reason"],
        "INDEPENDENT_SIMULATION_UNAVAILABLE"
    );
}
#[tokio::test]
async fn mixed_scope_fast_quote_reports_only_executable_ids_and_complete_source_ref() {
    let api = api();
    let s = mixed();
    let ordinary = s.pools.last().unwrap();
    api.replica.install_baseline(s.clone()).unwrap();
    for reverse in [false, true] {
        let (status, body) = post(&api, request("evm:31337", ordinary, reverse, "10003")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        indicative(&body);
        assert_eq!(body["quality"]["candidateScope"], json!([ordinary.pool_id]));
        let excluded = body["quality"]["excludedPools"].as_object().unwrap();
        assert_eq!(excluded.len(), 3);
        // Both Managed physical registrations and the old weighted sibling are
        // excluded. Reverse token requests below exercise the opposite edge.
        for p in &s.pools[..3] {
            assert_eq!(
                excluded[&p.pool_id],
                "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED"
            );
        }
        assert_eq!(body["scopeHash"], s.reference.scope_hash);
        assert_eq!(
            body["snapshotRef"],
            serde_json::to_value(&s.reference).unwrap()
        );
        assert_ne!(
            body["scopeHash"],
            scope_hash(std::slice::from_ref(ordinary)).unwrap()
        );
        assert_eq!(
            body["routes"][0]["path"]["edges"][0]["pool_id"],
            ordinary.pool_id
        );
        assert_eq!(body["consumedIn"], "10003");
        assert_eq!(body["refund"], "0");
    }
    let view = api.replica.view("evm:31337").unwrap();
    assert_eq!(view.pools, s.pools);
    assert_eq!(view.reference, s.reference);
}
#[tokio::test]
async fn blocked_managed_and_weighted_directions_return_no_quote_and_verified_stays_unavailable() {
    let api = api();
    let s = mixed();
    api.replica.install_baseline(s.clone()).unwrap();
    for p in &s.pools[..3] {
        for reverse in [false, true] {
            let (status, body) = post(
                &api,
                request("evm:31337", p, reverse, "1000000000000000000"),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
            assert_eq!(body["error"], "NO_ROUTE");
            assert!(body.get("quoteId").is_none());
            assert!(body.get("routes").is_none());
        }
    }
    let mut verified = request("evm:31337", s.pools.last().unwrap(), false, "10003");
    verified["verification"] = json!("VERIFIED");
    let (status, body) = post(&api, verified).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "INDEPENDENT_SIMULATION_UNAVAILABLE");
    assert!(body.get("quoteId").is_none());
}
#[tokio::test]
async fn unsupported_vault_does_not_change_another_chains_quotes() {
    let api = api();
    let mut other = source();
    other.reference.block.chain_key = "evm:31338".into();
    let mut p = ordinary();
    p.chain_key = "evm:31338".into();
    other.pools = vec![p.clone()];
    seal(&mut other);
    api.replica.install_baseline(other.clone()).unwrap();
    let request = request("evm:31338", &p, false, "10003");
    let (status, before) = post(&api, request.clone()).await;
    assert_eq!(status, StatusCode::OK, "{before}");
    let saved = api.replica.view("evm:31338").unwrap();
    api.replica.install_baseline(mixed()).unwrap();
    let mut fresh = request.clone();
    fresh["caller"] = json!("0x0000000000000000000000000000000000000003");
    let (status, after) = post(&api, request).await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(before, after);
    // Also force a new request hash, so this checks live quote computation
    // after publishing the blocked Vault rather than just the cache hit.
    let (fresh_status, fresh_body) = post(&api, fresh).await;
    assert_eq!(fresh_status, StatusCode::OK, "{fresh_body}");
    assert_ne!(before["quoteId"], fresh_body["quoteId"]);
    for field in [
        "grossBuyAmount",
        "consumedIn",
        "refund",
        "quality",
        "snapshotRef",
    ] {
        assert_eq!(before[field], fresh_body[field], "{field}");
    }
    indicative(&fresh_body);
    assert!(std::sync::Arc::ptr_eq(
        &saved,
        &api.replica.view("evm:31338").unwrap()
    ));
    assert_eq!(after["quality"]["candidateScope"], json!([p.pool_id]));
    assert_eq!(after["quality"]["excludedPools"], json!({}));
    assert_eq!(
        after["snapshotRef"],
        serde_json::to_value(other.reference).unwrap()
    );
    indicative(&after);
}
#[tokio::test]
async fn supported_shared_managed_fast_quote_matches_one_original_local_raw_swap() {
    let api = api();
    let s = source();
    api.replica.install_baseline(s.clone()).unwrap();
    let bank = raw();
    let raw = &bank["cases"][0]["expected_native"];
    let (status, body) = post(
        &api,
        request(
            "evm:31337",
            &s.pools[0],
            false,
            raw["amount_in"].as_str().unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["grossBuyAmount"], raw["amount_out"]);
    assert_eq!(body["routes"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["routes"][0]["path"]["edges"][0]["pool_id"],
        s.pools[0].pool_id
    );
    assert_eq!(body["quality"]["excludedPools"], json!({}));
    assert_eq!(
        body["quality"]["candidateScope"],
        json!(s.pools.iter().map(|p| &p.pool_id).collect::<Vec<_>>())
    );
    assert_eq!(
        body["snapshotRef"],
        serde_json::to_value(&s.reference).unwrap()
    );
    indicative(&body);
    assert_eq!(api.replica.view("evm:31337").unwrap().pools, s.pools);
}

fn apply_admission_bank(api: &Api, snapshot: &mut Snapshot, marker: u8) {
    snapshot.reference.block.parent_hash = snapshot.reference.block.hash.clone();
    snapshot.reference.block.hash = format!("0x{}", format!("{marker:02x}").repeat(32));
    snapshot.reference.block.number_or_slot += 1;
    snapshot.reference.journal_seq += 1;
    for pool in &mut snapshot.pools {
        if let Some(state) = pool
            .multi_asset
            .as_mut()
            .and_then(|m| m.managed_v2.as_mut())
        {
            state.block_number = snapshot.reference.block.number_or_slot;
            state.block_hash = snapshot.reference.block.hash.clone();
        }
    }
    api.replica
        .apply(xone_domain::Record {
            reference: snapshot.reference.clone(),
            kind: "BLOCK_COMMIT".into(),
            record_id: format!("api-admission-{marker}"),
            payload_hash: hash(&snapshot.pools).unwrap(),
            pools: snapshot.pools.clone(),
        })
        .unwrap();
}
async fn get_local(api: &Api, uri: &str) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
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
async fn live_http_admission_refresh_revokes_cached_quotes_and_restores_both_directions() {
    let api = api();
    let mut bank = source();
    bank.pools.push(weighted(&bank.pools[0], false));
    bank.pools.push(ordinary());
    seal(&mut bank);
    api.replica.install_baseline(bank.clone()).unwrap();
    let r = request("evm:31337", &bank.pools[2], false, "1000000000000000000");
    let (status, before) = post(&api, r.clone()).await;
    assert_eq!(status, StatusCode::OK, "{before}");
    let old_id = before["quoteId"].as_str().unwrap().to_owned();
    let original_vault = bank.pools[2].multi_asset.as_ref().unwrap().vault.clone();
    let original_code = bank.pools[2]
        .multi_asset
        .as_ref()
        .unwrap()
        .vault_code_sha256
        .clone();
    let managed = bank.pools[0].multi_asset.as_ref().unwrap();
    let vault = managed.vault.clone();
    let code = managed.vault_code_sha256.clone();
    bank.pools[2].multi_asset.as_mut().unwrap().vault = vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = code;
    apply_admission_bank(&api, &mut bank, 0xe1);
    let (status, failed) = post(&api, r.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{failed}");
    assert_eq!(failed["error"], "NO_ROUTE");
    let (status, evidence) = get_local(&api, &format!("/internal/quotes/{old_id}/evidence")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(evidence["snapshot_valid"], false);
    let (status, fallback) = post(&api, request("evm:31337", &bank.pools[3], false, "10003")).await;
    assert_eq!(status, StatusCode::OK, "{fallback}");
    assert_eq!(
        fallback["quality"]["excludedPools"]
            .as_object()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        fallback["quality"]["candidateScope"],
        json!([bank.pools[3].pool_id])
    );
    bank.pools[2].multi_asset.as_mut().unwrap().vault = original_vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = original_code;
    apply_admission_bank(&api, &mut bank, 0xe2);
    for reverse in [false, true] {
        let (status, after) = post(
            &api,
            request("evm:31337", &bank.pools[2], reverse, "1000000000000000000"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{after}");
        assert_ne!(after["quoteId"], before["quoteId"]);
        assert_eq!(after["quality"]["excludedPools"], json!({}));
        assert_eq!(after["consumedIn"], "1000000000000000000");
        assert_eq!(after["refund"], "0");
        indicative(&after);
    }
}
#[tokio::test]
async fn empty_admission_scope_http_is_unready_then_recovers_without_losing_cursor() {
    let api = api();
    let mut bank = source();
    bank.pools.push(weighted(&bank.pools[0], false));
    seal(&mut bank);
    api.replica.install_baseline(bank.clone()).unwrap();
    let old = bank.pools[2].multi_asset.as_ref().unwrap();
    let old_vault = old.vault.clone();
    let old_code = old.vault_code_sha256.clone();
    let shared = bank.pools[0].multi_asset.as_ref().unwrap();
    let vault = shared.vault.clone();
    let code = shared.vault_code_sha256.clone();
    bank.pools[2].multi_asset.as_mut().unwrap().vault = vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = code;
    apply_admission_bank(&api, &mut bank, 0xe3);
    let (status, body) = post(
        &api,
        request("evm:31337", &bank.pools[2], false, "1000000000000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"], "CHAIN_NOT_READY");
    assert_eq!(
        api.replica.last_reference("evm:31337").unwrap(),
        Some(bank.reference.clone())
    );
    api.replica.restore_owner_checkpoint(bank.clone()).unwrap();
    bank.pools[2].multi_asset.as_mut().unwrap().vault = old_vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = old_code;
    apply_admission_bank(&api, &mut bank, 0xe4);
    let (status, recovered) = post(
        &api,
        request("evm:31337", &bank.pools[2], false, "1000000000000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovered}");
    assert_eq!(
        recovered["snapshotRef"],
        serde_json::to_value(&bank.reference).unwrap()
    );
}
