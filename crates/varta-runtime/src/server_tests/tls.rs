//! K7 H3: native TLS 1.3 with ALPN `h2`/`http/1.1`, HTTP/2 bounds and
//! rapid-reset protection, and `net.listen` grants that name TLS endpoints.
use super::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use varta_common::{
    http::{HttpPolicy, TlsFiles},
    language::LanguageProfile as P,
};

const SERVICE: &str = r#"
async function handle(request: Request): Promise<Response> {
  const text = await request.text();
  return new Response(request.method + " " + request.url + " " + text);
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

/// A self-signed certificate for 127.0.0.1 in PEM files of a directory
/// removed on drop.
struct Certificate {
    directory: std::path::PathBuf,
    der: rustls::pki_types::CertificateDer<'static>,
}
impl Certificate {
    fn new(name: &str) -> Self {
        let certified =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into(), "localhost".into()])
                .unwrap();
        let directory = std::env::temp_dir().join(format!(
            "varta-tls-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("cert.pem"), certified.cert.pem()).unwrap();
        std::fs::write(
            directory.join("key.pem"),
            certified.key_pair.serialize_pem(),
        )
        .unwrap();
        Self {
            directory,
            der: certified.cert.der().clone(),
        }
    }
    fn policy(&self) -> HttpPolicy {
        HttpPolicy {
            limits: None,
            tls: Some(TlsFiles {
                certificate: self.directory.join("cert.pem").display().to_string(),
                key: self.directory.join("key.pem").display().to_string(),
            }),
        }
    }
    /// A client that trusts this certificate, offering `alpn` over
    /// `versions`.
    fn client(
        &self,
        alpn: &[&[u8]],
        versions: &[&'static rustls::SupportedProtocolVersion],
    ) -> tokio_rustls::TlsConnector {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.der.clone()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(versions)
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
        tokio_rustls::TlsConnector::from(Arc::new(config))
    }
}
impl Drop for Certificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// The `net.listen` grant of `bind` served with TLS.
fn tls_grant(bind: &str) -> CapabilityGrant {
    grant(varta_common::http::listen_endpoint(bind, true))
}

/// Start the service with `certificate`'s TLS and its TLS grant.
fn start(certificate: &Certificate) -> Server {
    let bytes =
        varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap();
    Server::start_tls(bytes, certificate.policy())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

type Tls = tokio_rustls::client::TlsStream<tokio::net::TcpStream>;

async fn connect(server: &Server, connector: &tokio_rustls::TlsConnector) -> std::io::Result<Tls> {
    let tcp = tokio::net::TcpStream::connect(server.addr).await?;
    let name = rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();
    connector.connect(name, tcp).await
}

/// Spawns hyper client tasks on the test runtime.
#[derive(Clone, Copy)]
struct Spawner;
impl<F> hyper::rt::Executor<F> for Spawner
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, future: F) {
        tokio::spawn(future);
    }
}

#[test]
fn tls13_serves_http2_and_http11_by_alpn_and_refuses_older_tls() {
    let certificate = Certificate::new("alpn");
    let server = start(&certificate);
    let port = server.addr.port();
    runtime().block_on(async {
        // HTTP/2 over ALPN `h2`.
        let tls = connect(
            &server,
            &certificate.client(&[b"h2"], &[&rustls::version::TLS13]),
        )
        .await
        .unwrap();
        assert_eq!(tls.get_ref().1.alpn_protocol(), Some(&b"h2"[..]));
        assert_eq!(
            tls.get_ref().1.protocol_version(),
            Some(rustls::ProtocolVersion::TLSv1_3)
        );
        let (mut sender, connection) =
            hyper::client::conn::http2::handshake(Spawner, hyper_util::rt::TokioIo::new(tls))
                .await
                .unwrap();
        tokio::spawn(connection);
        let request = hyper::Request::post(format!("https://127.0.0.1:{port}/two"))
            .body(http_body_util::Full::new(hyper::body::Bytes::from_static(
                b"hi",
            )))
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.version(), hyper::Version::HTTP_2);
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(
            &body[..],
            format!("POST https://127.0.0.1:{port}/two hi").as_bytes()
        );
        // HTTP/1.1 over ALPN `http/1.1`.
        let mut tls = connect(
            &server,
            &certificate.client(&[b"http/1.1"], &[&rustls::version::TLS13]),
        )
        .await
        .unwrap();
        assert_eq!(tls.get_ref().1.alpn_protocol(), Some(&b"http/1.1"[..]));
        tls.write_all(b"GET /one HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        tls.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(
            response.contains("GET https://example.test/one "),
            "{response}"
        );
        // TLS 1.2 and cleartext HTTP get no response.
        assert!(
            connect(
                &server,
                &certificate.client(&[b"h2"], &[&rustls::version::TLS12])
            )
            .await
            .is_err()
        );
        let mut plain = tokio::net::TcpStream::connect(server.addr).await.unwrap();
        plain
            .write_all(b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n")
            .await
            .unwrap();
        let mut received = Vec::new();
        let _ = plain.read_to_end(&mut received).await;
        assert!(!received.starts_with(b"HTTP/"), "{received:?}");
    });
    server.finish();
}

#[test]
fn listen_grants_name_whether_an_endpoint_serves_tls() {
    let certificate = Certificate::new("grants");
    let bytes =
        varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap();
    let runtime = VartaRuntime::new();
    let bind = runtime
        .inspect_module(&bytes)
        .unwrap()
        .http_service
        .unwrap()
        .bind;
    let plain = grant(bind.clone());
    let tls = tls_grant(&bind);
    let denied = |requested: Vec<CapabilityGrant>,
                  ceiling: Option<Vec<CapabilityGrant>>,
                  policy: Option<HttpPolicy>| {
        let report = runtime.run_with(
            &bytes,
            RunOptions {
                requested,
                ceiling,
                http: policy,
                ..RunOptions::default()
            },
        );
        matches!(
            report.failure,
            Some(RuntimeError::Http(HttpError::Denied(_)))
        )
    };
    // A TLS listener needs the TLS form in the package and in the ceiling.
    assert!(denied(
        vec![plain.clone()],
        None,
        Some(certificate.policy())
    ));
    assert!(denied(
        vec![tls.clone()],
        Some(vec![plain.clone()]),
        Some(certificate.policy())
    ));
    assert!(denied(
        vec![plain.clone()],
        Some(vec![tls.clone()]),
        Some(certificate.policy())
    ));
    // A cleartext listener needs the cleartext form.
    assert!(denied(vec![tls.clone()], None, None));
    // Unreadable files and a contract v1 service fail before bind.
    let mut missing = certificate.policy();
    if let Some(files) = missing.tls.as_mut() {
        files.key = certificate
            .directory
            .join("absent.pem")
            .display()
            .to_string();
    }
    let report = runtime.run_with(
        &bytes,
        RunOptions {
            requested: vec![tls.clone()],
            http: Some(missing),
            ..RunOptions::default()
        },
    );
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Http(HttpError::Policy(_)))
        ),
        "{:?}",
        report.failure
    );
    let legacy = varta_compiler::compile_typescript(
        "function handle(r:VartaHttpRequest):VartaHttpResponse{return {status:200,body:r.body,headers:[]};}export function main():void{}varta.http.serve({hostname:'127.0.0.1',port:0},handle);",
    )
    .unwrap();
    let report = runtime.run_with(
        &legacy,
        RunOptions {
            requested: vec![tls_grant(&bind)],
            http: Some(certificate.policy()),
            ..RunOptions::default()
        },
    );
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Http(HttpError::Policy(_)))
        ),
        "{:?}",
        report.failure
    );
}

