//! Wake the renderer without adding pointer samples to the activation FIFO.
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Wake {
    generation: Mutex<u64>,
    changed: Condvar,
}
impl Wake {
    pub fn snapshot(&self) -> u64 {
        *self.generation.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn notify(&self) {
        let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        *generation = generation.wrapping_add(1);
        self.changed.notify_one();
    }
    pub fn wait(&self, observed: u64, timeout: Duration) {
        let generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = self
            .changed
            .wait_timeout_while(generation, timeout, |current| *current == observed)
            .unwrap_or_else(|e| e.into_inner());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_before_wait_is_not_lost() {
        let wake = Wake::default();
        let seen = wake.snapshot();
        wake.notify();
        // If the generation check regresses, this test blocks until the timeout.
        let start = std::time::Instant::now();
        wake.wait(seen, Duration::from_secs(5));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_ne!(wake.snapshot(), seen);
    }
}
