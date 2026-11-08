use super::*;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Project(pub(super) PathBuf);
impl Project {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "varta-browser-dev-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("main.ts"),
            "export function main():void{console.log(123);}",
        )
        .unwrap();
        std::fs::write(root.join("secret.txt"), "must never be served").unwrap();
        std::fs::write(root.join("varta.config.json"),r#"{"entry":"main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v38","fs":{"roots":{"unused":"missing"}}}"#).unwrap();
        Self(root)
    }
    pub(super) fn config(&self) -> PathBuf {
        self.0.join("varta.config.json")
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn request(address: SocketAddr, text: String) -> String {
    tokio::task::spawn_blocking(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.write_all(text.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    })
    .await
    .unwrap()
}
fn get(address: SocketAddr, path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: {address}\r\n\r\n")
}

#[tokio::test]
async fn browser_dev_serves_only_an_immutable_bundle_with_security_headers() {
    let project = Project::new();
    let server =
        BrowserDevServer::bind(&project.config(), None, None, BrowserDevOptions::default())
            .unwrap();
    let address = server.address();
    let prefix = server.delivery.read().unwrap().latest().prefix();
    assert_eq!(server.url(), format!("http://{address}/"));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task =
        tokio::spawn(server.serve_until(async { stopped.await.map_err(std::io::Error::other) }));
    let html = request(address, get(address, "/")).await;
    assert!(html.starts_with("HTTP/1.1 200"));
    assert!(html.contains("data-varta-dev"));
    assert!(!html.contains("data-varta-bootstrap"));
    assert!(html.contains("frame-ancestors 'none'"));
    assert!(html.contains("cache-control: no-store"));
    assert!(html.contains("cross-origin-resource-policy: same-origin"));
    assert!(html.contains(&format!("src=\"{prefix}varta-host.js\"")));
    let loader_path = format!("{prefix}varta-host.js");
    let loader = request(address, get(address, &loader_path)).await;
    assert!(loader.contains("text/javascript"));
    assert!(loader.contains("devBootstrap({\"application\":false,\"reportBudgets\":false});"));
    let body = loader.split_once("\r\n\r\n").unwrap().1;
    let integrity = format!(
        "sha256-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body.as_bytes()))
    );
    assert_eq!(html.matches(&integrity).count(), 2);
    for path in [
        "/main.ts",
        "/secret.txt",
        "/varta.config.json",
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "//main.ts",
        "/dist/app.wasm",
        "/varta-host.js",
        "/app.wasm",
    ] {
        let value = request(address, get(address, path)).await;
        assert!(value.starts_with("HTTP/1.1 404"), "{path}: {value}");
        assert!(!value.contains("must never be served"));
    }
    assert!(
        !project.0.join("dist").exists(),
        "dev must not create production output"
    );
    std::fs::write(project.0.join("main.ts"), "invalid source after snapshot").unwrap();
    assert_eq!(
        request(address, get(address, &loader_path))
            .await
            .split_once("\r\n\r\n")
            .unwrap()
            .1,
        body
    );
    let head = request(
        address,
        format!("HEAD {loader_path} HTTP/1.1\r\nHost: {address}\r\n\r\n"),
    )
    .await;
    assert!(head.starts_with("HTTP/1.1 200"));
    assert_eq!(head.split_once("\r\n\r\n").unwrap().1, "");
    assert!(head.contains(&format!("content-length: {}", body.len())));
    let etag = loader
        .lines()
        .find_map(|line| line.strip_prefix("etag: "))
        .unwrap();
    let cached = request(
        address,
        format!("GET {loader_path} HTTP/1.1\r\nHost: {address}\r\nIf-None-Match: {etag}\r\n\r\n"),
    )
    .await;
    assert!(cached.starts_with("HTTP/1.1 304"));
    let status = request(address, format!("GET /__varta/status HTTP/1.1\r\nHost: {address}\r\nIf-None-Match: \"dev-status\"\r\n\r\n")).await;
    assert!(
        status.starts_with("HTTP/1.1 200"),
        "status must never become a stale 304"
    );
    let state: serde_json::Value =
        serde_json::from_str(status.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(state["revision"], prefix.split('/').nth(2).unwrap());
    let foreign = request(address, format!("GET /__varta/status HTTP/1.1\r\nHost: {address}\r\nOrigin: https://foreign.invalid\r\n\r\n")).await;
    assert!(foreign.starts_with("HTTP/1.1 403"));
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
    assert!(std::net::TcpStream::connect(address).is_err());
}

#[tokio::test]
async fn browser_dev_rejects_foreign_hosts_origins_methods_and_request_bodies() {
    let project = Project::new();
    let server =
        BrowserDevServer::bind(&project.config(), None, None, BrowserDevOptions::default())
            .unwrap();
    let address = server.address();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task =
        tokio::spawn(server.serve_until(async { stopped.await.map_err(std::io::Error::other) }));
    for (wire, code) in [
        (format!("GET * HTTP/1.1\r\nHost: {address}\r\n\r\n"), 400),
        (
            "GET / HTTP/1.1\r\nHost: attacker.invalid\r\n\r\n".into(),
            400,
        ),
        (
            format!("GET http://attacker.invalid/ HTTP/1.1\r\nHost: {address}\r\n\r\n"),
            400,
        ),
        (
            format!("GET / HTTP/1.1\r\nHost: {address}\r\nOrigin: http://attacker.invalid\r\n\r\n"),
            403,
        ),
        (
            format!("GET / HTTP/1.1\r\nHost: {address}\r\nSec-Fetch-Site: cross-site\r\n\r\n"),
            403,
        ),
        (
            format!("POST / HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\n\r\n"),
            405,
        ),
        (
            format!("GET / HTTP/1.1\r\nHost: {address}\r\nContent-Length: 1\r\n\r\nx"),
            400,
        ),
    ] {
        let value = request(address, wire).await;
        assert!(value.starts_with(&format!("HTTP/1.1 {code}")), "{value}");
    }
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn browser_dev_shutdown_closes_slow_connections() {
    let project = Project::new();
    let server =
        BrowserDevServer::bind(&project.config(), None, None, BrowserDevOptions::default())
            .unwrap();
    let address = server.address();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task =
        tokio::spawn(server.serve_until(async { stopped.await.map_err(std::io::Error::other) }));
    let client = tokio::task::spawn_blocking(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n").unwrap();
        stream
    });
    let mut client = client.await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(client.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn browser_dev_runtime_inputs_are_scoped_and_never_change_portable_files() {
    let project = Project::new();
    let base = BrowserDevServer::bind(&project.config(), None, None, BrowserDevOptions::default())
        .unwrap();
    let original = base.delivery.read().unwrap().latest().files["app.wasm"]
        .bytes
        .clone();
    let identity = varta_runtime::artifact_identity(&original);
    let decisions=DecisionFile::parse(serde_json::to_vec(&serde_json::json!({"applications":[{"artifact":identity,"decisions":[{"id":"time.timer","decision":"deny"}]},{"artifact":format!("sha256:{}","b".repeat(64)),"decisions":[{"id":"time.timer","decision":"allow"}]}]})).unwrap().as_slice()).unwrap();
    let mut manifest = varta_common::Manifest::new(&[]);
    manifest.abi_version = varta_common::language::LanguageProfile::CheckedF64Utf16V38.host_abi();
    manifest.budgets = Some(varta_common::budget::BudgetRequest {
        memory_bytes: Some(varta_common::budget::Budgets::PRODUCTION.memory_bytes as u64 / 2),
        ..Default::default()
    });
    let policy = HostPolicy::from_manifest(manifest).unwrap();
    let bounded = BrowserDevServer::bind(
        &project.config(),
        Some(&policy),
        Some(&decisions),
        BrowserDevOptions::default(),
    )
    .unwrap();
    assert_eq!(
        bounded.delivery.read().unwrap().latest().files["app.wasm"].bytes,
        original
    );
    assert_eq!(
        bounded.delivery.read().unwrap().latest().files["app.manifest.json"].bytes,
        base.delivery.read().unwrap().latest().files["app.manifest.json"].bytes
    );
    assert_ne!(
        bounded.delivery.read().unwrap().latest().files["app.private.wasm"].bytes,
        base.delivery.read().unwrap().latest().files["app.private.wasm"].bytes
    );
    let delivery = bounded.delivery.read().unwrap();
    let loader = std::str::from_utf8(&delivery.latest().files[LOADER].bytes).unwrap();
    let runtime = loader
        .rsplit_once("devBootstrap(")
        .unwrap()
        .1
        .strip_suffix(");\n")
        .unwrap();
    let runtime: serde_json::Value = serde_json::from_str(runtime).unwrap();
    assert_eq!(runtime["permissionCeiling"], serde_json::json!({}));
    assert_eq!(
        runtime["decisions"]["applications"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        runtime["decisions"]["applications"][0]["artifact"],
        identity
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn browser_dev_bootstrap_captures_only_environment_scopes_from_host_policy() {
    use crate::Artifact;
    use varta_common::{CapabilityGrant, CapabilityValue, Manifest};
    let project = Project::new();
    let (config, _, compilation) = crate::compile(&project.config(), false).unwrap();
    let artifact = Artifact {
        wasm: compilation.wasm,
        manifest: config.manifest(),
    };
    let grants = [
        CapabilityGrant {
            id: "env.get".into(),
            value: CapabilityValue::PathGlobs(vec!["PUBLIC_*".into()]),
        },
        CapabilityGrant {
            id: "env.has".into(),
            value: CapabilityValue::PathGlobs(vec!["EXISTS".into()]),
        },
        CapabilityGrant {
            id: "fs.read".into(),
            value: CapabilityValue::PathGlobs(vec!["/private/**".into()]),
        },
    ];
    let mut manifest = Manifest::new(&grants);
    manifest.abi_version = varta_common::language::LanguageProfile::CheckedF64Utf16V38.host_abi();
    let policy = HostPolicy::from_manifest(manifest).unwrap();
    let inputs = runtime_inputs(
        &artifact,
        Some(&policy),
        None,
        &BrowserDevOptions::default(),
    );
    assert_eq!(
        inputs["permissionCeiling"],
        serde_json::json!({"env.get":{"patterns":["PUBLIC_*"]},"env.has":{"patterns":["EXISTS"]}})
    );
    assert!(!inputs.to_string().contains("/private/**"));
}

#[test]
fn browser_dev_admission_and_loopback_checks_precede_listening_or_output() {
    let project = Project::new();
    let options = BrowserDevOptions {
        address: ([0, 0, 0, 0], 0).into(),
        ..Default::default()
    };
    assert!(matches!(
        BrowserDevServer::bind(&project.config(), None, None, options),
        Err(ProjectError::Usage(_))
    ));
    assert!(!project.0.join("dist").exists());
}

#[test]
fn browser_dev_script_optin_rewrite_never_changes_requested_origins() {
    let project = Project::new();
    let path = project.config();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["capabilities"] =
        serde_json::json!({"net":{"request":["https://data-varta-bootstrap.invalid/**"]}});
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let server = BrowserDevServer::bind(&path, None, None, BrowserDevOptions::default()).unwrap();
    let delivery = server.delivery.read().unwrap();
    let html = std::str::from_utf8(&delivery.latest().files[INDEX].bytes).unwrap();
    assert!(html.contains("https://data-varta-bootstrap.invalid"));
    assert!(!html.contains("https://data-varta-dev.invalid"));
    assert!(html.contains(" data-varta-dev src="));
}

#[test]
fn browser_dev_rejects_a_drifting_bootstrap_instead_of_dropping_runtime_policy() {
    let mut files = BTreeMap::from([
        (LOADER.into(), b"// trusted loader".to_vec()),
        (
            INDEX.into(),
            b"<script data-varta-bootstrap=\"\"></script>".to_vec(),
        ),
    ]);
    assert!(install_bootstrap(&mut files, &serde_json::json!({"permissionCeiling":{}})).is_err());
}

#[test]
fn browser_dev_never_embeds_environment_values_in_http_loader() {
    let mut files = BTreeMap::new();
    let error = install_bootstrap(
        &mut files,
        &serde_json::json!({"environmentInputs":[["SECRET","private"]]}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot be embedded"));
    assert!(files.is_empty());
}

#[test]
fn browser_dev_and_distribution_reject_http_contracts_before_output() {
    let project = Project::new();
    std::fs::write(project.0.join("main.ts"),r#"function handle(r:VartaHttpRequest):VartaHttpResponse{return {status:200,body:"hello",headers:[]};}export function main():void{console.log("must not run");}varta.http.serve({hostname:"127.0.0.1",port:0},handle);"#).unwrap();
    let error = BrowserDevServer::bind(&project.config(), None, None, BrowserDevOptions::default())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("HTTP service contracts"),
        "{error}"
    );
    let error = super::super::build_browser(&project.config()).unwrap_err();
    assert!(
        error.to_string().contains("HTTP service contracts"),
        "{error}"
    );
    assert!(!project.0.join("dist").exists());
}

#[test]
fn browser_builds_carry_every_packaged_face_of_the_resource_section() {
    const REGULAR: &[u8] = include_bytes!("../../../../varta-view/tests/fixtures/advance-400.ttf");
    const BOLD: &[u8] = include_bytes!("../../../../varta-view/tests/fixtures/bold-900.ttf");
    const ITALIC: &[u8] = include_bytes!("../../../../varta-view/tests/fixtures/italic-900.ttf");
    const MONO: &[u8] = include_bytes!("../../../../varta-view/tests/fixtures/advance-900.ttf");
    let project = Project::new();
    std::fs::create_dir(project.0.join("fonts")).unwrap();
    for (name, bytes) in [
        ("regular.ttf", REGULAR),
        ("bold.ttf", BOLD),
        ("italic.ttf", ITALIC),
        ("mono.ttf", MONO),
    ] {
        std::fs::write(project.0.join("fonts").join(name), bytes).unwrap();
    }
    let fonts = serde_json::json!([
        {"family": "Varta Fixture 400", "path": "fonts/bold.ttf", "weight": 700},
        {"family": "Varta Fixture 900", "path": "fonts/mono.ttf", "generic": ["monospace"]},
        {"family": "Varta Fixture 400", "path": "fonts/italic.ttf", "style": "italic"},
        {"family": "Varta Fixture 400", "path": "fonts/regular.ttf"},
    ]);
    let config = |browser: serde_json::Value| {
        let mut config = serde_json::json!({"entry":"main.ts","target":"wasm","out":"dist/app","languageProfile":"checked-f64-utf16-v38","fonts":fonts});
        if !browser.is_null() {
            config["browser"] = browser;
        }
        std::fs::write(project.config(), config.to_string()).unwrap();
    };
    // `fonts` serves every renderer; a second, browser-only face list is refused.
    config(serde_json::json!({"fonts":{"Brand":"fonts/regular.ttf"}}));
    let error = super::super::build_browser(&project.config()).unwrap_err();
    assert!(
        error.to_string().contains("cannot both be declared"),
        "{error}"
    );
    assert!(!project.0.join("dist").exists());

    config(serde_json::Value::Null);
    super::super::build_browser(&project.config()).unwrap();
    let web = project.0.join("dist/web");
    // The resource section's canonical order: the default family's faces by
    // weight, then style, then the monospace family.
    for (index, bytes) in [REGULAR, ITALIC, BOLD, MONO].into_iter().enumerate() {
        assert_eq!(
            std::fs::read(web.join(format!("varta-resource-{index}.ttf"))).unwrap(),
            bytes,
            "face {index}"
        );
    }
    // Descriptor keys are serialized in sorted order.
    let loader = std::fs::read_to_string(web.join("varta-host.js")).unwrap();
    for descriptor in [
        r#""generics":[],"key":"Varta Fixture 400","mediaType":"font/ttf","style":"normal","weight":400"#,
        r#""generics":[],"key":"Varta Fixture 400","mediaType":"font/ttf","style":"italic","weight":400"#,
        r#""generics":[],"key":"Varta Fixture 400","mediaType":"font/ttf","style":"normal","weight":700"#,
        r#""generics":["monospace"],"key":"Varta Fixture 900","mediaType":"font/ttf","style":"normal","weight":400"#,
    ] {
        assert!(loader.contains(descriptor), "{descriptor}");
    }
}

#[test]
fn browser_dev_protected_session_binding_is_trusted_runtime_input_only() {
    let artifact = crate::Artifact {
        wasm: Vec::new(),
        manifest: varta_common::Manifest::new(&[]),
    };
    let mut manifest = varta_common::Manifest::new(&[varta_common::CapabilityGrant {
        id: varta_common::session::CLIENT_CAPABILITY.into(),
        value: varta_common::CapabilityValue::Requests(varta_common::RequestScope {
            urls: vec!["https://api.test/api/**".into()],
            methods: Some(vec!["GET".into(), "POST".into()]),
            response_headers: None,
        }),
    }]);
    manifest.abi_version = varta_common::abi::PROTECTED_SESSION_VERSION;
    manifest.browser_session = Some(varta_common::session::ClientBinding {
        api_origin: "https://api.test".into(),
        client_origin: "https://studio.test".into(),
        allow_insecure_loopback: false,
    });
    assert!(manifest.validate_package().is_err());
    let policy = HostPolicy::from_manifest(manifest).unwrap();
    let inputs = runtime_inputs(
        &artifact,
        Some(&policy),
        None,
        &BrowserDevOptions::default(),
    );
    assert_eq!(inputs["sessionBinding"]["apiOrigin"], "https://api.test");
    assert_eq!(
        inputs["permissionCeiling"]["net.session"]["patterns"],
        serde_json::json!(["https://api.test/api/**"])
    );
    assert!(inputs.get("http").is_none());
    assert!(inputs.get("fsRoots").is_none());
}
