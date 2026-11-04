use xone_domain::Pool;
// Synthetic validation authority only; no original-chain identity assertion.
const SYNTHETIC: &str = r#"{"chain_key":"evm:1","pool_id":"0x0000000000000000000000000000000000000046","protocol":"balancer-v2-managed-aum","adapter_version":"balancer-v2/20230411-managed-aum-3","token0":"0x0000000000000000000000000000000000000001","token1":"0x0000000000000000000000000000000000000002","reserve0":"1000","reserve1":"1000","fee_numerator":1000000000000,"fee_denominator":1000000000000000000,"token_behavior":"standard","multi_asset":{"code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","vault":"0x0000000000000000000000000000000000000047","vault_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pool_id":"0x0000000000000000000000000000000000000046000100000000000000000000","assets":[{"token":"0x0000000000000000000000000000000000000001","balance":"1000","scaling":"1000000000000000000","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"token":"0x0000000000000000000000000000000000000002","balance":"1000","scaling":"1000000000000000000","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"weights":["500000000000000000","500000000000000000"],"amplification":"0","admin_fee":0,"managed_v2":{"timestamp":1000,"block_number":100,"block_hash":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","factory":"0x0000000000000000000000000000000000000050","factory_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pool_runtime_template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","fee_provider":"0x0000000000000000000000000000000000000051","fee_provider_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","protocol_collector":"0x0000000000000000000000000000000000000052","protocol_collector_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","dependencies":[{"role":"circuit-breaker-lib","address":"0x000000000000000000000000000000000000005a","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-add-remove-token-lib","address":"0x000000000000000000000000000000000000005b","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-amm-lib","address":"0x000000000000000000000000000000000000005c","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"external-weighted-math","address":"0x000000000000000000000000000000000000005d","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"recovery-mode-helper","address":"0x000000000000000000000000000000000000005e","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}],"total_supply":"1000","virtual_supply":"900","actual_supply":"905","pending_aum":"5","vault_bpt_cash":"100","vault_bpt_managed":"0","bpt_index":0,"manager":"0x0000000000000000000000000000000000000053","manager_bpt_balance":"600","protocol_collector_bpt_balance":"200","management_aum_fee":"10000000000000000","last_collection_timestamp":0,"provider_fee_id":"3","live_provider_aum_percentage":"700000000000000000","cached_protocol_aum_percentage":"300000000000000000","swaps_enabled":true,"paused":false,"vault_paused":false,"vault_pause_window_end":0,"vault_buffer_period_end":0,"recovery_mode":false,"allowlist_enabled":false,"pause_window_end":0,"buffer_period_end":0,"gradual_weight_end":0,"gradual_fee_end":0,"token_info":[{"cash":"700","managed":"300","asset_manager":"0x0000000000000000000000000000000000000054","last_change_block":0,"circuit_breaker_enabled":false,"vault_actual_balance":"700"},{"cash":"700","managed":"300","asset_manager":"0x0000000000000000000000000000000000000054","last_change_block":0,"circuit_breaker_enabled":false,"vault_actual_balance":"700"}],"vault_bpt_actual_balance":"100"}}}"#;
fn pool() -> Pool {
    serde_json::from_str(SYNTHETIC).unwrap()
}
#[test]
fn synthetic_profile() {
    pool().validate().unwrap();
}
#[test]
fn required_json_fields() {
    let v: serde_json::Value = serde_json::from_str(SYNTHETIC).unwrap();
    for key in v["multi_asset"]["managed_v2"].as_object().unwrap().keys() {
        for null in [false, true] {
            let mut p = v.clone();
            let m = p["multi_asset"]["managed_v2"].as_object_mut().unwrap();
            if null {
                m.insert(key.clone(), serde_json::Value::Null);
            } else {
                m.remove(key);
            }
            let result = serde_json::from_value::<Pool>(p);
            assert!(
                result.is_err() || result.unwrap().validate().is_err(),
                "{key}"
            );
        }
    }
}

