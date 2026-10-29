//! V35 `abort_signals`: the engine-provided `AbortController`, `AbortSignal`
//! and `DOMException` (a typed guest prelude compiled with the program only
//! when it names one of them). Expected logs of the `web_cases` programs were
//! produced by Node 24 (type stripping, `main()` appended) and are pinned here,
//! so the tests need no Node installation; Varta-specific results are marked.
use super::*;
use crate::web_headers_tests::{V35, module, rejected, run};
use varta_common::{exceptions::ThrownValue, language::LanguageProfile as P};

#[test]
fn abort_state_reasons_throw_if_aborted_and_dom_exceptions_match_node() {
    run(
        include_str!("web_cases/abort_basics.ts"),
        &[
            "false undefined running",
            "true DOMException AbortError This operation was aborted 20 thrown DOMException AbortError This operation was aborted 20 true",
            "true true",
            "string stop thrown string stop true",
            "DOMException AbortError This operation was aborted 20",
            "thrown Error boom true",
            "DOMException AbortError This operation was aborted 20",
            "other",
            "true",
            "AbortError: This operation was aborted true",
            "TimeoutError late 23 true",
            "[Error] [] 0",
        ],
    );
}

#[test]
fn signals_cancel_awaited_steps_and_reach_catch_as_unknown_like_node() {
    run(
        include_str!("web_cases/abort_flow.ts"),
        &[
            "done 20",
            "aborted after 6: This operation was aborted",
            "custom limit",
            "aborted 1 of 2",
        ],
    );
}

#[test]
fn web_type_language_guide_examples_match_node() {
    run(
        include_str!("web_cases/guide_headers.ts"),
        &[
            "text/plain, application/json",
            "30",
            "accept: text/plain, application/json",
            "content-type: application/json",
            "retry-after: 30",
            "application/json",
            r#"Headers.set: "bad name" is an invalid header name."#,
        ],
    );
    run(
        include_str!("web_cases/guide_abort.ts"),
        &["2", "true", "AbortError 20", "shutdown"],
    );
}

/// Varta-specific: an uncaught default reason is reported like a native error.
#[test]
fn uncaught_abort_reasons_report_their_name_and_message() {
    for (source, instance, display) in [
        (
            "export function main():void{const c=new AbortController();c.abort();c.signal.throwIfAborted();}",
            true,
            "AbortError: \"This operation was aborted\"",
        ),
        (
            "export function main():void{AbortSignal.abort(new TypeError('stop')).throwIfAborted();}",
            false,
            "TypeError: \"stop\"",
        ),
    ] {
        match VartaRuntime::new().run_wasm_bytes(&module(source), &[]) {
            Err(RuntimeError::Uncaught(exception)) => {
                // The default reason is a DOMException, an Error subclass instance.
                assert_eq!(
                    matches!(exception.value, ThrownValue::ErrorInstance { .. }),
                    instance,
                    "{source}"
                );
                assert_eq!(exception.value.to_string(), display, "{source}");
            }
            other => panic!("expected an uncaught exception, got {other:?}\n{source}"),
        }
    }
}

#[test]
fn pending_abort_members_fail_with_explicit_diagnostics() {
    let listeners = "abort event listeners";
    let timers = "AbortSignal.timeout and AbortSignal.any";
    for (source, expected) in [
        (
            "export function main():void{const s=new AbortController().signal;s.addEventListener('abort',()=>{});}",
            listeners,
        ),
        (
            "export function main():void{const s=new AbortController().signal;s.removeEventListener('abort',()=>{});}",
            listeners,
        ),
        (
            "export function main():void{const s=AbortSignal.abort();s.onabort=null;}",
            listeners,
        ),
        (
            "export function main():void{const s=AbortSignal.abort();console.log(String(s.onabort));}",
            listeners,
        ),
        (
            "export function main():void{AbortSignal.timeout(10);}",
            timers,
        ),
        (
            "export function main():void{AbortSignal.any([AbortSignal.abort()]);}",
            timers,
        ),
        (
            "export function main():void{const s=new AbortSignal();}",
            "the constructor is illegal",
        ),
        (
            "class Mine extends AbortSignal{} export function main():void{const s=new Mine();}",
            "the constructor is illegal",
        ),
        (
            "export function main():void{console.log(JSON.stringify(AbortSignal.abort()));}",
            "JSON.stringify of an engine Web type instance",
        ),
        (
            "export function main():void{const c=new AbortController();c.signal=AbortSignal.abort();}",
            "readonly class field signal",
        ),
        (
            "export function main():void{const s=AbortSignal.abort();s.aborted=false;}",
            "class getter aborted has no setter",
        ),
        (
            "export function main():void{const c=new AbortController();console.log(String(c.state));}",
            "class member state is private to AbortController",
        ),
    ] {
        let error = rejected(source, V35);
        assert!(error.contains(expected), "{source}: {error}");
    }
}

#[test]
fn abort_types_stay_rejected_before_v35() {
    for profile in [P::CheckedF64Utf16V34, P::CheckedF64Utf16V32, P::default()] {
        for (source, expected) in [
            (
                "function f(s:AbortSignal):void{} export function main():void{}",
                "AbortSignal",
            ),
            (
                "export function main():void{const c=new AbortController();}",
                "AbortController",
            ),
            (
                "export function main():void{const e=new DOMException('m');}",
                "DOMException",
            ),
        ] {
            let error = rejected(source, profile);
            assert!(error.contains(expected), "{profile:?} {source}: {error}");
        }
    }
}
