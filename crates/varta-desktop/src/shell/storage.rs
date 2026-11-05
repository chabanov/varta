//! Host-owned KV storage: one directory per application identity.
//!
//! A v38 package names its file roots and requests `kv.read`/`kv.write` on
//! virtual paths under them; it never chooses a host directory. The shell
//! maps every root that a package `kv.*` pattern names, and that the host
//! ceiling does not map itself, to `<base>/sha256-<digest>/<root>`, a
//! directory the shell creates for that artifact hash only. A different
//! artifact (any changed byte of its Wasm) gets a different directory. The
//! mapping grants nothing: every operation still needs the exact grant in
//! the package request and in the ceiling.
use super::ShellError;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use varta_common::{CapabilityValue, Manifest, files, kv, permission};
use varta_runtime::{FileRoots, RuntimeError};

/// The directory-name prefix of one artifact's storage, followed by the
/// lowercase hex SHA-256 digest of its Wasm (`:` is not portable in names).
pub const STORAGE_DIRECTORY_PREFIX: &str = "sha256-";
/// Only the user may list or enter storage directories.
#[cfg(unix)]
const STORAGE_DIRECTORY_MODE: u32 = 0o700;
/// The storage folder below the platform data directory.
const VENDOR_DIRECTORY: &str = "Varta";
const XDG_VENDOR_DIRECTORY: &str = "varta";
const STORAGE_DIRECTORY: &str = "storage";

/// The storage directory of `identity` (a `sha256:` application identity)
/// under `base`, or `None` for a malformed identity.
#[must_use]
pub fn storage_directory(base: &Path, identity: &str) -> Option<PathBuf> {
    if !permission::valid_artifact(identity) {
        return None;
    }
    let digest = identity.strip_prefix(permission::ARTIFACT_PREFIX)?;
    Some(base.join(format!("{STORAGE_DIRECTORY_PREFIX}{digest}")))
}

/// The platform's per-user application data directory for shell storage:
/// `~/Library/Application Support/Varta/storage` on macOS,
/// `$XDG_DATA_HOME/varta/storage` (or `~/.local/share/varta/storage`)
/// elsewhere on Unix and `%APPDATA%\Varta\storage` on Windows.
pub fn default_storage_base() -> Result<PathBuf, ShellError> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = if cfg!(target_os = "macos") {
        absolute("HOME").map(|home| {
            home.join("Library")
                .join("Application Support")
                .join(VENDOR_DIRECTORY)
        })
    } else if cfg!(windows) {
        absolute("APPDATA").map(|data| data.join(VENDOR_DIRECTORY))
    } else {
        absolute("XDG_DATA_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".local").join("share")))
            .map(|data| data.join(XDG_VENDOR_DIRECTORY))
    };
    base.map(|base| base.join(STORAGE_DIRECTORY))
        .ok_or(ShellError::NoStorageBase)
}

/// The roots that a package `kv.*` pattern names.
fn kv_roots(package: &Manifest) -> Result<BTreeSet<String>, ShellError> {
    let grants = package.grants().map_err(RuntimeError::from)?;
    let mut roots = BTreeSet::new();
    for grant in grants {
        if grant.id != kv::READ_CAPABILITY && grant.id != kv::WRITE_CAPABILITY {
            continue;
        }
        if let CapabilityValue::PathGlobs(patterns) = grant.value {
            for pattern in patterns {
                // Package validation already requires a literal declared root.
                if let Some(root) = pattern
                    .strip_prefix(files::SEPARATOR)
                    .and_then(|rest| rest.split(files::SEPARATOR).next())
                    .filter(|root| files::valid_root_name(root))
                {
                    roots.insert(root.to_owned());
                }
            }
        }
    }
    Ok(roots)
}

/// Pin the KV storage roots of `package` under `base`. A root the ceiling
/// maps keeps that explicit mapping. Nothing is created when the ceiling
/// leaves no `kv.read` (no database could ever be opened).
pub(super) fn pin(
    roots: &mut FileRoots,
    package: &Manifest,
    ceiling: Option<&Manifest>,
    base: &Path,
    identity: &str,
) -> Result<(), ShellError> {
    let wanted = kv_roots(package)?;
    if wanted.is_empty() {
        return Ok(());
    }
    if let Some(ceiling) = ceiling {
        let grants = ceiling.grants().map_err(RuntimeError::from)?;
        if !grants.iter().any(|grant| grant.id == kv::READ_CAPABILITY) {
            return Ok(());
        }
    }
    if !base.is_absolute() {
        return Err(ShellError::StorageNotAbsolute(base.to_path_buf()));
    }
    let directory = storage_directory(base, identity).ok_or_else(|| ShellError::Storage {
        path: base.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("malformed application identity {identity}"),
        ),
    })?;
    let mapped: BTreeSet<String> = roots.names().map(str::to_owned).collect();
    for root in wanted.difference(&mapped) {
        let path = directory.join(root);
        create_private(&path)?;
        roots.open(root, &path)?;
    }
    Ok(())
}

/// Create `path` and its missing parents, private to the user on Unix.
fn create_private(path: &Path) -> Result<(), ShellError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(STORAGE_DIRECTORY_MODE);
    }
    builder.create(path).map_err(|source| ShellError::Storage {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_directories_are_keyed_by_the_exact_artifact_digest() {
        let base = Path::new("/srv/varta");
        let identity = varta_runtime::artifact_identity(b"\0asm\x01\0\0\0");
        let directory = storage_directory(base, &identity).unwrap();
        let name = directory.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(STORAGE_DIRECTORY_PREFIX));
        assert_eq!(
            &name[STORAGE_DIRECTORY_PREFIX.len()..],
            identity.strip_prefix(permission::ARTIFACT_PREFIX).unwrap()
        );
        assert_ne!(
            storage_directory(
                base,
                &varta_runtime::artifact_identity(b"\0asm\x01\0\0\x01")
            ),
            Some(directory)
        );
        for malformed in ["", "sha256:", "sha256:../../etc", "md5:00", "sha256:ABCDEF"] {
            assert_eq!(storage_directory(base, malformed), None, "{malformed}");
        }
    }

    #[test]
    fn only_kv_patterns_name_storage_roots() {
        let manifest = Manifest::parse(
            br#"{"abiVersion":34,"fsRoots":["config","data","logs"],"capabilities":[
                {"id":"fs.read","patterns":["/config/**"]},
                {"id":"kv.read","patterns":["/data/app.kv","/logs/*.kv"]},
                {"id":"kv.write","patterns":["/data/app.kv"]}]}"#,
        )
        .unwrap();
        assert_eq!(
            kv_roots(&manifest).unwrap().into_iter().collect::<Vec<_>>(),
            ["data", "logs"]
        );
    }
}
