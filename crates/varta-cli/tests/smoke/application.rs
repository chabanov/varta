use super::*;
use std::{
    io::{BufRead, Read, Write},
    net::TcpStream,
    process::Stdio,
    sync::mpsc,
    time::Duration,
};

#[test]
fn application_host_requires_explicit_authority_and_keeps_control_private() {
    let root = temp_project();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("ceiling.json");
    let token = "a".repeat(64);
    fs::write(&path, serde_json::to_vec(&serde_json::json!({
        "host":{"schemaVersion":1,"controlListen":"127.0.0.1:0","publicListen":"[::1]:0","protectedOrigins":["http://127.0.0.1:8787"],"maxImports":1,"maxRuns":1,"maxRunSeconds":120,"importSeconds":60,"retentionSeconds":300,"validationDeadlineMs":15000,"cpuSeconds":10,"addressSpaceBytes":4294967296u64,"budgets":{}},
        "publicOrigin":"http://localhost:8802","nativeOrigin":"http://127.0.0.1:8787","nativeControlTokenEnv":"VARTA_SMOKE_APP_NATIVE","pathPrefix":"/private"
    })).unwrap()).unwrap();
    let denied = varta()
        .args(["application-host", "--ceiling"])
        .arg(&path)
        .env("VARTA_APPLICATION_TOKEN", &token)
        .env_remove("VARTA_SMOKE_APP_NATIVE")
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(combined(&denied).contains("isolated public application origin"));
    // A concrete nonzero port is mandatory; acquire one temporarily for this
    // process-owned smoke fixture rather than reserving a global fixed port.
    let listener = std::net::TcpListener::bind("[::1]:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["host"]["publicListen"] = format!("[::1]:{port}").into();
    config["publicOrigin"] = format!("http://localhost:{port}").into();
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let denied = varta()
        .args(["application-host", "--ceiling"])
        .arg(&path)
        .env("VARTA_APPLICATION_TOKEN", &token)
        .env_remove("VARTA_SMOKE_APP_NATIVE")
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(combined(&denied).contains("native control credential unavailable"));
    drop(listener);
    let mut child = varta()
        .args(["application-host", "--ceiling"])
        .arg(&path)
        .env("VARTA_APPLICATION_TOKEN", &token)
        .env("VARTA_SMOKE_APP_NATIVE", "b".repeat(64))
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
    let result = std::panic::catch_unwind(|| {
        let line = rx.recv_timeout(Duration::from_secs(60)).unwrap();
        let address = line
            .strip_prefix("varta application control: http://")
            .unwrap();
        for (authorization, expected) in [
            (String::new(), 401),
            (format!("Authorization: Bearer {token}\r\n"), 200),
        ] {
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            write!(socket,"GET /v1/info HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{authorization}\r\n").unwrap();
            let mut reply = String::new();
            socket.read_to_string(&mut reply).unwrap();
            assert!(
                reply.starts_with(&format!("HTTP/1.1 {expected}")),
                "{reply}"
            );
            if expected == 200 {
                assert!(reply.contains("scoped-json-v1"));
                assert!(reply.contains("official-app-browser-v1"));
                assert!(!reply.contains(&"b".repeat(64)));
            }
        }
    });
    let _ = child.kill();
    let _ = child.wait();
    fs::remove_dir_all(root).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
