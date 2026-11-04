use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn crypto_shared_allocations_recompute_price_raw_balances_and_lp_supply() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-service.json"
    ))
    .unwrap();
    let c = d["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "claim-gulp-donations-18-18")
        .unwrap();
    let pool: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
    let graph = Graph::build(std::slice::from_ref(&pool)).unwrap();
    let edge = graph.adjacency[&pool.token0]
        .iter()
        .find(|e| e.to == pool.token1)
        .unwrap()
        .clone();
    let mut state = BTreeMap::from([(pool.pool_id.clone(), pool.clone())]);
    let mut outputs = Vec::new();
    let mut routes = Vec::new();
    for _ in 0..2 {
        outputs.push(evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap());
        routes.push(AllocatedRoute {
            path: PathQuote {
                amount_in: Amount::zero(),
                amount_out: Amount::zero(),
                edges: vec![edge.clone()],
                tokens: vec![pool.token0.clone(), pool.token1.clone()],
            },
            amount_in: input.clone(),
            amount_out: Amount::zero(),
        });
    }
    assert_eq!(outputs[0].to_string(), c["expected"]["amount_out"]);
    assert_ne!(outputs[0], outputs[1]);
    let allocation =
        evaluate_allocation(std::slice::from_ref(&pool), &routes, &Amount::zero()).unwrap();
    assert_eq!(allocation.routes[0].amount_out, outputs[0]);
    assert_eq!(allocation.routes[1].amount_out, outputs[1]);
    assert_eq!(allocation.gross_buy_amount.0, &outputs[0].0 + &outputs[1].0);
    assert_eq!(allocation.refund, Amount::zero());
    let old = pool
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap();
    let next = state[&pool.pool_id]
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap();
    assert!(next.total_supply > old.total_supply);
    assert!(next.admin_receiver_balance > old.admin_receiver_balance);
    assert_ne!(next.invariant, old.invariant);
    assert_ne!(next.raw_balances, old.raw_balances);
    assert_eq!(next.block_hash, old.block_hash);
    assert_eq!(
        pool.multi_asset
            .as_ref()
            .unwrap()
            .curve_crypto
            .as_ref()
            .unwrap(),
        old
    );
}

#[test]
fn admin_lp_mint_cannot_silently_change_another_candidate_pool() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-service.json"
    ))
    .unwrap();
    let c = d["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "claim-gulp-donations-18-18")
        .unwrap();
    let mut p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let mut q = p.clone();
    q.pool_id = "0x0000000000000000000000000000000000000020".into();
    let mint = p
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap()
        .lp_token
        .clone();
    let mint_code = p
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap()
        .lp_token_code_sha256
        .clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap()
        .lp_token = "0x0000000000000000000000000000000000000097".into();
    q.multi_asset.as_mut().unwrap().assets[0].token = mint;
    q.multi_asset.as_mut().unwrap().assets[0].code_sha256 = mint_code;
    q.token0 = q.multi_asset.as_ref().unwrap().assets[0].token.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap()
        .raw_balances[0] = p
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap()
        .total_supply
        .clone();
    Graph::build(&[p.clone(), q.clone()]).unwrap();
    let g = Graph::build(std::slice::from_ref(&p)).unwrap();
    let edge = g.adjacency[&p.token0]
        .iter()
        .find(|e| e.to == p.token1)
        .unwrap()
        .clone();
    let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
    let receiver = q.pool_id.clone();
    let c = p
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap();
    c.admin_receiver = receiver;
    c.admin_receiver_balance = q
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap()
        .raw_balances[0]
        .clone();
    p.validate().unwrap();
    q.validate().unwrap();
    let minted =
        xone_protocol_math::multi_asset::quote_exact_in_tokens(&p, &input, &p.token0, &p.token1)
            .unwrap();
    assert!(
        minted
            .next_pool
            .multi_asset
            .as_ref()
            .unwrap()
            .curve_crypto
            .as_ref()
            .unwrap()
            .admin_receiver_balance
            > p.multi_asset
                .as_ref()
                .unwrap()
                .curve_crypto
                .as_ref()
                .unwrap()
                .admin_receiver_balance
    );
    assert!(
        Graph::build(&[p.clone(), q.clone()]).is_err(),
        "graph hid an LP donation dependency"
    );
    let mut state = BTreeMap::from([
        (p.pool_id.clone(), p.clone()),
        (q.pool_id.clone(), q.clone()),
    ]);
    assert!(
        evaluate_path(&mut state, std::slice::from_ref(&edge), &input).is_err(),
        "direct path hid an LP donation dependency"
    );
    let route = AllocatedRoute {
        path: PathQuote {
            amount_in: input.clone(),
            amount_out: Amount::zero(),
            edges: vec![edge],
            tokens: vec![p.token0.clone(), p.token1.clone()],
        },
        amount_in: input,
        amount_out: Amount::zero(),
    };
    let mut no_mint = vec![p.clone(), q.clone()];
    no_mint[0].multi_asset.as_mut().unwrap().admin_fee = 0;
    Graph::build(&no_mint).unwrap();
    assert!(
        evaluate_allocation(&[p, q], &[route], &Amount::zero()).is_err(),
        "allocation hid an LP donation dependency"
    );
}
