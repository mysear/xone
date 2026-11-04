use xone_domain::{singleton::SingletonCLState, Pool};
fn pools() -> Vec<Pool> {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/singleton-state.json")).unwrap();
    serde_json::from_value(f["pools"].clone()).unwrap()
}
#[test]
fn singleton_key_and_wire_are_independently_typed() {
    for p in pools() {
        p.validate().unwrap();
        let s = p.singleton_cl.unwrap();
        s.validate(&p.protocol, &p.token0, &p.token1).unwrap();
        let back = SingletonCLState::from_proto(s.to_proto().unwrap()).unwrap();
        assert_eq!(s, back)
    }
}
#[test]
fn singleton_cannot_be_relabelled_or_change_accounting() {
    for p in pools() {
        let mut wrong = p.clone();
        wrong.singleton_cl.as_mut().unwrap().hooks = format!("0x{}", "1".repeat(40));
        assert!(wrong.validate().is_err());
        let mut wrong = p.clone();
        wrong.protocol = "uniswap-v3".into();
        assert!(wrong.validate().is_err());
        let mut wrong = p.clone();
        wrong.singleton_cl.as_mut().unwrap().protocol_fee1 = 4001;
        assert!(wrong.validate().is_err());
        let mut wrong = p.clone();
        wrong.singleton_cl.as_mut().unwrap().pool_id = format!("0x{}", "a".repeat(64));
        assert!(wrong.validate().is_err());
        let mut wrong = p.clone();
        wrong.singleton_cl.as_mut().unwrap().core.ticks.pop();
        assert!(wrong.validate().is_err());
    }
}