use prost::Message;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use xone_domain::{
    multi_asset::MultiAssetState, scope_hash, wire, Amount, BlockRef, Snapshot, SnapshotRef,
};
fn val() -> Value {
    serde_json::from_str(SYNTHETIC).unwrap()
}
fn reject(v: Value) {
    let r = serde_json::from_value::<Pool>(v);
    assert!(r.is_err() || r.unwrap().validate().is_err());
}
#[test]
fn boundaries_and_profile_isolation() {
    for (path, value) in [
        (
            "/adapter_version",
            json!("balancer-v2-managed-fees/e281ff40-bpt-1"),
        ),
        ("/protocol", json!("balancer-v2-managed-snapshot")),
        ("/fee_numerator", json!(950_000_000_000_000_001u64)),
        ("/fee_numerator", json!(999_999_999_999u64)),
        ("/fee_denominator", json!(1u64)),
        (
            "/multi_asset/managed_v2/management_aum_fee",
            json!("950000000000000001"),
        ),
        (
            "/multi_asset/managed_v2/live_provider_aum_percentage",
            json!("1000000000000000001"),
        ),
        (
            "/multi_asset/managed_v2/cached_protocol_aum_percentage",
            json!("1000000000000000001"),
        ),
        (
            "/multi_asset/managed_v2/provider_fee_id",
            json!("9".repeat(79)),
        ),
        (
            "/multi_asset/managed_v2/last_collection_timestamp",
            json!(4_294_967_296u64),
        ),
        ("/multi_asset/managed_v2/bpt_index", json!(4_294_967_296u64)),
        ("/multi_asset/managed_v2/total_supply", json!("999")),
        ("/multi_asset/managed_v2/virtual_supply", json!("901")),
        ("/multi_asset/managed_v2/actual_supply", json!("906")),
        ("/multi_asset/managed_v2/pending_aum", json!("6")),
        ("/multi_asset/managed_v2/vault_bpt_managed", json!("1")),
        ("/multi_asset/managed_v2/manager_bpt_balance", json!("1001")),
        ("/multi_asset/managed_v2/manager_bpt_balance", json!("950")),
        (
            "/multi_asset/managed_v2/dependencies/0/role",
            json!("CircuitBreakerLib"),
        ),
        (
            "/multi_asset/managed_v2/dependencies/0/template_sha256",
            json!(""),
        ),
        (
            "/multi_asset/managed_v2/dependencies/0/code_sha256",
            json!("A".repeat(64)),
        ),
        ("/multi_asset/managed_v2/token_info/0/managed", json!("301")),
        (
            "/multi_asset/managed_v2/token_info/0/managed",
            json!((num_bigint::BigUint::from(1u8) << 112usize).to_string()),
        ),
        (
            "/multi_asset/managed_v2/token_info/0/circuit_breaker_enabled",
            json!(true),
        ),
        ("/multi_asset/managed_v2/bpt_index", json!(3)),
        ("/multi_asset/managed_v2/gradual_weight_end", json!(1001)),
        ("/multi_asset/managed_v2/gradual_fee_end", json!(1001)),
        ("/multi_asset/managed_v2/timestamp", json!(u64::MAX)),
        (
            "/multi_asset/managed_v2/block_hash",
            json!(format!("0x{}", "0".repeat(64))),
        ),
        ("/multi_asset/weights/0", json!("9999999999999999")),
        ("/multi_asset/weights/0", json!("500000000000000001")),
    ] {
        let mut v = val();
        *v.pointer_mut(path).unwrap() = value;
        reject(v);
    }
    let mut v = val();
    v["multi_asset"]["managed_v2"]["dependencies"][1]["role"] =
        v["multi_asset"]["managed_v2"]["dependencies"][0]["role"].clone();
    reject(v);
    let mut v = val();
    v["multi_asset"]["managed_v2"]["dependencies"]
        .as_array_mut()
        .unwrap()
        .pop();
    reject(v);
    let mut v = val();
    v["multi_asset"]["assets"][1]["token"] = v["pool_id"].clone();
    v["token1"] = v["pool_id"].clone();
    reject(v);
    let mut p = pool();
    p.fee_numerator = 950_000_000_000_000_000;
    p.validate().unwrap();
}
#[test]
fn exact_alias_ledger_and_zero_supply() {
    let mut v = val();
    v["multi_asset"]["managed_v2"]["manager"] =
        v["multi_asset"]["managed_v2"]["protocol_collector"].clone();
    reject(v);
    let mut v = val();
    let vault = v["multi_asset"]["vault"].clone();
    let m = &mut v["multi_asset"]["managed_v2"];
    m["manager"] = vault;
    m["manager_bpt_balance"] = json!("900");
    m["vault_bpt_cash"] = json!("900");
    m["vault_bpt_actual_balance"] = json!("900");
    m["virtual_supply"] = json!("100");
    m["actual_supply"] = json!("105");
    m["protocol_collector_bpt_balance"] = json!("50");
    serde_json::from_value::<Pool>(v)
        .unwrap()
        .validate()
        .unwrap();
    let mut p = pool();
    let s = p.multi_asset.as_mut().unwrap();
    let m = s.managed_v2.as_mut().unwrap();
    for a in [
        &mut m.total_supply,
        &mut m.virtual_supply,
        &mut m.actual_supply,
        &mut m.pending_aum,
        &mut m.vault_bpt_cash,
        &mut m.vault_bpt_managed,
        &mut m.vault_bpt_actual_balance,
        &mut m.manager_bpt_balance,
        &mut m.protocol_collector_bpt_balance,
    ] {
        *a = "0".parse::<Amount>().unwrap()
    }
    for (a, v) in s.assets.iter_mut().zip(&mut m.token_info) {
        a.balance = "0".parse().unwrap();
        v.cash = "0".parse().unwrap();
        v.managed = "0".parse().unwrap()
    }
    p.reserve0 = "0".parse().unwrap();
    p.reserve1 = "0".parse().unwrap();
    p.validate().unwrap();
}
#[test]
fn capacity_fifty_preserves_legacy_range() {
    for n in [2usize, 38, 39, 50, 51] {
        let mut v = val();
        let mut assets = vec![];
        let mut infos = vec![];
        let mut weights = vec![];
        for i in 0..n {
            let mut a = v["multi_asset"]["assets"][0].clone();
            a["token"] = json!(format!("0x{:040x}", i + 1));
            assets.push(a);
            infos.push(v["multi_asset"]["managed_v2"]["token_info"][0].clone());
            let w = if i == n - 1 {
                1_000_000_000_000_000_000u64 - 10_000_000_000_000_000u64 * (n as u64 - 1)
            } else {
                10_000_000_000_000_000u64
            };
            weights.push(json!(w.to_string()));
        }
        v["multi_asset"]["assets"] = json!(assets);
        v["multi_asset"]["weights"] = json!(weights);
        v["multi_asset"]["managed_v2"]["token_info"] = json!(infos);
        let p: Pool = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(p.validate().is_ok(), n <= 50, "{n}");
        if n >= 39 {
            v["protocol"] = json!("balancer-v2-managed-snapshot");
            v["adapter_version"] = json!("balancer-v2-managed-snapshot/no-bpt-fees-1");
            v["multi_asset"]
                .as_object_mut()
                .unwrap()
                .remove("managed_v2");
            reject(v);
        }
    }
}
fn snapshot(p: Pool) -> Snapshot {
    let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: p.chain_key.clone(),
                number_or_slot: m.block_number,
                hash: m.block_hash.clone(),
                parent_hash: String::new(),
                timestamp_ms: m.timestamp * 1000,
            },
            schema_version: 1,
            owner_service: "test".into(),
            owner_assignment_version: 1,
            writer_epoch: 1,
            canonical_epoch: 1,
            online_generation: 1,
            journal_seq: 1,
            adapter_set_hash: "synthetic".into(),
            scope_hash: scope_hash(std::slice::from_ref(&p)).unwrap(),
        },
        pools: vec![p],
        source: String::new(),
        history_gaps: vec![],
    }
}
#[test]
fn canonical_bank_and_explicit_derived_state() {
    snapshot(pool()).validate("test", 1).unwrap();
    for i in 0..3 {
        let mut s = snapshot(pool());
        match i {
            0 => s.reference.block.number_or_slot += 1,
            1 => s.reference.block.hash = format!("0x{}", "b".repeat(64)),
            _ => s.reference.block.timestamp_ms += 1,
        };
        assert!(s.validate("test", 1).is_err())
    }
    let mut p = pool();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.token_info[0].last_change_block = 101;
    m.last_collection_timestamp = 1001;
    p.validate().unwrap();
    assert!(snapshot(p).validate("test", 1).is_err());
}
#[test]
fn json_duplicates_case_null_and_nested_presence() {
    for bad in [
        SYNTHETIC.replacen("\"paused\":false", "\"paused\":false,\"paused\":false", 1),
        SYNTHETIC.replacen("\"paused\":false", "\"Paused\":false", 1),
        SYNTHETIC.replacen("\"managed_v2\":", "\"Managed_V2\":", 1),
    ] {
        assert!(serde_json::from_str::<Pool>(&bad).is_err())
    }
    for part in ["dependencies", "token_info"] {
        let v = val();
        for key in v["multi_asset"]["managed_v2"][part][0]
            .as_object()
            .unwrap()
            .keys()
        {
            let mut q = v.clone();
            q["multi_asset"]["managed_v2"][part][0]
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(serde_json::from_value::<Pool>(q).is_err(), "{part}.{key}")
        }
    }
    let mut v = val();
    v["multi_asset"]["managed_v2"] = Value::Null;
    assert!(serde_json::from_value::<Pool>(v).is_err());
}
#[test]
fn wire_round_trip_width_presence_and_shared_bytes() {
    let p = pool();
    let s = p.multi_asset.as_ref().unwrap();
    let w = s.to_proto().unwrap();
    let b = w.encode_to_vec();
    assert_eq!(
        hex::encode(Sha256::digest(&b)),
        "c681408d1fe8bcf3b52a3083b2684db036f8fadc1830c83f728e5cd3daf4751b"
    );
    assert_eq!(MultiAssetState::from_proto(w.clone()).unwrap(), *s);
    assert_eq!(wire::MultiAssetState::decode(b.as_slice()).unwrap(), w);
    for i in 0..19 {
        let mut q = w.clone();
        let m = q.managed_v2.as_mut().unwrap();
        match i {
            0 => m.timestamp = None,
            1 => m.block_number = None,
            2 => m.bpt_index = None,
            3 => m.last_collection_timestamp = None,
            4 => m.swaps_enabled = None,
            5 => m.paused = None,
            6 => m.recovery_mode = None,
            7 => m.allowlist_enabled = None,
            8 => m.pause_window_end = None,
            9 => m.buffer_period_end = None,
            10 => m.gradual_weight_end = None,
            11 => m.gradual_fee_end = None,
            12 => m.provider_fee_id = None,
            13 => m
                .provider_fee_id
                .as_mut()
                .unwrap()
                .uint256_be
                .pop()
                .map(|_| ())
                .unwrap(),
            14 => m.block_hash.clear(),
            15 => m.dependencies[0].template_sha256.clear(),
            16 => m.token_info[0].cash = None,
            17 => m.token_info[0].last_change_block = None,
            _ => m.token_info[0].circuit_breaker_enabled = None,
        };
        assert!(MultiAssetState::from_proto(q).is_err(), "{i}");
    }
    let mut q = p.clone();
    let m = q.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.dependencies[0].role = "changed".into();
    m.token_info[0].cash = "1".parse().unwrap();
    assert_ne!(q, p);
}

