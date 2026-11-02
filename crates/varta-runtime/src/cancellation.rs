//! Host-owned cancellation of one execution. Epoch checks stop guest loops;
//! cancellable native waits observe the same token without running guest work.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use wasmtime::Engine;

/// Maximum wait slice while an HTTP request can lose its owner. This never
/// advances guest time or fires a timer before its due time.
const WAIT_SLICE: Duration = Duration::from_millis(10);

#[derive(Debug, thiserror::Error)]
#[error("execution cancelled by its host owner")]
pub(crate) struct Cancelled;

#[derive(Clone)]
pub(crate) struct Cancellation {
    state: Arc<AtomicBool>,
    engine: Engine,
}
impl Cancellation {
    pub fn new(engine: Engine) -> Self {
        Self {
            state: Arc::new(AtomicBool::new(false)),
            engine,
        }
    }
    pub fn cancel(&self) {
        if !self.state.swap(true, Ordering::SeqCst) {
            self.engine.increment_epoch();
        }
    }
    pub fn check(&self) -> Result<(), wasmtime::Error> {
        if self.state.load(Ordering::SeqCst) {
            Err(wasmtime::Error::new(Cancelled))
        } else {
            Ok(())
        }
    }
    pub fn wait_until(&self, until: Instant) -> Instant {
        until.min(Instant::now() + WAIT_SLICE)
    }
}

/// Dropping an async owner does not abort a spawn_blocking worker by itself.
/// The guard signals its execution before that worker can outlive the owner.
pub(crate) struct CancelOnDrop(pub Option<Cancellation>);
impl CancelOnDrop {
    pub fn disarm(&mut self) {
        self.0 = None;
    }
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancellation) = &self.0 {
            cancellation.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        VartaRuntime,
        execution::{Execution, Setup},
        policy::FileMode,
    };
    #[test]
    fn cancellation_interrupts_guest_loops_without_cancelling_another_store() {
        let runtime = VartaRuntime::new();
        let looping = runtime
            .prepare_module(
                &varta_compiler::compile_typescript("export function main():void{while(true){}}")
                    .unwrap(),
            )
            .unwrap();
        let normal = runtime
            .prepare_module(
                &varta_compiler::compile_typescript(
                    "export function main():void{console.log('alive');}",
                )
                .unwrap(),
            )
            .unwrap();
        let token = Cancellation::new(runtime.engine.clone());
        let cancelled = token.clone();
        let engine = runtime.engine.clone();
        let (ready, started) = std::sync::mpsc::channel();
        let (completed, result) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut setup = Setup::production(&[], None, FileMode::HostPaths);
            setup.budgets.fuel_per_turn = varta_common::budget::MAX_FUEL_PER_TURN;
            let mut execution = Execution::with_setup(&engine, setup).unwrap();
            execution.store.data_mut().cancellation = Some(cancelled);
            let instance = execution.instantiate_prepared(&looping).unwrap();
            let start = instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap();
            ready.send(()).unwrap();
            completed
                .send(start.call(&mut execution.store, ()))
                .unwrap();
        });
        let mut other = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = other.instantiate_prepared(&normal).unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        token.cancel();
        let error = result
            .recv_timeout(Duration::from_secs(1))
            .expect("guest loop ignored cancellation")
            .unwrap_err();
        assert!(error.downcast_ref::<Cancelled>().is_some(), "{error:#}");
        instance
            .get_typed_func::<(), ()>(&mut other.store, "_start")
            .unwrap()
            .call(&mut other.store, ())
            .unwrap();
        assert_eq!(other.store.data().output.lines, ["varta log: alive"]);
        worker.join().unwrap();
    }
}
