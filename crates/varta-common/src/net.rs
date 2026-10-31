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
//!
//! Under v38 (`net_request_start_v2`) the head record is canonical and shows
//! only the response headers the grant declares (`responseHeaders`, or the
//! CORS-safelisted names without a declaration): see [`HeadForm`].
use std::collections::{BTreeMap, BTreeSet, btree_map};

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

/// Response header names one `net.request` grant may declare
/// (`responseHeaders`, v38).
pub const MAX_GRANT_RESPONSE_HEADERS: usize = 64;
/// Bytes of one declared response header name.
pub const MAX_RESPONSE_HEADER_NAME_BYTES: usize = 256;
/// Response header names no grant can declare and no host ever shows the
/// guest (WHATWG Fetch "forbidden response-header names"; a browser never
/// exposes them).
pub const FORBIDDEN_RESPONSE_HEADERS: [&str; 2] = ["set-cookie", "set-cookie2"];
/// The response headers a v38 request shows when its grant declares no
/// `responseHeaders`: the CORS-safelisted response-header names, which a
/// browser exposes without any server configuration. In byte order.
pub const SAFELISTED_RESPONSE_HEADERS: [&str; 7] = [
    "cache-control",
    "content-language",
    "content-length",
    "content-type",
    "expires",
    "last-modified",
    "pragma",
];
/// Joins the values of a repeated response header in a canonical (v38) head
/// record, in arrival order (WHATWG Fetch "combine").
pub const COMBINE_SEPARATOR: &str = ", ";

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
/// names alternating with their values; each value byte is one code unit,
/// as for a WebIDL ByteString). The `headers` of a version 1 descriptor
/// (v37) are in [`HeadForm::Arrival`], those of a version 2 descriptor (v38)
/// in [`HeadForm::Canonical`].
pub const HEAD_FIELDS: [&str; 5] = ["status", "statusText", "url", "location", "headers"];

/// The head record of a [`CompletionKind::Head`] completion: one compact JSON
/// object with exactly the [`HEAD_FIELDS`] members, written in member-name
/// order. `headers` alternates names and values.
#[must_use]
pub fn head_record(
    status: u16,
    status_text: &str,
    url: &str,
    location: &str,
    headers: Vec<String>,
) -> String {
    let [
        status_field,
        text_field,
        url_field,
        location_field,
        headers_field,
    ] = HEAD_FIELDS;
    let mut record = serde_json::Map::new();
    record.insert(status_field.to_owned(), status.into());
    record.insert(text_field.to_owned(), status_text.into());
    record.insert(url_field.to_owned(), url.into());
    record.insert(location_field.to_owned(), location.into());
    record.insert(headers_field.to_owned(), headers.into());
    // `Value`'s `Display` writes compact JSON and cannot fail.
    serde_json::Value::Object(record).to_string()
}

/// How the `headers` member of a head record lists the response fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadForm {
    /// V37 (`net_request_start`): every received field, names in the order
    /// of their first field, each repeated field its own pair in arrival
    /// order (never combined).
    Arrival,
    /// V38 (`net_request_start_v2`): only the visible names
    /// ([`visible_response_headers`]) the response carries, each once, in
    /// byte order, repeated values combined with [`COMBINE_SEPARATOR`] in
    /// arrival order. A forbidden name never appears. This is the form a
    /// browser's Fetch `Headers` produce, so every host can produce it
    /// byte-identically.
    Canonical(BTreeSet<String>),
}
impl HeadForm {
    /// The `headers` member for `fields`: the received fields grouped as the
    /// reference host's header map yields them (each name at its first
    /// field, followed by its values in arrival order), with lowercase names
    /// and values as code units.
    #[must_use]
    pub fn headers(&self, fields: impl IntoIterator<Item = (String, String)>) -> Vec<String> {
        let Self::Canonical(visible) = self else {
            return fields
                .into_iter()
                .flat_map(|(name, value)| [name, value])
                .collect();
        };
        let mut combined: BTreeMap<String, String> = BTreeMap::new();
        for (name, value) in fields {
            if !visible.contains(&name) || forbidden_response_header(&name) {
                continue;
            }
            match combined.entry(name) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(value);
                }
                btree_map::Entry::Occupied(mut entry) => {
                    let joined = entry.get_mut();
                    joined.push_str(COMBINE_SEPARATOR);
                    joined.push_str(&value);
                }
            }
        }
        combined
            .into_iter()
            .flat_map(|(name, value)| [name, value])
            .collect()
    }
}

/// Whether `name` (lowercase) is a forbidden response-header name.
#[must_use]
pub fn forbidden_response_header(name: &str) -> bool {
    FORBIDDEN_RESPONSE_HEADERS.contains(&name)
}

