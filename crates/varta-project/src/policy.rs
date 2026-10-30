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
    pub(crate) fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn read(path: &Path) -> Result<Self> {
        Self::from_manifest(Manifest::parse(&artifact::read_bounded(
            path,
            artifact::MAX_MANIFEST_BYTES,
        )?)?)
    }

    pub fn from_manifest(manifest: Manifest) -> Result<Self> {
        manifest.validate_ceiling()?;
        let grants = manifest.grants()?;
        Ok(Self { manifest, grants })
    }
}

/// Authorize each concrete resource against both the artifact and the host,
/// with the file roots the host policy maps.
pub fn run_with_policy(artifact: &Artifact, host: &HostPolicy) -> Result<RunOutput> {
    artifact.grants()?;
    let options =
        varta_runtime::RunOptions::from_manifests(&artifact.manifest, Some(&host.manifest))?;
    Ok(VartaRuntime::new()
        .run_with(&artifact.wasm, options)
        .into_result()?)
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
    // V38 file roots: a declared root is `mapped` when the host policy maps
    // it, `denied` when a host policy does not, `declared` without one.
    let roots: Vec<_> = requested
        .root_names()
        .iter()
        .map(|name| {
            let decision = match host {
                None => "declared",
                Some(host)
                    if host
                        .manifest
                        .root_directories()
                        .iter()
                        .any(|mapping| &mapping.name == name) =>
                {
                    "mapped"
                }
                Some(_) => "denied",
            };
            serde_json::json!({"name":name, "decision":decision})
        })
        .collect();
    Ok(serde_json::json!({
        "mode": if host.is_some() { "host_policy" } else { "declared" },
        "combination": "allOf",
        "requested": requested.capabilities,
        "hostCeiling": host.map(|h| &h.manifest.capabilities),
        "operations": operations,
        "fsRoots": roots,
    }))
}

fn enabled(grant: &CapabilityGrant) -> bool {
    match &grant.value {
        CapabilityValue::Boolean(enabled) => *enabled,
        CapabilityValue::PathGlobs(patterns)
        | CapabilityValue::UrlGlobs(patterns)
        | CapabilityValue::ListenEndpoints(patterns) => !patterns.is_empty(),
        // An empty method list denies every request, like an empty URL list.
        CapabilityValue::Requests(scope) => {
            !scope.urls.is_empty() && scope.methods.as_ref().is_none_or(|m| !m.is_empty())
        }
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
    fn review_reports_whether_the_host_maps_each_declared_root() {
        let requested = manifest(
            r#"{"abiVersion":34,"fsRoots":["project","data"],
            "capabilities":[{"id":"fs.read","patterns":["/project/**","/data/**"]}]}"#,
        );
        let host = HostPolicy::from_manifest(manifest(
            r#"{"abiVersion":34,"fsRoots":{"project":"/srv/app","other":"/srv/other"},
            "capabilities":[{"id":"fs.read","patterns":["/project/**"]}]}"#,
        ))
        .unwrap();
        let review = authorization(&requested, Some(&host)).unwrap();
        assert_eq!(
            review["fsRoots"],
            serde_json::json!([{"name":"project","decision":"mapped"},
                {"name":"data","decision":"denied"}])
        );
        let declared = authorization(&requested, None).unwrap();
        assert_eq!(declared["fsRoots"][0]["decision"], "declared");
        // A host policy that lists names instead of mapping them is invalid.
        assert!(
            HostPolicy::from_manifest(manifest(
                r#"{"abiVersion":34,"fsRoots":["project"],"capabilities":[]}"#
            ))
            .is_err()
        );
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
