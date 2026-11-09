use serde::{Deserialize, Serialize};
use varta_common::{CapabilityGrant, CapabilityValue, RequestScope};
pub const DEPENDENCY_POLICY: &str = "app-data-v1";
pub const POLICY_ID: &str = "scoped-json-v1";
pub const DATA_URL_ENV: &str = "VARTA_APP_DATA_URL";
pub const TOKEN_ENV: &str = "VARTA_APPLICATION_TOKEN";
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApplicationRequest {
    pub public_origin: String,
    pub access: Access,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Read,
    ReadWrite,
}
impl Access {
    pub fn methods(self) -> Vec<String> {
        match self {
            Self::Read => vec!["GET".into()],
            Self::ReadWrite => vec!["GET".into(), "PATCH".into(), "POST".into()],
        }
    }
}
impl ApplicationRequest {
    pub fn validate(&self) -> crate::toolchain::Result<()> {
        if !varta_common::session::origin(&self.public_origin, true)
            || !self.public_origin.starts_with("http://localhost:")
        {
            return Err(crate::toolchain::ToolchainError::Invalid(
                "local app origin",
            ));
        }
        Ok(())
    }
    pub fn grants(&self) -> Vec<CapabilityGrant> {
        vec![
            CapabilityGrant {
                id: "env.get".into(),
                value: CapabilityValue::PathGlobs(vec![DATA_URL_ENV.into()]),
            },
            CapabilityGrant {
                id: varta_common::session::CLIENT_CAPABILITY.into(),
                value: CapabilityValue::Requests(RequestScope {
                    urls: vec![format!("{}/runs/**", self.public_origin)],
                    methods: Some(self.access.methods()),
                    response_headers: Some(vec!["content-type".into()]),
                }),
            },
        ]
    }
    pub(crate) fn ceiling(&self, id: &str) -> varta_common::Manifest {
        let mut manifest = varta_common::Manifest::new(&self.grants());
        manifest.abi_version = varta_common::abi::PROTECTED_SESSION_VERSION;
        if let Some(grant) = manifest
            .capabilities
            .iter_mut()
            .find(|g| g.id == varta_common::session::CLIENT_CAPABILITY)
        {
            grant.patterns = Some(vec![format!("{}/runs/{id}/data/**", self.public_origin)]);
        }
        manifest.browser_session = Some(varta_common::session::ClientBinding {
            api_origin: self.public_origin.clone(),
            client_origin: self.public_origin.clone(),
            allow_insecure_loopback: true,
        });
        manifest
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApplicationCeiling {
    pub host: crate::preview::PreviewCeiling,
    pub public_origin: String,
    pub native_origin: String,
    pub native_control_token_env: String,
    pub path_prefix: String,
}
impl ApplicationCeiling {
    pub fn read(path: &std::path::Path) -> crate::preview::Result<Self> {
        use std::io::Read;
        let mut b = Vec::new();
        std::fs::File::open(path)?
            .take((crate::preview::MAX_JSON_BYTES + 1) as u64)
            .read_to_end(&mut b)?;
        if b.len() > crate::preview::MAX_JSON_BYTES {
            return Err(crate::preview::PreviewError::Invalid(
                "application ceiling size",
            ));
        }
        let value: Self = serde_json::from_slice(&b)?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> crate::preview::Result<()> {
        self.host.validate()?;
        let public = url::Url::parse(&self.public_origin)
            .map_err(|_| crate::preview::PreviewError::Invalid("public origin"))?;
        if !varta_common::session::origin(&self.public_origin, true)
            || !self.public_origin.starts_with("http://localhost:")
            || public.port() != Some(self.host.public_listen.port())
            || self.host.public_listen.port() == 0
            || self.host.protected_origins.iter().any(|origin| {
                url::Url::parse(origin).is_ok_and(|url| url.host_str() == public.host_str())
            })
        {
            return Err(crate::preview::PreviewError::Invalid(
                "isolated public application origin",
            ));
        }

        let url = url::Url::parse(&self.native_origin)
            .map_err(|_| crate::preview::PreviewError::Invalid("native origin"))?;
        if url.scheme() != "http"
            || url.origin().ascii_serialization() != self.native_origin
            || url
                .host_str()
                .and_then(|h| h.parse::<std::net::Ipv4Addr>().ok())
                .is_none_or(|ip| !ip.is_loopback())
            || url.port().is_none()
            || !self.host.protected_origins.contains(&self.native_origin)
            || !varta_common::session::delegation::prefix(&self.path_prefix)
            || self.native_control_token_env.is_empty()
            || self.native_control_token_env.len() > 64
            || !self
                .native_control_token_env
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(crate::preview::PreviewError::Invalid(
                "application upstream ceiling",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChannelStart {
    pub schema_version: u32,
    pub host_sha256: String,
    pub bundle_sha256: String,
    pub target: String,
    pub policy_id: String,
    pub ttl_seconds: u64,
    pub delegation_id: String,
    pub binding_id: String,
    pub context_sha256: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct NativeScope {
    pub schema_version: u32,
    pub id: String,
    pub binding_id: String,
    pub context_sha256: String,
    pub parent_generation: String,
    pub state: String,
    pub expires_at_ms: u64,
    pub path_prefix: String,
    pub methods: Vec<String>,
}

pub(crate) fn artifact_request(
    manifest: &varta_common::Manifest,
    origin: &str,
) -> crate::preview::Result<ApplicationRequest> {
    for access in [Access::Read, Access::ReadWrite] {
        let app = ApplicationRequest {
            public_origin: origin.into(),
            access,
        };
        if manifest
            .grants()
            .map_err(|_| crate::preview::PreviewError::Invalid("application grants"))?
            == app.grants()
            && manifest.fs_roots.is_none()
            && manifest.http.is_none()
            && manifest.browser_session.is_none()
            && manifest.budgets.is_none()
            && manifest.target.as_deref() == Some("wasm")
            && manifest
                .capabilities
                .iter()
                .all(|g| g.required.is_none() && g.purpose.is_none())
        {
            return Ok(app);
        }
    }
    Err(crate::preview::PreviewError::Invalid(
        "application artifact request policy",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_policy_only_admits_the_bound_public_request_and_no_host_authority() {
        let app = ApplicationRequest {
            public_origin: "http://localhost:8810".into(),
            access: Access::Read,
        };
        app.validate().unwrap();
        let mut manifest = varta_common::Manifest::new(&app.grants());
        manifest.abi_version = varta_common::abi::PROTECTED_SESSION_VERSION;
        manifest.target = Some("wasm".into());
        assert!(artifact_request(&manifest, &app.public_origin).is_ok());
        assert!(artifact_request(&manifest, "http://localhost:8811").is_err());
        manifest.http = Some(Default::default());
        assert!(artifact_request(&manifest, &app.public_origin).is_err());
        manifest.http = None;
        manifest.capabilities[0].patterns = Some(vec!["*".into()]);
        assert!(artifact_request(&manifest, &app.public_origin).is_err());
        let ceiling = app.ceiling(&"a".repeat(64));
        let scope = ceiling
            .capabilities
            .iter()
            .find(|g| g.id == varta_common::session::CLIENT_CAPABILITY)
            .unwrap();
        assert_eq!(
            scope.patterns.as_ref().unwrap(),
            &[format!(
                "http://localhost:8810/runs/{}/data/**",
                "a".repeat(64)
            )]
        );
        assert_eq!(scope.methods.as_ref().unwrap(), &["GET"]);
    }
}
