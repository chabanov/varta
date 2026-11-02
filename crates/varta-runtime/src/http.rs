use crate::HostFailure;
use std::time::Duration;
use varta_common::abi::MAX_RESULT_BYTES;
use varta_common::net::MAX_FETCH_HEADER_BYTES;

/// Cancellation is an execution failure, never a recoverable host status.
#[derive(Debug)]
pub(crate) enum Failure {
    Host(HostFailure),
    Cancelled,
}
impl From<HostFailure> for Failure {
    fn from(error: HostFailure) -> Self {
        Self::Host(error)
    }
}

pub(crate) fn fetch(
    url: &url::Url,
    deadline: std::time::Instant,
    body: bool,
    cancellation: Option<&crate::cancellation::Cancellation>,
) -> Result<(i32, Vec<u8>), Failure> {
    if let Some(cancellation) = cancellation {
        return crate::net::fetch_cancellable(url, deadline, body, cancellation);
    }
    legacy_fetch(
        url,
        deadline.saturating_duration_since(std::time::Instant::now()),
        body,
    )
    .map_err(Failure::Host)
}

fn legacy_fetch(
    url: &url::Url,
    remaining: Duration,
    body: bool,
) -> Result<(i32, Vec<u8>), HostFailure> {
    // No ambient proxy credentials or redirects: each new target needs authorization.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .max_response_header_size(MAX_FETCH_HEADER_BYTES)
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
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };
    fn answer(wire: Vec<u8>) -> (url::Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
            }
            // A bounded client may close while an oversized answer is written.
            let _ = stream.write_all(&wire);
        });
        (url, task)
    }
    #[test]
    fn cancellable_synchronous_calls_preserve_raw_bodies_statuses_and_no_redirects() {
        let engine = wasmtime::Engine::default();
        for owned in [false, true] {
            let token = owned.then(|| crate::cancellation::Cancellation::new(engine.clone()));
            for (wire,expected) in [
                (b"HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\nConnection: close\r\n\r\na\xffb".to_vec(),(404,b"a\xffb".to_vec())),
                (b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/never\r\nContent-Length: 4\r\nConnection: close\r\n\r\nstay".to_vec(),(302,b"stay".to_vec())),
                (b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\nab\r\n2\r\ncd\r\n0\r\nx-trailer: ignored\r\n\r\n".to_vec(),(200,b"abcd".to_vec())),
            ] {
                let (url,task)=answer(wire);
                let result=fetch(&url,Instant::now()+Duration::from_secs(2),true,token.as_ref()).unwrap();
                assert_eq!(result,expected);
                task.join().unwrap();
            }
        }
    }
    #[test]
    fn cancellable_synchronous_calls_bound_the_body_and_reject_malformed_framing() {
        let token = crate::cancellation::Cancellation::new(wasmtime::Engine::default());
        let mut large = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_RESULT_BYTES + 1
        )
        .into_bytes();
        large.resize(large.len() + MAX_RESULT_BYTES + 1, b'x');
        let (url, task) = answer(large);
        let error = fetch(
            &url,
            Instant::now() + Duration::from_secs(2),
            true,
            Some(&token),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                Failure::Host(HostFailure {
                    status: varta_common::abi::HostStatus::TooLarge,
                    ..
                })
            ),
            "{error:?}"
        );
        task.join().unwrap();
        let (url, task) =
            answer(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nx".to_vec());
        let error = fetch(
            &url,
            Instant::now() + Duration::from_secs(2),
            true,
            Some(&token),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                Failure::Host(HostFailure {
                    status: varta_common::abi::HostStatus::Io,
                    ..
                })
            ),
            "{error:?}"
        );
        task.join().unwrap();
    }
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
