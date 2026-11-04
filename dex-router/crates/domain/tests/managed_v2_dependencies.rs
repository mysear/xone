use serde_json::{json, Value};
use xone_domain::{validate_pool_dependencies, Pool};

// Source observations remain literal repo-local contract inputs. Mutations and
// cloned physical identities below have synthetic validation authority only.
fn source() -> Value {
    let bank: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-aum-wire.json"
    ))
    .unwrap();
    // Synthetic cloned registrations use a prefunded global Vault wallet;
    // their registered pricing balances remain the native template values.
    let mut value = bank["original_fields"].clone();
    for info in value["multi_asset"]["managed_v2"]["token_info"]
        .as_array_mut()
        .unwrap()
    {
        let cash = info["cash"]
            .as_str()
            .unwrap()
            .parse::<num_bigint::BigUint>()
            .unwrap();
        info["vault_actual_balance"] = json!((cash * 4u8).to_string());
    }
    value
}
fn sibling(v: &Value) -> Value {
    let mut v = v.clone();
    v["pool_id"] = json!("0x0000000000000000000000000000000000000048");
    v["multi_asset"]["pool_id"] = json!(format!(
        "{}000100000000000000000000",
        v["pool_id"].as_str().unwrap()
    ));
    v
}
fn parse(v: Value) -> Pool {
    serde_json::from_value(v).unwrap()
}

#[test]
fn managed_dependencies_reject_individually_valid_contradictions() {
    let base = source();
    let number = base["multi_asset"]["managed_v2"]["block_number"]
        .as_u64()
        .unwrap();
    let time = base["multi_asset"]["managed_v2"]["timestamp"]
        .as_u64()
        .unwrap();
    let actual = base["multi_asset"]["managed_v2"]["token_info"][0]["vault_actual_balance"]
        .as_str()
        .unwrap()
        .parse::<num_bigint::BigUint>()
        .unwrap();
    for (pointer, value, reason) in [
        (
            "/multi_asset/vault_code_sha256",
            json!("b".repeat(64)),
            "SHARED_VAULT_IDENTITY",
        ),
        (
            "/multi_asset/managed_v2/block_number",
            json!(number + 1),
            "SHARED_VAULT_SOURCE",
        ),
        (
            "/multi_asset/managed_v2/block_hash",
            json!(format!("0x{}", "b".repeat(64))),
            "SHARED_VAULT_SOURCE",
        ),
        (
            "/multi_asset/managed_v2/timestamp",
            json!(time + 1),
            "SHARED_VAULT_SOURCE",
        ),
        (
            "/multi_asset/managed_v2/execution_context",
            json!({"block_number": number + 1, "timestamp": time}),
            "SHARED_VAULT_CLOCK",
        ),
        (
            "/multi_asset/managed_v2/execution_context",
            json!({"block_number": number, "timestamp": time + 1}),
            "SHARED_VAULT_CLOCK",
        ),
        (
            "/multi_asset/managed_v2/vault_paused",
            json!(true),
            "SHARED_VAULT_PAUSE",
        ),
        (
            "/multi_asset/managed_v2/vault_pause_window_end",
            json!(
                base["multi_asset"]["managed_v2"]["vault_pause_window_end"]
                    .as_u64()
                    .unwrap()
                    + 1
            ),
            "SHARED_VAULT_PAUSE",
        ),
        (
            "/multi_asset/managed_v2/vault_buffer_period_end",
            json!(
                base["multi_asset"]["managed_v2"]["vault_buffer_period_end"]
                    .as_u64()
                    .unwrap()
                    + 1
            ),
            "SHARED_VAULT_PAUSE",
        ),
        (
            "/multi_asset/assets/0/code_sha256",
            json!("b".repeat(64)),
            "SHARED_TOKEN_IDENTITY",
        ),
        (
            "/multi_asset/assets/0/scaling",
            json!("10000000000000000000"),
            "SHARED_TOKEN_IDENTITY",
        ),
        (
            "/multi_asset/managed_v2/token_info/0/vault_actual_balance",
            json!((actual + 1u8).to_string()),
            "SHARED_VAULT_WALLET",
        ),
    ] {
        let mut other = sibling(&base);
        if pointer.ends_with("execution_context") {
            other["multi_asset"]["managed_v2"]["execution_context"] = value;
        } else {
            *other.pointer_mut(pointer).unwrap() = value;
        }
        let rows = [parse(base.clone()), parse(other)];
        for row in &rows {
            row.validate().expect(pointer);
        }
        for rows in [rows.clone(), [rows[1].clone(), rows[0].clone()]] {
            let error = validate_pool_dependencies(&rows).expect_err(pointer);
            assert!(error.to_string().contains(reason), "{pointer}: {error}");
            assert!(xone_domain::unavailable_pool_reasons(&rows)
                .unwrap_err()
                .to_string()
                .contains(reason));
        }
    }
}

