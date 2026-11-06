//! The native shell core without a window: packaged artifact validation,
//! grant resolution against the host ceiling, denial before effect, a
//! loopback `net.request`, a KV round-trip in host-owned storage, a
//! rendered first frame, artifact fonts with real face selection, and guest
//! clipboard and link effects through a recording platform sink.
//!
//! The guests are `fixtures/shell-probe` and `fixtures/view-probe` (with its
//! declared fonts), compiled at test time by the workspace's `varta` binary
//! into portable v38 artifacts ([`support`]): this crate never depends on
//! the compiler, not even for tests. As in the `varta-launcher` tests, each
//! test writes its own packaged artifact with `varta_artifact::Artifact`
//! into a temporary directory, here with the manifest (the package request)
//! the test needs. No test touches the system clipboard or browser.
mod support;

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{consent_probe, probe, view_probe};
use varta_artifact::{Artifact, Resources};
use varta_common::Manifest;
use varta_common::permission::{PermissionError, PermissionPrompt, PromptAnswer, StoredDecision};
use varta_desktop::shell::Started;
use varta_desktop::shell::consent::{
    AUDIT_FILE, CONSENT_FILES, ConsentStore, DECISIONS_FILE, HostPrompt, MAX_RESOURCE_LINES,
    STORE_POLL_INTERVAL, UNPRESENTABLE_PROMPT,
};
use varta_desktop::shell::{FontSet, Launch, Package, ShellError, storage_directory};
use varta_runtime::{Application, Step, ViewLayout};
use varta_view::{PlatformEffects, PlatformRefusal};

/// A platform sink that records every effect it receives and answers with
/// `refuse`; it never touches the system clipboard or browser.
#[derive(Debug, Default)]
struct Recording {
    refuse: bool,
    clipboard: Mutex<Vec<String>>,
    opened: Mutex<Vec<String>>,
}
impl Recording {
    fn answer(&self) -> Result<(), PlatformRefusal> {
        if self.refuse {
            Err(PlatformRefusal)
        } else {
            Ok(())
        }
    }
    fn clipboard(&self) -> Vec<String> {
        self.clipboard.lock().unwrap().clone()
    }
    fn opened(&self) -> Vec<String> {
        self.opened.lock().unwrap().clone()
    }
}
impl PlatformEffects for Recording {
    fn write_clipboard(&self, text: &str) -> Result<(), PlatformRefusal> {
        self.clipboard.lock().unwrap().push(text.into());
        self.answer()
    }
    fn open_url(&self, url: &str) -> Result<(), PlatformRefusal> {
        self.opened.lock().unwrap().push(url.into());
        self.answer()
    }
}
/// A recording sink for tests that only need one.
fn effects() -> Arc<dyn PlatformEffects> {
    Arc::new(Recording::default())
}

/// The guest log prefix of `console.log` lines.
const LOG: &str = "varta log: ";
/// The probe's last line.
const DONE: &str = "probe done";
/// How long a probe may take before the test fails.
const PROBE_DEADLINE: Duration = Duration::from_secs(30);
/// One application step's wait for timers and network completions.
const STEP_WAIT: Duration = Duration::from_millis(20);
/// The denial lines the runtime logs before each refused operation.
const URL_DENIED: &str = "<capability denied: fs.read> (/config/url.txt)";
const KV_DENIED: &str = "<capability denied: kv.read> (/data/app.kv)";
fn net_denied(url: &str) -> String {
    format!("<capability denied: net.request> (GET {url})")
}
/// The probe's KV database, relative to its `data` root.
const DATABASE: &str = "app.kv";
/// The loopback route the probe requests and the body it answers with.
const ROUTE: &str = "/probe";
const BODY: &str = "loopback";
/// The layout viewport of the headless runs, in CSS pixels.
const VIEW_WIDTH: u32 = 160;
const VIEW_HEIGHT: u32 = 96;
const ROOT_FONT_SIZE: f32 = 16.0;
/// The probe paints a 64x32 `#0f0` panel at the origin.
const PANEL: (u32, u32) = (64, 32);
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// A temporary directory of one test.
struct Temp(support::Temp);
impl Temp {
    fn new() -> Self {
        Self(support::Temp::new("varta-shell-test"))
    }
}
impl std::ops::Deref for Temp {
    type Target = support::Temp;
    fn deref(&self) -> &support::Temp {
        &self.0
    }
}

/// A test-only loopback HTTP/1.1 server on 127.0.0.1 that counts accepted
/// connections, so a denial test can prove that no socket was opened.
struct Loopback {
    base: String,
    accepted: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    address: std::net::SocketAddr,
}
impl Loopback {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (counter, stopped) = (accepted.clone(), stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                answer(stream);
            }
        });
        Self {
            base: format!("http://{address}"),
            accepted,
            stop,
            address,
        }
    }
    fn url(&self, route: &str) -> String {
        format!("{}{route}", self.base)
    }
    fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}
impl Drop for Loopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so its thread ends.
        let _ = TcpStream::connect(self.address);
    }
}
fn answer(stream: TcpStream) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
        if line == "\r\n" {
            break;
        }
        line.clear();
    }
    let _ = write!(
        reader.get_mut(),
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{BODY}",
        BODY.len()
    );
}

/// The probe's built package manifest as JSON.
fn probe_manifest() -> serde_json::Value {
    serde_json::from_slice(&probe().manifest.to_bytes().unwrap()).unwrap()
}

/// The probe's package manifest with `capabilities` as its request.
fn manifest(capabilities: serde_json::Value) -> Manifest {
    let mut manifest = probe_manifest();
    manifest["capabilities"] = capabilities;
    Manifest::parse(&serde_json::to_vec(&manifest).unwrap()).unwrap()
}

/// Write the probe as a packaged artifact (`app.wasm` + `app.manifest.json`).
fn package(temp: &Temp, capabilities: serde_json::Value) -> PathBuf {
    let path = temp.path("app.wasm");
    Artifact {
        wasm: probe().wasm.clone(),
        manifest: manifest(capabilities),
    }
    .write_wasm(&path)
    .unwrap();
    path
}

/// Write a host ceiling file.
fn policy(temp: &Temp, ceiling: &serde_json::Value) -> PathBuf {
    let path = temp.path("policy.json");
    std::fs::write(&path, serde_json::to_vec(ceiling).unwrap()).unwrap();
    path
}

/// A host ceiling that maps the probe's `config` root to a directory holding
/// `url.txt` (the URL the probe requests) and allows `capabilities`.
fn ceiling_with_url(temp: &Temp, url: &str, capabilities: serde_json::Value) -> PathBuf {
    let config = temp.path("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("url.txt"), url).unwrap();
    policy(
        temp,
        &serde_json::json!({
            "abiVersion": 34,
            "fsRoots": {"config": config},
            "capabilities": capabilities,
        }),
    )
}

fn kv_grants() -> [serde_json::Value; 2] {
    [
        serde_json::json!({"id": "kv.read", "patterns": ["/data/app.kv"]}),
        serde_json::json!({"id": "kv.write", "patterns": ["/data/app.kv"]}),
    ]
}
fn url_grants(url: &str) -> [serde_json::Value; 2] {
    [
        serde_json::json!({"id": "fs.read", "patterns": ["/config/url.txt"]}),
        serde_json::json!({"id": "net.request", "patterns": [url]}),
    ]
}

fn prepare(temp: &Temp, artifact: &Path, policy: Option<&Path>) -> Result<Launch, ShellError> {
    Launch::packaged(&Package {
        artifact,
        policy,
        storage: &temp.storage(),
        font: None,
    })
}

fn layout() -> ViewLayout {
    ViewLayout::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32, None, ROOT_FONT_SIZE).unwrap()
}

/// Start `launch` without a window and step it until the probe is done;
/// the instance and its guest log lines.
fn run(launch: Launch) -> (Application, Vec<String>) {
    let mut app = launch.start(layout(), effects()).unwrap();
    let mut lines = Vec::new();
    let deadline = Instant::now() + PROBE_DEADLINE;
    loop {
        lines.extend(
            app.take_output()
                .into_iter()
                .map(|line| line.strip_prefix(LOG).unwrap_or(&line).to_owned()),
        );
        if lines.iter().any(|line| line == DONE) {
            return (app, lines);
        }
        assert!(Instant::now() < deadline, "probe timed out: {lines:?}");
        match app.step(Some(STEP_WAIT)) {
            Step::Turn(_) | Step::Idle => {}
            Step::Finished { .. } => {}
            Step::Failed => panic!("probe failed: {:?} {lines:?}", app.failure()),
        }
    }
}

/// The probe's storage directory for `artifact` under the test's base.
fn storage_of(temp: &Temp, artifact: &Path) -> PathBuf {
    let wasm = Artifact::read(artifact).unwrap().wasm;
    storage_directory(&temp.storage(), &varta_runtime::artifact_identity(&wasm)).unwrap()
}

