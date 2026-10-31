use crate::{
    CapabilityGrant, CapabilityValue, GrantKind, RequestScope, Result, VartaError, capability_kind,
};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
pub const STRICT_POLICY: &str = "strict";
/// The `target` values a manifest or project configuration may name.
pub const MANIFEST_TARGETS: [&str; 2] = ["wasm", "native"];
/// The largest manifest, host policy, decision file or project
/// configuration a reader accepts (bytes).
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
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
    /// Requested per-turn and per-instance budgets (v38 `turn_budgets`,
    /// host ABI 34 and later); in a host ceiling, the budgets it allows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budgets: Option<crate::budget::BudgetRequest>,
    /// Virtual filesystem roots (v38 `file_roots`, host ABI 34 and later):
    /// the root names a package reads under, or, in a host policy, each
    /// root name mapped to an absolute host directory ([`crate::files`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs_roots: Option<crate::files::FsRoots>,
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
    /// Permission broker v1 (host ABI 34 and later): whether the application
    /// cannot run without this grant; absent means required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Permission broker v1 (host ABI 34 and later): untrusted text a prompt
    /// shows as a quote next to the broker's own description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
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
                    required: None,
                    purpose: None,
                })
                .collect(),
            budgets: None,
            fs_roots: None,
        }
    }

    /// The root names a package manifest declares (none in a host policy).
    #[must_use]
    pub fn root_names(&self) -> &[String] {
        self.fs_roots
            .as_ref()
            .map_or(&[], crate::files::FsRoots::names)
    }

    /// The host directories a host policy maps (none in a package).
    #[must_use]
    pub fn root_directories(&self) -> &[crate::files::RootDirectory] {
        self.fs_roots
            .as_ref()
            .map_or(&[], crate::files::FsRoots::directories)
    }

    /// The package-only rules: a package names its `fsRoots` and never maps
    /// them to host directories, and under host ABI 34 and later (v38
    /// `file_roots`) every `fs.*` pattern is a virtual path under a declared
    /// root.
    pub fn validate_package(&self) -> Result<()> {
        let grants = self.grants()?;
        if matches!(self.fs_roots, Some(crate::files::FsRoots::Directories(_))) {
            return Err(VartaError::InvalidManifest(
                "a package names its fsRoots; only a host policy maps them to directories".into(),
            ));
        }
        if self.abi_version < crate::abi::PROGRAM_LIMITS_VERSION {
            // `kv.*` (v38) exists only with virtual file roots.
            if let Some(grant) = grants.iter().find(|grant| grant.id.starts_with("kv.")) {
                return Err(VartaError::InvalidManifest(format!(
                    "{} requires abiVersion {} or later (checked-f64-utf16-v38)",
                    grant.id,
                    crate::abi::PROGRAM_LIMITS_VERSION
                )));
            }
            return Ok(());
        }
        for grant in &grants {
            if let (true, CapabilityValue::PathGlobs(patterns)) =
                (crate::is_path_grant(&grant.id), &grant.value)
            {
                for pattern in patterns {
                    crate::files::validate_virtual_pattern(&grant.id, pattern, self.root_names())?;
                }
            }
        }
        Ok(())
    }

    /// Whether the manifest uses a member of the v38 broker or budget schema.
    fn uses_instance_schema(&self) -> bool {
        self.budgets.is_some()
            || self
                .capabilities
                .iter()
                .any(|wire| wire.required.is_some() || wire.purpose.is_some())
    }

    /// The requested (or, in a host ceiling, allowed) budgets.
    #[must_use]
    pub fn budget_request(&self) -> crate::budget::BudgetRequest {
        self.budgets.unwrap_or_default()
    }

    /// The consent metadata of every grant, by capability id.
    #[must_use]
    pub fn consents(&self) -> std::collections::BTreeMap<String, crate::permission::Consent> {
        self.capabilities
            .iter()
            .map(|wire| {
                (
                    wire.id.clone(),
                    crate::permission::Consent {
                        required: wire.required.unwrap_or(true),
                        purpose: wire.purpose.clone(),
                    },
                )
            })
            .collect()
    }

    /// A host ceiling bounds grants and budgets; consent metadata belongs to
    /// the package and is rejected there.
    pub fn validate_ceiling(&self) -> Result<()> {
        self.grants()?;
        if self
            .capabilities
            .iter()
            .any(|wire| wire.required.is_some() || wire.purpose.is_some())
        {
            return Err(VartaError::InvalidManifest(
                "a host policy cannot declare required or purpose".into(),
            ));
        }
        if matches!(self.fs_roots, Some(crate::files::FsRoots::Names(_))) {
            return Err(VartaError::InvalidManifest(
                "a host policy maps fsRoots to host directories; it does not list names".into(),
            ));
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|e| VartaError::InvalidManifest(e.to_string()))?;
        manifest.grants()?;
        Ok(manifest)
    }

    pub fn grants(&self) -> Result<Vec<CapabilityGrant>> {
        if self.capabilities.len() > crate::MAX_POLICY_GRANTS {
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
                .is_some_and(|t| !MANIFEST_TARGETS.contains(&t.as_str()))
        {
            return Err(VartaError::InvalidManifest(
                "unsupported manifest version, ABI, policy or target".into(),
            ));
        }
        if self.uses_instance_schema() && self.abi_version < crate::abi::PROGRAM_LIMITS_VERSION {
            return Err(VartaError::InvalidManifest(format!(
                "budgets, required and purpose require abiVersion {} or later (checked-f64-utf16-v38)",
                crate::abi::PROGRAM_LIMITS_VERSION
            )));
        }
        if let Some(budgets) = &self.budgets {
            budgets.validate()?;
        }
        if let Some(roots) = &self.fs_roots {
            if self.abi_version < crate::abi::PROGRAM_LIMITS_VERSION {
                return Err(VartaError::InvalidManifest(format!(
                    "fsRoots require abiVersion {} or later (checked-f64-utf16-v38)",
                    crate::abi::PROGRAM_LIMITS_VERSION
                )));
            }
            roots.validate()?;
        }
        for wire in &self.capabilities {
            if let Some(purpose) = &wire.purpose {
                crate::permission::validate_purpose(&wire.id, purpose)?;
            }
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
                    (Some(GrantKind::Urls | GrantKind::OpenUrls), Some(p), None) => {
                        CapabilityValue::UrlGlobs(p.clone())
                    }
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
            r#"{"capabilities":[{"id":"net.request","patterns":["https://*"]}]}"#,
            r#"{"capabilities":[{"id":"net.fetch","patterns":["https://*"]}]}"#,
            r#"{"capabilities":[{"id":"ui.open_url","patterns":["http://*"]}]}"#,
            r#"{"capabilities":[{"id":"ui.open_url","patterns":["https://*.a.test/**"]}]}"#,
            r#"{"capabilities":[{"id":"ui.open_url","patterns":["ftp://a.test/**"]}]}"#,
            r#"{"capabilities":[{"id":"ui.open_url","enabled":true}]}"#,
            r#"{"capabilities":[{"id":"ui.open_url","patterns":["https://a.test/**"],"methods":["GET"]}]}"#,
            r#"{"capabilities":[{"id":"clipboard.write","patterns":["x"]}]}"#,
            r#"{"capabilities":[{"id":"clipboard.read","enabled":true}]}"#,
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
    fn instance_schema_requires_host_abi_34() {
        let v38 = Manifest::parse(
            br#"{"abiVersion":34,"budgets":{"fuelPerTurn":2000000,"deadlinePerTurnMs":250},
            "capabilities":[{"id":"net.request","patterns":["https://api.test/**"],"required":false,"purpose":"Sync your items"},
            {"id":"time.timer","enabled":true}]}"#,
        )
        .unwrap();
        assert_eq!(v38.budget_request().fuel_per_turn, Some(2_000_000));
        let consents = v38.consents();
        assert!(!consents["net.request"].required);
        assert_eq!(
            consents["net.request"].purpose.as_deref(),
            Some("Sync your items")
        );
        assert!(consents["time.timer"].required);
        // The wire form round-trips with its new members.
        let again = Manifest::parse(&v38.to_bytes().unwrap()).unwrap();
        assert_eq!(again.consents(), consents);
        assert_eq!(again.budgets, v38.budgets);
        for source in [
            r#"{"abiVersion":33,"budgets":{"fuelPerTurn":1},"capabilities":[]}"#,
            r#"{"budgets":{},"capabilities":[]}"#,
            r#"{"abiVersion":33,"capabilities":[{"id":"time.now","enabled":true,"required":false}]}"#,
            r#"{"capabilities":[{"id":"time.now","enabled":true,"purpose":"clock"}]}"#,
            r#"{"abiVersion":34,"budgets":{"fuelPerTurn":0},"capabilities":[]}"#,
            r#"{"abiVersion":34,"budgets":{"memoryBytes":2147483648},"capabilities":[]}"#,
            r#"{"abiVersion":34,"budgets":{"fuel":1},"capabilities":[]}"#,
            r#"{"abiVersion":34,"capabilities":[{"id":"time.now","enabled":true,"purpose":""}]}"#,
            r#"{"abiVersion":34,"capabilities":[{"id":"time.now","enabled":true,"purpose":"a\u0007b"}]}"#,
            r#"{"abiVersion":34,"capabilities":[{"id":"time.now","enabled":true,"required":"no"}]}"#,
        ] {
            assert!(Manifest::parse(source.as_bytes()).is_err(), "{source}");
        }
        // A host ceiling may cap budgets but never carries consent metadata.
        let ceiling = Manifest::parse(
            br#"{"abiVersion":34,"budgets":{"fuelPerTurn":3000000},"capabilities":[]}"#,
        )
        .unwrap();
        ceiling.validate_ceiling().unwrap();
        assert!(v38.validate_ceiling().is_err());
    }

    #[test]
    fn view_grants_are_exact() {
        // V38 `view_protocol`: external links take URL patterns, including the
        // any-HTTPS form only here; the clipboard takes a boolean.
        let manifest = Manifest::parse(
            br#"{"capabilities":[{"id":"ui.open_url","patterns":["https://*","https://docs.test/**","http://127.0.0.1:8080/x"]},{"id":"clipboard.write","enabled":true}]}"#,
        )
        .unwrap();
        let grants = manifest.grants().unwrap();
        assert_eq!(
            grants[0].value,
            CapabilityValue::UrlGlobs(vec![
                crate::view::ANY_HTTPS_PATTERN.into(),
                "https://docs.test/**".into(),
                "http://127.0.0.1:8080/x".into()
            ])
        );
        assert_eq!(grants[1].value, CapabilityValue::Boolean(true));
        let again = Manifest::parse(&Manifest::new(&grants).to_bytes().unwrap()).unwrap();
        assert_eq!(again.grants().unwrap(), grants);
    }
    #[test]
    fn file_roots_require_host_abi_34_and_their_role() {
        // V38 `file_roots`: a package names roots and requests virtual paths.
        let package = Manifest::parse(
            br#"{"abiVersion":34,"fsRoots":["project","data"],
            "capabilities":[{"id":"fs.read","patterns":["/project/fixtures/**","/data/a.txt"]}]}"#,
        )
        .unwrap();
        package.validate_package().unwrap();
        assert_eq!(package.root_names(), ["project", "data"]);
        assert!(package.root_directories().is_empty());
        let again = Manifest::parse(&package.to_bytes().unwrap()).unwrap();
        assert_eq!(again.fs_roots, package.fs_roots);
        // A host policy maps names to absolute host directories.
        let host = Manifest::parse(
            br#"{"abiVersion":34,"fsRoots":{"project":"/srv/app"},
            "capabilities":[{"id":"fs.read","patterns":["/project/**"]}]}"#,
        )
        .unwrap();
        host.validate_ceiling().unwrap();
        assert_eq!(host.root_directories()[0].directory, "/srv/app");
        assert!(host.root_names().is_empty());
        // Each role rejects the other's form.
        assert!(package.validate_ceiling().is_err());
        assert!(host.validate_package().is_err());
        for source in [
            // Earlier manifests never carry roots.
            r#"{"abiVersion":33,"fsRoots":["project"],"capabilities":[]}"#,
            r#"{"fsRoots":{"project":"/srv"},"capabilities":[]}"#,
            r#"{"abiVersion":34,"fsRoots":["project","project"],"capabilities":[]}"#,
            r#"{"abiVersion":34,"fsRoots":["Project"],"capabilities":[]}"#,
            r#"{"abiVersion":34,"fsRoots":{"project":"/a","project":"/b"},"capabilities":[]}"#,
            r#"{"abiVersion":34,"fsRoots":{"project":"relative"},"capabilities":[]}"#,
            r#"{"abiVersion":34,"fsRoots":{"project":"/srv/../etc"},"capabilities":[]}"#,
            r#"{"abiVersion":34,"fsRoots":"project","capabilities":[]}"#,
        ] {
            assert!(Manifest::parse(source.as_bytes()).is_err(), "{source}");
        }
        // Under host ABI 34 a package's fs patterns name a declared root.
        for source in [
            r#"{"abiVersion":34,"capabilities":[{"id":"fs.read","patterns":["/data/**"]}]}"#,
            r#"{"abiVersion":34,"fsRoots":["project"],"capabilities":[{"id":"fs.read","patterns":["/project-x/**"]}]}"#,
            r#"{"abiVersion":34,"fsRoots":["project"],"capabilities":[{"id":"fs.print","patterns":["/**"]}]}"#,
            r#"{"abiVersion":34,"fsRoots":["project"],"capabilities":[{"id":"fs.read","patterns":["/proj*/x"]}]}"#,
        ] {
            let manifest = Manifest::parse(source.as_bytes()).unwrap();
            assert!(manifest.validate_package().is_err(), "{source}");
        }
        // Earlier packages keep absolute host patterns.
        Manifest::parse(
            br#"{"abiVersion":33,"capabilities":[{"id":"fs.read","patterns":["/data/**"]}]}"#,
        )
        .unwrap()
        .validate_package()
        .unwrap();
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
