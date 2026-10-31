//! Asynchronous network requests of profile v37 (`net_request`): the contract
//! shared by the compiler's `fetch` prelude, the runtime's HTTP client and
//! artifact validation.
//!
//! A request is started by `net_request_start` with a request descriptor
//! (below) after an exact `net.request` grant check, before any name
//! resolution or socket. The runtime holds only request ids and promise
//! serials: each armed operation (the response head, then one body chunk per
//! `net_body_read`) completes once, and its completion wakes the single
//! reaction of an engine-internal pending promise registered with
//! `net_request_watch`, exactly like a timer (v36). The guest then takes the
//! completion kind with `net_request_take` and its text with
//! `net_request_text`. `net_request_abort` cancels the transfer and releases
//! the request.

/// The exact capability every request requires (URL scopes plus an optional
/// method list), distinct from `net.fetch`, `net.get` and `net.listen`.
pub const REQUEST_CAPABILITY: &str = "net.request";

/// Requests holding a runtime slot at once: started and not yet released
/// (a terminal completion taken, or aborted while no operation was armed).
pub const MAX_PENDING_REQUESTS: usize = 8;
/// UTF-16 bytes of one request descriptor, request body included.
pub const MAX_REQUEST_BYTES: usize = 256 * 1024;
/// Header fields of one request, and their total name and value bytes.
pub const MAX_REQUEST_HEADERS: usize = 64;
pub const MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;
/// Header fields of one response, and their total name and value bytes.
pub const MAX_RESPONSE_HEADERS: usize = 100;
pub const MAX_RESPONSE_HEADER_BYTES: usize = 64 * 1024;
/// Body bytes delivered by one `net_body_read` completion (as UTF-16 text
/// it stays far below the host result buffer).
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
/// Body bytes of one response, across all its chunks: the largest ASCII body
/// whose UTF-16 text still fits one string value (`abi::MAX_STRING_BYTES`).
pub const MAX_RESPONSE_BODY_BYTES: usize = crate::abi::MAX_STRING_BYTES / 2;
/// Bytes of a request URL, as for the earlier network operations.
pub const MAX_URL_BYTES: usize = 8192;
/// Bytes of the response head `net_get` and `net_fetch` accept (a larger
/// head is an I/O failure): the host input limit.
pub const MAX_FETCH_HEADER_BYTES: usize = crate::abi::MAX_INPUT_BYTES;
/// Methods of one `net.request` grant.
pub const MAX_GRANT_METHODS: usize = 16;
/// Methods `fetch` rejects (WHATWG "forbidden method"), never grantable.
pub const FORBIDDEN_METHODS: [&str; 3] = ["CONNECT", "TRACE", "TRACK"];

