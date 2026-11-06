//! Accepting and serving connections: cleartext HTTP/1.1, or TLS 1.3 with
//! ALPN `h2` (HTTP/2) or `http/1.1` (K7 H3), each within its idle, write,
//! lifetime and shutdown bounds.
use super::*;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};
use varta_common::http;

/// The protocol a connection speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Protocol {
    Http1,
    Http2,
}

/// The state the requests of one connection share.
pub(super) struct State {
    pub protocol: Protocol,
    /// Requests the connection received.
    pub served: AtomicUsize,
    /// Requests whose client left before their response (an HTTP/2 stream
    /// reset, a closed connection).
    abandoned: AtomicUsize,
    /// Requests whose service future is running or whose response body is
    /// still sending.
    active: AtomicUsize,
    /// Counts every start and end of a request or response body: the idle
    /// bound measures quiet since the last of them.
    events: AtomicUsize,
    /// Whether any request was received (a shutdown drains it).
    entered: AtomicBool,
    /// HTTP/2: ask the connection to finish its streams and close (`GOAWAY`).
    retire: tokio::sync::Notify,
}
impl State {
    fn new(protocol: Protocol) -> Arc<Self> {
        Arc::new(Self {
            protocol,
            served: AtomicUsize::new(0),
            abandoned: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            events: AtomicUsize::new(0),
            entered: AtomicBool::new(false),
            retire: tokio::sync::Notify::new(),
        })
    }
    /// A request begins; the guard ends it.
    pub(super) fn begin(self: &Arc<Self>) -> Active {
        self.entered.store(true, Ordering::Release);
        self.active.fetch_add(1, Ordering::AcqRel);
        self.events.fetch_add(1, Ordering::AcqRel);
        Active {
            state: self.clone(),
            completed: false,
        }
    }
    /// A response body that is still sending: the request stays running
    /// until the guard drops.
    pub(super) fn streaming(self: &Arc<Self>) -> Streaming {
        self.active.fetch_add(1, Ordering::AcqRel);
        self.events.fetch_add(1, Ordering::AcqRel);
        Streaming {
            state: self.clone(),
        }
    }
    /// Ask an HTTP/2 connection to finish its streams and close.
    pub(super) fn retire(&self) {
        self.retire.notify_one();
    }
}

/// A running request of a connection. Dropped before [`Active::complete`],
/// its client left: too many such requests end the connection (a rapid
/// reset flood that reached the application).
pub(super) struct Active {
    state: Arc<State>,
    completed: bool,
}
impl Active {
    pub(super) fn complete(mut self) {
        self.completed = true;
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        self.state.active.fetch_sub(1, Ordering::AcqRel);
        self.state.events.fetch_add(1, Ordering::AcqRel);
        if !self.completed
            && self.state.abandoned.fetch_add(1, Ordering::AcqRel) + 1
                > http::MAX_ABANDONED_REQUESTS
        {
            self.state.retire();
        }
    }
}

/// A response body still sending on its connection (see
/// [`State::streaming`]); a client that leaves mid-body is not counted as
/// abandoned, its request having completed.
pub(super) struct Streaming {
    state: Arc<State>,
}
impl Drop for Streaming {
    fn drop(&mut self) {
        self.state.active.fetch_sub(1, Ordering::AcqRel);
        self.state.events.fetch_add(1, Ordering::AcqRel);
    }
}

/// How long a connection that is closed early (a reset flood) has to read
/// its `GOAWAY` before the socket closes.
const RETIRE_DRAIN: Duration = Duration::from_secs(1);

pub(super) async fn serve(
    listener: TcpListener,
    address: SocketAddr,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    let listener = tokio::net::TcpListener::from_std(listener).map_err(HttpError::Io)?;
    if let Some(observer) = &shared.sink.observer {
        observer(RuntimeEvent::Listening {
            address,
            shutdown: shared.stop.clone(),
            scheme: shared.scheme,
        });
    }
    if shared.contract.version == http::UTF16_VERSION {
        pooled(listener, shared).await
    } else {
        sequential(listener, shared).await
    }
}

/// Contract v1: one connection at a time; a busy worker refuses with 503.
async fn sequential(
    listener: tokio::net::TcpListener,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    while !shared.stop.stopped() {
        let accepted = tokio::select! { _ = shared.stop.cancelled() => break, value = listener.accept() => value };
        let (stream, _) = accepted.map_err(HttpError::Io)?;
        http1_connection(stream, shared.clone()).await;
    }
    Ok(())
}

