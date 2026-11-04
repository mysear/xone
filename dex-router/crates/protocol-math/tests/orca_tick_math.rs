use num_bigint::BigUint;
use xone_domain::orca_math;

#[test]
fn original_orca_q64_vectors_and_bounds() {
    for (tick, expected) in [
        (-443636, "4295048016"),
        (-1, "18445821805675392311"),
        (0, "18446744073709551616"),
        (1, "18447666387855959850"),
        (443636, "79226673515401279992447579055"),
    ] {
        let p: BigUint = expected.parse().unwrap();
        assert_eq!(orca_math::sqrt_price_from_tick(tick).unwrap(), p);
        assert_eq!(orca_math::tick_from_sqrt_price(&p).unwrap(), tick);
    }
    assert!(orca_math::sqrt_price_from_tick(-443637).is_err());
    assert!(orca_math::sqrt_price_from_tick(443637).is_err());
    assert!(orca_math::sqrt_price_from_tick(i32::MIN).is_err());
    assert!(orca_math::tick_from_sqrt_price(&BigUint::from(4295048015u64)).is_err());
    assert!(
        orca_math::tick_from_sqrt_price(&("79226673515401279992447579056".parse().unwrap()))
            .is_err()
    );
}

#[test]
fn inverse_floors_original_orca_prices() {
    for tick in (-443635..443635).step_by(997) {
        let p = orca_math::sqrt_price_from_tick(tick).unwrap();
        assert_eq!(orca_math::tick_from_sqrt_price(&p).unwrap(), tick);
        let before_next = orca_math::sqrt_price_from_tick(tick + 1).unwrap() - 1u32;
        assert_eq!(orca_math::tick_from_sqrt_price(&before_next).unwrap(), tick);
    }
}
