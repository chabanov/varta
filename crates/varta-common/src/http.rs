//! Versioned HTTP/1 service and transport records. These carry no ambient authority.
use crate::{abi, capability_kind};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub const SECTION: &str = "varta.http";
pub const VERSION: u32 = 1;
pub const ABI_PREFIX: &str = "varta.http_abi.";
pub const HANDLER: &str = "varta.http_abi.handle";
pub const COPY: &str = "varta.http_abi.copy_string";
pub const RETAIN: &str = "varta.http_abi.retain_string";
pub const MAX_METADATA_BYTES: usize = 8192;
pub const MAX_HEADERS: usize = 32;
pub const MAX_HEADER_BYTES: usize = 4096;
pub const MAX_BODY_BYTES: usize = 8192;
pub const MAX_TARGET_BYTES: usize = 1024;
pub const HEADER_SECONDS: u64 = 2;
pub const CONNECTION_SECONDS: u64 = 10;
pub const HTTP_BUFFER_BYTES: usize = 8192;

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
    pub request_bytes: usize,
    pub body_bytes: usize,
    pub header_bytes: usize,
    pub headers: usize,
    pub target_bytes: usize,
    pub response_bytes: usize,
    pub handler_deadline_ms: u64,
}
impl Default for HttpLimits {
    fn default() -> Self {
        Self {
            connections: 1,
            request_bytes: abi::MAX_INPUT_BYTES,
            body_bytes: MAX_BODY_BYTES,
            header_bytes: MAX_HEADER_BYTES,
            headers: MAX_HEADERS,
            target_bytes: MAX_TARGET_BYTES,
            response_bytes: abi::MAX_RESULT_BYTES,
            handler_deadline_ms: abi::DEADLINE_SECONDS * 1000,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HttpContract {
    pub version: u32,
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
        if self.version != VERSION
            || self.export != HANDLER
            || addr.to_string() != self.bind
            || self.limits != HttpLimits::default()
            || self.required_effects.len() > 16
            || !self.required_effects.iter().any(|s| s == "net.listen")
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
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Deserialize)]
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
            bind: "127.0.0.1:0".into(),
            export: HANDLER.into(),
            required_effects: vec!["net.listen".into()],
            limits: HttpLimits::default(),
        };
        HttpContract::parse(&contract.to_bytes().unwrap()).unwrap();
        contract.limits.connections += 1;
        assert!(contract.validate().is_err());
        contract.limits = HttpLimits::default();
        contract.required_effects.clear();
        assert!(contract.validate().is_err());
        assert!(HttpContract::parse(&vec![b' '; MAX_METADATA_BYTES + 1]).is_err());
        assert!(HttpContract::parse(br#"{"version":1,"version":1}"#).is_err());
    }
}
