#![forbid(unsafe_code)]
//! Portable artifact/container I/O, independent of source tooling and the VM.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use varta_common::{Manifest, abi};
use varta_common::{Result, VartaError};

mod frames;
mod resources;

pub use resources::{
    FONT_WEIGHT_STEP, FaceError, FontFace, FontStyle, GenericFamily, IMAGE_RESOURCE_VERSION,
    ImageResource, MAX_FAMILY_BYTES, MAX_FONT_BYTES, MAX_FONT_FACE_BYTES, MAX_FONT_FACES,
    MAX_FONT_WEIGHT, MAX_RESOURCE_SECTION_BYTES, MIN_FONT_WEIGHT, NORMAL_FONT_WEIGHT,
    RESOURCE_MAGIC, RESOURCE_VERSION, ResourceError, Resources,
};
/// The packaged image limits, owned by `varta_common::view::images`.
pub use varta_common::view::images::{
    MAX_IMAGE_BYTES, MAX_IMAGE_DECODED_BYTES, MAX_IMAGE_NAME_BYTES, MAX_IMAGE_TOTAL_BYTES,
    MAX_IMAGES,
};

const MAGIC: &[u8; 8] = b"VARTA3\0\0";
/// Container format 1: Wasm and manifest, no resource section.
const FORMAT_VERSION: u32 = varta_common::launcher::CONTAINER_FORMAT;
/// Container format 2: Wasm, manifest and a resource section, whose
/// little-endian `u64` length precedes the fixed footer. The runtime
/// companion advertises the formats it reads (launcher protocol 3).
const RESOURCE_FORMAT_VERSION: u32 = varta_common::launcher::RESOURCE_CONTAINER_FORMAT;
/// The bare form's resource sidecar beside `app.wasm`: `app.resources`.
pub const RESOURCE_SIDECAR_EXTENSION: &str = "resources";
/// The footer's reserved word, zero in formats 1 and 2.
const RESERVED: u32 = 0;
/// Footer fields in order: the little-endian `u64` Wasm and manifest
/// lengths, the `u32` format version, the reserved `u32`, then the magic.
const LENGTH_BYTES: usize = size_of::<u64>();
const WORD_BYTES: usize = size_of::<u32>();
const WASM_LEN_AT: usize = 0;
const MANIFEST_LEN_AT: usize = WASM_LEN_AT + LENGTH_BYTES;
const VERSION_AT: usize = MANIFEST_LEN_AT + LENGTH_BYTES;
const RESERVED_AT: usize = VERSION_AT + WORD_BYTES;
const MAGIC_AT: usize = RESERVED_AT + WORD_BYTES;
const FOOTER_BYTES: usize = MAGIC_AT + MAGIC.len();
/// Permissions of a written executable (`rwxr-xr-x`).
#[cfg(unix)]
const EXECUTABLE_MODE: u32 = 0o755;
pub use varta_common::MAX_MANIFEST_BYTES;
/// Payloads are read within the largest module limit of any profile (v38:
/// the Wasm module-size limit); validation then applies the declared
/// profile's own limit.
const MAX_WASM_BYTES: usize = varta_common::limits::MAXIMUM.wasm_bytes;

pub struct Artifact {
    pub wasm: Vec<u8>,
    pub manifest: Manifest,
}

/// An artifact with its validated resource section (empty when it ships
/// none, as every artifact older than the section).
pub struct PackagedArtifact {
    pub artifact: Artifact,
    pub resources: Resources,
}

impl Artifact {
    /// Validate the bounded format and matching manifest/code ABI before using
    /// requests, including for artifacts constructed directly by an embedder.
    /// Instruction/signature validation remains the runtime's responsibility.
    pub fn grants(&self) -> Result<Vec<varta_common::CapabilityGrant>> {
        self.validate_payload()?;
        self.manifest.grants()
    }
    /// Read and validate an artifact, including its resource section (which
    /// is then dropped): a section that is malformed, tampered with or
    /// packaged with another payload fails here, as in [`Self::read_packaged`].
    pub fn read(path: &Path) -> Result<Self> {
        Self::read_packaged(path).map(|packaged| packaged.artifact)
    }

