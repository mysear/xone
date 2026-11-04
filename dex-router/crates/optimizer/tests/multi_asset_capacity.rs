use xone_domain::{Amount, Pool};
use xone_optimizer::optimize;
use xone_routing::{search, Budget, Graph};
#[test]
fn two_individually_small_weighted_or_managed_pools_fill_the_complete_order() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let base: Pool = serde_json::from_value(data["pools"][0].clone()).unwrap();
    let amount: Amount = "500000000000000000000000".parse().unwrap();
    let half = Amount::checked(&amount.0 / 2u8).unwrap();
    for managed in [false, true] {
        let mut p = base.clone();
        if managed {
            p.protocol = "balancer-v2-managed-snapshot".into();
            p.adapter_version = "balancer-v2-managed-snapshot/no-bpt-fees-1".into();
        }
        let mut second = p.clone();
        second.pool_id = "0x0000000000000000000000000000000000000006".into();
        second.multi_asset.as_mut().unwrap().pool_id =
            format!("{}0001{}1", second.pool_id, "0".repeat(19));
        let pools = vec![p, second];
        let graph = Graph::build(&pools).unwrap();
        let candidates = search(
            &graph,
            &pools,
            &pools[0].token0,
            &pools[0].token1,
            &amount,
            Budget {
                max_hops: 1,
                max_candidates: 8,
                max_expansions: 100,
                split_parts: 2,
            },
        )
        .unwrap();
        let result = optimize(
            &pools,
            &candidates.candidates,
            &amount,
            2,
            2,
            &Amount::zero(),
        )
        .unwrap();
        assert_eq!(result.sell_amount, amount);
        assert_eq!(result.refund, Amount::zero());
        assert_eq!(result.routes.len(), 2);
        assert!(result.routes.iter().all(|r| r.amount_in == half));
        let native = xone_protocol_math::quote_exact_in(&pools[0], &half, true).unwrap();
        assert_eq!(result.gross_buy_amount.0, native.amount_out.0 * 2u8);
    }
}
