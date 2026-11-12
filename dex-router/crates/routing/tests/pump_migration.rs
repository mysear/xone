use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{evaluate_path_with_funds, Edge, Funds, Graph};
#[test]
fn actual_migrate_created_pool_handoff_replaces_completed_curve_without_a_wrap_edge() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-migration.json")).unwrap();
    let before: Pool = serde_json::from_value(f["before"].clone()).unwrap();
    let after: Pool = serde_json::from_value(f["after"].clone()).unwrap();
    before.validate().unwrap();
    after.validate().unwrap();
    // Migration is a separate zeroed terminal state, never an exception for a
    // partially zero curve, a funded vault or an arbitrary rent balance.
    for variant in 0..3 {
        let mut bad = after.clone();
        let id = bad.pool_id.clone();
        let state = bad.pump.as_mut().unwrap();
        if variant == 0 {
            let a = state.account_mut(&id).unwrap();
            let mut raw = hex_data(&a.data);
            raw[8] = 1;
            a.data = to_hex(&raw);
        } else if variant == 1 {
            let vault = state.vault.clone();
            let a = state.account_mut(&vault).unwrap();
            let mut raw = hex_data(&a.data);
            raw[64] = 1;
            a.data = to_hex(&raw);
        } else {
            state.account_mut(&id).unwrap().lamports += 1;
        }
        assert!(bad.validate().is_err());
    }
    let mut legacy_after = after.clone();
    legacy_after.protocol = xone_domain::pump::PROFILE.into();
    legacy_after.adapter_version = xone_domain::pump::ADAPTER.into();
    assert!(legacy_after.validate().is_err());

    let g = Graph::build(std::slice::from_ref(&before)).unwrap();
    assert!(g.adjacency.is_empty());
    assert_eq!(g.excluded_pools[&before.pool_id], "PUMP_CURVE_COMPLETE");
    for c in f["cases"].as_array().unwrap() {
        let amm: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        amm.validate().unwrap();
        assert!(
            xone_domain::pump_amm::virtual_quote(amm.pump.as_ref().unwrap(), &amm).unwrap() > 0
        );
        let mut canonical = amm.clone();
        canonical.protocol = xone_domain::pump_amm::PROFILE.into();
        canonical.adapter_version = xone_domain::pump_amm::ADAPTER.into();
        canonical.validate().unwrap();

        assert_eq!(amm.token0, after.token0);
        assert_eq!(
            xone_domain::pump_amm::migration_target(&before).unwrap(),
            amm.pool_id
        );
        assert_eq!(
            xone_domain::pump_amm::migration_target(&after).unwrap(),
            amm.pool_id
        );
        let g = Graph::build(&[after.clone(), amm.clone()]).unwrap();
        assert!(!g.adjacency.contains_key(xone_domain::pump::NATIVE_SOL));
        assert!(g.adjacency.contains_key(xone_domain::pump_amm::WSOL));
        assert_eq!(g.excluded_pools[&after.pool_id], "PUMP_CURVE_COMPLETE");
        let zero = c["kind"] == "sell";
        let amount: Amount = c["amount"].as_str().unwrap().parse().unwrap();
        let edge = Edge {
            from_token: None,
            protocol: amm.protocol.clone(),
            pool_id: amm.pool_id.clone(),
            zero_for_one: zero,
            to: if zero {
                amm.token1.clone()
            } else {
                amm.token0.clone()
            },
        };
        let funds = Funds {
            sender: &amm.pump.as_ref().unwrap().user,
            recipient: &amm.pump.as_ref().unwrap().user,
        };
        let mut map = BTreeMap::from([
            (after.pool_id.clone(), after.clone()),
            (amm.pool_id.clone(), amm.clone()),
        ]);
        let output = if c["kind"] == "buy" {
            xone_routing::exact_out::evaluate_path(&mut map, &[edge], &amount, funds)
                .unwrap()
                .amount_out
        } else {
            evaluate_path_with_funds(&mut map, &[edge], &amount, funds).unwrap()
        };
        assert_eq!(
            output.to_string(),
            c[if zero { "actual_quote" } else { "actual_base" }]
                .as_i64()
                .unwrap()
                .to_string()
        );
        let mut expected: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        expected.pump.as_mut().unwrap().derived = true;
        assert_eq!(map[&amm.pool_id], expected);
    }
    let control = &f["virtual_reserve_control"];
    let p: Pool = serde_json::from_value(control["pool"].clone()).unwrap();
    p.validate().unwrap();
    assert!(xone_protocol_math::quote_exact_in(
        &p,
        &control["amount"].as_str().unwrap().parse().unwrap(),
        true
    )
    .is_err());
    let mut old = before.clone();
    old.protocol = xone_domain::pump::PROFILE.into();
    old.adapter_version = xone_domain::pump::ADAPTER.into();
    assert!(old.validate().is_err());
}

fn hex_data(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn to_hex(value: &[u8]) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn active_curve_sol_fee_wallets_do_not_alias_another_markets_wsol_fee_atas() {
    use xone_domain::meteora::{pda, pubkey};
    let fixture: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-bonding.json")).unwrap();
    let mut curve: Pool =
        serde_json::from_value(fixture["cases"][46]["steps"][0]["pool"].clone()).unwrap();
    let migration: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-migration.json")).unwrap();
    let amm: Pool = serde_json::from_value(migration["cases"][0]["pool"].clone()).unwrap();
    let mint = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
    let id = pda(
        &[b"bonding-curve", &pubkey(mint).unwrap()],
        xone_domain::pump::PROGRAM,
    )
    .unwrap();
    let vault = pda(
        &[
            &pubkey(&id).unwrap(),
            &pubkey(xone_domain::pump::TOKEN).unwrap(),
            &pubkey(mint).unwrap(),
        ],
        xone_domain::pump::ASSOCIATED,
    )
    .unwrap();
    let old = (
        curve.pool_id.clone(),
        curve.token0.clone(),
        curve.pump.as_ref().unwrap().vault.clone(),
    );
    curve.pool_id = id.clone();
    curve.token0 = mint.into();
    let state = curve.pump.as_mut().unwrap();
    state.vault = vault.clone();
    for a in &mut state.accounts {
        if a.address == old.0 {
            a.address = id.clone();
        } else if a.address == old.1 {
            a.address = mint.into();
        } else if a.address == old.2 {
            a.address = vault.clone();
            let mut raw = hex_data(&a.data);
            raw[..32].copy_from_slice(&pubkey(mint).unwrap());
            raw[32..64].copy_from_slice(&pubkey(&id).unwrap());
            a.data = to_hex(&raw);
        }
    }
    state.accounts.sort_by(|a, b| a.address.cmp(&b.address));
    state.slot = amm.pump.as_ref().unwrap().slot;
    state.timestamp = amm.pump.as_ref().unwrap().timestamp;
    curve.validate().unwrap();
    assert_eq!(state_wallet(&curve), state_wallet(&amm));
    let graph = Graph::build(&[curve.clone(), amm.clone()]).unwrap();
    assert!(!graph.excluded_pools.contains_key(&curve.pool_id));
    assert!(!graph.excluded_pools.contains_key(&amm.pool_id));
    assert!(graph.adjacency.contains_key(xone_domain::pump::NATIVE_SOL));
    assert!(graph.adjacency.contains_key(xone_domain::pump_amm::WSOL));
}
fn state_wallet(p: &Pool) -> &str {
    &p.pump.as_ref().unwrap().fee_recipient
}
