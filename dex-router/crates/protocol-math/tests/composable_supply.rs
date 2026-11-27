use num_bigint::BigUint;
use serde::Deserialize;
use xone_protocol_math::balancer_composable::{
    supply_view, CachedRate, SupplyInput, YieldFeePolicy,
};

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Rate {
    current: String,
    old: String,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    balances: Vec<String>,
    scales: Vec<String>,
    bpt_index: usize,
    rates: Vec<Option<Rate>>,
    exempt: bool,
    total_supply: String,
    last_join_exit_amp: String,
    last_post_join_exit_invariant: String,
    swap_fee: String,
    yield_fee: String,
    virtual_supply: String,
    pending_protocol_bpt: String,
    actual_supply: String,
}

fn amount(value: &str) -> BigUint {
    value.parse().unwrap()
}

#[test]
fn v6_actual_supply_matches_original_pool() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/composable-v6-supply.json")).unwrap();
    assert_eq!(corpus.cases.len(), 12);
    for case in corpus.cases {
        let rates = case
            .rates
            .iter()
            .map(|rate| {
                rate.as_ref().map(|rate| CachedRate {
                    current: amount(&rate.current),
                    old: amount(&rate.old),
                })
            })
            .collect::<Vec<_>>();
        let actual = supply_view(&SupplyInput {
            balances: &case.balances.iter().map(|v| amount(v)).collect::<Vec<_>>(),
            scales: &case.scales.iter().map(|v| amount(v)).collect::<Vec<_>>(),
            bpt_index: case.bpt_index,
            rates: &rates,
            yield_fee_policy: YieldFeePolicy::PoolWide {
                exempt: case.exempt,
            },
            total_supply: &amount(&case.total_supply),
            last_join_exit_amp: &amount(&case.last_join_exit_amp),
            last_post_join_exit_invariant: &amount(&case.last_post_join_exit_invariant),
            protocol_swap_fee: &amount(&case.swap_fee),
            protocol_yield_fee: &amount(&case.yield_fee),
        })
        .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert_eq!(
            actual.virtual_supply,
            amount(&case.virtual_supply),
            "{}",
            case.name
        );
        assert_eq!(
            actual.pending_protocol_bpt,
            amount(&case.pending_protocol_bpt),
            "{}",
            case.name
        );
        assert_eq!(
            actual.actual_supply,
            amount(&case.actual_supply),
            "{}",
            case.name
        );
    }
}
