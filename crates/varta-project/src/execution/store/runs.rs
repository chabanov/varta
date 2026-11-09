use super::*;
impl Registry {
    pub fn start(self: &Arc<Self>, request: StartRequest, key: &str) -> Api<RunStatus> {
        if !key_valid(key) {
            return Err("invalid_idempotency_key");
        }
        request
            .validate(self.ceiling.max_run_seconds)
            .map_err(|_| "invalid_request")?;
        if request.host_sha256 != self.host {
            return Err("host_mismatch");
        }
        let request_sha = identity(&request).map_err(|_| "invalid_request")?;
        let (status, catalog, cancel, until, startup) = {
            let mut s = lock(&self.state);
            self.expire_locked(&mut s);
            if s.tombstones.contains_key(key) {
                return Err("cancelled_request");
            }
            if let Some(r) = s.runs.values().find(|r| r.key == key) {
                return if r.status.request_sha256 == request_sha {
                    Ok(r.status.clone())
                } else {
                    Err("idempotency_conflict")
                };
            }
            if self.reap_failed.load(Ordering::Acquire) {
                return Err("unavailable");
            }
            let catalog = Arc::clone(
                s.catalogs
                    .get(&request.artifact_sha256)
                    .ok_or("artifact_not_found")?,
            );
            if Self::receipts(&s) >= MAX_RECEIPTS
                || s.runs
                    .values()
                    .filter(|r| !r.status.state.terminal())
                    .count()
                    >= self.ceiling.max_runs
            {
                return Err("busy");
            }
            let id = random().map_err(|_| "unavailable")?;
            let status = RunStatus {
                id: id.clone(),
                host_sha256: self.host.clone(),
                compiler_sha256: request.compiler_sha256,
                request_sha256: request_sha,
                artifact_sha256: request.artifact_sha256,
                state: RunState::Starting,
                address: None,
                error: None,
                expires_at_ms: now_ms() + request.ttl_seconds * 1000,
            };
            let (tx, cancel) = watch::channel(false);
            let until = Instant::now() + Duration::from_secs(request.ttl_seconds);
            s.runs.insert(
                id,
                Run {
                    key: key.into(),
                    status: status.clone(),
                    until,
                    terminal: None,
                    goal: RunState::Stopped,
                    cancel: tx,
                },
            );
            (
                status,
                catalog,
                cancel,
                until,
                Instant::now() + Duration::from_millis(self.ceiling.startup_deadline_ms),
            )
        };
        let this = Arc::clone(self);
        let id = status.id.clone();
        lock(&self.tasks).retain(|t| !t.is_finished());
        lock(&self.tasks).push(tokio::spawn(async move {
            this.execute(&id, catalog, cancel, until, startup).await;
        }));
        Ok(status)
    }
    pub fn status(&self, id: &str) -> Option<RunStatus> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        s.runs.get(id).map(|r| r.status.clone())
    }
    pub fn stop(&self, id: &str) -> Api<RunStatus> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = s.runs.get_mut(id).ok_or("not_found")?;
        Self::stop_locked(r, RunState::Stopped);
        Ok(r.status.clone())
    }
    pub fn cancel_key(&self, key: &str) -> Api<()> {
        if !key_valid(key) {
            return Err("invalid_idempotency_key");
        }
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        if let Some(r) = s.runs.values_mut().find(|r| r.key == key) {
            Self::stop_locked(r, RunState::Stopped);
            return Ok(());
        }
        if s.tombstones.contains_key(key) {
            return Ok(());
        }
        if Self::receipts(&s) >= MAX_RECEIPTS {
            return Err("busy");
        }
        s.tombstones.insert(key.into(), Instant::now());
        Ok(())
    }
    fn ready(&self, id: &str, address: std::net::SocketAddr) -> Api<()> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = s.runs.get_mut(id).ok_or("not_found")?;
        if r.status.state != RunState::Starting {
            return Err("cancelled");
        }
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err("worker_protocol");
        }
        r.status.state = RunState::Ready;
        r.status.address = Some(address);
        Ok(())
    }
    async fn execute(
        &self,
        id: &str,
        catalog: Arc<Catalog>,
        mut cancel: watch::Receiver<bool>,
        until: Instant,
        startup: Instant,
    ) {
        if *cancel.borrow() {
            self.finish(id, RunState::Failed, Some("cancelled"));
            return;
        }
        let b = match process::input(&self.ceiling, catalog.request.clone(), &catalog.files) {
            Ok(b) => b,
            Err(e) => {
                self.finish(id, RunState::Failed, Some(e));
                return;
            }
        };
        if *cancel.borrow() {
            self.finish(id, RunState::Failed, Some("cancelled"));
            return;
        }
        if Instant::now() >= startup {
            self.finish(id, RunState::Failed, Some("startup_deadline"));
            return;
        }
        let mut child = match process::spawn(&self.executable, &self.ceiling, false) {
            Ok(c) => c,
            Err(e) => {
                self.finish(id, RunState::Failed, Some(e));
                return;
            }
        };
        let outcome = {
            let activity = async {
                let (_stdin, mut out) = tokio::time::timeout_at(startup.into(), async {
                    let pipes = process::send(&mut child, &b).await?;
                    let (stdin, mut out) = pipes;
                    match process::event(&mut out).await? {
                        WorkerEvent::Listening { address } => self.ready(id, address)?,
                        _ => return Err("startup_failed"),
                    }
                    Ok((stdin, out))
                })
                .await
                .map_err(|_| "startup_deadline")??;
                let success = match process::event(&mut out).await? {
                    WorkerEvent::Exited { ok, exit_code } => ok && exit_code == 0,
                    _ => return Err("worker_protocol"),
                };
                let status = child.wait().await.map_err(|_| "worker_failed")?;
                Ok(success && status.success())
            };
            tokio::pin!(activity);
            tokio::select! {
                biased;
                _=cancel.changed()=>Err("cancelled"),
                _=tokio::time::sleep_until(until.into())=>{
                    let mut s=lock(&self.state);
                    if let Some(r)=s.runs.get_mut(id) {Self::stop_locked(r,RunState::Expired);}
                    Err("expired")
                },
                r=&mut activity=>r,
            }
        };
        if outcome.is_err() && process::reap(&mut child).await.is_err() {
            self.unconfirmed_reap(id);
            return;
        }
        match outcome {
            Ok(true) => self.finish(id, RunState::Exited, None),
            Ok(false) => self.finish(id, RunState::Failed, Some("guest_failed")),
            Err(e) => self.finish(id, RunState::Failed, Some(e)),
        }
    }
}
