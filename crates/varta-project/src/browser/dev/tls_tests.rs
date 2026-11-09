use super::tests::Project;
use super::*;
use std::{path::PathBuf, sync::atomic::AtomicU64};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use varta_common::{
    Manifest,
    http::{HttpPolicy, TlsFiles},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Certificate {
    root: PathBuf,
    der: rustls::pki_types::CertificateDer<'static>,
}
impl Certificate {
    fn new() -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let root = std::env::temp_dir().join(format!(
            "varta-browser-tls-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("cert.pem"), certified.cert.pem()).unwrap();
        std::fs::write(root.join("key.pem"), certified.key_pair.serialize_pem()).unwrap();
        Self {
            root,
            der: certified.cert.der().clone(),
        }
    }
    fn policy(&self) -> HostPolicy {
        let mut manifest = Manifest::new(&[]);
        manifest.http = Some(HttpPolicy {
            delegation: None,
            limits: None,
            session: None,
            tls: Some(TlsFiles {
                certificate: self.root.join("cert.pem").display().to_string(),
                key: self.root.join("key.pem").display().to_string(),
            }),
        });
        HostPolicy::from_manifest(manifest).unwrap()
    }
    fn client(
        &self,
        trusted: bool,
        versions: &[&'static rustls::SupportedProtocolVersion],
    ) -> tokio_rustls::TlsConnector {
        let mut roots = rustls::RootCertStore::empty();
        if trusted {
            roots.add(self.der.clone()).unwrap();
        }
        let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(versions)
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        tokio_rustls::TlsConnector::from(Arc::new(config))
    }
}
impl Drop for Certificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
async fn connect(
    address: SocketAddr,
    connector: &tokio_rustls::TlsConnector,
) -> std::io::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    connector
        .connect(
            rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
            tokio::net::TcpStream::connect(address).await?,
        )
        .await
}
async fn get(address: SocketAddr, connector: &tokio_rustls::TlsConnector, origin: &str) -> String {
    let mut stream = connect(address, connector).await.unwrap();
    assert_eq!(
        stream.get_ref().1.alpn_protocol(),
        Some(b"http/1.1".as_slice())
    );
    assert_eq!(
        stream.get_ref().1.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    stream
        .write_all(
            format!("GET / HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
    let mut output = Vec::new();
    stream.read_to_end(&mut output).await.unwrap();
    String::from_utf8(output).unwrap()
}

#[tokio::test]
async fn browser_dev_tls_checks_real_certificates_protocols_and_https_origin() {
    let project = Project::new();
    let certificate = Certificate::new();
    let policy = certificate.policy();
    let server = BrowserDevServer::bind(
        &project.config(),
        Some(&policy),
        None,
        BrowserDevOptions::default(),
    )
    .unwrap();
    let address = server.address();
    let origin = format!("https://{address}");
    assert_eq!(server.url(), format!("{origin}/"));
    assert!(server.bundle_url().starts_with(&origin));
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task =
        tokio::spawn(server.serve_until(async { stopped.await.map_err(std::io::Error::other) }));
    let client = certificate.client(true, &[&rustls::version::TLS13, &rustls::version::TLS12]);
    let html = get(address, &client, &origin).await;
    assert!(html.starts_with("HTTP/1.1 200"));
    assert!(html.contains("data-varta-dev"));
    assert!(
        get(address, &client, &format!("http://{address}"))
            .await
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        connect(
            address,
            &certificate.client(false, &[&rustls::version::TLS13])
        )
        .await
        .is_err()
    );
    assert!(
        connect(
            address,
            &certificate.client(true, &[&rustls::version::TLS12])
        )
        .await
        .is_err()
    );
    let mut plaintext = tokio::net::TcpStream::connect(address).await.unwrap();
    plaintext
        .write_all(b"GET / HTTP/1.1\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = Vec::new();
    let _ = plaintext.read_to_end(&mut bytes).await;
    assert!(!bytes.starts_with(b"HTTP/1.1 200"));
    let slow = tokio::net::TcpStream::connect(address).await.unwrap();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(slow);
    assert!(std::net::TcpListener::bind(address).is_ok());
}

#[test]
fn browser_dev_tls_rejects_invalid_keys_and_native_session_policy_before_serving() {
    let project = Project::new();
    let certificate = Certificate::new();
    let policy = certificate.policy();
    std::fs::write(certificate.root.join("key.pem"), "PRIVATE-KEY-MARKER").unwrap();
    let error = BrowserDevServer::bind(
        &project.config(),
        Some(&policy),
        None,
        BrowserDevOptions::default(),
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("TLS private key"));
    assert!(!error.contains("PRIVATE-KEY-MARKER"));
    let manifest:Manifest=serde_json::from_value(serde_json::json!({"schemaVersion":1,"abiVersion":36,"capabilities":[],"http":{"session":{"origin":"https://127.0.0.1:8797","clientOrigin":"https://127.0.0.1:8798"}}})).unwrap();
    let policy = HostPolicy::from_manifest(manifest).unwrap();
    assert!(
        BrowserDevServer::bind(
            &project.config(),
            Some(&policy),
            None,
            BrowserDevOptions::default()
        )
        .err()
        .unwrap()
        .to_string()
        .contains("accepts http.tls only")
    );
    assert!(!project.0.join("dist").exists());
}
