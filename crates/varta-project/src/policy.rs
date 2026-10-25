use crate::{Artifact, Result, artifact};
use std::path::Path;
use varta_common::{CapabilityGrant, CapabilityValue, Manifest};
use varta_runtime::{RunOutput, VartaRuntime};

/// An explicitly supplied, validated host ceiling. Its grants never replace a
/// package's requests and cannot be changed after construction.
pub struct HostPolicy {
    manifest: Manifest,
    grants: Vec<CapabilityGrant>,
}

impl HostPolicy {
    pub(crate) fn grants(&self) -> &[CapabilityGrant] {
        &self.grants
    }
    pub fn read(path: &Path) -> Result<Self> {
        Self::from_manifest(Manifest::parse(&artifact::read_bounded(
            path,
            artifact::MAX_MANIFEST_BYTES,
        )?)?)
    }

    pub fn from_manifest(manifest: Manifest) -> Result<Self> {
        let grants = manifest.grants()?;
        Ok(Self { manifest, grants })
    }
}

/// Authorize each concrete resource against both the artifact and the host.
pub fn run_with_policy(artifact: &Artifact, host: &HostPolicy) -> Result<RunOutput> {
    Ok(VartaRuntime::new().run_wasm_bytes_with_policy(
        &artifact.wasm,
        &artifact.manifest.grants()?,
        &host.grants,
    )?)
}

/// Review the exact filters without approximating wildcard-language intersection.
/// `runtime_check` is not a promise that the two scopes overlap.
pub fn authorization(requested: &Manifest, host: Option<&HostPolicy>) -> Result<serde_json::Value> {
    let requested_grants = requested.grants()?;
    let operations: Vec<_> = requested_grants
        .iter()
        .map(|grant| {
            let ceiling = host.and_then(|host| host.grants.iter().find(|h| h.id == grant.id));
            let decision = if !enabled(grant) || host.is_some() && !ceiling.is_some_and(enabled) {
                "denied"
            } else if matches!(grant.value, CapabilityValue::Boolean(true)) {
                "granted"
            } else {
                "runtime_check"
            };
            serde_json::json!({"id":grant.id, "decision":decision})
        })
        .collect();
    Ok(serde_json::json!({
        "mode": if host.is_some() { "host_policy" } else { "declared" },
        "combination": "allOf",
        "requested": requested.capabilities,
        "hostCeiling": host.map(|h| &h.manifest.capabilities),
        "operations": operations,
    }))
}

fn enabled(grant: &CapabilityGrant) -> bool {
    match &grant.value {
        CapabilityValue::Boolean(enabled) => *enabled,
        CapabilityValue::PathGlobs(patterns) | CapabilityValue::UrlGlobs(patterns) => {
            !patterns.is_empty()
        }
        CapabilityValue::Ports(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest(source: &str) -> Manifest {
        Manifest::parse(source.as_bytes()).unwrap()
    }

    #[test]
    fn review_distinguishes_no_ceiling_from_an_explicit_empty_ceiling() {
        let requested = manifest(r#"{"capabilities":[{"id":"time.now","enabled":true}]}"#);
        let empty = HostPolicy::from_manifest(manifest(r#"{"capabilities":[]}"#)).unwrap();
        let normal = authorization(&requested, None).unwrap();
        let limited = authorization(&requested, Some(&empty)).unwrap();
        assert_eq!(normal["mode"], "declared");
        assert!(normal["hostCeiling"].is_null());
        assert_eq!(normal["operations"][0]["decision"], "granted");
        assert_eq!(limited["hostCeiling"], serde_json::json!([]));
        assert_eq!(limited["operations"][0]["decision"], "denied");
    }

    #[test]
    fn review_does_not_claim_wildcard_overlap_or_invent_requested_rights() {
        let requested = manifest(r#"{"capabilities":[{"id":"env.get","patterns":["APP_*"]}]}"#);
        let host=HostPolicy::from_manifest(manifest(r#"{"capabilities":[{"id":"env.get","patterns":["OTHER_*"]},{"id":"process.exit","enabled":true}]}"#)).unwrap();
        let review = authorization(&requested, Some(&host)).unwrap();
        assert_eq!(review["combination"], "allOf");
        assert_eq!(review["operations"].as_array().unwrap().len(), 1);
        assert_eq!(review["operations"][0]["decision"], "runtime_check");
        assert_eq!(
            review["requested"][0]["patterns"],
            serde_json::json!(["APP_*"])
        );
        assert_eq!(
            review["hostCeiling"][0]["patterns"],
            serde_json::json!(["OTHER_*"])
        );
    }
}
