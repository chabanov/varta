//! Explicit wall waits leave the scripted timer clock independent of I/O.
use super::*;
use crate::{ApplicationError, ApplicationOptions, RunOptions, RuntimeError};
use std::time::{Duration, Instant};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};
use varta_common::RequestScope;

fn network(base: &str) -> CapabilityGrant {
    CapabilityGrant {
        id: "net.request".into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: vec![format!("{base}/**")],
            methods: None,
            response_headers: None,
        }),
    }
}
const LOAD: &str = r#"
async function load(label:number):Promise<void>{
  try {
    const response=await fetch("__URL__");
    const text=await response.text();
    View.setText(label,text);View.commit();console.log("loaded "+text);
  }catch(error){console.log("denied");}
}
export function main():void{
  const root=View.node();View.createElement(root,1);View.setAttribute(root,1,"app");
  const label=View.node();View.createText(label,"pending");View.insert(root,label,0);
  View.on(root,1,(event:ViewEvent):void=>{console.log("clicked");},0,0,0,0);
  View.mount(root);load(label);
  setTimeout(()=>{console.log("virtual timer");},50);
}
"#;

/// A real transfer that either holds the head until EOF or supplies the body
/// later than the first scripted wait's bound. EOF proves request cleanup.
fn held_transfer(
    body_after: Option<Duration>,
) -> (
    String,
    mpsc::Receiver<std::io::Result<usize>>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (closed, eof) = mpsc::channel();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
            if line == "\r\n" {
                break;
            }
        }
        if let Some(delay) = body_after {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\n")
                .unwrap();
            thread::sleep(delay);
            let _ = stream.write_all(b"network!");
        }
        let mut byte = [0];
        closed.send(stream.read(&mut byte)).unwrap();
    });
    (base, eof, task)
}

#[test]
fn explicit_network_wait_settles_streamed_bodies_without_advancing_virtual_timers() {
    let server = crate::net_loopback::Loopback::start();
    let bytes = compile(&LOAD.replace("__URL__", &format!("{}/utf8", server.base)));
    let session = script(
        r##"{"inputs":[{"input":"wait_network","ms":1000},{"input":"click","target":"#app"},{"input":"wait","ms":50}]}"##,
    );
    let result = run_view(&bytes, &[network(&server.base), timers()], None, &session).unwrap();
    assert_eq!(
        result.output.lines,
        [
            "varta log: loaded é€😀",
            "varta log: clicked",
            "varta log: virtual timer"
        ]
    );
    assert!(result.snapshot.to_string().contains("é€😀"));
    assert!(turn_kinds(&result).contains(&crate::TurnKind::Network));
}

#[test]
fn network_wait_denials_are_recoverable_without_a_socket() {
    let server = crate::net_loopback::Loopback::start();
    let bytes = compile(&LOAD.replace("__URL__", &format!("{}/hello", server.base)));
    let session = script(
        r##"{"inputs":[{"input":"wait_network","ms":1000},{"input":"click","target":"#app"}]}"##,
    );
    let result = run_view(&bytes, &[timers()], None, &session).unwrap();
    assert!(result.output.lines[0].contains("<capability denied: net.request>"));
    assert_eq!(
        &result.output.lines[1..],
        ["varta log: denied", "varta log: clicked"]
    );
    assert_eq!(server.accepted(), 0);
}

#[test]
fn a_network_wait_times_out_without_running_later_inputs() {
    let (base, closed, worker) = held_transfer(None);
    let bytes = compile(&LOAD.replace("__URL__", &format!("{base}/slow")));
    let session = script(
        r##"{"inputs":[{"input":"wait_network","ms":100},{"input":"click","target":"#app"}]}"##,
    );
    let at = Instant::now();
    let result = VartaRuntime::new().run_view(
        &bytes,
        ApplicationOptions {
            run: RunOptions {
                requested: vec![network(&base), timers()],
                ..Default::default()
            },
            ..Default::default()
        },
        &session,
    );
    assert!(
        matches!(
            result.report.failure,
            Some(RuntimeError::Application(ApplicationError::NetworkWait {
                step: 1,
                ..
            }))
        ),
        "{:?}",
        result.report.failure
    );
    assert!(at.elapsed() < Duration::from_secs(2));
    assert!(result.report.output.lines.is_empty());
    assert_eq!(
        closed
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap(),
        0
    );
    worker.join().unwrap();
}

