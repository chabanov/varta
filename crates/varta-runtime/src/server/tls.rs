//! Native TLS of the HTTP server (K7 H3): rustls with the ring provider,
//! TLS 1.3 only, ALPN `h2` before `http/1.1`, no 0-RTT data. The certificate
//! chain and key come from absolute paths the host policy names; the host
//! reads them once before the listener exists, and no guest ever sees them.
use std::{path::Path, sync::Arc};
use varta_common::http::{self, HttpError, TlsFiles};

/// The application protocols offered, in server preference order.
pub(super) const H2: &[u8] = b"h2";
const HTTP11: &[u8] = b"http/1.1";

/// A host-supplied PEM file of at most `http::MAX_TLS_FILE_BYTES`.
fn read(path: &str) -> Result<Vec<u8>, HttpError> {
    use std::io::Read;
    let failed = |error: std::io::Error| HttpError::Policy(format!("TLS file {path}: {error}"));
    if !Path::new(path).is_absolute() {
        return Err(HttpError::Policy(format!(
            "TLS file {path} is not an absolute path"
        )));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(failed)?
        .take(http::MAX_TLS_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failed)?;
    if bytes.len() > http::MAX_TLS_FILE_BYTES {
        return Err(HttpError::Policy(format!(
            "TLS file {path} exceeds {} bytes",
            http::MAX_TLS_FILE_BYTES
        )));
    }
    Ok(bytes)
}

/// The server configuration of `files`.
pub(super) fn config(files: &TlsFiles) -> Result<Arc<rustls::ServerConfig>, HttpError> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let certificates = CertificateDer::pem_slice_iter(&read(&files.certificate)?)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| HttpError::Policy(format!("TLS certificate chain: {error}")))?;
    if certificates.is_empty() {
        return Err(HttpError::Policy("TLS certificate chain is empty".into()));
    }
    let key = PrivateKeyDer::from_pem_slice(&read(&files.key)?)
        .map_err(|error| HttpError::Policy(format!("TLS private key: {error}")))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| HttpError::Policy(error.to_string()))?
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|error| HttpError::Policy(format!("TLS certificate and key: {error}")))?;
    config.alpn_protocols = vec![H2.to_vec(), HTTP11.to_vec()];
    // 0-RTT data could be replayed.
    config.max_early_data_size = 0;
    Ok(Arc::new(config))
}