#[test]
fn known_zero_and_execution_flags_are_complete_inputs() {
    let mut p = pool();
    let s = p.multi_asset.as_mut().unwrap();
    let m = s.managed_v2.as_mut().unwrap();
    m.timestamp = 0;
    m.block_number = 0;
    m.bpt_index = 0;
    m.provider_fee_id = "0".parse().unwrap();
    m.management_aum_fee = "0".parse().unwrap();
    m.pending_aum = Amount::zero();
    m.actual_supply = m.virtual_supply.clone();
    m.live_provider_aum_percentage = "0".parse().unwrap();
    m.cached_protocol_aum_percentage = "0".parse().unwrap();
    m.swaps_enabled = false;
    m.paused = true;
    m.recovery_mode = true;
    m.allowlist_enabled = true;
    let w = s.to_proto().unwrap();
    let mw = w.managed_v2.as_ref().unwrap();
    assert_eq!(mw.timestamp, Some(0));
    assert_eq!(mw.block_number, Some(0));
    assert_eq!(mw.bpt_index, Some(0));
    assert_eq!(mw.swaps_enabled, Some(false));
    assert_eq!(MultiAssetState::from_proto(w).unwrap(), *s);
    p.validate().unwrap();
    snapshot(p).validate("test", 1).unwrap();
}