/// The names in `directory`; none when it does not exist.
fn entries(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

/// Assert that host storage holds nothing but the artifact's own consent
/// files (every packaged start writes its audit log): no KV root or file,
/// no other artifact's directory and nothing else in the storage base.
fn assert_only_consent_files(temp: &Temp, artifact: &Path) {
    let directory = storage_of(temp, artifact);
    let own = directory
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let base = entries(&temp.storage());
    assert!(base.iter().all(|name| *name == own), "{base:?}");
    let files = entries(&directory);
    assert!(
        files
            .iter()
            .all(|name| CONSENT_FILES.contains(&name.as_str())),
        "{files:?}"
    );
}

fn database_of(temp: &Temp, artifact: &Path) -> PathBuf {
    storage_of(temp, artifact).join("data").join(DATABASE)
}

#[test]
fn package_request_alone_grants_a_kv_round_trip_in_host_owned_storage() {
    let temp = Temp::new();
    let artifact = package(&temp, serde_json::json!(kv_grants()));
    let launch = prepare(&temp, &artifact, None).unwrap();
    assert!(launch.ceiling().is_none());
    assert_eq!(launch.file_roots().collect::<Vec<_>>(), ["data"]);
    let identity = launch.identity().to_owned();
    let (_, lines) = run(launch);
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            "kv before missing",
            "kv after stored",
            DONE
        ]
    );
    let database = database_of(&temp, &artifact);
    assert!(database.is_file(), "{database:?}");
    // The directory is the artifact's own: its exact Wasm digest.
    assert_eq!(
        storage_of(&temp, &artifact),
        storage_directory(&temp.storage(), &identity).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(storage_of(&temp, &artifact))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "storage must be private: {mode:o}");
    }
    // A new instance of the same artifact reads what the first one stored.
    let (_, lines) = run(prepare(&temp, &artifact, None).unwrap());
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            "kv before stored",
            "kv after stored",
            DONE
        ]
    );
}

#[test]
fn exact_grants_admit_a_loopback_request_and_kv_within_the_ceiling() {
    let temp = Temp::new();
    let server = Loopback::start();
    let url = server.url(ROUTE);
    let mut request = url_grants(&url).to_vec();
    request.extend(kv_grants());
    let artifact = package(&temp, serde_json::json!(request));
    let ceiling = ceiling_with_url(&temp, &url, serde_json::json!(request));
    let launch = prepare(&temp, &artifact, Some(&ceiling)).unwrap();
    assert_eq!(launch.ceiling().map(<[_]>::len), Some(request.len()));
    assert_eq!(launch.file_roots().collect::<Vec<_>>(), ["config", "data"]);
    let (_, lines) = run(launch);
    assert_eq!(
        lines,
        [
            format!("net 200 {BODY}").as_str(),
            "kv before missing",
            "kv after stored",
            DONE
        ]
    );
    assert_eq!(server.accepted(), 1);
    assert!(database_of(&temp, &artifact).is_file());
}

#[test]
fn net_request_to_an_ungranted_url_is_denied_before_any_socket() {
    let temp = Temp::new();
    let server = Loopback::start();
    // The package and the ceiling grant only another route of the server.
    let granted = url_grants(&server.url("/other"));
    let artifact = package(&temp, serde_json::json!(granted));
    let ceiling = ceiling_with_url(&temp, &server.url(ROUTE), serde_json::json!(granted));
    let (_, lines) = run(prepare(&temp, &artifact, Some(&ceiling)).unwrap());
    assert_eq!(
        lines,
        [
            &net_denied(&server.url(ROUTE)),
            "net NotAllowedError",
            KV_DENIED,
            "kv NotAllowedError",
            DONE
        ]
    );
    assert_eq!(server.accepted(), 0);
    // The package requests no KV: no root is created for it.
    assert_only_consent_files(&temp, &artifact);
}

#[test]
fn kv_without_a_grant_is_denied_before_any_kv_file_exists() {
    let temp = Temp::new();
    // `fsRoots` still declares `data`, but nothing requests `kv.*`.
    let artifact = package(&temp, serde_json::json!([]));
    let launch = prepare(&temp, &artifact, None).unwrap();
    assert_eq!(launch.file_roots().count(), 0);
    let (_, lines) = run(launch);
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            KV_DENIED,
            "kv NotAllowedError",
            DONE
        ]
    );
    assert_only_consent_files(&temp, &artifact);
}

#[test]
fn a_narrower_ceiling_bounds_the_package_request() {
    let temp = Temp::new();
    let server = Loopback::start();
    let mut request = url_grants(&server.url(ROUTE)).to_vec();
    request.extend(kv_grants());
    let artifact = package(&temp, serde_json::json!(request));
    // Reading only, and requests only to another route.
    let ceiling = ceiling_with_url(
        &temp,
        &server.url(ROUTE),
        serde_json::json!([
            {"id": "fs.read", "patterns": ["/config/url.txt"]},
            {"id": "net.request", "patterns": [server.url("/other")]},
            {"id": "kv.read", "patterns": ["/data/app.kv"]},
        ]),
    );
    let (_, lines) = run(prepare(&temp, &artifact, Some(&ceiling)).unwrap());
    // Without `kv.write` the database opens read-only: a missing file is
    // not created.
    assert_eq!(
        lines,
        [
            &net_denied(&server.url(ROUTE)),
            "net NotAllowedError",
            "kv NotFoundError",
            DONE
        ]
    );
    assert_eq!(server.accepted(), 0);
    assert!(!database_of(&temp, &artifact).exists());
}

#[test]
fn a_ceiling_mapping_of_a_kv_root_wins_over_host_owned_storage() {
    let temp = Temp::new();
    let artifact = package(&temp, serde_json::json!(kv_grants()));
    let data = temp.path("ceiling-data");
    std::fs::create_dir(&data).unwrap();
    let ceiling = policy(
        &temp,
        &serde_json::json!({
            "abiVersion": 34,
            "fsRoots": {"data": data},
            "capabilities": kv_grants(),
        }),
    );
    let launch = prepare(&temp, &artifact, Some(&ceiling)).unwrap();
    assert_eq!(launch.file_roots().collect::<Vec<_>>(), ["data"]);
    let (_, lines) = run(launch);
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            "kv before missing",
            "kv after stored",
            DONE
        ]
    );
    // The database is in the directory the ceiling names, and the shell
    // created no root of its own (only its consent files, if any, under the
    // artifact's directory).
    assert!(data.join(DATABASE).is_file());
    assert_only_consent_files(&temp, &artifact);
}

#[test]
fn storage_is_created_only_for_roots_the_ceiling_can_open() {
    let temp = Temp::new();
    let artifact = package(&temp, serde_json::json!(kv_grants()));
    // A `kv.read` anywhere is not enough: this one names another root.
    let ceiling = policy(
        &temp,
        &serde_json::json!({
            "abiVersion": 34,
            "capabilities": [
                {"id": "kv.read", "patterns": ["/config/app.kv"]},
                {"id": "kv.write", "patterns": ["/data/app.kv"]},
            ],
        }),
    );
    let launch = prepare(&temp, &artifact, Some(&ceiling)).unwrap();
    assert_eq!(launch.file_roots().count(), 0);
    let (_, lines) = run(launch);
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            KV_DENIED,
            "kv NotAllowedError",
            DONE
        ]
    );
    assert_only_consent_files(&temp, &artifact);
}

#[test]
fn an_empty_ceiling_denies_everything_and_never_falls_back() {
    let temp = Temp::new();
    let server = Loopback::start();
    let mut request = url_grants(&server.url(ROUTE)).to_vec();
    request.extend(kv_grants());
    let artifact = package(&temp, serde_json::json!(request));
    let ceiling = ceiling_with_url(&temp, &server.url(ROUTE), serde_json::json!([]));
    let launch = prepare(&temp, &artifact, Some(&ceiling)).unwrap();
    assert_eq!(launch.ceiling(), Some(&[][..]));
    let (_, lines) = run(launch);
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            KV_DENIED,
            "kv NotAllowedError",
            DONE
        ]
    );
    assert_eq!(server.accepted(), 0);
    assert_only_consent_files(&temp, &artifact);
}

#[test]
fn grants_match_the_exact_operation() {
    let temp = Temp::new();
    let artifact = package(&temp, serde_json::json!(kv_grants()));
    // `kv.write` never opens a database, so a ceiling of `kv.write` alone
    // admits nothing; no prefix of an id matches another.
    let ceiling = policy(
        &temp,
        &serde_json::json!({
            "abiVersion": 34,
            "capabilities": [{"id": "kv.write", "patterns": ["/data/**"]}],
        }),
    );
    let (_, lines) = run(prepare(&temp, &artifact, Some(&ceiling)).unwrap());
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            KV_DENIED,
            "kv NotAllowedError",
            DONE
        ]
    );
    assert_only_consent_files(&temp, &artifact);
    // A ceiling never names an operation the registry does not know.
    let wildcard = policy(
        &temp,
        &serde_json::json!({"capabilities": [{"id": "kv.*", "patterns": ["/data/**"]}]}),
    );
    assert!(matches!(
        prepare(&temp, &artifact, Some(&wildcard)),
        Err(ShellError::Policy { .. })
    ));
}

