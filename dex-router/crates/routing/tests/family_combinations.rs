use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{evaluate_path_with_funds, Edge, Funds, Graph};
fn pump() -> Pool {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-bonding.json")).unwrap();
    serde_json::from_value(f["cases"][46]["steps"][0]["pool"].clone()).unwrap()
}
#[test]
fn native_pump_scope_can_coexist_with_wrapped_sol_dex_without_inventing_a_wrap_edge() {
    let p = pump();
    let mut r: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/raydium-state.json")).unwrap();
    r.chain_key = p.chain_key.clone();
    let g = Graph::build(&[p.clone(), r.clone()]).unwrap();
    assert!(g.adjacency.contains_key(xone_domain::pump::NATIVE_SOL));
    assert!(!g.adjacency[xone_domain::pump::NATIVE_SOL]
        .iter()
        .any(|e| e.to == "So11111111111111111111111111111111111111112"));
    let mut map = BTreeMap::from([
        (p.pool_id.clone(), p.clone()),
        (r.pool_id.clone(), r.clone()),
    ]);
    let before = map.clone();
    let edges = [
        Edge {
            from_token: None,
            protocol: p.protocol.clone(),
            pool_id: p.pool_id.clone(),
            zero_for_one: false,
            to: p.token0.clone(),
        },
        Edge {
            from_token: None,
            protocol: r.protocol.clone(),
            pool_id: r.pool_id.clone(),
            zero_for_one: true,
            to: r.token1.clone(),
        },
    ];
    let user = &p.pump.as_ref().unwrap().user;
    assert_eq!(
        evaluate_path_with_funds(
            &mut map,
            &edges,
            &"1000000".parse().unwrap(),
            Funds {
                sender: user,
                recipient: user
            }
        )
        .unwrap_err()
        .0,
        "PUMP_SINGLE_POOL_TEMPLATE"
    );
    assert_eq!(map, before);
}
#[test]
fn seven_first_wave_evm_chain_ids_reuse_pinned_dex_math_and_reject_cross_chain_composition() {
    let base: Vec<Pool> =
        serde_json::from_str::<Value>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()["pools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| serde_json::from_value(p.clone()).unwrap())
            .collect();
    for id in [1u64, 8453, 56, 42161, 10, 137, 43114] {
        let mut pools = base.clone();
        for p in &mut pools {
            p.chain_key = format!("evm:{id}");
        }
        let g = Graph::build(&pools).unwrap();
        let q = xone_routing::search(
            &g,
            &pools,
            "A",
            "C",
            &"10003".parse::<Amount>().unwrap(),
            xone_routing::Budget {
                split_parts: 4,
                max_hops: 3,
                max_candidates: 32,
                max_expansions: 1000,
            },
        )
        .unwrap();
        assert!(!q.candidates.is_empty());
    }
    let mut mixed = base.clone();
    mixed[0].chain_key = "evm:1".into();
    assert_eq!(Graph::build(&mixed).unwrap_err().0, "CROSS_CHAIN_GRAPH");
}

#[test]
fn shared_pump_fee_balances_are_consistent_and_both_pools_are_excluded_until_a_joint_ledger_exists()
{
    use xone_domain::meteora::{pda, pubkey};
    let a = pump();
    let mut b = a.clone();
    let mint = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
    let curve = pda(
        &[b"bonding-curve", &pubkey(mint).unwrap()],
        xone_domain::pump::PROGRAM,
    )
    .unwrap();
    let vault = pda(
        &[
            &pubkey(&curve).unwrap(),
            &pubkey(xone_domain::pump::TOKEN).unwrap(),
            &pubkey(mint).unwrap(),
        ],
        xone_domain::pump::ASSOCIATED,
    )
    .unwrap();
    let old_vault = b.pump.as_ref().unwrap().vault.clone();
    let old_curve = b.pool_id.clone();
    let old_mint = b.token0.clone();
    b.pool_id = curve.clone();
    b.token0 = mint.into();
    let state = b.pump.as_mut().unwrap();
    state.vault = vault.clone();
    for account in &mut state.accounts {
        if account.address == old_mint {
            account.address = mint.into();
        } else if account.address == old_curve {
            account.address = curve.clone();
        } else if account.address == old_vault {
            account.address = vault.clone();
            let mut raw = xone_domain::meteora::bytes(&account.data, 165).unwrap();
            raw[..32].copy_from_slice(&pubkey(mint).unwrap());
            raw[32..64].copy_from_slice(&pubkey(&curve).unwrap());
            account.data = raw.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    state.accounts.sort_by(|a, b| a.address.cmp(&b.address));
    b.validate().unwrap();
    let g = Graph::build(&[a.clone(), b.clone()]).unwrap();
    assert!(g.adjacency.is_empty());
    assert_eq!(
        g.excluded_pools[&a.pool_id],
        "PUMP_SHARED_FUND_ACCOUNT_NOT_MODELED"
    );
    assert_eq!(
        g.excluded_pools[&b.pool_id],
        "PUMP_SHARED_FUND_ACCOUNT_NOT_MODELED"
    );
    let receiver = b.pump.as_ref().unwrap().fee_recipient.clone();
    b.pump
        .as_mut()
        .unwrap()
        .account_mut(&receiver)
        .unwrap()
        .lamports += 1;
    assert_eq!(
        Graph::build(&[a, b]).unwrap_err().0,
        "PUMP_SHARED_ACCOUNT_CONFLICT"
    );
}
