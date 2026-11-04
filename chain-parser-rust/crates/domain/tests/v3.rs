use xone_domain::{v3::sqrt_ratio_at_tick, Pool};
#[test]
fn v3_bitmap_proves_each_initialized_tick_and_price() {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    p.validate().unwrap();
    p.v3.as_mut().unwrap().ticks.pop();
    assert_eq!(p.validate().unwrap_err().0, "V3_INCOMPLETE_TICKS");
}
#[test]
fn tick_math_matches_canonical_endpoints() {
    for (tick, expected) in [
        (0, "79228162514264337593543950336"),
        (-887272, "4295128739"),
        (887272, "1461446703485210103287273052203988822378723970342"),
    ] {
        assert_eq!(sqrt_ratio_at_tick(tick).unwrap().to_string(), expected);
    }
}

#[test]
fn minimum_initialized_state_roundtrips_in_shared_wire() {
    let p: Pool = serde_json::from_str(include_str!(
        "../../../test/contracts/v3-minimum-state.json"
    ))
    .unwrap();
    p.validate().unwrap();
    let v = p.v3.unwrap();
    let wire = v.to_proto().unwrap();
    let decoded = xone_domain::v3::V3State::from_proto(wire).unwrap();
    assert_eq!(decoded, v);
}

#[test]
fn optional_factory_digest_rejects_unreadable_wire_for_every_profile() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    for digest in ["aa".to_owned(), "z".repeat(64), "A".repeat(64)] {
        let mut v = p.v3.clone().unwrap();
        v.factory_code_sha256 = digest;
        assert!(v.validate().is_err(), "invalid factory digest accepted");
        assert!(v.to_proto().is_err(), "unreadable factory digest encoded");
    }
    for digest in [String::new(), "b".repeat(64)] {
        let mut v = p.v3.clone().unwrap();
        v.factory_code_sha256 = digest;
        let decoded = xone_domain::v3::V3State::from_proto(v.to_proto().unwrap()).unwrap();
        assert_eq!(decoded, v);
    }
}
