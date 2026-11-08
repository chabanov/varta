use super::*;
use std::{
    io::{BufRead, Read, Write},
    net::SocketAddr,
    process::{Child, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

fn project() -> RegressionProject {
    let project = RegressionProject::new(
        "export function main():void{console.log(\"browser only\");}",
        serde_json::json!({}),
    );
    project.configure(
        "languageProfile",
        serde_json::json!("checked-f64-utf16-v38"),
    );
    project
}

#[test]
fn browser_environment_native_oracle_preserves_unicode_and_denials() {
    let project = RegressionProject::new(
        r#"export function main():void {
          const allowed=varta.result.capture(varta.env.get("VARTA_BROWSER_ALLOWED"));
          const secret=varta.result.capture(varta.env.get("VARTA_BROWSER_SECRET"));
          const missing=varta.result.capture(varta.env.get("VARTA_BROWSER_MISSING"));
          console.log(JSON.stringify({value:allowed.value,allowed:allowed.status,secret:secret.status,missing:missing.status}));
        }"#,
        serde_json::json!({"env":{"get":["VARTA_BROWSER_ALLOWED","VARTA_BROWSER_MISSING"]}}),
    );
    project.configure(
        "languageProfile",
        serde_json::json!("checked-f64-utf16-v38"),
    );
    project.host_policy(serde_json::json!([]));
    for restricted in [false, true] {
        let mut command = varta();
        command.current_dir(&project.0).arg("dev");
        if restricted {
            command.args(["--policy", "host-policy.json"]);
        }
        let output = command
            .env("VARTA_BROWSER_ALLOWED", "Привіт 😀")
            .env("VARTA_BROWSER_SECRET", "PRIVATE")
            .env_remove("VARTA_BROWSER_MISSING")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let text = combined(&output);
        assert!(!text.contains("PRIVATE"));
        let json = text
            .lines()
            .filter_map(|line| line.strip_prefix("varta log: "))
            .find(|line| line.starts_with('{'))
            .unwrap_or_else(|| panic!("missing result JSON: {text}"));
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_ne!(value["secret"], 0);
        assert_ne!(value["missing"], 0);
        if restricted {
            assert_ne!(value["allowed"], 0);
        } else {
            assert_eq!(value["allowed"], 0);
            assert_eq!(value["value"], "Привіт 😀");
        }
    }
}
struct Running {
    child: Child,
    lines: mpsc::Receiver<String>,
    address: SocketAddr,
}
impl Running {
    fn start(project: &RegressionProject, args: &[&str]) -> Self {
        let mut child = varta()
            .current_dir(&project.0)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let first = match lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => line,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("browser dev did not start: {error}")
            }
        };
        let url = first.strip_prefix("varta dev: browser ").unwrap();
        let address = url
            .strip_prefix("http://")
            .unwrap()
            .trim_end_matches('/')
            .parse()
            .unwrap();
        Self {
            child,
            lines,
            address,
        }
    }
    fn get(&self, path: &str) -> String {
        let mut stream = std::net::TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {}\r\n\r\n",
            self.address
        )
        .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn browser_dev_cli_serves_without_native_execution_or_output_writes() {
    let project = project();
    let server = Running::start(&project, &["dev", "--target", "browser", "--budget-report"]);
    let html = server.get("/");
    let loader_path = html
        .split_once("data-varta-dev src=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    assert!(loader_path.starts_with("/__varta/"));
    let loader = server.get(loader_path);
    assert!(loader.contains("\"reportBudgets\":true"));
    assert!(server.get("/varta-host.js").starts_with("HTTP/1.1 404"));
    assert!(!server.get("/index.ts").contains("browser only"));
    assert!(!project.0.join("dist").exists());
    assert!(
        server.lines.try_recv().is_err(),
        "the server must not execute/log the guest"
    );
}

#[test]
fn browser_dev_cli_rejects_native_only_flags_and_nonloopback_listeners() {
    let project = project();
    for (args, needle) in [
        (
            vec!["dev", "--target", "browser", "--profile-fuel"],
            "native fuel",
        ),
        (
            vec![
                "dev",
                "--target",
                "browser",
                "--view-script",
                "missing.json",
            ],
            "native fuel",
        ),
        (
            vec!["dev", "--target", "browser", "--listen", "0.0.0.0:0"],
            "loopback",
        ),
        (
            vec!["dev", "--target", "native", "--listen", "127.0.0.1:0"],
            "--listen",
        ),
    ] {
        let output = project.command(&args);
        assert!(!output.status.success());
        assert!(combined(&output).contains(needle), "{}", combined(&output));
        assert!(!project.0.join("dist").exists());
    }
    let native = project.command(&["dev", "--target", "native"]);
    assert!(native.status.success(), "{}", combined(&native));
    assert!(combined(&native).contains("varta log: browser only"));
}
#[cfg(unix)]
#[test]
fn browser_dev_cli_ctrl_c_closes_the_server_successfully() {
    let project = project();
    let mut server = Running::start(&project, &["dev", "--target", "browser"]);
    assert!(server.get("/").starts_with("HTTP/1.1 200"));
    assert!(
        Command::new("kill")
            .args(["-INT", &server.child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = server.child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "Ctrl-C did not stop browser dev");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    assert!(std::net::TcpStream::connect(server.address).is_err());
}

#[test]
fn browser_dev_cli_protected_session_bootstrap_is_host_only_and_profile_gated() {
    let project = RegressionProject::new(
        "export function main():void{}",
        serde_json::json!({"net":{"session":{"urls":["http://127.0.0.1:8787/api/**"],"methods":["GET","POST"]}}}),
    );
    project.language_profile("checked-f64-utf16-v40");
    let binding = serde_json::json!({"apiOrigin":"http://127.0.0.1:8787","clientOrigin":"http://127.0.0.1:8788","allowInsecureLoopback":true});
    fs::write(project.0.join("host-policy.json"),serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"abiVersion":36,"capabilities":[{"id":"net.session","patterns":["http://127.0.0.1:8787/api/**"],"methods":["GET","POST"]}],"browserSession":binding})).unwrap()).unwrap();
    let server = Running::start(
        &project,
        &["dev", "--target", "browser", "--policy", "host-policy.json"],
    );
    let html = server.get("/");
    let loader_path = html
        .split_once("data-varta-dev src=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap();
    let loader = server.get(loader_path);
    assert!(loader.contains("\"sessionBinding\""));
    assert!(loader.contains("\"net.session\""));
    assert!(loader.contains("http://127.0.0.1:8788"));
    drop(server);
    let output = project.command(&["build", "--target", "browser"]);
    assert!(output.status.success(), "{}", combined(&output));
    // Distribution output contains the channel implementation, but no host binding.
    let portable = fs::read_to_string(project.0.join("dist/web/varta-host.js")).unwrap();
    assert!(!portable.contains("http://127.0.0.1:8788"));
    let output = project.command(&[
        "build",
        "--target",
        "browser",
        "--policy",
        "host-policy.json",
    ]);
    assert!(!output.status.success());
    project.set_capabilities(serde_json::json!({}));
    project.language_profile("checked-f64-utf16-v39");
    let output = project.command(&["dev", "--target", "browser", "--policy", "host-policy.json"]);
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("browserSession requires"),
        "{}",
        combined(&output)
    );
}
