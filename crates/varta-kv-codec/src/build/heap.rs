//! Synchronous adapters of the yielding heap contract.
use varta_common::json::build::Heap;
/// The same top-down builder with allocation/retention that may yield.
/// Memory stores remain synchronous and cannot expose a partial result.
#[allow(async_fn_in_trait)]
pub trait AsyncHeap: Heap {
    async fn allocate_async(&mut self, len: usize) -> Result<usize, Self::Error>;
    async fn retain_async(&mut self, handle: u64) -> Result<(), Self::Error>;
}

pub(crate) struct Synchronous<'a, H>(pub(crate) &'a mut H);
impl<H: Heap> Heap for Synchronous<'_, H> {
    type Error = H::Error;
    fn allocate(&mut self, len: usize) -> Result<usize, Self::Error> {
        self.0.allocate(len)
    }
    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.write(at, bytes)
    }
    fn zero(&mut self, at: usize, len: usize) -> Result<(), Self::Error> {
        self.0.zero(at, len)
    }
    fn retain(&mut self, handle: u64) -> Result<(), Self::Error> {
        self.0.retain(handle)
    }
}
impl<H: Heap> AsyncHeap for Synchronous<'_, H> {
    async fn allocate_async(&mut self, len: usize) -> Result<usize, Self::Error> {
        self.0.allocate(len)
    }
    async fn retain_async(&mut self, handle: u64) -> Result<(), Self::Error> {
        self.0.retain(handle)
    }
}
/// Only futures driven by the immediate synchronous adapter enter here.
/// A pending future is an internal programming error, never retried or blocked.
pub(crate) fn run_sync<F: core::future::Future>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future);
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        core::task::Poll::Ready(value) => value,
        core::task::Poll::Pending => unreachable!("synchronous heap suspended"),
    }
}
