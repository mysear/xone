//! T34.9: real allocation replay; raw receipt authority is limited to the two
//! original swaps. The split grid and cloned C identity are local scenarios.
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
use xone_domain::{managed_v2_state::BalancerManagedV2State, Amount, Pool};
use xone_optimizer::{evaluate_allocation, optimize, AllocatedRoute, Allocation};
use xone_protocol_math::managed_v2_pool::quote_at;
use xone_routing::{evaluate_path, Graph, PathQuote};

fn bank() -> &'static Value {
    static BANK: OnceLock<Value> = OnceLock::new();
    BANK.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../test/contracts/managed-v2-shared-vault.json"
        ))
        .unwrap()
    })
}
fn pools() -> Vec<Pool> {
    bank()["banks"]["H38"]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["pool"].clone()).unwrap())
        .collect()
}
fn m(p: &Pool) -> &BalancerManagedV2State {
    p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap()
}
fn amount(s: &str) -> Amount {
    s.parse().unwrap()
}
fn path(pools: &[Pool], index: usize) -> PathQuote {
    let p = &pools[index];
    let g = Graph::build(pools).unwrap();
    let edge = g.adjacency[&p.token0]
        .iter()
        .find(|e| e.pool_id == p.pool_id && e.to == p.token1)
        .unwrap()
        .clone();
    // Deliberately stale proposal amounts: the allocator must recompute them.
    PathQuote {
        edges: vec![edge],
        tokens: vec![p.token0.clone(), p.token1.clone()],
        amount_in: amount("1"),
        amount_out: amount("1"),
    }
}
fn route(path: &PathQuote, input: &Amount) -> AllocatedRoute {
    AllocatedRoute {
        path: path.clone(),
        amount_in: input.clone(),
        amount_out: amount("1"),
    }
}
fn unchanged_bpt(before: &Pool, after: &Pool) {
    let a = m(before);
    let b = m(after);
    assert_eq!(
        (
            &a.total_supply,
            &a.virtual_supply,
            &a.actual_supply,
            &a.pending_aum,
            a.last_collection_timestamp,
            &a.manager_bpt_balance,
            &a.protocol_collector_bpt_balance,
            &a.vault_bpt_cash,
            &a.vault_bpt_managed,
            &a.vault_bpt_actual_balance
        ),
        (
            &b.total_supply,
            &b.virtual_supply,
            &b.actual_supply,
            &b.pending_aum,
            b.last_collection_timestamp,
            &b.manager_bpt_balance,
            &b.protocol_collector_bpt_balance,
            &b.vault_bpt_cash,
            &b.vault_bpt_managed,
            &b.vault_bpt_actual_balance
        )
    );
}
fn replay(pools: &[Pool], allocation: &Allocation) -> BTreeMap<String, Pool> {
    let mut state: BTreeMap<_, _> = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    let mut total = Amount::zero();
    for r in &allocation.routes {
        let edge = &r.path.edges[0];
        let p = &state[&edge.pool_id];
        let s = m(p);
        let q = quote_at(
            p,
            &r.amount_in,
            &r.path.tokens[0],
            r.path.tokens.last().unwrap(),
            s.block_number,
            s.timestamp,
        )
        .unwrap();
        assert_eq!(r.amount_out, q.amount_out);
        assert_eq!(r.path.amount_out, q.amount_out);
        assert_eq!(r.path.amount_in, r.amount_in);
        let actual = evaluate_path(&mut state, &r.path.edges, &r.amount_in).unwrap();
        assert_eq!(actual, q.amount_out);
        assert_eq!(state[&edge.pool_id], q.next_pool);
        total.0 += &actual.0;
    }
    assert_eq!(allocation.gross_buy_amount, total);
    assert_eq!(allocation.refund, Amount::zero());
    for p in pools {
        unchanged_bpt(p, &state[&p.pool_id]);
    }
    state
}

#[test]
fn allocation_ab_uses_original_raw_outputs_and_carries_the_global_wallet() {
    let pools = pools();
    let saved = pools.clone();
    let routes: Vec<_> = (0..2)
        .map(|i| {
            route(
                &path(&pools, i),
                &amount(
                    bank()["cases"][i]["expected_native"]["amount_in"]
                        .as_str()
                        .unwrap(),
                ),
            )
        })
        .collect();
    let a = evaluate_allocation(&pools, &routes, &amount("1")).unwrap();
    let mut sum = Amount::zero();
    for (i, r) in a.routes.iter().enumerate() {
        let raw = &bank()["cases"][i]["expected_native"];
        assert_eq!(
            r.amount_out.to_string(),
            raw["amount_out"].as_str().unwrap()
        );
        assert_eq!(r.amount_in.to_string(), raw["amount_in"].as_str().unwrap());
        sum.0 += &r.amount_out.0;
    }
    assert_eq!(a.sell_amount, amount("200000000000000000000"));
    assert_eq!(a.gross_buy_amount, sum);
    assert_eq!(a.net_buy_amount.0, &sum.0 - 2u8);
    let state = replay(&pools, &a);
    for p in &pools {
        for (index, info) in m(&state[&p.pool_id]).token_info.iter().enumerate() {
            let original = &m(p).token_info[index].vault_actual_balance.0;
            let expected = if index == 0 {
                original + &a.sell_amount.0
            } else {
                original - &sum.0
            };
            assert_eq!(info.vault_actual_balance.0, expected);
        }
    }
    assert_eq!(pools, saved);
}

