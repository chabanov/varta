use super::contract::*;
use base64::Engine;
use std::{collections::BTreeMap, path::Path};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Child,
};
type Api<T> = std::result::Result<T, &'static str>;
pub(super) fn input(
    c: &ExecutionCeiling,
    request: ImportRequest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Api<Vec<u8>> {
    let input = WorkerInput {
        request,
        files: files
            .iter()
            .map(|(k, b)| {
                (
                    k.clone(),
                    base64::engine::general_purpose::STANDARD.encode(b),
                )
            })
            .collect(),
        budgets: c.budgets,
    };
    let b = serde_json::to_vec(&input).map_err(|_| "invalid_artifact")?;
    if b.len() > MAX_IPC {
        return Err("invalid_artifact");
    }
    Ok(b)
}
pub(super) fn spawn(executable: &Path, c: &ExecutionCeiling, validate: bool) -> Api<Child> {
    let mut command = tokio::process::Command::new(executable);
    command.args([
        "execution-worker",
        "--address-space-bytes",
        &c.address_space_bytes.to_string(),
        "--cpu-seconds",
        &c.cpu_seconds.to_string(),
    ]);
    if validate {
        command.arg("--validate-only");
    }
    command
        .env_clear()
        .env("MallocNanoZone", "0")
        .current_dir(executable.parent().ok_or("unavailable")?)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "worker_failed")
}
pub(super) async fn send(
    child: &mut Child,
    b: &[u8],
) -> Api<(tokio::process::ChildStdin, tokio::process::ChildStdout)> {
    let mut stdin = child.stdin.take().ok_or("worker_failed")?;
    let stdout = child.stdout.take().ok_or("worker_failed")?;
    stdin
        .write_all(&(b.len() as u64).to_le_bytes())
        .await
        .map_err(|_| "worker_failed")?;
    stdin.write_all(b).await.map_err(|_| "worker_failed")?;
    Ok((stdin, stdout))
}
pub(super) async fn event(out: &mut tokio::process::ChildStdout) -> Api<WorkerEvent> {
    let mut b = Vec::new();
    loop {
        let byte = out.read_u8().await.map_err(|_| "worker_failed")?;
        if byte == b'\n' {
            break;
        }
        if b.len() >= 1024 {
            return Err("worker_protocol");
        }
        b.push(byte);
    }
    serde_json::from_slice(&b).map_err(|_| "worker_protocol")
}
pub(super) async fn reap(child: &mut Child) -> Api<()> {
    let killed = child.kill().await;
    let waited = child.wait().await;
    if killed.is_ok() || waited.is_ok() {
        Ok(())
    } else {
        Err("reap_failed")
    }
}
pub(super) async fn validate(
    executable: &Path,
    c: &ExecutionCeiling,
    request: ImportRequest,
    files: &BTreeMap<String, Vec<u8>>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
    deadline: tokio::time::Instant,
) -> Api<()> {
    if *cancel.borrow() {
        return Err("cancelled");
    }
    let b = input(c, request, files)?;
    if tokio::time::Instant::now() >= deadline {
        return Err("deadline");
    }
    let mut child = spawn(executable, c, true)?;
    let result = tokio::select! {
        biased;
        _=cancel.changed()=>Err("cancelled"),
        r=tokio::time::timeout_at(deadline, async {
            let (_stdin,mut out)=send(&mut child,&b).await?;
            match event(&mut out).await? {
                WorkerEvent::Validated=>{},
                _=>return Err("worker_protocol"),
            }
            let status=child.wait().await.map_err(|_|"worker_failed")?;
            if !status.success() {return Err("invalid_artifact");}
            Ok(())
        })=>r.map_err(|_|"deadline").and_then(|x|x),
    };
    if result.is_err() {
        reap(&mut child).await?;
    }
    result
}
