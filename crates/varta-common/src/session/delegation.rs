//! Host-only native delegation policy. No application ownership schema.
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};
pub const CONTROL_PREFIX: &str = "/__varta/session-delegation/v1";
pub const REQUEST_PROOF_HEADER: &str = "x-varta-request-proof";
pub const MAX_CONTEXT_BYTES: usize = 4096;
pub const MAX_CONTROL_BYTES: usize = 32768;
pub const MAX_BINDINGS: usize = 1024;
pub const MAX_PROOFS: usize = 1024;
pub const MAX_AGE_SECONDS: u64 = 300;
pub const RETENTION_SECONDS: u64 = 300;
/// Opt-in native HTTP ceiling, never package/browser metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DelegationPolicy {
    pub control_token_env: String,
    pub max_bindings: usize,
    pub max_request_proofs: usize,
    pub max_age_seconds: u64,
    pub path_prefixes: Vec<String>,
    pub methods: Vec<String>,
}
impl DelegationPolicy {
    pub fn validate(&self) -> crate::Result<()> {
        if self.control_token_env.is_empty()
            || self.control_token_env.len() > 64
            || !self
                .control_token_env
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || !(1..=MAX_BINDINGS).contains(&self.max_bindings)
            || !(1..=MAX_PROOFS).contains(&self.max_request_proofs)
            || !(1..=MAX_AGE_SECONDS).contains(&self.max_age_seconds)
            || self.path_prefixes.is_empty()
            || self.path_prefixes.len() > 16
            || self.methods.is_empty()
            || self.methods.len() > 3
            || self
                .path_prefixes
                .iter()
                .any(|p| !prefix(p) || p.starts_with(CONTROL_PREFIX))
            || self.path_prefixes.windows(2).any(|p| p[0] >= p[1])
            || self
                .methods
                .iter()
                .any(|m| !matches!(m.as_str(), "GET" | "PATCH" | "POST"))
            || self.methods.windows(2).any(|m| m[0] >= m[1])
        {
            return Err(crate::VartaError::InvalidManifest(
                "invalid native delegation ceiling".into(),
            ));
        }
        Ok(())
    }
}
#[must_use]
pub fn prefix(s: &str) -> bool {
    s.len() > 1
        && s.len() <= 256
        && s.starts_with('/')
        && !s.ends_with('/')
        && s.split('/').skip(1).all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        })
}
#[must_use]
pub fn target_allowed(target: &str, prefix: &str) -> bool {
    let path = target.split('?').next().unwrap_or_default();
    path.len() <= 4096
        && path.starts_with('/')
        && !path.contains('%')
        && !path.contains('\\')
        && path.split('/').skip(1).all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        })
        && (path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|s| s.starts_with('/')))
        && !target.bytes().any(|b| b <= b' ' || b == b'#')
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> DelegationPolicy {
        DelegationPolicy {
            control_token_env: "TEST_CONTROL".into(),
            max_bindings: 16,
            max_request_proofs: 64,
            max_age_seconds: 300,
            path_prefixes: vec!["/private".into()],
            methods: vec!["GET".into(), "PATCH".into(), "POST".into()],
        }
    }
    #[test]
    fn delegation_policy_is_opt_in_bounded_and_has_no_domain_schema() {
        let p = policy();
        p.validate().unwrap();
        let mut bad = p.clone();
        bad.max_age_seconds = 301;
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.max_request_proofs = 1025;
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.path_prefixes = vec![CONTROL_PREFIX.into()];
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.methods.reverse();
        assert!(bad.validate().is_err());
        let mut bad = p;
        bad.control_token_env = "PATH=bad".into();
        assert!(bad.validate().is_err());
    }
    #[test]
    fn delegation_route_matching_has_literal_segment_boundaries() {
        assert!(target_allowed("/private/item?cursor=opaque%3D", "/private"));
        for t in [
            "/privateX",
            "/private/..",
            "/private//item",
            "/private/%2fitem",
            "/private/./item",
            "/private/item#part",
            "/private/item\n",
            "/private\\item",
        ] {
            assert!(!target_allowed(t, "/private"), "{t:?}");
        }
    }
    #[test]
    fn delegation_manifest_requires_native_parent_and_stays_host_only() {
        let mut m = crate::Manifest::new(&[]);
        m.abi_version = crate::abi::PROTECTED_SESSION_VERSION;
        m.http = Some(crate::http::HttpPolicy {
            delegation: Some(policy()),
            ..Default::default()
        });
        assert!(m.grants().is_err());
        m.http.as_mut().unwrap().session = Some(crate::session::ServerPolicy {
            origin: "https://api.test".into(),
            client_origin: "https://studio.test".into(),
            max_age_seconds: 3600,
            idle_seconds: 900,
            max_sessions: 2,
            same_site: crate::session::CookieSite::Strict,
            allow_insecure_loopback: false,
        });
        m.validate_ceiling().unwrap();
        assert!(m.validate_package().is_err());
        let bytes = m.to_bytes().unwrap();
        crate::Manifest::parse(&bytes)
            .unwrap()
            .validate_ceiling()
            .unwrap();
        m.abi_version -= 1;
        assert!(m.grants().is_err());
    }
}