#[test]
fn repeated_physical_route_recomputes_the_second_fill_in_integer_state() {
    let pools = pools();
    let p = path(&pools, 0);
    let input = amount("100000000000000000000");
    let routes = vec![route(&p, &input), route(&p, &input)];
    let a = evaluate_allocation(&pools, &routes, &Amount::zero()).unwrap();
    assert!(a.routes[1].amount_out < a.routes[0].amount_out);
    let state = replay(&pools, &a);
    assert_eq!(
        m(&state[&pools[1].pool_id]).token_info[0].cash,
        m(&pools[1]).token_info[0].cash
    );
    assert_eq!(
        m(&state[&pools[1].pool_id]).token_info[1].cash,
        m(&pools[1]).token_info[1].cash
    );
    assert_ne!(
        m(&state[&pools[1].pool_id]).token_info[0].vault_actual_balance,
        m(&pools[1]).token_info[0].vault_actual_balance
    );
}

#[test]
fn bounded_optimizer_matches_integer_grid_and_finds_capacity_fallback_among_abc() {
    let mut pools = pools();
    // A synthetic third physical registration; the original A/B weights remain
    // 80/20 and 50/50. No test-only pricing model is substituted.
    let mut c = pools[1].clone();
    c.pool_id = "0xfffffffffffffffffffffffffffffffffffffffc".into();
    c.multi_asset.as_mut().unwrap().pool_id = format!("{}000100000000000000000002", c.pool_id);
    c.validate().unwrap();
    let funding = m(&c)
        .token_info
        .iter()
        .map(|info| info.cash.clone())
        .collect::<Vec<_>>();
    pools.push(c);
    for pool in &mut pools {
        for (index, info) in pool
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap()
            .token_info
            .iter_mut()
            .enumerate()
        {
            info.vault_actual_balance.0 += &funding[index].0;
        }
    }
    let input = amount("5000000000000000000000");
    for p in &pools {
        let error = quote_at(
            p,
            &input,
            &p.token0,
            &p.token1,
            m(p).block_number,
            m(p).timestamp,
        )
        .expect_err("full order exceeds this pool's capacity");
        assert_eq!(error.to_string(), "MANAGED_V2_MAX_IN_RATIO");
    }
    // Feed real one-hop physical paths directly into the optimizer. Every
    // full-order allocation fails capacity; feasible split proposals must
    // survive those failures. Candidate search has its own routing target.
    let candidates: Vec<_> = (0..pools.len()).map(|i| path(&pools, i)).collect();
    // Independent tiny grid of direct integer pool quotes. Physical A/B/C
    // registrations have separate cash; their common wallet is replayed below.
    let mut scores = Vec::new();
    for i in 0..pools.len() {
        for j in i + 1..pools.len() {
            for part in 1..10u64 {
                let first = Amount(&input.0 * part / 10u8);
                let second = Amount(&input.0 - &first.0);
                let q = |index: usize, a: &Amount| {
                    let p = &pools[index];
                    quote_at(
                        p,
                        a,
                        &p.token0,
                        &p.token1,
                        m(p).block_number,
                        m(p).timestamp,
                    )
                };
                if let (Ok(a), Ok(b)) = (q(i, &first), q(j, &second)) {
                    scores.push(Amount(a.amount_out.0 + b.amount_out.0));
                }
            }
        }
    }
    assert!(scores.len() > 2);
    assert!(scores.iter().min() < scores.iter().max());
    let allocation = optimize(&pools, &candidates, &input, 10, 2, &Amount::zero()).unwrap();
    assert_eq!(&allocation.gross_buy_amount, scores.iter().max().unwrap());
    let coarse = optimize(&pools, &candidates, &input, 2, 2, &Amount::zero()).unwrap();
    assert!(
        allocation.gross_buy_amount > coarse.gross_buy_amount,
        "the finer split_parts grid must improve on the equal split for these raw weights"
    );
    assert_eq!(allocation.sell_amount, input);
    assert_eq!(allocation.routes.len(), 2);
    assert_ne!(
        allocation.routes[0].amount_in,
        allocation.routes[1].amount_in
    );
    let saved = pools.clone();
    let state = replay(&pools, &allocation);
    for p in &pools {
        let s = m(&state[&p.pool_id]);
        assert_eq!(
            s.token_info[0].vault_actual_balance.0,
            &m(p).token_info[0].vault_actual_balance.0 + &input.0
        );
        assert_eq!(
            s.token_info[1].vault_actual_balance.0,
            &m(p).token_info[1].vault_actual_balance.0 - &allocation.gross_buy_amount.0
        );
    }
    assert_eq!(pools, saved);
}
