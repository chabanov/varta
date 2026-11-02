//! System DNS runs outside execution-owned Tokio runtimes. OS lookups cannot
//! be forcibly interrupted; a fixed host pool bounds them, and dropped/expired
//! requests neither retain a Store nor start a queued lookup.
use std::{
    io,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::JoinHandle,
    time::Instant,
};
use tokio::sync::oneshot;

use varta_common::net::{
    MAX_DNS_ADDRESSES as ADDRESSES, MAX_DNS_QUEUED as QUEUED, MAX_DNS_WORKERS as WORKERS,
};
type Addresses = io::Result<Vec<SocketAddr>>;
type Lookup = Arc<dyn Fn(&str, u16) -> Addresses + Send + Sync>;
struct Job {
    host: String,
    port: u16,
    deadline: Instant,
    result: oneshot::Sender<Addresses>,
}
struct Resolver {
    jobs: Option<SyncSender<Job>>,
    workers: Vec<JoinHandle<()>>,
}
impl Resolver {
    fn new(workers: usize, queued: usize, lookup: Lookup) -> io::Result<Self> {
        if workers == 0 || queued == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty DNS pool",
            ));
        }
        let (jobs, receive) = mpsc::sync_channel::<Job>(queued);
        let receive = Arc::new(Mutex::new(receive));
        let mut resolver = Self {
            jobs: Some(jobs),
            workers: Vec::new(),
        };
        for index in 0..workers {
            let receive = receive.clone();
            let lookup = lookup.clone();
            resolver.workers.push(
                std::thread::Builder::new()
                    .name(format!("varta-dns-{index}"))
                    .spawn(move || {
                        loop {
                            let job = match receive.lock() {
                                Ok(receiver) => receiver.recv(),
                                Err(_) => break,
                            };
                            let Ok(job) = job else {
                                break;
                            };
                            if job.result.is_closed() {
                                continue;
                            }
                            if Instant::now() >= job.deadline {
                                let _ = job.result.send(Err(io::Error::new(
                                    io::ErrorKind::TimedOut,
                                    "DNS lookup expired",
                                )));
                                continue;
                            }
                            let addresses = lookup(&job.host, job.port).and_then(|addresses| {
                                if addresses.len() > ADDRESSES {
                                    Err(io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        "DNS address limit",
                                    ))
                                } else if addresses.is_empty() {
                                    Err(io::Error::new(
                                        io::ErrorKind::NotFound,
                                        "DNS returned no addresses",
                                    ))
                                } else {
                                    Ok(addresses)
                                }
                            });
                            // A receiver closed while the OS resolver ran owns no guest
                            // references; sending simply releases the native result.
                            let addresses = if Instant::now() >= job.deadline {
                                Err(expired())
                            } else {
                                addresses
                            };
                            let _ = job.result.send(addresses);
                        }
                    })?,
            );
        }
        Ok(resolver)
    }
    fn submit(
        &self,
        host: &str,
        port: u16,
        deadline: Instant,
    ) -> io::Result<oneshot::Receiver<Addresses>> {
        if Instant::now() >= deadline {
            return Err(expired());
        }
        let (result, receive) = oneshot::channel();
        let Some(jobs) = &self.jobs else {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "DNS pool closed"));
        };
        jobs.try_send(Job {
            host: host.to_owned(),
            port,
            deadline,
            result,
        })
        .map_err(|error| match error {
            TrySendError::Full(_) => io::Error::new(io::ErrorKind::WouldBlock, "DNS queue limit"),
            TrySendError::Disconnected(_) => {
                io::Error::new(io::ErrorKind::BrokenPipe, "DNS pool closed")
            }
        })?;
        Ok(receive)
    }
    async fn resolve(&self, host: &str, port: u16, deadline: Instant) -> Addresses {
        let receive = self.submit(host, port, deadline)?;
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), receive)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS lookup deadline"))?
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "DNS lookup stopped"))?
    }
}
impl Drop for Resolver {
    fn drop(&mut self) {
        self.jobs.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// The process owns this fixed service; a request owns only its result channel.
/// Numeric addresses do not start DNS workers or queue any host work.
pub(super) async fn resolve(host: &str, port: u16, deadline: Instant) -> Addresses {
    if Instant::now() >= deadline {
        return Err(expired());
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(address, port)]);
    }
    static RESOLVER: OnceLock<Result<Resolver, String>> = OnceLock::new();
    let resolver = RESOLVER.get_or_init(|| {
        Resolver::new(
            WORKERS,
            QUEUED,
            Arc::new(|host, port| {
                // Inspect at most one more than the host bound; do not silently choose
                // a truncated address list with different connection semantics.
                (host, port)
                    .to_socket_addrs()
                    .map(|addresses| addresses.take(ADDRESSES + 1).collect())
            }),
        )
        .map_err(|error| error.to_string())
    });
    let resolver = resolver
        .as_ref()
        .map_err(|error| io::Error::other(error.clone()))?;
    resolver.resolve(host, port, deadline).await
}

