//! The native shell core without a window: packaged artifact validation,
//! grant resolution against the host ceiling, denial before effect, a
//! loopback `net.request`, a KV round-trip in host-owned storage and a
//! rendered first frame.
//!
//! The guest is `fixtures/shell-probe`, compiled at test time by the
//! workspace's `varta` binary into a portable v38 artifact ([`support`]):
//! this crate never depends on the compiler, not even for tests. As in the
//! `varta-launcher` tests, each test writes its own packaged artifact with
//! `varta_artifact::Artifact` into a temporary directory, here with the
//! manifest (the package request) the test needs.
mod support;

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use support::probe;
use varta_artifact::Artifact;
use varta_common::Manifest;
use varta_desktop::shell::{
    Launch, Package, STORAGE_DIRECTORY_PREFIX, ShellError, storage_directory,
};
use varta_runtime::{Application, Step, ViewLayout};

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
    })
}

fn layout() -> ViewLayout {
    ViewLayout::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32, None, ROOT_FONT_SIZE).unwrap()
}

/// Start `launch` without a window and step it until the probe is done;
/// the instance and its guest log lines.
fn run(launch: Launch) -> (Application, Vec<String>) {
    let mut app = launch.start(layout()).unwrap();
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
    // The package requests no KV: nothing is created for it.
    assert!(!storage_of(&temp, &artifact).exists());
}

#[test]
fn kv_without_a_grant_is_denied_before_any_file_exists() {
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
    assert!(!storage_of(&temp, &artifact).exists());
    assert!(!temp.storage().exists());
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
    // created no per-artifact directory (here, not even the base).
    assert!(data.join(DATABASE).is_file());
    assert!(!storage_of(&temp, &artifact).exists());
    let artifact_directories = std::fs::read_dir(temp.storage())
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry.as_ref().is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(STORAGE_DIRECTORY_PREFIX)
            })
        })
        .count();
    assert_eq!(artifact_directories, 0);
    assert!(!temp.storage().exists());
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
    assert!(!temp.storage().exists());
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
    assert!(!temp.storage().exists());
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
    assert!(!temp.storage().exists());
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
        Ok(launch) => match launch.start(layout()) {
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
