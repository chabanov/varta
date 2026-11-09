//! Only the trusted CLI worker applies process limits and invokes compilation.
use super::{compile, contract::*};
use std::io::{Read, Write};

/// Hidden CLI entry point. Never called in the service's process.
pub fn run_worker(address_space_bytes: u64, cpu_seconds: u64, probe: bool) -> Result<()> {
    if !(MIN_ADDRESS_SPACE_BYTES..=MAX_ADDRESS_SPACE_BYTES).contains(&address_space_bytes)
        || !(1..=MAX_CPU_SECONDS).contains(&cpu_seconds)
    {
        return Err(ToolchainError::Invalid("worker limits"));
    }
    limits(address_space_bytes, cpu_seconds)?;
    if probe {
        let mut bytes = Vec::<u8>::new();
        if bytes
            .try_reserve_exact(address_space_bytes as usize + 2 * 1024 * 1024)
            .is_ok()
        {
            return Err(ToolchainError::ResourceLimits("reservation_probe"));
        }
        return Ok(());
    }
    let input = read_input(MAX_REQUEST_BYTES)?;
    let request: CompileRequest = serde_json::from_slice(&input)?;
    let output = serde_json::to_vec(&compile::compile(&request)?)?;
    if output.len() > MAX_WORKER_REPLY {
        return Err(ToolchainError::Invalid("worker reply limit"));
    }
    std::io::stdout().write_all(&output)?;
    Ok(())
}

pub(crate) fn read_input(max_bytes: usize) -> Result<Vec<u8>> {
    // Keep stdin as the parent's lifetime channel after the framed request.
    // EOF also stops a compiler whose controller died without running Drop.
    let mut stdin = std::io::stdin();
    let mut length = [0u8; 8];
    stdin.read_exact(&mut length)?;
    let length = u64::from_le_bytes(length);
    if length > max_bytes as u64 {
        return Err(ToolchainError::Invalid("request byte limit"));
    }
    let mut input = vec![0u8; length as usize];
    stdin.read_exact(&mut input)?;
    const PARENT_WATCH_STACK_BYTES: usize = 128 * 1024;
    std::thread::Builder::new()
        .name("toolchain-parent".into())
        .stack_size(PARENT_WATCH_STACK_BYTES)
        .spawn(move || {
            let mut byte = [0u8; 1];
            let _ = stdin.read_exact(&mut byte);
            std::process::exit(2);
        })?;
    Ok(input)
}

#[cfg(unix)]
pub(crate) fn limits(address_space_bytes: u64, cpu_seconds: u64) -> Result<()> {
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    let baseline = address_space_baseline()?;
    let address_space_bytes = baseline
        .checked_add(address_space_bytes)
        .ok_or(ToolchainError::ResourceLimits("address_space_overflow"))?;
    for (resource, value) in [
        (Resource::Core, 0),
        (Resource::As, address_space_bytes),
        (Resource::Cpu, cpu_seconds),
    ] {
        let value = getrlimit(resource)
            .maximum
            .map_or(value, |max| max.min(value));
        setrlimit(
            resource,
            Rlimit {
                current: Some(value),
                maximum: Some(value),
            },
        )
        .map_err(|_| {
            ToolchainError::ResourceLimits(match resource {
                Resource::Core => "core_limit",
                Resource::As => "address_space_limit",
                _ => "cpu_limit",
            })
        })?;
    }
    Ok(())
}
// Modern XNU refuses an AS ceiling below existing mappings. Find the existing
// footprint without unsafe task APIs, changing only the worker's soft ceiling.
// Older advisory-only kernels fail the subsequent allocation probe.
#[cfg(target_os = "macos")]
fn address_space_baseline() -> Result<u64> {
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    let maximum = getrlimit(Resource::As).maximum;
    let mut high = maximum.unwrap_or(1u64 << 48);
    let mut low = 0;
    setrlimit(
        Resource::As,
        Rlimit {
            current: Some(high),
            maximum,
        },
    )
    .map_err(|_| ToolchainError::ResourceLimits("address_space_baseline"))?;
    while high - low > 64 * 1024 {
        let middle = low + (high - low) / 2;
        match setrlimit(
            Resource::As,
            Rlimit {
                current: Some(middle),
                maximum,
            },
        ) {
            Ok(()) => high = middle,
            Err(rustix::io::Errno::INVAL) => low = middle,
            Err(_) => return Err(ToolchainError::ResourceLimits("address_space_baseline")),
        }
    }
    Ok(high)
}
#[cfg(target_os = "linux")]
fn address_space_baseline() -> Result<u64> {
    let mut text = String::new();
    std::fs::File::open("/proc/self/statm")?
        .take(1024)
        .read_to_string(&mut text)?;
    let pages = text
        .split_whitespace()
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .ok_or(ToolchainError::ResourceLimits("address_space_baseline"))?;
    pages
        .checked_mul(rustix::param::page_size() as u64)
        .ok_or(ToolchainError::ResourceLimits("address_space_baseline"))
}
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn address_space_baseline() -> Result<u64> {
    Err(ToolchainError::ResourceLimits("unsupported_platform"))
}
#[cfg(not(unix))]
pub(crate) fn limits(_: u64, _: u64) -> Result<()> {
    Err(ToolchainError::ResourceLimits("startup_probe"))
}
