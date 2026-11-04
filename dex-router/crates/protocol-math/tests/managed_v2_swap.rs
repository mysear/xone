use num_bigint::BigUint;
use xone_protocol_math::managed_v2_swap::{quote_regular, WeightedInputs};
fn n(s: &str) -> BigUint {
    BigUint::parse_bytes(s.as_bytes(), 10).unwrap()
}
fn one() -> BigUint {
    n("1000000000000000000")
}
#[test]
fn original_deployed_external_weighted_math_and_real_vault_swap_match() {
    let cash = [n("10000000000000000000000"), n("20000000000000000000000")];
    let managed = [n("0"), n("0")];
    let weights = [n("800000000000000000"), n("200000000000000000")];
    let scales = [one(), one()];
    let fee = n("10000000000000000");
    let input = n("100000000000000000000");
    let q = quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &fee,
        },
        &input,
        0,
        1,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("772779500379197940000"));
    assert_eq!(q.next_cash[0], n("10100000000000000000000"));
    assert_eq!(q.next_cash[1], n("19227220499620802060000"));
    assert_eq!(q.next_managed, managed);
}
#[test]
fn fee_after_upscale_preserves_fractional_raw_input_for_low_precision_token() {
    let cash = [n("1000000"), n("2000000000000000000")];
    let managed = [n("0"), n("0")];
    let weights = [n("500000000000000000"), n("500000000000000000")];
    let scales = [n("1000000000000000000000000000000"), one()];
    let fee = n("10000000000000000");
    let q = quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &fee,
        },
        &n("1"),
        0,
        1,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("1979998039800"));
    assert_eq!(q.next_cash[0], n("1000001"));
    assert!(
        xone_protocol_math::balancer_weighted::quote_exact_in_weighted(
            [&cash[0], &cash[1]],
            [&scales[0], &scales[1]],
            [&weights[0], &weights[1]],
            &fee,
            &n("1"),
            true
        )
        .is_err(),
        "historical raw-fee version must not silently change"
    );
}
#[test]
fn fifty_underlying_tokens_and_managed_cash_are_independent() {
    let cash = vec![n("1000000000000000000000"); 50];
    let managed = vec![n("1000000000000000000000"); 50];
    let weights = vec![n("20000000000000000"); 50];
    let scales = vec![one(); 50];
    let fee = n("10000000000000000");
    let q = quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &fee,
        },
        &n("1000000000000000000"),
        0,
        49,
    )
    .unwrap();
    assert_eq!(q.next_managed, managed);
    assert_eq!(q.next_cash[0], n("1001000000000000000000"));
    assert_eq!(q.next_cash[1], cash[1]);
    assert!(q.next_cash[49] < cash[49]);
}
#[test]
fn actual_vault_cash_and_total_u112_and_selected_net_input_limit_are_enforced() {
    let mut cash = [n("1000000000000000000000"), n("1000000000000000000")];
    let mut managed = [n("0"), n("1999000000000000000000")];
    let weights = [n("500000000000000000"), n("500000000000000000")];
    let scales = [one(), one()];
    let fee = n("10000000000000000");
    assert!(
        quote_regular(
            WeightedInputs {
                cash: &cash,
                managed: &managed,
                weights: &weights,
                scales: &scales,
                fee: &fee
            },
            &n("100000000000000000000"),
            0,
            1
        )
        .is_err(),
        "total balance must not hide insufficient Vault cash"
    );
    managed[1] = n("0");
    cash[1] = n("2000000000000000000000");
    assert!(quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &fee
        },
        &n("400000000000000000000"),
        0,
        1
    )
    .is_err());
    cash[0] = (BigUint::from(1u8) << 112usize) - BigUint::from(1u8);
    assert!(quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &fee
        },
        &n("100000000000000000000"),
        0,
        1
    )
    .is_err());
}