/// The response header names one grant shows a v38 request: its declared
/// `responseHeaders`, or [`SAFELISTED_RESPONSE_HEADERS`] when it declares
/// none. With a host ceiling, a request sees the intersection of the
/// package's set and the ceiling's set.
#[must_use]
pub fn visible_response_headers(declared: Option<&[String]>) -> BTreeSet<String> {
    match declared {
        Some(names) => names.iter().cloned().collect(),
        None => SAFELISTED_RESPONSE_HEADERS
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
    }
}

/// Whether `name` may be declared in `responseHeaders`: a nonempty RFC 9110
/// token of lowercase ASCII, at most [`MAX_RESPONSE_HEADER_NAME_BYTES`], and
/// no forbidden response-header name.
#[must_use]
pub fn declarable_response_header(name: &str) -> bool {
    name.len() <= MAX_RESPONSE_HEADER_NAME_BYTES
        && token(name)
        && !name.bytes().any(|byte| byte.is_ascii_uppercase())
        && !forbidden_response_header(name)
}

/// The text of a [`CompletionKind::Denied`] completion, also logged as
/// `varta log: <text>`.
#[must_use]
pub fn denial_text(method: &str, url: &str) -> String {
    format!("<capability denied: {REQUEST_CAPABILITY}> ({method} {url})")
}

/// Why a request completes as [`CompletionKind::Failed`]; its display is the
/// completion text. The variants with a payload carry a request field or the
/// rendering of a transport library error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestFailure {
    /// `net_request_start`: the method is no token `fetch` may send.
    #[error("invalid method: {0}")]
    Method(String),
    /// `net_request_start`: the URL does not parse.
    #[error("Invalid URL: {0}")]
    Url(String),
    /// `net_request_start`: the denial could not be logged.
    #[error("output budget exhausted")]
    OutputBudget,
    /// `net_request_start`: a `GET` or `HEAD` request with a body.
    #[error("a GET or HEAD request cannot have a body")]
    BodyNotAllowed,
    /// `net_request_start`: the header names and values exceed
    /// [`MAX_REQUEST_HEADER_BYTES`].
    #[error("request headers are too large")]
    HeaderBytes,
    /// `net_request_start`: a header of [`RUNTIME_HEADERS`], as spelled.
    #[error("header {0} is set by the runtime")]
    RuntimeHeader(String),
    /// `net_request_start`: a header name that is no field-name token.
    #[error("invalid header name: {0}")]
    HeaderName(String),
    /// `net_request_start`: a value with a unit above 0xFF or a byte no field
    /// value admits; the payload is the lowercased header name.
    #[error("invalid header value for {0}")]
    HeaderValue(String),
    /// `net_request_start` of an `https` URL: the TLS configuration failed.
    #[error("{0}")]
    TlsConfig(String),
    /// Transfer: the URL has no host.
    #[error("URL has no host")]
    NoHost,
    /// Transfer: the URL has no port and its scheme no default port.
    #[error("URL has no port")]
    NoPort,
    /// Transfer: name resolution or the TCP connect failed.
    #[error("connect failed: {0}")]
    Connect(String),
    /// Transfer: the host is no TLS server name.
    #[error("invalid TLS server name: {0}")]
    ServerName(String),
    /// Transfer: the TLS handshake failed (including certificate checks).
    #[error("TLS handshake failed: {0}")]
    TlsHandshake(String),
    /// Transfer: the HTTP/1.1 connection could not be set up.
    #[error("HTTP handshake failed: {0}")]
    HttpHandshake(String),
    /// Transfer: the request could not be built.
    #[error("invalid request: {0}")]
    Request(String),
    /// Transfer: sending the request or reading the head failed (including a
    /// head beyond the response header limits).
    #[error("request failed: {0}")]
    Send(String),
    /// Transfer: the head did not arrive before the deadline.
    #[error("request timed out")]
    Timeout,
    /// A body read: the body stream failed.
    #[error("response body failed: {0}")]
    Body(String),
    /// A body read: the body exceeds [`MAX_RESPONSE_BODY_BYTES`].
    #[error("response body exceeds {MAX_RESPONSE_BODY_BYTES} bytes")]
    BodyLimit,
    /// A body read: no chunk arrived before the deadline.
    #[error("response body timed out")]
    BodyTimeout,
}

