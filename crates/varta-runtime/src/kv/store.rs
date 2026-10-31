//! Opening Deno KV database files (v38 `kv`) through pinned descriptors and
//! running reads and commits on `denokv_sqlite`.
//!
//! SQLite opens files by path and cannot take a descriptor, so a database
//! is pinned and verified in this order, before SQLite writes anything:
//!
//! 1. The broker granted the virtual path (`kv.read`, and `kv.write` for a
//!    writable or missing file) against the request and the host ceiling.
//! 2. Every directory below the pinned root is opened no-follow from the
//!    root's descriptor, then the file itself no-follow (created exclusively
//!    when it is missing and writable). Its (device, inode) identity is the
//!    database's identity; the descriptor stays open while the database is.
//! 3. The host path is the root's host directory, canonicalized and checked
//!    to be the same directory as the pinned root descriptor, joined with
//!    the components; its no-follow metadata must name the pinned file.
//! 4. SQLite opens that path with `SQLITE_OPEN_NOFOLLOW` (which fails on a
//!    symbolic link in any component) and without `SQLITE_OPEN_CREATE`.
//! 5. A read transaction checks the schema; then the process's own open
//!    descriptors (`/dev/fd`, `/proc/self/fd` on Linux) are compared with
//!    the ones before step 4: each SQLite connection (the worker's, and the
//!    lock probe of a writable database) must have opened the pinned file. A swap of the path between steps 3 and 4 makes SQLite
//!    open another file, which this check rejects (KV opens are serialized
//!    in the process, so no other open of the same file can interleave).
//! 6. A writable database probes the write lock (`BEGIN IMMEDIATE` with no
//!    busy wait): a file another writer holds fails clearly.
//!
//! Commits fail fast and never outlive their turn (see [`Fence`]): each one
//! first probes the write lock without waiting on a second connection, and
//! the worker's connection has a commit hook that lets a guest commit reach
//! the file only before the commit cutoff, `kv::COMMIT_RESERVE_MS` before
//! the turn deadline. A commit not decided by the cutoff is refused at its
//! commit point whenever the worker gets there, so it is never applied, and
//! the database accepts no commit after that.
//!
//! SQLite opens the `-wal` and `-shm` files next to the verified file by
//! name, with `O_NOFOLLOW`; a process that can rename directories inside
//! the host root already controls the database and is outside this check.
use crate::policy::DatabaseTarget;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use denokv_proto::{
    AtomicWrite, AtomicWriteOutcome, Check, Mutation, ReadRange, ReadRangeOutput,
    SnapshotReadOptions,
};
use rand::SeedableRng;
use rusqlite::{OpenFlags, OptionalExtension};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use varta_common::kv;

/// One failed KV operation: the name of the error the guest throws and its
/// message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KvError {
    pub name: &'static str,
    pub message: String,
}

impl KvError {
    pub(crate) fn type_error(message: impl Into<String>) -> Self {
        Self {
            name: kv::error::TYPE_ERROR,
            message: message.into(),
        }
    }
    pub(crate) fn range_error(message: impl Into<String>) -> Self {
        Self {
            name: kv::error::RANGE_ERROR,
            message: message.into(),
        }
    }
    pub(crate) fn denied(id: &str, path: &str) -> Self {
        Self {
            name: kv::error::NOT_ALLOWED,
            message: format!("<capability denied: {id}> ({path})"),
        }
    }
    fn not_found(path: &str) -> Self {
        Self {
            name: kv::error::NOT_FOUND,
            message: format!("no KV database at {path}"),
        }
    }
    fn io(path: &str, error: impl std::fmt::Display) -> Self {
        Self::type_error(format!("cannot open the KV database {path}: {error}"))
    }
}

/// The busy wait of an open database (rusqlite's default, which Deno keeps);
/// the open-time checks wait for no lock at all.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// The sidecar files SQLite keeps next to a database.
const SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

/// KV opens, one at a time in the process (step 5 above).
static OPENING: Mutex<()> = Mutex::new(());

/// The (device, inode) identity of a file.
type Identity = (u64, u64);

