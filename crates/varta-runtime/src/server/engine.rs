//! The native server's engine: the runtime's compilation settings (so fuel
//! is identical), a pooling allocator sized by the registry, and an epoch
//! ticker that lets request tasks yield and meet their deadlines without a
//! thread per Store.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use varta_common::{abi, http};
use wasmtime::{Engine, InstanceAllocationStrategy, PoolingAllocationConfig};

/// How often the ticker advances the epoch: a running guest yields to the
/// executor and checks its deadline (and cancellation) at least this often.
pub(super) const TICK: Duration = Duration::from_millis(10);

/// The server engine for `slots` concurrent request instances, each memory
/// at most `http::REQUEST_MEMORY_BYTES`.
pub(super) fn engine(slots: usize) -> Result<Engine, wasmtime::Error> {
    let slots = u32::try_from(slots).map_err(wasmtime::Error::new)?;
    let mut pool = PoolingAllocationConfig::default();
    pool.total_core_instances(slots)
        .total_memories(slots)
        .total_tables(slots.saturating_mul(abi::MAX_TABLES as u32))
        .total_stacks(slots)
        .max_memories_per_module(abi::MAX_MEMORIES as u32)
        .max_tables_per_module(abi::MAX_TABLES as u32)
        .table_elements(abi::MAX_TABLE_ELEMENTS)
        .max_memory_size(http::REQUEST_MEMORY_BYTES);
    let mut config = crate::engine_config();
    config
        .allocation_strategy(InstanceAllocationStrategy::Pooling(pool))
        .memory_reservation(http::REQUEST_MEMORY_BYTES as u64);
    Engine::new(&config)
}

/// Advances `engine`'s epoch every [`TICK`] until dropped.
pub(super) struct Ticker {
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Ticker {
    pub(super) fn start(engine: Engine) -> std::io::Result<Self> {
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = stopped.clone();
        let thread = std::thread::Builder::new()
            .name("varta-http-epoch".into())
            .spawn(move || {
                while !flag.load(Ordering::Acquire) {
                    std::thread::sleep(TICK);
                    engine.increment_epoch();
                }
            })?;
        Ok(Self {
            stopped,
            thread: Some(thread),
        })
    }
}
impl Drop for Ticker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
