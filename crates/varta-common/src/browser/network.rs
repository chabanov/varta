//! Network ABI facts for the generated host; no transport or authority.
use crate::{abi, net as n};
use serde_json::{Value, json};

pub(super) fn registry() -> Value {
    use n::{CompletionKind as C, NetError as E, RequestFailure as F};
    let errors = json!({"budget":E::Budget.to_string(),"descriptor":E::Descriptor.to_string(),"state":E::State.to_string(),
            "bodyNotAllowed":F::BodyNotAllowed.to_string(),
            "method":F::Method("{value}".into()).to_string(),"url":F::Url("{value}".into()).to_string(),
            "runtimeHeader":F::RuntimeHeader("{value}".into()).to_string(),
            "headerName":F::HeaderName("{value}".into()).to_string(),"headerValue":F::HeaderValue("{value}".into()).to_string(),"headerBytes":F::HeaderBytes.to_string(),
            "bodyLimit":F::BodyLimit.to_string(),"timeout":F::Timeout.to_string(),"bodyTimeout":F::BodyTimeout.to_string()});
    json!({
        "capability":n::REQUEST_CAPABILITY,
        "maxPending":n::MAX_PENDING_REQUESTS,"maxRequestBytes":n::MAX_REQUEST_BYTES,
        "maxRequestHeaders":n::MAX_REQUEST_HEADERS,"maxRequestHeaderBytes":n::MAX_REQUEST_HEADER_BYTES,
        "maxResponseHeaders":n::MAX_RESPONSE_HEADERS,"maxResponseHeaderBytes":n::MAX_RESPONSE_HEADER_BYTES,
        "maxChunkBytes":n::MAX_CHUNK_BYTES,"maxReadBytes":n::MAX_READ_DELIVERY_BYTES,"maxBodyBytes":n::MAX_RESPONSE_BODY_BYTES,"maxUrlBytes":n::MAX_URL_BYTES,
        "maxPatterns":crate::MAX_GRANT_PATTERNS,"maxPatternBytes":crate::MAX_PATTERN_BYTES,
        "maxMatchWork":crate::MAX_PATTERN_MATCH_WORK,"maxMethods":n::MAX_GRANT_METHODS,
        "maxVisibleHeaders":n::MAX_GRANT_RESPONSE_HEADERS,"maxHeaderNameBytes":n::MAX_RESPONSE_HEADER_NAME_BYTES,
        "normalizedMethods":crate::NORMALIZED_METHODS,"forbiddenMethods":n::FORBIDDEN_METHODS,
        "runtimeHeaders":n::RUNTIME_HEADERS,"forbiddenResponseHeaders":n::FORBIDDEN_RESPONSE_HEADERS,
        "safelistedHeaders":n::SAFELISTED_RESPONSE_HEADERS,
        "version":n::DESCRIPTOR_VERSION_BYTES,"separator":n::DESCRIPTOR_SEPARATOR.to_string(),
        "noBody":n::DESCRIPTOR_NO_BODY,"textBody":n::DESCRIPTOR_BODY,"byteBody":n::DESCRIPTOR_BYTES,
        "headFields":n::HEAD_FIELDS,
        "completion":{"head":C::Head as i32,"chunk":C::Chunk as i32,"end":C::End as i32,
            "failed":C::Failed as i32,"aborted":C::Aborted as i32,"denied":C::Denied as i32},
        "result":{"base":abi::RESULT_BASE,"length":abi::RESULT_LEN,"maxBytes":abi::MAX_RESULT_BYTES,"pageBytes":abi::WASM_PAGE_BYTES},
        "errors":errors,
    })
}