/// Where a guest commit stands, decided under one lock by the host and by the
/// worker connection's commit hook (which runs at SQLite's commit point,
/// before anything of the transaction is in the file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Commit {
    /// No guest commit in flight: the worker's own maintenance (Deno KV's
    /// expiry collection and its schema creation) may commit.
    Idle,
    /// A guest commit may commit until the cutoff.
    Open(Instant),
    /// The hook let the guest commit through before the cutoff; SQLite is
    /// making it durable.
    Committing,
    /// The cutoff passed first: every later commit is refused.
    Closed,
}

/// The commit fence of a writable database.
#[derive(Debug)]
struct Fence(Mutex<Commit>);

impl Fence {
    fn state(&self) -> std::sync::MutexGuard<'_, Commit> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn set(&self, state: Commit) {
        *self.state() = state;
    }
    /// The commit hook: whether SQLite must turn this commit into a rollback.
    fn refuses(&self) -> bool {
        let mut state = self.state();
        match *state {
            Commit::Idle | Commit::Committing => false,
            Commit::Open(cutoff) if Instant::now() < cutoff => {
                *state = Commit::Committing;
                false
            }
            Commit::Open(_) | Commit::Closed => {
                *state = Commit::Closed;
                true
            }
        }
    }
    /// At the cutoff: close an undecided commit; whether the hook had let
    /// it through already.
    fn close(&self) -> bool {
        let mut state = self.state();
        if *state == Commit::Committing {
            return true;
        }
        *state = Commit::Closed;
        false
    }
}

/// An open database.
pub(crate) struct Database {
    sqlite: denokv_sqlite::Sqlite,
    pub writable: bool,
    /// The virtual path, checked again for every operation.
    pub path: String,
    /// The pinned file, held while the database is open.
    pinned: Option<std::fs::File>,
    /// A second connection to the verified file that probes the write lock
    /// before each commit (writable databases).
    probe: Option<rusqlite::Connection>,
    fence: std::sync::Arc<Fence>,
    /// A read or commit did not finish in its turn: the database refuses
    /// every operation, and it is not joined on close, since its worker may
    /// still be waiting for a lock.
    abandoned: bool,
}

impl Drop for Database {
    fn drop(&mut self) {
        if self.abandoned {
            // Closing the pinned descriptor would release the POSIX locks
            // the still running worker holds on the file: keep it open.
            std::mem::forget(self.pinned.take());
        } else {
            self.sqlite.close();
        }
    }
}

/// Open the granted database file `target` (the virtual `path`).
pub(crate) fn open(
    target: DatabaseTarget,
    path: &str,
    writable: bool,
) -> Result<Database, KvError> {
    open_with(target, path, writable, &mut |_| {})
}

