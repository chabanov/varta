use crate::{
    CapabilityGrant, CapabilityValue, GrantKind, RequestScope, Result, VartaError, capability_kind,
};
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
    /// The method list of a `net.request` grant (v37); absent admits every
    /// method, and no other operation accepts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methods: Option<Vec<String>>,
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
                        CapabilityValue::Requests(scope) => Some(scope.urls.clone()),
                        CapabilityValue::Boolean(_) => None,
                    },
                    enabled: match g.value {
                        CapabilityValue::Boolean(b) => Some(b),
                        _ => None,
                    },
                    methods: match &g.value {
                        CapabilityValue::Requests(scope) => scope.methods.clone(),
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
                let kind = capability_kind(&wire.id);
                if wire.methods.is_some() && kind != Some(GrantKind::Requests) {
                    return Err(VartaError::InvalidManifest(format!(
                        "invalid grant shape for {}",
                        wire.id
                    )));
                }
                let value = match (kind, &wire.patterns, wire.enabled) {
                    (Some(GrantKind::Boolean), None, Some(b)) => CapabilityValue::Boolean(b),
                    (Some(GrantKind::Requests), Some(p), None) => {
                        CapabilityValue::Requests(RequestScope {
                            urls: p.clone(),
                            methods: wire.methods.clone(),
                        })
                    }
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
            r#"{"capabilities":[{"id":"net.fetch","patterns":[],"methods":["GET"]}]}"#,
            r#"{"capabilities":[{"id":"net.request","enabled":true}]}"#,
            r#"{"capabilities":[{"id":"net.request","methods":["GET"]}]}"#,
            r#"{"capabilities":[{"id":"net.request","patterns":["https://a.test/**"],"methods":["get"]}]}"#,
            r#"{"capabilities":[{"id":"net.request","patterns":["https://a.test/**"],"methods":["CONNECT"]}]}"#,
            r#"{"capabilities":[{"id":"net.request","patterns":["https://a.test/**"],"methods":["GET","GET"]}]}"#,
            r#"{"capabilities":[{"id":"net.request","patterns":["ftp://a.test/**"]}]}"#,
            r#"{"capabilities":[{"id":"net.request","patterns":["https://*.a.test/**"]}]}"#,
            r#"{"capabilities":[{"id":"net.fetch","patterns":["https://a.test/**"]}]}"#,
        ] {
            assert!(Manifest::parse(source.as_bytes()).is_err(), "{source}");
        }
    }
    #[test]
    fn request_grants_keep_urls_and_optional_methods() {
        let manifest = Manifest::parse(
            br#"{"capabilities":[{"id":"net.request","patterns":["https://api.test/v1/**","http://127.0.0.1:8080/**"],"methods":["GET","PATCH"]}]}"#,
        )
        .unwrap();
        let grants = manifest.grants().unwrap();
        assert_eq!(
            grants[0].value,
            CapabilityValue::Requests(RequestScope {
                urls: vec![
                    "https://api.test/v1/**".into(),
                    "http://127.0.0.1:8080/**".into()
                ],
                methods: Some(vec!["GET".into(), "PATCH".into()]),
            })
        );
        // The wire form round-trips; a grant without methods keeps no field.
        let again = Manifest::parse(&Manifest::new(&grants).to_bytes().unwrap()).unwrap();
        assert_eq!(again.grants().unwrap(), grants);
        let any = Manifest::new(&[CapabilityGrant {
            id: "net.request".into(),
            value: CapabilityValue::Requests(RequestScope {
                urls: vec!["https://api.test/**".into()],
                methods: None,
            }),
        }]);
        let text = String::from_utf8(any.to_bytes().unwrap()).unwrap();
        assert!(!text.contains("methods"), "{text}");
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
