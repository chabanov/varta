//! Private immutable executable snapshot and resource-enforcement probe.
use super::{contract::*, store::random_id};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[cfg(unix)]
const MAX_EXECUTABLE_BYTES: u64 = 1024 * 1024 * 1024;
#[cfg(unix)]
const PROBE_SECONDS: u64 = 5;
pub(crate) struct PinnedWorker {
    root: PathBuf,
    pub path: PathBuf,
    pub identity: String,
}
impl Drop for PinnedWorker {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl PinnedWorker {
    pub fn new(executable: &Path, ceiling: &ToolchainCeiling) -> Result<Self> {
        Self::with_limits(executable, ceiling.address_space_bytes, ceiling.cpu_seconds)
    }
    pub(crate) fn with_limits(
        executable: &Path,
        address_space_bytes: u64,
        cpu_seconds: u64,
    ) -> Result<Self> {
        #[cfg(not(unix))]
        {
            let _ = (executable, address_space_bytes, cpu_seconds);
            Err(ToolchainError::ResourceLimits("startup_probe"))
        }
        #[cfg(unix)]
        {
            use sha2::{Digest, Sha256};
            use std::{
                io::{Read, Write},
                os::unix::fs::{DirBuilderExt, PermissionsExt},
                time::Duration,
            };
            let root = std::env::temp_dir().join(format!(
                "varta-toolchain-{}-{}",
                std::process::id(),
                random_id()?
            ));
            fs::DirBuilder::new().mode(0o700).create(&root)?;
            let mut pinned = Self {
                path: root.join("worker"),
                root,
                identity: String::new(),
            };
            let mut source = fs::File::open(executable)?;
            if !source.metadata()?.is_file() || source.metadata()?.len() > MAX_EXECUTABLE_BYTES {
                return Err(ToolchainError::Invalid("worker executable"));
            }
            let mut target = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pinned.path)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut total = 0u64;
            loop {
                let n = source.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                if total > MAX_EXECUTABLE_BYTES {
                    return Err(ToolchainError::Invalid("worker executable bytes"));
                }
                hash.update(&buffer[..n]);
                target.write_all(&buffer[..n])?;
            }
            target.sync_all()?;
            drop(target);
            fs::set_permissions(&pinned.path, fs::Permissions::from_mode(0o500))?;
            pinned.identity = format!("{:x}", hash.finalize());
            // A fixed trusted probe verifies that the kernel refuses an
            // allocation larger than the configured address-space allowance.
            let mut child = std::process::Command::new(&pinned.path)
                .args([
                    "toolchain-worker",
                    "--address-space-bytes",
                    &address_space_bytes.to_string(),
                    "--cpu-seconds",
                    &cpu_seconds.to_string(),
                    "--probe",
                ])
                .env_clear()
                .env("MallocNanoZone", "0")
                .current_dir(&pinned.root)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            let deadline = std::time::Instant::now() + Duration::from_secs(PROBE_SECONDS);
            loop {
                if let Some(status) = child.try_wait()? {
                    if !status.success() {
                        return Err(ToolchainError::ResourceLimits("startup_probe"));
                    }
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ToolchainError::ResourceLimits("startup_probe"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(pinned)
        }
    }
}
