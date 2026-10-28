//! Runtime companion negotiation. This identifies a trusted native build input;
//! it is not a signature or a guarantee about arbitrary native code.
use crate::{VartaError, abi, http, language, value};
use serde::{Deserialize, Serialize};
/// Version 2 adds an exact list of supported source-semantics profiles.
pub const PROTOCOL_VERSION: u32 = 2;
pub const KIND: &str = "varta-runtime";
pub const INFO_ARGUMENT: &str = "--runtime-info";
pub const VALIDATE_ARGUMENT: &str = "--validate";
pub const MAX_PROBE_BYTES: usize = 4096;
pub const PROBE_SECONDS: u64 = 5;
pub const MAX_LAUNCHER_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LauncherInfo {
    pub kind: String,
    pub protocol_version: u32,
    pub package_version: String,
    pub os: String,
    pub arch: String,
    pub pointer_width: u32,
    pub host_abi: u32,
    pub value_abi: u32,
    pub http_contract: u32,
    pub language_profiles: Vec<language::LanguageProfile>,
}
impl Default for LauncherInfo {
    fn default() -> Self {
        Self::current()
    }
}
impl LauncherInfo {
    #[must_use]
    pub fn current() -> Self {
        Self {
            kind: KIND.into(),
            protocol_version: PROTOCOL_VERSION,
            package_version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            pointer_width: usize::BITS,
            // Advertised maxima; the profile and payload probe select the pair.
            host_abi: abi::NUMBER_PARSE_VERSION,
            value_abi: value::SPARSE_VALUE_VERSION,
            http_contract: http::VERSION,
            language_profiles: language::SUPPORTED_PROFILES.to_vec(),
        }
    }
    pub fn validate_current(&self) -> Result<(), VartaError> {
        let expected = Self::current();
        if self.kind != expected.kind
            || self.protocol_version != expected.protocol_version
            || self.package_version != expected.package_version
            || self.os != expected.os
            || self.arch != expected.arch
            || self.pointer_width != expected.pointer_width
            || self.host_abi != expected.host_abi
            || self.value_abi != expected.value_abi
            || self.http_contract != expected.http_contract
            || self.language_profiles != expected.language_profiles
        {
            return Err(VartaError::InvalidArtifact(
                "runtime launcher version, protocol, target or language profiles are incompatible"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_requires_matching_kind_version_target_and_abi() {
        LauncherInfo::current().validate_current().unwrap();
        let mut wrong = LauncherInfo::current();
        wrong.arch = "other".into();
        assert!(wrong.validate_current().is_err());
        let mut wrong = LauncherInfo::current();
        wrong.protocol_version += 1;
        assert!(wrong.validate_current().is_err());
        let mut wrong = LauncherInfo::current();
        wrong.kind = "varta".into();
        assert!(wrong.validate_current().is_err());
        let mut wrong = LauncherInfo::current();
        wrong.language_profiles.clear();
        assert!(wrong.validate_current().is_err());
        let mut value = serde_json::to_value(LauncherInfo::current()).unwrap();
        value["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LauncherInfo>(value).is_err());
    }
}
