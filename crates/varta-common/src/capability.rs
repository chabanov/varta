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
    ListenEndpoints(Vec<String>),
    Boolean(bool),
    /// `net.request` (v37): URL scopes over `http` and `https` with an
    /// optional method list.
    Requests(RequestScope),
}

/// The scope of one `net.request` grant. A request must match one URL pattern
/// (the network URL rules: exact scheme, host and port, wildcards only after
/// the authority) and, when `methods` is present, one method exactly. An
/// empty URL list or an empty method list denies every request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestScope {
    pub urls: Vec<String>,
    /// `None` admits every method `fetch` can send.
    pub methods: Option<Vec<String>>,
}

/// Methods `fetch` normalizes to uppercase (WHATWG "normalize a method"); a
/// grant must spell them in uppercase, the only form a request carries.
pub const NORMALIZED_METHODS: [&str; 6] = ["DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT"];

use crate::{Result, VartaError};

/// Canonical host operations. Unknown operations and mismatched grant shapes fail closed.
#[must_use]
pub fn capability_kind(id: &str) -> Option<GrantKind> {
    match id {
        "time.now" | "time.timer" | "process.exit" => Some(GrantKind::Boolean),
        "env.has" | "env.print" | "env.get" | "fs.read" | "fs.print" => Some(GrantKind::Patterns),
        crate::kv::READ_CAPABILITY | crate::kv::WRITE_CAPABILITY => Some(GrantKind::Patterns),
        "net.fetch" | "net.get" => Some(GrantKind::Urls),
        crate::net::REQUEST_CAPABILITY => Some(GrantKind::Requests),
        "net.listen" => Some(GrantKind::Listeners),
        crate::view::OPEN_URL_CAPABILITY => Some(GrantKind::OpenUrls),
        crate::view::CLIPBOARD_WRITE_CAPABILITY => Some(GrantKind::Boolean),
        _ => None,
    }
}

/// Whether `id` is a grant over file paths: `fs.*`, and the v38 `kv.*`
/// database files, which are virtual paths under the same file roots.
#[must_use]
pub fn is_path_grant(id: &str) -> bool {
    id.starts_with("fs.") || matches!(id, crate::kv::READ_CAPABILITY | crate::kv::WRITE_CAPABILITY)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    Boolean,
    Patterns,
    Urls,
    Listeners,
    /// URL patterns plus an optional method list (`net.request`).
    Requests,
    /// External-link URL patterns (`ui.open_url`, v38): the network URL
    /// rules over `https://` and `http://`, plus the exact any-HTTPS form
    /// [`crate::view::ANY_HTTPS_PATTERN`], which no other capability admits.
    OpenUrls,
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
                    if is_path_grant(&self.id) {
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
                        validate_url_pattern(pattern, &[HTTP_SCHEME])?;
                    }
                }
                Ok(())
            }
            (GrantKind::Requests, CapabilityValue::Requests(scope)) => scope.validate(&self.id),
            (GrantKind::OpenUrls, CapabilityValue::UrlGlobs(patterns)) => {
                validate_open_url_patterns(&self.id, patterns)
            }
            (GrantKind::Listeners, CapabilityValue::ListenEndpoints(endpoints)) => {
                if endpoints.len() > 64 {
                    return Err(VartaError::InvalidManifest(
                        "too many listener endpoints".into(),
                    ));
                }
                for endpoint in endpoints {
                    let addr = endpoint.parse::<std::net::SocketAddr>().map_err(|_| {
                        VartaError::InvalidManifest(
                            "listener grants require literal IP:port endpoints".into(),
                        )
                    })?;
                    if addr.to_string() != *endpoint {
                        return Err(VartaError::InvalidManifest(
                            "listener endpoint must be canonical".into(),
                        ));
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
            CapabilityValue::PathGlobs(patterns)
            | CapabilityValue::UrlGlobs(patterns)
            | CapabilityValue::ListenEndpoints(patterns) => patterns.len(),
            CapabilityValue::Requests(scope) => scope.urls.len(),
            CapabilityValue::Boolean(_) => 0,
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

/// URL schemes of network grants: `net.fetch`/`net.get` admit plain HTTP only,
/// `net.request` (v37) admits HTTPS as well.
const HTTP_SCHEME: &str = "http";
const HTTPS_SCHEME: &str = "https";
const SCHEME_SEPARATOR: &str = "://";

impl RequestScope {
    fn validate(&self, id: &str) -> Result<()> {
        if self.urls.len() > 64 {
            return Err(VartaError::InvalidManifest(
                "too many patterns in one grant".into(),
            ));
        }
        for pattern in &self.urls {
            if pattern.len() > 512 || pattern.contains('\0') {
                return Err(VartaError::InvalidManifest(format!(
                    "unsafe pattern for {id}: {pattern}"
                )));
            }
            validate_url_pattern(pattern, &[HTTP_SCHEME, HTTPS_SCHEME])?;
        }
        let Some(methods) = &self.methods else {
            return Ok(());
        };
        if methods.len() > crate::net::MAX_GRANT_METHODS {
            return Err(VartaError::InvalidManifest(format!(
                "too many methods in one {id} grant"
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        for method in methods {
            let unnormalized = NORMALIZED_METHODS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(method) && known != method);
            if !crate::net::valid_method(method) || unnormalized || !seen.insert(method) {
                return Err(VartaError::InvalidManifest(format!(
                    "invalid method for {id}: {method}"
                )));
            }
        }
        Ok(())
    }
}

/// `ui.open_url` (v38): at most 64 patterns, each the exact any-HTTPS form or
/// an `https://`/`http://` URL pattern under the network URL rules.
fn validate_open_url_patterns(id: &str, patterns: &[String]) -> Result<()> {
    if patterns.len() > 64 {
        return Err(VartaError::InvalidManifest(
            "too many patterns in one grant".into(),
        ));
    }
    for pattern in patterns {
        if pattern == crate::view::ANY_HTTPS_PATTERN {
            continue;
        }
        if pattern.len() > 512 || pattern.contains('\0') {
            return Err(VartaError::InvalidManifest(format!(
                "unsafe pattern for {id}: {pattern}"
            )));
        }
        validate_url_pattern(pattern, &[HTTPS_SCHEME, HTTP_SCHEME])?;
    }
    Ok(())
}

fn validate_url_pattern(pattern: &str, schemes: &[&str]) -> Result<()> {
    // Wildcards are allowed only in the path. Authority and scheme are exact.
    let Some(rest) = schemes.iter().find_map(|scheme| {
        pattern
            .strip_prefix(scheme)
            .and_then(|rest| rest.strip_prefix(SCHEME_SEPARATOR))
    }) else {
        return Err(VartaError::InvalidManifest(if schemes.len() == 1 {
            "only http:// capabilities are supported".into()
        } else {
            "net.request patterns require http:// or https://".into()
        }));
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
