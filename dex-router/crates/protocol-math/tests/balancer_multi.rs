use balancer_multi::quote_exact_in;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_protocol_math::balancer_multi;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
fn e() -> BigUint {
    n("1000000000000000000")
}
fn state() -> (Vec<BigUint>, Vec<BigUint>, Vec<BigUint>) {
    (
        vec![n("1000000000000000000000000"); 3],
        vec![e() / 4u8, e() / 4u8, e() / 2u8],
        vec![e(); 3],
    )
}
#[test]
fn pair_rounding_and_gross_input_preserve_unselected_assets() {
    let (b, w, s) = state();
    let q = quote_exact_in(
        &b,
        &w,
        &s,
        &n("1000000000000000"),
        &n("1000000000000000000000"),
        0,
        1,
    )
    .unwrap();
    // Same pair and exact operation order as the previously verified 50/50 path.
    let net = n("999000000000000000000");
    let base = (&b[0] * e() - BigUint::one()) / (&b[0] + &net) + BigUint::one();
    let expected = &b[1] * (e() - base) / e();
    assert_eq!(q.amount_out, expected);
    assert_eq!(q.next_balances[0], &b[0] + n("1000000000000000000000"));
    assert_eq!(q.next_balances[1], &b[1] - q.amount_out);
    assert_eq!(q.next_balances[2], b[2]);
    assert_eq!(b, state().0);
}
#[test]
fn bounds_reject_bad_dimensions_indices_weights_scales_and_arithmetic() {
    let (b, w, s) = state();
    let fee = BigUint::zero();
    let input = n("1000");
    let run = |b: &[BigUint], w: &[BigUint], s: &[BigUint], i, j| {
        quote_exact_in(b, w, s, &fee, &input, i, j)
    };
    assert!(run(&b[..2], &w[..2], &s[..2], 0, 1).is_err());
    assert!(run(&b, &w[..2], &s, 0, 1).is_err());
    assert!(run(&b, &w, &s[..2], 0, 1).is_err());
    assert!(run(&b, &w, &s, 0, 0).is_err());
    assert!(run(&b, &w, &s, 3, 0).is_err());
    assert!(run(&b, &w, &s, 0, usize::MAX).is_err());
    assert!(run(&vec![e(); 21], &vec![e() / 21u8; 21], &vec![e(); 21], 0, 1).is_err());
    let mut bad = w.clone();
    bad[0] += 1u8;
    assert!(run(&b, &bad, &s, 0, 1).is_err());
    bad[0] = BigUint::one();
    assert!(run(&b, &bad, &s, 0, 1).is_err());
    let mut bad = s.clone();
    bad[2] += 1u8;
    assert!(run(&b, &w, &bad, 0, 1).is_err());
    let mut bad = b.clone();
    bad[2] = BigUint::one() << 112usize;
    assert!(run(&bad, &w, &s, 0, 1).is_err());
    bad[2] = BigUint::zero();
    assert!(run(&bad, &w, &s, 0, 1).is_err());
    assert!(quote_exact_in(&b, &w, &s, &(e() + 1u8), &input, 0, 1).is_err());
    assert!(quote_exact_in(&b, &w, &s, &fee, &BigUint::zero(), 0, 1).is_err());
    assert!(quote_exact_in(&b, &w, &s, &fee, &(BigUint::one() << 256usize), 0, 1).is_err());
    assert!(quote_exact_in(&b, &w, &s, &fee, &(&b[0] * 3u8 / 10u8 + 1u8), 0, 1).is_err());
    let mut max = b.clone();
    max[0] = (BigUint::one() << 112usize) - 1u8;
    assert!(run(&max, &w, &s, 0, 1).is_err());
}
#[test]
fn large_weight_ratio_has_no_spurious_max_output_limit() {
    let (b, _, s) = state();
    let w = vec![e() * 98u8 / 100u8, e() / 100u8, e() / 100u8];
    let q = quote_exact_in(&b, &w, &s, &BigUint::zero(), &(&b[0] * 3u8 / 10u8), 0, 1).unwrap();
    assert!(q.amount_out * 10u8 > b[1].clone() * 3u8);
}
#[test]
fn managed_snapshot_uses_observed_weights_without_static_renormalization() {
    use balancer_multi::{quote_managed_snapshot, ManagedSwapGuards};
    let (b, mut w, s) = state();
    w[0] -= 137u16;
    w[2] -= 419u16;
    let guards = ManagedSwapGuards {
        swaps_enabled: true,
        protocol_swap_fee: BigUint::zero(),
        management_swap_fee: BigUint::zero(),
    };
    let q = quote_managed_snapshot(
        &b,
        &w,
        &s,
        &BigUint::zero(),
        &n("1000000000000000000000"),
        0,
        2,
        &guards,
    )
    .unwrap();
    assert!(q.amount_out > BigUint::zero());
    assert!(quote_exact_in(
        &b,
        &w,
        &s,
        &BigUint::zero(),
        &n("1000000000000000000000"),
        0,
        2
    )
    .is_err());
    let disabled = ManagedSwapGuards {
        swaps_enabled: false,
        ..guards.clone()
    };
    assert!(quote_managed_snapshot(&b, &w, &s, &BigUint::zero(), &e(), 0, 2, &disabled).is_err());
    for (protocol, management) in [
        (BigUint::one(), BigUint::zero()),
        (BigUint::zero(), BigUint::one()),
    ] {
        let fees = ManagedSwapGuards {
            swaps_enabled: true,
            protocol_swap_fee: protocol,
            management_swap_fee: management,
        };
        assert!(quote_managed_snapshot(&b, &w, &s, &BigUint::zero(), &e(), 0, 2, &fees).is_err());
    }
}
#[test]
fn managed_compressed_weights_use_timestamp_clamps_and_original_rounding() {
    use balancer_multi::managed_weights_at;
    let starts = [461168601842738u64, 461168601842738, 922337203685477];
    let ends = [922337203685477u64, 461168601842738, 461168601842738];
    let start = managed_weights_at(&starts, &ends, &e(), 100, 200, 99).unwrap();
    let mid = managed_weights_at(&starts, &ends, &e(), 100, 200, 150).unwrap();
    let end = managed_weights_at(&starts, &ends, &e(), 100, 200, 201).unwrap();
    assert_eq!(start[0], n("249999999999999571"));
    assert_eq!(end[0], n("499999999999999685"));
    assert_eq!(mid[0], n("374999999999999628"));
    assert_eq!(mid[2], n("374999999999999628"));
    assert_eq!(
        managed_weights_at(&starts, &ends, &e(), 100, 100, 100).unwrap(),
        end
    );
    assert!(managed_weights_at(&starts, &ends, &BigUint::zero(), 100, 200, 150).is_err());
    assert!(managed_weights_at(&starts, &ends, &e(), 200, 100, 150).is_err());
}
