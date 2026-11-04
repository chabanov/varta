//! Opt-in host diagnostics; never guest fuel or a screen-presentation clock.
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InputStamp {
    queued: Instant,
    kind: &'static str,
}
impl InputStamp {
    pub fn now(kind: &'static str) -> Option<Self> {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        ENABLED
            .get_or_init(|| std::env::var_os("VARTA_PREVIEW_TIMING").is_some_and(|v| v == "1"))
            .then(|| Self {
                queued: Instant::now(),
                kind,
            })
    }
}

/// Preserve worker acceptance while an asynchronous host default completes.
/// The wait interval is reported separately rather than mislabeled queue time.
pub(crate) struct Deferred {
    input: InputStamp,
    accepted: Instant,
}
impl Deferred {
    pub fn capture(input: Option<InputStamp>) -> Option<Self> {
        input.map(|input| Self {
            input,
            accepted: Instant::now(),
        })
    }
}

#[derive(Default)]
pub(crate) struct Pending {
    first: Option<(InputStamp, Instant)>,
    inputs: usize,
    guest: Duration,
    deferred: Duration,
}
impl Pending {
    fn include(&mut self, input: InputStamp, accepted: Instant) {
        if self
            .first
            .is_none_or(|(first, _)| input.queued < first.queued)
        {
            self.first = Some((input, accepted));
        }
        self.inputs = self.inputs.saturating_add(1);
    }
    pub fn accept(&mut self, stamp: Option<InputStamp>) {
        if let Some(stamp) = stamp {
            self.include(stamp, Instant::now());
        }
    }
    pub fn complete(&mut self, stamp: Option<Deferred>) {
        if let Some(stamp) = stamp {
            self.deferred = self.deferred.max(stamp.accepted.elapsed());
            self.include(stamp.input, stamp.accepted);
        }
    }
    pub fn guest(&mut self, elapsed: Duration) {
        if self.first.is_some() {
            self.guest = self.guest.saturating_add(elapsed);
        }
    }
    pub fn frame(&mut self, render_started: Instant) -> Option<FrameTiming> {
        let pending = std::mem::take(self);
        pending.first.map(|(input, accepted)| FrameTiming {
            input,
            accepted,
            inputs: pending.inputs,
            guest: pending.guest,
            deferred: pending.deferred,
            render: render_started.elapsed(),
            prepared: Instant::now(),
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FrameTiming {
    input: InputStamp,
    accepted: Instant,
    inputs: usize,
    guest: Duration,
    deferred: Duration,
    render: Duration,
    prepared: Instant,
}
impl FrameTiming {
    pub fn report(self, sequence: u64, backend: &str, copy_started: Instant, returned: Instant) {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        eprintln!(
            "varta-ui-timing: {{\"boundary\":\"queue-to-present-return\",\"frame\":{sequence},\"backend\":\"{backend}\",\"kind\":\"{}\",\"inputs\":{},\"queue_ms\":{:.3},\"guest_ms\":{:.3},\"deferred_ms\":{:.3},\"prepare_ms\":{:.3},\"frame_wait_ms\":{:.3},\"host_present_ms\":{:.3},\"total_ms\":{:.3}}}",
            self.input.kind,
            self.inputs,
            ms(self.accepted.saturating_duration_since(self.input.queued)),
            ms(self.guest),
            ms(self.deferred),
            ms(self.render),
            ms(copy_started.saturating_duration_since(self.prepared)),
            ms(returned.saturating_duration_since(copy_started)),
            ms(returned.saturating_duration_since(self.input.queued)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deferred_work_preserves_acceptance_and_does_not_become_queue_latency() {
        let now = Instant::now();
        let accepted = now - Duration::from_secs(1);
        let queued = accepted - Duration::from_millis(1);
        let mut pending = Pending::default();
        pending.accept(Some(InputStamp {
            queued: now,
            kind: "pointer",
        }));
        pending.complete(Some(Deferred {
            input: InputStamp {
                queued,
                kind: "clipboard",
            },
            accepted,
        }));
        let frame = pending.frame(now).unwrap();
        assert_eq!(frame.input.kind, "clipboard");
        assert_eq!(frame.accepted, accepted);
        assert_eq!(
            frame.accepted.duration_since(frame.input.queued),
            Duration::from_millis(1)
        );
        assert!(frame.deferred >= Duration::from_secs(1));
        assert_eq!(frame.inputs, 2);
        assert!(pending.frame(now).is_none());
    }
    #[test]
    fn timing_keeps_oldest_input_and_resets_at_the_frame_boundary() {
        let now = Instant::now();
        let mut pending = Pending::default();
        pending.accept(None);
        pending.guest(Duration::from_secs(1));
        assert!(pending.frame(now).is_none());
        pending.accept(Some(InputStamp {
            queued: now,
            kind: "key",
        }));
        pending.guest(Duration::from_millis(2));
        pending.accept(Some(InputStamp {
            queued: now + Duration::from_millis(1),
            kind: "scroll",
        }));
        pending.guest(Duration::from_millis(3));
        let frame = pending.frame(now).unwrap();
        assert_eq!(frame.input.queued, now);
        assert_eq!(frame.input.kind, "key");
        assert_eq!(frame.inputs, 2);
        assert_eq!(frame.guest, Duration::from_millis(5));
        assert!(pending.frame(now).is_none());
    }
}
