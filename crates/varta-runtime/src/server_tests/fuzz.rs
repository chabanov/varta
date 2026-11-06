//! K7 H4: seeded fuzzing of the request envelopes and the compiled
//! adapters. Every random request reaches its handler exactly as sent (or
//! is refused before dispatch with a 4xx), never with a 500, and the
//! service keeps answering.
use super::*;
use rand::{Rng, SeedableRng, rngs::StdRng};
use varta_common::language::LanguageProfile as P;

/// The seed of every fuzz run: a failure reproduces exactly.
const SEED: u64 = 0x7661_7274_615f_6b37;
const REQUESTS: usize = 120;

/// A handler that echoes what it received, for each adapter form.
const SYNC: &str = r#"
function handle(r: VartaHttpRequest): VartaHttpResponse {
  return { status: 200, body: JSON.stringify({ method: r.method, target: r.target, body: r.body, headers: r.headers.length }), headers: [] };
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;
const ASYNC: &str = r#"
async function handle(r: VartaHttpRequest): Promise<VartaHttpResponse> {
  await Promise.resolve();
  return { status: 200, body: JSON.stringify({ method: r.method, target: r.target, body: r.body, headers: r.headers.length }), headers: [] };
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;
const WEB: &str = r#"
async function handle(r: Request): Promise<Response> {
  const body = await r.text();
  let headers = 0;
  r.headers.forEach((value: string, name: string, parent: Headers): void => { headers += 1; });
  return new Response(JSON.stringify({ method: r.method, target: r.url, body: body, headers: headers }));
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

const METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
/// Target characters a client may send unescaped (RFC 3986 pchar and query).
const TARGET: &[u8] = b"abcXYZ019-._~!$&'()*+,;=:@/?%";
/// Header value characters (visible ASCII and space).
const VALUE: std::ops::RangeInclusive<u8> = 0x20..=0x7e;

struct Generated {
    method: &'static str,
    target: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn generate(random: &mut StdRng, body_bytes: usize) -> Generated {
    let method = METHODS[random.gen_range(0..METHODS.len())];
    let mut target = String::from("/");
    for _ in 0..random.gen_range(0..40) {
        let byte = TARGET[random.gen_range(0..TARGET.len())];
        // A `%` starts a valid escape.
        if byte == b'%' {
            target.push_str(&format!("%{:02X}", random.gen_range(0..=255u8)));
        } else {
            target.push(byte as char);
        }
    }
    let headers = (0..random.gen_range(0..8))
        .map(|index| {
            let value: String = (0..random.gen_range(0..40))
                .map(|_| random.gen_range(VALUE) as char)
                .collect();
            (format!("x-fuzz-{index}"), value.trim().to_owned())
        })
        .collect();
    let body = if method == "GET" || method == "OPTIONS" {
        String::new()
    } else {
        // UTF-8 text, including supplementary characters.
        (0..random.gen_range(0..body_bytes / 4))
            .map(|_| match random.gen_range(0..4) {
                0 => 'я',
                1 => '🦀',
                2 => '"',
                _ => random.gen_range(b'a'..=b'z') as char,
            })
            .collect()
    };
    Generated {
        method,
        target,
        headers,
        body,
    }
}

fn wire(request: &Generated) -> Vec<u8> {
    let mut wire = format!(
        "{} {} HTTP/1.1\r\nHost: fuzz.test\r\nConnection: close\r\nContent-Length: {}\r\n",
        request.method,
        request.target,
        request.body.len()
    );
    for (name, value) in &request.headers {
        wire.push_str(&format!("{name}: {value}\r\n"));
    }
    wire.push_str("\r\n");
    let mut wire = wire.into_bytes();
    wire.extend(request.body.as_bytes());
    wire
}

/// The data of a chunked body, or the body itself.
fn data(headers: &str, body: &str) -> String {
    if !headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return body.to_owned();
    }
    let mut rest = body;
    let mut data = String::new();
    while let Some((size, tail)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size, 16).unwrap();
        if size == 0 {
            break;
        }
        data.push_str(&tail[..size]);
        rest = &tail[size + 2..];
    }
    data
}

/// Send `REQUESTS` random requests; each echo must equal what was sent, its
/// target as `target` presents it (a `VartaHttpRequest` target, or a Web
/// `Request`'s absolute URL).
fn fuzz(source: &str, profile: P, seed: u64, target: impl Fn(&str) -> String) {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
    let server = Server::start_bytes(bytes, Vec::new());
    let mut random = StdRng::seed_from_u64(seed);
    for _ in 0..REQUESTS {
        let request = generate(&mut random, varta_common::http::MAX_BODY_BYTES);
        let (status, headers, body) = server.raw(&wire(&request));
        assert_eq!(
            status, 200,
            "{} {}: {headers}",
            request.method, request.target
        );
        let echoed: serde_json::Value = serde_json::from_str(&data(&headers, &body)).unwrap();
        // The transport adds `host`, `connection` and `content-length`.
        assert_eq!(
            echoed,
            serde_json::json!({
                "method": request.method,
                "target": target(&request.target),
                "body": request.body,
                "headers": request.headers.len() + 3,
            }),
            "{}",
            request.target
        );
    }
    let diagnostics = server.diagnostics.lock().unwrap().clone();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    server.finish();
}

#[test]
fn random_requests_reach_the_v1_adapter_unchanged() {
    fuzz(SYNC, P::default(), SEED, str::to_owned);
}

#[test]
fn random_requests_reach_the_v2_adapters_unchanged() {
    fuzz(SYNC, P::CheckedF64Utf16V38, SEED ^ 1, str::to_owned);
    fuzz(ASYNC, P::CheckedF64Utf16V38, SEED ^ 2, str::to_owned);
    fuzz(WEB, P::CheckedF64Utf16V38, SEED ^ 3, |target| {
        format!("http://fuzz.test{target}")
    });
}

#[test]
fn malformed_requests_are_refused_before_dispatch_and_the_service_recovers() {
    let bytes =
        varta_compiler::compile_typescript_with_profile(SYNC, P::CheckedF64Utf16V38).unwrap();
    let server = Server::start_bytes(bytes, Vec::new());
    let mut random = StdRng::seed_from_u64(SEED ^ 4);
    let valid = wire(&generate(&mut random, 64));
    for _ in 0..REQUESTS {
        let mut mutated = valid.clone();
        for _ in 0..random.gen_range(1..4) {
            let at = random.gen_range(0..mutated.len());
            match random.gen_range(0..3) {
                0 => mutated[at] = random.r#gen(),
                1 => {
                    mutated.insert(at, random.r#gen());
                }
                _ => {
                    mutated.remove(at);
                }
            }
        }
        let mut stream = TcpStream::connect(server.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let _ = stream.write_all(&mutated);
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        // Any answer is a parsed status line, never a 500.
        let text = String::from_utf8_lossy(&response);
        if let Some(status) = text.split_whitespace().nth(1) {
            assert_ne!(status, "500", "{text}");
        }
    }
    assert_eq!(server.request("GET", "/after", b"").0, 200);
    server.finish();
}
