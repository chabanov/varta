//! Recorded network completion logs (v38, `net_request_start_v2`): the
//! versioned format a native host records from real transfers and every host
//! replays in place of its transport, and the divergence errors replay
//! reports. Shared by the native runtime and the generated browser host.
//!
//! A log lists the requests that reached the transport, in start order
//! ([`LoggedRequest`]: method and URL), and every completion the guest
//! received from a transport, in delivery order ([`LoggedEvent`]): heads,
//! body chunks with the read mode that armed them, ends, failures, and the
//! guest's aborts of open transfers. A head keeps only the status, status
//! text, resolved `Location` and the response headers the recording grant
//! showed, in canonical form ([`super::HeadForm::Canonical`]); a replaying
//! host rebuilds the head record from them with its own request URL and
//! visible names, so a replay never shows a header the replay grant hides.
//!
//! Replay does not bypass authorization: each request is checked against
//! the `net.request` grant (and the host ceiling) exactly as a real one,
//! and a host refuses a log naming a request its grants do not admit before
//! the instance starts. Request immediate completions (denials, invalid
//! URLs or requests) are produced by the host, never logged. Grants decide;
//! the log only supplies what a granted transport would have delivered.
use super::{
    HeadForm, MAX_READ_DELIVERY_BYTES, MAX_RESPONSE_HEADER_BYTES, MAX_RESPONSE_HEADERS,
    MAX_URL_BYTES, declarable_response_header, head_record, valid_method,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The `format` member of every log.
pub const LOG_FORMAT: &str = "varta-network-log";
/// The `version` member of a log this contract reads and writes.
pub const LOG_VERSION: u32 = 1;
/// Bytes of one log file (its JSON text), read or recorded.
pub const MAX_LOG_BYTES: usize = 16 * 1024 * 1024;
/// Requests one log lists.
pub const MAX_LOG_REQUESTS: usize = 4096;
/// Events one log lists.
pub const MAX_LOG_EVENTS: usize = 65_536;
/// Bytes of the text of one logged failure.
pub const MAX_FAILURE_TEXT_BYTES: usize = 8 * 1024;
/// The type of an event's `step` and `request`. The reader bounds both
/// through this type, so [`MAX_LOG_INDEX`] (exported to other hosts) is
/// derived from it and cannot drift from what the reader admits.
pub type LogIndex = u32;
/// The step of an event recorded outside a scripted session.
pub const UNSCRIPTED_STEP: LogIndex = 0;
/// The largest `step` and `request` an event carries: a reader refuses a
/// larger number, since it does not fit [`LogIndex`].
pub const MAX_LOG_INDEX: LogIndex = LogIndex::MAX;
/// The lowest and highest status a logged head may carry: the three-digit
/// codes an HTTP/1.1 status line has.
pub const MIN_STATUS: u16 = 100;
pub const MAX_STATUS: u16 = 999;

/// One request that reached a transport, in start order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggedRequest {
    /// The method as the request descriptor carried it.
    pub method: String,
    /// The request URL as the host parsed it (WHATWG serialization).
    pub url: String,
}

