pub type CapabilityId = String;

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
                let id = format!("{namespace}.{operation}");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> VartaConfig {
        VartaConfig {
            entry: "src/main.ts".to_string(),
            target: "native".to_string(),
            policy: "strict".to_string(),
            out: "dist/app".to_string(),
            capabilities: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn accepts_valid_policy_profiles() {
        for policy in ["strict", "compat"] {
            let mut config = valid_config();
            config.policy = policy.to_string();
            config.validate().unwrap();
        }
    }

    #[test]
    fn rejects_invalid_policy() {
        let mut config = valid_config();
        config.policy = "permissive".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_overly_broad_pattern() {
        let mut config = valid_config();
        let mut operations = std::collections::BTreeMap::new();
        operations.insert(
            "read".to_string(),
            CapabilitySpec::Patterns(vec!["*".to_string()]),
        );
        config.capabilities.insert("fs".to_string(), operations);
        assert!(config.validate().is_err());
    }

    #[test]
    fn normalizes_capability_manifest() {
        let mut config = valid_config();
        let mut operations = std::collections::BTreeMap::new();
        operations.insert(
            "fetch".to_string(),
            CapabilitySpec::Patterns(vec!["http://127.0.0.1/**".to_string()]),
        );
        operations.insert("now".to_string(), CapabilitySpec::Enabled(true));
        config
            .capabilities
            .insert("net".to_string(), operations.clone());

        let mut fs_operations = std::collections::BTreeMap::new();
        fs_operations.insert(
            "read".to_string(),
            CapabilitySpec::Patterns(vec!["/data/**".to_string()]),
        );
        config.capabilities.insert("fs".to_string(), fs_operations);

        let grants = config.normalized_capabilities();
        assert!(grants.iter().any(|grant| grant.id == "net.fetch"
            && matches!(&grant.value, CapabilityValue::UrlGlobs(patterns) if patterns == &vec!["http://127.0.0.1/**".to_string()])));
        assert!(
            grants.iter().any(|grant| grant.id == "net.now"
                && matches!(&grant.value, CapabilityValue::Boolean(true)))
        );
    }

    #[test]
    fn rejects_zero_port() {
        let mut config = valid_config();
        let mut operations = std::collections::BTreeMap::new();
        operations.insert("listen".to_string(), CapabilitySpec::Ports(vec![0]));
        config.capabilities.insert("http".to_string(), operations);
        assert!(config.validate().is_err());
    }
}