#[test]
fn a_completed_wait_does_not_shorten_a_response_used_by_a_later_input() {
    let first = crate::net_loopback::Loopback::start();
    let (second, closed, worker) = held_transfer(Some(Duration::from_millis(300)));
    let source=r#"
      async function readBody(response:Response,label:number):Promise<void>{
        const text=await response.text();View.setText(label,text);View.commit();console.log("body "+text);
      }
      async function install(root:number,label:number):Promise<void>{
        await fetch("__FIRST__/hello");
        const response=await fetch("__SECOND__/held");
        View.on(root,1,(event:ViewEvent):void=>{readBody(response,label);},0,0,0,0);
        View.commit();console.log("headers");
      }
      export function main():void{
        const root=View.node();View.createElement(root,1);View.setAttribute(root,1,"app");
        const label=View.node();View.createText(label,"pending");View.insert(root,label,0);
        View.mount(root);install(root,label);
      }
    "#.replace("__FIRST__",&first.base).replace("__SECOND__",&second);
    let session = script(
        r##"{"inputs":[{"input":"wait_network","ms":100},{"input":"click","target":"#app"},{"input":"wait_network","ms":1000}]}"##,
    );
    let mut grant = network(&first.base);
    if let CapabilityValue::Requests(scope) = &mut grant.value {
        scope.urls.push(format!("{second}/**"));
    }
    let result = run_view(&compile(&source), &[grant], None, &session).unwrap();
    assert_eq!(
        result.output.lines,
        ["varta log: headers", "varta log: body network!"]
    );
    assert!(result.snapshot.to_string().contains("network!"));
    assert_eq!(
        closed
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap(),
        0
    );
    worker.join().unwrap();
}

#[test]
fn network_wait_caps_a_running_continuation_without_raising_its_fuel_allowance() {
    let bytes = compile(
        r#"
      export function main():void{
        const root=View.node();View.createElement(root,1);View.setAttribute(root,1,"app");
        View.on(root,1,(event:ViewEvent):void=>{console.log("loop");while(true){}},0,0,0,0);
        View.mount(root);
      }
    "#,
    );
    let budgets = varta_common::budget::BudgetRequest {
        fuel_per_turn: Some(varta_common::budget::MAX_FUEL_PER_TURN),
        ..Default::default()
    };
    let at = Instant::now();
    let result = VartaRuntime::new().run_view(
        &bytes,
        ApplicationOptions {
            run: RunOptions {
                budgets,
                budget_ceiling: Some(budgets),
                ..Default::default()
            },
            ..Default::default()
        },
        &script(
            r##"{"inputs":[{"input":"click","target":"#app"},{"input":"wait_network","ms":25}]}"##,
        ),
    );
    assert!(
        matches!(
            result.report.failure,
            Some(RuntimeError::Application(ApplicationError::NetworkWait {
                step: 2,
                ..
            }))
        ),
        "{:?}",
        result.report.failure
    );
    assert_eq!(result.report.output.lines, ["varta log: loop"]);
    assert!(at.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_programmatically_invalid_network_wait_is_rejected_before_guest_start() {
    let session = crate::ViewScript {
        inputs: vec![varta_view::ScriptStep::WaitNetwork { ms: f64::NAN }],
        ..Default::default()
    };
    let result = VartaRuntime::new().run_view(
        &compile("export function main():void{console.log('started');}"),
        ApplicationOptions::default(),
        &session,
    );
    assert!(matches!(
        result.report.failure,
        Some(RuntimeError::ViewScript(_))
    ));
    assert!(result.report.output.lines.is_empty());
    assert!(result.snapshot.is_none());
}