/// How the body read that a chunk answered was armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoggedRead {
    /// `net_body_read`: the chunk text is decoded UTF-8.
    Text,
    /// `net_body_read_bytes`: each code unit of the chunk text is one byte.
    Bytes,
}
impl LoggedRead {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Bytes => "bytes",
        }
    }
}
impl std::fmt::Display for LoggedRead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One logged event. `step` is the scripted-session step during which the
/// guest received it (1-based; the settling after the last step counts as
/// one more step), or [`UNSCRIPTED_STEP`] outside a scripted session;
/// `request` indexes [`NetworkLog::requests`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LoggedEvent {
    /// The response head.
    Head {
        step: LogIndex,
        request: LogIndex,
        status: u16,
        status_text: String,
        /// The `Location` header resolved against the request URL, or the
        /// empty string.
        location: String,
        /// Canonical visible names alternating with their values.
        headers: Vec<String>,
    },
    /// One body chunk, as the read delivered it.
    Chunk {
        step: LogIndex,
        request: LogIndex,
        read: LoggedRead,
        text: String,
    },
    /// The end of the body.
    End { step: LogIndex, request: LogIndex },
    /// The transfer failed; `text` is the completion text.
    Failed {
        step: LogIndex,
        request: LogIndex,
        text: String,
    },
    /// The guest aborted the open transfer.
    Abort { step: LogIndex, request: LogIndex },
}
impl LoggedEvent {
    /// The wire names of the event kinds, in declaration order.
    pub const KINDS: [&'static str; 5] = ["head", "chunk", "end", "failed", "abort"];
    #[must_use]
    pub fn step(&self) -> u32 {
        match self {
            Self::Head { step, .. }
            | Self::Chunk { step, .. }
            | Self::End { step, .. }
            | Self::Failed { step, .. }
            | Self::Abort { step, .. } => *step,
        }
    }
    #[must_use]
    pub fn request(&self) -> u32 {
        match self {
            Self::Head { request, .. }
            | Self::Chunk { request, .. }
            | Self::End { request, .. }
            | Self::Failed { request, .. }
            | Self::Abort { request, .. } => *request,
        }
    }
    /// Whether the event ends its request's transfer.
    #[must_use]
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            Self::End { .. } | Self::Failed { .. } | Self::Abort { .. }
        )
    }
    /// The head record a replaying host delivers for a head event of a
    /// request to `url` whose grant shows `visible`: the logged names the
    /// replay grant also shows, in canonical form. `None` for other events.
    #[must_use]
    pub fn head_text(&self, url: &str, visible: &BTreeSet<String>) -> Option<String> {
        let Self::Head {
            status,
            status_text,
            location,
            headers,
            ..
        } = self
        else {
            return None;
        };
        let fields = headers
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[name, value]| (name.clone(), value.clone()));
        let headers = HeadForm::Canonical(visible.clone()).headers(fields);
        Some(head_record(*status, status_text, url, location, headers))
    }
}

/// A complete, validated log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkLog {
    pub format: String,
    pub version: u32,
    pub requests: Vec<LoggedRequest>,
    pub events: Vec<LoggedEvent>,
}
impl Default for NetworkLog {
    fn default() -> Self {
        Self {
            format: LOG_FORMAT.to_owned(),
            version: LOG_VERSION,
            requests: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// What a log violates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkLogError {
    #[error("network log exceeds {MAX_LOG_BYTES} bytes")]
    TooLarge,
    #[error("network log is no valid log JSON: {0}")]
    Json(String),
    #[error("network log format must be {LOG_FORMAT} version {LOG_VERSION}")]
    Format,
    #[error("network log lists more than {MAX_LOG_REQUESTS} requests")]
    Requests,
    #[error("network log lists more than {MAX_LOG_EVENTS} events")]
    Events,
    #[error("network log request {index} has an invalid method or URL")]
    Request { index: usize },
    #[error("network log event {index} is invalid: {violation}")]
    Event {
        index: usize,
        violation: EventViolation,
    },
}

/// Why one logged event is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EventViolation {
    #[error("it names no logged request")]
    Request,
    #[error("its step precedes the previous event's")]
    Step,
    #[error("its request is not open for it")]
    Order,
    #[error("its status is no three-digit code")]
    Status,
    #[error("its status text or a header value is no field text")]
    FieldText,
    #[error("its headers are no canonical visible names and values within the head limits")]
    Headers,
    #[error("its location is no URL")]
    Location,
    #[error("its chunk is empty, beyond one read's delivery or no bytes")]
    Chunk,
    #[error("its failure text is empty or too large")]
    Failure,
}

/// The lifecycle of one logged request while validating.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Life {
    /// Started; no head yet.
    Started,
    /// The head arrived; the body is open.
    Open,
    /// Ended, failed or aborted.
    Closed,
}

impl NetworkLog {
    /// Parse and validate the bytes of a log file.
    pub fn parse(bytes: &[u8]) -> Result<Self, NetworkLogError> {
        if bytes.len() > MAX_LOG_BYTES {
            return Err(NetworkLogError::TooLarge);
        }
        let log: Self = serde_json::from_slice(bytes)
            .map_err(|error| NetworkLogError::Json(error.to_string()))?;
        log.validate()?;
        Ok(log)
    }

