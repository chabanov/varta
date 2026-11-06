//! K7 H4: floods and slow clients. A service under a connection flood, a
//! request flood, slow headers (slowloris) or slow bodies keeps its bounds,
//! answers other clients, and frees every slot once the attack ends.
use super::*;
use std::time::Instant;
use varta_common::{
    http::{HostLimits, HttpPolicy},
    language::LanguageProfile as P,
};

const SERVICE: &str = r#"
async function handle(request: Request): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (path == "/slow") {
    await new Promise<void>((resolve) => { setTimeout(resolve, 400); });
  }
  const text = await request.text();
  return new Response("ok " + String(text.length));
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

fn start(limits: HostLimits) -> Server {
    let bytes =
        varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap();
    Server::start_options(
        bytes,
        RunOptions {
            requested: vec![CapabilityGrant {
                id: "time.timer".into(),
                value: CapabilityValue::Boolean(true),
            }],
            http: Some(HttpPolicy {
                limits: Some(limits),
                tls: None,
            }),
            ..RunOptions::default()
        },
    )
}

#[test]
fn slow_headers_from_many_clients_are_cut_at_the_idle_bound() {
    const CLIENTS: usize = 64;
    let idle = Duration::from_millis(500);
    let server = start(HostLimits {
        keep_alive_idle_ms: Some(idle.as_millis() as u64),
        ..HostLimits::default()
    });
    // Each client sends a byte of its head now and then, never the end.
    let started = Instant::now();
    let mut clients: Vec<TcpStream> = (0..CLIENTS)
        .map(|_| {
            let mut stream = TcpStream::connect(server.addr).unwrap();
            stream.write_all(b"GET / HTTP/1.1\r\nX-Slow:").unwrap();
            stream
        })
        .collect();
    for _ in 0..5 {
        thread::sleep(Duration::from_millis(150));
        for client in &mut clients {
            let _ = client.write_all(b"a");
        }
    }
    // Every slow connection ends within the idle bound of its start.
    for client in &mut clients {
        // A socket the server has already closed may refuse the option.
        let _ = client.set_read_timeout(Some(Duration::from_secs(10)));
        let mut rest = Vec::new();
        let _ = client.read_to_end(&mut rest);
    }
    assert!(started.elapsed() < idle * 6, "{:?}", started.elapsed());
    // Meanwhile and afterwards the service answers at once.
    assert_eq!(server.request("GET", "/", b"").0, 200);
    server.finish();
}

#[test]
fn a_connection_flood_waits_in_the_backlog_and_a_request_flood_gets_503() {
    const CONNECTIONS: usize = 8;
    let server = start(HostLimits {
        connections: Some(CONNECTIONS),
        workers: Some(2),
        worker_wait_ms: Some(200),
        ..HostLimits::default()
    });
    // Fill every connection slot with an idle keep-alive connection.
    let held: Vec<TcpStream> = (0..CONNECTIONS)
        .map(|_| TcpStream::connect(server.addr).unwrap())
        .collect();
    // A further connection is accepted by the OS but not served yet.
    let mut waiting = TcpStream::connect(server.addr).unwrap();
    waiting
        .write_all(b"GET / HTTP/1.1\r\nHost: flood.test\r\nConnection: close\r\n\r\n")
        .unwrap();
    waiting
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut early = [0; 1];
    assert!(
        waiting.read(&mut early).is_err(),
        "served beyond its connections"
    );
    // Freeing slots (the held connections idle out or close) serves it.
    drop(held);
    waiting
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut response = String::new();
    waiting.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    // A request flood beyond the two slots gets 503 with Retry-After for
    // what waits longer than the worker wait, and 200 for the rest.
    let flood: Vec<_> = (0..12)
        .map(|_| {
            let addr = server.addr;
            thread::spawn(move || {
                let mut stream = TcpStream::connect(addr).unwrap();
                stream
                    .write_all(
                        b"GET /slow HTTP/1.1\r\nHost: flood.test\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                response
            })
        })
        .collect();
    let responses: Vec<String> = flood.into_iter().map(|t| t.join().unwrap()).collect();
    let busy = responses
        .iter()
        .filter(|r| r.starts_with("HTTP/1.1 503"))
        .count();
    assert!(busy > 0, "{responses:?}");
    assert!(responses.iter().all(|r| {
        r.starts_with("HTTP/1.1 200") || r.to_ascii_lowercase().contains("retry-after: 1")
    }));
    // Every slot is free again.
    assert_eq!(server.request("GET", "/", b"").0, 200);
    server.finish();
}

#[test]
fn slow_request_bodies_from_many_clients_end_at_the_body_idle_bound() {
    const CLIENTS: usize = 32;
    let server = start(HostLimits {
        body_idle_ms: Some(300),
        ..HostLimits::default()
    });
    let started = Instant::now();
    let clients: Vec<_> = (0..CLIENTS)
        .map(|_| {
            let addr = server.addr;
            thread::spawn(move || {
                let mut stream = TcpStream::connect(addr).unwrap();
                stream
                    .write_all(b"POST / HTTP/1.1\r\nHost: flood.test\r\nConnection: close\r\nContent-Length: 100\r\n\r\nabc")
                    .unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut response = String::new();
                let _ = stream.read_to_string(&mut response);
                response
            })
        })
        .collect();
    for client in clients {
        // The guest's read fails: the handler's error is a 500 of its own.
        let response = client.join().unwrap();
        assert!(response.starts_with("HTTP/1.1 500"), "{response}");
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(server.request("GET", "/", b"").0, 200);
    server.finish();
}
