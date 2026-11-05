//! Recorded network replay for application instances (v38): the native
//! recorder and replayer of `varta_common::net::replay` logs.
//!
//! Recording keeps every real transfer and logs, in delivery order, each
//! completion the guest receives from a transport and each abort of an open
//! transfer, with the request's place in start order and the read mode that
//! armed a chunk. Replay replaces the transport: a request is planned and
//! authorized exactly as a real one (its grant check, broker decision and
//! audit entry included), then claims the next logged request, which must
//! have its method and URL. Logged events are released strictly in log
//! order, each once its request has the matching operation armed and, in a
//! scripted session, once the session reached the event's step. A released
//! event arrives as a transfer's completion does: when the instance next
//! waits for network, after its idle period (and that period's collection);
//! every event released by then arrives at once, in log order. So the
//! guest's imports, turns and usage are those of a real transfer. Nothing
//! waits for the wall clock: an armed operation the next logged event
//! cannot reach stalls the replay, which is fatal where the session waits
//! for network.
use crate::policy::Policy;
use std::collections::VecDeque;
use std::sync::Arc;
use varta_common::net::replay::{
    LoggedEvent, LoggedRequest, MAX_LOG_BYTES, MAX_LOG_EVENTS, MAX_LOG_REQUESTS, NetworkLog,
    ReplayError, UNSCRIPTED_STEP,
};

/// How an application instance's network requests reach a transport.
#[derive(Debug, Clone, Default)]
pub enum NetworkMode {
    /// Real transfers.
    #[default]
    Live,
    /// Real transfers, every completion the guest receives from a transport
    /// logged into [`crate::RunReport::network_log`].
    Record,
    /// The log's completions instead of a transport; the log's requests
    /// must be granted ([`check_grants`]).
    Replay(Arc<NetworkLog>),
}

/// A bounded log under construction.
pub(crate) struct Recorder {
    log: NetworkLog,
    /// Bytes of the log's JSON text so far, separators included.
    bytes: usize,
}
impl Recorder {
    pub(crate) fn new() -> Self {
        let log = NetworkLog::default();
        Self {
            bytes: log.to_json().len(),
            log,
        }
    }
    /// Account for one more element of `size` JSON bytes and its separator.
    fn grow(&mut self, size: Result<Vec<u8>, serde_json::Error>) -> Result<(), ReplayError> {
        const SEPARATOR_BYTES: usize = 1;
        let size = size.map_err(|_| ReplayError::RecordingLimit)?.len();
        self.bytes = self.bytes.saturating_add(size + SEPARATOR_BYTES);
        if self.bytes > MAX_LOG_BYTES {
            return Err(ReplayError::RecordingLimit);
        }
        Ok(())
    }
    /// Log a request that reached the transport; its place in the log.
    pub(crate) fn request(&mut self, method: &str, url: &str) -> Result<u32, ReplayError> {
        let request = LoggedRequest {
            method: method.to_owned(),
            url: url.to_owned(),
        };
        let place =
            u32::try_from(self.log.requests.len()).map_err(|_| ReplayError::RecordingLimit)?;
        if self.log.requests.len() >= MAX_LOG_REQUESTS {
            return Err(ReplayError::RecordingLimit);
        }
        self.grow(serde_json::to_vec(&request))?;
        self.log.requests.push(request);
        Ok(place)
    }
    /// Log one event, checked as a log reader checks it.
    pub(crate) fn event(&mut self, event: LoggedEvent) -> Result<(), ReplayError> {
        if event.check().is_err() {
            return Err(ReplayError::Unrecordable {
                request: event.request(),
            });
        }
        if self.log.events.len() >= MAX_LOG_EVENTS {
            return Err(ReplayError::RecordingLimit);
        }
        self.grow(serde_json::to_vec(&event))?;
        self.log.events.push(event);
        Ok(())
    }
    pub(crate) fn finish(self) -> NetworkLog {
        self.log
    }
}

