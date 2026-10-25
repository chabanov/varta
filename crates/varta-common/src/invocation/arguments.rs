use super::{ToolInvocationError, ToolValue};
use crate::{ToolContract, ToolType, abi};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use std::{collections::BTreeMap, fmt};

impl ToolContract {
    /// Parse one closed named-argument object against a validated contract.
    /// Integer syntax is deliberately exact: decimal/fraction/exponent forms
    /// are rejected rather than rounding an external number through f64.
    pub fn parse_arguments(&self, json: &[u8]) -> Result<Vec<ToolValue>, ToolInvocationError> {
        if json.len() > self.limits.input_bytes.min(abi::MAX_INPUT_BYTES) {
            return Err(ToolInvocationError::InvalidArguments(
                "JSON exceeds input byte limit".into(),
            ));
        }
        let mut deserializer = serde_json::Deserializer::from_slice(json);
        let arguments = (&mut deserializer)
            .deserialize_map(Arguments(self))
            .map_err(|e| ToolInvocationError::InvalidArguments(e.to_string()))?;
        deserializer
            .end()
            .map_err(|e| ToolInvocationError::InvalidArguments(e.to_string()))?;
        Ok(arguments)
    }
}

struct Arguments<'a>(&'a ToolContract);
impl<'de> Visitor<'de> for Arguments<'_> {
    type Value = Vec<ToolValue>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an object containing exactly the named tool parameters")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut values = BTreeMap::new();
        while let Some(name) = map.next_key::<String>()? {
            let parameter = self
                .0
                .parameters
                .iter()
                .find(|p| p.name == name)
                .ok_or_else(|| de::Error::custom("unknown tool parameter"))?;
            if values.contains_key(&name) {
                return Err(de::Error::custom("duplicate tool parameter"));
            }
            let value = map.next_value::<serde_json::Value>()?;
            let value = match (parameter.value_type, value) {
                (ToolType::Integer, serde_json::Value::Number(number)) => number
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .map(ToolValue::Integer),
                (ToolType::Boolean, serde_json::Value::Bool(value)) => {
                    Some(ToolValue::Boolean(value))
                }
                (ToolType::String, serde_json::Value::String(value))
                    if value.len() <= self.0.limits.string_bytes.min(abi::MAX_STRING_BYTES) =>
                {
                    Some(ToolValue::String(value))
                }
                _ => None,
            }
            .ok_or_else(|| {
                de::Error::custom(format!(
                    "invalid type, range or byte length for parameter {}",
                    parameter.name
                ))
            })?;
            values.insert(name, value);
        }
        self.0
            .parameters
            .iter()
            .map(|parameter| {
                values.remove(&parameter.name).ok_or_else(|| {
                    de::Error::custom(format!("missing tool parameter {}", parameter.name))
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ToolLimits, ToolParameter};

    fn contract() -> ToolContract {
        ToolContract {
            name: "choose".into(),
            description: "Choose".into(),
            export: "varta.tool.choose".into(),
            parameters: vec![
                ToolParameter {
                    name: "title".into(),
                    value_type: ToolType::String,
                },
                ToolParameter {
                    name: "count".into(),
                    value_type: ToolType::Integer,
                },
                ToolParameter {
                    name: "enabled".into(),
                    value_type: ToolType::Boolean,
                },
            ],
            result: ToolType::String,
            required_effects: vec![],
            limits: ToolLimits::default(),
        }
    }
    #[test]
    fn named_arguments_are_reordered_without_coercion_and_preserve_unicode() {
        let args = contract()
            .parse_arguments(
                "{\"enabled\":true,\"count\":-2147483648,\"title\":\"Привіт🦀\\u0000\"}".as_bytes(),
            )
            .unwrap();
        assert_eq!(
            args,
            [
                ToolValue::String("Привіт🦀\0".into()),
                ToolValue::Integer(i32::MIN),
                ToolValue::Boolean(true)
            ]
        );
    }
    #[test]
    fn malformed_duplicate_unknown_missing_and_non_i32_values_fail_closed() {
        for json in [
            "null",
            "[]",
            "true",
            "{}",
            "{",
            "{}{}",
            r#"{"title":"x","count":1,"enabled":true,"other":0}"#,
            r#"{"title":"x","title":"y","count":1,"enabled":true}"#,
            r#"{"title":"x","ti\u0074le":"y","count":1,"enabled":true}"#,
            r#"{"title":"x","count":1.0,"enabled":true}"#,
            r#"{"title":"x","count":1e0,"enabled":true}"#,
            r#"{"title":"x","count":-0,"enabled":true}"#,
            r#"{"title":"x","count":2147483648,"enabled":true}"#,
            r#"{"title":"x","count":-2147483649,"enabled":true}"#,
            r#"{"title":"x","count":"1","enabled":true}"#,
            r#"{"title":"x","count":1,"enabled":1}"#,
            r#"{"title":null,"count":1,"enabled":true}"#,
            r#"{"title":{},"count":1,"enabled":true}"#,
            r#"{"title":"\uD800","count":1,"enabled":true}"#,
        ] {
            assert!(
                contract().parse_arguments(json.as_bytes()).is_err(),
                "{json}"
            );
        }
        assert!(
            contract()
                .parse_arguments(&vec![b' '; abi::MAX_INPUT_BYTES + 1])
                .is_err()
        );
        assert!(contract().parse_arguments(b"{\"title\":\"\xff\"}").is_err());
        let mut limited = contract();
        limited.limits.string_bytes = 3;
        assert!(
            limited
                .parse_arguments("{\"title\":\"🦀\",\"count\":1,\"enabled\":true}".as_bytes())
                .is_err()
        );
    }
}
