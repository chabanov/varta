use crate::{CapabilityGrant, CapabilityValue, Result, VartaError};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum CapabilitySpec {
    Patterns(Vec<String>),
    Ports(Vec<u16>),
    Enabled(bool),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VartaConfig {
    pub entry: String,
    pub target: String,
    #[serde(default = "default_policy")]
    pub policy: String,
    pub out: String,
    #[serde(default)]
    pub capabilities:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, CapabilitySpec>>,
}

pub(crate) fn default_policy() -> String {
    "strict".into()
}

impl VartaConfig {
    pub fn validate(&self) -> Result<()> {
        if self.entry.trim().is_empty() || self.out.trim().is_empty() {
            return Err(VartaError::InvalidManifest(
                "entry and out must not be empty".into(),
            ));
        }
        if !matches!(self.target.as_str(), "wasm" | "native") {
            return Err(VartaError::InvalidManifest(format!(
                "unsupported target: {}",
                self.target
            )));
        }
        if self.policy != "strict" {
            return Err(VartaError::InvalidManifest(
                "only the strict policy is implemented".into(),
            ));
        }
        crate::validate_grants(&self.normalized_capabilities())
    }

    #[must_use]
    pub fn normalized_capabilities(&self) -> Vec<CapabilityGrant> {
        self.capabilities
            .iter()
            .flat_map(|(namespace, operations)| {
                operations
                    .iter()
                    .map(move |(operation, spec)| CapabilityGrant {
                        id: format!("{namespace}.{operation}"),
                        value: match spec {
                            CapabilitySpec::Patterns(patterns) if namespace == "net" => {
                                CapabilityValue::UrlGlobs(patterns.clone())
                            }
                            CapabilitySpec::Patterns(patterns) => {
                                CapabilityValue::PathGlobs(patterns.clone())
                            }
                            CapabilitySpec::Ports(ports) => CapabilityValue::Ports(ports.clone()),
                            CapabilitySpec::Enabled(enabled) => CapabilityValue::Boolean(*enabled),
                        },
                    })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(caps: serde_json::Value) -> VartaConfig {
        serde_json::from_value(serde_json::json!({"entry":"src/main.ts","target":"native","out":"dist/app","capabilities":caps})).unwrap()
    }
    #[test]
    fn grant_shapes_and_operations_are_checked() {
        for grants in [
            serde_json::json!({"env":{"get":true}}),
            serde_json::json!({"env":{"unknown":["X"]}}),
            serde_json::json!({"time":{"now":["X"]}}),
            serde_json::json!({"fs":{"read":["relative/**"]}}),
            serde_json::json!({"fs":{"read":["/data/../secret"]}}),
            serde_json::json!({"env":{"get":["***"]}}),
        ] {
            assert!(config(grants).validate().is_err());
        }
        assert!(
            config(serde_json::json!({"env":{"get":[]},"time":{"now":false}}))
                .validate()
                .is_ok()
        );
    }
    #[test]
    fn unsupported_transport_and_profiles_cannot_silently_pass() {
        for pattern in [
            "https://example.com/**",
            "http://*.example.com/**",
            "http://example.com/public/../private",
            "http://example.com/public/%2fprivate",
        ] {
            assert!(
                config(serde_json::json!({"net":{"fetch":[pattern]}}))
                    .validate()
                    .is_err()
            );
        }
        let mut config = config(serde_json::json!({}));
        config.policy = "compat".into();
        assert!(config.validate().is_err());
    }
}