/// Contract v2: up to `limits.connections` open keep-alive connections, which
/// cost no worker while idle; each request waits a bounded time for its
/// admission. A further connection waits in the listener's backlog, never
/// in an unbounded queue. Shutdown stops accepting and waits for the open
/// connections, which drain or end their responses.
async fn pooled(
    listener: tokio::net::TcpListener,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    let slots = Arc::new(tokio::sync::Semaphore::new(shared.limits.connections));
    let mut open = tokio::task::JoinSet::new();
    let result = loop {
        while open.try_join_next().is_some() {}
        let slot = tokio::select! {
            _ = shared.stop.cancelled() => break Ok(()),
            slot = slots.clone().acquire_owned() => slot,
        };
        // The semaphore is never closed.
        let Ok(slot) = slot else { break Ok(()) };
        let accepted = tokio::select! { _ = shared.stop.cancelled() => break Ok(()), value = listener.accept() => value };
        let (stream, _) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => break Err(HttpError::Io(error).into()),
        };
        let service = shared.clone();
        open.spawn(async move {
            let _slot = slot;
            accept(stream, service).await;
        });
    };
    while open.join_next().await.is_some() {}
    result
}

/// A v2 connection: cleartext HTTP/1.1, or a TLS handshake within
/// `http::TLS_HANDSHAKE_SECONDS` and the protocol its ALPN chose. Writes
/// that make no progress for the write idle bound end it.
async fn accept(stream: tokio::net::TcpStream, shared: Arc<Shared>) {
    let write_idle = shared
        .limits
        .transport
        .map(|transport| Duration::from_millis(transport.write_idle_ms));
    let stream = io::WriteIdle::new(stream, write_idle);
    let Some(acceptor) = shared.tls.clone() else {
        return http1_connection(stream, shared).await;
    };
    let handshake = tokio::time::timeout(
        Duration::from_secs(http::TLS_HANDSHAKE_SECONDS),
        acceptor.accept(stream),
    );
    let tls = tokio::select! {
        _ = shared.stop.cancelled() => return,
        tls = handshake => tls,
    };
    match tls {
        Ok(Ok(tls)) => {
            if tls.get_ref().1.alpn_protocol() == Some(tls::H2) {
                http2_connection(tls, shared).await;
            } else {
                http1_connection(tls, shared).await;
            }
        }
        // A client that speaks no TLS 1.3, or too slowly, is no request.
        Ok(Err(error)) => shared
            .sink
            .diagnostic(format!("HTTP TLS handshake failed: {error}")),
        Err(_) => shared.sink.diagnostic("HTTP TLS handshake deadline".into()),
    }
}

/// Serve one HTTP/1.1 connection. Contract v1: one request, then close,
/// within the connection bound. Contract v2: keep-alive requests (the idle
/// time is also the head deadline) within the connection lifetime, after
/// which the connection finishes its current response and closes.
async fn http1_connection<I>(io: I, shared: Arc<Shared>)
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let pool = shared.pool();
    let state = State::new(Protocol::Http1);
    let mut builder = http1::Builder::new();
    builder
        .keep_alive(pool.is_some())
        .max_headers(shared.limits.headers)
        .max_buf_size(http::request_buffer_bytes(shared.limits.header_bytes))
        .timer(TokioTimer::new())
        .header_read_timeout(
            pool.map_or(Duration::from_secs(http::HEADER_SECONDS), |pool| {
                Duration::from_millis(pool.keep_alive_idle_ms)
            }),
        );
    let service = shared.clone();
    let requests = state.clone();
    let connection = builder.serve_connection(
        TokioIo::new(io),
        service_fn(move |req| transport::handle(req, service.clone(), requests.clone())),
    );
    tokio::pin!(connection);
    let result = drive(connection, &shared, &state, |connection| {
        connection.graceful_shutdown()
    })
    .await;
    report(&shared, &state, result);
}

/// The HTTP/2 settings: at most `http::MAX_CONCURRENT_STREAMS` streams at
/// once, a header list within `http::MAX_HEADER_LIST_BYTES` (the request
/// header bounds still apply), bounded flow-control windows and send
/// buffer, keep-alive pings and the rapid-reset bounds of `h2`.
fn http2_builder() -> http2::Builder<Spawner> {
    let mut builder = http2::Builder::new(Spawner);
    builder
        .timer(TokioTimer::new())
        .max_concurrent_streams(http::MAX_CONCURRENT_STREAMS)
        .max_header_list_size(http::MAX_HEADER_LIST_BYTES)
        .initial_stream_window_size(http::STREAM_WINDOW_BYTES)
        .initial_connection_window_size(http::CONNECTION_WINDOW_BYTES)
        .max_send_buf_size(http::STREAM_CHUNK_BYTES)
        .max_pending_accept_reset_streams(http::MAX_PENDING_RESET_STREAMS)
        .max_local_error_reset_streams(http::MAX_PENDING_RESET_STREAMS)
        .keep_alive_interval(Duration::from_secs(http::PING_INTERVAL_SECONDS))
        .keep_alive_timeout(Duration::from_secs(http::PING_TIMEOUT_SECONDS));
    builder
}

