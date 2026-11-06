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
/// Requests a v2 service runs at once (H2: the instance slots of its
/// pooling allocator), each a task with its own Store, and together within
/// [`SERVER_MEMORY_BYTES`].
pub const MAX_CONCURRENT_REQUESTS: usize = 1024;
/// A v2 service's memory ceiling (H2): a request runs only while the
/// memory budgets of the running requests, its own included, fit it, so
/// the server's guest memory has a fixed bound whatever the load (128
/// requests of the full [`REQUEST_MEMORY_BYTES`]).
pub const SERVER_MEMORY_BYTES: usize = 1024 * 1024 * 1024;
/// A v2 request waits at most this long for a slot and its memory, then
/// gets 503 with `Retry-After`.
pub const WORKER_WAIT_SECONDS: u64 = 10;
/// The `Retry-After` of a 503 for a request that found no worker in time.
pub const BUSY_RETRY_AFTER_SECONDS: u64 = 1;
/// The memory one request's Store may use (and the size of a pool slot):
/// a request keeps the pre-v38 bound while a long-lived application
/// instance gets the v38 maximum.
pub const REQUEST_MEMORY_BYTES: usize = crate::abi::MAX_MEMORY_BYTES;
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
/// reconnects. A chunk holds at most `net::MAX_CHUNK_BYTES`. The idle wait
/// equals the network inactivity window (owner decision 2026-11-06), so a
/// service that streams an upstream response never ends it while its
/// upstream may still be quiet.
pub const STREAM_IDLE_SECONDS: u64 = crate::net::READ_INACTIVITY_SECONDS;
pub const STREAM_LIFETIME_SECONDS: u64 = 3600;
pub const STREAM_CHUNK_BYTES: usize = crate::net::MAX_CHUNK_BYTES;
/// A v2 Web `Request` service: the most bytes one request body may stream
/// to the guest. The host holds at most one received frame of it at a
/// time; the guest reads it chunk by chunk.
pub const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
/// A v2 Web `Request` service: the longest wait for the next request body
/// frame once the guest asked for it (nginx's `client_body_timeout`).
pub const REQUEST_BODY_IDLE_SECONDS: u64 = 60;
/// A v2 Web `Request` service: a request lives at most this long from its
/// head to its response head, its body included (Node's `requestTimeout`);
/// a streaming response adds the stream lifetime.
pub const REQUEST_LIFETIME_SECONDS: u64 = 300;
/// A v2 connection whose response write makes no progress this long is
/// closed, so a client that stops reading holds its connection no longer
/// (nginx's `send_timeout`).
pub const WRITE_IDLE_SECONDS: u64 = 60;
/// The scheme prefix of a `net.listen` endpoint that serves TLS
/// (`"https://127.0.0.1:8443"`, K7 H3); a bare `IP:port` serves cleartext.
pub const LISTEN_TLS_PREFIX: &str = "https://";
/// The largest certificate chain or private key file a host policy names.
pub const MAX_TLS_FILE_BYTES: usize = 1024 * 1024;
/// A TLS client must complete its handshake within this time.
pub const TLS_HANDSHAKE_SECONDS: u64 = 10;
/// HTTP/2: the streams one connection may have open at once
/// (`SETTINGS_MAX_CONCURRENT_STREAMS`).
pub const MAX_CONCURRENT_STREAMS: u32 = 100;
/// HTTP/2: the largest decoded request header list
/// (`SETTINGS_MAX_HEADER_LIST_SIZE`); the request header bounds still apply.
pub const MAX_HEADER_LIST_BYTES: u32 = 16 * 1024;
/// HTTP/2 flow control: the receive window of a stream and of a connection.
pub const STREAM_WINDOW_BYTES: u32 = 256 * 1024;
pub const CONNECTION_WINDOW_BYTES: u32 = 1024 * 1024;
/// HTTP/2 rapid reset (CVE-2023-44487): the streams a client may reset
/// before the server accepted them, and those the server may reset for
/// errors, before the connection ends with `GOAWAY`.
pub const MAX_PENDING_RESET_STREAMS: usize = 20;
/// The requests a client may abandon on one connection (a rapid reset that
/// reached the application) before the server retires it.
pub const MAX_ABANDONED_REQUESTS: usize = 100;
/// HTTP/2 keep-alive pings: how often, and how long a peer has to answer.
pub const PING_INTERVAL_SECONDS: u64 = 30;
pub const PING_TIMEOUT_SECONDS: u64 = 10;