/// [`open`] with `before_sqlite` called with the verified host path right
/// before SQLite opens it: the window a concurrent swap would use (tests).
pub(crate) fn open_with(
    target: DatabaseTarget,
    path: &str,
    writable: bool,
    before_sqlite: &mut dyn FnMut(&Path),
) -> Result<Database, KvError> {
    let _serial = OPENING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((name, parents)) = target.components.split_last() else {
        return Err(KvError::type_error(format!(
            "{path} names no database file"
        )));
    };
    if name.starts_with(':') || SIDECAR_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
        return Err(KvError::type_error(format!(
            "{path} is not a valid database file name"
        )));
    }
    let Some(directory) = &target.directory else {
        return Err(KvError::type_error(format!(
            "the file root of {path} has no host directory"
        )));
    };
    let mut parent = target.root.try_clone().map_err(|e| KvError::io(path, e))?;
    for part in parents {
        parent = parent
            .open_dir_nofollow(part)
            .map_err(|e| KvError::io(path, e))?;
    }
    let pinned = pin_file(&parent, name, path, writable)?;
    let identity = file_identity(&pinned).map_err(|e| KvError::io(path, e))?;
    let host =
        host_path(&target.root, directory, &target.components).map_err(|e| KvError::io(path, e))?;
    if path_identity(&host).ok() != Some(identity) {
        return Err(KvError::type_error(format!(
            "the KV database {path} changed while it was opened"
        )));
    }
    let before = open_descriptors().map_err(|e| KvError::io(path, e))?;
    before_sqlite(&host);
    let mut flags = OpenFlags::SQLITE_OPEN_NOFOLLOW | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    flags |= if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection =
        rusqlite::Connection::open_with_flags(&host, flags).map_err(|e| KvError::io(path, e))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|e| KvError::io(path, e))?;
    check_schema(&connection, path, writable)?;
    let probe = if writable {
        let probe = rusqlite::Connection::open_with_flags(&host, flags)
            .map_err(|e| KvError::io(path, e))?;
        probe
            .busy_timeout(Duration::ZERO)
            .map_err(|e| KvError::io(path, e))?;
        Some(probe)
    } else {
        None
    };
    let after = open_descriptors().map_err(|e| KvError::io(path, e))?;
    // The pinned descriptor itself is no open by SQLite, whatever a rename
    // of its file did to the change time it shows.
    let pinned_fd = descriptor(&pinned);
    let reference = pinned_fd.and_then(shown);
    // Each SQLite connection opens the file once: the worker's, and the
    // probe of a writable database.
    let opened = after
        .difference(&before)
        .filter(|(fd, opened)| Some(*fd) != pinned_fd && Some(*opened) == reference)
        .count();
    if reference.is_none() || opened < 1 + usize::from(writable) {
        return Err(KvError::type_error(format!(
            "the KV database {path} changed while it was opened"
        )));
    }
    if writable {
        probe_write_lock(&connection, path)?;
    }
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|e| KvError::io(path, e))?;
    let fence = std::sync::Arc::new(Fence(Mutex::new(Commit::Idle)));
    let hook = std::sync::Arc::clone(&fence);
    connection
        .commit_hook(Some(move || hook.refuses()))
        .map_err(|e| KvError::io(path, e))?;
    if writable {
        // A commit is reported only once it is durable (WAL, synchronous FULL).
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|e| KvError::io(path, e))?;
    }
    let mut connection = Some(connection);
    let sqlite = denokv_sqlite::Sqlite::new(
        move || {
            let connection = connection.take().ok_or_else(|| {
                deno_error::JsErrorBox::generic("the KV connection is opened once")
            })?;
            if writable {
                // Deno's connection factory sets WAL mode on every open.
                connection
                    .pragma_update(None, "journal_mode", "wal")
                    .map_err(|e| deno_error::JsErrorBox::generic(e.to_string()))?;
            }
            Ok((
                connection,
                Box::new(rand::rngs::StdRng::from_entropy()) as Box<dyn rand::RngCore + Send>,
            ))
        },
        denokv_sqlite::SqliteNotifier::default(),
        denokv_sqlite::SqliteConfig {
            num_workers: 1,
            batch_timeout: None,
        },
    )
    .map_err(|e| KvError::io(path, e))?;
    Ok(Database {
        sqlite,
        writable,
        path: path.to_owned(),
        pinned: Some(pinned),
        probe,
        fence,
        abandoned: false,
    })
}

/// Open (or, writable, create) the database file `name` of `parent`
/// no-follow and check that it is a regular file.
fn pin_file(
    parent: &Dir,
    name: &str,
    path: &str,
    writable: bool,
) -> Result<std::fs::File, KvError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(writable)
        .follow(FollowSymlinks::No);
    let file = match parent.open_with(name, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !writable {
                return Err(KvError::not_found(path));
            }
            let mut create = OpenOptions::new();
            create
                .read(true)
                .write(true)
                .create_new(true)
                .follow(FollowSymlinks::No);
            parent
                .open_with(name, &create)
                .map_err(|e| KvError::io(path, e))?
        }
        Err(error) => return Err(KvError::io(path, error)),
    };
    let file = file.into_std();
    if !file.metadata().map_err(|e| KvError::io(path, e))?.is_file() {
        return Err(KvError::type_error(format!("{path} is not a regular file")));
    }
    Ok(file)
}

