use super::*;
impl Registry {
    pub fn prepare(&self, request: ImportRequest, key: &str) -> Api<ImportStatus> {
        if !key_valid(key) || !import_valid(&request) {
            return Err("invalid_request");
        }
        if request.host_sha256 != self.host {
            return Err("host_mismatch");
        }
        let request_sha = identity(&request).map_err(|_| "invalid_request")?;
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        if let Some(i) = s.imports.values().find(|i| i.key == key) {
            return if i.status.request_sha256 == request_sha {
                Ok(i.status.clone())
            } else {
                Err("idempotency_conflict")
            };
        }
        if self.reap_failed.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        if Self::receipts(&s) >= MAX_RECEIPTS
            || s.imports
                .values()
                .filter(|i| {
                    matches!(
                        i.status.state,
                        ImportState::Uploading | ImportState::Sealing
                    )
                })
                .count()
                >= self.ceiling.max_imports
        {
            return Err("busy");
        }
        let id = random().map_err(|_| "unavailable")?;
        let status = ImportStatus {
            id: id.clone(),
            host_sha256: self.host.clone(),
            request_sha256: request_sha,
            artifact_sha256: request.artifact_sha256.clone(),
            state: ImportState::Uploading,
            error: None,
            expires_at_ms: now_ms() + self.ceiling.import_seconds * 1000,
        };
        let chunks = request
            .files
            .iter()
            .map(|f| vec![None; f.bytes.div_ceil(CHUNK_BYTES)])
            .collect();
        let (cancel, _) = watch::channel(false);
        s.imports.insert(
            id,
            Import {
                key: key.into(),
                request,
                status: status.clone(),
                chunks,
                until: Instant::now() + Duration::from_secs(self.ceiling.import_seconds),
                terminal: None,
                cancel,
            },
        );
        Ok(status)
    }
    pub fn import_status(&self, id: &str) -> Option<ImportStatus> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        s.imports.get(id).map(|i| i.status.clone())
    }
    pub fn chunk(&self, id: &str, file: usize, chunk: usize, b: Vec<u8>) -> Api<()> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        if let Some(old) = s
            .imports
            .get(id)
            .and_then(|i| i.chunks.get(file))
            .and_then(|c| c.get(chunk))
            .and_then(Option::as_ref)
        {
            return if *old == b {
                Ok(())
            } else {
                Err("chunk_conflict")
            };
        }
        if Self::retained(&s) + b.len() > MAX_RETAINED {
            return Err("busy");
        }
        let i = s.imports.get_mut(id).ok_or("not_found")?;
        if i.status.state != ImportState::Uploading {
            return Err("not_uploading");
        }
        let f = i.request.files.get(file).ok_or("invalid_chunk")?;
        let at = chunk
            .checked_mul(CHUNK_BYTES)
            .filter(|n| *n < f.bytes)
            .ok_or("invalid_chunk")?;
        if b.len() != (f.bytes - at).min(CHUNK_BYTES) {
            return Err("invalid_chunk");
        }
        *i.chunks
            .get_mut(file)
            .and_then(|c| c.get_mut(chunk))
            .ok_or("invalid_chunk")? = Some(b);
        Ok(())
    }
    pub fn cancel_import(&self, id: &str) -> Api<ImportStatus> {
        let mut s = lock(&self.state);
        self.expire_locked(&mut s);
        let i = s.imports.get_mut(id).ok_or("not_found")?;
        if i.status.state == ImportState::Ready {
            return Err("already_ready");
        }
        if matches!(
            i.status.state,
            ImportState::Uploading | ImportState::Sealing
        ) {
            i.status.state = ImportState::Cancelled;
            i.status.error = Some("cancelled");
            i.terminal = Some(Instant::now());
            i.chunks.clear();
            i.cancel.send_replace(true);
        }
        Ok(i.status.clone())
    }
    pub fn seal(self: &Arc<Self>, id: &str) -> Api<ImportStatus> {
        let (request, files, mut cancel, until, status) = {
            let mut s = lock(&self.state);
            self.expire_locked(&mut s);
            let i = s.imports.get_mut(id).ok_or("not_found")?;
            if i.status.state != ImportState::Uploading {
                return Ok(i.status.clone());
            }
            if i.chunks.iter().flatten().any(Option::is_none) {
                return Err("missing_chunks");
            }
            i.status.state = ImportState::Sealing;
            let files = i
                .request
                .files
                .iter()
                .zip(&i.chunks)
                .map(|(f, chunks)| {
                    (
                        f.path.clone(),
                        chunks
                            .iter()
                            .flatten()
                            .flat_map(|b| b.iter().copied())
                            .collect(),
                    )
                })
                .collect::<BTreeMap<String, Vec<u8>>>();
            (
                i.request.clone(),
                files,
                i.cancel.subscribe(),
                i.until.min(
                    Instant::now() + Duration::from_millis(self.ceiling.validation_deadline_ms),
                ),
                i.status.clone(),
            )
        };
        let this = Arc::clone(self);
        let id = id.to_owned();
        lock(&self.tasks).retain(|t| !t.is_finished());
        let task = tokio::spawn(async move {
            let deadline = tokio::time::Instant::from_std(until);
            let result = async {
                let slot = tokio::select! {
                    biased;
                    _=cancel.changed()=>return Err("cancelled"),
                    _=tokio::time::sleep_until(deadline)=>return Err("deadline"),
                    p=this.validators.clone().acquire_owned()=>p.map_err(|_|"unavailable")?,
                };
                if *cancel.borrow() {
                    return Err("cancelled");
                }
                let result = process::validate(
                    &this.executable,
                    &this.ceiling,
                    request.clone(),
                    &files,
                    cancel.clone(),
                    deadline,
                )
                .await;
                if result == Err("reap_failed") {
                    // Preserve the process slot when OS termination was not confirmed.
                    slot.forget();
                    this.reap_failed.store(true, Ordering::Release);
                }
                result
            }
            .await;
            let mut s = lock(&this.state);
            this.expire_locked(&mut s);
            if s.imports
                .get(&id)
                .is_none_or(|i| i.status.state != ImportState::Sealing)
            {
                return;
            }
            let result = result.and_then(|()| {
                let bytes = files.values().map(Vec::len).sum();
                if !s.catalogs.contains_key(&request.artifact_sha256)
                    && Self::retained(&s) + bytes > MAX_RETAINED
                {
                    return Err("busy");
                }
                s.catalogs
                    .entry(request.artifact_sha256.clone())
                    .or_insert_with(|| {
                        Arc::new(Catalog {
                            request,
                            files,
                            bytes,
                        })
                    });
                Ok(())
            });
            if let Some(i) = s.imports.get_mut(&id) {
                i.chunks.clear();
                match result {
                    Ok(()) => i.status.state = ImportState::Ready,
                    Err(e) => {
                        i.status.state = ImportState::Failed;
                        i.status.error = Some(e);
                        i.terminal = Some(Instant::now());
                    }
                }
            }
        });
        lock(&self.tasks).push(task);
        Ok(status)
    }
}
