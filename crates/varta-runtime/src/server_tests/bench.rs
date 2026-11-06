//! H4 load benchmarks (ignored; run in release):
//! `cargo test -p varta-runtime --release --lib -- --ignored --nocapture server_tests::bench`.
//! They print throughput, latency percentiles and memory per idle
//! connection and per open stream, the acceptance numbers recorded in the
//! development plan. They assert only what a correct server must do.
use super::*;
use std::io::{BufRead, BufReader};
use std::time::Instant;
use varta_common::language::LanguageProfile as P;

const SERVICE: &str = r#"
async function handle(request: Request): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (path == "/events") {
    const stream = new ReadableStream<Uint8Array>({
      pull: async (controller: ReadableStreamDefaultController<Uint8Array>): Promise<void> => {
        await new Promise<void>((resolve) => { setTimeout(resolve, 60000); });
        controller.enqueue(new TextEncoder().encode("data: tick\n\n"));
      },
    });
    return new Response(stream, { headers: { "content-type": "text/event-stream" } });
  }
  const text = await request.text();
  return new Response(JSON.stringify({ path: path, bytes: text.length }), {
    headers: { "content-type": "application/json" },
  });
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

fn start() -> Server {
    let bytes =
        varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap();
    Server::start_bytes(
        bytes,
        vec![CapabilityGrant {
            id: "time.timer".into(),
            value: CapabilityValue::Boolean(true),
        }],
    )
}

/// The resident set of this process in bytes (`ps`, no unsafe code).
fn resident_bytes() -> usize {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<usize>()
        .unwrap()
        * 1024
}

/// One keep-alive client: `requests` sequential GETs, each latency recorded.
fn client(addr: SocketAddr, requests: usize) -> Vec<Duration> {
    let stream = TcpStream::connect(addr).unwrap();
    stream.set_nodelay(true).unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    let mut latencies = Vec::with_capacity(requests);
    for _ in 0..requests {
        let started = Instant::now();
        writer
            .write_all(b"GET /item HTTP/1.1\r\nHost: bench.test\r\n\r\n")
            .unwrap();
        read_response(&mut reader);
        latencies.push(started.elapsed());
    }
    latencies
}

/// Read one chunked or sized response off a keep-alive connection.
fn read_response(reader: &mut BufReader<TcpStream>) {
    let mut length = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
        chunked |= lower.starts_with("transfer-encoding: chunked");
        if line == "\r\n" {
            break;
        }
    }
    if let Some(length) = length {
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        return;
    }
    assert!(chunked);
    loop {
        let mut size = String::new();
        reader.read_line(&mut size).unwrap();
        let size = usize::from_str_radix(size.trim(), 16).unwrap();
        let mut data = vec![0; size + 2];
        reader.read_exact(&mut data).unwrap();
        if size == 0 {
            return;
        }
    }
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    sorted[((sorted.len() as f64 - 1.0) * fraction).round() as usize]
}

#[test]
#[ignore = "load benchmark; run in release with --ignored"]
fn bench_throughput_and_latency_of_web_requests() {
    const CLIENTS: usize = 32;
    const REQUESTS: usize = 500;
    let server = start();
    // Warm up the pool.
    client(server.addr, 10);
    let started = Instant::now();
    let workers: Vec<_> = (0..CLIENTS)
        .map(|_| {
            let addr = server.addr;
            thread::spawn(move || client(addr, REQUESTS))
        })
        .collect();
    let mut latencies: Vec<Duration> = workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect();
    let elapsed = started.elapsed();
    latencies.sort();
    let total = latencies.len();
    println!(
        "bench web requests: {CLIENTS} keep-alive clients, {total} requests in {elapsed:?}: {:.0} requests/s, p50 {:?}, p99 {:?}, max {:?}",
        total as f64 / elapsed.as_secs_f64(),
        percentile(&latencies, 0.50),
        percentile(&latencies, 0.99),
        latencies[total - 1],
    );
    server.finish();
}

#[test]
#[ignore = "memory benchmark; run in release with --ignored"]
fn bench_memory_per_idle_connection_and_per_open_stream() {
    const CONNECTIONS: usize = 1000;
    const STREAMS: usize = 100;
    let server = start();
    client(server.addr, 10);
    let before = resident_bytes();
    // Idle keep-alive connections, each having answered one request.
    let idle: Vec<_> = (0..CONNECTIONS)
        .map(|_| {
            let stream = TcpStream::connect(server.addr).unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            writer
                .write_all(b"GET /item HTTP/1.1\r\nHost: bench.test\r\n\r\n")
                .unwrap();
            read_response(&mut reader);
            reader
        })
        .collect();
    let connected = resident_bytes();
    println!(
        "bench memory: {CONNECTIONS} idle connections add {} bytes ({} per connection)",
        connected.saturating_sub(before),
        connected.saturating_sub(before) / CONNECTIONS
    );
    drop(idle);
    thread::sleep(Duration::from_millis(500));
    let before = resident_bytes();
    let streams: Vec<_> = (0..STREAMS)
        .map(|_| {
            let mut stream = TcpStream::connect(server.addr).unwrap();
            stream
                .write_all(b"GET /events HTTP/1.1\r\nHost: bench.test\r\n\r\n")
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("HTTP/1.1 200"), "{line}");
            reader
        })
        .collect();
    let open = resident_bytes();
    println!(
        "bench memory: {STREAMS} open event streams add {} bytes ({} per stream)",
        open.saturating_sub(before),
        open.saturating_sub(before) / STREAMS
    );
    drop(streams);
    server.finish();
}