#[test]
fn optional_grants_are_denied_without_a_prompt() {
    let temp = Temp::new();
    let artifact = package(
        &temp,
        serde_json::json!([
            {"id": "kv.read", "patterns": ["/data/app.kv"], "required": false},
            {"id": "kv.write", "patterns": ["/data/app.kv"], "required": false},
        ]),
    );
    let (_, lines) = run(prepare(&temp, &artifact, None).unwrap());
    assert_eq!(
        lines,
        [
            URL_DENIED,
            "net no-url",
            KV_DENIED,
            "kv NotAllowedError",
            DONE
        ]
    );
    assert!(!database_of(&temp, &artifact).exists());
}

#[test]
fn invalid_host_ceilings_and_storage_bases_are_typed_errors() {
    let temp = Temp::new();
    let artifact = package(&temp, serde_json::json!(kv_grants()));
    let malformed = temp.path("malformed.json");
    std::fs::write(&malformed, "{bad").unwrap();
    for ceiling in [
        malformed,
        temp.path("missing.json"),
        // Consent metadata belongs to the package, never the ceiling.
        policy(
            &temp,
            &serde_json::json!({"abiVersion": 34, "capabilities": [
                {"id": "kv.read", "patterns": ["/data/**"], "required": true}]}),
        ),
    ] {
        assert!(
            matches!(
                prepare(&temp, &artifact, Some(&ceiling)),
                Err(ShellError::Policy { .. })
            ),
            "{ceiling:?}"
        );
    }
    // A ceiling lists root names only in a package.
    let names = policy(
        &temp,
        &serde_json::json!({"abiVersion": 34, "fsRoots": ["data"], "capabilities": []}),
    );
    assert!(matches!(
        prepare(&temp, &artifact, Some(&names)),
        Err(ShellError::Policy { .. })
    ));
    assert!(matches!(
        Launch::packaged(&Package {
            artifact: &artifact,
            policy: None,
            storage: Path::new("relative/storage"),
            font: None,
        }),
        Err(ShellError::StorageNotAbsolute(_))
    ));
    assert!(!temp.storage().exists());
}

/// Prepare and, when preparation succeeds, start `artifact`; nothing of the
/// guest may run.
fn assert_refused_before_any_guest_effect(
    temp: &Temp,
    artifact: &Path,
    server: &Loopback,
    expected: Refusal,
) {
    let ceiling = ceiling_with_url(temp, &server.url(ROUTE), serde_json::json!([]));
    let refusal = match prepare(temp, artifact, Some(&ceiling)) {
        Err(ShellError::Artifact { .. }) => Refusal::Package,
        Err(other) => panic!("{artifact:?}: unexpected error {other}"),
        Ok(launch) => match launch.start(layout(), effects()) {
            Err(ShellError::Runtime(_)) => Refusal::Start,
            Err(other) => panic!("{artifact:?}: unexpected error {other}"),
            Ok(mut app) => panic!("{artifact:?} started: {:?}", app.take_output()),
        },
    };
    assert_eq!(refusal, expected, "{artifact:?}");
    assert_eq!(server.accepted(), 0);
}

/// Where a refused artifact must stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// Reading the package (`varta-artifact`), before grants are resolved.
    Package,
    /// The runtime's validation at start, before instantiation.
    Start,
}

#[test]
fn tampered_truncated_and_foreign_artifacts_are_refused_before_any_guest_effect() {
    let temp = Temp::new();
    let server = Loopback::start();
    let mut request = url_grants(&server.url(ROUTE)).to_vec();
    request.extend(kv_grants());
    let wasm = &probe().wasm;
    let valid = Artifact {
        wasm: wasm.clone(),
        manifest: manifest(serde_json::json!(request)),
    };
    // The fixed-footer container behind an arbitrary native prefix is read
    // from its footer, exactly as the launcher reads it.
    let prefix = temp.path("prefix.bin");
    std::fs::write(&prefix, b"native prefix").unwrap();
    let container = temp.path("app.bin");
    valid.write_executable(&container, &prefix).unwrap();
    assert!(prepare(&temp, &container, None).is_ok());
    let bytes = std::fs::read(&container).unwrap();
    let footer = bytes.len() - FOOTER_BYTES;

    let tampered_lengths = temp.path("tampered-lengths.bin");
    let mut data = bytes.clone();
    data[footer..footer + size_of::<u64>()].copy_from_slice(&u64::MAX.to_le_bytes());
    std::fs::write(&tampered_lengths, data).unwrap();

    let tampered_magic = temp.path("tampered-magic.bin");
    let mut data = bytes.clone();
    let last = data.len() - 1;
    data[last] ^= 1;
    std::fs::write(&tampered_magic, data).unwrap();

    let truncated_container = temp.path("truncated.bin");
    std::fs::write(&truncated_container, &bytes[..bytes.len() - 1]).unwrap();

    // A bare module whose manifest claims another host ABI.
    let abi = temp.path("abi.wasm");
    std::fs::write(&abi, wasm).unwrap();
    let mut wrong = probe_manifest();
    wrong["abiVersion"] = serde_json::json!(1);
    std::fs::write(
        abi.with_extension("manifest.json"),
        serde_json::to_vec(&wrong).unwrap(),
    )
    .unwrap();

    // A bare module cut short, and one with a damaged code body.
    let truncated_wasm = temp.path("truncated.wasm");
    valid.write_wasm(&truncated_wasm).unwrap();
    std::fs::write(&truncated_wasm, &wasm[..wasm.len() / 2]).unwrap();
    let damaged = temp.path("damaged.wasm");
    valid.write_wasm(&damaged).unwrap();
    std::fs::write(&damaged, damage_first_function(wasm)).unwrap();

    // Foreign payloads: arbitrary bytes, and a valid Wasm module that is not
    // a v38 application.
    let foreign = temp.path("foreign.bin");
    std::fs::write(&foreign, b"\x7fELF not a varta artifact at all, no footer").unwrap();
    let empty_module = temp.path("empty.wasm");
    Artifact {
        wasm: b"\0asm\x01\0\0\0".to_vec(),
        manifest: Manifest::new(&[]),
    }
    .write_wasm(&empty_module)
    .unwrap();

    for (artifact, refusal) in [
        (tampered_lengths, Refusal::Package),
        (tampered_magic, Refusal::Package),
        (truncated_container, Refusal::Package),
        (abi, Refusal::Package),
        (truncated_wasm, Refusal::Package),
        (damaged, Refusal::Start),
        (foreign, Refusal::Package),
        (empty_module, Refusal::Start),
        (temp.path("missing.wasm"), Refusal::Package),
    ] {
        assert_refused_before_any_guest_effect(&temp, &artifact, &server, refusal);
    }
    // Preparing the valid container without a ceiling created its storage
    // directories (they precede the runtime's code validation); every
    // refused artifact had an empty ceiling, so it created none, and no
    // guest wrote anything.
    assert_eq!(files_under(&temp.storage()), Vec::<PathBuf>::new());
}

/// Every non-directory entry below `path`.
fn files_under(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    entries
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}
/// The container magic's length.
const MAGIC_BYTES: usize = 8;
/// The fixed container footer: two `u64` lengths, two `u32` words, the magic.
const FOOTER_BYTES: usize = 2 * size_of::<u64>() + 2 * size_of::<u32>() + MAGIC_BYTES;
/// The Wasm module header: magic and version.
const WASM_HEADER_BYTES: usize = 8;
/// The Wasm code section id.
const CODE_SECTION: u8 = 10;
const LEB_CONTINUATION: u8 = 0x80;
const LEB_PAYLOAD: u8 = 0x7f;
const LEB_SHIFT: u32 = 7;

/// An unsigned LEB128 value at `at` and the offset after it.
fn leb(bytes: &[u8], mut at: usize) -> (usize, usize) {
    let (mut value, mut shift) = (0usize, 0);
    loop {
        let byte = bytes[at];
        at += 1;
        value |= usize::from(byte & LEB_PAYLOAD) << shift;
        if byte & LEB_CONTINUATION == 0 {
            return (value, at);
        }
        shift += LEB_SHIFT;
    }
}

/// `wasm` with the body of its first function overwritten by bytes that
/// cannot decode (an unterminated local count), keeping every size intact.
fn damage_first_function(wasm: &[u8]) -> Vec<u8> {
    let mut at = WASM_HEADER_BYTES;
    while at < wasm.len() {
        let id = wasm[at];
        let (size, start) = leb(wasm, at + 1);
        if id == CODE_SECTION {
            let (_, first) = leb(wasm, start);
            let (body, body_start) = leb(wasm, first);
            let mut damaged = wasm.to_vec();
            damaged[body_start..body_start + body].fill(u8::MAX);
            return damaged;
        }
        at = start + size;
    }
    panic!("the probe has no code section")
}

