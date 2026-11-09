use super::*;

impl Registry {
    pub fn start(&self, request: StartRequest, key: &str) -> Api<RunStatus> {
        if self.application.is_some() {
            return Err("invalid_request");
        }
        let sha = identity(&request).map_err(|_| "invalid_request")?;
        self.start_inner(request, key, sha, None)
    }
    pub(super) fn start_inner(
        &self,
        request: StartRequest,
        key: &str,
        request_sha: String,
        scope: Option<(
            crate::application::NativeScope,
            crate::application::ApplicationRequest,
        )>,
    ) -> Api<RunStatus> {
        if !key_valid(key) {
            return Err("invalid_idempotency_key");
        }
        request
            .validate(self.ceiling.max_run_seconds)
            .map_err(|_| "invalid_request")?;
        if request.host_sha256 != self.host {
            return Err("host_mismatch");
        }
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
        if !s.catalogs.contains_key(&request.bundle_sha256) {
            return Err("bundle_not_found");
        }
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
        let expires_at_ms = scope.as_ref().map_or_else(
            || now_ms() + request.ttl_seconds * POLL_MS,
            |(native, _)| {
                native
                    .expires_at_ms
                    .min(now_ms() + request.ttl_seconds * POLL_MS)
            },
        );
        let ttl = if scope.is_some() {
            Duration::from_millis(expires_at_ms.saturating_sub(now_ms()))
        } else {
            Duration::from_secs(request.ttl_seconds)
        };
        if ttl.is_zero() {
            return Err("ended");
        }
        let (cancel, _) = watch::channel(false);
        let status = RunStatus {
            id: id.clone(),
            host_sha256: self.host.clone(),
            request_sha256: request_sha,
            bundle_sha256: request.bundle_sha256,
            state: RunState::Ready,
            guest_state: GuestState::Pending,
            error: None,
            expires_at_ms,
        };
        s.runs.insert(
            id,
            Run {
                scope,
                cancel,
                key: key.into(),
                status: status.clone(),
                until: Instant::now() + ttl,
                terminal: None,
                ticket: None,
                session: None,
                client: None,
                lease: None,
            },
        );
        Ok(status)
    }
    pub fn status(&self, id: &str) -> Option<RunStatus> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        s.runs.get(id).map(|r| r.status.clone())
    }
    pub(super) fn end(r: &mut Run, state: RunState, error: Option<&'static str>) {
        r.cancel.send_replace(true);
        r.status.state = state;
        r.status.error = error;
        r.ticket = None;
        r.session = None;
        r.client = None;
        r.lease = None;
        r.terminal = Some(Instant::now());
    }
    pub fn stop(&self, id: &str) -> Api<RunStatus> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = s.runs.get_mut(id).ok_or("not_found")?;
        if !r.status.state.terminal() {
            Self::end(r, RunState::Stopped, None);
        }
        Ok(r.status.clone())
    }
    pub fn cancel_key(&self, key: &str) -> Api<()> {
        if !key_valid(key) {
            return Err("invalid_idempotency_key");
        }
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        if let Some(r) = s.runs.values_mut().find(|r| r.key == key) {
            if !r.status.state.terminal() {
                Self::end(r, RunState::Stopped, None);
            }
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
    pub fn ticket(&self, id: &str) -> Api<(String, u64)> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = s.runs.get_mut(id).ok_or("not_found")?;
        if r.status.state != RunState::Ready {
            return Err("not_ready");
        }
        let token = random().map_err(|_| "unavailable")?;
        let time =
            Duration::from_millis(TICKET_MS).min(r.until.saturating_duration_since(Instant::now()));
        r.ticket = Some((hash(token.as_bytes()), Instant::now() + time));
        Ok((
            format!("{}/#{token}", self.origin),
            now_ms() + time.as_millis() as u64,
        ))
    }
    pub fn activate(&self, ticket: &str) -> Api<(String, String, u64)> {
        if !digest(ticket) {
            return Err("unauthorized");
        }
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let now = Instant::now();
        let r = s
            .runs
            .values_mut()
            .find(|r| {
                r.status.state == RunState::Ready
                    && r.ticket
                        .as_ref()
                        .is_some_and(|(h, until)| *until > now && matches_secret(ticket, h))
            })
            .ok_or("unauthorized")?;
        let token = random().map_err(|_| "unavailable")?;
        r.ticket = None;
        r.session = Some(hash(token.as_bytes()));
        r.status.state = RunState::Active;
        // Cold loading/parsing the trusted loader starts no guest. Bound that
        // phase by the ticket window; the first claim starts the short lease.
        r.lease = Some(
            now + Duration::from_millis(TICKET_MS).min(r.until.saturating_duration_since(now)),
        );
        Ok((
            r.status.id.clone(),
            token,
            r.until.saturating_duration_since(now).as_secs().max(1),
        ))
    }
    pub(super) fn browser_run<'a>(s: &'a mut State, id: &str, cookie: &str) -> Api<&'a mut Run> {
        let r = s.runs.get_mut(id).ok_or("not_found")?;
        if r.status.state != RunState::Active {
            return Err("ended");
        }
        if !digest(cookie)
            || !r
                .session
                .as_ref()
                .is_some_and(|h| matches_secret(cookie, h))
        {
            return Err("unauthorized");
        }
        Ok(r)
    }
    pub fn claim(&self, id: &str, cookie: &str) -> Api<(String, u64)> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = Self::browser_run(&mut s, id, cookie)?;
        if r.client.is_some() {
            return Err("already_claimed");
        }
        let token = random().map_err(|_| "unavailable")?;
        r.client = Some(hash(token.as_bytes()));
        let ms = LEASE_MS.min(
            r.until
                .saturating_duration_since(Instant::now())
                .as_millis() as u64,
        );
        r.lease = Some(Instant::now() + Duration::from_millis(ms));
        Ok((token, ms))
    }
    pub fn lease(
        &self,
        id: &str,
        cookie: &str,
        client: &str,
        report: Option<GuestState>,
    ) -> Api<u64> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let r = Self::browser_run(&mut s, id, cookie)?;
        if !digest(client) || !r.client.as_ref().is_some_and(|h| matches_secret(client, h)) {
            return Err("unauthorized");
        }
        let ms = LEASE_MS.min(
            r.until
                .saturating_duration_since(Instant::now())
                .as_millis() as u64,
        );
        r.lease = Some(Instant::now() + Duration::from_millis(ms));
        if let Some(state) = report {
            r.status.guest_state = state;
            if matches!(state, GuestState::Failed | GuestState::Closed) {
                Self::end(
                    r,
                    if state == GuestState::Failed {
                        RunState::Failed
                    } else {
                        RunState::Stopped
                    },
                    if state == GuestState::Failed {
                        Some("guest_failed")
                    } else {
                        None
                    },
                );
            }
        }
        Ok(ms)
    }
    pub fn file(&self, id: &str, cookie: &str, path: &str) -> Api<Bytes> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let sha = Self::browser_run(&mut s, id, cookie)?
            .status
            .bundle_sha256
            .clone();
        s.catalogs
            .get(&sha)
            .and_then(|c| c.files.get(path))
            .cloned()
            .ok_or("not_found")
    }
}