    /// Read and validate an artifact and its resource section: a bare Wasm
    /// with its sidecar manifest and optional sidecar `*.resources`, or a
    /// container of format 1 (no resources) or 2 (with resources). Unknown
    /// container and section versions are refused.
    pub fn read_packaged(path: &Path) -> Result<PackagedArtifact> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(invalid("expected a regular file"));
        }
        let size = metadata.len();
        let mut head = [0u8; abi::WASM_HEADER.len()];
        file.read_exact(&mut head)?;
        if head == *abi::WASM_HEADER {
            file.seek(SeekFrom::Start(0))?;
            let mut wasm = Vec::new();
            (&mut file)
                .take(MAX_WASM_BYTES as u64 + 1)
                .read_to_end(&mut wasm)?;
            if wasm.len() > MAX_WASM_BYTES {
                return Err(invalid("file exceeds size limit"));
            }
            let manifest_path = path.with_extension("manifest.json");
            let manifest = match read_bounded(&manifest_path, MAX_MANIFEST_BYTES) {
                Ok(bytes) => Manifest::parse(&bytes)?,
                Err(VartaError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut manifest = Manifest::new(&[]);
                    manifest.abi_version = host_abi(&wasm)?;
                    manifest
                }
                Err(error) => return Err(error),
            };
            let artifact = Self { wasm, manifest };
            artifact.validate_abi()?;
            let resources = match read_bounded(
                &path.with_extension(RESOURCE_SIDECAR_EXTENSION),
                MAX_RESOURCE_SECTION_BYTES,
            ) {
                Ok(bytes) => Resources::decode(&bytes, &artifact.wasm)?,
                Err(VartaError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    Resources::default()
                }
                Err(error) => return Err(error),
            };
            return Ok(PackagedArtifact {
                artifact,
                resources,
            });
        }
        Self::container(&mut file, size)
    }

    /// Read only a fixed EOF capsule. Ordinary bare executables return None;
    /// a recognized but malformed capsule is always an error.
    pub fn embedded(path: &Path) -> Result<Option<Self>> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(invalid("expected a regular file"));
        }
        let size = metadata.len();
        if size < FOOTER_BYTES as u64 {
            return Ok(None);
        }
        file.seek(SeekFrom::End(-(MAGIC.len() as i64)))?;
        let mut magic = [0; MAGIC.len()];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Ok(None);
        }
        Self::container(&mut file, size).map(|packaged| Some(packaged.artifact))
    }
    fn container(file: &mut File, size: u64) -> Result<PackagedArtifact> {
        let footer_start = size
            .checked_sub(FOOTER_BYTES as u64)
            .ok_or_else(|| invalid("missing container footer"))?;
        file.seek(SeekFrom::Start(footer_start))?;
        let mut footer = [0u8; FOOTER_BYTES];
        file.read_exact(&mut footer)?;
        let version = read_u32(&footer[VERSION_AT..RESERVED_AT])?;
        if &footer[MAGIC_AT..] != MAGIC
            || !varta_common::launcher::CONTAINER_FORMATS.contains(&version)
            || footer[RESERVED_AT..MAGIC_AT] != RESERVED.to_le_bytes()
        {
            return Err(invalid(
                "unsupported artifact format; legacy/native AOT payloads are not accepted",
            ));
        }
        let wasm_len = read_u64(&footer[WASM_LEN_AT..MANIFEST_LEN_AT])?;
        let manifest_len = read_u64(&footer[MANIFEST_LEN_AT..VERSION_AT])?;
        if wasm_len == 0
            || wasm_len > MAX_WASM_BYTES as u64
            || manifest_len == 0
            || manifest_len > MAX_MANIFEST_BYTES as u64
        {
            return Err(invalid("invalid container lengths"));
        }
        let (payload_end, resources_len) = if version == RESOURCE_FORMAT_VERSION {
            let length_start = footer_start
                .checked_sub(LENGTH_BYTES as u64)
                .ok_or_else(|| invalid("container lengths exceed file size"))?;
            file.seek(SeekFrom::Start(length_start))?;
            let mut length = [0u8; LENGTH_BYTES];
            file.read_exact(&mut length)?;
            let resources_len = read_u64(&length)?;
            if resources_len == 0 || resources_len > MAX_RESOURCE_SECTION_BYTES as u64 {
                return Err(invalid("invalid container lengths"));
            }
            (length_start, resources_len)
        } else {
            (footer_start, 0)
        };
        let start = payload_end
            .checked_sub(resources_len)
            .and_then(|v| v.checked_sub(manifest_len))
            .and_then(|v| v.checked_sub(wasm_len))
            .ok_or_else(|| invalid("container lengths exceed file size"))?;
        file.seek(SeekFrom::Start(start))?;
        // These lengths have been capped above and therefore fit every supported usize.
        let mut wasm = vec![0u8; wasm_len as usize];
        file.read_exact(&mut wasm)?;
        let mut manifest = vec![0u8; manifest_len as usize];
        file.read_exact(&mut manifest)?;
        // Capped above by the section limit.
        let mut section = vec![0u8; resources_len as usize];
        file.read_exact(&mut section)?;
        if !wasm.starts_with(abi::WASM_HEADER) {
            return Err(invalid("container must contain portable Wasm"));
        }
        let artifact = Self {
            wasm,
            manifest: Manifest::parse(&manifest)?,
        };
        artifact.validate_abi()?;
        let resources = if section.is_empty() {
            Resources::default()
        } else {
            Resources::decode(&section, &artifact.wasm)?
        };
        Ok(PackagedArtifact {
            artifact,
            resources,
        })
    }

    fn validate_payload(&self) -> Result<()> {
        if self.wasm.len() > MAX_WASM_BYTES || !self.wasm.starts_with(abi::WASM_HEADER) {
            return Err(invalid("artifact must contain bounded portable Wasm"));
        }
        if self.manifest.to_bytes()?.len() > MAX_MANIFEST_BYTES {
            return Err(invalid("manifest exceeds size limit"));
        }
        self.validate_abi()
    }
    fn validate_abi(&self) -> Result<()> {
        if self.manifest.abi_version != host_abi(&self.wasm)? {
            return Err(invalid("manifest and Wasm host ABI versions differ"));
        }
        // A package names its file roots and, under v38, requests virtual
        // paths under them; it never maps a root to a host directory.
        self.manifest.validate_package()?;
        // The resumable-frame surface needs a profile that admits it.
        frames::validate(&self.wasm)
    }

    /// Write the bare form without resources (removing a stale
    /// `*.resources` sidecar), as [`Self::write_wasm_with`].
    pub fn write_wasm(&self, path: &Path) -> Result<()> {
        self.write_wasm_with(path, &Resources::default())
    }

    /// Write the bare form: the Wasm, its sidecar manifest and, when the
    /// artifact ships resources, the sidecar resource section bound to this
    /// Wasm; otherwise an existing sidecar section is removed, so no stale
    /// section survives a rebuild. Each file is replaced atomically; an
    /// interrupted write can leave a section bound to another payload, which
    /// readers refuse.
    pub fn write_wasm_with(&self, path: &Path, resources: &Resources) -> Result<()> {
        self.validate_payload()?;
        let manifest = self.manifest.to_bytes()?;
        let section = resources.encode(&self.wasm);
        atomic_write(path, |file| {
            file.write_all(&self.wasm)?;
            Ok(())
        })?;
        atomic_write(&path.with_extension("manifest.json"), |file| {
            file.write_all(&manifest)?;
            Ok(())
        })?;
        let sidecar = path.with_extension(RESOURCE_SIDECAR_EXTENSION);
        match section {
            Some(section) => atomic_write(&sidecar, |file| {
                file.write_all(&section)?;
                Ok(())
            }),
            None => match fs::remove_file(&sidecar) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
                _ => Ok(()),
            },
        }
    }

    /// Serialize the container around a trusted native prefix. This format-level
    /// API does not execute/verify the prefix; project builds verify a dedicated
    /// runtime companion before calling it.
    pub fn write_executable(&self, path: &Path, launcher: &Path) -> Result<()> {
        self.write_executable_with(path, launcher, &Resources::default())
    }

    /// Serialize the container with a resource section: format 2 when the
    /// artifact ships resources, otherwise the unchanged format 1 bytes.
    pub fn write_executable_with(
        &self,
        path: &Path,
        launcher: &Path,
        resources: &Resources,
    ) -> Result<()> {
        self.validate_payload()?;
        let manifest = self.manifest.to_bytes()?;
        let section = resources.encode(&self.wasm);
        atomic_write(path, |file| {
            let mut source = File::open(launcher)?;
            std::io::copy(&mut source, file)?;
            file.write_all(&self.wasm)?;
            file.write_all(&manifest)?;
            let version = match &section {
                Some(section) => {
                    file.write_all(section)?;
                    file.write_all(&(section.len() as u64).to_le_bytes())?;
                    RESOURCE_FORMAT_VERSION
                }
                None => FORMAT_VERSION,
            };
            file.write_all(&(self.wasm.len() as u64).to_le_bytes())?;
            file.write_all(&(manifest.len() as u64).to_le_bytes())?;
            file.write_all(&version.to_le_bytes())?;
            file.write_all(&RESERVED.to_le_bytes())?;
            file.write_all(MAGIC)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(EXECUTABLE_MODE))?;
            }
            Ok(())
        })
    }
}

