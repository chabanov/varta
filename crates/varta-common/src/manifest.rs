use crate::{CapabilityGrant, CapabilityValue, GrantKind, Result, VartaError, capability_kind};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
pub const STRICT_POLICY: &str = "strict";
fn default_policy() -> String {
    STRICT_POLICY.into()
}
fn schema_version() -> u32 {
    SCHEMA_VERSION
}
fn abi_version() -> u32 {
    crate::abi::VERSION
}

/// Strict wire schema shared by standalone and embedded artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default = "abi_version")]
    pub abi_version: u32,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default = "default_policy")]
    pub policy: String,
    pub capabilities: Vec<WireGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireGrant {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl Manifest {
    #[must_use]
    pub fn new(grants: &[CapabilityGrant]) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            abi_version: crate::abi::VERSION,
            entry: None,
            target: None,
            policy: STRICT_POLICY.into(),
            capabilities: grants
                .iter()
                .map(|g| WireGrant {
                    id: g.id.clone(),
                    patterns: match &g.value {
                        CapabilityValue::PathGlobs(p)
                        | CapabilityValue::UrlGlobs(p)
                        | CapabilityValue::ListenEndpoints(p) => Some(p.clone()),
                        _ => None,
                    },
                    enabled: match g.value {
                        CapabilityValue::Boolean(b) => Some(b),
                        _ => None,
                    },
                })
                .collect(),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|e| VartaError::InvalidManifest(e.to_string()))?;
        manifest.grants()?;
        Ok(manifest)
    }

    pub fn grants(&self) -> Result<Vec<CapabilityGrant>> {
        if self.capabilities.len() > 128 {
            return Err(VartaError::InvalidManifest(
                "too many capability grants".into(),
            ));
        }
        if self.schema_version != SCHEMA_VERSION
            || !crate::abi::supported(self.abi_version)
            || self.policy != STRICT_POLICY
            || self
                .target
                .as_ref()
                .is_some_and(|t| !matches!(t.as_str(), "wasm" | "native"))
        {
            return Err(VartaError::InvalidManifest(
                "unsupported manifest version, ABI, policy or target".into(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        let grants: Vec<CapabilityGrant> = self
            .capabilities
            .iter()
            .map(|wire| {
                if !seen.insert(&wire.id) {
                    return Err(VartaError::InvalidManifest(format!(
                        "duplicate capability: {}",
                        wire.id
                    )));
                }
                let value = match (capability_kind(&wire.id), &wire.patterns, wire.enabled) {
                    (Some(GrantKind::Boolean), None, Some(b)) => CapabilityValue::Boolean(b),
                    (Some(GrantKind::Patterns), Some(p), None) => {
                        CapabilityValue::PathGlobs(p.clone())
                    }
                    (Some(GrantKind::Urls), Some(p), None) => CapabilityValue::UrlGlobs(p.clone()),
                    (Some(GrantKind::Listeners), Some(p), None) => {
                        CapabilityValue::ListenEndpoints(p.clone())
                    }
                    _ => {
                        return Err(VartaError::InvalidManifest(format!(
                            "invalid grant shape for {}",
                            wire.id
                        )));
                    }
                };
                let grant = CapabilityGrant {
                    id: wire.id.clone(),
                    value,
                };
                grant.validate()?;
                Ok(grant)
            })
            .collect::<Result<_>>()?;
        crate::validate_grants(&grants)?;
        Ok(grants)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.grants()?;
        serde_json::to_vec_pretty(self).map_err(|e| VartaError::InvalidManifest(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_grants_fail_closed() {
        for source in [
            r#"{"capabilities":[{"id":"env.get","patterns":"INVALID"}]}"#,
            r#"{"capabilities":[{"id":"env.get"}]}"#,
            r#"{"capabilities":[{"id":"env.get","enabled":true}]}"#,
            r#"{"capabilities":[{"id":"time.now","patterns":["PATH"]}]}"#,
            r#"{"capabilities":[{"id":"env.unknown","patterns":[]}]}"#,
        ] {
            assert!(Manifest::parse(source.as_bytes()).is_err(), "{source}");
        }
    }
    #[test]
    fn empty_patterns_are_explicit_denials() {
        let manifest =
            Manifest::parse(br#"{"capabilities":[{"id":"env.get","patterns":[]}]}"#).unwrap();
        assert_eq!(
            manifest.grants().unwrap()[0].value,
            CapabilityValue::PathGlobs(vec![])
        );
    }
}
