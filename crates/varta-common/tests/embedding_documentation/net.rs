//! The network-request contract of `docs/embedding.md` beyond its limits:
//! the request descriptor grammar (with examples that go through the real
//! `net::RequestDescriptor` parser), the runtime-controlled headers, the
//! completion kinds and their texts, the head record, the recoverable
//! failure texts and the fatal `NetError`s, generated from `varta_common::net`.
use super::DOCUMENT;
use crate::support::{assert_block, table};
use varta_common::net::{
    self, CompletionKind, DescriptorVersion, NetError, RequestDescriptor, RequestFailure,
};
use varta_common::scheduler::SchedulerError;

fn import(version: DescriptorVersion) -> &'static str {
    match version {
        DescriptorVersion::V1 => "`net_request_start`",
        DescriptorVersion::V2 => "`net_request_start_v2`",
    }
}

fn code(text: &str) -> String {
    format!("`{text}`")
}

/// A descriptor as a JSON string literal, so separators and control units
/// stay visible.
fn literal(text: &str) -> String {
    // Infallible: a string always serializes.
    code(&serde_json::to_string(text).unwrap())
}

fn descriptor_items() -> String {
    let mut rows = Vec::new();
    for version in DescriptorVersion::ALL {
        rows.push(vec![
            format!("First line of {}", import(version)),
            code(version.line()),
            code(match version {
                DescriptorVersion::V1 => "net::DESCRIPTOR_VERSION",
                DescriptorVersion::V2 => "net::DESCRIPTOR_VERSION_BYTES",
            }),
        ]);
    }
    rows.push(vec![
        "Field separator (the only one; no `\\r`)".to_owned(),
        format!("U+{:04X} (`\\n`)", u32::from(net::DESCRIPTOR_SEPARATOR)),
        code("net::DESCRIPTOR_SEPARATOR"),
    ]);
    let flags = [
        (
            net::DESCRIPTOR_NO_BODY,
            "No body; the descriptor ends with the separator after the flag",
            "net::DESCRIPTOR_NO_BODY",
        ),
        (
            net::DESCRIPTOR_BODY,
            "Text body: the rest of the descriptor, sent as UTF-8",
            "net::DESCRIPTOR_BODY",
        ),
        (
            net::DESCRIPTOR_BYTES,
            "Byte body: the rest, one byte per code unit 0x00–0xFF",
            "net::DESCRIPTOR_BYTES",
        ),
    ];
    for (flag, meaning, constant) in flags {
        let admitted: Vec<&str> = DescriptorVersion::ALL
            .into_iter()
            .filter(|version| version.body_flags().contains(&flag))
            .map(import)
            .collect();
        rows.push(vec![
            format!("Body flag: {meaning} ({})", admitted.join(", ")),
            code(flag),
            code(constant),
        ]);
    }
    rows.push(vec![
        "Largest header count".to_owned(),
        net::MAX_REQUEST_HEADERS.to_string(),
        code("net::MAX_REQUEST_HEADERS"),
    ]);
    table(&["Item", "Value", "Source constant"], rows)
}

/// The descriptor the preludes write: the canonical form of the grammar.
fn encode(
    version: DescriptorVersion,
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<(&str, &str)>,
) -> String {
    let separator = net::DESCRIPTOR_SEPARATOR;
    let mut text = format!(
        "{}{separator}{method}{separator}{url}{separator}{}{separator}",
        version.line(),
        headers.len()
    );
    for (name, value) in headers {
        text.push_str(&format!("{name}{separator}{value}{separator}"));
    }
    match body {
        None => text.push_str(&format!("{}{separator}", net::DESCRIPTOR_NO_BODY)),
        Some((flag, content)) => text.push_str(&format!("{flag}{separator}{content}")),
    }
    text
}