/// The host path of the database: the root's host directory, canonicalized
/// and verified to be the pinned root, joined with the components.
fn host_path(root: &Dir, directory: &Path, components: &[String]) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(directory)?;
    if path_identity(&canonical)? != directory_identity(root)? {
        return Err(std::io::Error::other(
            "the host directory of the file root is no longer the pinned directory",
        ));
    }
    Ok(components
        .iter()
        .fold(canonical, |path, component| path.join(component)))
}

#[cfg(unix)]
fn file_identity(file: &std::fs::File) -> std::io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
fn directory_identity(directory: &Dir) -> std::io::Result<Identity> {
    use cap_std::fs::MetadataExt;
    let metadata = directory.dir_metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
fn path_identity(path: &Path) -> std::io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

/// Where the process lists its open descriptors.
#[cfg(target_os = "linux")]
const DESCRIPTORS: &str = "/proc/self/fd";
#[cfg(all(unix, not(target_os = "linux")))]
const DESCRIPTORS: &str = "/dev/fd";

/// What a descriptor shows of its open file through the descriptor
/// directory: device and inode (macOS reports its devfs device there, so the
/// size and change time are compared too). Read with `stat`, never by
/// opening the descriptor: closing any descriptor of a file releases every
/// POSIX lock the process holds on it, SQLite's included.
type Shown = (u64, u64, u64, i64, i64);

/// The regular files the process has open, as (descriptor, shown file)
/// pairs: a descriptor number the listing itself used before is reused by a
/// later open, so a new open is a new pair, not a new number.
#[cfg(unix)]
fn open_descriptors() -> std::io::Result<BTreeSet<(u32, Shown)>> {
    let mut numbers = Vec::new();
    for entry in std::fs::read_dir(DESCRIPTORS)? {
        if let Some(fd) = entry?.file_name().to_str().and_then(|n| n.parse().ok()) {
            numbers.push(fd);
        }
    }
    Ok(numbers
        .into_iter()
        .filter_map(|fd| shown(fd).map(|file| (fd, file)))
        .collect())
}

/// The regular file open as descriptor `fd`, as the descriptor directory
/// shows it.
#[cfg(unix)]
fn shown(fd: u32) -> Option<Shown> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(Path::new(DESCRIPTORS).join(fd.to_string())).ok()?;
    metadata.is_file().then(|| {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.size(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    })
}

#[cfg(unix)]
fn descriptor(file: &std::fs::File) -> Option<u32> {
    use std::os::fd::AsRawFd;
    u32::try_from(file.as_raw_fd()).ok()
}

#[cfg(not(unix))]
fn unsupported() -> std::io::Error {
    std::io::Error::other("KV databases need a Unix host")
}
#[cfg(not(unix))]
fn file_identity(_: &std::fs::File) -> std::io::Result<Identity> {
    Err(unsupported())
}
#[cfg(not(unix))]
fn directory_identity(_: &Dir) -> std::io::Result<Identity> {
    Err(unsupported())
}
#[cfg(not(unix))]
fn path_identity(_: &Path) -> std::io::Result<Identity> {
    Err(unsupported())
}
#[cfg(not(unix))]
fn open_descriptors() -> std::io::Result<BTreeSet<(u32, Shown)>> {
    Err(unsupported())
}
#[cfg(not(unix))]
fn shown(_: u32) -> Option<Shown> {
    None
}
#[cfg(not(unix))]
fn descriptor(_: &std::fs::File) -> Option<u32> {
    None
}

/// The schema check, in one read transaction: an empty file is a new
/// database (writable only); any other file must have exactly the tables,
/// `kv` columns and migration version of `denokv_sqlite` 0.14.0.
fn check_schema(
    connection: &rusqlite::Connection,
    path: &str,
    writable: bool,
) -> Result<(), KvError> {
    let failure = |error: rusqlite::Error| {
        if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy) {
            locked(path)
        } else {
            KvError::io(path, error)
        }
    };
    let unsupported = |detail: String| {
        KvError::type_error(format!(
            "{path} is not a Deno KV database of schema version {}: {detail}",
            kv::SCHEMA_VERSION
        ))
    };
    let mut statement = connection
        .prepare(
            "select name from sqlite_master where type = 'table' \
             and name not like 'sqlite\\_%' escape '\\' order by name",
        )
        .map_err(failure)?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(failure)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failure)?;
    if tables.is_empty() {
        let objects: i64 = connection
            .query_row("select count(*) from sqlite_master", [], |row| row.get(0))
            .map_err(failure)?;
        if objects == 0 && writable {
            return Ok(());
        }
        return Err(unsupported("it has no KV tables".into()));
    }
    if tables != kv::SCHEMA_TABLES {
        return Err(unsupported(format!("tables {}", tables.join(", "))));
    }
    let version: Option<i64> = connection
        .query_row(
            "select version from migration_state where k = 0",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(failure)?;
    if version != Some(kv::SCHEMA_VERSION) {
        return Err(unsupported(format!("migration version {version:?}")));
    }
    let mut statement = connection
        .prepare("select name from pragma_table_info('kv') order by cid")
        .map_err(failure)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(failure)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failure)?;
    if columns != kv::KV_COLUMNS {
        return Err(unsupported(format!("kv columns {}", columns.join(", "))));
    }
    Ok(())
}