/// The `net.listen` pattern of `bind` served with TLS (`tls`) or without.
#[must_use]
pub fn listen_endpoint(bind: &str, tls: bool) -> String {
    if tls {
        format!("{LISTEN_TLS_PREFIX}{bind}")
    } else {
        bind.to_owned()
    }
}

/// The socket address a `net.listen` pattern names, its TLS prefix removed.
#[must_use]
pub fn listen_address(pattern: &str) -> &str {
    pattern.strip_prefix(LISTEN_TLS_PREFIX).unwrap_or(pattern)
}
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
    #[error("invalid host HTTP policy: {0}")]
    Policy(String),
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
    /// V2 only: request bodies, request lifetime and slow clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<TransportLimits>,
    pub request_bytes: usize,
    pub body_bytes: usize,
    pub header_bytes: usize,
    pub headers: usize,
    pub target_bytes: usize,
    pub response_bytes: usize,
    pub handler_deadline_ms: u64,
}
/// [`MAX_CONCURRENT_REQUESTS`], [`SERVER_MEMORY_BYTES`],
/// [`WORKER_WAIT_SECONDS`], [`KEEP_ALIVE_IDLE_SECONDS`],
/// [`MAX_KEEP_ALIVE_REQUESTS`] and [`CONNECTION_LIFETIME_SECONDS`] (times in
/// milliseconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PoolLimits {
    pub workers: usize,
    pub memory_bytes: usize,
    pub worker_wait_ms: u64,
    pub keep_alive_idle_ms: u64,
    pub keep_alive_requests: usize,
    pub connection_lifetime_ms: u64,
}
/// [`MAX_REQUEST_BODY_BYTES`], [`REQUEST_BODY_IDLE_SECONDS`],
/// [`REQUEST_LIFETIME_SECONDS`] and [`WRITE_IDLE_SECONDS`] (times in
/// milliseconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TransportLimits {
    pub request_body_bytes: usize,
    pub body_idle_ms: u64,
    pub request_lifetime_ms: u64,
    pub write_idle_ms: u64,
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
                    memory_bytes: SERVER_MEMORY_BYTES,
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
                transport: Some(TransportLimits {
                    request_body_bytes: MAX_REQUEST_BODY_BYTES,
                    body_idle_ms: REQUEST_BODY_IDLE_SECONDS * millis,
                    request_lifetime_ms: REQUEST_LIFETIME_SECONDS * millis,
                    write_idle_ms: WRITE_IDLE_SECONDS * millis,
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
            transport: None,
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
    /// V2 only, with `asynchronous`: the handler takes a Web `Request`
    /// ([`WebRequestHead`]) whose body streams from the host.
    #[serde(default, skip_serializing_if = "is_false")]
    pub web_request: bool,
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
            || self.web_request && !self.asynchronous
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
/// The request envelope of a v2 Web `Request` service, from which the
/// adapter makes the handler's `Request`: `url` is absolute (the scheme and
/// authority the request reached, its target's path and query) and `body`
/// the id of the transfer its body streams through (`net_body_read*`), or
/// 0 for a request without a body.
#[derive(Debug, Serialize)]
pub struct WebRequestHead {
    pub method: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub body: u64,
}

/// The `http` member of a host policy: what the host sets for every
/// service it runs, never a package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HttpPolicy {
    /// Lower server limits ([`HttpLimits::lowered`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<HostLimits>,
    /// The certificate chain and key every service of this host serves
    /// over TLS 1.3 (K7 H3); its `net.listen` grants must then name the
    /// endpoint with [`LISTEN_TLS_PREFIX`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsFiles>,
}

/// Absolute paths of the PEM files the host reads before binding: the
/// certificate chain (leaf first) and its private key. Neither ever reaches
/// a package or guest code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TlsFiles {
    pub certificate: String,
    pub key: String,
}

