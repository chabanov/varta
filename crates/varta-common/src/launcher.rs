//! Runtime companion negotiation. This identifies a trusted native build input;
//! it is not a signature or a guarantee about arbitrary native code.
use crate::{VartaError, abi, http, language, value};
use serde::{Deserialize, Serialize};
/// Version 2 adds an exact list of supported source-semantics profiles;
/// version 3 adds the artifact container formats the companion reads.
pub const PROTOCOL_VERSION: u32 = 3;
/// Artifact container format 1 (the fixed EOF footer's version): Wasm and
/// manifest, no resource section.
pub const CONTAINER_FORMAT: u32 = 1;
/// Artifact container format 2: Wasm, manifest and a resource section.
pub const RESOURCE_CONTAINER_FORMAT: u32 = 2;
/// Every container format this build reads and writes, ascending.
pub const CONTAINER_FORMATS: [u32; 2] = [CONTAINER_FORMAT, RESOURCE_CONTAINER_FORMAT];
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
    /// The container formats the companion reads (protocol 3). Absent from
    /// an older companion's identity, which then reads as none, so a build
    /// that needs a format can name what the companion lacks.
    #[serde(default)]
    pub container_formats: Vec<u32>,
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
            host_abi: abi::PROTECTED_SESSION_VERSION,
            value_abi: value::HASHED_DICTIONARY_VALUE_VERSION,
            http_contract: http::LATEST_VERSION,
            language_profiles: language::SUPPORTED_PROFILES.to_vec(),
            container_formats: CONTAINER_FORMATS.to_vec(),
        }
    }
    /// Refuse a companion that cannot read container `format` before any
    /// output is written: an older companion would otherwise produce a
    /// package that fails at start with an unsupported-format error.
    pub fn require_container_format(&self, format: u32) -> Result<(), VartaError> {
        if self.container_formats.contains(&format) {
            return Ok(());
        }
        Err(VartaError::InvalidArtifact(format!(
            "runtime launcher does not read artifact container format {format} (packages with a resource section); rebuild or update the varta-runtime companion"
        )))
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
            || self.container_formats != expected.container_formats
        {
            return Err(VartaError::InvalidArtifact(
                "runtime launcher version, protocol, target, language profiles or container formats are incompatible"
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
        let mut wrong = LauncherInfo::current();
        wrong.container_formats.pop();
        assert!(wrong.validate_current().is_err());
        let mut value = serde_json::to_value(LauncherInfo::current()).unwrap();
        value["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LauncherInfo>(value).is_err());
    }

    #[test]
    fn a_protocol_2_identity_reads_as_no_container_formats() {
        let mut value = serde_json::to_value(LauncherInfo::current()).unwrap();
        value["protocolVersion"] = serde_json::json!(PROTOCOL_VERSION - 1);
        value.as_object_mut().unwrap().remove("containerFormats");
        let old: LauncherInfo = serde_json::from_value(value).unwrap();
        assert!(old.container_formats.is_empty());
        let error = old
            .require_container_format(RESOURCE_CONTAINER_FORMAT)
            .unwrap_err()
            .to_string();
        assert!(error.contains("container format 2"), "{error}");
        assert!(old.validate_current().is_err());
        let current = LauncherInfo::current();
        for format in CONTAINER_FORMATS {
            current.require_container_format(format).unwrap();
        }
    }
}
