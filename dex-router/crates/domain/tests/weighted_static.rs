use num_bigint::BigUint;
use xone_domain::{weighted::WeightedState, Pool};
#[test]
fn static_profiles_preserve_full_weights_wire_and_old_admission_matrix() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/weighted-static-state.json"
    ))
    .unwrap();
    let pools: Vec<Pool> = serde_json::from_value(data["pools"].clone()).unwrap();
    assert_eq!(pools.len(), 6);
    for p in pools {
        p.validate().unwrap();
        let s = p.weighted.as_ref().unwrap();
        let back = WeightedState::from_proto(s.to_proto().unwrap()).unwrap();
        assert_eq!(&back, s);
        let mut crossed = p.clone();
        crossed.adapter_version = "balancer-v2-weighted2/equal-1".into();
        assert!(crossed.validate().is_err());
        crossed.protocol = "balancer-v2-weighted2-equal".into();
        assert_eq!(
            crossed.validate().is_ok(),
            s.weight0.0 == BigUint::from(500_000_000_000_000_000u64)
        );
        let mut bad = p.clone();
        bad.weighted.as_mut().unwrap().weight0 = "9999999999999999".parse().unwrap();
        bad.weighted.as_mut().unwrap().weight1 = "990000000000000001".parse().unwrap();
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.weighted.as_mut().unwrap().weight0 = "10000000000000000".parse().unwrap();
        bad.weighted.as_mut().unwrap().weight1 = "990000000000000000".parse().unwrap();
        bad.validate().unwrap();
        bad.protocol = "balancer-v2-managed".into();
        assert!(bad.validate().is_err());
    }
}