/// The descriptor version an import accepts: version 1
/// (`net_request_start`, text bodies) or version 2 (`net_request_start_v2`,
/// v38, text or byte bodies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorVersion {
    V1,
    V2,
}
impl DescriptorVersion {
    pub const ALL: [Self; 2] = [Self::V1, Self::V2];
    /// The first line of a descriptor of this version.
    #[must_use]
    pub fn line(self) -> &'static str {
        match self {
            Self::V1 => DESCRIPTOR_VERSION,
            Self::V2 => DESCRIPTOR_VERSION_BYTES,
        }
    }
    /// The body flags this version admits, in flag order.
    #[must_use]
    pub fn body_flags(self) -> &'static [&'static str] {
        match self {
            Self::V1 => &[DESCRIPTOR_NO_BODY, DESCRIPTOR_BODY],
            Self::V2 => &[DESCRIPTOR_NO_BODY, DESCRIPTOR_BODY, DESCRIPTOR_BYTES],
        }
    }
}

/// The fields of a request descriptor. Splitting checks only the structure:
/// the method, URL and headers are checked by the start that follows, which
/// turns every refusal into a [`RequestFailure`] or a denial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDescriptor<'a> {
    pub method: &'a str,
    pub url: &'a str,
    /// Names and values in descriptor order.
    pub headers: Vec<(&'a str, &'a str)>,
    /// The bytes to send: a text body as UTF-8, a byte body one byte per
    /// code unit; `None` without a body.
    pub body: Option<Vec<u8>>,
}
impl<'a> RequestDescriptor<'a> {
    /// Split `text`, the descriptor already decoded from UTF-16. A malformed
    /// structure is fatal: only a tampered or miscompiled module can produce
    /// one.
    pub fn parse(text: &'a str, version: DescriptorVersion) -> Result<Self, NetError> {
        /// Version, method, URL, then the rest from the header count on.
        const LEADING_FIELDS: usize = 4;
        let mut fields = text.splitn(LEADING_FIELDS, DESCRIPTOR_SEPARATOR);
        let mut field = || fields.next().ok_or(NetError::Descriptor);
        if field()? != version.line() {
            return Err(NetError::Descriptor);
        }
        let method = field()?;
        let url = field()?;
        let mut rest = field()?;
        let count: usize = split(&mut rest)?
            .parse()
            .map_err(|_| NetError::Descriptor)?;
        if count > MAX_REQUEST_HEADERS {
            return Err(NetError::Descriptor);
        }
        let mut headers = Vec::with_capacity(count);
        for _ in 0..count {
            let name = split(&mut rest)?;
            let value = split(&mut rest)?;
            headers.push((name, value));
        }
        let body = match rest.split_once(DESCRIPTOR_SEPARATOR) {
            Some((DESCRIPTOR_NO_BODY, "")) => None,
            Some((DESCRIPTOR_BODY, body)) => Some(body.as_bytes().to_vec()),
            // Version 2 bytes: each code unit is one byte.
            Some((DESCRIPTOR_BYTES, body)) if version == DescriptorVersion::V2 => Some(
                body.chars()
                    .map(|unit| u8::try_from(u32::from(unit)).ok())
                    .collect::<Option<Vec<u8>>>()
                    .ok_or(NetError::Descriptor)?,
            ),
            _ => return Err(NetError::Descriptor),
        };
        Ok(Self {
            method,
            url,
            headers,
            body,
        })
    }
}
fn split<'a>(rest: &mut &'a str) -> Result<&'a str, NetError> {
    let (field, tail) = rest
        .split_once(DESCRIPTOR_SEPARATOR)
        .ok_or(NetError::Descriptor)?;
    *rest = tail;
    Ok(field)
}

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

/// Whether `text` is a nonempty RFC 9110 token.
fn token(text: &str) -> bool {
    const TOKEN_SYMBOLS: &[u8] = b"!#$%&'*+-.^_`|~";
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || TOKEN_SYMBOLS.contains(&byte))
}