    /// The compact JSON text of the log.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        // A log holds only strings, integers and arrays: serialization
        // cannot fail.
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Check the format, the bounds, every request and the lifecycle of
    /// every request's events: a head or a failure first, then chunks, then
    /// one end or failure; an abort may close a request at any point, and
    /// nothing follows a closing event. Steps never decrease.
    pub fn validate(&self) -> Result<(), NetworkLogError> {
        if self.format != LOG_FORMAT || self.version != LOG_VERSION {
            return Err(NetworkLogError::Format);
        }
        if self.requests.len() > MAX_LOG_REQUESTS {
            return Err(NetworkLogError::Requests);
        }
        if self.events.len() > MAX_LOG_EVENTS {
            return Err(NetworkLogError::Events);
        }
        for (index, request) in self.requests.iter().enumerate() {
            if !valid_request(request) {
                return Err(NetworkLogError::Request { index });
            }
        }
        let mut lives = vec![Life::Started; self.requests.len()];
        let mut step = UNSCRIPTED_STEP;
        for (index, event) in self.events.iter().enumerate() {
            let fail = |violation| NetworkLogError::Event { index, violation };
            let life = usize::try_from(event.request())
                .ok()
                .and_then(|request| lives.get_mut(request))
                .ok_or(fail(EventViolation::Request))?;
            if event.step() < step {
                return Err(fail(EventViolation::Step));
            }
            step = event.step();
            *life = next_life(*life, event).ok_or(fail(EventViolation::Order))?;
            event.check().map_err(fail)?;
        }
        Ok(())
    }
}

fn next_life(life: Life, event: &LoggedEvent) -> Option<Life> {
    match (life, event) {
        (Life::Started, LoggedEvent::Head { .. }) => Some(Life::Open),
        (Life::Open, LoggedEvent::Chunk { .. }) => Some(Life::Open),
        (Life::Open, LoggedEvent::End { .. })
        | (Life::Started | Life::Open, LoggedEvent::Failed { .. } | LoggedEvent::Abort { .. }) => {
            Some(Life::Closed)
        }
        _ => None,
    }
}

/// A method `fetch` may send and an absolute URL in its own serialization.
fn valid_request(request: &LoggedRequest) -> bool {
    valid_method(&request.method)
        && request.url.len() <= MAX_URL_BYTES
        && url::Url::parse(&request.url).is_ok_and(|url| url.as_str() == request.url)
}

/// Field text: code units of bytes (as a WebIDL ByteString) that a header
/// field value admits, horizontal tab included.
fn field_text(text: &str) -> bool {
    const TAB: char = '\t';
    const DELETE: char = '\u{7f}';
    text.chars().all(|unit| {
        u32::from(unit) <= u32::from(u8::MAX)
            && (!unit.is_ascii_control() || unit == TAB)
            && unit != DELETE
    })
}

impl LoggedEvent {
    /// Check the event's own content: what a log reader and a recorder
    /// require of every event, apart from its place in the log.
    pub fn check(&self) -> Result<(), EventViolation> {
        check_event(self)
    }
}

fn check_event(event: &LoggedEvent) -> Result<(), EventViolation> {
    match event {
        LoggedEvent::Head {
            status,
            status_text,
            location,
            headers,
            ..
        } => {
            if !(MIN_STATUS..=MAX_STATUS).contains(status) {
                return Err(EventViolation::Status);
            }
            if !field_text(status_text) {
                return Err(EventViolation::FieldText);
            }
            check_headers(headers)?;
            if !location.is_empty()
                && (location.len() > MAX_URL_BYTES || url::Url::parse(location).is_err())
            {
                return Err(EventViolation::Location);
            }
            Ok(())
        }
        LoggedEvent::Chunk { read, text, .. } => {
            let bytes = *read == LoggedRead::Bytes;
            if text.is_empty()
                || text.encode_utf16().count() > MAX_READ_DELIVERY_BYTES
                || bytes
                    && text
                        .chars()
                        .any(|unit| u32::from(unit) > u32::from(u8::MAX))
            {
                return Err(EventViolation::Chunk);
            }
            Ok(())
        }
        LoggedEvent::Failed { text, .. } => {
            if text.is_empty() || text.len() > MAX_FAILURE_TEXT_BYTES {
                return Err(EventViolation::Failure);
            }
            Ok(())
        }
        LoggedEvent::End { .. } | LoggedEvent::Abort { .. } => Ok(()),
    }
}