fn snapshot(pools: Vec<Pool>) -> xone_domain::Snapshot {
    use xone_domain::{scope_hash, BlockRef, Snapshot, SnapshotRef};
    let p = &pools[0];
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
            scope_hash: scope_hash(&pools).unwrap(),
        },
        pools,
        source: String::new(),
        history_gaps: vec![],
    }
}
fn weighted(base: &Value) -> Pool {
    let mut v = sibling(base);
    v["protocol"] = json!("balancer-v2-weighted-multi");
    v["adapter_version"] = json!("balancer-v2-weighted-multi/static-1");
    v["multi_asset"]
        .as_object_mut()
        .unwrap()
        .remove("managed_v2");
    let mut third = v["multi_asset"]["assets"][0].clone();
    third["token"] = json!("0xfffffffffffffffffffffffffffffffffffffffe");
    v["multi_asset"]["assets"]
        .as_array_mut()
        .unwrap()
        .push(third);
    v["multi_asset"]["weights"] = json!([
        "300000000000000000",
        "300000000000000000",
        "400000000000000000"
    ]);
    // Another adapter's scaling is not Managed token identity authority.
    v["multi_asset"]["assets"][0]["scaling"] = json!("10000000000000000000");
    parse(v)
}
fn assert_unblocked(rows: &[Pool]) {
    for row in rows {
        row.validate().unwrap();
    }
    xone_domain::managed_v2_dependencies::validate(rows).unwrap();
    validate_pool_dependencies(rows).unwrap();
    assert!(xone_domain::unavailable_pool_reasons(rows)
        .unwrap()
        .is_empty());
}
#[test]
fn managed_dependencies_source_contract_and_pool_local_facts() {
    let bank: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-aum-wire.json"
    ))
    .unwrap();
    let row = parse(bank["original_fields"].clone());
    assert_unblocked(std::slice::from_ref(&row));
    snapshot(vec![row]).validate("test", 1).unwrap();
    let canonical: xone_domain::Snapshot =
        serde_json::from_value(bank["snapshot"].clone()).unwrap();
    canonical
        .validate(
            &canonical.reference.owner_service,
            canonical.reference.owner_assignment_version,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&canonical.pools[0]).unwrap(),
        bank["original_fields"]
    );
    let base = source();
    let mut other = sibling(&base);
    let m = &mut other["multi_asset"]["managed_v2"];
    m["paused"] = json!(true);
    m["pause_window_end"] = json!(m["pause_window_end"].as_u64().unwrap() + 1);
    m["buffer_period_end"] = json!(m["buffer_period_end"].as_u64().unwrap() + 1);
    m["cached_protocol_aum_percentage"] = json!("400000000000000000");
    let rows = vec![parse(base.clone()), parse(other)];
    assert_unblocked(&rows);
    snapshot(rows).validate("test", 1).unwrap();
    let mut other = sibling(&base);
    let m = &base["multi_asset"]["managed_v2"];
    other["multi_asset"]["managed_v2"]["execution_context"] =
        json!({"block_number":m["block_number"],"timestamp":m["timestamp"]});
    let rows = vec![parse(base), parse(other)];
    assert_unblocked(&rows); // None is the source clock, not a distinct clock.
    assert!(snapshot(rows)
        .validate("test", 1)
        .unwrap_err()
        .to_string()
        .contains("DERIVED_STATE_NOT_CANONICAL"));
    assert_unblocked(&[]);
}
#[test]
fn managed_dependencies_chain_vault_and_token_isolation() {
    let base = source();
    let mut other = sibling(&base);
    other["multi_asset"]["vault"] = json!("0x0000000000000000000000000000000000000049");
    other["multi_asset"]["vault_code_sha256"] = json!("b".repeat(64));
    let m = &mut other["multi_asset"]["managed_v2"];
    m["block_number"] = json!(m["block_number"].as_u64().unwrap() + 1);
    m["timestamp"] = json!(m["timestamp"].as_u64().unwrap() + 1);
    m["block_hash"] = json!(format!("0x{}", "b".repeat(64)));
    m["vault_paused"] = json!(true);
    m["token_info"][0]["vault_actual_balance"] = json!("20000000000000000000000");
    assert_unblocked(&[parse(base.clone()), parse(other.clone())]);
    // Managed token identity still spans Vaults on a chain.
    other["multi_asset"]["assets"][0]["scaling"] = json!("10000000000000000000");
    assert!(
        validate_pool_dependencies(&[parse(base.clone()), parse(other.clone())])
            .unwrap_err()
            .to_string()
            .contains("SHARED_TOKEN_IDENTITY")
    );
    // All cross-pool fact keys include chain; a general slice may be multi-chain.
    other["chain_key"] = json!("evm:1");
    other["multi_asset"]["vault"] = base["multi_asset"]["vault"].clone();
    let rows = vec![parse(base.clone()), parse(other)];
    assert_unblocked(&rows);
    assert!(snapshot(rows).validate("test", 1).is_err());
    // Different tokens in the same Vault do not share wallet values.
    let mut other = sibling(&base);
    other["multi_asset"]["assets"][0]["token"] =
        json!("0x0000000000000000000000000000000000000001");
    other["token0"] = other["multi_asset"]["assets"][0]["token"].clone();
    other["multi_asset"]["managed_v2"]["token_info"][0]["vault_actual_balance"] =
        json!("20000000000000000000000");
    assert_unblocked(&[parse(base), parse(other)]);
}
#[test]
fn managed_dependencies_validate_individual_manager_constraints_first() {
    let mut v = source();
    v["multi_asset"]["managed_v2"]["token_info"][0]["managed"] = json!("1");
    v["multi_asset"]["managed_v2"]["token_info"][0]["cash"] = json!("9999999999999999999999");
    let p = parse(v.clone());
    assert!(xone_domain::managed_v2_dependencies::validate([&p]).is_err());
    assert!(xone_domain::unavailable_pool_reasons([&p]).is_err());
    v["multi_asset"]["managed_v2"]["token_info"][0]["asset_manager"] =
        json!("0x0000000000000000000000000000000000000054");
    assert_unblocked(&[parse(v)]);
    let mut p = parse(source());
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .token_info
        .pop();
    assert!(xone_domain::unavailable_pool_reasons([&p]).is_err());
    p.multi_asset.as_mut().unwrap().managed_v2 = None;
    assert!(xone_domain::unavailable_pool_reasons([&p]).is_err());
}
#[test]
fn managed_capabilities_mixed_groups_preserve_canonical_bank() {
    let base = source();
    let managed = parse(base.clone());
    let weighted = weighted(&base);
    let mut ordinary = parse(sibling(&base));
    ordinary.pool_id = "0x0000000000000000000000000000000000000055".into();
    let s = ordinary.multi_asset.as_mut().unwrap();
    s.pool_id = format!("{}000100000000000000000000", ordinary.pool_id);
    s.vault = "0x0000000000000000000000000000000000000056".into();
    let rows = vec![managed.clone(), weighted.clone(), ordinary.clone()];
    for row in &rows {
        row.validate().unwrap();
    }
    snapshot(rows.clone()).validate("test", 1).unwrap();
    for rows in [rows.clone(), rows.into_iter().rev().collect()] {
        let blocked = xone_domain::unavailable_pool_reasons(&rows).unwrap();
        assert_eq!(
            blocked,
            xone_domain::managed_v2_dependencies::blocked_pools(&rows).unwrap()
        );
        assert_eq!(blocked.len(), 2);
        for id in [&managed.pool_id, &weighted.pool_id] {
            assert_eq!(blocked[id], "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED");
        }
        assert!(!blocked.contains_key(&ordinary.pool_id));
    }
    let mut second = weighted.clone();
    second.pool_id = "0x0000000000000000000000000000000000000057".into();
    second.multi_asset.as_mut().unwrap().pool_id =
        format!("{}000100000000000000000000", second.pool_id);
    assert_unblocked(&[weighted.clone(), second]);
    let mut cross_chain = weighted.clone();
    cross_chain.chain_key = "evm:1".into();
    assert_unblocked(&[managed.clone(), cross_chain]);
    let mut different_vault = weighted.clone();
    different_vault.multi_asset.as_mut().unwrap().vault =
        "0x0000000000000000000000000000000000000056".into();
    assert_unblocked(&[managed.clone(), different_vault]);
    let mut contradiction = weighted;
    contradiction
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = "b".repeat(64);
    assert!(
        xone_domain::unavailable_pool_reasons([&managed, &contradiction])
            .unwrap_err()
            .to_string()
            .contains("SHARED_VAULT_IDENTITY")
    );
}
#[test]
fn managed_capabilities_cross_pool_bpt_blocks_whole_group_and_takes_precedence() {
    let base = source();
    let mut p = parse(base.clone());
    let mut q = parse(sibling(&base));
    let underlying_cash = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .token_info[0]
        .cash
        .clone();
    let qm = q.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    qm.vault_bpt_actual_balance = xone_domain::Amount(&qm.vault_bpt_cash.0 + &underlying_cash.0);
    let s = p.multi_asset.as_mut().unwrap();
    s.assets[0].token = q.pool_id.clone();
    s.assets[0].code_sha256 = q.multi_asset.as_ref().unwrap().code_sha256.clone();
    p.token0 = q.pool_id.clone();
    s.managed_v2.as_mut().unwrap().token_info[0].vault_actual_balance = q
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .vault_bpt_actual_balance
        .clone();
    let mut w = weighted(&base);
    w.pool_id = "0x0000000000000000000000000000000000000057".into();
    w.multi_asset.as_mut().unwrap().pool_id = format!("{}000100000000000000000000", w.pool_id);
    let mut unrelated = parse(sibling(&base));
    unrelated.pool_id = "0x0000000000000000000000000000000000000058".into();
    unrelated.multi_asset.as_mut().unwrap().pool_id =
        format!("{}000100000000000000000000", unrelated.pool_id);
    unrelated.multi_asset.as_mut().unwrap().vault =
        "0x0000000000000000000000000000000000000059".into();
    let rows = vec![p.clone(), q.clone(), w, unrelated.clone()];
    snapshot(rows.clone()).validate("test", 1).unwrap();
    let blocked = xone_domain::unavailable_pool_reasons(&rows).unwrap();
    assert_eq!(blocked.len(), 3);
    assert!(blocked
        .values()
        .all(|r| r == "SHARED_VAULT_BPT_UNSUPPORTED"));
    assert!(!blocked.contains_key(&unrelated.pool_id));
    // Without the mixed sibling it is still unsupported.
    assert_eq!(
        xone_domain::unavailable_pool_reasons([&p, &q])
            .unwrap()
            .len(),
        2
    );
    // Conflicting own BPT/underlying wallets are factual errors, even in a blocked group.
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .token_info[0]
        .vault_actual_balance = "3000000000000000000000000000000000".parse().unwrap();
    assert!(xone_domain::unavailable_pool_reasons([&q, &p])
        .unwrap_err()
        .to_string()
        .contains("SHARED_VAULT_WALLET"));
    // Cross-Vault and cross-chain physical IDs do not establish a BPT dependency.
    let mut q2 = q.clone();
    q2.multi_asset.as_mut().unwrap().vault = "0x0000000000000000000000000000000000000059".into();
    assert_unblocked(&[p.clone(), q2]);
    let mut q2 = q;
    q2.chain_key = "evm:1".into();
    assert_unblocked(&[p, q2]);
}