/// Serve one HTTP/2 connection (contract v2 over TLS): its streams run as
/// requests of their own; the keep-alive request count, the idle bound, too
/// many abandoned requests and the lifetime end it with `GOAWAY`.
async fn http2_connection<I>(io: I, shared: Arc<Shared>)
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let state = State::new(Protocol::Http2);
    let service = shared.clone();
    let requests = state.clone();
    let connection = http2_builder().serve_connection(
        TokioIo::new(io),
        service_fn(move |req| transport::handle(req, service.clone(), requests.clone())),
    );
    tokio::pin!(connection);
    let result = drive(connection, &shared, &state, |connection| {
        connection.graceful_shutdown()
    })
    .await;
    report(&shared, &state, result);
}

/// Spawns hyper's HTTP/2 stream tasks on the server runtime.
#[derive(Clone, Copy)]
struct Spawner;
impl<F> hyper::rt::Executor<F> for Spawner
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, future: F) {
        tokio::spawn(future);
    }
}

/// Run `connection` to its end, its lifetime (plus the stream lifetime for
/// a streaming service, then the drain of its current response), a service
/// shutdown (which drains a received request and closes an idle or
/// partial-header connection at once) or, for HTTP/2, its retirement or idle
/// bound. `graceful` stops it reading new requests or streams. `None`: the
/// connection deadline cut it.
async fn drive<C>(
    mut connection: std::pin::Pin<&mut C>,
    shared: &Shared,
    state: &State,
    graceful: impl Fn(std::pin::Pin<&mut C>),
) -> Option<Result<(), hyper::Error>>
where
    C: std::future::Future<Output = Result<(), hyper::Error>>,
{
    let pool = shared.pool();
    let stream_lifetime = shared
        .limits
        .stream
        .filter(|_| shared.contract.streaming)
        .map_or(Duration::ZERO, |limits| {
            Duration::from_millis(limits.lifetime_ms)
        });
    let bound = pool.map_or(Duration::from_secs(http::CONNECTION_SECONDS), |pool| {
        Duration::from_millis(pool.connection_lifetime_ms)
    }) + stream_lifetime;
    let until = tokio::time::Instant::now() + bound;
    // The current response of a connection whose lifetime ended may finish
    // within its own bound: a Web `Request` service's request lifetime.
    let drain = match shared.limits.transport {
        Some(transport) if shared.contract.web_request => {
            Duration::from_millis(transport.request_lifetime_ms) + stream_lifetime
        }
        _ => Duration::from_secs(http::CONNECTION_SECONDS),
    };
    let idle = pool.map(|pool| Duration::from_millis(pool.keep_alive_idle_ms));
    let idle = async {
        match (state.protocol, idle) {
            (Protocol::Http2, Some(idle)) => idled(state, idle).await,
            _ => std::future::pending().await,
        }
    };
    tokio::select! {
        result = connection.as_mut() => Some(result),
        _ = tokio::time::sleep_until(until) => {
            if pool.is_some() {
                // The lifetime ends: finish the current response, then close.
                graceful(connection.as_mut());
                tokio::time::timeout(drain, connection.as_mut()).await.ok()
            } else {
                None
            }
        }
        _ = shared.stop.cancelled() => {
            if state.entered.load(Ordering::Acquire) {
                // Drain the current response within the connection bound.
                graceful(connection.as_mut());
                tokio::time::timeout_at(until, connection.as_mut()).await.ok()
            } else {
                // No request exists to drain. Closing a partial header or
                // an idle accepted socket must not wait for its timeout.
                Some(Ok(()))
            }
        }
        () = state.retire.notified() => {
            graceful(connection.as_mut());
            let drain = if state.abandoned.load(Ordering::Acquire) > http::MAX_ABANDONED_REQUESTS {
                RETIRE_DRAIN
            } else {
                drain
            };
            tokio::time::timeout(drain, connection.as_mut()).await.ok()
        }
        () = idle => {
            graceful(connection.as_mut());
            tokio::time::timeout(drain, connection.as_mut()).await.ok()
        }
    }
}

/// Resolves once the connection had no running request for `idle`: a whole
/// `idle` passed with nothing running and no request or body starting or
/// ending, so the quiet is measured from the last of them (a body that ends
/// just before a check does not make the connection idle at once).
async fn idled(state: &State, idle: Duration) {
    loop {
        let events = state.events.load(Ordering::Acquire);
        tokio::time::sleep(idle).await;
        if state.events.load(Ordering::Acquire) == events
            && state.active.load(Ordering::Acquire) == 0
        {
            return;
        }
    }
}

fn report(shared: &Shared, state: &State, result: Option<Result<(), hyper::Error>>) {
    match result {
        Some(Ok(())) => {}
        // A kept-alive connection that idled past its bound closes: not a
        // rejection.
        Some(Err(error)) if error.is_timeout() && state.served.load(Ordering::Acquire) > 0 => {}
        Some(Err(error)) => shared
            .sink
            .diagnostic(format!("HTTP connection rejected: {error}")),
        None => shared.sink.diagnostic("HTTP connection deadline".into()),
    }
}