/// Canonical names (declarable, strictly ascending) alternating with field
/// text values, within the response head limits.
fn check_headers(headers: &[String]) -> Result<(), EventViolation> {
    let (pairs, rest) = headers.as_chunks::<2>();
    if !rest.is_empty() || pairs.len() > MAX_RESPONSE_HEADERS {
        return Err(EventViolation::Headers);
    }
    let mut bytes = 0usize;
    let mut previous: Option<&str> = None;
    for [name, value] in pairs {
        // Field text is one wire byte per code unit (an obs-text byte is one
        // unit above 0x7F, two UTF-8 bytes in the log text), as the head
        // limit counts it and the browser reader does.
        bytes = bytes.saturating_add(name.chars().count() + value.chars().count());
        if !declarable_response_header(name)
            || previous.is_some_and(|previous| previous >= name.as_str())
            || bytes > MAX_RESPONSE_HEADER_BYTES
        {
            return Err(EventViolation::Headers);
        }
        if !field_text(value) {
            return Err(EventViolation::FieldText);
        }
        previous = Some(name);
    }
    Ok(())
}

/// Whether recording and replay refuse a module importing `operation`: the
/// direct network reads `varta.net.get` and `varta.net.fetch`, which reach
/// the network outside the request transport.
#[must_use]
pub fn unserved(operation: crate::HostOperation) -> bool {
    use crate::HostOperation as H;
    matches!(
        operation,
        H::NetGet | H::NetGet16 | H::NetFetch | H::NetFetch16
    )
}