fn locked(path: &str) -> KvError {
    KvError::type_error(format!(
        "the KV database {path} is locked by another writer"
    ))
}

/// Take and release the write lock without waiting for it.
fn probe_write_lock(connection: &rusqlite::Connection, path: &str) -> Result<(), KvError> {
    match connection.execute_batch("begin immediate; rollback;") {
        Ok(()) => Ok(()),
        Err(error) if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy) => {
            Err(locked(path))
        }
        Err(error) => Err(KvError::io(path, error)),
    }
}

/// Why a read or commit returned without a result.
#[derive(Debug)]
pub(crate) enum Wait {
    Failed(KvError),
    /// The deadline passed while the database worker was still running.
    Deadline,
}

impl Database {
    /// Run one future of the database worker on `runtime` until `deadline`.
    fn wait<T, E: std::fmt::Display>(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        deadline: Instant,
        future: impl std::future::Future<Output = Result<T, E>>,
    ) -> Result<T, Wait> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match runtime.block_on(async { tokio::time::timeout(remaining, future).await }) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(Wait::Failed(KvError::type_error(error.to_string()))),
            Err(_) => {
                self.abandoned = true;
                Err(Wait::Deadline)
            }
        }
    }

    pub(crate) fn read(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        deadline: Instant,
        ranges: Vec<ReadRange>,
    ) -> Result<Vec<ReadRangeOutput>, Wait> {
        let options = SnapshotReadOptions {
            consistency: denokv_proto::Consistency::Strong,
        };
        self.available()?;
        let sqlite = self.sqlite.clone();
        self.wait(runtime, deadline, sqlite.snapshot_read(ranges, options))
    }

    /// Commit, or `None` when a check failed.
    pub(crate) fn commit(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        deadline: Instant,
        checks: Vec<Check>,
        mutations: Vec<Mutation>,
    ) -> Result<Option<denokv_proto::Versionstamp>, Wait> {
        self.commit_with(runtime, deadline, (checks, mutations), &mut || {})
    }

    /// [`Self::commit`] with `before_submit` called after the lock probe,
    /// right before the write goes to the worker (tests take the lock there).
    ///
    /// The commit fails fast: a busy write lock, or too little time left in
    /// the turn, fails before anything is submitted. Otherwise the commit
    /// may reach the file only until the cutoff; one the commit hook let
    /// through is waited for until it is durable and reported, one still
    /// undecided at the cutoff is refused at its commit point (never
    /// applied) and fails with a `TypeError`, and the database then accepts
    /// nothing more.
    pub(crate) fn commit_with(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        deadline: Instant,
        (checks, mutations): (Vec<Check>, Vec<Mutation>),
        before_submit: &mut dyn FnMut(),
    ) -> Result<Option<denokv_proto::Versionstamp>, Wait> {
        if !self.writable {
            return Err(Wait::Failed(KvError::type_error(format!(
                "the KV database {} is open read-only",
                self.path
            ))));
        }
        self.available()?;
        let cutoff = deadline
            .checked_sub(Duration::from_millis(kv::COMMIT_RESERVE_MS))
            .filter(|cutoff| Instant::now() < *cutoff)
            .ok_or_else(|| {
                Wait::Failed(KvError::type_error(format!(
                    "too little time is left in this turn to commit to the KV database {}; nothing was applied",
                    self.path
                )))
            })?;
        if let Some(probe) = &self.probe {
            // Wait briefly, never past the cutoff: the worker's own expiry
            // collection holds the lock for a moment.
            let wait = Duration::from_millis(kv::LOCK_WAIT_MS)
                .min(cutoff.saturating_duration_since(Instant::now()));
            probe
                .busy_timeout(wait)
                .map_err(|e| Wait::Failed(KvError::io(&self.path, e)))?;
            probe_write_lock(probe, &self.path).map_err(|error| {
                Wait::Failed(KvError {
                    message: format!("{}; nothing was applied", error.message),
                    ..error
                })
            })?;
        }
        before_submit();
        let write = AtomicWrite {
            checks,
            mutations,
            enqueues: Vec::new(),
        };
        let sqlite = self.sqlite.clone();
        let fence = std::sync::Arc::clone(&self.fence);
        fence.set(Commit::Open(cutoff));
        let outcome = runtime.block_on(async {
            let mut write = std::pin::pin!(sqlite.atomic_write(write));
            let cutoff = tokio::time::Instant::from_std(cutoff);
            match tokio::time::timeout_at(cutoff, write.as_mut()).await {
                Ok(outcome) => Some(outcome),
                // Let through before the cutoff: SQLite holds the write lock
                // and is making the commit durable; wait for it.
                Err(_) if fence.close() => Some(write.await),
                Err(_) => None,
            }
        });
        let Some(outcome) = outcome else {
            self.abandoned = true;
            return Err(Wait::Failed(KvError::type_error(format!(
                "the commit to the KV database {} did not finish in this turn (another writer may hold its lock); nothing was applied",
                self.path
            ))));
        };
        fence.set(Commit::Idle);
        match outcome {
            Ok(AtomicWriteOutcome::Committed(result)) => Ok(Some(result.versionstamp)),
            Ok(AtomicWriteOutcome::CheckFailed { .. }) => Ok(None),
            Err(error) => Err(Wait::Failed(KvError::type_error(error.to_string()))),
        }
    }

    /// Fail once a read or commit was abandoned.
    fn available(&self) -> Result<(), Wait> {
        if self.abandoned {
            return Err(Wait::Failed(KvError::type_error(format!(
                "the KV database {} is unavailable after an unfinished operation; close and open it again",
                self.path
            ))));
        }
        Ok(())
    }
}

/// The versionstamp text Deno returns: 20 lowercase hex digits.
pub(crate) fn versionstamp_text(versionstamp: &denokv_proto::Versionstamp) -> String {
    versionstamp
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Hexadecimal digits of one versionstamp byte, and their radix.
const HEX_DIGITS_PER_BYTE: usize = 2;
const HEX_RADIX: u32 = 16;
/// The digits of a versionstamp text: 20 for Deno's ten-byte versionstamp.
const VERSIONSTAMP_DIGITS: usize = HEX_DIGITS_PER_BYTE * size_of::<denokv_proto::Versionstamp>();

/// A versionstamp text Deno accepts in a check: exactly 20 hex digits.
pub(crate) fn parse_versionstamp(text: &str) -> Option<denokv_proto::Versionstamp> {
    if text.len() != VERSIONSTAMP_DIGITS || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut versionstamp = denokv_proto::Versionstamp::default();
    for (index, byte) in versionstamp.iter_mut().enumerate() {
        let digits = HEX_DIGITS_PER_BYTE * index..HEX_DIGITS_PER_BYTE * (index + 1);
        *byte = u8::from_str_radix(&text[digits], HEX_RADIX).ok()?;
    }
    Some(versionstamp)
}
