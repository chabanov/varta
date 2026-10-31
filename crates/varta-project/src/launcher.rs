//! Verify a private snapshot of the native runtime before publishing a package.
use crate::{Artifact, ProjectError, Result};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use varta_common::launcher::{self, LauncherInfo};

/// How often the probe polls the runtime companion for its exit.
const PROBE_POLL_MS: u64 = 10;
/// Attempts to create a fresh private staging directory.
const STAGING_ATTEMPTS: usize = 16;
/// Owner-only permissions of the staging directory and the staged runtime.
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o700;

pub(crate) struct VerifiedLauncher {
    directory: PathBuf,
    executable: PathBuf,
}
impl Drop for VerifiedLauncher {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
impl VerifiedLauncher {
    pub fn prepare(candidate: &Path, artifact: &Artifact) -> Result<Self> {
        let source = File::open(candidate).map_err(|e| ProjectError::Usage(format!("cannot open runtime launcher {}: {e}; build/install varta-launcher or pass --launcher PATH", candidate.display())))?;
        let metadata = source.metadata()?;
        if !metadata.is_file() || metadata.len() > launcher::MAX_LAUNCHER_BYTES {
            return Err(invalid("runtime launcher must be a bounded regular file"));
        }
        let directory = directory()?;
        let executable = directory.join(format!("varta-runtime{}", std::env::consts::EXE_SUFFIX));
        let staged = Self {
            directory,
            executable,
        };
        {
            let mut target = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&staged.executable)?;
            let copied = std::io::copy(
                &mut source.take(launcher::MAX_LAUNCHER_BYTES + 1),
                &mut target,
            )?;
            if copied > launcher::MAX_LAUNCHER_BYTES {
                return Err(invalid("runtime launcher byte limit"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                target.set_permissions(fs::Permissions::from_mode(PRIVATE_MODE))?;
            }
            target.sync_all()?;
        }
        if Artifact::embedded(&staged.executable)?.is_some() {
            return Err(invalid("runtime launcher already contains an application"));
        }
        let info = probe(
            &staged.executable,
            &[OsString::from(launcher::INFO_ARGUMENT)],
        )?;
        let info: LauncherInfo = serde_json::from_slice(&info)
            .map_err(|_| invalid("runtime launcher did not return a valid identity"))?;
        info.validate_current()?;
        let wasm = staged.directory.join("validate.wasm");
        artifact.write_wasm(&wasm)?;
        let result = probe(
            &staged.executable,
            &[
                OsString::from(launcher::VALIDATE_ARGUMENT),
                wasm.into_os_string(),
            ],
        )?;
        let result: serde_json::Value = serde_json::from_slice(&result)
            .map_err(|_| invalid("invalid runtime bytecode-validation response"))?;
        if result != serde_json::json!({"status":"ok"}) {
            return Err(invalid("runtime launcher rejected bytecode"));
        }
        Ok(staged)
    }
    pub fn path(&self) -> &Path {
        &self.executable
    }
}
fn invalid(message: &str) -> ProjectError {
    ProjectError::Artifact(message.into())
}
fn directory() -> Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ProjectError::Artifact(e.to_string()))?
        .as_nanos();
    for _ in 0..STAGING_ATTEMPTS {
        let path = std::env::temp_dir().join(format!(
            "varta-launcher-{}-{now}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(PRIVATE_MODE);
        }
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(invalid(
        "unable to create private launcher staging directory",
    ))
}
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn reader(input: impl Read + Send + 'static) -> Result<mpsc::Receiver<std::io::Result<Vec<u8>>>> {
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("varta-launcher-probe".into())
        .spawn(move || {
            let mut data = Vec::new();
            let result = input
                .take(launcher::MAX_PROBE_BYTES as u64 + 1)
                .read_to_end(&mut data)
                .map(|_| data);
            let _ = send.send(result);
        })?;
    Ok(receive)
}
fn probe(path: &Path, args: &[OsString]) -> Result<Vec<u8>> {
    let mut process = Running(
        Command::new(path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let stdout = reader(
        process
            .0
            .stdout
            .take()
            .ok_or_else(|| invalid("missing runtime stdout"))?,
    )?;
    let stderr = reader(
        process
            .0
            .stderr
            .take()
            .ok_or_else(|| invalid("missing runtime stderr"))?,
    )?;
    let until = Instant::now() + Duration::from_secs(launcher::PROBE_SECONDS);
    let status = loop {
        if let Some(status) = process.0.try_wait()? {
            break status;
        }
        if Instant::now() >= until {
            return Err(invalid("runtime launcher probe timed out"));
        }
        std::thread::sleep(Duration::from_millis(PROBE_POLL_MS));
    };
    let take = |receive: mpsc::Receiver<std::io::Result<Vec<u8>>>| -> Result<Vec<u8>> {
        let bytes = receive
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .map_err(|_| invalid("runtime launcher output timed out"))??;
        if bytes.len() > launcher::MAX_PROBE_BYTES {
            return Err(invalid("runtime launcher output byte limit"));
        }
        Ok(bytes)
    };
    let output = take(stdout)?;
    let errors = take(stderr)?;
    if !status.success() {
        return Err(ProjectError::Artifact(format!(
            "runtime launcher probe failed: {}",
            String::from_utf8_lossy(&errors)
        )));
    }
    Ok(output)
}

/// Resolve only the installed companion, never an ambient executable in cwd/PATH.
pub fn companion(cli: &Path) -> PathBuf {
    cli.with_file_name(OsStr::new(&format!(
        "varta-runtime{}",
        std::env::consts::EXE_SUFFIX
    )))
}
