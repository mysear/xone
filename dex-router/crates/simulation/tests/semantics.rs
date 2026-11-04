use xone_domain::*;
use xone_execution_plan::*;
use xone_optimizer::*;
use xone_routing::*;
#[test]
fn no_backend_never_returns_verified_success() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let input = "10000".parse().unwrap();
    let candidates = search(
        &Graph::build(&s.pools).unwrap(),
        &s.pools,
        "A",
        "B",
        &input,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 100,
        },
    )
    .unwrap();
    let a = optimize(
        &s.pools,
        &candidates.candidates,
        &input,
        10,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let p = build(
        &Context {
            chain_key: "evm:31337".into(),
            caller: "caller".into(),
            recipient: "recipient".into(),
            sell_token: "A".into(),
            buy_token: "B".into(),
            slippage_bps: 50,
            deadline_ms: 1000,
        },
        &a,
        &s.reference,
        "policy",
    )
    .unwrap();
    assert_eq!(
        xone_simulation::verify(&p).unwrap_err().0,
        "INDEPENDENT_SIMULATION_UNAVAILABLE"
    );
}
