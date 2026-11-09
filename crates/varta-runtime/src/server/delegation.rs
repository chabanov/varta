//! Opt-in native control plane. Authentication and provenance never come from
//! guest claims. It bypasses worker admission so a single worker can verify its
//! own request through the protected server-side network API.
use super::*;
use http_body_util::{BodyExt, Either, Full};
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
};
use serde::Deserialize;
use varta_common::session::delegation as wire;

#[derive(Clone)]
pub(super) struct Route(pub String);
pub(super) enum Dispatch {
    Reply(Response<transport::ReplyBody>),
    Relay(Request<Incoming>),
}
fn reply(status: StatusCode, value: serde_json::Value) -> Response<transport::ReplyBody> {
    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    if bytes.len() > 65536 {
        return reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error":"native delegation response bound"}),
        );
    }
    let mut r = Response::new(Either::Left(Full::new(Bytes::from(bytes))));
    *r.status_mut() = status;
    for (k, v) in [
        ("content-type", "application/json"),
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
    ] {
        r.headers_mut()
            .insert(k, hyper::header::HeaderValue::from_static(v));
    }
    r
}
fn failed(status: StatusCode) -> Dispatch {
    Dispatch::Reply(reply(
        status,
        serde_json::json!({"error":"native delegation request refused"}),
    ))
}
fn single<'a>(req: &'a Request<Incoming>, name: &str) -> Result<Option<&'a str>, StatusCode> {
    let mut it = req.headers().get_all(name).iter();
    let value = it
        .next()
        .map(|v| v.to_str().map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?;
    if it.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(value)
}
fn authenticate(req: &Request<Incoming>, vault: &crate::session::Vault) -> Result<(), StatusCode> {
    let expected = vault.policy.origin.split_once("://").map(|(_, s)| s);
    let host = single(req, "host")?;
    let authority = req.uri().authority().map(|a| a.as_str());
    if host.or(authority) != expected
        || host.is_some_and(|h| Some(h) != expected)
        || authority.is_some_and(|a| Some(a) != expected)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    for name in [
        "origin",
        "cookie",
        varta_common::session::HEADER,
        wire::REQUEST_PROOF_HEADER,
    ] {
        if single(req, name)?.is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if !single(req, "authorization")?.is_some_and(|a| vault.delegation_auth(a)) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Lookup {
    request_proof: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Revoke {
    request_proof: String,
    delegation_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Scope {
    delegation_id: String,
}
async fn body(req: Request<Incoming>) -> Result<Vec<u8>, StatusCode> {
    if single(&req, "content-type")? != Some("application/json") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut incoming = req.into_body();
    let mut out = Vec::new();
    while let Some(f) = incoming.frame().await {
        let f = f.map_err(|_| StatusCode::BAD_REQUEST)?;
        if let Some(b) = f.data_ref() {
            if b.len() > wire::MAX_CONTROL_BYTES - out.len() {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            out.extend_from_slice(b);
        } else if f.trailers_ref().is_some_and(|t| !t.is_empty()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(out)
}
pub(super) async fn dispatch(mut req: Request<Incoming>, shared: &Shared) -> Dispatch {
    let Some(vault) = shared.sessions.as_ref().filter(|v| v.delegation_enabled()) else {
        return failed(StatusCode::NOT_FOUND);
    };
    if let Err(s) = authenticate(&req, vault) {
        return failed(s);
    }
    let path = req.uri().path().to_owned();
    let relay = format!("{}/relay/", wire::CONTROL_PREFIX);
    if let Some(tail) = path.strip_prefix(&relay) {
        let Some((id, path)) = tail.split_once('/') else {
            return failed(StatusCode::BAD_REQUEST);
        };
        if id.len() != 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return failed(StatusCode::BAD_REQUEST);
        }
        let target = format!(
            "/{path}{}",
            req.uri()
                .query()
                .map(|q| format!("?{q}"))
                .unwrap_or_default()
        );
        if !vault.delegation_route(id, &target, req.method().as_str()) {
            return failed(StatusCode::FORBIDDEN);
        }
        let id = id.to_owned();
        let Ok(uri) = target.parse() else {
            return failed(StatusCode::BAD_REQUEST);
        };
        *req.uri_mut() = uri;
        req.headers_mut().remove("authorization");
        req.extensions_mut().insert(Route(id));
        return Dispatch::Relay(req);
    }
    if req.uri().query().is_some() {
        return failed(StatusCode::BAD_REQUEST);
    }
    let route = path.strip_prefix(wire::CONTROL_PREFIX).unwrap_or_default();
    if route == "/info" && req.method() == hyper::Method::GET {
        return Dispatch::Reply(reply(
            StatusCode::OK,
            serde_json::json!({"schemaVersion":1,"requestProofHeader":wire::REQUEST_PROOF_HEADER,"policy":vault.delegation_policy()}),
        ));
    }
    if req.method() != hyper::Method::POST
        || !matches!(route, "/lookup" | "/issue" | "/revoke" | "/scope")
    {
        return failed(StatusCode::NOT_FOUND);
    }
    let bytes = match tokio::time::timeout(
        std::time::Duration::from_secs(varta_common::http::HEADER_SECONDS),
        body(req),
    )
    .await
    {
        Ok(Ok(v)) => v,
        Ok(Err(s)) => return failed(s),
        Err(_) => return failed(StatusCode::REQUEST_TIMEOUT),
    };
    let result = match route {
        "/scope" => serde_json::from_slice::<Scope>(&bytes)
            .map_err(|_| ())
            .and_then(|r| vault.scope_descriptor(&r.delegation_id).map_err(|_| ())),
        "/lookup" => serde_json::from_slice::<Lookup>(&bytes)
            .map_err(|_| ())
            .and_then(|r| vault.lookup_proof(&r.request_proof).map_err(|_| ()))
            .and_then(|r| serde_json::to_value(r).map_err(|_| ())),
        "/issue" => serde_json::from_slice::<crate::session::Issue>(&bytes)
            .map_err(|_| ())
            .and_then(|r| vault.issue_delegation(r).map_err(|_| ()))
            .and_then(|r| serde_json::to_value(r).map_err(|_| ())),
        "/revoke" => serde_json::from_slice::<Revoke>(&bytes)
            .map_err(|_| ())
            .and_then(|r| {
                vault
                    .revoke_delegation(&r.request_proof, &r.delegation_id)
                    .map_err(|_| ())
            })
            .and_then(|r| serde_json::to_value(r).map_err(|_| ())),
        _ => Err(()),
    };
    match result {
        Ok(value) => Dispatch::Reply(reply(StatusCode::OK, value)),
        Err(()) => failed(StatusCode::FORBIDDEN),
    }
}