fn host_abi(wasm: &[u8]) -> Result<u32> {
    let mut version = None;
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        if let wasmparser::Payload::CustomSection(section) =
            payload.map_err(|e| invalid(&e.to_string()))?
            && section.name() == abi::SECTION
        {
            let declared = section.data().try_into().ok().map(u32::from_le_bytes);
            if version.is_some() || !declared.is_some_and(abi::supported) {
                return Err(invalid("unsupported or duplicate Wasm host ABI"));
            }
            version = declared;
        }
    }
    Ok(version.unwrap_or(abi::VERSION))
}

fn invalid(message: &str) -> VartaError {
    VartaError::InvalidArtifact(message.into())
}
fn read_u32(bytes: &[u8]) -> Result<u32> {
    let bytes = bytes
        .try_into()
        .map_err(|_| invalid("invalid version field"))?;
    Ok(u32::from_le_bytes(bytes))
}
fn read_u64(bytes: &[u8]) -> Result<u64> {
    let bytes = bytes
        .try_into()
        .map_err(|_| invalid("invalid length field"))?;
    Ok(u64::from_le_bytes(bytes))
}

pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("expected a regular file"));
    }
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("file exceeds size limit"));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| VartaError::InvalidArtifact(e.to_string()))?
        .as_nanos();
    let temporary = parent.join(format!(".varta-{}-{nonce}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        write(&mut file)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "varta-artifact-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn empty() -> Artifact {
        Artifact {
            wasm: b"\0asm\x01\0\0\0".to_vec(),
            manifest: Manifest::new(&[]),
        }
    }
    #[test]
    fn length_decoding_is_fixed_width_and_checked() {
        assert_eq!(read_u64(&u64::MAX.to_le_bytes()).unwrap(), u64::MAX);
        assert!(read_u64(&[0; 4]).is_err());
    }
    #[test]
    fn manifest_abi_matches_payload_and_missing_sidecars_still_mean_zero_grants() {
        let temp = Temp::new();
        let path = temp.0.join("number.wasm");
        let mut artifact = empty();
        artifact.wasm.extend([0, 14, 9]); // fixed custom-section/name lengths for varta.abi
        artifact.wasm.extend(abi::SECTION.as_bytes());
        artifact.wasm.extend(abi::NUMBER_VERSION.to_le_bytes());
        assert!(artifact.grants().is_err());
        assert!(artifact.write_wasm(&path).is_err());
        artifact.manifest.abi_version = abi::NUMBER_VERSION;
        artifact.write_wasm(&path).unwrap();
        assert_eq!(
            Artifact::read(&path).unwrap().manifest.abi_version,
            abi::NUMBER_VERSION
        );
        fs::remove_file(path.with_extension("manifest.json")).unwrap();
        let inferred = Artifact::read(&path).unwrap();
        assert_eq!(inferred.manifest.abi_version, abi::NUMBER_VERSION);
        assert!(inferred.manifest.capabilities.is_empty());
        fs::write(
            path.with_extension("manifest.json"),
            empty().manifest.to_bytes().unwrap(),
        )
        .unwrap();
        assert!(Artifact::read(&path).is_err());
    }
    #[test]
    fn fixed_footer_preserves_legacy_prefixes_and_embedded_manifest_authority() {
        let temp = Temp::new();
        let prefix = temp.0.join("prefix");
        fs::write(
            &prefix,
            b"old development executable VARTA3\0\0 internal marker",
        )
        .unwrap();
        assert!(Artifact::embedded(&prefix).unwrap().is_none());
        let path = temp.0.join("app");
        empty().write_executable(&path, &prefix).unwrap();
        fs::write(path.with_extension("manifest.json"), b"malformed sidecar").unwrap();
        for artifact in [
            Artifact::read(&path).unwrap(),
            Artifact::embedded(&path).unwrap().unwrap(),
        ] {
            assert_eq!(artifact.wasm, empty().wasm);
            assert!(artifact.manifest.capabilities.is_empty());
        }
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            bytes.len(),
            fs::metadata(prefix).unwrap().len() as usize
                + empty().wasm.len()
                + empty().manifest.to_bytes().unwrap().len()
                + FOOTER_BYTES
        );
        for (offset, value) in [
            (0, u64::MAX.to_le_bytes().to_vec()),
            (16, 2u32.to_le_bytes().to_vec()),
            (16, 3u32.to_le_bytes().to_vec()),
            (20, 1u32.to_le_bytes().to_vec()),
        ] {
            let mut invalid = bytes.clone();
            let start = invalid.len() - FOOTER_BYTES + offset;
            invalid[start..start + value.len()].copy_from_slice(&value);
            fs::write(&path, invalid).unwrap();
            assert!(Artifact::read(&path).is_err());
            assert!(Artifact::embedded(&path).is_err());
        }
    }
    #[test]
    fn payload_bounds_fail_before_replacing_outputs() {
        let temp = Temp::new();
        let path = temp.0.join("app.wasm");
        empty().write_wasm(&path).unwrap();
        for wasm in [
            b"native AOT payload".to_vec(),
            vec![0; abi::MAX_WASM_BYTES + 1],
        ] {
            let artifact = Artifact {
                wasm,
                manifest: Manifest::new(&[]),
            };
            assert!(artifact.write_wasm(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), empty().wasm);
        }
        fs::write(path.with_extension("manifest.json"), b"malformed").unwrap();
        assert!(Artifact::read(&path).is_err());
        assert!(read_bounded(&path, empty().wasm.len() - 1).is_err());
        assert!(read_bounded(&temp.0, MAX_MANIFEST_BYTES).is_err());
    }

    fn fonts() -> Resources {
        Resources::new(vec![
            FontFace {
                family: "Varta Fixture 400".into(),
                weight: NORMAL_FONT_WEIGHT,
                style: FontStyle::Normal,
                generics: vec![GenericFamily::SansSerif],
                data: include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf").to_vec(),
            },
            FontFace {
                family: "Varta Fixture 400".into(),
                weight: NORMAL_FONT_WEIGHT,
                style: FontStyle::Italic,
                generics: vec![GenericFamily::SansSerif],
                data: include_bytes!("../../varta-view/tests/fixtures/italic-900.ttf").to_vec(),
            },
        ])
        .unwrap()
    }

    #[test]
    fn containers_carry_a_resource_section_and_keep_format_one_without_one() {
        let temp = Temp::new();
        let prefix = temp.0.join("prefix");
        fs::write(&prefix, b"native prefix").unwrap();
        let plain = temp.0.join("plain");
        empty().write_executable(&plain, &prefix).unwrap();
        let with_fonts = temp.0.join("fonts");
        empty()
            .write_executable_with(&with_fonts, &prefix, &fonts())
            .unwrap();
        // Without resources the bytes stay format 1.
        let bytes = fs::read(&plain).unwrap();
        assert_eq!(
            bytes[bytes.len() - FOOTER_BYTES + VERSION_AT..][..WORD_BYTES],
            FORMAT_VERSION.to_le_bytes()
        );
        assert!(
            Artifact::read_packaged(&plain)
                .unwrap()
                .resources
                .is_empty()
        );
        let packaged = Artifact::read_packaged(&with_fonts).unwrap();
        assert_eq!(packaged.resources, fonts());
        assert_eq!(packaged.artifact.wasm, empty().wasm);
        assert_eq!(
            Artifact::embedded(&with_fonts).unwrap().unwrap().wasm,
            empty().wasm
        );
        let bytes = fs::read(&with_fonts).unwrap();
        let section = fonts().encode(&empty().wasm).unwrap();
        assert_eq!(
            bytes.len(),
            fs::metadata(&prefix).unwrap().len() as usize
                + empty().wasm.len()
                + empty().manifest.to_bytes().unwrap().len()
                + section.len()
                + LENGTH_BYTES
                + FOOTER_BYTES
        );
        // A tampered section, an unknown version and a bad section length
        // are refused by both readers before anything runs.
        let footer = bytes.len() - FOOTER_BYTES;
        let length_at = footer - LENGTH_BYTES;
        for (at, value) in [
            (length_at - 1, vec![bytes[length_at - 1] ^ 1]),
            (footer + VERSION_AT, 3u32.to_le_bytes().to_vec()),
            (footer + VERSION_AT, FORMAT_VERSION.to_le_bytes().to_vec()),
            (length_at, 0u64.to_le_bytes().to_vec()),
            (
                length_at,
                (MAX_RESOURCE_SECTION_BYTES as u64 + 1)
                    .to_le_bytes()
                    .to_vec(),
            ),
            (length_at, u64::MAX.to_le_bytes().to_vec()),
        ] {
            let mut invalid = bytes.clone();
            invalid[at..at + value.len()].copy_from_slice(&value);
            fs::write(&with_fonts, invalid).unwrap();
            assert!(Artifact::read(&with_fonts).is_err(), "{at}");
            assert!(Artifact::embedded(&with_fonts).is_err(), "{at}");
        }
    }

    #[test]
    fn bare_artifacts_carry_a_bound_sidecar_section_and_drop_stale_ones() {
        let temp = Temp::new();
        let path = temp.0.join("app.wasm");
        let sidecar = path.with_extension(RESOURCE_SIDECAR_EXTENSION);
        empty().write_wasm_with(&path, &fonts()).unwrap();
        assert_eq!(Artifact::read_packaged(&path).unwrap().resources, fonts());
        // A section packaged with another payload never travels to this one.
        let mut other = empty();
        other.wasm.extend([0, 1, 0]);
        let foreign = fonts().encode(&other.wasm).unwrap();
        fs::write(&sidecar, &foreign).unwrap();
        assert!(Artifact::read_packaged(&path).is_err());
        assert!(Artifact::read(&path).is_err());
        let mut tampered = fonts().encode(&empty().wasm).unwrap();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        fs::write(&sidecar, &tampered).unwrap();
        assert!(Artifact::read(&path).is_err());
        // Rebuilding without resources removes the sidecar.
        empty().write_wasm(&path).unwrap();
        assert!(!sidecar.exists());
        assert!(Artifact::read_packaged(&path).unwrap().resources.is_empty());
        // A missing sidecar is no resources; an unreadable one is an error.
        fs::create_dir(&sidecar).unwrap();
        assert!(Artifact::read(&path).is_err());
    }
}