/// The replay position in a log.
pub(crate) struct Replayer {
    log: Arc<NetworkLog>,
    /// The next logged request a starting request claims.
    claimed: usize,
    /// The next logged event to release.
    cursor: usize,
    /// Released completions that arrive when the instance next waits.
    released: VecDeque<crate::net::Event>,
    /// Arrived completions not yet received by the event loop.
    arrived: VecDeque<crate::net::Event>,
}
impl Replayer {
    pub(crate) fn new(log: Arc<NetworkLog>) -> Self {
        Self {
            log,
            claimed: 0,
            cursor: 0,
            released: VecDeque::new(),
            arrived: VecDeque::new(),
        }
    }
    /// Release the completion of the next logged event.
    pub(crate) fn release(&mut self, event: crate::net::Event) {
        self.released.push_back(event);
        self.cursor += 1;
    }
    /// An arrived completion, without waiting.
    pub(crate) fn poll(&mut self) -> Option<crate::net::Event> {
        self.arrived.pop_front()
    }
    /// The instance waits: everything released arrives now.
    pub(crate) fn wait(&mut self) -> Option<crate::net::Event> {
        self.arrived.append(&mut self.released);
        self.arrived.pop_front()
    }
    /// Whether waiting would deliver a completion.
    pub(crate) fn waiting(&self) -> bool {
        !self.released.is_empty() || !self.arrived.is_empty()
    }
    /// A request reached the transport: claim the next logged request,
    /// which must have the same method and URL.
    pub(crate) fn claim(&mut self, method: &str, url: &str) -> Result<u32, ReplayError> {
        let Some(logged) = self.log.requests.get(self.claimed) else {
            return Err(ReplayError::Unlogged {
                method: method.to_owned(),
                url: url.to_owned(),
            });
        };
        let place = u32::try_from(self.claimed).map_err(|_| ReplayError::Unlogged {
            method: method.to_owned(),
            url: url.to_owned(),
        })?;
        if logged.method != method || logged.url != url {
            return Err(ReplayError::Request {
                request: place,
                method: method.to_owned(),
                url: url.to_owned(),
                logged_method: logged.method.clone(),
                logged_url: logged.url.clone(),
            });
        }
        self.claimed += 1;
        Ok(place)
    }
    /// The next logged event, if a session at `step` (`None`: no scripted
    /// session) has reached it.
    pub(crate) fn next(&self, step: Option<u32>) -> Option<&LoggedEvent> {
        self.log
            .events
            .get(self.cursor)
            .filter(|event| step.is_none_or(|step| event.step() <= step))
    }
    /// The guest aborted the open transfer of logged request `request`: the
    /// next logged event must be that abort.
    pub(crate) fn abort(&mut self, request: u32, step: Option<u32>) -> Result<(), ReplayError> {
        match self.next(step) {
            Some(LoggedEvent::Abort {
                request: logged, ..
            }) if *logged == request => {
                self.cursor += 1;
                Ok(())
            }
            _ => Err(ReplayError::Abort { request }),
        }
    }
    /// The index of the next logged event to release.
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }
    /// Logged events not released yet.
    pub(crate) fn remaining(&self) -> usize {
        self.log.events.len().saturating_sub(self.cursor)
    }
}

/// The step a recorded event carries.
pub(crate) fn recorded_step(step: Option<u32>) -> u32 {
    step.unwrap_or(UNSCRIPTED_STEP)
}

/// Before an instance starts: every logged request must be admitted by the
/// package's `net.request` grants and the host ceiling, so a log never
/// names a request the grants would refuse. Each replayed request is still
/// checked again when it starts.
pub(crate) fn check_grants(log: &NetworkLog, policy: &Policy) -> Result<(), ReplayError> {
    for (place, request) in (0..).zip(&log.requests) {
        if policy.request(&request.url, &request.method).is_none() {
            return Err(ReplayError::Ungranted {
                request: place,
                method: request.method.clone(),
                url: request.url.clone(),
            });
        }
    }
    Ok(())
}

/// Replay failures raised inside guest calls keep their typed error.
pub(crate) fn typed(error: crate::RuntimeError) -> crate::RuntimeError {
    let crate::RuntimeError::Execution(source) = error else {
        return error;
    };
    match source.downcast::<ReplayError>() {
        Ok(error) => crate::RuntimeError::Replay(error),
        Err(source) => crate::RuntimeError::Execution(source),
    }
}