#[test]
fn canonical_source_rejects_future_collection_clock() {
    let mut p = pool();
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .last_collection_timestamp = 1001;
    p.validate().unwrap();
    assert!(snapshot(p).validate("test", 1).is_err());
}

#[test]
fn vault_recipient_uses_global_erc20_wallet_ledger() {
    for recipient in ["manager", "collector", "both"] {
        let mut p = pool();
        let s = p.multi_asset.as_mut().unwrap();
        let m = s.managed_v2.as_mut().unwrap();
        m.vault_bpt_cash = "900".parse().unwrap();
        m.vault_bpt_actual_balance = "950".parse().unwrap();
        m.virtual_supply = "100".parse().unwrap();
        m.actual_supply = "105".parse().unwrap();
        m.manager_bpt_balance = "30".parse().unwrap();
        m.protocol_collector_bpt_balance = "30".parse().unwrap();
        if recipient != "collector" {
            m.manager = s.vault.clone();
            m.manager_bpt_balance = "950".parse().unwrap()
        };
        if recipient != "manager" {
            m.protocol_collector = s.vault.clone();
            m.protocol_collector_bpt_balance = "950".parse().unwrap()
        };
        p.validate().unwrap();
    }
}

#[test]
fn wire_storage_words_reject_u32_overflow_instead_of_truncating() {
    for field in ["index", "clock"] {
        let p = pool();
        let mut w = p.multi_asset.unwrap().to_proto().unwrap();
        let m = w.managed_v2.as_mut().unwrap();
        if field == "index" {
            m.bpt_index = Some(1u64 << 32)
        } else {
            m.last_collection_timestamp = Some(1u64 << 32)
        };
        assert!(MultiAssetState::from_proto(w).is_err());
    }
}

