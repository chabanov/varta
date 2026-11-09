mod imports;
mod runs;
use super::{contract::*, process};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, watch};
type Api<T> = std::result::Result<T, &'static str>;
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
struct Import {
    key: String,
    request: ImportRequest,
    status: ImportStatus,
    chunks: Vec<Vec<Option<Vec<u8>>>>,
    until: Instant,
    terminal: Option<Instant>,
    cancel: watch::Sender<bool>,
}
struct Catalog {
    request: ImportRequest,
    files: BTreeMap<String, Vec<u8>>,
    bytes: usize,
}
struct Run {
    key: String,
    status: RunStatus,
    until: Instant,
    terminal: Option<Instant>,
    goal: RunState,
    cancel: watch::Sender<bool>,
}
#[derive(Default)]
struct State {
    imports: BTreeMap<String, Import>,
    catalogs: BTreeMap<String, Arc<Catalog>>,
    runs: BTreeMap<String, Run>,
    tombstones: BTreeMap<String, Instant>,
}
pub(super) struct Registry {
    pub ceiling: ExecutionCeiling,
    pub host: String,
    executable: PathBuf,
    state: Mutex<State>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    validators: Arc<Semaphore>,
    reap_failed: AtomicBool,
}
impl Registry {
    pub fn new(ceiling: ExecutionCeiling, host: String, executable: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            ceiling,
            host,
            executable,
            state: Mutex::new(State::default()),
            tasks: Mutex::new(Vec::new()),
            validators: Arc::new(Semaphore::new(1)),
            reap_failed: AtomicBool::new(false),
        })
    }
    fn receipts(s: &State) -> usize {
        s.imports.len() + s.runs.len() + s.tombstones.len()
    }
    fn retained(s: &State) -> usize {
        s.catalogs.values().map(|v| v.bytes).sum::<usize>()
            + s.imports
                .values()
                .flat_map(|i| &i.chunks)
                .flatten()
                .flatten()
                .map(Vec::len)
                .sum::<usize>()
    }
    fn stop_locked(r: &mut Run, goal: RunState) {
        if !r.status.state.terminal() && r.status.state != RunState::Stopping {
            r.goal = goal;
            r.status.state = RunState::Stopping;
            r.status.address = None;
            r.cancel.send_replace(true);
        }
    }
    fn expire_locked(&self, s: &mut State) {
        let now = Instant::now();
        for i in s.imports.values_mut() {
            if i.until <= now
                && matches!(
                    i.status.state,
                    ImportState::Uploading | ImportState::Sealing | ImportState::Ready
                )
            {
                i.status.state = ImportState::Expired;
                i.status.error = Some("expired");
                i.chunks.clear();
                i.terminal = Some(now);
                i.cancel.send_replace(true);
            }
        }
        for r in s.runs.values_mut() {
            if r.until <= now {
                Self::stop_locked(r, RunState::Expired);
            }
        }
        let retention = Duration::from_secs(self.ceiling.retention_seconds);
        s.imports
            .retain(|_, i| i.terminal.is_none_or(|t| now.duration_since(t) < retention));
        s.runs
            .retain(|_, r| r.terminal.is_none_or(|t| now.duration_since(t) < retention));
        s.tombstones
            .retain(|_, t| now.duration_since(*t) < retention);
        s.catalogs.retain(|sha, _| {
            s.imports
                .values()
                .any(|i| i.status.state == ImportState::Ready && i.status.artifact_sha256 == *sha)
                || s.runs
                    .values()
                    .any(|r| !r.status.state.terminal() && r.status.artifact_sha256 == *sha)
        });
    }
    pub fn expire(&self) {
        self.expire_locked(&mut lock(&self.state));
        lock(&self.tasks).retain(|t| !t.is_finished());
    }
    pub fn revoke(&self) {
        let mut s = lock(&self.state);
        for i in s.imports.values() {
            i.cancel.send_replace(true);
        }
        for r in s.runs.values_mut() {
            Self::stop_locked(r, RunState::Stopped);
        }
    }
    pub async fn shutdown(&self) -> Api<()> {
        self.revoke();
        let tasks = std::mem::take(&mut *lock(&self.tasks));
        for t in tasks {
            if t.await.is_err() {
                self.reap_failed.store(true, Ordering::Release);
            }
        }
        if self.reap_failed.load(Ordering::Acquire) {
            return Err("reap_failed");
        }
        *lock(&self.state) = State::default();
        Ok(())
    }
    fn unconfirmed_reap(&self, id: &str) {
        self.reap_failed.store(true, Ordering::Release);
        if let Some(r) = lock(&self.state).runs.get_mut(id) {
            Self::stop_locked(r, RunState::Stopped);
            r.status.error = Some("reap_failed");
        }
    }
    fn finish(&self, id: &str, state: RunState, error: Option<&'static str>) {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        if let Some(r) = s.runs.get_mut(id) {
            if r.status.state == RunState::Stopping {
                r.status.state = r.goal;
                r.status.error = (r.goal == RunState::Expired).then_some("expired");
            } else {
                r.status.state = state;
                r.status.error = error;
            }
            r.status.address = None;
            r.terminal = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registry() -> Arc<Registry> {
        Registry::new(
            crate::execution::tests::ceiling(),
            "a".repeat(64),
            PathBuf::from("/must-not-execute"),
        )
    }
    fn request() -> StartRequest {
        StartRequest {
            schema_version: 1,
            host_sha256: "a".repeat(64),
            compiler_sha256: "c".repeat(64),
            artifact_sha256: "b".repeat(64),
            target: TARGET.into(),
            policy_id: POLICY_ID.into(),
            ttl_seconds: 180,
        }
    }
    fn seed(r: &Registry) -> String {
        let req = request();
        let id = "d".repeat(64);
        let (tx, _) = watch::channel(false);
        lock(&r.state).runs.insert(
            id.clone(),
            Run {
                key: "existing-request-0001".into(),
                status: RunStatus {
                    id: id.clone(),
                    host_sha256: req.host_sha256.clone(),
                    compiler_sha256: req.compiler_sha256.clone(),
                    request_sha256: identity(&req).unwrap(),
                    artifact_sha256: req.artifact_sha256.clone(),
                    state: RunState::Ready,
                    address: Some("127.0.0.1:45678".parse().unwrap()),
                    error: None,
                    expires_at_ms: now_ms() + 180000,
                },
                until: Instant::now() + Duration::from_secs(180),
                terminal: None,
                goal: RunState::Stopped,
                cancel: tx,
            },
        );
        id
    }
    #[test]
    fn native_stop_and_ttl_remain_stopping_until_process_reap() {
        let r = registry();
        let id = seed(&r);
        let first = r.status(&id).unwrap();
        assert_eq!(r.stop(&id).unwrap().state, RunState::Stopping);
        assert!(r.status(&id).unwrap().address.is_none());
        assert_eq!(r.status(&id).unwrap().expires_at_ms, first.expires_at_ms);
        r.finish(&id, RunState::Exited, None);
        assert_eq!(r.status(&id).unwrap().state, RunState::Stopped);
        let id = seed(&r);
        lock(&r.state).runs.get_mut(&id).unwrap().until = Instant::now() - Duration::from_millis(1);
        assert_eq!(r.status(&id).unwrap().state, RunState::Stopping);
        r.finish(&id, RunState::Failed, Some("late readiness"));
        assert_eq!(r.status(&id).unwrap().state, RunState::Expired);
        let id = seed(&r);
        r.unconfirmed_reap(&id);
        assert_eq!(r.status(&id).unwrap().state, RunState::Stopping);
        assert_eq!(
            r.start(request(), "new-after-reap-error").unwrap_err(),
            "unavailable"
        );
    }
    #[test]
    fn native_cancel_queued_validation_never_spawns_or_publishes() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let r = registry();
                let slot = r.validators.clone().acquire_owned().await.unwrap();
                let request = ImportRequest {
                    schema_version: 1,
                    host_sha256: r.host.clone(),
                    artifact_sha256: "f".repeat(64),
                    files: ["app.manifest.json", "app.wasm"]
                        .into_iter()
                        .map(|name| BundleFile {
                            path: name.into(),
                            sha256: hash(&[0]),
                            bytes: 1,
                        })
                        .collect(),
                };
                let status = r.prepare(request, "queued-import-0001").unwrap();
                r.chunk(&status.id, 0, 0, vec![0]).unwrap();
                r.chunk(&status.id, 1, 0, vec![0]).unwrap();
                r.seal(&status.id).unwrap();
                tokio::task::yield_now().await;
                assert_eq!(
                    r.cancel_import(&status.id).unwrap().state,
                    ImportState::Cancelled
                );
                drop(slot);
                let tasks = std::mem::take(&mut *lock(&r.tasks));
                for task in tasks {
                    task.await.unwrap();
                }
                assert_eq!(
                    r.import_status(&status.id).unwrap().state,
                    ImportState::Cancelled
                );
                assert!(lock(&r.state).catalogs.is_empty());
            });
    }
    #[test]
    fn native_replay_and_precreate_cancel_precede_catalog_and_capacity() {
        let r = registry();
        let id = seed(&r);
        assert_eq!(r.start(request(), "existing-request-0001").unwrap().id, id);
        r.stop(&id).unwrap();
        r.finish(&id, RunState::Exited, None);
        assert_eq!(
            r.start(request(), "existing-request-0001").unwrap().state,
            RunState::Stopped
        );
        let mut changed = request();
        changed.compiler_sha256 = "f".repeat(64);
        assert_eq!(
            r.start(changed, "existing-request-0001").unwrap_err(),
            "idempotency_conflict"
        );
        r.cancel_key("cancel-before-create-1").unwrap();
        assert_eq!(
            r.start(request(), "cancel-before-create-1").unwrap_err(),
            "cancelled_request"
        );
        for n in 0..MAX_RECEIPTS - 2 {
            r.cancel_key(&format!("cancel-key-{n:08}")).unwrap();
        }
        assert_eq!(r.cancel_key("one-too-many-00001").unwrap_err(), "busy");
        assert_eq!(
            r.start(request(), "new-missing-artifact1").unwrap_err(),
            "artifact_not_found"
        );
    }
}
