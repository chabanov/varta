use super::*;

impl Registry {
    pub(super) async fn validate_process(
        &self,
        request: ImportRequest,
        chunks: Vec<Vec<Option<Bytes>>>,
        mut cancel: watch::Receiver<bool>,
        deadline: tokio::time::Instant,
    ) -> Api<BTreeMap<String, Vec<u8>>> {
        let _slot = tokio::select! {biased;_ = cancel.changed()=>return Err("cancelled"),_=tokio::time::sleep_until(deadline)=>return Err("deadline"),p=self.workers.clone().acquire_owned()=>p.map_err(|_|"unavailable")?};
        if *cancel.borrow() {
            return Err("cancelled");
        }
        let files = request
            .files
            .iter()
            .zip(chunks)
            .map(|(f, chunks)| {
                let b: Vec<u8> = chunks
                    .into_iter()
                    .flatten()
                    .flat_map(|b| b.to_vec())
                    .collect();
                (
                    f.path.clone(),
                    base64::engine::general_purpose::STANDARD.encode(b),
                )
            })
            .collect();
        let input = serde_json::to_vec(&ValidationInput {
            application_origin: self.application.as_ref().map(|_| self.origin.clone()),
            request,
            files,
            budgets: self.ceiling.budgets,
        })
        .map_err(|_| "invalid_bundle")?;
        if input.len() > MAX_WORKER_INPUT {
            return Err("invalid_bundle");
        }
        let mut child = tokio::process::Command::new(&self.executable)
            .args([
                if self.application.is_some() {
                    "application-worker"
                } else {
                    "preview-worker"
                },
                "--address-space-bytes",
                &self.ceiling.address_space_bytes.to_string(),
                "--cpu-seconds",
                &self.ceiling.cpu_seconds.to_string(),
            ])
            .env_clear()
            .env("MallocNanoZone", "0")
            .current_dir(self.executable.parent().ok_or("unavailable")?)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
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
                let mut output = Vec::new();
                stdout
                    .take((MAX_WORKER_OUTPUT + 1) as u64)
                    .read_to_end(&mut output)
                    .await
                    .map_err(|_| "worker_failed")?;
                if output.len() > MAX_WORKER_OUTPUT {
                    return Err("resource_limit");
                }
                let status = child.wait().await.map_err(|_| "worker_failed")?;
                drop(stdin);
                if !status.success() {
                    return Err("invalid_bundle");
                }
                let decoded: ValidationOutput =
                    serde_json::from_slice(&output).map_err(|_| "worker_failed")?;
                if decoded.files.len() > MAX_FILES {
                    return Err("resource_limit");
                }
                let mut files = BTreeMap::new();
                let mut total = 0;
                for (path, encoded) in decoded.files {
                    let b = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_| "worker_failed")?;
                    total += b.len();
                    if !path_valid(&path) || total > MAX_BYTES + MAX_JSON_BYTES {
                        return Err("resource_limit");
                    }
                    files.insert(path, b);
                }
                Ok(files)
            };
            tokio::pin!(run);
            tokio::select! {_=cancel.changed()=>Err("cancelled"),_=tokio::time::sleep_until(deadline)=>Err("deadline"),r=&mut run=>r}
        };
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result
    }
}
