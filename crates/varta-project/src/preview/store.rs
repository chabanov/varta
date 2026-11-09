mod application;
mod imports;
mod readiness;
mod runs;
mod validation;
use super::{
    bundle::{ValidationInput, ValidationOutput},
    contract::*,
};
use base64::Engine;
use hyper::body::Bytes;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Semaphore, watch},
};

type Api<T> = std::result::Result<T, &'static str>;
struct Import {
    key: String,
    request: ImportRequest,
    status: ImportStatus,
    chunks: Vec<Vec<Option<Bytes>>>,
    until: Instant,
    terminal: Option<Instant>,
    cancel: watch::Sender<bool>,
}
pub(super) struct Catalog {
    pub files: BTreeMap<String, Bytes>,
    bytes: usize,
}
struct Run {
    scope: Option<(
        crate::application::NativeScope,
        crate::application::ApplicationRequest,
    )>,
    cancel: watch::Sender<bool>,
    key: String,
    status: RunStatus,
    until: Instant,
    terminal: Option<Instant>,
    ticket: Option<(String, Instant)>,
    session: Option<String>,
    client: Option<String>,
    lease: Option<Instant>,
}
#[derive(Default)]
struct State {
    imports: BTreeMap<String, Import>,
    catalogs: BTreeMap<String, Arc<Catalog>>,
    runs: BTreeMap<String, Run>,
    tombstones: BTreeMap<String, Instant>,
}
pub(super) struct Registry {
    pub application: Option<crate::application::Upstream>,
    pub ceiling: PreviewCeiling,
    pub host: String,
    pub host_instance: String,
    pub origin: String,
    executable: PathBuf,
    state: Mutex<State>,
    workers: Arc<Semaphore>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn matches_secret(secret: &str, expected: &str) -> bool {
    bool::from(
        hash(secret.as_bytes())
            .as_bytes()
            .ct_eq(expected.as_bytes()),
    )
}
impl Registry {
    fn receipts(s: &State) -> usize {
        s.imports.len() + s.runs.len() + s.tombstones.len()
    }
    #[cfg(test)]
    pub fn new(
        ceiling: PreviewCeiling,
        host: String,
        origin: String,
        executable: PathBuf,
    ) -> Arc<Self> {
        Self::with_application(ceiling, host, origin, executable, None).unwrap()
    }
    pub fn with_application(
        ceiling: PreviewCeiling,
        host: String,
        origin: String,
        executable: PathBuf,
        application: Option<crate::application::Upstream>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            application,
            ceiling,
            host,
            host_instance: random()?,
            origin,
            executable,
            state: Mutex::new(State::default()),
            workers: Arc::new(Semaphore::new(1)),
            tasks: Mutex::new(Vec::new()),
        }))
    }
    fn retained(s: &State) -> usize {
        s.catalogs.values().map(|c| c.bytes).sum::<usize>()
            + s.imports
                .values()
                .flat_map(|i| &i.chunks)
                .flatten()
                .flatten()
                .map(Bytes::len)
                .sum::<usize>()
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
                i.terminal = Some(now);
                i.chunks.clear();
                i.cancel.send_replace(true);
            }
        }
        for r in s.runs.values_mut() {
            if !r.status.state.terminal() {
                if r.until <= now {
                    Self::end(r, RunState::Expired, Some("expired"));
                } else if r.lease.is_some_and(|at| at <= now) {
                    Self::end(r, RunState::Stopped, Some("lease_lost"));
                }
            }
        }
        let retention = Duration::from_secs(self.ceiling.retention_seconds);
        s.imports
            .retain(|_, i| i.terminal.is_none_or(|t| now.duration_since(t) < retention));
        s.runs
            .retain(|_, r| r.terminal.is_none_or(|t| now.duration_since(t) < retention));
        s.tombstones
            .retain(|_, at| now.duration_since(*at) < retention);
        s.catalogs.retain(|sha, _| {
            s.imports
                .values()
                .any(|i| i.status.state == ImportState::Ready && i.status.artifact_sha256 == *sha)
                || s.runs
                    .values()
                    .any(|r| !r.status.state.terminal() && r.status.bundle_sha256 == *sha)
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
            Self::end(r, RunState::Stopped, Some("shutdown"));
        }
    }
    pub async fn shutdown(&self) {
        self.revoke();
        let tasks = std::mem::take(&mut *lock(&self.tasks));
        for t in tasks {
            let _ = t.await;
        }
        *lock(&self.state) = State::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registry() -> Arc<Registry> {
        Registry::new(
            super::super::tests::ceiling(),
            "a".repeat(64),
            "http://127.0.0.2:8800".into(),
            PathBuf::from("/not-executed"),
        )
    }
    fn request() -> StartRequest {
        StartRequest {
            schema_version: 1,
            host_sha256: "a".repeat(64),
            bundle_sha256: "b".repeat(64),
            target: TARGET.into(),
            policy_id: POLICY_ID.into(),
            ttl_seconds: 180,
        }
    }
    fn seed(r: &Registry) {
        let mut s = lock(&r.state);
        let (cancel, _) = watch::channel(false);
        s.imports.insert(
            "seed".into(),
            Import {
                key: "seed-key-00000000".into(),
                request: ImportRequest {
                    schema_version: 1,
                    host_sha256: "a".repeat(64),
                    artifact_sha256: "b".repeat(64),
                    files: vec![],
                },
                status: ImportStatus {
                    id: "seed".into(),
                    host_sha256: "a".repeat(64),
                    request_sha256: "c".repeat(64),
                    artifact_sha256: "b".repeat(64),
                    state: ImportState::Ready,
                    error: None,
                    expires_at_ms: now_ms() + 60_000,
                },
                chunks: vec![],
                until: Instant::now() + Duration::from_secs(60),
                terminal: None,
                cancel,
            },
        );
        s.catalogs.insert(
            "b".repeat(64),
            Arc::new(Catalog {
                files: BTreeMap::from([("index.html".into(), Bytes::from_static(b"trusted"))]),
                bytes: 7,
            }),
        );
    }
    #[test]
    fn preview_replay_precedes_catalog_availability_and_terminal_cancel_never_restarts() {
        let r = registry();
        assert_eq!(
            r.start(request(), "request-00000001").unwrap_err(),
            "bundle_not_found"
        );
        assert!(lock(&r.state).runs.is_empty());
        seed(&r);
        let first = r.start(request(), "request-00000001").unwrap();
        lock(&r.state).imports.clear();
        assert_eq!(r.start(request(), "request-00000001").unwrap().id, first.id);
        r.stop(&first.id).unwrap();
        assert_eq!(
            r.start(request(), "request-00000001").unwrap().state,
            RunState::Stopped
        );
        let mut changed = request();
        changed.ttl_seconds = 179;
        assert_eq!(
            r.start(changed, "request-00000001").unwrap_err(),
            "idempotency_conflict"
        );
        r.cancel_key("never-started-0001").unwrap();
        assert_eq!(
            r.start(request(), "never-started-0001").unwrap_err(),
            "cancelled_request"
        );
        assert_eq!(lock(&r.state).runs.len(), 1);
    }
    #[test]
    fn preview_ticket_is_one_use_client_claim_is_unique_and_stop_revokes_every_resource() {
        let r = registry();
        seed(&r);
        let run = r.start(request(), "request-00000001").unwrap();
        let (old, _) = r.ticket(&run.id).unwrap();
        let (url, _) = r.ticket(&run.id).unwrap();
        assert_eq!(
            r.activate(old.split('#').nth(1).unwrap()).unwrap_err(),
            "unauthorized"
        );
        let token = url.split('#').nth(1).unwrap();
        let (id, cookie, _) = r.activate(token).unwrap();
        assert_eq!(r.activate(token).unwrap_err(), "unauthorized");
        assert!(r.file(&id, &cookie, "index.html").is_ok());
        assert!(r.file(&id, &"f".repeat(64), "index.html").is_err());
        let (client, _) = r.claim(&id, &cookie).unwrap();
        assert_eq!(r.claim(&id, &cookie).unwrap_err(), "already_claimed");
        assert!(r.lease(&id, &cookie, &"f".repeat(64), None).is_err());
        r.lease(&id, &cookie, &client, Some(GuestState::Ready))
            .unwrap();
        assert_eq!(r.status(&id).unwrap().guest_state, GuestState::Ready);
        let encoded = serde_json::to_string(&r.status(&id).unwrap()).unwrap();
        for secret in [token, &cookie, &client] {
            assert!(!encoded.contains(secret));
        }
        r.stop(&id).unwrap();
        assert!(r.file(&id, &cookie, "index.html").is_err());
        assert!(r.lease(&id, &cookie, &client, None).is_err());
    }
    #[test]
    fn preview_upload_exact_replays_cancellation_and_receipts_are_bounded() {
        let r = registry();
        let req = ImportRequest {
            schema_version: 1,
            host_sha256: "a".repeat(64),
            artifact_sha256: "b".repeat(64),
            files: vec![BundleFile {
                path: "app.wasm".into(),
                sha256: hash(b"data"),
                bytes: 4,
            }],
        };
        let first = r.prepare(req.clone(), "upload-000000001").unwrap();
        r.chunk(&first.id, 0, 0, Bytes::from_static(b"data"))
            .unwrap();
        r.chunk(&first.id, 0, 0, Bytes::from_static(b"data"))
            .unwrap();
        assert_eq!(
            r.chunk(&first.id, 0, 0, Bytes::from_static(b"evil"))
                .unwrap_err(),
            "chunk_conflict"
        );
        assert_eq!(
            r.prepare(req.clone(), "upload-000000001").unwrap().id,
            first.id
        );
        r.cancel_import(&first.id).unwrap();
        assert_eq!(
            r.prepare(req, "upload-000000001").unwrap().state,
            ImportState::Cancelled
        );
        for i in 0..MAX_RECEIPTS - 1 {
            r.cancel_key(&format!("tombstone-{i:08}")).unwrap();
        }
        assert_eq!(r.cancel_key("tombstone-overflow").unwrap_err(), "busy");
        assert_eq!(Registry::receipts(&lock(&r.state)), MAX_RECEIPTS);
    }
    #[test]
    fn preview_expiry_never_slides_and_lease_loss_ends_authority() {
        let r = registry();
        seed(&r);
        let run = r.start(request(), "request-00000001").unwrap();
        let (url, _) = r.ticket(&run.id).unwrap();
        let (id, cookie, _) = r.activate(url.split('#').nth(1).unwrap()).unwrap();
        let (client, _) = r.claim(&id, &cookie).unwrap();
        r.lease(&id, &cookie, &client, None).unwrap();
        assert_eq!(r.status(&id).unwrap().expires_at_ms, run.expires_at_ms);
        lock(&r.state).runs.get_mut(&id).unwrap().lease =
            Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(r.status(&id).unwrap().state, RunState::Stopped);
        assert_eq!(r.status(&id).unwrap().error, Some("lease_lost"));
        assert!(r.file(&id, &cookie, "index.html").is_err());
    }
}
