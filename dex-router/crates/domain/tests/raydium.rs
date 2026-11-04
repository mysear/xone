use xone_domain::{raydium::RaydiumCPMMState, Pool};
fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/raydium-state.json")).unwrap()
}
#[test]
fn raydium_creator_metadata_and_net_reserves_are_strict() {
    let p = pool();
    p.validate().unwrap();
    let state = p.raydium_cpmm.unwrap();
    assert_eq!(
        RaydiumCPMMState::from_proto(state.to_proto().unwrap()).unwrap(),
        state
    );
    let mut p = pool();
    p.raydium_cpmm.as_mut().unwrap().creator_fee_on = 3;
    assert!(p.validate().is_err());
    let mut p = pool();
    p.raydium_cpmm.as_mut().unwrap().raw_vault0 = 0;
    assert!(p.validate().is_err());
    let mut p = pool();
    p.protocol = "fixture-cpmm".into();
    assert!(p.validate().is_err());
    let mut p = pool();
    p.chain_key = "evm:31337".into();
    assert!(p.validate().is_err());
}
