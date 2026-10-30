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

const MAGIC: &[u8; 8] = b"VARTA3\0\0";
const FORMAT_VERSION: u32 = 1;
const FOOTER_BYTES: usize = 32;
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Payloads are read within the largest module limit of any profile (v38:
/// the Wasm module-size limit); validation then applies the declared
/// profile's own limit.
const MAX_WASM_BYTES: usize = varta_common::limits::MAXIMUM.wasm_bytes;

pub struct Artifact {
    pub wasm: Vec<u8>,
    pub manifest: Manifest,
}

impl Artifact {
    /// Validate the bounded format and matching manifest/code ABI before using
    /// requests, including for artifacts constructed directly by an embedder.
    /// Instruction/signature validation remains the runtime's responsibility.
    pub fn grants(&self) -> Result<Vec<varta_common::CapabilityGrant>> {
        self.validate_payload()?;
        self.manifest.grants()
    }
    pub fn read(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(invalid("expected a regular file"));
        }
        let size = metadata.len();
        let mut head = [0u8; 8];
        file.read_exact(&mut head)?;
        if head == *b"\0asm\x01\0\0\0" {
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
            return Ok(artifact);
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
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Ok(None);
        }
        Self::container(&mut file, size).map(Some)
    }
    fn container(file: &mut File, size: u64) -> Result<Self> {
        let footer_start = size
            .checked_sub(FOOTER_BYTES as u64)
            .ok_or_else(|| invalid("missing container footer"))?;
        file.seek(SeekFrom::Start(footer_start))?;
        let mut footer = [0u8; FOOTER_BYTES];
        file.read_exact(&mut footer)?;
        if &footer[24..] != MAGIC
            || footer[16..20] != FORMAT_VERSION.to_le_bytes()
            || footer[20..24] != [0; 4]
        {
            return Err(invalid(
                "unsupported artifact format; legacy/native AOT payloads are not accepted",
            ));
        }
        let wasm_len = read_u64(&footer[..8])?;
        let manifest_len = read_u64(&footer[8..16])?;
        if wasm_len == 0
            || wasm_len > MAX_WASM_BYTES as u64
            || manifest_len == 0
            || manifest_len > MAX_MANIFEST_BYTES as u64
        {
            return Err(invalid("invalid container lengths"));
        }
        let start = footer_start
            .checked_sub(manifest_len)
            .and_then(|v| v.checked_sub(wasm_len))
            .ok_or_else(|| invalid("container lengths exceed file size"))?;
        file.seek(SeekFrom::Start(start))?;
        // These lengths have been capped above and therefore fit every supported usize.
        let mut wasm = vec![0u8; wasm_len as usize];
        file.read_exact(&mut wasm)?;
        let mut manifest = vec![0u8; manifest_len as usize];
        file.read_exact(&mut manifest)?;
        if !wasm.starts_with(b"\0asm\x01\0\0\0") {
            return Err(invalid("container must contain portable Wasm"));
        }
        let artifact = Self {
            wasm,
            manifest: Manifest::parse(&manifest)?,
        };
        artifact.validate_abi()?;
        Ok(artifact)
    }

    fn validate_payload(&self) -> Result<()> {
        if self.wasm.len() > MAX_WASM_BYTES || !self.wasm.starts_with(b"\0asm\x01\0\0\0") {
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

    pub fn write_wasm(&self, path: &Path) -> Result<()> {
        self.validate_payload()?;
        let manifest = self.manifest.to_bytes()?;
        atomic_write(path, |file| {
            file.write_all(&self.wasm)?;
            Ok(())
        })?;
        atomic_write(&path.with_extension("manifest.json"), |file| {
            file.write_all(&manifest)?;
            Ok(())
        })
    }

    /// Serialize the container around a trusted native prefix. This format-level
    /// API does not execute/verify the prefix; project builds verify a dedicated
    /// runtime companion before calling it.
    pub fn write_executable(&self, path: &Path, launcher: &Path) -> Result<()> {
        self.validate_payload()?;
        let manifest = self.manifest.to_bytes()?;
        atomic_write(path, |file| {
            let mut source = File::open(launcher)?;
            std::io::copy(&mut source, file)?;
            file.write_all(&self.wasm)?;
            file.write_all(&manifest)?;
            file.write_all(&(self.wasm.len() as u64).to_le_bytes())?;
            file.write_all(&(manifest.len() as u64).to_le_bytes())?;
            file.write_all(&FORMAT_VERSION.to_le_bytes())?;
            file.write_all(&0u32.to_le_bytes())?;
            file.write_all(MAGIC)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o755))?;
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
}
