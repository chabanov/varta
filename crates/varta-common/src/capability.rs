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

use crate::{Result, VartaError};

/// Canonical host operations. Unknown operations and mismatched grant shapes fail closed.
#[must_use]
pub fn capability_kind(id: &str) -> Option<GrantKind> {
    match id {
        "time.now" | "process.exit" => Some(GrantKind::Boolean),
        "env.has" | "env.print" | "env.get" | "fs.read" | "fs.print" => Some(GrantKind::Patterns),
        "net.fetch" | "net.get" => Some(GrantKind::Urls),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    Boolean,
    Patterns,
    Urls,
}

impl CapabilityGrant {
    pub fn validate(&self) -> Result<()> {
        let kind = capability_kind(&self.id).ok_or_else(|| {
            VartaError::InvalidManifest(format!("unknown capability: {}", self.id))
        })?;
        match (&kind, &self.value) {
            (GrantKind::Boolean, CapabilityValue::Boolean(_)) => Ok(()),
            (GrantKind::Patterns, CapabilityValue::PathGlobs(patterns))
            | (GrantKind::Urls, CapabilityValue::UrlGlobs(patterns)) => {
                if patterns.len() > 64 {
                    return Err(VartaError::InvalidManifest(
                        "too many patterns in one grant".into(),
                    ));
                }
                for pattern in patterns {
                    if pattern.len() > 512
                        || pattern.trim().is_empty()
                        || pattern.trim_matches(['*', '?']).is_empty()
                        || pattern.contains('\0')
                    {
                        return Err(VartaError::InvalidManifest(format!(
                            "unsafe pattern for {}: {pattern}",
                            self.id
                        )));
                    }
                    if self.id.starts_with("fs.") {
                        let path = std::path::Path::new(pattern);
                        if !path.is_absolute()
                            || path
                                .components()
                                .any(|c| matches!(c, std::path::Component::ParentDir))
                        {
                            return Err(VartaError::InvalidManifest(format!(
                                "{} requires absolute paths without traversal",
                                self.id
                            )));
                        }
                    }
                    if kind == GrantKind::Urls {
                        validate_url_pattern(pattern)?;
                    }
                }
                Ok(())
            }
            _ => Err(VartaError::InvalidManifest(format!(
                "invalid grant shape for {}",
                self.id
            ))),
        }
    }
}

pub fn validate_grants(grants: &[CapabilityGrant]) -> Result<()> {
    let patterns: usize = grants
        .iter()
        .map(|grant| match &grant.value {
            CapabilityValue::PathGlobs(patterns) | CapabilityValue::UrlGlobs(patterns) => {
                patterns.len()
            }
            _ => 0,
        })
        .sum();
    if grants.len() > 128 || patterns > 256 {
        return Err(VartaError::InvalidManifest(
            "capability policy exceeds complexity limits".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for grant in grants {
        if !seen.insert(&grant.id) {
            return Err(VartaError::InvalidManifest(
                "duplicate capability grant".into(),
            ));
        }
        grant.validate()?;
    }
    Ok(())
}

fn validate_url_pattern(pattern: &str) -> Result<()> {
    // Wildcards are allowed only in the path. Authority and scheme are exact.
    let Some(rest) = pattern.strip_prefix("http://") else {
        return Err(VartaError::InvalidManifest(
            "only http:// capabilities are supported".into(),
        ));
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty()
        || authority.contains(['*', '?', '@', '#'])
        || pattern
            .chars()
            .any(|c| c.is_ascii_control() || c.is_whitespace())
        || path.contains(['#', '%', '\\'])
        || path.split('/').any(|part| matches!(part, "." | ".."))
    {
        return Err(VartaError::InvalidManifest(format!(
            "invalid URL pattern: {pattern}"
        )));
    }
    let parsed =
        url::Url::parse(pattern).map_err(|e| VartaError::InvalidManifest(e.to_string()))?;
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path().contains('%')
    {
        return Err(VartaError::InvalidManifest(format!(
            "invalid URL authority: {pattern}"
        )));
    }
    Ok(())
}

/// The one mapping shared by validation and inspection.
#[must_use]
pub fn host_capability(import: &str) -> Option<Option<&'static str>> {
    crate::HostOperation::from_name(import).map(crate::HostOperation::capability)
}
