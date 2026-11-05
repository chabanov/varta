//! The host-owned decision file and audit log of one application identity.
//!
//! Both live in the artifact's storage directory
//! (`<base>/sha256-<digest>/`, [`crate::shell::storage_directory`]), beside
//! its KV roots. Their names contain a `.`, which no file root name may
//! contain, so no root can ever be or reach them: the guest can neither
//! read nor write its decisions or its audit log.
//!
//! Every read-modify-write of the decision file holds an exclusive
//! advisory lock on [`LOCK_FILE`] ([`ConsentStore::locked`]): a revocation
//! from another process and an instance's own new decisions are applied
//! one after the other, never interleaved, so neither overwrites the other.
use crate::shell::{ShellError, storage};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use varta_common::permission::{
    ApplicationDecisions, DecisionFile, DecisionRecord, PermissionError, StoredDecision,
};

/// The decision file: a [`DecisionFile`] holding this identity only.
pub const DECISIONS_FILE: &str = "permissions.json";
/// The audit log: one JSON object per line, appended.
pub const AUDIT_FILE: &str = "permission-audit.jsonl";
/// The previous audit log, kept once the current one reaches
/// [`MAX_AUDIT_LOG_BYTES`].
pub const PREVIOUS_AUDIT_FILE: &str = "permission-audit.jsonl.1";
/// The lock file every writer of the decision file holds while it reads,
/// changes and replaces it. It holds no data.
pub const LOCK_FILE: &str = "permissions.lock";
/// Every file the consent store may create in an artifact's storage
/// directory (besides a short-lived temporary decision file).
pub const CONSENT_FILES: [&str; 4] = [DECISIONS_FILE, AUDIT_FILE, PREVIOUS_AUDIT_FILE, LOCK_FILE];
/// The size at which the audit log is rotated: at most this much (plus one
/// batch of rows) is kept in each of the two files.
pub const MAX_AUDIT_LOG_BYTES: u64 = 1024 * 1024;
/// Only the user may read or write the files.
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;
/// The prefix of a decision file being replaced.
const TEMPORARY_PREFIX: &str = ".permissions-";
const TEMPORARY_SUFFIX: &str = ".tmp";

/// What identifies one version of the decision file on disk, so a reader
/// can skip rereading an unchanged file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    inode: u64,
}

/// The decision file and audit log of one application identity.
#[derive(Debug, Clone)]
pub struct ConsentStore {
    directory: PathBuf,
    identity: String,
}

impl ConsentStore {
    /// The store of `identity` (a `sha256:` application identity) under the
    /// absolute storage `base`. Nothing is created until something is
    /// written.
    pub fn new(base: &Path, identity: &str) -> Result<Self, ShellError> {
        if !base.is_absolute() {
            return Err(ShellError::StorageNotAbsolute(base.to_path_buf()));
        }
        let directory =
            storage::storage_directory(base, identity).ok_or_else(|| ShellError::Decisions {
                path: base.to_path_buf(),
                source: PermissionError::InvalidDecisions(format!(
                    "malformed application identity {identity}"
                )),
            })?;
        Ok(Self {
            directory,
            identity: identity.to_owned(),
        })
    }

    /// The application identity the store belongs to.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The decision file's path.
    #[must_use]
    pub fn decisions_path(&self) -> PathBuf {
        self.directory.join(DECISIONS_FILE)
    }

    /// The audit log's path.
    #[must_use]
    pub fn audit_path(&self) -> PathBuf {
        self.directory.join(AUDIT_FILE)
    }

    fn invalid(&self, source: PermissionError) -> ShellError {
        ShellError::Decisions {
            path: self.decisions_path(),
            source,
        }
    }

    /// The stored decisions; none when the file does not exist. A file
    /// that is malformed, oversized, or names another identity is a typed
    /// error, never an empty set.
    pub fn load(&self) -> Result<BTreeMap<String, StoredDecision>, ShellError> {
        let path = self.decisions_path();
        let bytes = match varta_artifact::read_bounded(&path, varta_common::MAX_MANIFEST_BYTES) {
            Ok(bytes) => bytes,
            Err(varta_common::VartaError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(BTreeMap::new());
            }
            Err(error) => {
                return Err(self.invalid(PermissionError::InvalidDecisions(error.to_string())));
            }
        };
        let file = DecisionFile::parse(&bytes).map_err(|error| self.invalid(error))?;
        if file
            .applications
            .iter()
            .any(|application| application.artifact != self.identity)
        {
            return Err(self.invalid(PermissionError::InvalidDecisions(
                "the file holds decisions of another application".into(),
            )));
        }
        Ok(file.decisions_for(&self.identity))
    }

    /// The decisions in the form the runtime reads them.
    pub(super) fn file(&self, decisions: &BTreeMap<String, StoredDecision>) -> DecisionFile {
        DecisionFile {
            schema_version: varta_common::permission::DECISION_SCHEMA_VERSION,
            applications: vec![ApplicationDecisions {
                artifact: self.identity.clone(),
                decisions: decisions
                    .iter()
                    .map(|(id, decision)| DecisionRecord {
                        id: id.clone(),
                        decision: *decision,
                    })
                    .collect(),
            }],
        }
    }

