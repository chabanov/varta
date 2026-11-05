//! Host-owned KV storage: one directory per application identity.
//!
//! A v38 package names its file roots and requests `kv.read`/`kv.write` on
//! virtual paths under them; it never chooses a host directory. The shell
//! maps every root that could hold a database (a package `kv.read` pattern
//! names it and, when a ceiling is given, a ceiling `kv.read` pattern can
//! cover it) and that the ceiling does not map itself to
//! `<base>/sha256-<digest>/<root>`, a directory the shell creates for that
//! artifact hash only. A different artifact (any changed byte of its Wasm)
//! gets a different directory. The mapping grants nothing: every operation
//! still needs the exact grant in the package request and in the ceiling.
//!
//! The directories are created by `Launch::packaged`, after the artifact
//! and the ceiling are validated and before the runtime validates the code
//! and its language profile at start (which compiles the module once, with
//! the roots already pinned). A module the runtime then refuses leaves them
//! behind, empty; no guest code ever ran.
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

/// Characters that make a path pattern component a glob.
const WILDCARDS: [char; 2] = ['*', '?'];

/// The first component (the root) of each `kv.read` pattern of `manifest`.
fn kv_read_roots(manifest: &Manifest) -> Result<Vec<String>, ShellError> {
    let grants = manifest.grants().map_err(RuntimeError::from)?;
    let mut roots = Vec::new();
    for grant in grants {
        if grant.id != kv::READ_CAPABILITY {
            continue;
        }
        if let CapabilityValue::PathGlobs(patterns) = grant.value {
            roots.extend(patterns.iter().filter_map(|pattern| {
                pattern
                    .strip_prefix(files::SEPARATOR)
                    .and_then(|rest| rest.split(files::SEPARATOR).next())
                    .map(str::to_owned)
            }));
        }
    }
    Ok(roots)
}

/// The roots that need KV storage: every root that a package `kv.read`
/// pattern names (package validation requires a literal declared root)
/// and, when a ceiling is given, that a ceiling `kv.read` pattern can cover:
/// its first component is that root, or a glob (conservatively counted for
/// every root). Opening a database needs `kv.read` on its path in both, so
/// no other root could ever hold one; a root reached only by `kv.write`
/// grants is not created.
fn kv_roots(
    package: &Manifest,
    ceiling: Option<&Manifest>,
) -> Result<BTreeSet<String>, ShellError> {
    let mut roots: BTreeSet<String> = kv_read_roots(package)?
        .into_iter()
        .filter(|root| files::valid_root_name(root))
        .collect();
    if let Some(ceiling) = ceiling {
        let covered = kv_read_roots(ceiling)?;
        roots.retain(|root| {
            covered
                .iter()
                .any(|first| first == root || first.contains(WILDCARDS))
        });
    }
    Ok(roots)
}

/// Pin the KV storage roots of `package` ([`kv_roots`]) under `base`,
/// creating `<base>/sha256-<digest>/<root>` for each. A root the ceiling
/// maps keeps that explicit mapping and gets no directory here.
pub(super) fn pin(
    roots: &mut FileRoots,
    package: &Manifest,
    ceiling: Option<&Manifest>,
    base: &Path,
    identity: &str,
) -> Result<(), ShellError> {
    let mut wanted = kv_roots(package, ceiling)?;
    for mapped in roots.names() {
        wanted.remove(mapped);
    }
    if wanted.is_empty() {
        return Ok(());
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
    for root in &wanted {
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

    fn roots(package: &Manifest, ceiling: Option<&str>) -> Vec<String> {
        let ceiling = ceiling.map(|text| Manifest::parse(text.as_bytes()).unwrap());
        kv_roots(package, ceiling.as_ref())
            .unwrap()
            .into_iter()
            .collect()
    }

    #[test]
    fn only_package_kv_read_patterns_name_storage_roots() {
        let package = Manifest::parse(
            br#"{"abiVersion":34,"fsRoots":["config","data","logs","outbox"],"capabilities":[
                {"id":"fs.read","patterns":["/config/**"]},
                {"id":"kv.read","patterns":["/data/app.kv","/logs/*.kv"]},
                {"id":"kv.write","patterns":["/data/app.kv","/outbox/app.kv"]}]}"#,
        )
        .unwrap();
        // `outbox` has only `kv.write`, which never opens a database.
        assert_eq!(roots(&package, None), ["data", "logs"]);
    }

    #[test]
    fn the_ceiling_gates_each_root_by_its_own_kv_read_patterns() {
        let package = Manifest::parse(
            br#"{"abiVersion":34,"fsRoots":["data","logs"],"capabilities":[
                {"id":"kv.read","patterns":["/data/app.kv","/logs/app.kv"]},
                {"id":"kv.write","patterns":["/data/app.kv","/logs/app.kv"]}]}"#,
        )
        .unwrap();
        let ceiling =
            |capabilities: &str| format!(r#"{{"abiVersion":34,"capabilities":{capabilities}}}"#);
        // A `kv.read` for another root creates nothing for `data`.
        assert_eq!(
            roots(
                &package,
                Some(&ceiling(r#"[{"id":"kv.read","patterns":["/logs/**"]}]"#))
            ),
            ["logs"]
        );
        // `kv.write` alone, or any other operation, never opens a database.
        assert!(
            roots(
                &package,
                Some(&ceiling(
                    r#"[{"id":"kv.write","patterns":["/data/**","/logs/**"]},
                        {"id":"fs.read","patterns":["/data/**"]}]"#
                ))
            )
            .is_empty()
        );
        assert!(roots(&package, Some(&ceiling("[]"))).is_empty());
        // A glob in the root component conservatively counts for every root.
        assert_eq!(
            roots(
                &package,
                Some(&ceiling(r#"[{"id":"kv.read","patterns":["/**"]}]"#))
            ),
            ["data", "logs"]
        );
        assert_eq!(
            roots(
                &package,
                Some(&ceiling(r#"[{"id":"kv.read","patterns":["/d*/app.kv"]}]"#))
            ),
            ["data", "logs"]
        );
    }
}
