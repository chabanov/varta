//! Host-owned ingress and response policy; no application identity or routes.
use super::*;
use hyper::{Request, StatusCode, body::Incoming};
use varta_common::{http::HttpHeader, session as contract};

pub(super) fn ingress(shared: &Shared, request: &Request<Incoming>) -> Result<bool, StatusCode> {
    let Some(vault) = &shared.sessions else {
        return Ok(false);
    };
    let headers = request.headers();
    let single = |name: &str| -> Result<Option<&str>, StatusCode> {
        let mut values = headers.get_all(name).iter();
        let value = values
            .next()
            .map(|v| v.to_str().map_err(|_| StatusCode::BAD_REQUEST))
            .transpose()?;
        if values.next().is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(value)
    };
    let host = single("host")?;
    let authority = request.uri().authority().map(|a| a.as_str());
    let expected = vault.policy.origin.split_once("://").map(|(_, a)| a);
    if host.or(authority) != expected
        || host.is_some_and(|value| Some(value) != expected)
        || authority.is_some_and(|value| Some(value) != expected)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let origin = single("origin")?;
    if origin.is_some_and(|o| o != vault.policy.client_origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    let marker = single(contract::HEADER)?;
    if marker.is_some_and(|v| v != contract::HEADER_VALUE) {
        return Err(StatusCode::FORBIDDEN);
    }
    if marker.is_some() && single("authorization")?.is_some() {
        return Err(StatusCode::FORBIDDEN);
    }
    if request.method() != hyper::Method::OPTIONS {
        let mut copy = headers
            .iter()
            .map(|(name, value)| {
                Ok(HttpHeader {
                    name: name.to_string(),
                    value: value.to_str().map_err(|_| StatusCode::BAD_REQUEST)?.into(),
                })
            })
            .collect::<Result<Vec<_>, StatusCode>>()?;
        if marker.is_some() && !contract::ALLOWED_METHODS.contains(&request.method().as_str()) {
            return Err(StatusCode::FORBIDDEN);
        }
        crate::session::RequestSession::new(
            vault.clone(),
            &mut copy,
            request.method().as_str(),
            None,
        )
        .map_err(|_| StatusCode::FORBIDDEN)?;
        return Ok(false);
    }
    if origin != Some(vault.policy.client_origin.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let method = single("access-control-request-method")?.ok_or(StatusCode::FORBIDDEN)?;
    if !contract::ALLOWED_METHODS.contains(&method) {
        return Err(StatusCode::FORBIDDEN);
    }
    let requested = single("access-control-request-headers")?.unwrap_or_default();
    let mut session = false;
    for name in requested
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        if name.eq_ignore_ascii_case(contract::HEADER) {
            session = true;
        } else if !contract::request_header(name) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if !session {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(true)
}
pub(super) fn cors(shared: &Shared, response: &mut hyper::Response<super::transport::ReplyBody>) {
    let Some(vault) = &shared.sessions else {
        return;
    };
    let fields = [
        (
            "access-control-allow-origin",
            vault.policy.client_origin.as_str(),
        ),
        ("access-control-allow-credentials", "true"),
        ("vary", "Origin"),
        ("cache-control", "no-store"),
    ];
    for (name, value) in fields {
        if name == "vary" {
            if !response.headers().get_all(name).iter().any(|v| {
                v.to_str().is_ok_and(|s| {
                    s.split(',')
                        .any(|s| s.trim().eq_ignore_ascii_case("origin"))
                })
            }) {
                response
                    .headers_mut()
                    .append("vary", hyper::header::HeaderValue::from_static("Origin"));
            }
        } else if let Ok(value) = hyper::header::HeaderValue::from_str(value) {
            response.headers_mut().insert(name, value);
        }
    }
}
pub(super) fn preflight_headers() -> Vec<HttpHeader> {
    vec![
        HttpHeader {
            name: "access-control-allow-methods".into(),
            value: contract::ALLOWED_METHODS.join(", "),
        },
        HttpHeader {
            name: "access-control-allow-headers".into(),
            value: contract::ALLOWED_HEADERS
                .iter()
                .copied()
                .chain([contract::HEADER])
                .collect::<Vec<_>>()
                .join(", "),
        },
        HttpHeader {
            name: "vary".into(),
            value: "Origin, Access-Control-Request-Method, Access-Control-Request-Headers".into(),
        },
    ]
}