#[test]
fn vault_history_is_uint32_even_on_a_wider_chain_block() {
    let mut p = pool();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.block_number = (1u64 << 32) + 8;
    m.token_info[0].last_change_block = (1u64 << 32) + 1;
    assert!(
        p.validate().is_err(),
        "accepted impossible packed Vault history"
    );
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .token_info[0]
        .last_change_block = u64::from(u32::MAX);
    p.validate().unwrap();
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .token_info[0]
        .last_change_block = 7;
    p.validate().unwrap();
}

#[test]
fn derived_execution_context_roundtrip_clone_and_canonical_boundaries() {
    let context = serde_json::json!({"block_number": (1u64 << 32) + 1, "timestamp": 1001});
    let mut value = serde_json::to_value(pool()).unwrap();
    value["multi_asset"]["managed_v2"]["execution_context"] = context.clone();
    let p: Pool = serde_json::from_value(value).expect("derived context is valid JSON state");
    p.validate().unwrap();
    let mut cloned = p.clone();
    let encoded = serde_json::to_value(&cloned).unwrap();
    assert_eq!(
        encoded["multi_asset"]["managed_v2"]["execution_context"],
        context
    );
    assert_eq!(serde_json::from_value::<Pool>(encoded).unwrap(), p);
    // A cloned optional context must own its contents independently.
    cloned
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .execution_context
        .as_mut()
        .unwrap()
        .timestamp = 1002;
    assert_eq!(
        p.multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap()
            .execution_context
            .as_ref()
            .unwrap()
            .timestamp,
        1001
    );
    assert_ne!(cloned, p);
    assert!(snapshot(p.clone()).validate("test", 1).is_err());
    assert!(
        p.multi_asset.as_ref().unwrap().to_proto().is_err(),
        "must not silently discard context"
    );
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!({"block_number":101}),
        serde_json::json!({"block_number":101,"timestamp":null}),
        serde_json::json!({"block_number":101,"timestamp":1001,"unknown":0}),
        serde_json::json!({"block_number":99,"timestamp":1001}),
        serde_json::json!({"block_number":101,"timestamp":999}),
        serde_json::json!({"block_number":101,"timestamp":u64::MAX}),
    ] {
        let mut v = serde_json::to_value(&p).unwrap();
        v["multi_asset"]["managed_v2"]["execution_context"] = invalid;
        assert!(serde_json::from_value::<Pool>(v)
            .map(|p| p.validate().is_err())
            .unwrap_or(true));
    }
    for collection in [true, false] {
        let mut invalid = p.clone();
        let m = invalid
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        m.execution_context.as_mut().unwrap().block_number = 101;
        if collection {
            m.last_collection_timestamp = 1002;
        } else {
            m.token_info[0].last_change_block = 102;
        }
        assert!(
            invalid.validate().is_err(),
            "context precedes updated ledger"
        );
    }
    let mut equal = serde_json::to_value(pool()).unwrap();
    equal["multi_asset"]["managed_v2"]["execution_context"] =
        serde_json::json!({"block_number":100,"timestamp":1000});
    let equal: Pool = serde_json::from_value(equal).unwrap();
    equal.validate().unwrap();
    assert!(snapshot(equal.clone()).validate("test", 1).is_err());
    assert!(equal.multi_asset.unwrap().to_proto().is_err());
    let canonical = serde_json::to_value(pool()).unwrap();
    assert!(canonical["multi_asset"]["managed_v2"]
        .get("execution_context")
        .is_none());
    assert_eq!(
        canonical["multi_asset"]["managed_v2"]
            .as_object()
            .unwrap()
            .len(),
        39
    );
}

