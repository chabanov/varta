use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{BufRead, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
const TOKEN: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
struct Host {
    child: Child,
    address: SocketAddr,
    pin: String,
    root: PathBuf,
    forced: bool,
}
impl Host {
    fn start() -> Self {
        let root = temp_project();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("ceiling.json"),serde_json::to_vec(&json!({"schemaVersion":1,"listen":"127.0.0.1:0","maxRuns":2,"maxImports":4,"maxRunSeconds":300,"importSeconds":60,"retentionSeconds":300,"startupDeadlineMs":15000,"validationDeadlineMs":15000,"cpuSeconds":30,"addressSpaceBytes":8589934592u64,"budgets":{}})).unwrap()).unwrap();
        let mut child = varta()
            .args(["execution-host", "--ceiling"])
            .arg(root.join("ceiling.json"))
            .env("VARTA_EXECUTION_TOKEN", TOKEN)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let out = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for l in std::io::BufReader::new(out).lines() {
                if tx.send(l.unwrap()).is_err() {
                    break;
                }
            }
        });
        let line = rx
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|e| {
                let _ = child.kill();
                let _ = child.wait();
                panic!("execution readiness: {e}")
            });
        let (url, pin) = line
            .strip_prefix("varta execution: ")
            .unwrap()
            .split_once(" host=")
            .unwrap();
        let address = url.strip_prefix("http://").unwrap().parse().unwrap();
        Self {
            child,
            address,
            pin: pin.into(),
            root,
            forced: false,
        }
    }
    fn raw(&self, method: &str, path: &str, b: &[u8], headers: &str) -> (u16, Vec<u8>) {
        raw(self.address, method, path, b, headers)
    }
    fn api(&self, method: &str, path: &str, value: &Value, key: Option<&str>) -> (u16, Value) {
        let mut headers =
            format!("Authorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\n");
        if let Some(k) = key {
            headers.push_str(&format!("Idempotency-Key: {k}\r\n"));
        }
        let (code, b) = self.raw(method, path, &serde_json::to_vec(value).unwrap(), &headers);
        (code, serde_json::from_slice(&b).unwrap())
    }
    fn upload(&self, files: &BTreeMap<String, Vec<u8>>, key: &str) -> (String, String) {
        let mut h = Sha256::new();
        h.update(b"varta-toolchain-artifact-v1\0");
        let descriptions: Vec<_> = files
            .iter()
            .map(|(path, b)| {
                h.update((path.len() as u64).to_le_bytes());
                h.update(path.as_bytes());
                h.update((b.len() as u64).to_le_bytes());
                h.update(b);
                json!({"path":path,"sha256":format!("{:x}",Sha256::digest(b)),"bytes":b.len()})
            })
            .collect();
        let sha = format!("{:x}", h.finalize());
        let(code,status)=self.api("POST","/v1/artifacts",&json!({"schemaVersion":1,"hostSha256":self.pin,"artifactSha256":sha,"files":descriptions}),Some(key));
        assert_eq!(code, 202, "{status}");
        let id = status["id"].as_str().unwrap().to_owned();
        for (file, (_, b)) in files.iter().enumerate() {
            for (chunk, data) in b.chunks(8192).enumerate() {
                let path = format!("/v1/artifacts/{id}/files/{file}/chunks/{chunk}");
                let headers = format!(
                    "Authorization: Bearer {TOKEN}\r\nContent-Type: application/octet-stream\r\n"
                );
                assert_eq!(self.raw("PUT", &path, data, &headers).0, 204);
                if chunk == 0 {
                    assert_eq!(self.raw("PUT", &path, data, &headers).0, 204);
                }
            }
        }
        (id, sha)
    }
    fn seal(&self, id: &str) -> Value {
        let (code, v) = self.api(
            "POST",
            &format!("/v1/artifacts/{id}/seal"),
            &json!({}),
            None,
        );
        assert_eq!(code, 202, "{v}");
        self.wait(&format!("/v1/artifacts/{id}"), |v| v["state"] != "sealing")
    }
    fn wait(&self, path: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let (code, v) = self.api("GET", path, &json!({}), None);
            assert_eq!(code, 200, "{v}");
            if predicate(&v) {
                return v;
            }
            assert!(Instant::now() < until, "native lifecycle stalled: {v}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn start_run(&self, sha: &str, key: &str, ttl: u64) -> (u16, Value) {
        self.api("POST","/v1/runs",&json!({"schemaVersion":1,"hostSha256":self.pin,"compilerSha256":self.pin,"artifactSha256":sha,"target":"native-http","policyId":"loopback-http-v1","ttlSeconds":ttl}),Some(key))
    }
    fn kill_controller(&mut self) {
        self.forced = true;
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
    fn private_snapshots(&self) -> Vec<PathBuf> {
        let prefix = format!("varta-toolchain-{}-", self.child.id());
        fs::read_dir(std::env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .map(|e| e.path())
            .collect()
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status();
            let until = Instant::now() + Duration::from_secs(10);
            while self.child.try_wait().unwrap().is_none() {
                if Instant::now() > until {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let snapshots = self.private_snapshots();
        if !self.forced && !std::thread::panicking() {
            assert!(
                snapshots.is_empty(),
                "controlled shutdown left a private worker snapshot"
            );
            assert!(self.child.try_wait().unwrap().unwrap().success());
        }
        for p in snapshots {
            let _ = fs::remove_dir_all(p);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn raw(address: SocketAddr, method: &str, path: &str, b: &[u8], headers: &str) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s,"{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n",b.len()).unwrap();
    s.write_all(b).unwrap();
    let mut r = std::io::BufReader::new(s);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        assert!(!line.is_empty());
        if line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    let code = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length: "))
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(0);
    let mut b = vec![0; len];
    r.read_exact(&mut b).unwrap();
    (code, b)
}
fn files(trap: bool) -> BTreeMap<String, Vec<u8>> {
    let source = if trap {
        r#"function handle(r:VartaHttpRequest):VartaHttpResponse{return {status:200,body:"ok",headers:[]};}varta.http.serve({hostname:"127.0.0.1",port:0},handle);export function main():void{throw new Error("expected startup trap");}"#
    } else {
        r#"let count=0;function handle(r:VartaHttpRequest):VartaHttpResponse{if(r.target==="/trap"){throw new Error("request trap");}if(r.target==="/exit"){varta.http.stop();}count++;return {status:200,body:"ok"+String(count),headers:[]};}varta.http.serve({hostname:"127.0.0.1",port:0},handle);export function main():void{console.log("event: listening forged by guest");}"#
    };
    let p = RegressionProject::new(source, json!({"net":{"listen":["127.0.0.1:0"]}}));
    p.configure("languageProfile", json!("checked-f64-utf16-v40"));
    let out = p.command(&["build"]);
    assert!(out.status.success(), "{}", combined(&out));
    let artifact = varta_project::Artifact::read(&p.0.join("dist/app.wasm")).unwrap();
    BTreeMap::from([
        (
            "app.manifest.json".into(),
            artifact.manifest.to_bytes().unwrap(),
        ),
        ("app.wasm".into(), artifact.wasm),
    ])
}
#[test]
fn execution_cli_native_http_readiness_trap_isolation_terminal_replay_and_reap() {
    let h = Host::start();
    assert_eq!(h.raw("GET", "/v1/info", b"", "").0, 401);
    assert_eq!(
        h.raw(
            "GET",
            "/v1/info",
            b"",
            &format!("Authorization: Bearer {TOKEN}\r\nOrigin: http://untrusted.test\r\n")
        )
        .0,
        403
    );
    assert_eq!(
        h.start_run(&"f".repeat(64), "native-missing-0001", 180).0,
        404
    );
    let (id, sha) = h.upload(&files(false), "native-good-import-01");
    assert_eq!(h.seal(&id)["state"], "ready");
    let (code, run) = h.start_run(&sha, "native-good-start-01", 180);
    assert_eq!(code, 202, "{run}");
    let run_id = run["id"].as_str().unwrap();
    let path = format!("/v1/runs/{run_id}");
    let ready = h.wait(&path, |v| v["state"] != "starting");
    assert_eq!(ready["state"], "ready", "{ready}");
    let address: SocketAddr = ready["address"].as_str().unwrap().parse().unwrap();
    assert!(address.ip().is_loopback());
    assert_eq!(raw(address, "GET", "/", b"", "").1, b"ok1");
    assert_eq!(raw(address, "GET", "/trap", b"", "").0, 500);
    assert_eq!(raw(address, "GET", "/", b"", "").1, b"ok1");
    let (code, stopping) = h.api("POST", &format!("{path}/stop"), &json!({}), None);
    assert_eq!(code, 200);
    assert!(matches!(
        stopping["state"].as_str(),
        Some("stopping" | "stopped")
    ));
    let stopped = h.wait(&path, |v| v["state"] != "stopping");
    assert_eq!(stopped["state"], "stopped");
    assert!(stopped["address"].is_null());
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    assert_eq!(
        h.start_run(&sha, "native-good-start-01", 180).1["state"],
        "stopped"
    );
    assert_eq!(
        h.api(
            "POST",
            "/v1/runs/cancel-by-key",
            &json!({"schemaVersion":1,"requestId":"native-cancel-first-01"}),
            None
        )
        .0,
        200
    );
    assert_eq!(h.start_run(&sha, "native-cancel-first-01", 180).0, 409);
    let (_, again) = h.start_run(&sha, "native-natural-exit-01", 180);
    let path = format!("/v1/runs/{}", again["id"].as_str().unwrap());
    let ready = h.wait(&path, |v| v["state"] != "starting");
    assert_eq!(ready["state"], "ready", "{ready}");
    let address = ready["address"].as_str().unwrap().parse().unwrap();
    assert_eq!(raw(address, "GET", "/exit", b"", "").0, 200);
    assert_eq!(h.wait(&path, |v| v["state"] != "ready")["state"], "exited");
}
#[test]
fn execution_cli_expiry_startup_failure_and_parent_loss_end_every_process() {
    let mut h = Host::start();
    let (id, sha) = h.upload(&files(true), "native-trap-import-01");
    assert_eq!(h.seal(&id)["state"], "ready");
    let (_, run) = h.start_run(&sha, "native-trap-start-001", 180);
    let failed = h.wait(&format!("/v1/runs/{}", run["id"].as_str().unwrap()), |v| {
        v["state"] != "starting"
    });
    assert_eq!(failed["state"], "failed");
    assert!(failed["address"].is_null());
    let (id, sha) = h.upload(&files(false), "native-ttl-import-001");
    assert_eq!(h.seal(&id)["state"], "ready");
    let (_, run) = h.start_run(&sha, "native-ttl-start-0001", 1);
    let expired = h.wait(&format!("/v1/runs/{}", run["id"].as_str().unwrap()), |v| {
        matches!(v["state"].as_str(), Some("expired" | "failed"))
    });
    assert_eq!(expired["state"], "expired", "{expired}");
    let (_, run) = h.start_run(&sha, "native-parent-loss-01", 180);
    let ready = h.wait(&format!("/v1/runs/{}", run["id"].as_str().unwrap()), |v| {
        v["state"] != "starting"
    });
    assert_eq!(ready["state"], "ready", "{ready}");
    let address = ready["address"].as_str().unwrap().parse().unwrap();
    h.kill_controller();
    let until = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
        assert!(Instant::now() < until, "worker outlived controller");
        std::thread::sleep(Duration::from_millis(20));
    }
}