/// HTTP/2 frames of RFC 9113 the raw client writes and reads.
const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const FRAME_HEADER_BYTES: usize = 9;
/// One SETTINGS entry: a 16-bit identifier and a 32-bit value.
const SETTING_BYTES: usize = 6;
const SETTINGS: u8 = 0x4;
const HEADERS: u8 = 0x1;
const RST_STREAM: u8 = 0x3;
const GOAWAY: u8 = 0x7;
const END_STREAM_AND_HEADERS: u8 = 0x5;
const CANCEL: u32 = 0x8;
const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 0x3;
const SETTINGS_MAX_HEADER_LIST_SIZE: u16 = 0x6;
/// HPACK: `:method GET`, `:scheme https`, `:path /` from the static table
/// and `:authority a` as a literal without indexing.
const REQUEST_BLOCK: &[u8] = &[0x82, 0x87, 0x84, 0x01, 0x01, b'a'];

fn frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = (payload.len() as u32).to_be_bytes()[1..].to_vec();
    bytes.extend([kind, flags]);
    bytes.extend(stream.to_be_bytes());
    bytes.extend(payload);
    bytes
}

/// Read one frame: its type and payload; `None` at the end of the stream.
async fn read_frame(tls: &mut Tls) -> Option<(u8, Vec<u8>)> {
    let mut header = [0; FRAME_HEADER_BYTES];
    tls.read_exact(&mut header).await.ok()?;
    let length = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
    let mut payload = vec![0; length];
    tls.read_exact(&mut payload).await.ok()?;
    Some((header[3], payload))
}