#[test]
fn the_shell_core_renders_the_first_frame_without_a_window() {
    let temp = Temp::new();
    let artifact = package(&temp, serde_json::json!([]));
    let (app, lines) = run(prepare(&temp, &artifact, None).unwrap());
    assert_eq!(lines.last().map(String::as_str), Some(DONE));
    assert_eq!(app.view_title(), "Varta shell probe");
    assert!(app.view_commits() > 0);
    let mut renderer = varta_view::Renderer::new(None, ROOT_FONT_SIZE).unwrap();
    let frame = app
        .view_render(&mut renderer, VIEW_WIDTH, VIEW_HEIGHT, 1.0)
        .unwrap();
    assert_eq!((frame.width, frame.height), (VIEW_WIDTH, VIEW_HEIGHT));
    let pixel = |x: u32, y: u32| {
        let at = ((y * frame.width + x) * 4) as usize;
        [
            frame.pixels[at],
            frame.pixels[at + 1],
            frame.pixels[at + 2],
            frame.pixels[at + 3],
        ]
    };
    assert_eq!(pixel(PANEL.0 / 2, PANEL.1 / 2), GREEN);
    assert_ne!(pixel(PANEL.0 + PANEL.0 / 2, PANEL.1 + PANEL.1 / 2), GREEN);
}

/// The view probe's sans family (regular, italic and bold faces) and its
/// monospace family, as its configuration declares them.
const SANS: &str = "Varta Fixture 400";
const MONO: &str = "Varta Fixture 900";
/// The fixture faces' `A` advance in em: the regular face is narrow, the
/// bold, italic and monospace faces are wide.
const NARROW_EM: f32 = 0.4;
const WIDE_EM: f32 = 0.9;
const TEXT_SIZE: f32 = 20.0;
/// Layout tolerance of an advance, in CSS pixels.
const ADVANCE_TOLERANCE: f32 = 0.01;
/// The probe's copy text, link and the results it logs.
const GESTURE_COPY: &str = "copied by a gesture";
const LINK: &str = "https://docs.test/guide";
const MAIN_COPY_INVALID_CONTEXT: &str = "main copy 3";
fn click_copy(status: u32) -> String {
    format!("click copy {status}")
}
fn open_url(status: u32) -> String {
    format!("open_url {status}")
}
/// `command_result` statuses ([`varta_common::view::CommandStatus`]).
const OK: u32 = 0;
const NOT_ALLOWED: u32 = 1;
const REFUSED_BY_PLATFORM: u32 = 2;

/// The view probe packaged with `capabilities` as its request: its Wasm,
/// its built manifest with that request, and its resource section.
fn package_view(temp: &Temp, capabilities: serde_json::Value) -> PathBuf {
    let built = view_probe();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&built.artifact.manifest.to_bytes().unwrap()).unwrap();
    manifest["capabilities"] = capabilities;
    // The shell probe's file roots, so a request may also name KV paths.
    manifest["fsRoots"] = probe_manifest()["fsRoots"].clone();
    let path = temp.path("view.wasm");
    Artifact {
        wasm: built.artifact.wasm.clone(),
        manifest: Manifest::parse(&serde_json::to_vec(&manifest).unwrap()).unwrap(),
    }
    .write_wasm_with(&path, &built.resources)
    .unwrap();
    path
}
fn view_grants() -> serde_json::Value {
    serde_json::json!([
        {"id": "ui.open_url", "patterns": ["https://docs.test/**"]},
        {"id": "clipboard.write", "enabled": true},
    ])
}

/// Start the view probe with `fonts` laid out and `sink` as its platform,
/// and step it through main.
fn start_view(launch: Launch, sink: Arc<dyn PlatformEffects>) -> (Application, Vec<String>) {
    let font = launch.fonts().map(|fonts| fonts.measurer().unwrap());
    let layout =
        ViewLayout::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32, font, ROOT_FONT_SIZE).unwrap();
    let mut app = launch.start(layout, sink).unwrap();
    let mut lines = Vec::new();
    settle(&mut app, &mut lines, MAIN_COPY_INVALID_CONTEXT);
    (app, lines)
}

/// Step `app` until it logged `expected`.
fn settle(app: &mut Application, lines: &mut Vec<String>, expected: &str) {
    let deadline = Instant::now() + PROBE_DEADLINE;
    loop {
        lines.extend(
            app.take_output()
                .into_iter()
                .map(|line| line.strip_prefix(LOG).unwrap_or(&line).to_owned()),
        );
        if lines.iter().any(|line| line == expected) {
            return;
        }
        assert!(Instant::now() < deadline, "no {expected:?}: {lines:?}");
        match app.step(Some(STEP_WAIT)) {
            Step::Turn(_) | Step::Idle | Step::Finished { .. } => {}
            Step::Failed => panic!("view probe failed: {:?} {lines:?}", app.failure()),
        }
    }
}

/// A trusted click on the element with `id`.
fn click(app: &mut Application, id: &str) {
    let clicked = app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Ident(format!("#{id}")),
    });
    assert!(clicked.is_ok(), "{id}: {:?}", app.failure());
}

/// A broker that grants nothing, for layout-only views.
struct NoGrants;
impl varta_view::Broker for NoGrants {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> varta_view::Access {
        varta_view::Access::Denied
    }
    fn clipboard_write(&mut self) -> varta_view::Access {
        varta_view::Access::Denied
    }
}

/// One `A` per face style: regular, bold, italic and the monospace generic
/// family, each a span of one 20px line.
fn face_view() -> (varta_view::View, Vec<u32>) {
    use varta_common::view::{CssProperty as P, Tag};
    const LINE: u32 = 2;
    const FIRST_SPAN: u32 = 10;
    const FIRST_TEXT: u32 = 20;
    let styles: [Option<(P, &str)>; 4] = [
        None,
        Some((P::FontWeight, "700")),
        Some((P::FontStyle, "italic")),
        Some((P::FontFamily, "monospace")),
    ];
    let mut batch = varta_view::BatchWriter::new();
    batch
        .create_element(LINE, Tag::Div)
        .insert(1, LINE, 0)
        .set_style(LINE, P::FontSize, "20px")
        .set_style(LINE, P::LineHeight, "20px")
        .set_style(LINE, P::Color, "#000");
    let mut spans = Vec::new();
    for (index, style) in (0u32..).zip(styles) {
        let (span, text) = (FIRST_SPAN + index, FIRST_TEXT + index);
        batch
            .create_element(span, Tag::Span)
            .insert(LINE, span, 0)
            .create_text(text, "A")
            .insert(span, text, 0);
        if let Some((property, value)) = style {
            batch.set_style(span, property, value);
        }
        spans.push(span);
    }
    let mut view = varta_view::View::default();
    view.commit(&batch.finish(), false, &mut NoGrants).unwrap();
    (view, spans)
}

#[test]
fn artifact_fonts_are_packaged_read_and_selected_by_weight_and_style() {
    let temp = Temp::new();
    // `varta build` wrote the four declared faces in canonical order: the
    // first declared family first (the default), each family's faces by
    // weight and style.
    let faces: Vec<_> = view_probe()
        .resources
        .fonts()
        .iter()
        .map(|face| (face.family.as_str(), face.weight, face.style.name()))
        .collect();
    assert_eq!(
        faces,
        [
            (SANS, 400, "normal"),
            (SANS, 400, "italic"),
            (SANS, 700, "normal"),
            (MONO, 400, "normal"),
        ]
    );
    let artifact = package_view(&temp, view_grants());
    let launch = prepare(&temp, &artifact, None).unwrap();
    let fonts = launch.fonts().unwrap();
    assert_eq!(fonts.len(), faces.len());
    assert_eq!(
        fonts.families().last(),
        Some((MONO, ["monospace".to_owned()].as_slice()))
    );
    // Real faces: bold, italic and the monospace generic each select their
    // own wide face; the default family's regular face is narrow.
    let (view, spans) = face_view();
    let mut measurer = fonts.measurer().unwrap();
    let boxes = view
        .layout_boxes_with_text(
            VIEW_WIDTH as f32,
            VIEW_HEIGHT as f32,
            &mut measurer,
            TEXT_SIZE,
        )
        .unwrap();
    let widths: Vec<f32> = spans.iter().map(|span| boxes[span].width).collect();
    for (width, em) in widths.iter().zip([NARROW_EM, WIDE_EM, WIDE_EM, WIDE_EM]) {
        assert!(
            (width - em * TEXT_SIZE).abs() < ADVANCE_TOLERANCE,
            "{widths:?}"
        );
    }
    assert!(
        view.rasterize_scaled(
            VIEW_WIDTH,
            VIEW_HEIGHT,
            1.0,
            Some((&mut measurer, TEXT_SIZE))
        )
        .is_ok()
    );
    // With only the regular face, bold and italic would need synthesis,
    // which stays refused at paint.
    let regular = FontSet::single(view_probe().resources.fonts()[0].data.clone(), SANS.into());
    let mut regular = regular.measurer().unwrap();
    assert!(
        view.rasterize_scaled(
            VIEW_WIDTH,
            VIEW_HEIGHT,
            1.0,
            Some((&mut regular, TEXT_SIZE))
        )
        .is_err()
    );
}

