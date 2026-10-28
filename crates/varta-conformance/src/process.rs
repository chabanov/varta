//! Deadline and bounded, concurrently drained protocol pipes around each worker.
use crate::{model::*, worker::detail};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
const MAX_STDOUT: usize = 64 * 1024;
const MAX_STDERR: usize = 8 * 1024;
fn drain(
    mut input: impl Read,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut saved = Vec::new();
    let mut buf = [0; 8192];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            return Ok(saved);
        }
        let take = n.min(limit.saturating_sub(saved.len()));
        saved.extend_from_slice(&buf[..take]);
        if take < n {
            overflow.store(true, Ordering::Relaxed);
        }
    }
}
fn failed(category: InfrastructureFailure, text: impl ToString) -> Observation {
    Observation::Infrastructure {
        category,
        detail: detail(text),
    }
}
pub(crate) fn invoke(executable: &Path, request: Request, timeout: Duration) -> Observation {
    let started = Instant::now();
    let mut child = match Command::new(executable)
        .arg("--worker")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return failed(InfrastructureFailure::Launch, error),
    };
    // The writer cannot block the deadline loop if a worker stops reading input.
    let Some(mut input) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return failed(InfrastructureFailure::Protocol, "missing worker stdin");
    };
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return failed(InfrastructureFailure::Protocol, "missing worker stdout");
    };
    let Some(errors) = child.stderr.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return failed(InfrastructureFailure::Protocol, "missing worker stderr");
    };
    let overflow = Arc::new(AtomicBool::new(false));
    let out_flag = overflow.clone();
    let err_flag = overflow.clone();
    let writer = thread::spawn(move || -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&request)?;
        input.write_all(&bytes)
    });
    let reader = thread::spawn(move || drain(output, MAX_STDOUT, out_flag));
    let error_reader = thread::spawn(move || drain(errors, MAX_STDERR, err_flag));
    let mut failure = None;
    let status = loop {
        if overflow.load(Ordering::Relaxed) {
            failure = Some(InfrastructureFailure::OutputLimit);
            break None;
        }
        if started.elapsed() >= timeout {
            failure = Some(InfrastructureFailure::Timeout);
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(_) => {
                failure = Some(InfrastructureFailure::Process);
                break None;
            }
        }
    };
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let written = writer.join();
    let stdout = reader.join();
    let stderr = error_reader.join();
    if overflow.load(Ordering::Relaxed) {
        return failed(
            InfrastructureFailure::OutputLimit,
            "worker protocol output limit exceeded",
        );
    }
    if let Some(kind) = failure {
        return failed(
            kind,
            "worker exceeded deadline or process monitoring failed",
        );
    }
    let (Ok(Ok(stdout)), Ok(Ok(stderr))) = (stdout, stderr) else {
        return failed(InfrastructureFailure::Protocol, "worker pipe failed");
    };
    if !status.is_some_and(|s| s.success()) {
        return failed(
            InfrastructureFailure::Process,
            String::from_utf8_lossy(&stderr),
        );
    }
    if !matches!(written, Ok(Ok(()))) {
        return failed(InfrastructureFailure::Protocol, "worker input failed");
    }
    match serde_json::from_slice(&stdout) {
        Ok(result) => result,
        Err(error) => failed(InfrastructureFailure::Protocol, error),
    }
}
