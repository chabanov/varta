pub type CapabilityId = &'static str;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGrant {
    pub id: CapabilityId,
    pub value: CapabilityValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityValue {
    PathGlobs(Vec<String>),
    UrlGlobs(Vec<String>),
    Ports(Vec<u16>),
    Boolean(bool),
}

#[derive(Debug, thiserror::Error)]
pub enum VartaError {
    #[error("capability denied: {capability} for {target}")]
    CapabilityDenied { capability: String, target: String },
    #[error("invalid capability manifest: {0}")]
    InvalidManifest(String),
}

pub type Result<T> = std::result::Result<T, VartaError>;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum CapabilitySpec {
    Patterns(Vec<String>),
    Ports(Vec<u16>),
    Enabled(bool),
}

#[derive(Debug, Clone, serde::Deserialize)]
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

fn default_policy() -> String {
    "strict".to_string()
}

impl VartaConfig {
    pub fn validate(&self) -> Result<()> {
        if self.entry.trim().is_empty() {
            return Err(VartaError::InvalidManifest(
                "entry must not be empty".into(),
            ));
        }
        if !matches!(self.target.as_str(), "wasm" | "native") {
            return Err(VartaError::InvalidManifest(format!(
                "unsupported target: {}",
                self.target
            )));
        }
        if !matches!(self.policy.as_str(), "strict" | "compat") {
            return Err(VartaError::InvalidManifest(format!(
                "unsupported policy: {}",
                self.policy
            )));
        }
        if self.out.trim().is_empty() {
            return Err(VartaError::InvalidManifest("out must not be empty".into()));
        }

        for (namespace, operations) in &self.capabilities {
            if namespace.trim().is_empty() {
                return Err(VartaError::InvalidManifest(
                    "capability namespace must not be empty".into(),
                ));
            }
            for (operation, spec) in operations {
                if operation.trim().is_empty() {
                    return Err(VartaError::InvalidManifest(
                        "capability operation must not be empty".into(),
                    ));
                }
                match spec {
                    CapabilitySpec::Patterns(patterns) => {
                        if patterns.is_empty() {
                            return Err(VartaError::InvalidManifest(format!(
                                "{namespace}.{operation} requires at least one pattern"
                            )));
                        }
                        for pattern in patterns {
                            if pattern.trim().is_empty() || pattern == "*" {
                                return Err(VartaError::InvalidManifest(format!(
                                    "unsafe pattern for {namespace}.{operation}: {pattern}"
                                )));
                            }
                        }
                    }
                    CapabilitySpec::Ports(ports) => {
                        if ports.is_empty() {
                            return Err(VartaError::InvalidManifest(format!(
                                "{namespace}.{operation} requires at least one port"
                            )));
                        }
                        for port in ports {
                            if *port == 0 {
                                return Err(VartaError::InvalidManifest(format!(
                                    "invalid port for {namespace}.{operation}: {port}"
                                )));
                            }
                        }
                    }
                    CapabilitySpec::Enabled(_) => {}
                }
            }
        }

        Ok(())
    }

    pub fn normalized_capabilities(&self) -> Vec<CapabilityGrant> {
        let mut grants = Vec::new();
        for (namespace, operations) in &self.capabilities {
            for (operation, spec) in operations {
                let id = Box::leak(format!("{namespace}.{operation}").into_boxed_str());
                let value = match spec {
                    CapabilitySpec::Patterns(patterns) => {
                        if namespace == "net" {
                            CapabilityValue::UrlGlobs(patterns.clone())
                        } else {
                            CapabilityValue::PathGlobs(patterns.clone())
                        }
                    }
                    CapabilitySpec::Ports(ports) => CapabilityValue::Ports(ports.clone()),
                    CapabilitySpec::Enabled(enabled) => CapabilityValue::Boolean(*enabled),
                };
                grants.push(CapabilityGrant { id, value });
            }
        }
        grants
    }
}

/// Placeholder for an opaque host handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostHandle(u64);