#[test]
fn the_view_probe_renders_bold_italic_and_mono_text_from_its_artifact() {
    let temp = Temp::new();
    let artifact = package_view(&temp, view_grants());
    let launch = prepare(&temp, &artifact, None).unwrap();
    let render_font = launch.fonts().map(|fonts| fonts.measurer().unwrap());
    let (app, lines) = start_view(launch, effects());
    assert!(lines.contains(&MAIN_COPY_INVALID_CONTEXT.to_owned()));
    let mut renderer = varta_view::Renderer::new(render_font, ROOT_FONT_SIZE).unwrap();
    let frame = app
        .view_render(&mut renderer, VIEW_WIDTH, VIEW_HEIGHT, 1.0)
        .unwrap();
    // Some text ink was painted.
    assert!(
        frame
            .pixels
            .chunks(4)
            .any(|rgba| rgba[3] != 0 && rgba[..3] == [0, 0, 0])
    );
}

#[test]
fn granted_gesture_effects_reach_the_platform_and_report_its_answer() {
    for (refuse, status) in [(false, OK), (true, REFUSED_BY_PLATFORM)] {
        let temp = Temp::new();
        let artifact = package_view(&temp, view_grants());
        let sink = Arc::new(Recording {
            refuse,
            ..Recording::default()
        });
        let (mut app, mut lines) =
            start_view(prepare(&temp, &artifact, None).unwrap(), sink.clone());
        // The copy in main is not a gesture: `invalid_context`, no effect.
        assert!(sink.clipboard().is_empty());
        click(&mut app, "copy");
        settle(&mut app, &mut lines, &click_copy(status));
        assert_eq!(sink.clipboard(), [GESTURE_COPY]);
        click(&mut app, "docs");
        settle(&mut app, &mut lines, &open_url(status));
        assert_eq!(sink.opened(), [LINK]);
    }
}

#[test]
fn denied_effects_never_reach_the_platform() {
    let temp = Temp::new();
    // The request admits both, the ceiling neither: an empty ceiling
    // denies everything.
    let artifact = package_view(&temp, view_grants());
    let ceiling = policy(
        &temp,
        &serde_json::json!({"abiVersion": 34, "capabilities": []}),
    );
    let sink = Arc::new(Recording::default());
    let (mut app, mut lines) = start_view(
        prepare(&temp, &artifact, Some(&ceiling)).unwrap(),
        sink.clone(),
    );
    click(&mut app, "copy");
    settle(&mut app, &mut lines, &click_copy(NOT_ALLOWED));
    click(&mut app, "docs");
    settle(&mut app, &mut lines, &open_url(NOT_ALLOWED));
    // A request without the clipboard grant (the link pattern stays: an
    // external href must match the request at patch time).
    let artifact = package_view(
        &temp,
        serde_json::json!([{"id": "ui.open_url", "patterns": ["https://docs.test/**"]}]),
    );
    let (mut app, mut lines) = start_view(prepare(&temp, &artifact, None).unwrap(), sink.clone());
    click(&mut app, "copy");
    settle(&mut app, &mut lines, &click_copy(NOT_ALLOWED));
    assert!(sink.clipboard().is_empty());
    assert!(sink.opened().is_empty());
}

#[test]
fn tampered_oversized_and_malformed_fonts_are_refused_before_guest_start() {
    let temp = Temp::new();
    // KV grants: a launch that got past its fonts would create storage.
    let grants = serde_json::json!(kv_grants());
    let artifact = package_view(&temp, grants.clone());
    let sidecar = artifact.with_extension("resources");
    let section = std::fs::read(&sidecar).unwrap();
    let refused = |expected: fn(&ShellError) -> bool, font: Option<&FontSet>| {
        let result = Launch::packaged(&Package {
            artifact: &artifact,
            policy: None,
            storage: &temp.storage(),
            font,
        });
        match result {
            Err(error) => assert!(expected(&error), "{error}"),
            Ok(_) => panic!("the launch was prepared"),
        }
        assert_eq!(files_under(&temp.storage()), Vec::<PathBuf>::new());
        assert!(!temp.storage().exists());
    };
    let artifact_error = |error: &ShellError| matches!(error, ShellError::Artifact { .. });
    // A flipped font byte, a section packaged with another payload and an
    // oversized section.
    let mut tampered = section.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    std::fs::write(&sidecar, &tampered).unwrap();
    refused(artifact_error, None);
    let foreign = view_probe().resources.encode(&probe().wasm).unwrap();
    std::fs::write(&sidecar, &foreign).unwrap();
    refused(artifact_error, None);
    std::fs::write(
        &sidecar,
        vec![0; varta_artifact::MAX_RESOURCE_SECTION_BYTES + 1],
    )
    .unwrap();
    refused(artifact_error, None);
    // The same tampering inside a format 2 container.
    std::fs::write(&sidecar, &section).unwrap();
    let prefix = temp.path("prefix.bin");
    std::fs::write(&prefix, b"native prefix").unwrap();
    let container = temp.path("view.bin");
    Artifact::read(&artifact)
        .unwrap()
        .write_executable_with(&container, &prefix, &view_probe().resources)
        .unwrap();
    let mut bytes = std::fs::read(&container).unwrap();
    let in_section = bytes.len() - FOOTER_BYTES - size_of::<u64>() - 1;
    bytes[in_section] ^= 1;
    std::fs::write(&container, bytes).unwrap();
    assert!(matches!(
        prepare(&temp, &container, None),
        Err(ShellError::Artifact { .. })
    ));
    assert!(!temp.storage().exists());
    // `--font` is for artifacts without fonts, and is parsed before storage.
    let explicit = FontSet::single(view_probe().resources.fonts()[0].data.clone(), SANS.into());
    refused(
        |error| matches!(error, ShellError::FontConflict),
        Some(&explicit),
    );
    let plain = package(&temp, grants);
    let malformed = FontSet::single(b"not a font at all".to_vec(), SANS.into());
    let launch = Launch::packaged(&Package {
        artifact: &plain,
        policy: None,
        storage: &temp.storage(),
        font: Some(&malformed),
    });
    assert!(matches!(launch, Err(ShellError::Fonts(_))));
    assert!(!temp.storage().exists());
    // An unchanged section still prepares (and so creates storage).
    let unchanged = Resources::decode(&section, &probe_wasm_of(&artifact)).unwrap();
    assert_eq!(&unchanged, &view_probe().resources);
    assert!(prepare(&temp, &artifact, None).unwrap().fonts().is_some());
    assert!(temp.storage().exists());
}

fn probe_wasm_of(artifact: &Path) -> Vec<u8> {
    Artifact::read(artifact).unwrap().wasm
}

// Native consent (P10b) without a window: a scripted decision provider
// stands in for the host prompt window and answers the broker's prompts.

/// The consent probe's required and optional grants and their purposes.
const REQUIRED_GRANT: &str = "kv.write";
const OPTIONAL_GRANT: &str = "kv.read";
const REQUIRED_PURPOSE: &str = "Save your notes on this device";
const OPTIONAL_PURPOSE: &str = "Read your saved notes";
const DATABASE_PATH: &str = "/data/app.kv";
/// The consent probe's last line of main.
const READY: &str = "probe ready";

/// A decision provider that answers from a script and records every prompt
/// it was shown, and every request the consent seam refused to show; an
/// unscripted prompt is dismissed.
#[derive(Clone, Default)]
struct Scripted {
    answers: Arc<Mutex<VecDeque<PromptAnswer>>>,
    shown: Arc<Mutex<Vec<PermissionPrompt>>>,
    refused: Arc<Mutex<Vec<String>>>,
    /// How long the "user" takes to answer.
    delay: Duration,
}
impl Scripted {
    fn new(answers: &[PromptAnswer]) -> Self {
        let scripted = Self::default();
        scripted.answer(answers);
        scripted
    }
    fn answer(&self, answers: &[PromptAnswer]) {
        self.answers.lock().unwrap().extend(answers);
    }
    fn shown(&self) -> Vec<PermissionPrompt> {
        self.shown.lock().unwrap().clone()
    }
    /// The host diagnostics of requests refused without a prompt.
    fn refused(&self) -> Vec<String> {
        self.refused.lock().unwrap().clone()
    }
    /// The host prompt `prompter` stands in for, reporting refusals here.
    fn host(&self, prompter: Box<dyn varta_runtime::PermissionPrompter>) -> HostPrompt {
        let refused = self.refused.clone();
        HostPrompt {
            prompter,
            unpresentable: Box::new(move |error| refused.lock().unwrap().push(error.to_string())),
        }
    }
    /// `(capability, required)` of every prompt shown.
    fn asked(&self) -> Vec<(String, bool)> {
        self.shown()
            .into_iter()
            .map(|prompt| (prompt.capability, prompt.required))
            .collect()
    }
}
impl varta_runtime::PermissionPrompter for Scripted {
    fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer {
        self.shown.lock().unwrap().push(prompt.clone());
        std::thread::sleep(self.delay);
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(PromptAnswer::Dismiss)
    }
}

