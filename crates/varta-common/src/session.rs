//! Protected session facts shared by native hosts and the browser policy Wasm.
//! Application authentication and identity schemas never enter this module.
use alloc::string::String;

#[cfg(feature = "std")]
mod policy;
#[cfg(feature = "std")]
pub use policy::{ClientBinding, CookieSite, ServerPolicy};

pub const READ_CAPABILITY: &str = "session.read";
pub const WRITE_CAPABILITY: &str = "session.write";
pub const CLEAR_CAPABILITY: &str = "session.clear";
pub const CLIENT_CAPABILITY: &str = "net.session";
pub const HEADER: &str = "x-varta-session";
pub const HEADER_VALUE: &str = "1";
// Copied UTF-16 host results: value prefix, anonymous read, denial, invalid state.
pub const RESULT_OK: &str = "O";
pub const RESULT_ANONYMOUS: &str = "N";
pub const RESULT_DENIED: &str = "D";
pub const RESULT_INVALID: &str = "E";
pub const COOKIE: &str = "__Host-varta-session";
pub const DEVELOPMENT_COOKIE: &str = "varta-session-dev";
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_HEX_BYTES: usize = TOKEN_BYTES * 2;
pub const MAX_SECRET_BYTES: usize = 4096;
pub const MAX_ORIGIN_BYTES: usize = 256;
pub const MAX_AGE_SECONDS: u64 = 24 * 60 * 60;
pub const DEFAULT_AGE_SECONDS: u64 = 60 * 60;
pub const DEFAULT_IDLE_SECONDS: u64 = 15 * 60;
pub const MAX_SESSIONS: usize = 4096;
pub const DEFAULT_SESSIONS: usize = 1024;
pub const MAX_POLICY_BYTES: usize = 16 * 1024;
pub const MAX_REQUEST_BYTES: usize = 256 * 1024;
pub const ALLOWED_HEADERS: &[&str] = &["accept", "content-type", "idempotency-key", "if-match"];
pub const ALLOWED_METHODS: &[&str] = &["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];

/// Exact canonical origin, without credentials, path, wildcards or default port.
/// The restricted DNS grammar deliberately refuses noncanonical URL spellings.
#[must_use]
pub fn origin(value: &str, development: bool) -> bool {
    if value.len() > MAX_ORIGIN_BYTES {
        return false;
    }
    let (authority, secure) = if let Some(a) = value.strip_prefix("https://") {
        (a, true)
    } else if let Some(a) = value.strip_prefix("http://") {
        (a, false)
    } else {
        return false;
    };
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return false;
        };
        // Canonical IPv6 loopback is the only IP-literal spelling admitted here.
        if &rest[..end] != "::1" {
            return false;
        }
        let suffix = &rest[end + 1..];
        if !suffix.is_empty() && !suffix.starts_with(':') {
            return false;
        }
        (
            "[::1]",
            if suffix.is_empty() {
                None
            } else {
                suffix.strip_prefix(':')
            },
        )
    } else {
        let mut parts = authority.split(':');
        let host = parts.next().unwrap_or_default();
        let port = parts.next();
        if parts.next().is_some() {
            return false;
        }
        if host.is_empty()
            || host.len() > MAX_ORIGIN_BYTES
            || !host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
        {
            return false;
        }
        (host, port)
    };
    if let Some(port) = port {
        if port.is_empty() || port.starts_with('0') || !port.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let Ok(number) = port.parse::<u16>() else {
            return false;
        };
        if number == 0 || number == if secure { 443 } else { 80 } {
            return false;
        }
    }
    secure || development && matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// A session transport cannot let a guest supply any authority-bearing header.
#[must_use]
pub fn request_header(name: &str) -> bool {
    ALLOWED_HEADERS
        .iter()
        .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

/// The host-owned binding is checked again beside each request, before Fetch.
/// Package and host URL/method grants must independently approve the request.
#[must_use]
pub fn channel(
    api: &str,
    client: &str,
    page: &str,
    url: &str,
    method: &str,
    development: bool,
) -> bool {
    if !origin(api, development)
        || !origin(client, development)
        || page != client
        || !ALLOWED_METHODS.contains(&method)
        || url.len() > crate::session::MAX_REQUEST_BYTES
    {
        return false;
    }
    let Some(path) = url
        .strip_prefix(api)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return false;
    };
    // URL authorization has its own stricter path checks; reject authority
    // confusion and line/binary framing here before the transport sees it.
    !path.bytes().any(|b| b <= b' ' || matches!(b, b'\\' | b'#'))
}

/// Format one host-owned cookie; a guest never sees this value through Fetch.
#[must_use]
pub fn cookie(token: &str, seconds: u64, secure: bool, site: &str) -> String {
    use alloc::format;
    let name = if secure { COOKIE } else { DEVELOPMENT_COOKIE };
    let tls = if secure { "; Secure" } else { "" };
    format!("{name}={token}; Path=/; HttpOnly{tls}; SameSite={site}; Max-Age={seconds}")
}

/// Bounded bridge framing for the same policy compiled to browser Wasm.
#[must_use]
pub fn descriptor(text: &str) -> bool {
    if text.len() > MAX_POLICY_BYTES {
        return false;
    }
    let mut fields = text.split('\n');
    let Some(api) = fields.next() else {
        return false;
    };
    let Some(client) = fields.next() else {
        return false;
    };
    let Some(page) = fields.next() else {
        return false;
    };
    let Some(url) = fields.next() else {
        return false;
    };
    let Some(method) = fields.next() else {
        return false;
    };
    let development = match fields.next() {
        Some("0") => false,
        Some("1") => true,
        _ => return false,
    };
    channel(api, client, page, url, method, development) && fields.all(request_header)
}

#[cfg(all(test, feature = "std"))]
mod tests;

#[cfg(feature = "std")]
pub mod delegation;
