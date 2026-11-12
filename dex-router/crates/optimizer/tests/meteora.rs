use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, optimize, AllocatedRoute};
use xone_routing::{Edge, PathQuote};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/meteora-mm.json")).unwrap()
}
fn path(p: &Pool, step: &serde_json::Value) -> PathQuote {
    let zero = step["zero_for_one"].as_bool().unwrap();
    let (a, b) = if zero {
        (&p.token0, &p.token1)
    } else {
        (&p.token1, &p.token0)
    };
    PathQuote {
        amount_in: step["input"].as_str().unwrap().parse().unwrap(),
        amount_out: step["output"].as_str().unwrap().parse().unwrap(),
        tokens: vec![a.clone(), b.clone()],
        edges: vec![Edge {
            from_token: None,
            protocol: p.protocol.clone(),
            pool_id: p.pool_id.clone(),
            zero_for_one: zero,
            to: b.clone(),
        }],
    }
}
#[test]
fn shared_bin_ledger_replays_original_consecutive_amounts_in_order() {
    let f = data();
    let mut sequences = 0;
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["steps"].as_array().unwrap().len() == 3)
    {
        let p: Pool = serde_json::from_value(c["steps"][0]["pool"].clone()).unwrap();
        let routes = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let path = path(&p, s);
                AllocatedRoute {
                    amount_in: path.amount_in.clone(),
                    amount_out: path.amount_out.clone(),
                    path,
                }
            })
            .collect::<Vec<_>>();
        let result = evaluate_allocation(&[p], &routes, &Amount::zero()).unwrap();
        let total = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["output"].as_str().unwrap().parse::<u64>().unwrap())
            .sum::<u64>();
        assert_eq!(result.gross_buy_amount.to_string(), total.to_string());
        assert_eq!(result.sell_amount.to_string(), "1030000");
        assert_eq!(result.refund, Amount::zero());
        sequences += 1;
    }
    assert_eq!(sequences, 10);
}
#[test]
fn optimization_recomputes_stale_proposal_output_and_conserves_integer_input() {
    let f = data();
    let s = &f["cases"][3]["steps"][0];
    let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
    let mut proposal = path(&p, s);
    proposal.amount_out = "999999999999999".parse().unwrap();
    let q = optimize(
        &[p],
        &[proposal],
        &"1000000".parse().unwrap(),
        8,
        1,
        &Amount::zero(),
    )
    .unwrap();
    assert_eq!(
        q.gross_buy_amount.to_string(),
        s["output"].as_str().unwrap()
    );
    assert_eq!(q.sell_amount.to_string(), "1000000");
    assert_eq!(q.refund, Amount::zero());
}