/// Write the consent probe as a packaged artifact, with `edit` applied to
/// its built manifest.
fn package_consent(temp: &Temp, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&consent_probe().manifest.to_bytes().unwrap()).unwrap();
    edit(&mut manifest);
    let path = temp.path("consent.wasm");
    Artifact {
        wasm: consent_probe().wasm.clone(),
        manifest: Manifest::parse(&serde_json::to_vec(&manifest).unwrap()).unwrap(),
    }
    .write_wasm(&path)
    .unwrap();
    path
}

fn start_consent(temp: &Temp, artifact: &Path, scripted: &Scripted) -> Result<Started, ShellError> {
    prepare(temp, artifact, None)?.start_with(
        layout(),
        effects(),
        Some(scripted.host(Box::new(scripted.clone()))),
    )
}

/// Step the instance, syncing its consent state between turns as the
/// window's worker does, until a line after the first `from` lines is
/// `expected`.
fn settle_synced(started: &mut Started, lines: &mut Vec<String>, from: usize, expected: &str) {
    let deadline = Instant::now() + PROBE_DEADLINE;
    let consent = started.consent.as_mut().unwrap();
    let app = &mut started.application;
    loop {
        consent.sync(app).unwrap();
        lines.extend(
            app.take_output()
                .into_iter()
                .map(|line| line.strip_prefix(LOG).unwrap_or(&line).to_owned()),
        );
        if lines[from.min(lines.len())..]
            .iter()
            .any(|line| line == expected)
        {
            return;
        }
        assert!(Instant::now() < deadline, "no {expected:?}: {lines:?}");
        match app.step(Some(STEP_WAIT)) {
            Step::Turn(_) | Step::Idle | Step::Finished { .. } => {}
            Step::Failed => panic!("consent probe failed: {:?} {lines:?}", app.failure()),
        }
    }
}

/// Click `id` and settle until a new line is `expected`.
fn click_until(started: &mut Started, lines: &mut Vec<String>, id: &str, expected: &str) {
    let from = lines.len();
    click(&mut started.application, id);
    settle_synced(started, lines, from, expected);
}

fn store_of(temp: &Temp, artifact: &Path) -> ConsentStore {
    let wasm = Artifact::read(artifact).unwrap().wasm;
    ConsentStore::new(&temp.storage(), &varta_runtime::artifact_identity(&wasm)).unwrap()
}

fn decisions(entries: &[(&str, StoredDecision)]) -> BTreeMap<String, StoredDecision> {
    entries
        .iter()
        .map(|(id, decision)| ((*id).to_owned(), *decision))
        .collect()
}

/// The audit log's rows.
fn audit_rows(store: &ConsentStore) -> Vec<serde_json::Value> {
    std::fs::read_to_string(store.audit_path())
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
/// Whether a row records `operation` on `resource` from `source` with
/// `outcome`.
fn audited(
    rows: &[serde_json::Value],
    operation: &str,
    resource: &str,
    source: &str,
    outcome: &str,
) -> bool {
    rows.iter().any(|row| {
        row["operation"] == operation
            && row["resource"] == resource
            && row["source"] == source
            && row["outcome"] == outcome
    })
}

fn required_denied(result: Result<Started, ShellError>) -> bool {
    matches!(
        result,
        Err(ShellError::Runtime(varta_runtime::RuntimeError::Permission(
            PermissionError::RequiredDenied { ref id, .. }
        ))) if id == REQUIRED_GRANT
    )
}

#[test]
fn first_run_asks_each_required_grant_once_before_any_guest_code() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    // Only the required grant was asked, with the broker's description:
    // the identity, the exact requested scope and the purpose as a quote.
    let shown = scripted.shown();
    assert_eq!(scripted.asked(), [(REQUIRED_GRANT.to_owned(), true)]);
    assert_eq!(shown[0].artifact, store.identity());
    assert_eq!(shown[0].scope, [DATABASE_PATH]);
    assert_eq!(shown[0].purpose.as_deref(), Some(REQUIRED_PURPOSE));
    // The answer is stored before main's turns finish.
    assert_eq!(
        store.load().unwrap(),
        decisions(&[(REQUIRED_GRANT, StoredDecision::Allow)])
    );
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    // Main's request is not a user gesture: no prompt, the optional grant
    // stays in prompt state and its operation is denied recoverably.
    assert_eq!(
        lines,
        ["start prompt", KV_DENIED, "start kv NotAllowedError", READY]
    );
    assert_eq!(scripted.shown().len(), 1);
    drop(started);
    // A later start of the same artifact is not asked again.
    let again = Scripted::default();
    let mut started = start_consent(&temp, &artifact, &again).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    assert!(again.shown().is_empty());
    assert_eq!(lines[0], "start prompt");
    // The first-run answer is audited with its source.
    let rows = audit_rows(&store);
    assert!(
        audited(&rows, REQUIRED_GRANT, "", "prompt", "granted"),
        "{rows:?}"
    );
    assert!(
        audited(&rows, REQUIRED_GRANT, "", "stored", "granted"),
        "{rows:?}"
    );
}

#[test]
fn a_refused_required_grant_stops_the_start_and_is_stored_unless_dismissed() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    // Deny: the start fails before any guest code and the denial is stored
    // and audited, so later starts fail without asking.
    let scripted = Scripted::new(&[PromptAnswer::Deny]);
    assert!(required_denied(start_consent(&temp, &artifact, &scripted)));
    assert_eq!(
        store.load().unwrap(),
        decisions(&[(REQUIRED_GRANT, StoredDecision::Deny)])
    );
    assert!(audited(
        &audit_rows(&store),
        REQUIRED_GRANT,
        "",
        "prompt",
        "denied"
    ));
    let again = Scripted::new(&[PromptAnswer::Allow]);
    assert!(required_denied(start_consent(&temp, &artifact, &again)));
    assert!(again.shown().is_empty());
    // A headless start (no prompt) honours the stored denial too.
    assert!(matches!(
        prepare(&temp, &artifact, None)
            .unwrap()
            .start(layout(), effects()),
        Err(ShellError::Runtime(
            varta_runtime::RuntimeError::Permission(_)
        ))
    ));
    // Dismiss refuses this start only: nothing is stored, the next start
    // asks again.
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Dismiss]);
    assert!(required_denied(start_consent(&temp, &artifact, &scripted)));
    assert!(store.load().unwrap().is_empty());
    let again = Scripted::new(&[PromptAnswer::Allow]);
    assert!(start_consent(&temp, &artifact, &again).is_ok());
    assert_eq!(again.asked(), [(REQUIRED_GRANT.to_owned(), true)]);
}

#[test]
fn a_request_that_cannot_be_shown_in_full_is_denied_without_a_prompt_and_audited() {
    use varta_common::{MAX_GRANT_PATTERNS, MAX_PATTERN_BYTES};
    // The required grant requests as many patterns of the longest length as
    // one grant holds: more lines than any prompt shows.
    const COMPONENT: usize = 100;
    let patterns: Vec<String> = (0..MAX_GRANT_PATTERNS)
        .map(|index| {
            let mut pattern = format!("/data/{index:02}");
            while pattern.len() + 1 + COMPONENT <= MAX_PATTERN_BYTES {
                pattern.push('/');
                pattern.push_str(&"n".repeat(COMPONENT));
            }
            pattern
        })
        .collect();
    let temp = Temp::new();
    let artifact = package_consent(&temp, |manifest| {
        for grant in manifest["capabilities"].as_array_mut().unwrap() {
            if grant["id"] == REQUIRED_GRANT {
                grant["patterns"] = serde_json::json!(patterns);
            }
        }
    });
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow]);
    assert!(required_denied(start_consent(&temp, &artifact, &scripted)));
    // Never shown, so the scripted "user" never answered it; the host was
    // told why, in words that say no user decided.
    assert!(scripted.shown().is_empty());
    let refused = scripted.refused();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].starts_with(&format!(
            "denied the {REQUIRED_GRANT} permission request without asking"
        )) && refused[0].contains("no user decided"),
        "{refused:?}"
    );
    assert_eq!(
        store.load().unwrap(),
        decisions(&[(REQUIRED_GRANT, StoredDecision::Deny)])
    );
    let rows = audit_rows(&store);
    assert!(
        rows.iter().any(|row| row["operation"] == REQUIRED_GRANT
            && row["refused"] == UNPRESENTABLE_PROMPT
            && row["outcome"] == "denied"
            && row["artifact"] == store.identity()
            && row["resource_lines"]
                .as_u64()
                .is_some_and(|lines| lines > MAX_RESOURCE_LINES as u64)
            && row["max_resource_lines"] == MAX_RESOURCE_LINES),
        "{rows:?}"
    );
    assert!(audited(&rows, REQUIRED_GRANT, "", "prompt", "denied"));
}

/// The consent probe's optional any-HTTPS link grant.
const LINK_GRANT: &str = varta_common::view::OPEN_URL_CAPABILITY;