fn expired() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "DNS lookup expired")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    fn answer(port: u16) -> Vec<SocketAddr> {
        vec![SocketAddr::from(([127, 0, 0, 1], port))]
    }
    #[test]
    fn resolver_queue_is_bounded_and_cancelled_or_expired_jobs_do_not_lookup() {
        let (entered, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let wait = Mutex::new(wait);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let resolver = Resolver::new(
            1,
            2,
            Arc::new(move |_, port| {
                observed.fetch_add(1, Ordering::SeqCst);
                entered.send(()).unwrap();
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
                Ok(answer(port))
            }),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let first = resolver.submit("first", 80, deadline).unwrap();
        started.recv_timeout(Duration::from_secs(1)).unwrap();
        let cancelled = resolver.submit("cancelled", 80, deadline).unwrap();
        drop(cancelled);
        let queued_deadline = Instant::now() + Duration::from_millis(50);
        let expired = resolver.submit("expired", 80, queued_deadline).unwrap();
        assert_eq!(
            resolver.submit("full", 80, deadline).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        std::thread::sleep(queued_deadline.saturating_duration_since(Instant::now()));
        release.send(()).unwrap();
        assert_eq!(first.blocking_recv().unwrap().unwrap(), answer(80));
        assert_eq!(
            expired.blocking_recv().unwrap().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        drop(resolver);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn dropping_a_request_runtime_does_not_wait_for_system_lookup() {
        let (entered, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let wait = Mutex::new(wait);
        let pool = Arc::new(
            Resolver::new(
                1,
                1,
                Arc::new(move |_, port| {
                    entered.send(()).unwrap();
                    wait.lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(2))
                        .unwrap();
                    Ok(answer(port))
                }),
            )
            .unwrap(),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let request = pool.clone();
        runtime.spawn(async move {
            request
                .resolve("held", 80, Instant::now() + Duration::from_secs(2))
                .await
        });
        runtime.block_on(tokio::task::yield_now());
        started.recv_timeout(Duration::from_secs(1)).unwrap();
        let at = Instant::now();
        drop(runtime);
        assert!(at.elapsed() < Duration::from_secs(1));
        assert_eq!(Arc::strong_count(&pool), 1);
        release.send(()).unwrap();
        drop(pool);
    }
    #[test]
    fn resolver_rejects_oversized_answers_without_silently_truncating() {
        let resolver = Resolver::new(
            1,
            1,
            Arc::new(|_, _| Ok(vec![answer(80)[0]; ADDRESSES + 1])),
        )
        .unwrap();
        let result = resolver
            .submit("wide", 80, Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            result.blocking_recv().unwrap().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn a_system_lookup_finishing_after_its_deadline_cannot_return_success() {
        let (entered, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let wait = Mutex::new(wait);
        let resolver = Resolver::new(
            1,
            1,
            Arc::new(move |_, port| {
                entered.send(()).unwrap();
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
                Ok(answer(port))
            }),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_millis(100);
        let result = resolver.submit("late", 80, deadline).unwrap();
        started.recv_timeout(Duration::from_secs(1)).unwrap();
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        release.send(()).unwrap();
        assert_eq!(
            result.blocking_recv().unwrap().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn numeric_addresses_also_obey_the_deadline() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = runtime.block_on(resolve("127.0.0.1", 80, Instant::now()));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
}
