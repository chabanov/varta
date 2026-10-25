use crate::HostFailure;
use std::time::Duration;
use varta_common::abi::{MAX_INPUT_BYTES, MAX_RESULT_BYTES};

pub(crate) fn fetch(
    url: &url::Url,
    remaining: Duration,
    body: bool,
) -> Result<(i32, Vec<u8>), HostFailure> {
    // No ambient proxy credentials or redirects: each new target needs authorization.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .max_response_header_size(MAX_INPUT_BYTES)
        .timeout_global(Some(remaining))
        .build()
        .into();
    let mut response = agent.get(url.as_str()).call().map_err(HostFailure::io)?;
    let status = i32::from(response.status().as_u16());
    let bytes = if body {
        read_body(response.body_mut())?
    } else {
        Vec::new()
    };
    Ok((status, bytes))
}

fn read_body(body: &mut ureq::Body) -> Result<Vec<u8>, HostFailure> {
    body.with_config()
        .limit(MAX_RESULT_BYTES as u64)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => HostFailure::too_large(),
            other => HostFailure::io(other),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_body_limit_is_enforced_without_a_network_service() {
        let mut body = ureq::Body::builder().data(vec![b'A'; MAX_RESULT_BYTES + 1]);
        let error = read_body(&mut body).unwrap_err();
        assert!(matches!(
            error.status,
            varta_common::abi::HostStatus::TooLarge
        ));
        let mut body = ureq::Body::builder().data(b"small response".to_vec());
        assert_eq!(read_body(&mut body).unwrap(), b"small response");
    }
}