#[test]
fn a_link_that_cannot_be_shown_in_full_is_denied_reported_audited_and_never_stored() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    // The link grant was allowed earlier; as the any-HTTPS form, each
    // activation still prompts with the full URL.
    store
        .save(&decisions(&[(LINK_GRANT, StoredDecision::Allow)]))
        .unwrap();
    let scripted = Scripted::new(&[PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    // The "user" would allow the link if it were shown; it is not.
    scripted.answer(&[PromptAnswer::Allow]);
    click_until(&mut started, &mut lines, "long", &open_url(NOT_ALLOWED));
    assert_eq!(scripted.asked(), [(REQUIRED_GRANT.to_owned(), true)]);
    let refused = scripted.refused();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].starts_with(&format!(
            "denied the {LINK_GRANT} permission request without asking"
        )),
        "{refused:?}"
    );
    // No Deny is stored for a link: the earlier decision stays.
    assert_eq!(
        store.load().unwrap(),
        decisions(&[
            (LINK_GRANT, StoredDecision::Allow),
            (REQUIRED_GRANT, StoredDecision::Allow),
        ])
    );
    let rows = audit_rows(&store);
    assert!(
        rows.iter().any(|row| row["operation"] == LINK_GRANT
            && row["refused"] == UNPRESENTABLE_PROMPT
            && row["outcome"] == "denied"
            && row["resource_lines"]
                .as_u64()
                .is_some_and(|lines| lines > MAX_RESOURCE_LINES as u64)),
        "{rows:?}"
    );
    assert!(
        rows.iter().any(|row| row["operation"] == LINK_GRANT
            && row["source"] == "prompt"
            && row["outcome"] == "denied"
            && row["resource"]
                .as_str()
                .is_some_and(|url| url.starts_with("https://docs.test/"))),
        "{rows:?}"
    );
}

/// A decision provider that answers like [`Scripted`] and, while asking
/// the prompt after an Allow, stores a revocation of the allowed grant from
/// outside the instance (`varta-shell ARTIFACT --revoke` while the prompt
/// is open).
struct RevokingWhileAsked {
    scripted: Scripted,
    revoke: Box<dyn FnMut(&str) + Send>,
    allowed: Option<String>,
}
impl varta_runtime::PermissionPrompter for RevokingWhileAsked {
    fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer {
        if let Some(id) = self.allowed.take() {
            (self.revoke)(&id);
        }
        let answer = self.scripted.prompt(prompt);
        if answer == PromptAnswer::Allow {
            self.allowed = Some(prompt.capability.clone());
        }
        answer
    }
}

#[test]
fn a_refused_start_never_stores_an_answer_over_a_revocation_made_while_asking() {
    let temp = Temp::new();
    // Both KV grants required: the first is allowed, the second denied,
    // and the first is revoked from outside while the second is asked.
    let artifact = package_consent(&temp, |manifest| {
        for grant in manifest["capabilities"].as_array_mut().unwrap() {
            if grant["id"] == OPTIONAL_GRANT {
                grant["required"] = true.into();
            }
        }
    });
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow, PromptAnswer::Deny]);
    let (revoked, storage) = (artifact.clone(), temp.storage());
    let prompter = RevokingWhileAsked {
        scripted: scripted.clone(),
        revoke: Box::new(move |id| {
            varta_desktop::shell::revoke(&revoked, &storage, id).unwrap();
        }),
        allowed: None,
    };
    let started = prepare(&temp, &artifact, None).unwrap().start_with(
        layout(),
        effects(),
        Some(scripted.host(Box::new(prompter))),
    );
    assert!(matches!(
        started,
        Err(ShellError::Runtime(
            varta_runtime::RuntimeError::Permission(PermissionError::RequiredDenied { .. })
        ))
    ));
    let asked = scripted.asked();
    assert_eq!(asked.len(), 2, "{asked:?}");
    let (allowed, denied) = (asked[0].0.as_str(), asked[1].0.as_str());
    // The revocation wins over the earlier Allow; the Deny is stored.
    assert_eq!(
        store.load().unwrap(),
        decisions(&[
            (allowed, StoredDecision::Deny),
            (denied, StoredDecision::Deny)
        ])
    );
    // The user's answers are still audited as given.
    let rows = audit_rows(&store);
    assert!(audited(&rows, allowed, "", "prompt", "granted"), "{rows:?}");
    assert!(audited(&rows, denied, "", "prompt", "denied"), "{rows:?}");
}

#[test]
fn ending_a_session_stores_and_audits_what_turns_decided_since_the_last_sync() {
    const TURN_FAILED: &str = "turn failed";
    const SYNC_FAILED: &str = "sync failed";
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow, PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    // A gesture's answer in turns that run without a sync, as when the
    // window closes or a turn fails right after it.
    click(&mut started.application, "ask");
    settle(&mut started.application, &mut lines, "ask kv ok");
    assert_eq!(store.load().unwrap().get(OPTIONAL_GRANT), None);
    let consent = started.consent.as_mut().unwrap();
    // The session failed: the last sync still runs and the failure, not
    // the sync's outcome, is what the session reports.
    assert_eq!(
        consent.end(&mut started.application, Err(TURN_FAILED), |_| SYNC_FAILED),
        Err(TURN_FAILED)
    );
    assert_eq!(
        store.load().unwrap(),
        decisions(&[
            (OPTIONAL_GRANT, StoredDecision::Allow),
            (REQUIRED_GRANT, StoredDecision::Allow),
        ])
    );
    assert!(audited(
        &audit_rows(&store),
        OPTIONAL_GRANT,
        "",
        "prompt",
        "granted"
    ));
    // A session that ended normally reports the sync's own outcome.
    assert_eq!(
        consent.end(&mut started.application, Ok(()), |_| SYNC_FAILED),
        Ok(())
    );
}

#[test]
fn optional_grants_are_asked_only_in_a_gesture_and_the_answer_persists() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    // A gesture that only uses the grant does not prompt.
    click_until(&mut started, &mut lines, "use", "use kv NotAllowedError");
    assert_eq!(scripted.shown().len(), 1);
    // A request in a click prompts once, between turns.
    scripted.answer(&[PromptAnswer::Allow]);
    click_until(&mut started, &mut lines, "ask", "ask kv ok");
    let shown = scripted.shown();
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[1].capability, OPTIONAL_GRANT);
    assert!(!shown[1].required);
    assert_eq!(shown[1].scope, [DATABASE_PATH]);
    assert_eq!(shown[1].purpose.as_deref(), Some(OPTIONAL_PURPOSE));
    assert!(lines.iter().any(|line| line == "ask granted"), "{lines:?}");
    assert_eq!(
        store.load().unwrap(),
        decisions(&[
            (OPTIONAL_GRANT, StoredDecision::Allow),
            (REQUIRED_GRANT, StoredDecision::Allow),
        ])
    );
    let rows = audit_rows(&store);
    assert!(
        audited(&rows, OPTIONAL_GRANT, "", "prompt", "granted"),
        "{rows:?}"
    );
    assert!(audited(
        &rows,
        OPTIONAL_GRANT,
        DATABASE_PATH,
        "policy",
        "denied"
    ));
    assert!(audited(
        &rows,
        OPTIONAL_GRANT,
        DATABASE_PATH,
        "stored",
        "granted"
    ));
    drop(started);
    // The stored allow applies at the next start, prompted or headless.
    let again = Scripted::default();
    let mut started = start_consent(&temp, &artifact, &again).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    assert_eq!(lines, ["start granted", "start kv ok", READY]);
    assert!(again.shown().is_empty());
    let mut app = prepare(&temp, &artifact, None)
        .unwrap()
        .start(layout(), effects())
        .unwrap();
    let mut lines = Vec::new();
    settle(&mut app, &mut lines, READY);
    assert_eq!(lines, ["start granted", "start kv ok", READY]);
}

#[test]
fn prompts_are_rate_limited_and_repeated_dismissals_become_a_stored_denial() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    // Two requests in one gesture open one prompt
    // (`permission::MAX_OPEN_PROMPTS`): the second resolves at once to
    // the current state.
    assert_eq!(varta_common::permission::MAX_OPEN_PROMPTS, 1);
    scripted.answer(&[PromptAnswer::Dismiss]);
    click_until(&mut started, &mut lines, "twice", "first prompt");
    assert!(
        lines.iter().any(|line| line == "second prompt"),
        "{lines:?}"
    );
    assert_eq!(scripted.shown().len(), 2);
    // `permission::MAX_DISMISSALS` dismissals become a stored denial; then
    // a gesture no longer prompts.
    let dismissals = varta_common::permission::MAX_DISMISSALS as usize;
    for remaining in (1..dismissals).rev() {
        scripted.answer(&[PromptAnswer::Dismiss]);
        let expected = if remaining == 1 {
            "ask denied"
        } else {
            "ask prompt"
        };
        click_until(&mut started, &mut lines, "ask", expected);
    }
    assert_eq!(scripted.shown().len(), 1 + dismissals);
    click_until(&mut started, &mut lines, "ask", "ask kv NotAllowedError");
    assert_eq!(scripted.shown().len(), 1 + dismissals);
    assert_eq!(store.load().unwrap()[OPTIONAL_GRANT], StoredDecision::Deny);
    drop(started);
    // The stored denial applies at the next start without a prompt.
    let again = Scripted::new(&[PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &again).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    click_until(&mut started, &mut lines, "ask", "ask denied");
    assert!(again.shown().is_empty());
}

