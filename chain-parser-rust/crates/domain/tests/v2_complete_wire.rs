use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Snapshot,
};
#[test]
fn independent_cpp_complete_wire_preserves_all_values_and_explicit_zero() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/v2-complete-wire.json"
    ))
    .unwrap();
    assert_eq!(f["cases"].as_array().unwrap().len(), 3);
    for c in f["cases"].as_array().unwrap() {
        let expected: Snapshot = serde_json::from_value(c["snapshot"].clone()).unwrap();
        let bytes = hex::decode(c["protobuf_hex"].as_str().unwrap()).unwrap();
        let wire = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
        assert_eq!(
            snapshot_from_proto(wire.clone()).unwrap(),
            expected,
            "{}",
            c["name"]
        );
        assert_eq!(snapshot_to_proto(&expected).unwrap(), wire, "{}", c["name"]);
    }
}