#[test]
fn actual_vault_wallets_are_distinct_from_registered_cash_and_count_aliases_once() {
    let mut v = val();
    v["multi_asset"]["managed_v2"]["vault_bpt_actual_balance"] = json!("200");
    for item in v["multi_asset"]["managed_v2"]["token_info"]
        .as_array_mut()
        .unwrap()
    {
        item["vault_actual_balance"] = json!("2000");
    }
    let valid = serde_json::from_value::<Pool>(v.clone()).unwrap();
    valid.validate().unwrap();
    for (pointer, invalid) in [
        (
            "/multi_asset/managed_v2/vault_bpt_actual_balance",
            json!("99"),
        ),
        (
            "/multi_asset/managed_v2/vault_bpt_actual_balance",
            json!("201"),
        ),
        (
            "/multi_asset/managed_v2/token_info/0/vault_actual_balance",
            json!("699"),
        ),
        (
            "/multi_asset/managed_v2/token_info/0/vault_actual_balance",
            json!((num_bigint::BigUint::from(1u8) << 256usize).to_string()),
        ),
        (
            "/multi_asset/managed_v2/manager",
            v["multi_asset"]["vault"].clone(),
        ),
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(pointer).unwrap() = invalid;
        reject(bad);
    }
    let vault = v["multi_asset"]["vault"].clone();
    let m = &mut v["multi_asset"]["managed_v2"];
    m["manager"] = vault.clone();
    m["protocol_collector"] = vault;
    m["manager_bpt_balance"] = json!("200");
    m["protocol_collector_bpt_balance"] = json!("200");
    serde_json::from_value::<Pool>(v)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn reject_older_adapter_that_cannot_retain_actual_wallets() {
    let mut p = pool();
    p.adapter_version = "balancer-v2/20230411-managed-aum-1".into();
    assert!(p.validate().is_err());
}

#[test]
fn separate_pause_windows_and_effective_clock() {
    use xone_domain::managed_v2_state::ManagedV2ExecutionContext;
    for vault in [false, true] {
        let mut p = pool();
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        m.pause_window_end = 999;
        m.buffer_period_end = 1000;
        m.vault_pause_window_end = 998;
        m.vault_buffer_period_end = 1000;
        m.paused = !vault;
        m.vault_paused = vault;
        p.validate().expect("equality remains effectively paused");
        p.multi_asset.as_ref().unwrap().to_proto().unwrap();
        p.multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap()
            .timestamp = 1001;
        assert!(
            p.validate().is_err(),
            "expired source effective flag vault={vault}"
        );
        assert!(p.multi_asset.as_ref().unwrap().to_proto().is_err());
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        m.timestamp = 1000;
        m.execution_context = Some(ManagedV2ExecutionContext {
            block_number: 101,
            timestamp: 1001,
        });
        assert!(
            p.validate().is_err(),
            "expired derived effective flag vault={vault}"
        );
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        m.paused = false;
        m.vault_paused = false;
        m.validate().unwrap();
        assert_eq!((m.block_number, m.timestamp), (100, 1000));
        p.validate().unwrap();
    }
    for vault in [false, true] {
        let mut p = pool();
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        if vault {
            m.vault_pause_window_end = 1;
        } else {
            m.pause_window_end = 1;
        }
        assert!(
            p.validate().is_err(),
            "unordered pause windows vault={vault}"
        );
    }
}
#[test]
fn vault_pause_wire_presence_and_pre_separation_adapters() {
    let mut p = pool();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.vault_paused = true;
    m.vault_pause_window_end = 999;
    m.vault_buffer_period_end = 1000;
    let s = p.multi_asset.as_ref().unwrap();
    let w = s.to_proto().unwrap();
    assert_eq!(w.managed_v2.as_ref().unwrap().vault_paused, Some(true));
    assert_eq!(MultiAssetState::from_proto(w.clone()).unwrap(), *s);
    for i in 0..3 {
        let mut q = w.clone();
        let m = q.managed_v2.as_mut().unwrap();
        match i {
            0 => m.vault_paused = None,
            1 => m.vault_pause_window_end = None,
            _ => m.vault_buffer_period_end = None,
        }
        assert!(
            MultiAssetState::from_proto(q).is_err(),
            "absent vault scalar {i}"
        );
    }
    for version in [
        "balancer-v2/20230411-managed-aum-1",
        "balancer-v2/20230411-managed-aum-2",
    ] {
        p.adapter_version = version.into();
        assert!(p.validate().is_err(), "accepted {version}");
    }
}
