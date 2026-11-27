use num_bigint::BigUint;
use serde::Deserialize;
use xone_protocol_math::balancer_composable::{
    quote_bpt_swap, refresh_token_rate_caches, supply_view, BptSwapKind, CachedRate, SupplyInput,
    TokenRateCache, YieldFeePolicy,
};

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
    bpt_swaps: Vec<Swap>,
    rejected_joins: usize,
    refreshes: Vec<Refresh>,
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

#[derive(Deserialize)]
struct Swap {
    before: Case,
    after: Case,
    kind: u8,
    i: usize,
    j: usize,
    amount: String,
    amp: String,
    pool_fee: String,
    amount_in: String,
    amount_out: String,
    mint_after_swap: String,
}

#[derive(Deserialize)]
struct Cache {
    current: String,
    old: String,
    duration: u32,
    expires: u32,
}

impl Cache {
    fn parsed(&self) -> TokenRateCache {
        TokenRateCache {
            rate: CachedRate {
                current: amount(&self.current),
                old: amount(&self.old),
            },
            duration: self.duration,
            expires: self.expires,
        }
    }
}

#[derive(Deserialize)]
struct Refresh {
    name: String,
    before: Vec<Option<Cache>>,
    after: Vec<Option<Cache>>,
    bpt: usize,
    decimals: Vec<String>,
    called: Vec<usize>,
    timestamp: u64,
    provider_rates: Vec<Option<String>>,
    scales: Vec<String>,
}

fn amount(value: &str) -> BigUint {
    value.parse().unwrap()
}

impl Case {
    fn rates(&self) -> Vec<Option<CachedRate>> {
        self.rates
            .iter()
            .map(|rate| {
                rate.as_ref().map(|rate| CachedRate {
                    current: amount(&rate.current),
                    old: amount(&rate.old),
                })
            })
            .collect()
    }

    fn with_supply_input<T>(&self, f: impl FnOnce(&SupplyInput<'_>) -> T) -> T {
        f(&SupplyInput {
            balances: &self.balances.iter().map(|v| amount(v)).collect::<Vec<_>>(),
            scales: &self.scales.iter().map(|v| amount(v)).collect::<Vec<_>>(),
            bpt_index: self.bpt_index,
            rates: &self.rates(),
            yield_fee_policy: YieldFeePolicy::PoolWide {
                exempt: self.exempt,
            },
            total_supply: &amount(&self.total_supply),
            last_join_exit_amp: &amount(&self.last_join_exit_amp),
            last_post_join_exit_invariant: &amount(&self.last_post_join_exit_invariant),
            protocol_swap_fee: &amount(&self.swap_fee),
            protocol_yield_fee: &amount(&self.yield_fee),
        })
    }
}

fn corpus() -> Corpus {
    serde_json::from_str(include_str!("fixtures/composable-v6-supply.json")).unwrap()
}

#[test]
fn v6_rate_cache_refresh_matches_original_vault() {
    let refreshes = corpus().refreshes;
    assert_eq!(refreshes.len(), 4);
    for case in refreshes {
        let original = case
            .before
            .iter()
            .map(|cache| cache.as_ref().map(Cache::parsed))
            .collect::<Vec<_>>();
        let decimal_scales = case.decimals.iter().map(|v| amount(v)).collect::<Vec<_>>();
        let mut called = Vec::new();
        let (caches, scales) =
            refresh_token_rate_caches(&decimal_scales, &original, case.bpt, case.timestamp, |i| {
                called.push(i);
                Ok(amount(case.provider_rates[i].as_ref().unwrap()))
            })
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert_eq!(called, case.called, "{}", case.name);
        assert_eq!(
            caches,
            case.after
                .iter()
                .map(|cache| cache.as_ref().map(Cache::parsed))
                .collect::<Vec<_>>(),
            "{}",
            case.name
        );
        assert_eq!(
            scales,
            case.scales.iter().map(|v| amount(v)).collect::<Vec<_>>(),
            "{}",
            case.name
        );
        let earliest = original
            .iter()
            .flatten()
            .map(|cache| cache.expires)
            .min()
            .unwrap();
        let (at_expiry, _) = refresh_token_rate_caches(
            &decimal_scales,
            &original,
            case.bpt,
            u64::from(earliest),
            |_| panic!("provider called at cache expiry"),
        )
        .unwrap();
        assert_eq!(at_expiry, original);
        let unchanged = original.clone();
        let failure = refresh_token_rate_caches(
            &decimal_scales,
            &original,
            case.bpt,
            u64::from(u32::MAX),
            |i| Ok(amount(case.provider_rates[i].as_ref().unwrap())),
        )
        .unwrap_err();
        assert!(failure
            .to_string()
            .contains("BALANCER_COMPOSABLE_CACHE_EXPIRY"));
        assert_eq!(original, unchanged);
    }
}

#[test]
fn v6_actual_supply_matches_original_pool() {
    let cases = corpus().cases;
    assert_eq!(cases.len(), 12);
    for case in cases {
        let actual = case
            .with_supply_input(supply_view)
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

#[test]
fn v6_bpt_swaps_match_original_vault() {
    let reference = corpus();
    assert_eq!(reference.rejected_joins, 3);
    let swaps = reference.bpt_swaps;
    assert_eq!(swaps.len(), 12);
    for swap in swaps {
        let kind = match swap.kind {
            0 => BptSwapKind::GivenIn,
            1 => BptSwapKind::GivenOut,
            other => panic!("swap kind {other}"),
        };
        let name = &swap.before.name;
        let actual = swap
            .before
            .with_supply_input(|input| {
                quote_bpt_swap(
                    input,
                    &amount(&swap.amp),
                    &amount(&swap.pool_fee),
                    &amount(&swap.amount),
                    swap.i,
                    swap.j,
                    kind,
                )
            })
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(actual.amount_in, amount(&swap.amount_in), "{name}");
        assert_eq!(actual.amount_out, amount(&swap.amount_out), "{name}");
        assert_eq!(
            actual.next_balances,
            swap.after
                .balances
                .iter()
                .map(|v| amount(v))
                .collect::<Vec<_>>(),
            "{name}"
        );
        assert_eq!(
            actual.next_total_supply,
            amount(&swap.after.total_supply),
            "{name}"
        );
        assert_eq!(
            actual.next_actual_supply_after_mint,
            amount(&swap.after.virtual_supply),
            "{name}"
        );
        assert_eq!(
            actual.protocol_bpt_before_swap,
            amount(&swap.before.pending_protocol_bpt),
            "{name}"
        );
        assert_eq!(
            actual.protocol_bpt_after_swap,
            amount(&swap.mint_after_swap),
            "{name}"
        );
        assert_eq!(
            actual.last_join_exit_amp,
            amount(&swap.after.last_join_exit_amp),
            "{name}"
        );
        assert_eq!(
            actual.last_join_exit_invariant,
            amount(&swap.after.last_post_join_exit_invariant),
            "{name}"
        );
        assert_eq!(actual.next_rates, swap.after.rates(), "{name}");
    }
}
