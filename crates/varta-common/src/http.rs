//! Versioned HTTP/1 service and transport records. These carry no ambient authority.
use crate::{abi, capability_kind};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub const SECTION: &str = "varta.http";
pub const VERSION: u32 = 1;
/// V38 synchronous adapters exchange UTF-16 guest strings and binary64 status.
/// The wire envelope remains UTF-8 JSON; v1 keeps its UTF-8 guest strings.
pub const UTF16_VERSION: u32 = 2;
pub const LATEST_VERSION: u32 = UTF16_VERSION;

/// The explicit HTTP contract admitted by a source profile.
pub fn version_for(profile: crate::language::LanguageProfile) -> Option<u32> {
    if profile.turn_budgets() {
        Some(UTF16_VERSION)
    } else if !profile.binary64() {
        Some(VERSION)
    } else {
        None
    }
}
pub const ABI_PREFIX: &str = "varta.http_abi.";
pub const HANDLER: &str = "varta.http_abi.handle";
pub const COPY: &str = "varta.http_abi.copy_string";
pub const RETAIN: &str = "varta.http_abi.retain_string";
pub const STATE: &str = "varta.http_abi.state";
pub const VALUE: &str = "varta.http_abi.value";
/// V2 streaming services: `handle`'s promise fulfils with a record holding
/// the response's pull function; `pull(i64 record) -> i64` calls it and
/// returns its `Promise<string>`, which the record keeps until the next pull;
/// `chunk_state`/`chunk_value` are that promise's pure accessors. The first pull yields the head envelope, every later one a
/// [`STREAM_DATA`] or [`STREAM_END`] record.
pub const PULL: &str = "varta.http_abi.pull";
pub const CHUNK_STATE: &str = "varta.http_abi.chunk_state";
pub const CHUNK_VALUE: &str = "varta.http_abi.chunk_value";
/// The first code unit of a body record: data (followed by a byte string,
/// one code unit 0..=0xFF per byte) or the end of the body.
pub const STREAM_DATA: char = 'D';
pub const STREAM_END: char = 'E';
fn is_false(value: &bool) -> bool {
    !*value
}
pub const MAX_METADATA_BYTES: usize = 8192;
pub const MAX_HEADERS: usize = 32;
pub const MAX_HEADER_BYTES: usize = 4096;
pub const MAX_BODY_BYTES: usize = 8192;
pub const MAX_TARGET_BYTES: usize = 1024;
pub const HEADER_SECONDS: u64 = 2;
pub const CONNECTION_SECONDS: u64 = 10;
pub const HTTP_BUFFER_BYTES: usize = 8192;
/// One bounded active connection and worker (contract v1).
pub const MAX_CONNECTIONS: usize = 1;
/// Open connections of a v2 service (HTTP/1.1 keep-alive): cheap while
/// idle; a further connection waits in the listener's backlog.
pub const MAX_OPEN_CONNECTIONS: usize = 1024;
/// Requests a v2 service runs at once, each with its own worker and Store.
pub const MAX_CONCURRENT_REQUESTS: usize = 8;
/// A v2 request waits at most this long for a worker, then gets 503 with
/// `Retry-After`.
pub const WORKER_WAIT_SECONDS: u64 = 10;
/// The `Retry-After` of a 503 for a request that found no worker in time.
pub const BUSY_RETRY_AFTER_SECONDS: u64 = 1;
/// A keep-alive connection's idle time between requests, which is also the
/// time a client has to send a request's head (Node's keep-alive default).
pub const KEEP_ALIVE_IDLE_SECONDS: u64 = 5;
/// Requests on one keep-alive connection; the last answer closes it.
pub const MAX_KEEP_ALIVE_REQUESTS: usize = 1000;
/// A keep-alive connection's lifetime, after which it finishes its current
/// response and closes (a streaming service adds the stream lifetime).
pub const CONNECTION_LIFETIME_SECONDS: u64 = 600;
/// A v2 streaming response: the longest wait for the next body chunk (each
/// chunk is one turn with fresh budgets and this deadline) and the longest
/// whole response, after which the transport ends the body and the client
/// reconnects. A chunk holds at most `net::MAX_CHUNK_BYTES`.
pub const STREAM_IDLE_SECONDS: u64 = 60;
pub const STREAM_LIFETIME_SECONDS: u64 = 3600;
pub const STREAM_CHUNK_BYTES: usize = crate::net::MAX_CHUNK_BYTES;
/// Request details retained by a native server report. Totals keep counting
/// after this bound; observers can receive every completed request instead.
pub const MAX_REPORTED_REQUESTS: usize = 32;
/// Effects one contract may list.
pub const MAX_REQUIRED_EFFECTS: usize = 16;
/// The capability every contract requires and `http_stop` checks.
pub const LISTEN_CAPABILITY: &str = "net.listen";
/// The response statuses an adapter may return (RFC 9110 2xx to 5xx).
pub const MIN_STATUS: i32 = 200;
pub const MAX_STATUS: i32 = 599;
/// Statuses whose response never has a body (RFC 9110: No Content, Reset
/// Content, Not Modified).
pub const BODYLESS_STATUSES: [i32; 3] = [204, 205, 304];

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("invalid HTTP contract: {0}")]
    Contract(String),
    #[error("invalid HTTP message: {0}")]
    Message(String),
    #[error("HTTP listener denied: {0}")]
    Denied(String),
    #[error("HTTP I/O: {0}")]
    Io(#[from] std::io::Error),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HttpLimits {
    pub connections: usize,
    /// V2 only: [`STREAM_IDLE_SECONDS`] and [`STREAM_LIFETIME_SECONDS`] in
    /// milliseconds and [`STREAM_CHUNK_BYTES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<StreamLimits>,
    /// V2 only: workers and keep-alive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolLimits>,
    pub request_bytes: usize,
    pub body_bytes: usize,
    pub header_bytes: usize,
    pub headers: usize,
    pub target_bytes: usize,
    pub response_bytes: usize,
    pub handler_deadline_ms: u64,
}
/// [`MAX_CONCURRENT_REQUESTS`], [`WORKER_WAIT_SECONDS`],
/// [`KEEP_ALIVE_IDLE_SECONDS`], [`MAX_KEEP_ALIVE_REQUESTS`] and
/// [`CONNECTION_LIFETIME_SECONDS`] (times in milliseconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PoolLimits {
    pub workers: usize,
    pub worker_wait_ms: u64,
    pub keep_alive_idle_ms: u64,
    pub keep_alive_requests: usize,
    pub connection_lifetime_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StreamLimits {
    pub idle_ms: u64,
    pub lifetime_ms: u64,
    pub chunk_bytes: usize,
}
impl HttpLimits {
    /// The fixed limits of a contract version: v1 keeps one connection and
    /// no streaming; v2 has the connection pool and the stream limits.
    #[must_use]
    pub fn for_version(version: u32) -> Self {
        let legacy = Self::default();
        if version == UTF16_VERSION {
            let millis = crate::budget::MILLISECONDS_PER_SECOND;
            Self {
                connections: MAX_OPEN_CONNECTIONS,
                pool: Some(PoolLimits {
                    workers: MAX_CONCURRENT_REQUESTS,
                    worker_wait_ms: WORKER_WAIT_SECONDS * millis,
                    keep_alive_idle_ms: KEEP_ALIVE_IDLE_SECONDS * millis,
                    keep_alive_requests: MAX_KEEP_ALIVE_REQUESTS,
                    connection_lifetime_ms: CONNECTION_LIFETIME_SECONDS * millis,
                }),
                stream: Some(StreamLimits {
                    idle_ms: STREAM_IDLE_SECONDS * crate::budget::MILLISECONDS_PER_SECOND,
                    lifetime_ms: STREAM_LIFETIME_SECONDS * crate::budget::MILLISECONDS_PER_SECOND,
                    chunk_bytes: STREAM_CHUNK_BYTES,
                }),
                ..legacy
            }
        } else {
            legacy
        }
    }
}
impl Default for HttpLimits {
    /// The contract v1 limits.
    fn default() -> Self {
        Self {
            connections: MAX_CONNECTIONS,
            stream: None,
            pool: None,
            request_bytes: abi::MAX_INPUT_BYTES,
            body_bytes: MAX_BODY_BYTES,
            header_bytes: MAX_HEADER_BYTES,
            headers: MAX_HEADERS,
            target_bytes: MAX_TARGET_BYTES,
            response_bytes: abi::MAX_RESULT_BYTES,
            handler_deadline_ms: abi::DEADLINE_SECONDS * crate::budget::MILLISECONDS_PER_SECOND,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HttpContract {
    pub version: u32,
    /// V2 only: handle returns a rooted Promise<string>; state/value are pure accessors.
    #[serde(default, skip_serializing_if = "is_false")]
    pub asynchronous: bool,
    /// V2 only, with `asynchronous`: the handler returns a Web `Response`
    /// whose body the host pulls in chunks ([`PULL`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub streaming: bool,
    pub bind: String,
    pub export: String,
    pub required_effects: Vec<String>,
    pub limits: HttpLimits,
}
impl HttpContract {
    pub fn validate(&self) -> Result<(), HttpError> {
        let addr = self
            .bind
            .parse::<SocketAddr>()
            .map_err(|_| HttpError::Contract("literal IP endpoint required".into()))?;
        if !matches!(self.version, VERSION | UTF16_VERSION)
            || self.asynchronous && self.version != UTF16_VERSION
            || self.streaming && !self.asynchronous
            || self.export != HANDLER
            || addr.to_string() != self.bind
            || self.limits != HttpLimits::for_version(self.version)
            || self.required_effects.len() > MAX_REQUIRED_EFFECTS
            || !self.required_effects.iter().any(|s| s == LISTEN_CAPABILITY)
            || self.required_effects.windows(2).any(|p| p[0] >= p[1])
            || self
                .required_effects
                .iter()
                .any(|s| capability_kind(s).is_none())
        {
            return Err(HttpError::Contract(
                "version, export, endpoint, effects or limits".into(),
            ));
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, HttpError> {
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(HttpError::Contract("metadata byte limit".into()));
        }
        let value: Self =
            serde_json::from_slice(bytes).map_err(|e| HttpError::Contract(e.to_string()))?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, HttpError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| HttpError::Contract(e.to_string()))?;
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(HttpError::Contract("metadata byte limit".into()));
        }
        Ok(bytes)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Serialize)]
pub struct HttpRequest {
    pub method: String,
    pub target: String,
    pub body: String,
    pub headers: Vec<HttpHeader>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpResponse {
    pub status: i32,
    pub body: String,
    pub headers: Vec<HttpHeader>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityGrant, CapabilityValue, Manifest};
    #[test]
    fn service_versions_preserve_legacy_profiles_and_gate_utf16_to_v38() {
        use crate::language::{LanguageProfile as P, SUPPORTED_PROFILES};
        for profile in SUPPORTED_PROFILES {
            let expected = if *profile == P::CheckedF64Utf16V38 {
                Some(UTF16_VERSION)
            } else if !profile.binary64() {
                Some(VERSION)
            } else {
                None
            };
            assert_eq!(version_for(*profile), expected, "{profile:?}");
        }
        let mut contract = HttpContract {
            version: UTF16_VERSION,
            asynchronous: false,
            streaming: false,
            bind: "127.0.0.1:0".into(),
            export: HANDLER.into(),
            required_effects: vec![LISTEN_CAPABILITY.into()],
            limits: HttpLimits::for_version(UTF16_VERSION),
        };
        HttpContract::parse(&contract.to_bytes().unwrap()).unwrap();
        contract.version = LATEST_VERSION + 1;
        assert!(contract.validate().is_err());
    }
    #[test]
    fn listener_grants_are_exact_endpoints_with_no_outbound_or_boolean_fallback() {
        for endpoint in ["127.0.0.1:0", "[::1]:8080", "0.0.0.0:8080"] {
            let grant = CapabilityGrant {
                id: "net.listen".into(),
                value: CapabilityValue::ListenEndpoints(vec![endpoint.into()]),
            };
            grant.validate().unwrap();
            assert_eq!(
                Manifest::parse(
                    &Manifest::new(std::slice::from_ref(&grant))
                        .to_bytes()
                        .unwrap()
                )
                .unwrap()
                .grants()
                .unwrap(),
                [grant]
            );
        }
        for endpoint in [
            "localhost:8080",
            "http://127.0.0.1:8080/",
            "*:80",
            "127.0.0.1:*",
            "[0:0:0:0:0:0:0:1]:80",
        ] {
            assert!(
                CapabilityGrant {
                    id: "net.listen".into(),
                    value: CapabilityValue::ListenEndpoints(vec![endpoint.into()])
                }
                .validate()
                .is_err(),
                "{endpoint}"
            );
        }
        for value in [
            CapabilityValue::Boolean(true),
            CapabilityValue::UrlGlobs(vec!["http://127.0.0.1:80/**".into()]),
        ] {
            assert!(
                CapabilityGrant {
                    id: "net.listen".into(),
                    value
                }
                .validate()
                .is_err()
            );
        }
    }
    #[test]
    fn service_metadata_is_strict_versioned_and_cannot_raise_limits() {
        let mut contract = HttpContract {
            version: VERSION,
            asynchronous: false,
            streaming: false,
            bind: "127.0.0.1:0".into(),
            export: HANDLER.into(),
            required_effects: vec!["net.listen".into()],
            limits: HttpLimits::default(),
        };
        let legacy = contract.to_bytes().unwrap();
        assert!(
            !std::str::from_utf8(&legacy)
                .unwrap()
                .contains("asynchronous")
        );
        HttpContract::parse(&legacy).unwrap();
        contract.asynchronous = true;
        assert!(contract.validate().is_err());
        contract.asynchronous = false;
        contract.limits.connections += 1;
        assert!(contract.validate().is_err());
        contract.limits = HttpLimits::default();
        contract.required_effects.clear();
        assert!(contract.validate().is_err());
        assert!(HttpContract::parse(&vec![b' '; MAX_METADATA_BYTES + 1]).is_err());
        assert!(HttpContract::parse(br#"{"version":1,"version":1}"#).is_err());
        // V1 keeps its one connection and no stream limits; v2 has the pool
        // and the stream limits, and streaming needs v2's asynchronous form.
        contract.required_effects = vec!["net.listen".into()];
        assert!(!std::str::from_utf8(&legacy).unwrap().contains("stream"));
        contract.streaming = true;
        assert!(contract.validate().is_err());
        contract.limits = HttpLimits::for_version(UTF16_VERSION);
        contract.version = UTF16_VERSION;
        assert!(contract.validate().is_err());
        contract.asynchronous = true;
        HttpContract::parse(&contract.to_bytes().unwrap()).unwrap();
        assert_eq!(contract.limits.connections, MAX_OPEN_CONNECTIONS);
        assert_eq!(
            contract.limits.pool.map(|pool| pool.workers),
            Some(MAX_CONCURRENT_REQUESTS)
        );
        contract.limits = HttpLimits::default();
        assert!(contract.validate().is_err());
    }
}
