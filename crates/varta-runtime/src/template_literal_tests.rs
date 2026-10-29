//! Template literals (v32). Expected lines were checked against Node.js; booleans
//! are never logged directly because `console.log` prints them as 0/1.
use super::*;
use varta_common::{abi, language::LanguageProfile as P};

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn execute(source: &str) -> Result<Vec<String>, RuntimeError> {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .map(|output| output.lines)
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        execute(source).unwrap_or_else(|e| panic!("{e:?}\n{source}")),
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}

#[test]
fn template_literals_convert_every_admitted_primitive_kind() {
    run(
        r#"function show(v:number|boolean|string|null|undefined):string{return `<${v}>`;}
function big(v:bigint|string|undefined):string{return `[${v}]`;}
function quiet():void{}
export function main():void{const z=-0;const yes=true;const b=12345678901234567890n;console.log(`a${1}b${2.5}c`);console.log(`${z}|${1e21}|${Number.MIN_VALUE}|${NaN}|${-Infinity}|${0.1+0.2}|${123456789012345680000}`);console.log(`${yes}/${false}/${null}/${undefined}/${quiet()}`);console.log(`${b}|${-5n}|${0n}`);console.log(show(-0)+show(true)+show(null)+show(undefined)+show('s')+show(7));console.log(big(10n)+big('t')+big(undefined));console.log(``.length);console.log(`${''}`.length);console.log(`plain`);const name='x';console.log(`${name}`);console.log(`${`nested ${1+1}`}!`);console.log(`${1}${2}${3}`);}"#,
        &[
            "a1b2.5c",
            "0|1e+21|5e-324|NaN|-Infinity|0.30000000000000004|123456789012345680000",
            "true/false/null/undefined/undefined",
            "12345678901234567890|-5|0",
            "<0><true><null><undefined><s><7>",
            "[10][t][undefined]",
            "0",
            "0",
            "plain",
            "x",
            "nested 2!",
            "123",
        ],
    );
}
#[test]
fn template_literal_language_guide_example_runs() {
    run(
        "type Options = { userAgent: string };\
         function activatePath(agentId: number): string { return `/api/v1/agents/${String(agentId)}/activate`; }\
         export function main(): void { const defaults: Options = { userAgent: `varta/${0.5}` };\
         console.log(activatePath(42)); console.log(`${defaults.userAgent} ${undefined} ${10n ** 20n}`); }",
        &[
            "/api/v1/agents/42/activate",
            "varta/0.5 undefined 100000000000000000000",
        ],
    );
}
#[test]
fn template_literal_parts_keep_utf16_escapes_and_lone_surrogates_exact() {
    run(
        r#"export function main():void{const lone='\uDC00';const t=`\uD800${lone}\uDFFF`;console.log(t.length);console.log(t.charCodeAt(0));console.log(t.charCodeAt(1));console.log(t.charCodeAt(2));const e=`\u{1F600}\n\t\`\${x}\\\x41\0`;console.log(e.length);console.log(e.charCodeAt(0));console.log(e.charCodeAt(1));console.log(e.charCodeAt(2));console.log(e.charCodeAt(3));console.log(e.substring(4));const c=`a\
b`;console.log(c);const m=`x
y`;console.log(m.length);console.log(m.charCodeAt(1));const s='\uD83D';console.log(`${s}\uDE00`==='😀'?'pair':'split');}"#,
        &[
            "3",
            "55296",
            "56320",
            "57343",
            "12",
            "55357",
            "56832",
            "10",
            "9",
            "`${x}\\A\0",
            "ab",
            "3",
            "10",
            "pair",
        ],
    );
    // A raw CR LF or CR line terminator in a template part cooks to LF.
    run(
        "export function main():void{const m=`x\r\ny\rz`;console.log(m.length);console.log(m.charCodeAt(1));console.log(m.charCodeAt(3));}",
        &["5", "10", "10"],
    );
}
#[test]
fn template_substitutions_evaluate_once_in_order_and_stop_at_a_throw() {
    run(
        r#"type Trace={t:string};
function tick(sink:Trace,name:string,v:number):number{sink.t=sink.t+name;return v;}
function fail():string{throw RangeError('boom');}
function drop(v:string|undefined):string{if(v!==undefined){return `${v}${v=undefined}${v}`;}return 'none';}
export function main():void{const trace:Trace={t:''};let i=0;console.log(`${i++}-${i++}-${i}`);console.log(`${tick(trace,'a',1)}${tick(trace,'b',2)}${tick(trace,'c',3)}`);console.log(trace.t);trace.t='';try{console.log(`${tick(trace,'x',1)}${fail()}${tick(trace,'y',2)}`);}catch(e){if(e instanceof RangeError){console.log(e.message);}}console.log(trace.t);let s='a';console.log(`${s}${s=s+'b'}${s}`);console.log(drop('x'));const prefix='/api';const path=(id:number):string=>`${prefix}/agents/${id}/activate`;console.log(path(7));}"#,
        &[
            "0-1-2",
            "123",
            "abc",
            "boom",
            "x",
            "aabab",
            "xundefinedundefined",
            "/api/agents/7/activate",
        ],
    );
}
#[test]
fn template_literals_are_strings_in_every_value_context() {
    run(
        r#"type Opt={userAgent:string};
function url(base:string,id:number,suffix=`activate`):string{return `${base}/api/v1/agents/${String(id)}/${suffix}`;}
export function main():void{const d:Opt={userAgent:`varta/${1}`};const o:Opt={userAgent:'pilot'};console.log(`${d.userAgent} ${o.userAgent}`);console.log(url('https://x',42));console.log(url('',1,`off${0}`));const parts=[`a${1}`,`b`];console.log(JSON.stringify(parts));console.log(`k${1}`===`k1`?'eq':'ne');if(`v${2}`!=='v2'){console.log('ne');}else{console.log('if');}console.log(`abc${'def'}`.length);console.log(`${1}`+`${2}`);console.log(JSON.stringify({p:`${d.userAgent}!`}));let acc='';for(let i=0;i<3;i++){acc=`${acc}${i},`;}console.log(acc);}"#,
        &[
            "varta/1 pilot",
            "https://x/api/v1/agents/42/activate",
            "/api/v1/agents/1/off0",
            "[\"a1\",\"b\"]",
            "eq",
            "if",
            "6",
            "12",
            "{\"p\":\"varta/1!\"}",
            "0,1,2,",
        ],
    );
}
#[test]
fn template_literals_keep_roots_across_collection_and_the_per_value_limit() {
    // Each iteration allocates enough garbage to force collection while the
    // already joined prefix is held only by the concatenation roots.
    // About 20 MiB of garbage against the fixed memory limit; only the fuel
    // budget is raised, explicitly and for this test.
    let source = "function junk(n:number):string{let g='x';for(let k=0;k<14;k++){g=`${g}${g}`;}for(let k=0;k<8;k++){const t=`${g}${k}`;}return `j${n}`;}\
         export function main():void{let last='';for(let i=0;i<64;i++){const head=`h${i}`;last=`${head}-${junk(i)}-${i*0.5}|${head}`;}console.log(last);}";
    assert_eq!(fueled(source).unwrap(), ["varta log: h63-j63-31.5|h63"]);
    // UTF-16 code units are two bytes: two halves reach the 1 MiB value limit.
    let half = abi::MAX_STRING_BYTES / 4;
    let exact = format!(
        "export function main():void{{const h='X'.repeat({half});console.log(`${{h}}${{h}}`.length);}}"
    );
    assert_eq!(
        fueled(&exact).unwrap(),
        [format!("varta log: {}", half * 2)]
    );
    let over = format!(
        "export function main():void{{const h='X'.repeat({half});console.log(`${{h}}${{h}}!`.length);}}"
    );
    let error = fueled(&over).unwrap_err();
    assert!(format!("{error:?}").contains("unreachable"), "{error:?}");
}
/// Run with production memory limits and an explicit test-only fuel budget.
fn fueled(source: &str) -> Result<Vec<String>, wasmtime::Error> {
    let runtime = VartaRuntime::new();
    let module = runtime
        .validate_module(&compile(source, P::CheckedF64Utf16V32).unwrap())
        .unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(300 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())?;
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    Ok(execution.store.data().output.lines.clone())
}
#[test]
fn template_literals_reject_object_substitutions_and_tagged_templates() {
    for (source, expected) in [
        (
            "export function main():void{const r={a:1};console.log(`${r}`);}",
            "template literal substitution requires a primitive value",
        ),
        (
            "export function main():void{const a=[1];console.log(`x${a}`);}",
            "template literal substitution requires a primitive value",
        ),
        (
            "export function main():void{const f=():number=>1;console.log(`${f}`);}",
            "template literal substitution requires a primitive value",
        ),
        (
            "export function main():void{console.log(`${RangeError('e')}`);}",
            "template literal substitution requires a primitive value",
        ),
        (
            "function g(v:{a:number}|string):string{return `${v}`;}export function main():void{}",
            "template literal substitution requires a primitive value",
        ),
        (
            "export function main():void{console.log(String.raw`a\\n`);}",
            "unsupported tagged template literal",
        ),
        (
            "function tag(s:string[]):string{return '';}export function main():void{console.log(tag`a${1}`);}",
            "unsupported tagged template literal",
        ),
    ] {
        let error = rejected(source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
    // The diagnostic points at the offending substitution, not the template.
    let source = "export function main():void{const r={a:1};console.log(`ok ${1} ${r}`);}";
    let error = compile(source, P::CheckedF64Utf16V32).unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
    assert_eq!(
        diagnostic.range.start.byte_offset,
        source.find("r}`").unwrap()
    );
}
#[test]
fn template_literals_stay_rejected_before_v32() {
    for source in [
        "export function main():void{console.log(`a`);}",
        "export function main():void{console.log(`a${1}b`);}",
        "export function main():void{console.log(String.raw`a`);}",
    ] {
        for profile in [
            P::CheckedF64Utf16V31,
            P::CheckedF64Utf16V19,
            P::CheckedI32Utf8V1,
        ] {
            let error = rejected(source, profile);
            assert!(error.contains("unsupported expression"), "{error}");
        }
        compile(
            source.replace("String.raw", "").as_str(),
            P::CheckedF64Utf16V32,
        )
        .unwrap();
    }
    // An unannotated parameter default infers `string` only under v32.
    let source =
        "function f(s=`a`):string{return s;}export function main():void{console.log(f());}";
    let error = rejected(source, P::CheckedF64Utf16V31);
    assert!(error.contains("parameter needs an annotation"), "{error}");
    run(source, &["a"]);
}