    /// Replace the stored decisions: validated, written to a private
    /// temporary file in the same directory and renamed over the old one,
    /// so a reader sees either the old or the new file.
    pub fn save(&self, decisions: &BTreeMap<String, StoredDecision>) -> Result<(), ShellError> {
        let file = self.file(decisions);
        file.validate().map_err(|error| self.invalid(error))?;
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|error| self.invalid(PermissionError::InvalidDecisions(error.to_string())))?;
        storage::create_private(&self.directory)?;
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| ShellError::Storage { path, source }
        };
        let temporary = self.directory.join(format!(
            "{TEMPORARY_PREFIX}{}-{}{TEMPORARY_SUFFIX}",
            std::process::id(),
            unique()
        ));
        let result = (|| {
            let mut handle = private_options()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(io(&temporary))?;
            handle.write_all(&bytes).map_err(io(&temporary))?;
            handle.sync_all().map_err(io(&temporary))?;
            std::fs::rename(&temporary, self.decisions_path()).map_err(io(&temporary))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    /// Run `body` (a read, change and [`Self::save`] of the decision file)
    /// while holding the exclusive lock on [`LOCK_FILE`], waiting for any
    /// other holder (another process or instance of this identity). The
    /// lock is released when `body` returns, also on failure, and by the
    /// operating system when a holder exits.
    pub fn locked<T>(&self, body: impl FnOnce() -> Result<T, ShellError>) -> Result<T, ShellError> {
        storage::create_private(&self.directory)?;
        let path = self.directory.join(LOCK_FILE);
        let lock = private_options()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| ShellError::Storage {
                path: path.clone(),
                source,
            })?;
        lock.lock()
            .map_err(|source| ShellError::Storage { path, source })?;
        // Dropping the handle after `body` releases the lock.
        body()
    }

    /// Store a denial of `capability` for this identity. A running shell of
    /// the same artifact revokes it for new operations when it next reads
    /// the file; every later instance starts with it.
    pub fn revoke(&self, capability: &str) -> Result<(), ShellError> {
        if varta_common::capability_kind(capability).is_none() {
            return Err(ShellError::UnknownCapability(capability.to_owned()));
        }
        self.locked(|| {
            let mut decisions = self.load()?;
            decisions.insert(capability.to_owned(), StoredDecision::Deny);
            self.save(&decisions)
        })
    }

    /// The decision file's current stamp, `None` when it does not exist.
    pub(super) fn stamp(&self) -> Option<Stamp> {
        let metadata = std::fs::metadata(self.decisions_path()).ok()?;
        Some(Stamp {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            inode: std::os::unix::fs::MetadataExt::ino(&metadata),
        })
    }

    /// Append `rows` (one JSON object each) to the audit log, rotating it
    /// to [`PREVIOUS_AUDIT_FILE`] first once it holds
    /// [`MAX_AUDIT_LOG_BYTES`].
    pub(super) fn append_audit(&self, rows: &[serde_json::Value]) -> Result<(), ShellError> {
        if rows.is_empty() {
            return Ok(());
        }
        storage::create_private(&self.directory)?;
        let path = self.audit_path();
        let storage_error = |source| ShellError::Storage {
            path: path.clone(),
            source,
        };
        if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= MAX_AUDIT_LOG_BYTES) {
            std::fs::rename(&path, self.directory.join(PREVIOUS_AUDIT_FILE))
                .map_err(storage_error)?;
        }
        let mut text = Vec::new();
        for row in rows {
            serde_json::to_writer(&mut text, row)
                .map_err(|error| storage_error(std::io::Error::other(error)))?;
            text.push(b'\n');
        }
        let mut handle = private_options()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(storage_error)?;
        handle.write_all(&text).map_err(storage_error)
    }
}

/// Options that create files private to the user.
fn private_options() -> std::fs::OpenOptions {
    let mut options = std::fs::OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(FILE_MODE);
    }
    options
}

/// A per-process counter for temporary names.
fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::files;

    #[test]
    fn host_files_can_never_be_a_guest_file_root() {
        for name in CONSENT_FILES {
            assert!(!files::valid_root_name(name), "{name}");
        }
        assert!(!files::valid_root_name(TEMPORARY_PREFIX));
    }

    #[test]
    fn the_lock_serializes_writers_of_the_decision_file() {
        let base = std::env::temp_dir().join(format!(
            "varta-consent-lock-{}-{}",
            std::process::id(),
            unique()
        ));
        let identity = format!("sha256:{}", "ab".repeat(32));
        let store = ConsentStore::new(&base, &identity).unwrap();
        let held = std::sync::Arc::new(std::sync::Barrier::new(2));
        let (entered, order) = std::sync::mpsc::channel();
        // A writer that holds the lock across a slow read-change-write.
        let writer = {
            let store = store.clone();
            let held = held.clone();
            let entered = entered.clone();
            std::thread::spawn(move || {
                store
                    .locked(|| {
                        held.wait();
                        let mut decisions = store.load()?;
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        decisions.insert("kv.read".into(), StoredDecision::Allow);
                        store.save(&decisions)?;
                        entered.send("instance").unwrap();
                        Ok(())
                    })
                    .unwrap();
            })
        };
        held.wait();
        // A revocation started meanwhile waits for the lock, then sees the
        // other writer's decision and keeps it.
        store.revoke("kv.write").unwrap();
        entered.send("revoke").unwrap();
        writer.join().unwrap();
        assert_eq!(order.try_iter().collect::<Vec<_>>(), ["instance", "revoke"]);
        let decisions = store.load().unwrap();
        assert_eq!(decisions.get("kv.read"), Some(&StoredDecision::Allow));
        assert_eq!(decisions.get("kv.write"), Some(&StoredDecision::Deny));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
