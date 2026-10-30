use varta_common::{
    CapabilityGrant, CapabilityValue, RequestScope, Result, STRICT_POLICY, VartaError,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum CapabilitySpec {
    Patterns(Vec<String>),
    Enabled(bool),
    /// `"net": { "request": { "urls": [...], "methods": [...] } }` (v37); a
    /// plain pattern list admits every method.
    Requests(RequestSpec),
}

/// The object form of a `net.request` grant.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSpec {
    pub urls: Vec<String>,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VartaConfig {
    #[serde(default, rename = "languageProfile")]
    pub language_profile: varta_common::language::LanguageProfile,
    pub entry: String,
    pub target: String,
    #[serde(default = "default_policy")]
    pub policy: String,
    pub out: String,
    #[serde(default)]
    pub capabilities:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, CapabilitySpec>>,
    /// Requested per-turn and per-instance budgets (checked-f64-utf16-v38);
    /// the host ceiling caps them.
    #[serde(default)]
    pub budgets: Option<varta_common::budget::BudgetRequest>,
    /// Consent metadata of requested grants by capability id
    /// (checked-f64-utf16-v38): `required` and `purpose`.
    #[serde(default)]
    pub permissions: std::collections::BTreeMap<String, PermissionSpec>,
}

/// `"permissions": { "net.request": { "required": false, "purpose": "..." } }`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionSpec {
    pub required: Option<bool>,
    pub purpose: Option<String>,
}