#[test]
fn revocation_applies_to_new_operations_and_is_stored() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    let scripted = Scripted::new(&[PromptAnswer::Allow, PromptAnswer::Allow]);
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
    click_until(&mut started, &mut lines, "ask", "ask kv ok");
    click_until(&mut started, &mut lines, "use", "use kv ok");
    // Another process (`varta-shell ARTIFACT --revoke kv.read`) stores a
    // denial; the running instance applies it at its next sync.
    let identity =
        varta_desktop::shell::revoke(&artifact, &temp.storage(), OPTIONAL_GRANT).unwrap();
    assert_eq!(identity, store.identity());
    std::thread::sleep(STORE_POLL_INTERVAL);
    click_until(&mut started, &mut lines, "use", "use kv NotAllowedError");
    let rows = audit_rows(&store);
    assert!(
        audited(&rows, OPTIONAL_GRANT, "", "stored", "denied"),
        "{rows:?}"
    );
    assert!(audited(
        &rows,
        OPTIONAL_GRANT,
        DATABASE_PATH,
        "stored",
        "denied"
    ));
    // A revocation inside the instance is stored for later starts.
    assert!(started.application.revoke(REQUIRED_GRANT));
    started
        .consent
        .as_mut()
        .unwrap()
        .sync(&mut started.application)
        .unwrap();
    assert_eq!(
        store.load().unwrap(),
        decisions(&[
            (OPTIONAL_GRANT, StoredDecision::Deny),
            (REQUIRED_GRANT, StoredDecision::Deny),
        ])
    );
    drop(started);
    assert!(required_denied(start_consent(
        &temp,
        &artifact,
        &Scripted::default()
    )));
    // Only registered capabilities can be revoked.
    assert!(matches!(
        varta_desktop::shell::revoke(&artifact, &temp.storage(), "kv.everything"),
        Err(ShellError::UnknownCapability(_))
    ));
}

#[test]
fn audit_rows_carry_identity_run_and_ordered_sequences() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let store = store_of(&temp, &artifact);
    for _ in 0..2 {
        let scripted = Scripted::new(&[PromptAnswer::Allow]);
        let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
        let mut lines = Vec::new();
        settle_synced(&mut started, &mut lines, 0, READY);
    }
    let rows = audit_rows(&store);
    let mut runs: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for row in &rows {
        assert_eq!(row["artifact"], store.identity(), "{row}");
        runs.entry(row["run"].as_u64().unwrap())
            .or_default()
            .push(row["sequence"].as_u64().unwrap());
        assert!(row["at_ms"].is_u64());
    }
    // Two sessions (their run stamps may coincide within a millisecond),
    // each with strictly increasing sequences from 1.
    let sequences: Vec<u64> = runs.values().flatten().copied().collect();
    assert_eq!(
        sequences.iter().filter(|sequence| **sequence == 1).count(),
        2
    );
    assert!(
        rows.iter()
            .all(|row| row["source"].is_string() && row["outcome"].is_string())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in [DECISIONS_FILE, AUDIT_FILE] {
            let path = storage_of(&temp, &artifact).join(name);
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{name} must be private: {mode:o}");
        }
    }
}

#[test]
fn waiting_for_first_run_approval_spends_no_turn_deadline() {
    // A test-only per-turn deadline far shorter than the user's answer.
    const DEADLINE_MS: u64 = 200;
    const ANSWER_DELAY: Duration = Duration::from_millis(3 * DEADLINE_MS);
    let temp = Temp::new();
    let artifact = package_consent(&temp, |manifest| {
        manifest["budgets"] = serde_json::json!({"deadlinePerTurnMs": DEADLINE_MS});
    });
    let scripted = Scripted {
        delay: ANSWER_DELAY,
        ..Scripted::new(&[PromptAnswer::Allow])
    };
    let mut started = start_consent(&temp, &artifact, &scripted).unwrap();
    assert!(
        started.application.failure().is_none(),
        "{:?}",
        started.application.failure()
    );
    let mut lines = Vec::new();
    settle_synced(&mut started, &mut lines, 0, READY);
}

#[test]
fn malformed_or_foreign_decision_files_are_typed_errors_before_start() {
    let temp = Temp::new();
    let artifact = package_consent(&temp, |_| {});
    let directory = storage_of(&temp, &artifact);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join(DECISIONS_FILE);
    std::fs::write(&file, "{bad").unwrap();
    assert!(matches!(
        prepare(&temp, &artifact, None),
        Err(ShellError::Decisions { .. })
    ));
    // A file holding another application's decisions is refused, never
    // read as this one's.
    let other = "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    std::fs::write(
        &file,
        format!(r#"{{"applications":[{{"artifact":"{other}","decisions":[{{"id":"kv.read","decision":"allow"}}]}}]}}"#),
    )
    .unwrap();
    assert!(matches!(
        prepare(&temp, &artifact, None),
        Err(ShellError::Decisions { .. })
    ));
    std::fs::remove_file(&file).unwrap();
    assert!(prepare(&temp, &artifact, None).is_ok());
}

/// The image probe's log line once its view is committed.
const IMAGE_READY: &str = "image ready";
/// The image probe's `img` box: a 40x20 block at the origin.
const IMAGE_BOX: (u32, u32) = (40, 20);
const RED: [u8; 4] = [255, 0, 0, 255];

/// The image probe packaged with its built manifest and resource section.
fn package_image(temp: &Temp) -> PathBuf {
    let built = support::image_probe();
    let path = temp.path("image.wasm");
    built
        .artifact
        .write_wasm_with(&path, &built.resources)
        .unwrap();
    path
}

#[test]
fn packaged_images_are_decoded_laid_out_drawn_and_named() {
    let temp = Temp::new();
    let artifact = package_image(&temp);
    let launch = prepare(&temp, &artifact, None).unwrap();
    let images = launch.images().unwrap();
    assert_eq!(images.size("icons/red.png"), Some((1, 1)));
    let mut app = launch.start(layout(), effects()).unwrap();
    let mut lines = Vec::new();
    settle(&mut app, &mut lines, IMAGE_READY);
    let mut renderer = varta_view::Renderer::new(None, ROOT_FONT_SIZE).unwrap();
    let frame = app
        .view_render(&mut renderer, VIEW_WIDTH, VIEW_HEIGHT, 1.0)
        .unwrap();
    let pixel = |x: u32, y: u32| {
        let at = ((y * frame.width + x) * 4) as usize;
        [
            frame.pixels[at],
            frame.pixels[at + 1],
            frame.pixels[at + 2],
            frame.pixels[at + 3],
        ]
    };
    // The one red texel fills the content box; the rounded corner clips it
    // and nothing is drawn outside the box.
    assert_eq!(pixel(IMAGE_BOX.0 / 2, IMAGE_BOX.1 / 2), RED);
    assert_eq!(pixel(0, 0)[3], 0);
    assert_eq!(pixel(IMAGE_BOX.0 + 1, IMAGE_BOX.1 / 2)[3], 0);
    // The image is its own pointer target and is named by its `alt`.
    let target = frame
        .hit_test((IMAGE_BOX.0 / 2) as f32, (IMAGE_BOX.1 / 2) as f32)
        .unwrap();
    let tree = app.view_accessibility_tree().unwrap();
    let node = &tree.nodes[&target];
    assert_eq!(
        (node.role.as_str(), node.name.as_str()),
        ("image", "Red square")
    );
    // The GPU path has no image primitive: the frame is an explicit
    // software raster.
    let prepared = app
        .view_prepare(&mut renderer, VIEW_WIDTH, VIEW_HEIGHT, 1.0)
        .unwrap();
    assert!(matches!(prepared, varta_view::PreparedFrame::Raster(_)));
}

#[test]
fn an_artifact_without_the_named_image_refuses_the_guest_batch() {
    let temp = Temp::new();
    let built = support::image_probe();
    // The same guest without its image: the view refuses the batch that
    // names an unpackaged image, as the browser host does.
    let path = temp.path("bare.wasm");
    built.artifact.write_wasm(&path).unwrap();
    let launch = prepare(&temp, &path, None).unwrap();
    assert!(launch.images().unwrap().is_empty());
    let mut app = launch.start(layout(), effects()).unwrap();
    let deadline = Instant::now() + PROBE_DEADLINE;
    while !matches!(app.step(Some(STEP_WAIT)), Step::Failed) {
        assert!(Instant::now() < deadline, "the probe was not refused");
        assert!(
            !app.take_output()
                .iter()
                .any(|line| line.ends_with(IMAGE_READY)),
            "the batch was admitted"
        );
    }
    let failure = app
        .failure()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(failure.contains("not a packaged image"), "{failure}");
}
