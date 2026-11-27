use std::collections::BTreeMap;

use serde_json::json;
use xone_domain::curve_meta_state::{
    BASE_COINS, BASE_LP, BASE_LP_RUNTIME_SHA256, BASE_POOL, BASE_RUNTIME_SHA256, RUNTIME_SHA256,
};
use xone_domain::{Amount, Pool};
use xone_routing::{evaluate_path, exact_out, Funds, Graph};

const USDC: &str = BASE_COINS[1];
const TOKEN: &str = "0x2222222222222222222222222222222222222222";
const POOL: &str = "0x3333333333333333333333333333333333333333";
const PEER: &str = "0x4444444444444444444444444444444444444444";
const PEER_TOKEN: &str = "0x5555555555555555555555555555555555555555";

fn pool(address: &str, token: &str) -> Pool {
    let balance = "1000000000000000000000000";
    serde_json::from_value(json!({
        "chain_key": "evm:1", "pool_id": address, "protocol": "curve-metausd-direct",
        "token0": token, "token1": BASE_LP, "reserve0": balance, "reserve1": balance,
        "fee_numerator": 4000000u64, "fee_denominator": 10000000000u64,
        "adapter_version": "curve-metausd-direct/5272fd2b-fixed-base-1", "token_behavior": "standard",
        "multi_asset": {
            "code_sha256": "54ce32a723f0fb6800eaa07b8c5c30ff180462b9987692d0dd9d9734d58be849",
            "vault": "", "vault_code_sha256": "", "pool_id": "",
            "assets": [
                { "token": token, "balance": balance, "scaling": "1000000000000000000", "code_sha256": "a".repeat(64) },
                { "token": BASE_LP, "balance": balance, "scaling": "1000000000000000000", "code_sha256": BASE_LP_RUNTIME_SHA256 }
            ],
            "weights": [], "amplification": "10000", "admin_fee": 5000000000u64,
            "curve_meta": {
                "implementation": "0x1111111111111111111111111111111111111111",
                "implementation_code_sha256": RUNTIME_SHA256, "base_pool": BASE_POOL,
                "base_pool_code_sha256": BASE_RUNTIME_SHA256, "base_virtual_price": "1035816864512184633",
                "stored_balances": [balance, balance], "admin_balances": ["0", "0"],
                "timestamp": 1730036375u64, "block_number": 21057317u64,
                "block_hash": "0xe629de9301b3e54aeded60fdfa5978a0df85a7297d350d7edb5a76e19529dd80",
                "base_pool_state": {
                    "balances": ["28086765629238325382913520", "28487956359872", "103331634741618"],
                    "admin_balances": ["5702159388109678049974", "7183458578", "6778098172"],
                    "rates": ["1000000000000000000", "1000000000000000000000000000000", "1000000000000000000000000000000"],
                    "amplification": "200000", "fee": 1000000u64, "admin_fee": 5000000000u64,
                    "total_supply": "154355679851216307800770291"
                }
            }
        }
    })).unwrap()
}

fn meta(pool: &Pool) -> &xone_domain::curve_meta_state::CurveMetaState {
    pool.multi_asset
        .as_ref()
        .unwrap()
        .curve_meta
        .as_ref()
        .unwrap()
}

#[test]
fn meta_paths_share_base_post_state_in_both_directions() {
    let pools = [pool(POOL, TOKEN), pool(PEER, PEER_TOKEN)];
    let graph = Graph::build(&pools).unwrap();
    let join = graph.adjacency[USDC]
        .iter()
        .find(|edge| edge.pool_id == POOL && edge.to == TOKEN)
        .unwrap();
    let exit = graph.adjacency[PEER_TOKEN]
        .iter()
        .find(|edge| edge.pool_id == PEER && edge.to == USDC)
        .unwrap();
    assert!(!graph.adjacency[USDC].iter().any(|edge| edge.to == BASE_LP));
    let initial: BTreeMap<_, _> = pools
        .into_iter()
        .map(|pool| (pool.pool_id.clone(), pool))
        .collect();
    let mut joined = initial.clone();
    let amount: Amount = "1342933528".parse().unwrap();
    let target = evaluate_path(&mut joined, std::slice::from_ref(join), &amount).unwrap();
    let updated = meta(&joined[POOL]);
    assert_eq!(
        updated.base_virtual_price.to_string(),
        "1035816864780448739"
    );
    assert_eq!(
        meta(&joined[PEER]).base_virtual_price,
        updated.base_virtual_price
    );
    assert_eq!(meta(&joined[PEER]).base_pool_state, updated.base_pool_state);
    assert_eq!(joined[PEER].reserve0, initial[PEER].reserve0);

    let funds = Funds {
        sender: "0x6666666666666666666666666666666666666666",
        recipient: "0x7777777777777777777777777777777777777777",
    };
    let mut exact_join = initial.clone();
    let result =
        exact_out::evaluate_path(&mut exact_join, std::slice::from_ref(join), &target, funds)
            .unwrap();
    assert_eq!(result.amount_in, amount);
    assert_eq!(exact_join, joined);

    let mut exited = joined.clone();
    let exit_input: Amount = "1000000000000000000".parse().unwrap();
    let target = evaluate_path(&mut exited, std::slice::from_ref(exit), &exit_input).unwrap();
    assert_eq!(
        meta(&exited[POOL]).base_pool_state,
        meta(&exited[PEER]).base_pool_state
    );
    assert!(
        meta(&exited[POOL])
            .base_pool_state
            .as_ref()
            .unwrap()
            .total_supply
            .0
            < meta(&joined[POOL])
                .base_pool_state
                .as_ref()
                .unwrap()
                .total_supply
                .0
    );
    let result =
        exact_out::evaluate_path(&mut joined, std::slice::from_ref(exit), &target, funds).unwrap();
    assert_eq!(result.amount_out, target);
    assert!(result.amount_in.0 <= exit_input.0);
}
