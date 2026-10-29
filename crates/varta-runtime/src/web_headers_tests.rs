//! V35 `web_headers`: the engine-provided `Headers` class (a typed guest
//! prelude compiled with the program only when it names `Headers` or
//! `HeadersInit`). Expected logs of the `web_cases` programs were produced by
//! Node 24 (type stripping, `main()` appended) and are pinned here, so the
//! tests need no Node installation (differential oracle).
use super::*;
use varta_common::{HostOperation as H, language::LanguageProfile as P};

pub(crate) const V35: P = P::CheckedF64Utf16V35;

pub(crate) fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
pub(crate) fn module(source: &str) -> Vec<u8> {
    let bytes = compile(source, V35).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
pub(crate) fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&module(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>(),
        "{source}"
    );
}
pub(crate) fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
/// Run with an explicit test-only fuel budget so the churn forces collections.
pub(crate) fn run_collected(source: &str) -> Vec<String> {
    let bytes = module(source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(400 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    execution.store.data().output.lines.clone()
}
/// Fuel and host calls a program consumes under the production budgets.
pub(crate) fn cost(source: &str) -> (u64, u32) {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&module(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start
        .call(&mut *store, &[], &mut [])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::scheduler::checkpoint(&instance, store).unwrap();
    (
        abi::FUEL - store.get_fuel().unwrap(),
        store.data().host_calls,
    )
}
/// A typical request's header work stays well inside the production fuel and
/// host-call budgets: header validation, normalization and lowercasing are
/// guest scans, and the sorted header chain is never re-sorted.
#[test]
fn headers_typical_workload_stays_within_production_budgets() {
    run(
        include_str!("web_cases/headers_workload.ts"),
        &[
            "30 true false",
            "accept,authorization,cache-control,content-length,content-type,retry-after,set-cookie,set-cookie,user-agent,x-request-id",
            "10",
        ],
    );
    // The same header operations without the program's own logging and
    // number conversions.
    let source = "export function main():void{\
        const h=new Headers({'Content-Type':'application/json; charset=utf-8',Authorization:'Bearer token-123',\
        'User-Agent':'uarp-sdk/0.12.0',Accept:'application/json'});\
        h.append('X-Request-Id','req-42');h.append('Retry-After',' 30 ');h.append('Set-Cookie','a=1');\
        h.append('Set-Cookie','b=2');h.set('Cache-Control','no-store');h.append('Content-Length','120');\
        h.append('Accept-Encoding','gzip');h.delete('accept-encoding');h.get('retry-after');h.has('authorization');\
        let names='';for(const [name] of h){names=names+name!;}let count=0;h.forEach(()=>{count++;});}";
    let (empty, none) = cost("export function main():void{}");
    let (fuel, host_calls) = cost(source);
    // One host call: `Array.isArray` tells a record from a sequence.
    assert_eq!((none, host_calls), (0, 1));
    // About a tenth of the production fuel (measured: 155,000 with 17,000
    // for an empty program).
    assert!(fuel - empty < abi::FUEL / 5, "{fuel}");
}
/// The host operations a module imports.
pub(crate) fn imports(bytes: &[u8]) -> Vec<H> {
    let module = wasmtime::Module::new(&VartaRuntime::new().engine, bytes).unwrap();
    module
        .imports()
        .map(|i| H::from_name(i.name()).unwrap())
        .collect()
}

#[test]
fn headers_construction_validation_iteration_and_for_each_match_node() {
    run(
        include_str!("web_cases/headers_basics.ts"),
        &[
            "[a=1, 3][b=2][set-cookie=x=1][set-cookie=y=2]",
            r#"1, 3 x=1, y=2 ["x=1","y=2"]"#,
            r#"name: TypeError: "Headers.append: \"a b\" is an invalid header name.""#,
            r#"value: TypeError: "Headers.append: \"x\ny\" is an invalid header value.""#,
            r#"bytes: TypeError: "Cannot convert argument to a ByteString because the character at index 0 has a value of 8364 which is greater than 255.""#,
            r#"pair: TypeError: "Headers constructor: expected name/value pair to be length 2, found 1.""#,
            r#"empty: TypeError: "Headers.append: \"\" is an invalid header name.""#,
            r#"get: TypeError: "Headers.get: \"a b\" is an invalid header name.""#,
            r#"has: TypeError: "Headers.has: \"é\" is an invalid header name.""#,
            r#"delete: TypeError: "Headers.delete: \"(\" is an invalid header name.""#,
            r#"[set-cookie][set-cookie][x] [a=1][b=2][2] ["a=1","b=2"]"#,
            "[set-cookie=c=3][x=2]",
            "[x=2] false null",
            "a 2 true",
            "c 3 true",
            "d 4 true",
            "2",
            "3",
            "4",
            "[a=2][c=3][d=4] [a=2][c=3][d=4][e=5] true",
            "a: 2",
            "c: 3",
            "d: 4",
            "e: 5",
        ],
    );
}

#[test]
fn headers_byte_strings_tokens_normalization_and_conversion_order_match_node() {
    let byte_string = |index: u32, unit: u32| {
        format!(
            r#""Cannot convert argument to a ByteString because the character at index {index} has a value of {unit} which is greater than 255.""#
        )
    };
    let expected = [
        format!("bytes first: {}", byte_string(0, 8364)),
        format!("name bytes: {}", byte_string(0, 8364)),
        r#"nul: "Headers.append: \"x\u0000\" is an invalid header value.""#.into(),
        format!("late unit: {}", byte_string(1, 257)),
        r#"set name: "Headers.set: \"a b\" is an invalid header name.""#.into(),
        r#"set value: "Headers.set: \"\u0000\" is an invalid header value.""#.into(),
        r#"normalized message: "Headers.append: \"x\u0000\" is an invalid header value.""#.into(),
        r#"name before value: "Headers.append: \"a b\" is an invalid header name.""#.into(),
        r#"pair before name: "Headers constructor: expected name/value pair to be length 2, found 1.""#.into(),
        r#"name before pair: "Headers.append: \"x y\" is an invalid header name.""#.into(),
        format!("conversion first: {}", byte_string(0, 8364)),
        format!("record conversion first: {}", byte_string(0, 8364)),
        r#"hole pair: "Headers constructor: init[0] (undefined) is not iterable.""#.into(),
        "empty sequence: ok".into(),
        "token symbols: ok".into(),
        "\"x, x\\ty, \\u000b, \u{a0}x\u{a0}, , \"".into(),
        "true false ok".into(),
        r#"[a="undefined"]"#.into(),
        r#"[1="one"][2="two"][a="upper, lower"][b="bee"]"#.into(),
    ];
    run(
        include_str!("web_cases/headers_validation.ts"),
        &expected.iter().map(String::as_str).collect::<Vec<_>>(),
    );
}

#[test]
fn headers_are_nominal_values_in_fields_unions_unknown_closures_and_frames() {
    run(
        include_str!("web_cases/headers_values.ts"),
        &[
            "30",
            "undefined",
            "undefined",
            "headers a1",
            "signal false",
            "controller false",
            "string",
            "union true",
            "live live",
            "1",
            "1, 2",
            "retry-after:30",
            "x-call:1, 2",
            "x-id:a1",
            "30 true",
            "aborted",
            "ab 0 true",
        ],
    );
}

#[test]
fn headers_and_signals_survive_collection_with_exact_tracing() {
    assert_eq!(
        run_collected(include_str!("web_cases/web_collect.ts")),
        [
            "[accept=a/b][content-type=text/plain]",
            "[accept=a/b][content-type=text/plain][x-copy=cd]",
            "1 true",
            "kept!",
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

/// Callbacks may declare fewer parameters than their function type (v35).
#[test]
fn function_literals_may_omit_trailing_context_parameters_from_v35() {
    let source = "function each(f:(value:string,key:string,index:number)=>void):void{f('v','k',2);}\
        export function main():void{each((value)=>console.log(value));\
        each(function(value:string,key:string):void{console.log(value+key);});\
        each(()=>console.log('none'));each((v,k,i)=>console.log(v+k+String(i)));}";
    run(source, &["v", "vk", "none", "vk2"]);
    // Earlier profiles keep the exact count, and extra parameters stay errors.
    assert!(rejected(source, P::CheckedF64Utf16V34).contains("function parameter count mismatch"));
    let extra = "function each(f:(value:string)=>void):void{f('v');}\
        export function main():void{each((a,b)=>console.log(a));}";
    assert!(rejected(extra, V35).contains("function parameter count mismatch"));
}

#[test]
fn programs_that_do_not_name_web_types_are_unchanged() {
    // Without a Web type name no prelude is compiled: the module equals the v34
    // module apart from the declared profile name.
    let source = "class Box{value:string;constructor(value:string){this.value=value;}}\
        export function main():void{const o={Headers:'x'};const b=new Box(o.Headers+'Headers');\
        console.log(b.value);const list=[1,2,3];list.forEach((n)=>console.log(String(n)));}";
    let v35 = module(source);
    let v34 = compile(source, P::CheckedF64Utf16V34).unwrap();
    let rename = |bytes: &[u8]| {
        let (from, to) = (b"checked-f64-utf16-v35", b"checked-f64-utf16-v34");
        let at = bytes
            .windows(from.len())
            .position(|window| window == from)
            .unwrap();
        let mut bytes = bytes.to_vec();
        bytes[at..at + to.len()].copy_from_slice(to);
        bytes
    };
    assert_eq!(rename(&v35), v34);
    // Naming AbortSignal compiles the abort prelude only.
    let abort = module(
        "function f(s:AbortSignal):boolean{return s.aborted;} export function main():void{}",
    );
    // UTF-16 profiles store static strings as UTF-16LE.
    let contains = |bytes: &[u8], text: &str| {
        let units: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        bytes.windows(units.len()).any(|window| window == units)
    };
    let headers = module("export function main():void{new Headers();}");
    for (bytes, header, aborted) in [
        (&abort, false, true),
        (&headers, true, false),
        (&v35, false, false),
    ] {
        assert_eq!(contains(bytes, "invalid header name"), header);
        assert_eq!(contains(bytes, "This operation was aborted"), aborted);
    }
}

#[test]
fn web_types_need_no_capability_and_add_no_host_import() {
    for source in [
        include_str!("web_cases/headers_basics.ts"),
        include_str!("web_cases/abort_basics.ts"),
    ] {
        for operation in imports(&module(source)) {
            assert_eq!(operation.capability(), None, "{operation:?}");
            assert!(operation.minimum_abi() <= V35.host_abi(), "{operation:?}");
        }
    }
}

#[test]
fn headers_unsupported_forms_fail_with_explicit_diagnostics() {
    for (source, expected) in [
        (
            "export function main():void{console.log(JSON.stringify(new Headers()));}",
            "JSON.stringify of an engine Web type instance",
        ),
        (
            "export function main():void{const x:{h:Headers}={h:new Headers()};console.log(JSON.stringify(x));}",
            "JSON.stringify of an engine Web type instance",
        ),
        (
            "export function main():void{const x:unknown=1;const h=new Headers();console.log(JSON.stringify(x));}",
            "JSON.stringify of an engine Web type instance",
        ),
        (
            "export function main():void{const h=new Headers();console.log(String(h.first===null));}",
            "class member first is private to Headers",
        ),
        (
            "export function main():void{const pairs:[string,string][]=[];new Headers(pairs);}",
            "unsupported type annotation",
        ),
        (
            "export function main():void{new Headers().forEach((v)=>{},null);}",
            "class method expects 1 arguments",
        ),
        (
            "export function main():void{new Headers(new Headers().keys());}",
            "not a member of the expected union",
        ),
        // The prelude intrinsics do not exist in program modules.
        (
            "export function main():void{const h=new Headers();console.log(varta.web.asciiLowercase('A'));}",
            "unsupported call",
        ),
        (
            "export function main():void{for(const x of new AbortController()){}}",
            "for-of requires a checked array",
        ),
        (
            "export function main():void{const i=new Headers().entries();i.next();}",
            "next requires a generator receiver",
        ),
    ] {
        let error = rejected(source, V35);
        assert!(error.contains(expected), "{source}: {error}");
    }
}

#[test]
fn headers_stay_rejected_before_v35() {
    for profile in [P::CheckedF64Utf16V34, P::CheckedF64Utf16V32, P::default()] {
        for (source, expected) in [
            (
                "function f(h:Headers):void{} export function main():void{}",
                "Headers",
            ),
            (
                "export function main():void{const h=new Headers();}",
                "Headers",
            ),
            (
                "function f(h:HeadersInit):void{} export function main():void{}",
                "HeadersInit",
            ),
        ] {
            let error = rejected(source, profile);
            assert!(error.contains(expected), "{profile:?} {source}: {error}");
        }
    }
}
