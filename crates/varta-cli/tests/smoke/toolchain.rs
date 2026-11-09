use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

// Synthetic local credential only; never a provider or operator credential.
const TOKEN: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
struct Host {
    child: Child,
    address: SocketAddr,
    compiler: String,
    root: PathBuf,
    lines: mpsc::Receiver<String>,
}
impl Host {
    fn start(deadline: u64) -> Self {
        let root = temp_project();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("ceiling.json"),serde_json::to_vec(&json!({"schemaVersion":1,"listen":"127.0.0.1:0","operations":["check","build"],"profiles":["checked-f64-utf16-v40"],"maxJobs":3,"maxWorkers":1,"deadlineMs":deadline,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"retentionSeconds":300})).unwrap()).unwrap();
        let mut child = varta()
            .args(["toolchain", "--ceiling"])
            .arg(root.join("ceiling.json"))
            .env("VARTA_TOOLCHAIN_TOKEN", TOKEN)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        // The debug executable must be copied and hashed before listening.
        // Workspace acceptance can start several pinned hosts concurrently.
        let line = lines
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|e| {
                let _ = child.kill();
                let _ = child.wait();
                panic!("toolchain did not start: {e}")
            });
        let (url, compiler) = line
            .strip_prefix("varta toolchain: ")
            .unwrap()
            .split_once(" compiler=")
            .unwrap();
        let address = url.strip_prefix("http://").unwrap().parse().unwrap();
        Self {
            child,
            address,
            compiler: compiler.into(),
            root,
            lines,
        }
    }
    fn raw(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        extra: &str,
        auth: bool,
    ) -> (u16, Vec<u8>, String) {
        let mut stream = TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(stream,"{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}",self.address,body.len()).unwrap();
        if auth {
            write!(stream, "Authorization: Bearer {TOKEN}\r\n").unwrap();
        }
        write!(stream, "Connection: close\r\n\r\n").unwrap();
        stream.write_all(body).unwrap();
        let mut bytes = Vec::new();
        stream
            .take((34 * 1024 * 1024) as u64)
            .read_to_end(&mut bytes)
            .unwrap();
        let split = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
        let header = String::from_utf8(bytes[..split].to_vec()).unwrap();
        let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, bytes[split + 4..].to_vec(), header)
    }
    fn request(&self, operation: &str, source: &str) -> Value {
        json!({"schemaVersion":1,"compilerSha256":self.compiler,"operation":operation,"target":"browser","languageProfile":"checked-f64-utf16-v40","dependencyPolicy":"local-only-v1","entry":"main.ts","files":[{"path":"main.ts","content":source}]})
    }
    fn submit(&self, request: &Value, key: &str) -> (u16, Value) {
        let (status, body, _) = self.raw(
            "POST",
            "/v1/jobs",
            &serde_json::to_vec(request).unwrap(),
            &format!("Idempotency-Key: {key}\r\n"),
            true,
        );
        (status, serde_json::from_slice(&body).unwrap())
    }
    fn json(&self, method: &str, path: &str) -> Value {
        let (status, body, _) = self.raw(method, path, &[], "", true);
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        serde_json::from_slice(&body).unwrap()
    }
    fn finish(&self, id: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let value = self.json("GET", &format!("/v1/jobs/{id}"));
            if !matches!(value["state"].as_str().unwrap(), "queued" | "running") {
                return value;
            }
            assert!(Instant::now() < deadline, "operation stalled");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn children(&self) -> Vec<u32> {
        let output = Command::new("ps")
            .args(["-axo", "ppid=,pid="])
            .output()
            .unwrap();
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| {
                let mut numbers = line.split_whitespace();
                let parent = numbers.next()?.parse::<u32>().ok()?;
                let id = numbers.next()?.parse().ok()?;
                (parent == self.child.id()).then_some(id)
            })
            .collect()
    }
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_some() {
            return;
        }
        assert!(
            Command::new("kill")
                .args(["-INT", &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "{status}");
                break;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("shutdown did not reap workers");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let prefix = format!("varta-toolchain-{}-", self.child.id());
        assert!(
            !fs::read_dir(std::env::temp_dir()).unwrap().any(|p| p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&prefix)),
            "pinned executable leaked"
        );
        assert!(
            self.lines.try_iter().next().is_none(),
            "guest main wrote output"
        );
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.stop();
        } else {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn toolchain_cli_checks_builds_diagnostics_idempotency_and_closed_artifact_downloads() {
    let host = Host::start(15000);
    assert_eq!(host.raw("GET", "/v1/info", &[], "", false).0, 401);
    assert_eq!(
        host.raw(
            "GET",
            "/v1/info",
            &[],
            "Origin: https://untrusted.test\r\n",
            true
        )
        .0,
        403
    );
    assert_eq!(
        host.json("GET", "/v1/info")["compilerSha256"],
        host.compiler
    );
    let request=host.request("build","export function main():void{console.log('MUST_NOT_RUN');throw new Error('must not execute');}");
    let (status, accepted) = host.submit(&request, "accepted-response-recovery");
    assert_eq!(status, 202);
    let id = accepted["id"].as_str().unwrap();
    assert_eq!(
        host.submit(&request, "accepted-response-recovery").1["id"],
        id
    );
    let mut changed = request.clone();
    changed["entry"] = json!("other.ts");
    changed["files"][0]["path"] = json!("other.ts");
    assert_eq!(host.submit(&changed, "accepted-response-recovery").0, 409);
    let mut denied = request.clone();
    denied["languageProfile"] = json!("checked-f64-utf16-v39");
    assert_eq!(host.submit(&denied, "profile-denied-correlation").0, 403);
    denied = request.clone();
    denied["compilerSha256"] = json!("b".repeat(64));
    assert_eq!(host.submit(&denied, "compiler-mismatch-correlation").0, 409);
    denied = request.clone();
    denied["capabilities"] = json!({"net":{"request":["*"]}});
    assert_eq!(
        host.submit(&denied, "source-config-denied-correlation").0,
        400
    );
    let completed = host.finish(id);
    assert_eq!(completed["state"], "succeeded", "{completed}");
    let artifact = completed["artifact"]["sha256"].as_str().unwrap();
    assert_eq!(completed["result"]["artifactSha256"], artifact);
    let download = host.root.join("download");
    fs::create_dir(&download).unwrap();
    let mut digest = Sha256::new();
    digest.update(b"varta-toolchain-artifact-v1\0");
    for file in completed["artifact"]["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        let (status, bytes, headers) = host.raw(
            "GET",
            &format!("/v1/artifacts/{artifact}/{path}"),
            &[],
            "",
            true,
        );
        assert_eq!(status, 200);
        assert!(headers.contains("application/octet-stream"));
        assert!(headers.contains("attachment"));
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            file["sha256"].as_str().unwrap()
        );
        assert_eq!(bytes.len() as u64, file["bytes"].as_u64().unwrap());
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
        fs::write(download.join(path), bytes).unwrap();
    }
    assert_eq!(format!("{:x}", digest.finalize()), artifact);
    let output = varta()
        .arg("validate")
        .arg(download.join("app.wasm"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    host.json("DELETE", &format!("/v1/jobs/{id}"));
    let replay = host.submit(&request, "accepted-response-recovery").1;
    assert_eq!(replay["id"], id);
    assert_eq!(replay["forgotten"], true);
    assert_eq!(
        host.raw(
            "GET",
            &format!("/v1/artifacts/{artifact}/app.wasm"),
            &[],
            "",
            true
        )
        .0,
        404
    );
    let check = host.request("check", "export function main():void{}");
    let (status, job) = host.submit(&check, "check-only-correlation");
    assert_eq!(status, 202);
    let job = host.finish(job["id"].as_str().unwrap());
    assert_eq!(job["state"], "succeeded");
    assert_eq!(job["result"]["artifactSha256"], "");
    assert!(job["artifact"].is_null());
    let bad = host.request(
        "check",
        "export function main():void{const імʼя:number='текст';}",
    );
    let (status, job) = host.submit(&bad, "diagnostic-correlation");
    assert_eq!(status, 202);
    let job = host.finish(job["id"].as_str().unwrap());
    assert_eq!(job["state"], "failed");
    assert_eq!(job["result"]["diagnostics"][0]["path"], "main.ts");
    assert_eq!(job["result"]["diagnostics"][0]["line"], 1);
}

#[test]
fn toolchain_cli_worker_stops_when_its_parent_lifetime_channel_closes() {
    let request = json!({"schemaVersion":1,"compilerSha256":"a".repeat(64),"operation":"build","target":"browser","languageProfile":"checked-f64-utf16-v40","dependencyPolicy":"local-only-v1","entry":"main.ts","files":[{"path":"main.ts","content":heavy()}]});
    let bytes = serde_json::to_vec(&request).unwrap();
    let mut child = varta()
        .args([
            "toolchain-worker",
            "--address-space-bytes",
            "4294967296",
            "--cpu-seconds",
            "10",
        ])
        .env_clear()
        .env("MallocNanoZone", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(&(bytes.len() as u64).to_le_bytes())
        .unwrap();
    stdin.write_all(&bytes).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert!(child.try_wait().unwrap().is_none());
    // Closing this pipe also happens when the service dies without Drop.
    drop(stdin);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("worker survived parent channel closure");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn heavy() -> String {
    format!(
        "export function main():void{{let x=0;{}console.log(x);}}",
        "x+=1;".repeat(50000)
    )
}
#[test]
fn toolchain_cli_hard_cancels_queued_and_running_work_recovers_and_reaps_on_shutdown() {
    let mut host = Host::start(15000);
    let heavy = host.request("build", &heavy());
    let (status, job) = host.submit(&heavy, "running-cancel-correlation");
    assert_eq!(status, 202);
    let id = job["id"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.children().is_empty() {
        assert!(Instant::now() < deadline, "compiler child never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    let (status, queued) = host.submit(&heavy, "queued-cancel-correlation");
    assert_eq!(status, 202);
    assert_eq!(queued["state"], "queued");
    let queued_id = queued["id"].as_str().unwrap();
    assert_eq!(
        host.json("POST", &format!("/v1/jobs/{queued_id}/cancel"))["state"],
        "cancelled"
    );
    assert_eq!(
        host.json("POST", &format!("/v1/jobs/{id}/cancel"))["state"],
        "cancelled"
    );
    while !host.children().is_empty() {
        assert!(Instant::now() < deadline, "cancel did not reap child");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        host.submit(&heavy, "running-cancel-correlation").1["state"],
        "cancelled"
    );
    let (status, recovered) = host.submit(
        &host.request("check", "export function main():void{}"),
        "recovered-correlation",
    );
    assert_eq!(status, 202);
    assert_eq!(
        host.finish(recovered["id"].as_str().unwrap())["state"],
        "succeeded"
    );
    let (status, _) = host.submit(&heavy, "shutdown-correlation");
    assert_eq!(status, 202);
    while host.children().is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let children = host.children();
    host.stop();
    for child in children {
        assert!(
            !Command::new("kill")
                .args(["-0", &child.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success(),
            "compiler orphaned"
        );
    }
    let deadline_host = Host::start(50);
    let (status, job) = deadline_host.submit(
        &deadline_host.request("build", &crate::toolchain::heavy()),
        "deadline-correlation",
    );
    assert_eq!(status, 202);
    let failed = deadline_host.finish(job["id"].as_str().unwrap());
    assert_eq!(failed["error"], "deadline");
}