pub(crate) fn default_policy() -> String {
    STRICT_POLICY.into()
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
        if self.policy != STRICT_POLICY {
            return Err(VartaError::InvalidManifest(
                "only the strict policy is implemented".into(),
            ));
        }
        let grants = self.normalized_capabilities();
        varta_common::validate_grants(&grants)?;
        for id in self.permissions.keys() {
            if !grants.iter().any(|grant| &grant.id == id) {
                return Err(VartaError::InvalidManifest(format!(
                    "permissions names {id}, which the capabilities do not request"
                )));
            }
        }
        self.manifest().grants().map(|_| ())
    }

    /// The package manifest of this configuration (host ABI, grants, budgets
    /// and consent metadata); the manifest schema validates the members the
    /// profile admits.
    #[must_use]
    pub fn manifest(&self) -> varta_common::Manifest {
        let mut manifest = varta_common::Manifest::new(&self.normalized_capabilities());
        manifest.abi_version = self.language_profile.host_abi();
        manifest.entry = Some(self.entry.clone());
        manifest.target = Some(self.target.clone());
        manifest.budgets = self.budgets;
        for wire in &mut manifest.capabilities {
            if let Some(spec) = self.permissions.get(&wire.id) {
                wire.required = spec.required;
                wire.purpose = spec.purpose.clone();
            }
        }
        manifest
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
                            CapabilitySpec::Patterns(patterns)
                                if namespace == "net" && operation == "listen" =>
                            {
                                CapabilityValue::ListenEndpoints(patterns.clone())
                            }
                            CapabilitySpec::Patterns(patterns)
                                if namespace == "net" && operation == "request" =>
                            {
                                CapabilityValue::Requests(RequestScope {
                                    urls: patterns.clone(),
                                    methods: None,
                                })
                            }
                            // Any other operation rejects the shape in validation.
                            CapabilitySpec::Requests(spec) => {
                                CapabilityValue::Requests(RequestScope {
                                    urls: spec.urls.clone(),
                                    methods: Some(spec.methods.clone()),
                                })
                            }
                            CapabilitySpec::Patterns(patterns)
                                if namespace == "net"
                                    || format!("{namespace}.{operation}")
                                        == varta_common::view::OPEN_URL_CAPABILITY =>
                            {
                                CapabilityValue::UrlGlobs(patterns.clone())
                            }
                            CapabilitySpec::Patterns(patterns) => {
                                CapabilityValue::PathGlobs(patterns.clone())
                            }
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
    fn timer_grants_are_exact_booleans() {
        // V36 timers: `time.timer` is its own boolean operation.
        let timers = config(serde_json::json!({"time":{"timer":true,"now":false}}));
        timers.validate().unwrap();
        assert_eq!(
            timers.normalized_capabilities(),
            [
                CapabilityGrant {
                    id: "time.now".into(),
                    value: CapabilityValue::Boolean(false),
                },
                CapabilityGrant {
                    id: varta_common::scheduler::TIMER_CAPABILITY.into(),
                    value: CapabilityValue::Boolean(true),
                },
            ]
        );
        for grants in [
            serde_json::json!({"time":{"timer":["X"]}}),
            serde_json::json!({"time":{"sleep":true}}),
            serde_json::json!({"time":{"timers":true}}),
        ] {
            assert!(config(grants).validate().is_err());
        }
    }
    #[test]
    fn request_grants_take_urls_with_an_optional_method_list() {
        // V37 `net.request`: HTTPS and HTTP origins, all methods by default.
        let plain = config(serde_json::json!({"net":{"request":["https://api.test/v1/**"]}}));
        plain.validate().unwrap();
        assert_eq!(
            plain.normalized_capabilities()[0].value,
            CapabilityValue::Requests(RequestScope {
                urls: vec!["https://api.test/v1/**".into()],
                methods: None,
            })
        );
        let scoped = config(serde_json::json!({"net":{"request":{
            "urls":["http://127.0.0.1:8080/**"],"methods":["GET","POST"]}}}));
        scoped.validate().unwrap();
        assert_eq!(
            scoped.normalized_capabilities()[0].value,
            CapabilityValue::Requests(RequestScope {
                urls: vec!["http://127.0.0.1:8080/**".into()],
                methods: Some(vec!["GET".into(), "POST".into()]),
            })
        );
        for grants in [
            serde_json::json!({"net":{"request":true}}),
            serde_json::json!({"net":{"request":["https://*.test/**"]}}),
            serde_json::json!({"net":{"request":["ws://api.test/**"]}}),
            serde_json::json!({"net":{"request":{"urls":["https://api.test/**"],"methods":["get"]}}}),
            serde_json::json!({"net":{"request":{"urls":["https://api.test/**"],"methods":["TRACE"]}}}),
            serde_json::json!({"net":{"request":{"urls":["https://api.test/**"]}}}),
            serde_json::json!({"net":{"request":{"urls":[],"methods":[],"extra":1}}}),
            serde_json::json!({"net":{"fetch":{"urls":["http://api.test/**"],"methods":["GET"]}}}),
            serde_json::json!({"env":{"get":{"urls":["X"],"methods":["GET"]}}}),
        ] {
            let parsed = serde_json::from_value::<VartaConfig>(serde_json::json!({
                "entry":"src/main.ts","target":"native","out":"dist/app","capabilities":grants}));
            assert!(
                parsed.map_or(true, |config| config.validate().is_err()),
                "{grants}"
            );
        }
    }
    #[test]
    fn view_grants_take_link_patterns_and_a_clipboard_boolean() {
        // V38 view capabilities: exact link patterns (the any-HTTPS form only
        // here) and a boolean clipboard grant.
        let view = config(serde_json::json!({
            "ui": {"open_url": ["https://*", "https://docs.test/**"]},
            "clipboard": {"write": true}}));
        view.validate().unwrap();
        assert_eq!(
            view.normalized_capabilities(),
            [
                CapabilityGrant {
                    id: "clipboard.write".into(),
                    value: CapabilityValue::Boolean(true),
                },
                CapabilityGrant {
                    id: "ui.open_url".into(),
                    value: CapabilityValue::UrlGlobs(vec![
                        "https://*".into(),
                        "https://docs.test/**".into()
                    ]),
                },
            ]
        );
        for grants in [
            serde_json::json!({"ui":{"open_url":true}}),
            serde_json::json!({"ui":{"open_url":["http://*"]}}),
            serde_json::json!({"ui":{"open_url":["https://*.docs.test/**"]}}),
            serde_json::json!({"net":{"request":["https://*"]}}),
            serde_json::json!({"clipboard":{"write":["x"]}}),
            serde_json::json!({"clipboard":{"read":true}}),
        ] {
            assert!(config(grants.clone()).validate().is_err(), "{grants}");
        }
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

    #[test]
    fn budgets_and_consents_need_profile_v38_and_requested_grants() {
        let config = |profile: &str, extra: serde_json::Value| {
            let mut value = serde_json::json!({"languageProfile":profile,"entry":"src/main.ts",
                "target":"native","out":"dist/app","capabilities":{"time":{"timer":true}}});
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            serde_json::from_value::<VartaConfig>(value)
        };
        let v38 = config(
            "checked-f64-utf16-v38",
            serde_json::json!({"budgets":{"fuelPerTurn":2000000},
                "permissions":{"time.timer":{"required":false,"purpose":"Retry later"}}}),
        )
        .unwrap();
        v38.validate().unwrap();
        let manifest = v38.manifest();
        assert_eq!(
            manifest.abi_version,
            varta_common::abi::PROGRAM_LIMITS_VERSION
        );
        assert_eq!(manifest.budget_request().fuel_per_turn, Some(2_000_000));
        assert!(!manifest.consents()["time.timer"].required);
        for (profile, extra) in [
            (
                "checked-f64-utf16-v37",
                serde_json::json!({"budgets":{"fuelPerTurn":1}}),
            ),
            (
                "checked-f64-utf16-v37",
                serde_json::json!({"permissions":{"time.timer":{"required":false}}}),
            ),
            (
                "checked-f64-utf16-v38",
                serde_json::json!({"permissions":{"time.now":{"required":false}}}),
            ),
            (
                "checked-f64-utf16-v38",
                serde_json::json!({"budgets":{"fuelPerTurn":0}}),
            ),
            (
                "checked-f64-utf16-v38",
                serde_json::json!({"permissions":{"time.timer":{"purpose":""}}}),
            ),
        ] {
            assert!(
                config(profile, extra.clone()).unwrap().validate().is_err(),
                "{profile} {extra}"
            );
        }
        assert!(
            config(
                "checked-f64-utf16-v38",
                serde_json::json!({"permissions":{"time.timer":{"optional":true}}})
            )
            .is_err()
        );
    }

    #[test]
    fn unsupported_numeric_port_lists_are_rejected_by_the_project_schema() {
        for operation in ["fetch", "listen"] {
            let operations = std::collections::BTreeMap::from([(operation, vec![80, 443])]);
            let value = serde_json::json!({"entry":"src/main.ts","target":"wasm","out":"dist/app",
                "capabilities":{"net":operations}});
            assert!(serde_json::from_value::<VartaConfig>(value).is_err());
        }
    }
}
