use serde_json::Value;
use xone_domain::Pool;
#[test]
fn only_y_and_input_only_policies_cannot_be_relabelled_on_the_same_raw_bank() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/meteora-only-y.json")).unwrap();
    assert_eq!(f["cases"].as_array().unwrap().len(), 236);
    for c in f["cases"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(c["steps"][0]["pool"].clone()).unwrap();
        p.validate().unwrap();
        p.protocol = xone_domain::meteora::PROFILE.into();
        p.adapter_version = xone_domain::meteora::ADAPTER.into();
        assert!(p.validate().is_err());
    }
}