/// Replay failures. Each is fatal for the instance: the guest received
/// nothing the log does not hold, and a diverging session stops where it
/// diverged. `request` is a 0-based index into [`NetworkLog::requests`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    /// Checked before the instance starts: the grants (and the host
    /// ceiling) do not admit a logged request.
    #[error("network replay refused: logged request {request} ({method} {url}) is not granted")]
    Ungranted {
        request: u32,
        method: String,
        url: String,
    },
    /// A request reached the transport after every logged request.
    #[error("network replay diverged: request {method} {url} is not in the log")]
    Unlogged { method: String, url: String },
    /// A request reached the transport with another method or URL than the
    /// logged request of its place.
    #[error(
        "network replay diverged: request {request} is {method} {url}, the log recorded {logged_method} {logged_url}"
    )]
    Request {
        request: u32,
        method: String,
        url: String,
        logged_method: String,
        logged_url: String,
    },
    /// A body read was armed in another mode than the logged chunk.
    #[error("network replay diverged: request {request} read {actual}, the log recorded {logged}")]
    Read {
        request: u32,
        actual: LoggedRead,
        logged: LoggedRead,
    },
    /// The guest aborted a transfer where the log does not.
    #[error("network replay diverged: request {request} was aborted where the log continues")]
    Abort { request: u32 },
    /// An armed operation waits and the next logged event cannot reach it.
    #[error(
        "network replay stalled at log event {event}: no logged completion can reach an armed request"
    )]
    Stalled { event: usize },
    /// The session ended with logged events never delivered.
    #[error("network replay ended with {remaining} logged events undelivered")]
    Unconsumed { remaining: usize },
    /// Recording and replay serve `net_request_start_v2` requests only.
    #[error("network recording and replay serve net_request_start_v2 requests only")]
    Descriptor,
    /// Checked before the instance starts: the module imports a network
    /// operation that bypasses the request transport a log records
    /// ([`unserved`]), so a recording would miss it and a replay would
    /// reach the network.
    #[error(
        "network recording and replay refuse a module importing {import}: only net_request_start_v2 requests are recorded and replayed"
    )]
    Unserved { import: String },
    /// A recording would exceed the log limits.
    #[error(
        "network recording exceeds its limits ({MAX_LOG_REQUESTS} requests, {MAX_LOG_EVENTS} events, {MAX_LOG_BYTES} bytes)"
    )]
    RecordingLimit,
    /// A recorded completion could not be logged (a failure text beyond its
    /// limit or a head that is no head record).
    #[error("network recording cannot log the completion of request {request}")]
    Unrecordable { request: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(events: Vec<LoggedEvent>) -> NetworkLog {
        NetworkLog {
            requests: vec![LoggedRequest {
                method: "GET".into(),
                url: "http://127.0.0.1:8080/items".into(),
            }],
            events,
            ..NetworkLog::default()
        }
    }
    fn head(headers: &[&str]) -> LoggedEvent {
        LoggedEvent::Head {
            step: 1,
            request: 0,
            status: 200,
            status_text: "OK".into(),
            location: String::new(),
            headers: headers.iter().map(|text| (*text).to_owned()).collect(),
        }
    }
    fn chunk(read: LoggedRead, text: &str) -> LoggedEvent {
        LoggedEvent::Chunk {
            step: 1,
            request: 0,
            read,
            text: text.into(),
        }
    }
    const END: LoggedEvent = LoggedEvent::End {
        step: 1,
        request: 0,
    };

    #[test]
    fn logs_round_trip_through_their_wire_form() {
        let original = log(vec![
            head(&["content-type", "text/event-stream"]),
            chunk(LoggedRead::Text, "data: é\n\n"),
            chunk(LoggedRead::Bytes, "\u{0}\u{ff}"),
            END,
        ]);
        let text = String::from_utf8(original.to_json()).unwrap();
        assert!(text.starts_with(r#"{"format":"varta-network-log","version":1,"#));
        assert!(text.contains(r#"{"kind":"head","step":1,"request":0,"status":200,"statusText":"OK","location":"","headers":["content-type","text/event-stream"]}"#));
        assert!(
            text.contains(
                r#"{"kind":"chunk","step":1,"request":0,"read":"bytes","text":"\u0000ÿ"}"#
            )
        );
        assert_eq!(NetworkLog::parse(text.as_bytes()).unwrap(), original);
    }

    #[test]
    fn invalid_logs_are_refused_with_their_violation() {
        let event = |events, violation| {
            assert_eq!(
                log(events).validate(),
                Err(NetworkLogError::Event {
                    index: 0,
                    violation
                })
            );
        };
        event(vec![chunk(LoggedRead::Text, "x")], EventViolation::Order);
        event(vec![END], EventViolation::Order);
        event(
            vec![head(&["x-b", "1", "x-a", "2"])],
            EventViolation::Headers,
        );
        event(vec![head(&["set-cookie", "a"])], EventViolation::Headers);
        event(vec![head(&["x-a", "a\r\nb"])], EventViolation::FieldText);
        let mut wrong = log(vec![head(&[])]);
        wrong.events[0] = LoggedEvent::Abort {
            step: 0,
            request: 1,
        };
        assert!(matches!(
            wrong.validate(),
            Err(NetworkLogError::Event {
                violation: EventViolation::Request,
                ..
            })
        ));
        let closed = log(vec![head(&[]), END, chunk(LoggedRead::Text, "late")]);
        assert_eq!(
            closed.validate(),
            Err(NetworkLogError::Event {
                index: 2,
                violation: EventViolation::Order
            })
        );
        let empty = log(vec![head(&[]), chunk(LoggedRead::Text, "")]);
        assert!(empty.validate().is_err());
        let wide = log(vec![head(&[]), chunk(LoggedRead::Bytes, "ā")]);
        assert!(wide.validate().is_err());
        let long = log(vec![
            head(&[]),
            chunk(LoggedRead::Text, &"x".repeat(MAX_READ_DELIVERY_BYTES + 1)),
        ]);
        assert!(long.validate().is_err());
        let backwards = log(vec![
            head(&[]),
            LoggedEvent::End {
                step: 0,
                request: 0,
            },
        ]);
        assert_eq!(
            backwards.validate(),
            Err(NetworkLogError::Event {
                index: 1,
                violation: EventViolation::Step
            })
        );
        let mut request = log(Vec::new());
        request.requests[0].url = "HTTP://127.0.0.1:8080/items".into();
        assert_eq!(
            request.validate(),
            Err(NetworkLogError::Request { index: 0 })
        );
        request.requests[0].url = "http://127.0.0.1:8080/items".into();
        request.requests[0].method = "TRACE".into();
        assert!(request.validate().is_err());
        let mut format = log(Vec::new());
        format.version = LOG_VERSION + 1;
        assert_eq!(format.validate(), Err(NetworkLogError::Format));
        assert!(matches!(
            NetworkLog::parse(
                br#"{"format":"varta-network-log","version":1,"requests":[],"events":[],"extra":1}"#
            ),
            Err(NetworkLogError::Json(_))
        ));
        assert!(matches!(
            NetworkLog::parse(
                br#"{"format":"varta-network-log","version":1,"requests":[],"events":[{"kind":"end","step":0,"request":0,"extra":1}]}"#
            ),
            Err(NetworkLogError::Json(_))
        ));
        assert!(NetworkLog::parse(&vec![b' '; MAX_LOG_BYTES + 1]).is_err());
        // The browser reader refuses these as well (network-replay.test.mjs):
        // an index beyond 32 bits, a float, a duplicate member, an escaped
        // lone surrogate, a literal where a field needs another type (no log
        // field is boolean or null) and a byte order mark.
        let prefix = r#"{"format":"varta-network-log","version":1,"requests":[{"method":"GET","url":"http://127.0.0.1:8080/items"}],"events":["#;
        let beyond = u64::from(MAX_LOG_INDEX) + 1;
        let beyond = format!(r#"{{"kind":"abort","step":{beyond},"request":0}}"#);
        for event in [
            beyond.as_str(),
            r#"{"kind":"head","step":1,"request":0,"status":200.0,"statusText":"OK","location":"","headers":[]}"#,
            r#"{"kind":"abort","step":1,"step":1,"request":0}"#,
            r#"{"kind":"failed","step":1,"request":0,"text":"\ud800"}"#,
            r#"{"kind":"head","step":1,"request":0,"status":200,"statusText":"OK","location":true,"headers":[]}"#,
            r#"{"kind":"head","step":1,"request":0,"status":200,"statusText":"OK","location":false,"headers":[]}"#,
            r#"{"kind":"head","step":1,"request":0,"status":200,"statusText":"OK","location":null,"headers":[]}"#,
        ] {
            let text = format!("{prefix}{event}]}}");
            assert!(
                matches!(
                    NetworkLog::parse(text.as_bytes()),
                    Err(NetworkLogError::Json(_))
                ),
                "{text}"
            );
        }
        let bom = format!("\u{feff}{prefix}]}}");
        assert!(NetworkLog::parse(bom.as_bytes()).is_err());
        NetworkLog::parse(format!("{prefix}]}}").as_bytes()).unwrap();
        // The exported bound itself is admitted.
        let last = format!(r#"{prefix}{{"kind":"abort","step":{MAX_LOG_INDEX},"request":0}}]}}"#);
        NetworkLog::parse(last.as_bytes()).unwrap();
    }

    /// The response head limit counts field text as wire bytes, one per
    /// code unit, so an obs-text value fills it exactly as on the wire.
    #[test]
    fn head_limits_count_one_byte_per_field_text_unit() {
        const NAME: &str = "x-a";
        let fill = |units: usize| {
            log(vec![head(&[NAME, &"\u{e9}".repeat(units - NAME.len())])]).validate()
        };
        fill(MAX_RESPONSE_HEADER_BYTES).unwrap();
        assert_eq!(
            fill(MAX_RESPONSE_HEADER_BYTES + 1),
            Err(NetworkLogError::Event {
                index: 0,
                violation: EventViolation::Headers
            })
        );
    }

    #[test]
    fn aborts_and_failures_close_requests_before_or_after_the_head() {
        let failed = LoggedEvent::Failed {
            step: 1,
            request: 0,
            text: "request timed out".into(),
        };
        let abort = LoggedEvent::Abort {
            step: 1,
            request: 0,
        };
        for events in [
            vec![failed.clone()],
            vec![abort.clone()],
            vec![head(&[]), chunk(LoggedRead::Text, "a"), failed],
            vec![head(&[]), abort.clone()],
            vec![head(&[])],
            Vec::new(),
        ] {
            log(events).validate().unwrap();
        }
        assert!(log(vec![abort.clone(), abort]).validate().is_err());
    }

    /// A replayed head shows the logged names its own grant also shows,
    /// with its own URL, as the canonical record a real transfer produces.
    #[test]
    fn replayed_heads_show_only_what_the_replay_grant_shows() {
        let event = head(&["content-type", "text/plain", "x-id", "7"]);
        let visible: BTreeSet<String> = ["content-type".to_owned()].into();
        assert_eq!(
            event.head_text("http://127.0.0.1:1/a", &visible).unwrap(),
            r#"{"headers":["content-type","text/plain"],"location":"","status":200,"statusText":"OK","url":"http://127.0.0.1:1/a"}"#
        );
        assert_eq!(END.head_text("http://127.0.0.1:1/a", &visible), None);
    }
}