#[test]
fn managed_capabilities_leave_unrelated_v2_candidates_available() {
    let bank: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/v2-complete-wire.json"
    ))
    .unwrap();
    let v2 = parse(bank["cases"][0]["snapshot"]["pools"][0].clone());
    v2.validate().unwrap();
    let base = source();
    let rows = vec![parse(base.clone()), weighted(&base), v2.clone()];
    validate_pool_dependencies(&rows).unwrap();
    let blocked = xone_domain::unavailable_pool_reasons(&rows).unwrap();
    assert_eq!(blocked.len(), 2);
    assert!(!blocked.contains_key(&v2.pool_id));
    // Even without any multi-asset state, unrelated pool data stays intact.
    assert_eq!(rows[2], v2);
}
#[test]
fn managed_dependencies_common_derived_clock_is_valid_only_for_execution() {
    let base = source();
    let mut rows = vec![parse(base.clone()), parse(sibling(&base))];
    for row in &mut rows {
        let m = row
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        m.execution_context = Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
            block_number: m.block_number + 1,
            timestamp: m.timestamp + 1,
        });
    }
    assert_unblocked(&rows);
    assert!(snapshot(rows)
        .validate("test", 1)
        .unwrap_err()
        .to_string()
        .contains("DERIVED_STATE_NOT_CANONICAL"));
}

