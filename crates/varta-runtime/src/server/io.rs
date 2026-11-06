//! Connection I/O bounds against slow clients.
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A connection whose writes must make progress: a write, flush or shutdown
/// that stays pending for `idle` fails with `TimedOut`, which ends the
/// connection, so a client that stops reading its response holds it no
/// longer. Any progress rearms the bound. Reads pass through: hyper bounds
/// a request head, and a streamed request body has its own idle bound.
pub(super) struct WriteIdle<T> {
    inner: T,
    /// `None`: writes are unbounded here (contract v1, which has its whole
    /// connection bound instead).
    idle: Option<Duration>,
    /// Armed while a write is pending.
    stalled: Option<Pin<Box<tokio::time::Sleep>>>,
}
impl<T> WriteIdle<T> {
    pub(super) fn new(inner: T, idle: Option<Duration>) -> Self {
        Self {
            inner,
            idle,
            stalled: None,
        }
    }
    /// `poll`, failed once it has been pending for the idle bound.
    fn progress<R>(
        &mut self,
        context: &mut Context<'_>,
        poll: Poll<io::Result<R>>,
    ) -> Poll<io::Result<R>> {
        if poll.is_ready() {
            self.stalled = None;
            return poll;
        }
        let Some(idle) = self.idle else {
            return Poll::Pending;
        };
        let stalled = self
            .stalled
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(idle)));
        if stalled.as_mut().poll(context).is_ready() {
            self.stalled = None;
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the client read no response bytes within the write idle bound",
            )));
        }
        Poll::Pending
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for WriteIdle<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(context, buffer)
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for WriteIdle<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_write(context, buffer);
        this.progress(context, poll)
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_write_vectored(context, buffers);
        this.progress(context, poll)
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_flush(context);
        this.progress(context, poll)
    }
    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_shutdown(context);
        this.progress(context, poll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn write<T: AsyncWrite + Unpin>(
        writer: &mut WriteIdle<T>,
        bytes: &[u8],
    ) -> io::Result<usize> {
        std::future::poll_fn(|context| Pin::new(&mut *writer).poll_write(context, bytes)).await
    }

    /// A writer that never accepts a byte.
    struct Blocked;
    impl AsyncWrite for Blocked {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn a_stalled_write_fails_after_the_idle_bound_and_progress_passes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let idle = Duration::from_millis(50);
            let started = std::time::Instant::now();
            let error = write(&mut WriteIdle::new(Blocked, Some(idle)), b"x")
                .await
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(started.elapsed() >= idle);
            let mut written = WriteIdle::new(Vec::new(), Some(idle));
            assert_eq!(write(&mut written, b"progress").await.unwrap(), 8);
            assert_eq!(written.inner, b"progress");
        });
    }
}