/// Whether `method` is an RFC 9110 token that `fetch` may send.
#[must_use]
pub fn valid_method(method: &str) -> bool {
    token(method)
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
    fn descriptors_split_into_bounded_fields() {
        use DescriptorVersion::{V1, V2};
        let text = format!(
            "{DESCRIPTOR_VERSION}\nPOST\nhttp://127.0.0.1:1/x\n2\naccept\n*/*\nx-a\nb\n1\nline1\nline2"
        );
        let parsed = RequestDescriptor::parse(&text, V1).unwrap();
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.url, "http://127.0.0.1:1/x");
        assert_eq!(parsed.headers, [("accept", "*/*"), ("x-a", "b")]);
        assert_eq!(parsed.body.as_deref(), Some(&b"line1\nline2"[..]));
        let get = format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n0\n0\n");
        assert_eq!(RequestDescriptor::parse(&get, V1).unwrap().body, None);
        // Version 1 never carries bytes, and each import accepts only its
        // own version.
        let bytes = format!("{DESCRIPTOR_VERSION}\nPOST\nhttp://a.test/\n0\n2\nab");
        assert_eq!(
            RequestDescriptor::parse(&bytes, V1).err(),
            Some(NetError::Descriptor)
        );
        assert_eq!(
            RequestDescriptor::parse(&text, V2).err(),
            Some(NetError::Descriptor)
        );
        for bad in [
            String::new(),
            "varta-net-request-0\nGET\nhttp://a.test/\n0\n0\n".into(),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n1\nx\n0\n"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\nx\n0\n"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n0\n2\n"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n0\n0\nextra"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n65\n0\n"),
        ] {
            assert_eq!(
                RequestDescriptor::parse(&bad, V1).err(),
                Some(NetError::Descriptor),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn version_two_descriptors_carry_text_or_bytes() {
        let version = DescriptorVersion::V2;
        let head = format!("{DESCRIPTOR_VERSION_BYTES}\nPOST\nhttp://a.test/\n0\n");
        let text = format!("{head}1\n\u{e9}");
        assert_eq!(
            RequestDescriptor::parse(&text, version)
                .unwrap()
                .body
                .as_deref(),
            Some("\u{e9}".as_bytes())
        );
        let bytes = format!("{head}2\n\u{0}\u{e9}\u{ff}");
        assert_eq!(
            RequestDescriptor::parse(&bytes, version)
                .unwrap()
                .body
                .as_deref(),
            Some(&[0x00, 0xE9, 0xFF][..])
        );
        // A code unit above 0xFF is no byte.
        let wide = format!("{head}2\n\u{100}");
        assert_eq!(
            RequestDescriptor::parse(&wide, version).err(),
            Some(NetError::Descriptor)
        );
        let none = format!("{DESCRIPTOR_VERSION_BYTES}\nGET\nhttp://a.test/\n0\n0\n");
        assert_eq!(RequestDescriptor::parse(&none, version).unwrap().body, None);
    }

    #[test]
    fn head_records_are_compact_and_ordered_by_member_name() {
        let record = head_record(
            200,
            "OK",
            "http://a.test/",
            "",
            vec!["a".into(), "1".into()],
        );
        assert_eq!(
            record,
            r#"{"headers":["a","1"],"location":"","status":200,"statusText":"OK","url":"http://a.test/"}"#
        );
    }

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn canonical_heads_sort_combine_and_show_only_visible_names() {
        let received = fields(&[
            ("x-b", "1"),
            ("x-b", "2"),
            ("content-type", "text/plain"),
            ("set-cookie", "a=1"),
            ("set-cookie", "b=2"),
            ("x-a", "z"),
            ("x-hidden", "secret"),
        ]);
        // V37 keeps every field as its own pair, never combined.
        assert_eq!(
            HeadForm::Arrival.headers(received.clone()),
            [
                "x-b",
                "1",
                "x-b",
                "2",
                "content-type",
                "text/plain",
                "set-cookie",
                "a=1",
                "set-cookie",
                "b=2",
                "x-a",
                "z",
                "x-hidden",
                "secret"
            ]
        );
        // A visible set naming set-cookie still never shows it.
        let visible = ["x-b", "x-a", "content-type", "set-cookie", "x-absent"]
            .map(str::to_owned)
            .into_iter()
            .collect();
        assert_eq!(
            HeadForm::Canonical(visible).headers(received.clone()),
            ["content-type", "text/plain", "x-a", "z", "x-b", "1, 2"]
        );
        // Without a declaration only the safelisted content-type shows.
        assert_eq!(
            HeadForm::Canonical(visible_response_headers(None)).headers(received.clone()),
            ["content-type", "text/plain"]
        );
        assert!(
            HeadForm::Canonical(visible_response_headers(Some(&[])))
                .headers(received)
                .is_empty()
        );
    }

    #[test]
    fn declarable_response_headers_are_lowercase_tokens_without_set_cookie() {
        for name in ["x-request-id", "retry-after", "etag", "x_a.b~c"] {
            assert!(declarable_response_header(name), "{name}");
        }
        let long = "a".repeat(MAX_RESPONSE_HEADER_NAME_BYTES + 1);
        for name in [
            "",
            "X-Request-Id",
            "x request",
            "x:a",
            "\u{e9}",
            "set-cookie",
            "set-cookie2",
            long.as_str(),
        ] {
            assert!(!declarable_response_header(name), "{name}");
        }
        let mut sorted = SAFELISTED_RESPONSE_HEADERS;
        sorted.sort_unstable();
        assert_eq!(sorted, SAFELISTED_RESPONSE_HEADERS);
        assert!(
            SAFELISTED_RESPONSE_HEADERS
                .iter()
                .all(|name| declarable_response_header(name))
        );
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