/// Server limits a host policy lowers: each present value replaces the
/// registry's, and must lie between 1 and it. A contract v1 service has no
/// lowerable limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HostLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_wait_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_idle_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_requests: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_lifetime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_idle_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_lifetime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_idle_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_lifetime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_idle_ms: Option<u64>,
}
impl HostLimits {
    /// Whether the policy lowers any limit.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Replace `limit` by a host's lower `value`: 1 to `limit` inclusive.
fn lower<T: Copy + PartialOrd + From<u8> + std::fmt::Display>(
    name: &str,
    limit: &mut T,
    value: Option<T>,
) -> Result<(), HttpError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value < T::from(1) || value > *limit {
        return Err(HttpError::Policy(format!(
            "{name} {value} is outside 1..={limit}; a host may only lower a limit"
        )));
    }
    *limit = value;
    Ok(())
}

impl HttpLimits {
    /// The limits a host policy leaves: each of `host`'s values replaces the
    /// registry's, which it may only lower. A contract v1 service (no pool)
    /// admits no lowered limit.
    pub fn lowered(&self, host: &HostLimits) -> Result<Self, HttpError> {
        let mut limits = self.clone();
        if host.is_empty() {
            return Ok(limits);
        }
        let (Some(pool), Some(stream), Some(transport)) = (
            limits.pool.as_mut(),
            limits.stream.as_mut(),
            limits.transport.as_mut(),
        ) else {
            return Err(HttpError::Policy(
                "host HTTP limits apply to contract v2 services only".into(),
            ));
        };
        lower("connections", &mut limits.connections, host.connections)?;
        lower("workers", &mut pool.workers, host.workers)?;
        lower("memoryBytes", &mut pool.memory_bytes, host.memory_bytes)?;
        lower(
            "workerWaitMs",
            &mut pool.worker_wait_ms,
            host.worker_wait_ms,
        )?;
        lower(
            "keepAliveIdleMs",
            &mut pool.keep_alive_idle_ms,
            host.keep_alive_idle_ms,
        )?;
        lower(
            "keepAliveRequests",
            &mut pool.keep_alive_requests,
            host.keep_alive_requests,
        )?;
        lower(
            "connectionLifetimeMs",
            &mut pool.connection_lifetime_ms,
            host.connection_lifetime_ms,
        )?;
        lower("streamIdleMs", &mut stream.idle_ms, host.stream_idle_ms)?;
        lower(
            "streamLifetimeMs",
            &mut stream.lifetime_ms,
            host.stream_lifetime_ms,
        )?;
        lower(
            "requestBodyBytes",
            &mut transport.request_body_bytes,
            host.request_body_bytes,
        )?;
        lower("bodyIdleMs", &mut transport.body_idle_ms, host.body_idle_ms)?;
        lower(
            "requestLifetimeMs",
            &mut transport.request_lifetime_ms,
            host.request_lifetime_ms,
        )?;
        lower(
            "writeIdleMs",
            &mut transport.write_idle_ms,
            host.write_idle_ms,
        )?;
        Ok(limits)
    }
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
            web_request: false,
            bind: "127.0.0.1:0".into(),
            export: HANDLER.into(),
            required_effects: vec![LISTEN_CAPABILITY.into()],
            limits: HttpLimits::for_version(UTF16_VERSION),
        };
        HttpContract::parse(&contract.to_bytes().unwrap()).unwrap();
        // A Web `Request` handler needs the asynchronous v2 adapter.
        contract.web_request = true;
        assert!(contract.validate().is_err());
        contract.asynchronous = true;
        let bytes = contract.to_bytes().unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("\"webRequest\":true"));
        HttpContract::parse(&bytes).unwrap();
        contract.version = LATEST_VERSION + 1;
        assert!(contract.validate().is_err());
    }
    #[test]
    fn host_policies_only_lower_v2_limits_and_never_v1() {
        let registry = HttpLimits::for_version(UTF16_VERSION);
        assert_eq!(registry.lowered(&HostLimits::default()).unwrap(), registry);
        let host = HostLimits {
            connections: Some(16),
            workers: Some(2),
            request_body_bytes: Some(1024),
            write_idle_ms: Some(500),
            ..HostLimits::default()
        };
        let lowered = registry.lowered(&host).unwrap();
        assert_eq!(lowered.connections, 16);
        assert_eq!(lowered.pool.map(|pool| pool.workers), Some(2));
        let transport = lowered.transport.unwrap();
        assert_eq!(
            (transport.request_body_bytes, transport.write_idle_ms),
            (1024, 500)
        );
        assert_eq!(
            transport.body_idle_ms,
            REQUEST_BODY_IDLE_SECONDS * crate::budget::MILLISECONDS_PER_SECOND
        );
        // Raising, zero and v1 lowering are policy errors.
        for host in [
            HostLimits {
                connections: Some(MAX_OPEN_CONNECTIONS + 1),
                ..HostLimits::default()
            },
            HostLimits {
                request_body_bytes: Some(MAX_REQUEST_BODY_BYTES + 1),
                ..HostLimits::default()
            },
            HostLimits {
                workers: Some(0),
                ..HostLimits::default()
            },
        ] {
            assert!(matches!(registry.lowered(&host), Err(HttpError::Policy(_))));
        }
        assert!(matches!(
            HttpLimits::default().lowered(&host),
            Err(HttpError::Policy(_))
        ));
        let policy: HttpPolicy =
            serde_json::from_str(r#"{"limits":{"workers":1,"bodyIdleMs":1000}}"#).unwrap();
        assert_eq!(
            policy
                .limits
                .map(|limits| (limits.workers, limits.body_idle_ms)),
            Some((Some(1), Some(1000)))
        );
        assert!(serde_json::from_str::<HttpPolicy>(r#"{"limits":{"chunkBytes":1}}"#).is_err());
        // TLS files and the grant form that names a TLS endpoint.
        let policy: HttpPolicy =
            serde_json::from_str(r#"{"tls":{"certificate":"/etc/c.pem","key":"/etc/k.pem"}}"#)
                .unwrap();
        assert_eq!(policy.tls.map(|tls| tls.key), Some("/etc/k.pem".into()));
        assert!(serde_json::from_str::<HttpPolicy>(r#"{"tls":{"certificate":"/c"}}"#).is_err());
        assert_eq!(
            listen_endpoint("127.0.0.1:443", true),
            "https://127.0.0.1:443"
        );
        assert_eq!(listen_endpoint("127.0.0.1:80", false), "127.0.0.1:80");
        assert_eq!(listen_address("https://[::1]:443"), "[::1]:443");
        assert_eq!(listen_address("[::1]:80"), "[::1]:80");
    }
    #[test]
    fn listener_grants_are_exact_endpoints_with_no_outbound_or_boolean_fallback() {
        // A bare endpoint serves cleartext; the `https://` form serves TLS.
        for endpoint in [
            "127.0.0.1:0",
            "[::1]:8080",
            "0.0.0.0:8080",
            "https://127.0.0.1:8443",
            "https://[::1]:443",
        ] {
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
            "https://localhost:443",
            "https://127.0.0.1:8443/",
            "http://127.0.0.1:8080",
            "tls://127.0.0.1:8443",
            "HTTPS://127.0.0.1:8443",
            "https://https://127.0.0.1:8443",
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
            web_request: false,
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
        assert!(!std::str::from_utf8(&legacy).unwrap().contains("transport"));
        assert!(!std::str::from_utf8(&legacy).unwrap().contains("webRequest"));
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
