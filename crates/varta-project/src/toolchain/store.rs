//! Bounded ephemeral tool operations, not application jobs or durable ownership.
use super::{compile::WorkerReply, contract::*};
use base64::Engine;
use hyper::body::Bytes;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{Semaphore, watch},
    task::JoinHandle,
};

pub(super) struct ArtifactData {
    pub description: ArtifactDescription,
    pub files: BTreeMap<String, Bytes>,
    bytes: usize,
}
struct Job {
    key: String,
    status: JobStatus,
    cancel: watch::Sender<bool>,
    finished: Option<Instant>,
    artifact: Option<ArtifactData>,
}
pub(super) struct Registry {
    pub ceiling: ToolchainCeiling,
    pub compiler: String,
    pub executable: PathBuf,
    jobs: Mutex<BTreeMap<String, Job>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    workers: Arc<Semaphore>,
}
pub(super) fn random_id() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| std::io::Error::other("toolchain randomness unavailable"))?;
    use std::fmt::Write;
    let mut id = String::new();
    for b in bytes {
        write!(&mut id, "{b:02x}").expect("writing a String");
    }
    Ok(id)
}
impl Registry {
    pub fn new(ceiling: ToolchainCeiling, compiler: String, executable: PathBuf) -> Arc<Self> {
        let workers = Arc::new(Semaphore::new(ceiling.max_workers));
        Arc::new(Self {
            ceiling,
            compiler,
            executable,
            workers,
            jobs: Mutex::new(BTreeMap::new()),
            tasks: Mutex::new(Vec::new()),
        })
    }
    pub fn submit(
        self: &Arc<Self>,
        request: CompileRequest,
        key: &str,
    ) -> std::result::Result<JobStatus, &'static str> {
        if !idempotency_valid(key) {
            return Err("invalid_idempotency_key");
        }
        request.validate().map_err(|_| "invalid_request")?;
        if request.compiler_sha256 != self.compiler {
            return Err("compiler_mismatch");
        }
        if !self.ceiling.operations.contains(&request.operation)
            || !self.ceiling.profiles.contains(&request.language_profile)
        {
            return Err("denied");
        }
        if let Some(app) = &request.application
            && !self
                .ceiling
                .application_origins
                .as_ref()
                .is_some_and(|origins| origins.contains(&app.public_origin))
        {
            return Err("denied");
        }
        self.expire();
        let request_sha256 = request.identity().map_err(|_| "invalid_request")?;
        let mut jobs = self.jobs.lock().map_err(|_| "unavailable")?;
        if let Some(job) = jobs.values().find(|job| job.key == key) {
            return if job.status.request_sha256 == request_sha256 {
                Ok(job.status.clone())
            } else {
                Err("idempotency_conflict")
            };
        }
        let mut tasks = self.tasks.lock().map_err(|_| "unavailable")?;
        tasks.retain(|t| !t.is_finished());
        if jobs.len() >= MAX_RECEIPTS || tasks.len() >= self.ceiling.max_jobs {
            return Err("busy");
        }
        let id = random_id().map_err(|_| "unavailable")?;
        if jobs.contains_key(&id) {
            return Err("unavailable");
        }
        let status = JobStatus {
            id: id.clone(),
            compiler_sha256: self.compiler.clone(),
            request_sha256,
            state: JobState::Queued,
            error: None,
            result: None,
            artifact: None,
            forgotten: false,
        };
        let (cancel, rx) = watch::channel(false);
        jobs.insert(
            id.clone(),
            Job {
                key: key.to_owned(),
                status: status.clone(),
                cancel,
                finished: None,
                artifact: None,
            },
        );
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(self.ceiling.deadline_ms);
        let owner = Arc::clone(self);
        tasks.push(tokio::spawn(async move {
            owner.run(id, request, rx, deadline).await;
        }));
        Ok(status)
    }
    pub fn status(&self, id: &str) -> Option<JobStatus> {
        self.expire();
        self.jobs.lock().ok()?.get(id).map(|j| j.status.clone())
    }
    pub fn cancel(&self, id: &str) -> Option<JobStatus> {
        let mut jobs = self.jobs.lock().ok()?;
        let job = jobs.get_mut(id)?;
        if !job.status.state.terminal() {
            job.status.state = JobState::Cancelled;
            job.status.error = Some("cancelled");
            job.finished = Some(Instant::now());
            job.cancel.send_replace(true);
        }
        Some(job.status.clone())
    }
    pub fn forget(&self, id: &str) -> std::result::Result<bool, &'static str> {
        let mut jobs = self.jobs.lock().map_err(|_| "unavailable")?;
        if jobs.get(id).is_some_and(|j| !j.status.state.terminal()) {
            return Err("running");
        }
        let Some(job) = jobs.get_mut(id) else {
            return Ok(false);
        };
        job.artifact = None;
        job.status.artifact = None;
        job.status.forgotten = true;
        Ok(true)
    }
    pub fn file(&self, digest: &str, path: &str) -> Option<Bytes> {
        self.expire();
        self.jobs
            .lock()
            .ok()?
            .values()
            .filter_map(|j| j.artifact.as_ref())
            .find(|a| a.description.sha256 == digest)?
            .files
            .get(path)
            .cloned()
    }
    pub fn expire(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.retain(|_, j| {
                !j.finished.is_some_and(|at| {
                    at.elapsed() >= Duration::from_secs(self.ceiling.retention_seconds)
                })
            });
        }
    }
    pub async fn shutdown(&self) {
        self.cancel_all();
        let tasks = self
            .tasks
            .lock()
            .map(|mut ts| std::mem::take(&mut *ts))
            .unwrap_or_default();
        for task in tasks {
            let _ = task.await;
        }
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.clear();
        }
    }
    pub fn cancel_all(&self) {
        if let Ok(jobs) = self.jobs.lock() {
            for job in jobs.values() {
                job.cancel.send_replace(true);
            }
        }
    }
    async fn run(
        self: Arc<Self>,
        id: String,
        request: CompileRequest,
        mut cancel: watch::Receiver<bool>,
        deadline: tokio::time::Instant,
    ) {
        let slot = tokio::select! {
            biased;
            _=cancel.changed()=>{return;},
            _=tokio::time::sleep_until(deadline)=>{self.finish(&id,Err("deadline"),deadline);return;},
            slot=self.workers.clone().acquire_owned()=>slot,
        };
        let Ok(_slot) = slot else {
            self.finish(&id, Err("unavailable"), deadline);
            return;
        };
        {
            let Ok(mut jobs) = self.jobs.lock() else {
                return;
            };
            let Some(job) = jobs.get_mut(&id) else {
                return;
            };
            if job.status.state != JobState::Queued || *cancel.borrow() {
                return;
            }
            job.status.state = JobState::Running;
        }
        let result = self.invoke(&request, &mut cancel, deadline).await;
        self.finish(&id, result, deadline);
    }
    async fn invoke(
        &self,
        request: &CompileRequest,
        cancel: &mut watch::Receiver<bool>,
        deadline: tokio::time::Instant,
    ) -> std::result::Result<WorkerReply, &'static str> {
        use std::process::Stdio;
        let input = serde_json::to_vec(request).map_err(|_| "invalid_request")?;
        if input.len() > MAX_REQUEST_BYTES {
            return Err("invalid_request");
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("deadline");
        }
        let mut child = Command::new(&self.executable)
            .args([
                "toolchain-worker",
                "--address-space-bytes",
                &self.ceiling.address_space_bytes.to_string(),
                "--cpu-seconds",
                &self.ceiling.cpu_seconds.to_string(),
            ])
            .env_clear()
            .env("MallocNanoZone", "0")
            .current_dir(self.executable.parent().expect("pinned worker parent"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "worker_failed")?;
        let result = {
            let run = async {
                let mut stdin = child.stdin.take().ok_or("worker_failed")?;
                stdin
                    .write_all(&(input.len() as u64).to_le_bytes())
                    .await
                    .map_err(|_| "worker_failed")?;
                stdin.write_all(&input).await.map_err(|_| "worker_failed")?;
                let stdout = child.stdout.take().ok_or("worker_failed")?;
                let mut bytes = Vec::new();
                stdout
                    .take((MAX_WORKER_REPLY + 1) as u64)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| "worker_failed")?;
                if bytes.len() > MAX_WORKER_REPLY {
                    return Err("resource_limit");
                }
                let status = child.wait().await.map_err(|_| "worker_failed")?;
                drop(stdin);
                if !status.success() {
                    return Err("worker_failed");
                }
                serde_json::from_slice(&bytes).map_err(|_| "worker_failed")
            };
            tokio::pin!(run);
            if *cancel.borrow() {
                Err("cancelled")
            } else {
                tokio::select! {_=cancel.changed()=>Err("cancelled"),_=tokio::time::sleep_until(deadline)=>Err("deadline"),result=&mut run=>result}
            }
        };
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result
    }
    fn finish(
        &self,
        id: &str,
        result: std::result::Result<WorkerReply, &'static str>,
        deadline: tokio::time::Instant,
    ) {
        // Decode outside the state lock. Cancellation/revocation wins publication.
        let result = result.and_then(decode);
        let result = if tokio::time::Instant::now() >= deadline {
            Err("deadline")
        } else {
            result
        };
        let Ok(mut jobs) = self.jobs.lock() else {
            return;
        };
        let retained: usize = jobs
            .values()
            .filter_map(|j| j.artifact.as_ref())
            .map(|a| a.bytes)
            .sum();
        let Some(job) = jobs.get_mut(id) else {
            return;
        };
        if job.status.state.terminal() {
            return;
        }
        job.finished = Some(Instant::now());
        match result {
            Ok((mut reply, artifact))
                if artifact
                    .as_ref()
                    .is_none_or(|a| retained + a.bytes <= MAX_RETAINED_BYTES) =>
            {
                job.status.state = if reply.ok {
                    JobState::Succeeded
                } else {
                    JobState::Failed
                };
                if !reply.ok {
                    job.status.error = Some("compile_error");
                }
                if let Some(artifact) = artifact {
                    reply.artifact_sha256 = artifact.description.sha256.clone();
                    job.status.artifact = Some(artifact.description.clone());
                    job.artifact = Some(artifact);
                }
                job.status.result = Some(reply);
            }
            Ok(_) => {
                job.status.state = JobState::Failed;
                job.status.error = Some("resource_limit");
            }
            Err(error) => {
                job.status.state = if error == "cancelled" {
                    JobState::Cancelled
                } else {
                    JobState::Failed
                };
                job.status.error = Some(error);
            }
        }
    }
}
fn decode(
    reply: WorkerReply,
) -> std::result::Result<(CompilationResult, Option<ArtifactData>), &'static str> {
    if reply.files.len() > MAX_ARTIFACT_FILES
        || reply.result.diagnostics.len() > 1
        || (!reply.result.ok && !reply.files.is_empty())
        || !reply.result.artifact_sha256.is_empty()
    {
        return Err("worker_failed");
    }
    if reply.files.is_empty() {
        return Ok((reply.result, None));
    }
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"varta-toolchain-artifact-v1\0");
    let mut files = BTreeMap::new();
    let mut descriptions = Vec::new();
    let mut total = 0;
    for (path, encoded) in reply.files {
        if !valid_path(&path) {
            return Err("worker_failed");
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "worker_failed")?;
        total += bytes.len();
        if total > MAX_ARTIFACT_BYTES {
            return Err("resource_limit");
        }
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
        descriptions.push(ArtifactFile {
            path: path.clone(),
            sha256: hash(&bytes),
            bytes: bytes.len(),
        });
        files.insert(path, Bytes::from(bytes));
    }
    Ok((
        reply.result,
        Some(ArtifactData {
            description: ArtifactDescription {
                sha256: format!("{:x}", digest.finalize()),
                files: descriptions,
            },
            files,
            bytes: total,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn toolchain_idempotency_recovers_lost_acceptance_before_busy_and_fences_cancel() {
        let ceiling:ToolchainCeiling=serde_json::from_value(serde_json::json!({"schemaVersion":1,"listen":"127.0.0.1:0","operations":["check","build"],"profiles":["checked-f64-utf16-v40"],"maxJobs":1,"maxWorkers":1,"deadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"retentionSeconds":300})).unwrap();
        let compiler = "a".repeat(64);
        let registry = Registry::new(ceiling, compiler.clone(), PathBuf::from("must-not-start"));
        let gate = registry.workers.clone().acquire_owned().await.unwrap();
        let request:CompileRequest=serde_json::from_value(serde_json::json!({"schemaVersion":1,"compilerSha256":compiler,"operation":"check","target":"browser","languageProfile":"checked-f64-utf16-v40","dependencyPolicy":"local-only-v1","entry":"main.ts","files":[{"path":"main.ts","content":"export function main():void{}"}]})).unwrap();
        let key = "lost-response-correlation";
        let accepted = registry.submit(request.clone(), key).unwrap();
        assert_eq!(
            registry.submit(request.clone(), key).unwrap().id,
            accepted.id
        );
        assert_eq!(
            registry
                .submit(request.clone(), "different-correlation")
                .unwrap_err(),
            "busy"
        );
        let mut changed = request.clone();
        changed.files[0].content.push(' ');
        assert_eq!(
            registry.submit(changed, key).unwrap_err(),
            "idempotency_conflict"
        );
        assert_eq!(
            registry.cancel(&accepted.id).unwrap().state,
            JobState::Cancelled
        );
        registry.forget(&accepted.id).unwrap();
        let replay = registry.submit(request.clone(), key).unwrap();
        assert_eq!(replay.id, accepted.id);
        assert!(replay.forgotten);
        assert_eq!(replay.state, JobState::Cancelled);
        tokio::task::yield_now().await;
        for i in 1..MAX_RECEIPTS {
            let key = format!("bounded-receipt-{i:05}");
            let job = registry.submit(request.clone(), &key).unwrap();
            registry.cancel(&job.id).unwrap();
            tokio::task::yield_now().await;
        }
        assert_eq!(
            registry
                .submit(request.clone(), "receipt-capacity-overflow")
                .unwrap_err(),
            "busy"
        );
        assert_eq!(
            registry.submit(request.clone(), key).unwrap().id,
            accepted.id
        );
        {
            let mut jobs = registry.jobs.lock().unwrap();
            for job in jobs.values_mut() {
                job.finished = Some(Instant::now() - Duration::from_secs(301));
            }
        }
        registry.expire();
        assert!(registry.jobs.lock().unwrap().is_empty());
        let renewed = registry.submit(request, key).unwrap();
        assert_ne!(renewed.id, accepted.id);
        registry.cancel(&renewed.id);
        drop(gate);
        registry.shutdown().await;
    }
}