/// Request headers the runtime's HTTP client controls (framing, the
/// connection and the target authority); a request that sets one fails.
pub const RUNTIME_HEADERS: [&str; 9] = [
    "connection",
    "content-length",
    "expect",
    "host",
    "keep-alive",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// The first line of every request descriptor.
pub const DESCRIPTOR_VERSION: &str = "varta-net-request-1";
/// Separates descriptor fields. Methods, URLs, header names and header values
/// cannot contain it; the body is the remainder after the last field.
pub const DESCRIPTOR_SEPARATOR: char = '\n';
/// The body flag of a descriptor without and with a request body.
pub const DESCRIPTOR_NO_BODY: &str = "0";
pub const DESCRIPTOR_BODY: &str = "1";
/// The first line of a v38 (`web_bytes`) descriptor, accepted only by
/// `net_request_start_v2`. Its body flag is [`DESCRIPTOR_NO_BODY`],
/// [`DESCRIPTOR_BODY`] (text, sent as UTF-8) or [`DESCRIPTOR_BYTES`].
pub const DESCRIPTOR_VERSION_BYTES: &str = "varta-net-request-2";
/// The body flag of a version 2 descriptor whose body is bytes: each code
/// unit of the remainder is one byte (a unit above 0xFF is a malformed
/// descriptor), so a byte body is at most half of [`MAX_REQUEST_BYTES`].
pub const DESCRIPTOR_BYTES: &str = "2";

/// What a taken completion is (`net_request_take`). The prelude mirrors these
/// codes as named constants; a compiler test keeps both in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum CompletionKind {
    /// The response head; its text is the head record (JSON, below).
    Head = 1,
    /// One body chunk; its text is the chunk decoded as UTF-8 (a sequence
    /// split between chunks is completed by the next one; invalid bytes become
    /// U+FFFD as with a non-fatal `TextDecoder`). A chunk armed by
    /// `net_body_read_bytes` (v38) carries each received byte as one code
    /// unit instead, undecoded.
    Chunk = 2,
    /// The body ended; the request is released.
    End = 3,
    /// The transfer failed (name resolution, connection, TLS, protocol, a
    /// limit or the URL); its text is the reason. The request is released.
    Failed = 4,
    /// `net_request_abort` cancelled the armed operation; the request is
    /// released.
    Aborted = 5,
    /// The `net.request` grant (or the host ceiling) does not admit the URL
    /// and method; nothing was resolved or connected. Its text is the
    /// denial. The request is released.
    Denied = 6,
}
impl CompletionKind {
    pub const ALL: [Self; 6] = [
        Self::Head,
        Self::Chunk,
        Self::End,
        Self::Failed,
        Self::Aborted,
        Self::Denied,
    ];
    /// Whether taking this completion releases the request.
    #[must_use]
    pub fn terminal(self) -> bool {
        !matches!(self, Self::Head | Self::Chunk)
    }
}

/// The JSON fields of a head completion's text, read by the prelude with a
/// checked `varta.json.parse`: `status` (number), `statusText`, `url` (the
/// request URL as the runtime parsed it), `location` (the `Location` header
/// resolved against `url`, or the empty string) and `headers` (lowercased
/// names alternating with their values, in response order; each value byte
/// is one code unit, as for a WebIDL ByteString).
pub const HEAD_FIELDS: [&str; 5] = ["status", "statusText", "url", "location", "headers"];

/// Fatal request failures: each ends the execution, like the timer state
/// errors, because only a tampered or miscompiled module can cause them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error("execution exceeds {MAX_PENDING_REQUESTS} pending network requests")]
    Budget,
    #[error("malformed network request descriptor")]
    Descriptor,
    #[error("invalid network request operation")]
    State,
}

/// Whether `method` is an RFC 9110 token that `fetch` may send.
#[must_use]
pub fn valid_method(method: &str) -> bool {
    const TOKEN_SYMBOLS: &[u8] = b"!#$%&'*+-.^_`|~";
    !method.is_empty()
        && method
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || TOKEN_SYMBOLS.contains(&byte))
        && !FORBIDDEN_METHODS
            .iter()
            .any(|forbidden| forbidden.eq_ignore_ascii_case(method))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_codes_are_distinct_and_terminal_kinds_release() {
        let codes: std::collections::BTreeSet<i32> = CompletionKind::ALL
            .iter()
            .map(|kind| *kind as i32)
            .collect();
        assert_eq!(codes.len(), CompletionKind::ALL.len());
        assert!(!CompletionKind::Head.terminal() && !CompletionKind::Chunk.terminal());
        for kind in [
            CompletionKind::End,
            CompletionKind::Failed,
            CompletionKind::Aborted,
            CompletionKind::Denied,
        ] {
            assert!(kind.terminal());
        }
    }

    #[test]
    fn methods_are_tokens_and_forbidden_methods_are_refused() {
        for method in ["GET", "POST", "PATCH", "patch", "X-CUSTOM"] {
            assert!(valid_method(method), "{method}");
        }
        for method in ["", "GET POST", "GE\nT", "CONNECT", "trace", "Track", "GÉT"] {
            assert!(!valid_method(method), "{method}");
        }
    }
}