#[test]
fn original_shared_wallet_backs_total_cash_in_every_declared_registration() {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-shared-vault.json"
    ))
    .unwrap();
    let mut rows = raw["banks"]["H39"]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| parse(row["pool"].clone()))
        .collect::<Vec<_>>();
    validate_pool_dependencies(rows.iter()).unwrap();
    let total = rows.iter().fold(num_bigint::BigUint::from(0u8), |sum, p| {
        sum + &p
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap()
            .token_info[0]
            .cash
            .0
    }) - 1u8;
    for row in &mut rows {
        row.multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap()
            .token_info[0]
            .vault_actual_balance = xone_domain::Amount(total.clone());
        row.validate().unwrap();
    }
    assert!(validate_pool_dependencies(rows.iter())
        .unwrap_err()
        .to_string()
        .contains("SHARED_VAULT_REGISTERED_CASH"));
    assert!(xone_domain::unavailable_pool_reasons(rows.iter()).is_err());
}

#[test]
fn original_same_source_roles_and_live_provider_getters_cannot_conflict() {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-shared-vault.json"
    ))
    .unwrap();
    let originals = raw["banks"]["H38"]["pools"].as_array().unwrap();
    let first = parse(originals[0]["pool"].clone());
    let second = parse(originals[1]["pool"].clone());
    for field in [
        "factory_code_sha256",
        "fee_provider_code_sha256",
        "protocol_collector_code_sha256",
        "live_provider_aum_percentage",
        "dependency_code",
        "dependency_template",
    ] {
        let mut changed = serde_json::to_value(&second).unwrap();
        let state = &mut changed["multi_asset"]["managed_v2"];
        match field {
            "dependency_code" => state["dependencies"][0]["code_sha256"] = json!("b".repeat(64)),
            "dependency_template" => {
                state["dependencies"][0]["template_sha256"] = json!("b".repeat(64))
            }
            "live_provider_aum_percentage" => state[field] = json!("400000000000000000"),
            _ => state[field] = json!("b".repeat(64)),
        }
        let changed = parse(changed);
        changed.validate().unwrap();
        for rows in [
            vec![first.clone(), changed.clone()],
            vec![changed.clone(), first.clone()],
        ] {
            assert!(
                validate_pool_dependencies(rows.iter()).is_err(),
                "same-source role conflict {field}"
            );
            assert!(
                snapshot(rows.clone()).validate("test", 1).is_err(),
                "canonical role conflict {field}"
            );
            assert!(
                xone_domain::unavailable_pool_reasons(rows.iter()).is_err(),
                "role conflict is not capability isolation {field}"
            );
        }
    }
}
#[test]
fn duplicate_registration_conflicts_reject_in_either_order_without_counting_identical_twice() {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-shared-vault.json"
    ))
    .unwrap();
    let originals = raw["banks"]["H38"]["pools"].as_array().unwrap();
    let first = parse(originals[0]["pool"].clone());
    let second = parse(originals[1]["pool"].clone());
    validate_pool_dependencies([&first, &first, &second]).unwrap();
    let mut changed = first.clone();
    changed.reserve0.0 += 1u8;
    let multi = changed.multi_asset.as_mut().unwrap();
    multi.assets[0].balance.0 += 1u8;
    multi.managed_v2.as_mut().unwrap().token_info[0].cash.0 += 1u8;
    changed.validate().unwrap();
    for rows in [
        vec![first.clone(), changed.clone(), second.clone()],
        vec![changed, first, second],
    ] {
        assert!(validate_pool_dependencies(rows.iter())
            .unwrap_err()
            .to_string()
            .contains("DUPLICATE_PHYSICAL_POOL_CONFLICT"));
    }
}
