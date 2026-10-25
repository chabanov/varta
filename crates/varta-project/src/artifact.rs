use crate::{ProjectError, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use varta_common::{Manifest, abi};

const MAGIC: &[u8; 8] = b"VARTA3\0\0";
const FORMAT_VERSION: u32 = 1;
const FOOTER_BYTES: usize = 32;
pub(crate) const MAX_MANIFEST_BYTES: usize = 256 * 1024;

pub struct Artifact {
    pub wasm: Vec<u8>,
    pub manifest: Manifest,
}

impl Artifact {
    pub fn read(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let size = file.metadata()?.len();
        let mut head = [0u8; 8];
        file.read_exact(&mut head)?;
        if head == *b"\0asm\x01\0\0\0" {
            let wasm = read_bounded(path, abi::MAX_WASM_BYTES)?;
            let manifest_path = path.with_extension("manifest.json");
            let manifest = match read_bounded(&manifest_path, MAX_MANIFEST_BYTES) {
                Ok(bytes) => Manifest::parse(&bytes)?,
                Err(ProjectError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    Manifest::new(&[])
                }
                Err(error) => return Err(error),
            };
            return Ok(Self { wasm, manifest });
        }
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
            || wasm_len > abi::MAX_WASM_BYTES as u64
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
        Ok(Self {
            wasm,
            manifest: Manifest::parse(&manifest)?,
        })
    }

    pub fn write_wasm(&self, path: &Path) -> Result<()> {
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

    pub fn write_executable(&self, path: &Path, launcher: &Path) -> Result<()> {
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

fn invalid(message: &str) -> ProjectError {
    ProjectError::Artifact(message.into())
}
fn read_u64(bytes: &[u8]) -> Result<u64> {
    let bytes = bytes
        .try_into()
        .map_err(|_| invalid("invalid length field"))?;
    Ok(u64::from_le_bytes(bytes))
}

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
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
        .map_err(|e| ProjectError::Artifact(e.to_string()))?
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
    #[test]
    fn length_decoding_is_fixed_width_and_checked() {
        assert_eq!(read_u64(&u64::MAX.to_le_bytes()).unwrap(), u64::MAX);
        assert!(read_u64(&[0; 4]).is_err());
    }
}
