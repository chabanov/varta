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

#[derive(Default)]
pub(crate) struct Pending {
    first: Option<(InputStamp, Instant)>,
    inputs: usize,
    guest: Duration,
}
impl Pending {
    pub fn accept(&mut self, stamp: Option<InputStamp>) {
        if let Some(stamp) = stamp {
            self.first.get_or_insert((stamp, Instant::now()));
            self.inputs = self.inputs.saturating_add(1);
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
    render: Duration,
    prepared: Instant,
}
impl FrameTiming {
    pub fn report(self, sequence: u64, backend: &str, copy_started: Instant, returned: Instant) {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        eprintln!(
            "varta-ui-timing: {{\"boundary\":\"queue-to-present-return\",\"frame\":{sequence},\"backend\":\"{backend}\",\"kind\":\"{}\",\"inputs\":{},\"queue_ms\":{:.3},\"guest_ms\":{:.3},\"prepare_ms\":{:.3},\"frame_wait_ms\":{:.3},\"host_present_ms\":{:.3},\"total_ms\":{:.3}}}",
            self.input.kind,
            self.inputs,
            ms(self.accepted.saturating_duration_since(self.input.queued)),
            ms(self.guest),
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
