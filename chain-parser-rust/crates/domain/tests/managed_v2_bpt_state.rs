use prost::Message;
use serde_json::{json, Value};
use xone_domain::{
    managed_v2_bpt_state::{ManagedV2BptState, ADAPTER_VERSION, PROTOCOL},
    multi_asset::MultiAssetState,
    wire, Pool,
};
fn fixture() -> Pool {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-scheduled-wire.json");
    let v: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let mut p: Pool = serde_json::from_value(v["snapshot"]["pools"][0].clone()).unwrap();
    p.protocol = PROTOCOL.into();
    p.adapter_version = ADAPTER_VERSION.into();
    let schedule = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .schedule
        .as_ref()
        .unwrap()
        .clone();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.bpt=Some(serde_json::from_value(json!({"join_exit_enabled":true,"minimum_bpt_locked":"1000000","bpt_last_change_block":0,"allowlist_members":[{"address":"0x0000000000000000000000000000000000000001","allowed":false}]})).unwrap());
    assert_eq!(m.schedule.as_ref().unwrap(), &schedule);
    p
}
#[test]
fn bpt_profile_is_separate_and_requires_schedule_and_complete_control_state() {
    let p = fixture();
    p.validate().unwrap();
    assert_eq!(
        xone_domain::protocols::protocol(PROTOCOL).unwrap().adapter,
        ADAPTER_VERSION
    );
    let mut q = p.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .bpt = None;
    assert!(q.validate().is_err());
    let mut q = p.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .schedule = None;
    assert!(q.validate().is_err());
    for old in [
        xone_domain::managed_v2_state::PROTOCOL,
        xone_domain::managed_v2_schedule::PROTOCOL,
    ] {
        let mut q = p.clone();
        q.protocol = old.into();
        q.adapter_version = xone_domain::protocols::protocol(old)
            .unwrap()
            .adapter
            .into();
        assert!(
            q.validate().is_err(),
            "old versions reject new BPT capability"
        );
    }
}
#[test]
fn required_json_scalars_and_amounts_cannot_be_null_absent_or_unknown() {
    let p = fixture();
    let value =
        serde_json::to_value(p.multi_asset.unwrap().managed_v2.unwrap().bpt.unwrap()).unwrap();
    for key in [
        "join_exit_enabled",
        "minimum_bpt_locked",
        "bpt_last_change_block",
        "allowlist_members",
    ] {
        let mut x = value.clone();
        x.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<ManagedV2BptState>(x).is_err(),
            "missing {key}"
        );
        let mut x = value.clone();
        x[key] = Value::Null;
        assert!(
            serde_json::from_value::<ManagedV2BptState>(x).is_err(),
            "null {key}"
        );
    }
    let mut x = value;
    x["unexpected"] = json!(false);
    assert!(serde_json::from_value::<ManagedV2BptState>(x).is_err());
}
#[test]
fn bpt_control_words_membership_and_wire_presence_are_exact() {
    let p = fixture();
    let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    let b = m.bpt.as_ref().unwrap();
    assert!(!b
        .member_allowed("0x0000000000000000000000000000000000000001")
        .unwrap());
    assert!(b
        .member_allowed("0x0000000000000000000000000000000000000002")
        .is_err());
    let mut q = p.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .bpt
        .as_mut()
        .unwrap()
        .join_exit_enabled = false;
    assert!(q.validate().is_err(), "raw bit255 matches explicit state");
    let mut q = p.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .bpt
        .as_mut()
        .unwrap()
        .minimum_bpt_locked = "0".parse().unwrap();
    assert!(q.validate().is_err(), "fixed locked minimum");
    let mut q = p.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .bpt
        .as_mut()
        .unwrap()
        .bpt_last_change_block = u32::MAX as u64 + 1;
    assert!(q.validate().is_err());
    let multi = p.multi_asset.as_ref().unwrap();
    let proto = multi.to_proto().unwrap();
    let bytes = proto.encode_to_vec();
    assert_eq!(
        MultiAssetState::from_proto(wire::MultiAssetState::decode(bytes.as_slice()).unwrap())
            .unwrap(),
        *multi
    );
    let mut proto = b.to_proto().unwrap();
    proto.join_exit_enabled = None;
    assert!(ManagedV2BptState::from_proto(proto).is_err());
    let mut proto = b.to_proto().unwrap();
    proto.minimum_bpt_locked = None;
    assert!(ManagedV2BptState::from_proto(proto).is_err());
    let mut proto = b.to_proto().unwrap();
    proto.bpt_last_change_block = None;
    assert!(ManagedV2BptState::from_proto(proto).is_err());
}
#[test]
fn membership_order_uniqueness_and_execution_derived_isolation_hold() {
    let p = fixture();
    let mut q = p.clone();
    let b = q
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .bpt
        .as_mut()
        .unwrap();
    b.allowlist_members.push(b.allowlist_members[0].clone());
    assert!(q.validate().is_err());
    let mut q = p;
    let m = q.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.execution_context = Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
        block_number: m.block_number,
        timestamp: m.timestamp,
    });
    q.validate().unwrap();
    assert!(q.multi_asset.as_ref().unwrap().to_proto().is_err());
}
