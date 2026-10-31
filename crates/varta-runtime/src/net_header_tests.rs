//! V38 response header visibility (`net.request` `responseHeaders`): a v38
//! program sees exactly the declared headers the response carries (the
//! CORS-safelisted names without a declaration), never `set-cookie`, in the
//! canonical head; the host ceiling narrows the set; undeclared bytes are
//! never charged; v37 keeps every received field. Loopback servers only.
use crate::net_loopback::{Loopback, PAD_BYTES};
use crate::*;
use varta_common::language::LanguageProfile;
use varta_common::net::REQUEST_CAPABILITY;
use varta_common::{CapabilityValue, RequestScope};

const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;

fn compile(source: &str, profile: LanguageProfile) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

/// The server's whole origin, showing `headers` (`None`: no declaration).
fn grant(server: &Loopback, headers: Option<&[&str]>) -> CapabilityGrant {
    CapabilityGrant {
        id: REQUEST_CAPABILITY.into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: vec![format!("{}/**", server.base)],
            methods: None,
            response_headers: headers.map(|h| h.iter().map(|h| (*h).into()).collect()),
        }),
    }
}

/// Every header the guest sees, then the reads of hidden names.
const HEADERS: &str = "export async function main(): Promise<void> {
  const r = await fetch('__BASE__/headers');
  const parts: string[] = [];
  r.headers.forEach((value: string, name: string) => {
    parts.push(name + '=' + value);
  });
  console.log(parts.join(';'));
  console.log(String(r.headers.get('x-hidden')) + ' ' + String(r.headers.get('set-cookie')) + ' ' + String(r.headers.getSetCookie().length) + ' ' + String(r.headers.get('X-B')));
}";

fn run(
    server: &Loopback,
    profile: LanguageProfile,
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
) -> RunReport {
    let source = HEADERS.replace("__BASE__", &server.base);
    VartaRuntime::new().run_with(
        &compile(&source, profile),
        RunOptions {
            requested,
            ceiling,
            ..RunOptions::default()
        },
    )
}

fn lines(report: &RunReport) -> Vec<String> {
    assert!(report.failure.is_none(), "{:?}", report.failure);
    report.output.lines.clone()
}

#[test]
fn v38_programs_see_exactly_the_declared_headers() {
    let server = Loopback::start();
    let declared = grant(&server, Some(&["x-a", "x-b", "content-type", "x-absent"]));
    assert_eq!(
        lines(&run(&server, V38, vec![declared], None)),
        [
            "varta log: content-type=text/plain;x-a=z;x-b=1, 2",
            "varta log: null null 0 1, 2",
        ]
    );
    // Without a declaration only the CORS-safelisted names show.
    assert_eq!(
        lines(&run(&server, V38, vec![grant(&server, None)], None)),
        [
            "varta log: cache-control=no-store;content-length=1;content-type=text/plain",
            "varta log: null null 0 null",
        ]
    );
    // An empty declaration shows nothing.
    assert_eq!(
        lines(&run(&server, V38, vec![grant(&server, Some(&[]))], None)),
        ["varta log: ", "varta log: null null 0 null"]
    );
}

#[test]
fn the_host_ceiling_narrows_the_visible_headers() {
    let server = Loopback::start();
    let requested = vec![grant(&server, Some(&["x-a", "x-b"]))];
    let narrowed = run(
        &server,
        V38,
        requested.clone(),
        Some(vec![grant(&server, Some(&["x-b", "x-hidden"]))]),
    );
    assert_eq!(
        lines(&narrowed),
        ["varta log: x-b=1, 2", "varta log: null null 0 1, 2"]
    );
    // A ceiling without a declaration admits only the safelisted names.
    let safelisted = run(&server, V38, requested, Some(vec![grant(&server, None)]));
    assert_eq!(
        lines(&safelisted),
        ["varta log: ", "varta log: null null 0 null"]
    );
}

/// V37 is frozen: every received field, set-cookie included, whatever a
/// grant (here a ceiling-shaped declaration) says.
#[test]
fn v37_programs_keep_every_received_header() {
    let server = Loopback::start();
    let report = run(&server, V37, vec![grant(&server, Some(&["x-a"]))], None);
    assert_eq!(
        lines(&report),
        [
            "varta log: cache-control=no-store;connection=close;content-length=1;content-type=text/plain;set-cookie=a=1;set-cookie=b=2;x-a=z;x-b=1, 2;x-hidden=secret",
            "varta log: secret a=1, b=2 2 1, 2",
        ]
    );
}

/// Fair fuel charges the head record the guest receives: an undeclared
/// field costs nothing, a declared one is charged.
#[test]
fn undeclared_header_bytes_are_never_charged() {
    let server = Loopback::start();
    let fuel = |path: &str, headers: Option<&[&str]>| {
        let source = format!(
            "export async function main(): Promise<void> {{
               const r = await fetch('{}{path}');
               console.log(String(r.status) + ' ' + String(r.headers.get('x-pad') !== null));
             }}",
            server.base
        );
        let report = VartaRuntime::new().run_with(
            &compile(&source, V38),
            RunOptions {
                requested: vec![grant(&server, headers)],
                ..RunOptions::default()
            },
        );
        let visible = lines(&report);
        (report.budget.unwrap().total_fuel, visible)
    };
    let (plain, plain_lines) = fuel("/pad0", None);
    let (padded, padded_lines) = fuel("/pad1", None);
    assert_eq!(plain_lines, ["varta log: 200 false"]);
    assert_eq!(padded_lines, plain_lines);
    assert_eq!(padded, plain, "an undeclared field was charged");
    let declared = Some(&["x-pad", "content-type"][..]);
    let (declared_plain, _) = fuel("/pad0", declared);
    let (declared_padded, declared_lines) = fuel("/pad1", declared);
    assert_eq!(declared_lines, ["varta log: 200 true"]);
    // The delivered field is charged per UTF-16 byte of its record text
    // (on top of the guest work of reading it).
    let record_bytes = (r#""x-pad","","#.len() + PAD_BYTES) * size_of::<u16>();
    assert!(
        declared_padded >= declared_plain + record_bytes as u64 * varta_common::fuel::BYTE,
        "{declared_padded} {declared_plain}"
    );
}
