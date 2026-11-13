use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::Pool;
fn collect(value: &Value, found: &mut BTreeMap<String, Pool>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect(item, found);
            }
        }
        Value::Object(items) => {
            if items.contains_key("protocol")
                && items.contains_key("reserve0")
                && items.contains_key("adapter_version")
            {
                if let Ok(pool) = serde_json::from_value::<Pool>(value.clone()) {
                    if pool.validate().is_ok() {
                        found.entry(pool.protocol.clone()).or_insert(pool);
                    }
                }
            }
            for item in items.values() {
                collect(item, found);
            }
        }
        _ => {}
    }
}
#[test]
fn capability_labels_cover_existing_versioned_fixture_models_and_reject_unknowns() {
    let directory =
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/contracts"));
    let mut found = BTreeMap::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json")
            && path.metadata().unwrap().len() < 8_000_000
        {
            let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            collect(&value, &mut found);
        }
    }
    assert!(found.len() > 35, "only {} models", found.len());
    for (protocol, pool) in &found {
        let c = xone_protocol_math::capabilities::describe(pool).unwrap();
        assert_eq!(&c.protocol, protocol);
        assert_eq!(c.adapter_version, pool.adapter_version);
        assert_eq!(
            c.quote_modes.iter().any(|m| m == "EXACT_OUT"),
            xone_protocol_math::exact_out::supports_protocol(protocol)
        );
        assert_eq!(
            c.execution_preparation,
            xone_domain::meteora::supports_profile(protocol)
        );
        assert!(!c.wallet_submission && !c.independent_simulation);
    }
    let mut unknown = found
        .values()
        .find(|p| p.protocol == "uniswap-v2")
        .unwrap()
        .clone();
    unknown.protocol = "unknown-hook".into();
    unknown.adapter_version = "unknown/1".into();
    unknown.v2 = None;
    let c = xone_protocol_math::capabilities::describe(&unknown).unwrap();
    assert!(c.quote_modes.is_empty() && !c.unsigned_transaction && c.execution_blocker.is_some());
}

#[test]
fn protocol_catalog_is_complete_versioned_and_does_not_invent_live_views() {
    let catalog = xone_protocol_math::capabilities::catalog();
    assert_eq!(catalog.len(), xone_domain::protocols::NAMES.len());
    for name in xone_domain::protocols::NAMES {
        let profile = xone_domain::protocols::protocol(name).unwrap();
        assert_eq!(catalog[*name].family, profile.family);
        assert_eq!(catalog[*name].adapter_version, profile.adapter);
        assert_eq!(
            catalog[*name].model_version,
            xone_protocol_math::capabilities::VERSION
        );
    }
}
