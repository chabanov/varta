//! H2: requests are tasks of a few worker threads. A CPU-bound guest yields
//! (so other requests are answered), streams beyond the old worker count
//! stay open at once, admission follows the memory ceiling, and yielding
//! changes no fuel.
use super::*;
use std::time::Instant;
use varta_common::{
    http::{HostLimits, HttpPolicy},
    language::LanguageProfile as P,
};

const SERVICE: &str = r#"
async function handle(request: Request): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (path == "/spin") {
    let total = 0;
    for (let i = 0; i < 10000000; i++) {
      total = (total + i) % 1000003;
    }
    return new Response(String(total));
  }
  if (path == "/hold") {
    const stream = new ReadableStream<Uint8Array>({
      pull: async (controller: ReadableStreamDefaultController<Uint8Array>): Promise<void> => {
        await new Promise<void>((resolve) => { setTimeout(resolve, 30000); });
        controller.close();
      },
    });
    return new Response(stream);
  }
  return new Response("quick");
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

fn timers() -> CapabilityGrant {
    CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    }
}

fn start(limits: Option<HostLimits>) -> Server {
    let bytes =
        varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap();
    Server::start_options(
        bytes,
        RunOptions {
            requested: vec![timers()],
            http: limits.map(|limits| HttpPolicy {
                limits: Some(limits),
            }),
            ..RunOptions::default()
        },
    )
}

/// Send a GET and keep the connection with its response head read.
fn hold(server: &Server, target: &str) -> TcpStream {
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: tasks.test\r\n\r\n").as_bytes())
        .unwrap();
    let mut head = [0; 12];
    stream.read_exact(&mut head).unwrap();
    assert_eq!(&head, b"HTTP/1.1 200");
    stream
}

#[test]
fn a_cpu_bound_guest_yields_so_other_requests_are_answered() {
    let server = start(None);
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    // Twice as many spinning requests as worker threads.
    let spinning: Vec<_> = (0..2 * threads)
        .map(|_| {
            let addr = server.addr;
            thread::spawn(move || {
                let mut stream = TcpStream::connect(addr).unwrap();
                stream
                    .write_all(
                        b"GET /spin HTTP/1.1\r\nHost: tasks.test\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                response
            })
        })
        .collect();
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    let (status, _, _) = server.request("GET", "/quick", b"");
    let waited = started.elapsed();
    assert_eq!(status, 200);
    for spin in spinning {
        let response = spin.join().unwrap();
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "{response} {:?}",
            server.diagnostics.lock().unwrap()
        );
    }
    // A request that waited for a whole spin would take far longer.
    assert!(waited < Duration::from_secs(2), "{waited:?}");
    let reports = server.requests.lock().unwrap().clone();
    server.finish();
    // Yielding consumes no fuel: every spin used the same fuel.
    let fuels: std::collections::BTreeSet<u64> = reports
        .iter()
        .filter_map(|report| report.budget.as_ref())
        .filter(|budget| budget.total_fuel > 1_000_000)
        .map(|budget| budget.total_fuel)
        .collect();
    assert_eq!(fuels.len(), 1, "{fuels:?}");
}

#[test]
fn streams_beyond_the_old_worker_count_stay_open_at_once() {
    const STREAMS: usize = 64;
    let server = start(None);
    let open: Vec<TcpStream> = (0..STREAMS).map(|_| hold(&server, "/hold")).collect();
    // Every stream holds a request; a further request is still answered.
    assert_eq!(server.request("GET", "/quick", b"").0, 200);
    drop(open);
    server.finish();
}

#[test]
fn admission_follows_the_memory_ceiling() {
    let request = varta_common::http::REQUEST_MEMORY_BYTES;
    let server = start(Some(HostLimits {
        memory_bytes: Some(2 * request),
        worker_wait_ms: Some(300),
        ..HostLimits::default()
    }));
    let first = hold(&server, "/hold");
    let second = hold(&server, "/hold");
    // A third request's memory does not fit: 503 after the worker wait.
    let (status, headers, _) = server.request("GET", "/quick", b"");
    assert_eq!(status, 503, "{headers}");
    assert!(headers.to_ascii_lowercase().contains("retry-after: 1"));
    drop(first);
    // Once a stream ends its memory is free again.
    let started = Instant::now();
    loop {
        let (status, _, _) = server.request("GET", "/quick", b"");
        if status == 200 {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    drop(second);
    server.finish();
    // A ceiling below one request fails before bind.
    let bytes =
        varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap();
    let runtime = VartaRuntime::new();
    let endpoint = runtime
        .inspect_module(&bytes)
        .unwrap()
        .http_service
        .unwrap()
        .bind;
    let report = runtime.run_with(
        &bytes,
        RunOptions {
            requested: vec![grant(endpoint)],
            http: Some(HttpPolicy {
                limits: Some(HostLimits {
                    memory_bytes: Some(request - 1),
                    ..HostLimits::default()
                }),
            }),
            ..RunOptions::default()
        },
    );
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Http(HttpError::Policy(_)))
        ),
        "{:?}",
        report.failure
    );
}
