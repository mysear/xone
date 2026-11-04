use xone_domain::{raydium_clmm::RaydiumCLMMState, Pool};
fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap()
}
#[test]
fn rayclmm_token_fee_has_explicit_profile_and_wire_mode() {
    for mode in [1, 2] {
        let mut p = pool();
        p.protocol = "raydium-clmm-token-fee".into();
        p.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
        p.raydium_clmm.as_mut().unwrap().collect_fee_on = mode;
        p.validate().unwrap();
        let s = p.raydium_clmm.as_ref().unwrap();
        assert_eq!(
            &RaydiumCLMMState::from_proto(s.to_proto().unwrap()).unwrap(),
            s
        );
        p.protocol = "raydium-clmm-static".into();
        p.adapter_version = "raydium-clmm/6d7a0f82-spl-static-1".into();
        assert!(p.validate().is_err());
    }
    for mode in [0, 3, 255, 256] {
        let mut p = pool();
        p.protocol = "raydium-clmm-token-fee".into();
        p.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
        p.raydium_clmm.as_mut().unwrap().collect_fee_on = mode;
        assert!(p.validate().is_err());
    }
}
