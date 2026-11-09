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
const TOKEN: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
struct Host {
    child: Child,
    control: SocketAddr,
    public: SocketAddr,
    host: String,
    root: PathBuf,
}
impl Host {
    fn start() -> Self {
        let root = temp_project();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("ceiling.json"),serde_json::to_vec(&json!({"schemaVersion":1,"controlListen":"127.0.0.1:0","publicListen":"[::1]:0","protectedOrigins":["http://127.0.0.1:8787"],"maxImports":4,"maxRuns":4,"maxRunSeconds":300,"importSeconds":60,"retentionSeconds":300,"validationDeadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"budgets":{}})).unwrap()).unwrap();
        let mut child = varta()
            .args(["preview-host", "--ceiling"])
            .arg(root.join("ceiling.json"))
            .env("VARTA_PREVIEW_TOKEN", TOKEN)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        // Pinning hashes/copies the large debug executable before listening.
        // Allow contention with the workspace's other native acceptance tests;
        // this does not change any admitted guest/validation deadline.
        let line = rx
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|e| {
                let _ = child.kill();
                let _ = child.wait();
                panic!("preview readiness: {e}")
            });
        let mut fields = line
            .strip_prefix("varta preview: http://")
            .unwrap()
            .split_whitespace();
        let control = fields.next().unwrap().parse().unwrap();
        let public = fields
            .next()
            .unwrap()
            .strip_prefix("public=http://")
            .unwrap()
            .parse()
            .unwrap();
        let host = fields
            .next()
            .unwrap()
            .strip_prefix("host=")
            .unwrap()
            .to_owned();
        Self {
            child,
            control,
            public,
            host,
            root,
        }
    }
    fn raw(
        &self,
        public: bool,
        method: &str,
        path: &str,
        body: &[u8],
        headers: &str,
    ) -> (u16, Vec<u8>, String) {
        let address = if public { self.public } else { self.control };
        let mut s = TcpStream::connect(address).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        s.set_write_timeout(Some(Duration::from_secs(20))).unwrap();
        write!(s,"{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n",body.len()).unwrap();
        s.write_all(body).unwrap();
        let mut reader = std::io::BufReader::new(s);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(!line.is_empty());
            if line == "\r\n" {
                break;
            }
            head.push_str(&line);
        }
        let code = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        let length = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .map(|n| n.parse::<usize>().unwrap())
            .unwrap_or(0);
        let mut b = vec![0u8; length];
        reader.read_exact(&mut b).unwrap();
        (code, b, head)
    }
    fn control(&self, method: &str, path: &str, value: &Value, key: Option<&str>) -> (u16, Value) {
        let mut headers =
            format!("Authorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\n");
        if let Some(k) = key {
            headers.push_str(&format!("Idempotency-Key: {k}\r\n"));
        }
        let (code, b, _) = self.raw(
            false,
            method,
            path,
            &serde_json::to_vec(value).unwrap(),
            &headers,
        );
        (code, serde_json::from_slice(&b).unwrap())
    }
    fn upload(&self, files: &BTreeMap<String, Vec<u8>>, key: &str) -> (String, String) {
        let mut h = Sha256::new();
        h.update(b"varta-toolchain-artifact-v1\0");
        let description: Vec<_> = files
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
        let req = json!({"schemaVersion":1,"hostSha256":self.host,"artifactSha256":sha,"files":description});
        let (code, status) = self.control("POST", "/v1/bundles", &req, Some(key));
        assert_eq!(code, 202, "{status}");
        let id = status["id"].as_str().unwrap().to_owned();
        assert_eq!(
            self.control("POST", "/v1/bundles", &req, Some(key)).1["id"],
            id
        );
        for (index, (_, b)) in files.iter().enumerate() {
            for (chunk, data) in b.chunks(8192).enumerate() {
                let path = format!("/v1/bundles/{id}/files/{index}/chunks/{chunk}");
                let headers = format!(
                    "Authorization: Bearer {TOKEN}\r\nContent-Type: application/octet-stream\r\n"
                );
                assert_eq!(self.raw(false, "PUT", &path, data, &headers).0, 204);
                if chunk == 0 {
                    assert_eq!(self.raw(false, "PUT", &path, data, &headers).0, 204);
                }
            }
        }
        (id, sha)
    }
    fn sealed(&self, id: &str) -> Value {
        assert_eq!(
            self.control("POST", &format!("/v1/bundles/{id}/seal"), &json!({}), None)
                .0,
            202
        );
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let (code, v) = self.control("GET", &format!("/v1/bundles/{id}"), &json!({}), None);
            assert_eq!(code, 200);
            if v["state"] != "sealing" {
                return v;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn start_run(&self, sha: &str, key: &str, seconds: u64) -> (u16, Value) {
        self.control("POST","/v1/runs",&json!({"schemaVersion":1,"hostSha256":self.host,"bundleSha256":sha,"target":"browser","policyId":"deny-all-v1","ttlSeconds":seconds}),Some(key))
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let id = self.child.id();
        let _ = Command::new("kill")
            .args(["-TERM", &id.to_string()])
            .status();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if self.child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() > until {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("preview shutdown exceeded bound");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let prefix = format!("varta-toolchain-{id}-");
        assert!(
            !fs::read_dir(std::env::temp_dir())
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn files() -> BTreeMap<String, Vec<u8>> {
    let p = RegressionProject::new(
        "export function main():void{throw new Error('only browser may run this');}",
        json!({}),
    );
    p.configure("languageProfile", json!("checked-f64-utf16-v40"));
    let out = p.command(&["build", "--target", "browser"]);
    assert!(out.status.success(), "{}", combined(&out));
    fs::read_dir(p.0.join("dist/web"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_str().unwrap().to_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}
#[test]
fn preview_cli_import_canonical_validation_private_activation_and_terminal_replay() {
    let h = Host::start();
    assert_eq!(h.raw(false, "GET", "/v1/info", b"", "").0, 401);
    assert_eq!(
        h.raw(
            false,
            "GET",
            "/v1/info",
            b"",
            &format!("Authorization: Bearer {TOKEN}\r\nOrigin: http://untrusted.test\r\n")
        )
        .0,
        403
    );
    let (code, info) = h.control("GET", "/v1/info", &json!({}), None);
    assert_eq!(code, 200);
    assert_eq!(info["policyId"], "deny-all-v1");
    assert_eq!(
        h.start_run(&"f".repeat(64), "start-missing-0001", 180).0,
        404
    );
    let files = files();
    let (id, sha) = h.upload(&files, "import-valid-0001");
    let status = h.sealed(&id);
    assert_eq!(status["state"], "ready", "{status}");
    let (code, run) = h.start_run(&sha, "start-valid-00001", 180);
    assert_eq!(code, 202, "{run}");
    let run_id = run["id"].as_str().unwrap();
    let ticket_path = format!("/v1/runs/{run_id}/tickets");
    let (_, ticket) = h.control("POST", &ticket_path, &json!({}), None);
    let token = ticket["launchUrl"]
        .as_str()
        .unwrap()
        .split('#')
        .nth(1)
        .unwrap();
    assert!(!serde_json::to_string(&run).unwrap().contains(token));
    assert_eq!(
        h.raw(
            true,
            "POST",
            "/activate",
            b"",
            &format!("Authorization: Bearer {token}\r\nOrigin: http://127.0.0.1:8787\r\n")
        )
        .0,
        403
    );
    let origin = format!("http://{}", h.public);
    let (code, _, headers) = h.raw(
        true,
        "POST",
        "/activate",
        b"",
        &format!("Authorization: Bearer {token}\r\nOrigin: {origin}\r\n"),
    );
    assert_eq!(code, 200);
    let cookie = headers
        .lines()
        .find_map(|l| l.strip_prefix("set-cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert!(headers.contains("HttpOnly"));
    assert!(headers.contains("SameSite=Strict"));
    assert!(headers.contains(&format!("Path=/runs/{run_id}/")));
    assert_eq!(
        h.raw(
            true,
            "POST",
            "/activate",
            b"",
            &format!("Authorization: Bearer {token}\r\nOrigin: {origin}\r\n")
        )
        .0,
        401
    );
    let index = format!("/runs/{run_id}/index.html");
    assert_eq!(h.raw(true, "GET", &index, b"", "").0, 401);
    let (code, b, headers) = h.raw(true, "GET", &index, b"", &format!("Cookie: {cookie}\r\n"));
    assert_eq!(code, 200);
    assert!(headers.contains("frame-ancestors 'none'"));
    assert!(String::from_utf8(b).unwrap().contains("data-varta-preview"));
    let loader = h.raw(
        true,
        "GET",
        &format!("/runs/{run_id}/varta-host.js"),
        b"",
        &format!("Cookie: {cookie}\r\n"),
    );
    assert!(
        String::from_utf8(loader.1)
            .unwrap()
            .contains("previewBootstrap")
    );
    let (code, b, _) = h.raw(
        true,
        "POST",
        &format!("/runs/{run_id}/claim"),
        b"{}",
        &format!("Cookie: {cookie}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\n"),
    );
    assert_eq!(code, 200);
    let client: Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(client["leaseMs"], 3000);
    assert_eq!(
        h.raw(
            true,
            "POST",
            &format!("/runs/{run_id}/claim"),
            b"{}",
            &format!("Cookie: {cookie}\r\nOrigin: {origin}\r\n")
        )
        .0,
        409
    );
    assert_eq!(
        h.control("POST", &format!("/v1/runs/{run_id}/stop"), &json!({}), None)
            .0,
        200
    );
    assert_eq!(
        h.raw(true, "GET", &index, b"", &format!("Cookie: {cookie}\r\n"))
            .0,
        410
    );
    assert_eq!(
        h.start_run(&sha, "start-valid-00001", 180).1["state"],
        "stopped"
    );
    h.control(
        "POST",
        "/v1/runs/cancel-by-key",
        &json!({"schemaVersion":1,"requestId":"start-cancel-0001"}),
        None,
    );
    assert_eq!(
        h.start_run(&sha, "start-cancel-0001", 180).1["error"],
        "cancelled_request"
    );
    let mut bad = files;
    bad.insert("varta-host.js".into(), b"arbitrary JS".to_vec());
    let (id, _) = h.upload(&bad, "import-badjs-0001");
    assert_eq!(h.sealed(&id)["state"], "failed");
}
#[test]
fn preview_cli_expiry_and_validation_cancel_are_bounded() {
    let h = Host::start();
    let files = files();
    let (id, sha) = h.upload(&files, "import-expiry-001");
    assert_eq!(h.sealed(&id)["state"], "ready");
    let (_, run) = h.start_run(&sha, "start-expiry-0001", 1);
    let run_id = run["id"].as_str().unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    assert_eq!(
        h.control("GET", &format!("/v1/runs/{run_id}"), &json!({}), None)
            .1["state"],
        "expired"
    );
    assert_eq!(
        h.start_run(&sha, "start-expiry-0001", 1).1["state"],
        "expired"
    );
    let (id, _) = h.upload(&files, "import-cancel-001");
    h.control("POST", &format!("/v1/bundles/{id}/seal"), &json!({}), None);
    let (code, cancel) = h.control("DELETE", &format!("/v1/bundles/{id}"), &json!({}), None);
    assert!(
        code == 200 && cancel["state"] == "cancelled"
            || code == 409 && cancel["error"] == "already_ready"
    );
}
