//! Strict JSON parsing for untrusted JSON-RPC envelopes and nested results.
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;
use xone_domain::{err, Result};
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate object members")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(n.into()))
                    .ok_or_else(|| E::custom("INVALID_JSON_NUMBER"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut input: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(value)) = input.next_element()? {
                    values.push(value);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut input: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = input.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("DUPLICATE_JSON_MEMBER"));
                    }
                    let Unique(value) = input.next_value()?;
                    values.insert(key, value);
                }
                Ok(Unique(values.into()))
            }
        }
        decoder.deserialize_any(JsonVisitor)
    }
}
pub(super) fn parse(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|_| err("EVM_RPC_ENVELOPE"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_members_and_trailing_data_are_rejected() {
        for raw in [
            r#"{"id":1,"id":2}"#,
            r#"{"result":{"hash":"a","hash":"b"}}"#,
            r#"{"result":[{"key":true,"key":false}]}"#,
            r#"{"id":1} trailing"#,
        ] {
            assert_eq!(parse(raw.as_bytes()).unwrap_err().0, "EVM_RPC_ENVELOPE");
        }
        assert_eq!(
            parse(br#"{"id":1,"result":[null,true,2,-1,"x",1.5]}"#).unwrap(),
            serde_json::json!({"id":1,"result":[null,true,2,-1,"x",1.5]})
        );
    }
}