#[test]
fn http2_announces_its_bounds_and_ends_a_rapid_reset_flood() {
    let certificate = Certificate::new("reset");
    let server = start(&certificate);
    let connector = certificate.client(&[b"h2"], &[&rustls::version::TLS13]);
    runtime().block_on(async {
        let mut tls = connect(&server, &connector).await.unwrap();
        tls.write_all(PREFACE).await.unwrap();
        tls.write_all(&frame(SETTINGS, 0, 0, &[])).await.unwrap();
        // The server's first frame is its SETTINGS.
        let (kind, payload) = read_frame(&mut tls).await.unwrap();
        assert_eq!(kind, SETTINGS);
        let settings: std::collections::BTreeMap<u16, u32> = payload
            .as_chunks::<SETTING_BYTES>()
            .0
            .iter()
            .map(|entry| {
                (
                    u16::from_be_bytes([entry[0], entry[1]]),
                    u32::from_be_bytes([entry[2], entry[3], entry[4], entry[5]]),
                )
            })
            .collect();
        assert_eq!(
            settings.get(&SETTINGS_MAX_CONCURRENT_STREAMS),
            Some(&varta_common::http::MAX_CONCURRENT_STREAMS)
        );
        assert_eq!(
            settings.get(&SETTINGS_MAX_HEADER_LIST_SIZE),
            Some(&varta_common::http::MAX_HEADER_LIST_BYTES)
        );
        // Open and reset streams as fast as the connection takes them.
        let mut flood = Vec::new();
        for index in 0..1000u32 {
            let stream = 2 * index + 1;
            flood.extend(frame(
                HEADERS,
                END_STREAM_AND_HEADERS,
                stream,
                REQUEST_BLOCK,
            ));
            flood.extend(frame(RST_STREAM, 0, stream, &CANCEL.to_be_bytes()));
        }
        // The server may close while the flood is being written.
        let _ = tls.write_all(&flood).await;
        // It ends the connection: GOAWAY, then the end of the stream.
        let ended = tokio::time::timeout(Duration::from_secs(10), async {
            let mut goaway = false;
            while let Some((kind, _)) = read_frame(&mut tls).await {
                goaway |= kind == GOAWAY;
            }
            goaway
        })
        .await
        .expect("the server kept a rapid-reset connection open");
        assert!(ended, "no GOAWAY");
    });
    // The server still answers others, and the flood started few workers.
    let started = server.requests.lock().unwrap().len();
    assert!(
        started <= varta_common::http::MAX_ABANDONED_REQUESTS + 1,
        "{started}"
    );
    runtime().block_on(async {
        let mut tls = connect(
            &server,
            &certificate.client(&[b"http/1.1"], &[&rustls::version::TLS13]),
        )
        .await
        .unwrap();
        tls.write_all(b"GET /after HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        tls.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    });
    server.finish();
}