fn fields(parsed: &RequestDescriptor<'_>) -> String {
    let headers = if parsed.headers.is_empty() {
        "none".to_owned()
    } else {
        parsed
            .headers
            .iter()
            .map(|(name, value)| format!("{} = {}", code(name), literal(value)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let body = match &parsed.body {
        None => "none".to_owned(),
        Some(bytes) if bytes.is_empty() => "empty".to_owned(),
        Some(bytes) => {
            let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
            format!("{} byte(s) `{}`", bytes.len(), hex.join(" "))
        }
    };
    format!(
        "method {}, URL {}; headers {headers}; body {body}",
        code(parsed.method),
        code(parsed.url)
    )
}

/// Accepted descriptors: each is the canonical encoding of its fields unless
/// marked otherwise, and its documented fields come from the real parser.
fn descriptor_examples() -> String {
    use DescriptorVersion::{V1, V2};
    let text_body = Some((net::DESCRIPTOR_BODY, "a\nb"));
    let canonical = [
        (
            V1,
            "GET",
            "https://api.example.test/v1/items",
            &[][..],
            None,
        ),
        (
            V1,
            "POST",
            "https://api.example.test/v1/items",
            &[
                ("content-type", "text/plain;charset=UTF-8"),
                ("x-empty", ""),
            ][..],
            text_body,
        ),
        (V2, "GET", "http://127.0.0.1:8080/", &[][..], None),
        (
            V2,
            "POST",
            "https://api.example.test/upload",
            &[][..],
            Some((net::DESCRIPTOR_BODY, "\u{e9}")),
        ),
        (
            V2,
            "PUT",
            "https://api.example.test/blob",
            &[("content-type", "application/octet-stream")][..],
            Some((net::DESCRIPTOR_BYTES, "\u{0}\u{e9}\u{ff}")),
        ),
        (
            V1,
            "POST",
            "https://api.example.test/v1/items",
            &[][..],
            Some((net::DESCRIPTOR_BODY, "")),
        ),
    ];
    let mut rows = Vec::new();
    for (version, method, url, headers, body) in canonical {
        let text = encode(version, method, url, headers, body);
        let parsed = RequestDescriptor::parse(&text, version)
            .unwrap_or_else(|error| panic!("{text:?} does not parse: {error}"));
        assert_eq!(
            (parsed.method, parsed.url, parsed.headers.as_slice()),
            (method, url, headers),
            "{text:?} does not round-trip"
        );
        rows.push(vec![
            import(version).to_owned(),
            literal(&text),
            "canonical".to_owned(),
            fields(&parsed),
        ]);
    }
    // The count is read as an unsigned decimal integer, so the reference
    // parser also takes a `+` sign and leading zeros (the preludes write
    // neither).
    let separator = net::DESCRIPTOR_SEPARATOR;
    for count in ["+1", "01"] {
        let text = format!(
            "{}{separator}GET{separator}http://a.test/{separator}{count}{separator}accept{separator}*/*{separator}{}{separator}",
            V1.line(),
            net::DESCRIPTOR_NO_BODY
        );
        let parsed = RequestDescriptor::parse(&text, V1)
            .unwrap_or_else(|error| panic!("{text:?} does not parse: {error}"));
        rows.push(vec![
            import(V1).to_owned(),
            literal(&text),
            "accepted, not canonical".to_owned(),
            fields(&parsed),
        ]);
    }
    table(
        &["Import", "Descriptor (JSON string)", "Form", "Fields"],
        rows,
    )
}

/// Structurally malformed descriptors: each is a fatal
/// `NetError::Descriptor` through the real parser.
fn descriptor_rejections() -> String {
    use DescriptorVersion::{V1, V2};
    let separator = net::DESCRIPTOR_SEPARATOR;
    let head = |version: DescriptorVersion| {
        format!(
            "{}{separator}GET{separator}http://a.test/{separator}",
            version.line()
        )
    };
    let none = net::DESCRIPTOR_NO_BODY;
    let cases = [
        (
            V2,
            format!("{}0{separator}{none}{separator}", head(V1)),
            "version 1 line under the version 2 import",
        ),
        (
            V1,
            format!("{}0{separator}{none}{separator}", head(V2)),
            "version 2 line under the version 1 import",
        ),
        (
            V1,
            format!("{}0{separator}{none}", head(V1)),
            "no separator after the body flag",
        ),
        (
            V1,
            format!("{}0{separator}{none}{separator}x", head(V1)),
            "text after the no-body flag",
        ),
        (
            V1,
            format!(
                "{}0{separator}{}{separator}ab",
                head(V1),
                net::DESCRIPTOR_BYTES
            ),
            "byte-body flag under version 1",
        ),
        (
            V2,
            format!(
                "{}0{separator}{}{separator}\u{100}",
                head(V2),
                net::DESCRIPTOR_BYTES
            ),
            "byte body with a code unit above 0xFF",
        ),
        (
            V1,
            format!(
                "{}{}{separator}{none}{separator}",
                head(V1),
                net::MAX_REQUEST_HEADERS + 1
            ),
            "header count above the limit",
        ),
        (
            V1,
            format!("{}1{separator}accept{separator}{none}{separator}", head(V1)),
            "fewer header lines than the count",
        ),
        (
            V1,
            format!("{}x{separator}{none}{separator}", head(V1)),
            "count not a decimal integer",
        ),
        (V1, format!("{}0{separator}", head(V1)), "no body flag"),
    ];
    table(
        &["Import", "Descriptor (JSON string)", "Why it is malformed"],
        cases.iter().map(|(version, text, why)| {
            assert_eq!(
                RequestDescriptor::parse(text, *version).err(),
                Some(NetError::Descriptor),
                "{text:?} under {why}"
            );
            vec![
                import(*version).to_owned(),
                literal(text),
                (*why).to_owned(),
            ]
        }),
    )
}

fn runtime_headers() -> String {
    table(
        &["Header (matched ASCII case-insensitively)"],
        net::RUNTIME_HEADERS.iter().map(|name| vec![code(name)]),
    )
}

fn completions() -> String {
    table(
        &["Code", "Kind", "Releases the request", "`net_request_text`"],
        CompletionKind::ALL.iter().map(|kind| {
            // Exhaustive on purpose: a new kind fails compilation here.
            let (name, text) = match kind {
                CompletionKind::Head => ("Head", "The [head record](#head-record)".to_owned()),
                CompletionKind::Chunk => (
                    "Chunk",
                    "One nonempty body chunk: decoded UTF-8 after `net_body_read`, the bytes as code units 0x00–0xFF after `net_body_read_bytes` ([body chunks](#body-chunks))"
                        .to_owned(),
                ),
                CompletionKind::End => ("End", "Empty".to_owned()),
                CompletionKind::Failed => (
                    "Failed",
                    "The failure text ([failures](#request-failures))".to_owned(),
                ),
                CompletionKind::Aborted => ("Aborted", "Empty".to_owned()),
                CompletionKind::Denied => (
                    "Denied",
                    format!("The denial: {}", code(&net::denial_text("<method>", "<url>"))),
                ),
            };
            vec![
                (*kind as i32).to_string(),
                code(name),
                if kind.terminal() { "yes" } else { "no" }.to_owned(),
                text,
            ]
        }),
    )
}

fn head_fields() -> String {
    table(
        &["Member", "JSON type", "Value"],
        net::HEAD_FIELDS.iter().map(|field| {
            let (kind, value) = match *field {
                "status" => ("number", "The status code as received"),
                "statusText" => (
                    "string",
                    "The reason phrase exactly as received, each byte one code unit (empty if the server sent none)",
                ),
                "url" => (
                    "string",
                    "The request URL as the runtime parsed it (its WHATWG serialization)",
                ),
                "location" => (
                    "string",
                    "The first `location` value resolved against `url` when it is visible ASCII and resolves; otherwise empty",
                ),
                "headers" => (
                    "array of strings",
                    "Lowercase name, value, name, value, …: names in the order of their first field, each repeated field its own pair in arrival order (never combined), each value byte one code unit",
                ),
                other => panic!("undocumented head field {other}"),
            };
            vec![code(field), kind.to_owned(), value.to_owned()]
        }),
    )
}

fn head_example() -> String {
    let headers = [
        "content-type",
        "application/json",
        "set-cookie",
        "a=1",
        "set-cookie",
        "b=2",
    ]
    .map(str::to_owned)
    .to_vec();
    let record = net::head_record(200, "OK", "https://api.example.test/v1/items", "", headers);
    format!("```json\n{record}\n```")
}

const METHOD: &str = "<method>";
const URL: &str = "<url>";
const NAME: &str = "<name>";
const ERROR: &str = "<error>";

/// Where a failure arises and why.
fn failure_cause(failure: &RequestFailure) -> (&'static str, &'static str) {
    use RequestFailure as F;
    const START: &str = "`net_request_start*`";
    const TRANSFER: &str = "Transfer (head)";
    const BODY: &str = "Body read";
    // Exhaustive on purpose: a new failure fails compilation here.
    match failure {
        F::Method(_) => (
            START,
            "The method is no RFC 9110 token or is a forbidden method",
        ),
        F::Url(_) => (START, "The URL does not parse"),
        F::OutputBudget => (
            START,
            "The denial could not be logged within the output budget",
        ),
        F::BodyNotAllowed => (START, "A `GET` or `HEAD` request with a body"),
        F::HeaderBytes => (
            START,
            "The header names and values so far exceed the request header bytes",
        ),
        F::RuntimeHeader(_) => (
            START,
            "A [runtime header](#runtime-headers), named as the descriptor spells it",
        ),
        F::HeaderName(_) => (START, "A header name that is no field-name token"),
        F::HeaderValue(_) => (
            START,
            "A value with a unit above 0xFF or a byte no field value admits; the name lowercased",
        ),
        F::TlsConfig(_) => (
            START,
            "An `https` URL whose TLS configuration could not be built",
        ),
        F::NoHost => (TRANSFER, "The URL has no host"),
        F::NoPort => (
            TRANSFER,
            "The URL has no port and its scheme no default port",
        ),
        F::Connect(_) => (TRANSFER, "Name resolution or the TCP connect failed"),
        F::ServerName(_) => (TRANSFER, "The host is no TLS server name"),
        F::TlsHandshake(_) => (TRANSFER, "The TLS handshake or certificate check failed"),
        F::HttpHandshake(_) => (TRANSFER, "The HTTP/1.1 connection could not be set up"),
        F::Request(_) => (TRANSFER, "The request could not be built"),
        F::Send(_) => (
            TRANSFER,
            "Sending the request or reading the head failed (a head beyond the response limits included)",
        ),
        F::Timeout => (TRANSFER, "The head did not arrive before the deadline"),
        F::Body(_) => (BODY, "The body stream failed"),
        F::BodyLimit => (BODY, "The body exceeds the response body bytes"),
        F::BodyTimeout => (BODY, "No chunk arrived before the deadline"),
    }
}

/// Every recoverable failure and the denial, in the order the runtime checks
/// them.
fn failures() -> String {
    use RequestFailure as F;
    let failed = |failure: RequestFailure| {
        let (by, cause) = failure_cause(&failure);
        vec![
            by.to_owned(),
            code("Failed"),
            code(&failure.to_string()),
            cause.to_owned(),
        ]
    };
    let (start, _) = failure_cause(&F::Url(URL.into()));
    let mut rows = vec![
        failed(F::Method(METHOD.into())),
        failed(F::Url(URL.into())),
        vec![
            start.to_owned(),
            code("Denied"),
            code(&net::denial_text(METHOD, URL)),
            "The `net.request` grant or the host ceiling does not admit the URL and method"
                .to_owned(),
        ],
    ];
    rows.extend(
        [
            F::OutputBudget,
            F::BodyNotAllowed,
            F::HeaderBytes,
            F::RuntimeHeader(NAME.into()),
            F::HeaderName(NAME.into()),
            F::HeaderValue(NAME.into()),
            F::TlsConfig(ERROR.into()),
            F::NoHost,
            F::NoPort,
            F::Connect(ERROR.into()),
            F::ServerName(ERROR.into()),
            F::TlsHandshake(ERROR.into()),
            F::HttpHandshake(ERROR.into()),
            F::Request(ERROR.into()),
            F::Send(ERROR.into()),
            F::Timeout,
            F::Body(ERROR.into()),
            F::BodyLimit,
            F::BodyTimeout,
        ]
        .into_iter()
        .map(failed),
    );
    table(&["Raised by", "Kind", "Completion text", "Cause"], rows)
}

fn net_error_name(error: &NetError) -> &'static str {
    // Exhaustive on purpose: a new request error fails compilation here.
    match error {
        NetError::Budget => "Budget",
        NetError::Descriptor => "Descriptor",
        NetError::State => "State",
    }
}

fn errors() -> String {
    table(
        &["Error", "Reference diagnostic"],
        [NetError::Budget, NetError::Descriptor, NetError::State]
            .iter()
            .map(|error| {
                vec![
                    code(&format!("NetError::{}", net_error_name(error))),
                    code(&error.to_string()),
                ]
            }),
    )
}

/// Import misuse and the fatal error it raises, in check order per import.
fn misuse() -> String {
    let net = |error: NetError| code(&format!("NetError::{}", net_error_name(&error)));
    let unknown = code(&format!(
        "SchedulerError::{:?}",
        SchedulerError::UnknownSerial
    ));
    let start = "`net_request_start`, `net_request_start_v2`";
    let read = "`net_body_read`, `net_body_read_bytes`";
    let rows = vec![
        (start, "`ptr` or `len` negative, `len` odd or above the descriptor bytes, or the range outside the exported memory".to_owned(), net(NetError::Descriptor)),
        (start, "the descriptor does not match the [grammar](#request-descriptor) of this import's version".to_owned(), net(NetError::Descriptor)),
        (start, format!("{} requests already hold a slot (checked after the grant decision and its log line)", net::MAX_PENDING_REQUESTS), net(NetError::Budget)),
        ("`net_request_watch`", "serial not issued, 0, a timer's serial, or without exactly one registered reaction".to_owned(), unknown),
        ("`net_request_watch`", "id not a positive integer, or no request holds it (never issued or released)".to_owned(), net(NetError::State)),
        ("`net_request_watch`", "a waiter is already registered".to_owned(), net(NetError::State)),
        ("`net_request_watch`", "nothing armed and no completion waiting (the last head or chunk was taken and no read followed)".to_owned(), net(NetError::State)),
        ("`net_request_take`", "id not a positive integer, or no request holds it".to_owned(), net(NetError::State)),
        ("`net_request_take`", "no completion waiting (the operation is still armed, or the completion was taken)".to_owned(), net(NetError::State)),
        ("`net_request_text`", "id not a positive integer, nothing taken since the last text, or the last take was of another request".to_owned(), net(NetError::State)),
        (read, "id not a positive integer, or no request holds it".to_owned(), net(NetError::State)),
        (read, "an operation armed, a completion not yet taken, or a waiter registered".to_owned(), net(NetError::State)),
    ];
    table(
        &["Import", "Misuse", "Error"],
        rows.into_iter()
            .map(|(import, misuse, error)| vec![import.to_owned(), misuse, error]),
    )
}

#[test]
fn embedding_network_requests_match_the_contract() {
    assert_block(DOCUMENT, "embedding-net-descriptor", &descriptor_items());
    assert_block(
        DOCUMENT,
        "embedding-net-descriptor-examples",
        &descriptor_examples(),
    );
    assert_block(
        DOCUMENT,
        "embedding-net-descriptor-rejections",
        &descriptor_rejections(),
    );
    assert_block(
        DOCUMENT,
        "embedding-net-runtime-headers",
        &runtime_headers(),
    );
    assert_block(DOCUMENT, "embedding-net-completions", &completions());
    assert_block(DOCUMENT, "embedding-net-head-fields", &head_fields());
    assert_block(DOCUMENT, "embedding-net-head-example", &head_example());
    assert_block(DOCUMENT, "embedding-net-failures", &failures());
    assert_block(DOCUMENT, "embedding-net-errors", &errors());
    assert_block(DOCUMENT, "embedding-net-misuse", &misuse());
}
