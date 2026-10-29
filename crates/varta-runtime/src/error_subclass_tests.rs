//! v32 `error_subclasses()`: `class X extends Error` and the native families.
//! Expected lines were checked against Node with the same sources (Varta logs
//! booleans as 0/1).
use super::*;
use varta_common::{exceptions::ThrownValue, language::LanguageProfile as P};

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn module(source: &str) -> Vec<u8> {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    let bytes = module(source);
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        lines(expected)
    );
}
fn uncaught(source: &str) -> ThrownValue {
    let bytes = module(source);
    match VartaRuntime::new().run_wasm_bytes(&bytes, &[]) {
        Err(RuntimeError::Uncaught(exception)) => exception.value,
        other => panic!("expected an uncaught exception, got {other:?}\n{source}"),
    }
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// The pilot `errors.ts` hierarchy with admitted field types (`cause` is a
/// string and `Headers`/`ProblemDocument` are omitted).
const HIERARCHY: &str = "
export class UarpError extends Error {
  constructor(message: string, options?: { cause?: string }) {
    super(message, options);
    this.name = new.target.name;
  }
}
export class APIConnectionError extends UarpError {
  constructor(message = 'Connection error', options?: { cause?: string }) {
    super(message, options);
  }
}
export class APITimeoutError extends APIConnectionError {
  constructor(message = 'Request timed out') {
    super(message);
  }
}
export class APIError extends UarpError {
  readonly status: number;
  readonly code: string | undefined;
  constructor(status: number, code: string | undefined, message?: string) {
    super(message ?? `HTTP ${status}`);
    this.status = status;
    this.code = code;
  }
  get retryable(): boolean {
    return this.status >= 500;
  }
}
export class BadRequestError extends APIError {}
export class InternalServerError extends APIError {}
export class ServiceUnavailableError extends InternalServerError {}
function errorForStatus(status: number): APIError {
  if (status === 400) return new BadRequestError(status, 'BAD');
  if (status === 503) return new ServiceUnavailableError(status, undefined);
  if (status >= 500) return new InternalServerError(status, undefined);
  return new APIError(status, undefined, 'custom');
}
function request(status: number): string {
  if (status >= 400) throw errorForStatus(status);
  return 'ok';
}
";

#[test]
fn pilot_error_hierarchy_names_messages_instanceof_and_catch() {
    let source = format!(
        "{HIERARCHY}
export function main(): void {{
  const t = new APITimeoutError();
  console.log(t.name);
  console.log(t.message);
  console.log(t.toString());
  console.log(t instanceof Error);
  console.log(t instanceof UarpError);
  console.log(t instanceof APIConnectionError);
  console.log(t instanceof APITimeoutError);
  console.log(t instanceof TypeError);
  const e = errorForStatus(503);
  console.log(e.name);
  console.log(e.message);
  console.log(e.status);
  console.log(e.retryable);
  console.log(e instanceof InternalServerError);
  console.log(e instanceof BadRequestError);
  const c = new APIConnectionError('down', {{ cause: 'socket' }});
  console.log(c.cause === 'socket');
  console.log(new APIConnectionError().cause === undefined);
  attempt(200);
  attempt(400);
  attempt(404);
  attempt(503);
}}
function attempt(status: number): void {{
  try {{
    console.log(request(status));
  }} catch (error) {{
    if (error instanceof APIError && error.status === 400) {{
      console.log('bad ' + error.name + ' ' + String(error.code));
    }} else if (error instanceof InternalServerError) {{
      console.log('server ' + error.toString());
    }} else if (error instanceof Error) {{
      console.log('other ' + error.message + ' ' + error.name);
    }}
  }} finally {{
    console.log('done ' + String(status));
  }}
}}"
    );
    run(
        &source,
        &[
            "APITimeoutError",
            "Request timed out",
            "APITimeoutError: Request timed out",
            "1",
            "1",
            "1",
            "1",
            "0",
            "ServiceUnavailableError",
            "HTTP 503",
            "503",
            "1",
            "1",
            "0",
            "1",
            "1",
            "ok",
            "done 200",
            "bad BadRequestError BAD",
            "done 400",
            "other custom APIError",
            "done 404",
            "server ServiceUnavailableError: HTTP 503",
            "done 503",
        ],
    );
}

/// The `docs/language-guide.md` example, verbatim.
#[test]
fn language_guide_error_subclass_example_runs() {
    run(
        r#"export class UarpError extends Error {
  constructor(message: string, options?: { cause?: string }) {
    super(message, options);
    this.name = new.target.name;
  }
}

export class APIConnectionError extends UarpError {
  constructor(message = 'Connection error', options?: { cause?: string }) {
    super(message, options);
  }
}

export class APIError extends UarpError {
  readonly status: number;
  constructor(status: number) {
    super(`HTTP ${status}`);
    this.status = status;
  }
}

class NotFoundError extends APIError {}

function fetchUser(id: number): string {
  if (id < 0) throw new APIConnectionError(undefined, { cause: 'socket closed' });
  throw new NotFoundError(404);
}

export function main(): void {
  try {
    fetchUser(7);
  } catch (error) {
    if (error instanceof APIError && error.status === 404) {
      console.log(error.toString()); // NotFoundError: HTTP 404
    }
  }
  try {
    fetchUser(-1);
  } catch (error) {
    console.log(error instanceof Error); // 1
    if (error instanceof APIConnectionError) {
      console.log(error.message); // Connection error
      console.log(error.cause === 'socket closed'); // 1
    }
  }
}"#,
        &["NotFoundError: HTTP 404", "1", "Connection error", "1"],
    );
}

#[test]
fn native_families_implicit_constructors_causes_and_mutation() {
    run(
        r#"
class Plain extends Error {}
class Range extends RangeError {}
class Bad extends TypeError {
  constructor(message: string) {
    super(message);
    this.name = 'Bad';
  }
}
class Wrapped extends Error {}
function fail(kind: number): void {
  if (kind === 0) throw new Plain('plain');
  if (kind === 1) throw new Range();
  if (kind === 2) throw new Bad('bad');
  if (kind === 3) throw new TypeError('native');
  throw 'text';
}
function wrap(kind: number): Wrapped {
  try {
    fail(kind);
  } catch (error) {
    return new Wrapped('wrapped', { cause: error });
  }
  return new Wrapped();
}
export function main(): void {
  const p = new Plain();
  console.log(p.name);
  console.log(p.message === '');
  console.log(p.toString());
  console.log(new Plain('m').toString());
  const r = new Range('r');
  console.log(r.name);
  console.log(r.toString());
  console.log(r instanceof RangeError);
  console.log(r instanceof TypeError);
  console.log(r instanceof Error);
  console.log(new Bad('b') instanceof TypeError);
  for (let kind = 0; kind < 5; kind++) {
    try {
      fail(kind);
    } catch (error) {
      console.log(error instanceof Error);
      console.log(error instanceof TypeError);
      console.log(error instanceof RangeError);
      console.log(error instanceof Plain);
      if (error instanceof Error) {
        console.log(error.name + '|' + error.message + '|' + error.toString());
      } else {
        console.log(typeof error);
      }
    }
  }
  const w = wrap(2);
  const cause = w.cause;
  console.log(cause instanceof Bad);
  console.log(cause instanceof TypeError);
  if (cause instanceof Bad) {
    console.log(cause.message);
  }
  const text = wrap(4).cause;
  console.log(typeof text === 'string');
  console.log(text === 'text');
  console.log(new Wrapped('x', { cause: 'c' }).cause === 'c');
  console.log(new Wrapped('x').cause === undefined);
  p.name = 'Renamed';
  p.message = 'changed';
  console.log(p.toString());
  p.name = '';
  console.log(p.toString());
  p.message = '';
  console.log(p.toString() === '');
}
"#,
        &[
            "Error",
            "1",
            "Error",
            "Error: m",
            "RangeError",
            "RangeError: r",
            "1",
            "0",
            "1",
            "1",
            "1",
            "0",
            "0",
            "1",
            "Error|plain|Error: plain",
            "1",
            "0",
            "1",
            "0",
            "RangeError||RangeError",
            "1",
            "1",
            "0",
            "0",
            "Bad|bad|Bad: bad",
            "1",
            "1",
            "0",
            "0",
            "TypeError|native|TypeError: native",
            "0",
            "0",
            "0",
            "0",
            "string",
            "1",
            "1",
            "bad",
            "1",
            "1",
            "1",
            "1",
            "Renamed: changed",
            "changed",
            "1",
        ],
    );
}

#[test]
fn new_target_dispatch_downcasts_and_propagation_through_finally() {
    run(
        r#"
class Base extends Error {
  readonly code: number;
  constructor(message: string, code: number) {
    super(message);
    this.code = code;
    this.name = new.target.name;
  }
  describe(): string {
    return this.name + '#' + String(this.code);
  }
}
class Middle extends Base {}
class Leaf extends Middle {
  constructor() {
    super('leaf', 7);
  }
  describe(): string {
    return 'leaf ' + super.describe();
  }
}
class Shape {
  label: string;
  constructor() {
    this.label = new.target.name;
  }
}
class Square extends Shape {}
function scaled(error: Base): number {
  if (error instanceof Leaf) {
    return error.code * 10;
  }
  return error.code;
}
function deep(depth: number): number {
  try {
    if (depth === 0) throw new Leaf();
    return deep(depth - 1);
  } finally {
    console.log('unwind ' + String(depth));
  }
}
function rethrow(): void {
  try {
    deep(2);
  } catch (error) {
    if (error instanceof Middle) {
      console.log('caught ' + error.describe());
      throw error;
    }
  }
}
export function main(): void {
  console.log(new Middle('m', 1).name);
  console.log(new Leaf().name);
  console.log(new Leaf().describe());
  console.log(new Square().label);
  console.log(new Shape().label);
  console.log(scaled(new Leaf()));
  console.log(scaled(new Middle('m', 2)));
  try {
    rethrow();
  } catch ({ name, message }) {
    console.log(String(name) + ':' + String(message));
  }
  try {
    throw new Middle('outer', 3);
  } catch (error) {
    try {
      throw new Base('inner', 4);
    } catch (inner) {
      if (inner instanceof Base && error instanceof Base) {
        console.log(inner.describe() + ' in ' + error.describe());
      }
    }
  }
  let same = new Leaf();
  try {
    throw same;
  } catch (error) {
    console.log(error === same);
  }
}
"#,
        &[
            "Middle",
            "Leaf",
            "leaf Leaf#7",
            "Square",
            "Shape",
            "70",
            "2",
            "unwind 0",
            "unwind 1",
            "unwind 2",
            "caught leaf Leaf#7",
            "Leaf:leaf",
            "Base#4 in Middle#3",
            "1",
        ],
    );
}

#[test]
fn uncaught_error_instances_report_current_name_and_message() {
    let cases: [(&str, &str, &str, &str); 4] = [
        (
            "class Failure extends Error{constructor(m:string){super(m);this.name=new.target.name;}}\
             class Timeout extends Failure{}\
             function go():void{throw new Timeout('slow \"net\"');}\
             export function main():void{try{go();}finally{console.log('cleanup');}}",
            "Timeout",
            "slow \"net\"",
            "Timeout: \"slow \\\"net\\\"\"",
        ),
        (
            "class Plain extends RangeError{}export function main():void{throw new Plain();}",
            "RangeError",
            "",
            "RangeError",
        ),
        (
            "class Quiet extends Error{constructor(){super('only');this.name='';}}\
             export function main():void{throw new Quiet();}",
            "",
            "only",
            "\"only\"",
        ),
        (
            "class Later extends Error{}\
             export function main():void{const e=new Later('m');e.name='Renamed';e.message='after';throw e;}",
            "Renamed",
            "after",
            "Renamed: \"after\"",
        ),
    ];
    for (source, name, message, display) in cases {
        let value = uncaught(source);
        assert_eq!(
            value,
            ThrownValue::ErrorInstance {
                name: units(name),
                message: units(message),
            },
            "{source}"
        );
        assert_eq!(value.to_string(), display, "{source}");
    }
}

#[test]
fn error_instances_flow_through_arrays_callbacks_closures_and_joins() {
    run(
        r#"
class Failure extends Error {
  readonly attempt: number;
  constructor(message: string, attempt: number) {
    super(message);
    this.attempt = attempt;
    this.name = new.target.name;
  }
}
class Retryable extends Failure {}
function collect(count: number): Failure[] {
  const out: Failure[] = [];
  for (let i = 0; i < count; i++) {
    try {
      if (i % 2 === 0) throw new Retryable('r' + String(i), i);
      throw new Failure('f' + String(i), i);
    } catch (error) {
      if (error instanceof Failure) out.push(error);
    }
  }
  return out;
}
export function main(): void {
  const failures = collect(4);
  const labels = failures.map((f) => f.toString());
  console.log(labels.reduce((acc, label) => (acc === '' ? label : acc + ',' + label), ''));
  console.log(failures.filter((f) => f instanceof Retryable).length);
  const first = failures[0];
  if (first !== undefined) {
    const describe = () => first.name + '@' + String(first.attempt);
    first.name = 'Changed';
    console.log(describe());
  }
  const pick = (flag: boolean) => (flag ? new Retryable('a', 1) : new Failure('b', 2));
  console.log(pick(true).name + pick(false).name);
}
"#,
        &[
            "Retryable: r0,Failure: f1,Retryable: r2,Failure: f3",
            "2",
            "Changed@0",
            "RetryableFailure",
        ],
    );
}

/// Run with an explicit test-only fuel budget so the churn forces collections.
fn run_collected(source: &str) -> Vec<String> {
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

#[test]
fn error_instances_and_causes_survive_collection_and_repeated_catches() {
    // Live instances are held only through causes, arrays and locals while
    // thousands of thrown and caught instances become garbage.
    let output = run_collected(
        r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<350;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
class Failure extends Error{detail:string;constructor(m:string,detail:string,options?:{cause?:string}){super(m,options);this.detail=detail;this.name=new.target.name+"!";}}
class Chained extends Error{}
function garbage(i:number):void{try{throw new Failure(i>0?"g"+"h":"k","d"+"e",{cause:"c"+"d"});}catch(e){if(e instanceof Failure){e.toString();}}}
function keep():Chained{try{throw new Failure("held"+"!","de"+"tail",{cause:"ro"+"ot"});}catch(e){return new Chained("wrap"+"per",{cause:e});}return new Chained();}
export function main():void{
 const held=keep();
 const list:Failure[]=[new Failure("a"+"1","x"+"y")];
 churn();for(let i=0;i<3000;i=i+1){garbage(i);}churn();
 console.log(held.toString());
 const inner=held.cause;
 if(inner instanceof Failure){console.log(inner.toString());console.log(inner.detail);const root=inner.cause;if(typeof root==="string"){console.log(root);}}
 const first=list[0];if(first!==undefined){console.log(first.name+first.message+first.detail);}
}
"#,
    );
    assert_eq!(
        output,
        lines(&[
            "Error: wrapper",
            "Failure!: held!",
            "detail",
            "root",
            "Failure!a1xy",
        ])
    );
}

#[test]
fn error_subclass_forms_fail_with_construct_diagnostics() {
    for (source, expected) in [
        (
            "class A extends Error{constructor(){super();const t=new.target;}}",
            "new.target",
        ),
        (
            "class A extends Error{label:string=new.target.name;}",
            "new.target",
        ),
        (
            "class A extends Error{constructor(){super();const f=()=>new.target.name;}}",
            "constructor closures cannot capture new.target",
        ),
        (
            "function f():string{return new.target.name;}",
            "new.target.name outside a class constructor",
        ),
        (
            "class A extends Error{m():string{return new.target.name;}}",
            "new.target.name outside a class constructor",
        ),
        (
            "class A extends Error{constructor(){super('a',undefined,1);}}",
            "super(...) of an Error subclass other than (message?, options?)",
        ),
        (
            "class A extends Error{constructor(){super(...['x']);}}",
            "super(...) of an Error subclass other than (message?, options?)",
        ),
        (
            "class A extends Error{constructor(){super(1);}}",
            "Error message must be string or undefined",
        ),
        (
            "class A extends Error{constructor(){super('m','x');}}",
            "Error options must be an object type or undefined",
        ),
        (
            "class A extends Error{constructor(o:{cause:number[]}){super('m',o);}}",
            "Error cause must be a primitive, BigInt or Error value",
        ),
        (
            "class A extends Error{constructor(o:{cause?:unknown}){super('m',o);}}",
            "unsupported type annotation",
        ),
        (
            "class A extends Error{name='A';}",
            "duplicate class member: name",
        ),
        (
            "class A extends Error{message:string='m';}",
            "duplicate class member: message",
        ),
        (
            "class A extends Error{cause():number{return 1;}}",
            "duplicate class member: cause",
        ),
        (
            "class A extends Error{}function f():string{return JSON.stringify(new A());}",
            "JSON.stringify of an Error subclass instance",
        ),
        (
            "class A extends Error{}function f():string{try{throw new A();}catch(e){return JSON.stringify(e);}}",
            "JSON.stringify of an Error subclass instance",
        ),
        (
            "class A extends Error{}function f():string{return new A().toString(1);}",
            "toString requires Error receiver and no arguments",
        ),
        (
            "class A extends Error{}function f():void{const e:Error=new A();}",
            "not a member of the expected union",
        ),
        (
            "class A extends Error{}function f():string{const x:A|TypeError=new A();return String(x.cause);}",
            "property other than name/message",
        ),
        (
            "class A extends Error{}function f():void{new A().stack;}",
            "unknown class member: stack",
        ),
        (
            "class A extends Error{}function f():boolean{return Object.hasOwn(new A(),'message');}",
            "Object.hasOwn on a non-record value",
        ),
    ] {
        let source = format!("{source}export function main():void{{}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}

#[test]
fn error_subclasses_stay_rejected_before_v32() {
    for (source, expected) in [
        (
            "class A extends Error{}export function main():void{}",
            "unsupported top-level declaration",
        ),
        (
            "export class A extends TypeError{constructor(){super('m');this.name=new.target.name;}}export function main():void{}",
            "unsupported top-level declaration",
        ),
        (
            "function f():string{return new.target.name;}export function main():void{}",
            "unsupported expression",
        ),
    ] {
        for profile in [P::CheckedF64Utf16V31, P::CheckedF64Utf16V30] {
            let error = rejected(source, profile);
            assert!(error.contains(expected), "{profile:?} {source}\n{error}");
        }
    }
    // A v31 module keeps the channel-v4 selector; v32 exports only v5.
    let v31 = compile(
        "export function main():void{try{throw new TypeError('t');}catch(e){}}",
        P::CheckedF64Utf16V31,
    )
    .unwrap();
    let v32 = module("export function main():void{try{throw new TypeError('t');}catch(e){}}");
    let exports = |bytes: &[u8]| -> Vec<String> {
        wasmtime::Module::new(&VartaRuntime::new().engine, bytes)
            .unwrap()
            .exports()
            .map(|e| e.name().to_owned())
            .filter(|name| name.starts_with(varta_common::exceptions::PREFIX))
            .collect()
    };
    let (old, new) = (exports(&v31), exports(&v32));
    assert!(
        old.iter()
            .any(|n| n == varta_common::exceptions::BIGINT_VERSION)
    );
    assert!(
        !old.iter()
            .any(|n| n == varta_common::exceptions::CLASS_ERROR_VERSION)
    );
    assert!(
        new.iter()
            .any(|n| n == varta_common::exceptions::CLASS_ERROR_VERSION)
    );
    assert!(
        !new.iter()
            .any(|n| n == varta_common::exceptions::BIGINT_VERSION)
    );
}

#[test]
fn recoverable_json_errors_publish_through_channel_v5() {
    // The JSON host publishes native errors after checking the v5 selector.
    run(
        "class Wrapped extends Error{}\
         function read(text:string):number{return varta.json.parse<{n:number}>(text).n;}\
         export function main():void{\
          try{read('{');}catch(e){if(e instanceof SyntaxError){console.log(e.name);}\
           const w=new Wrapped('wrapped',{cause:e});console.log(w.cause instanceof SyntaxError);}\
          try{read('{\"n\":\"x\"}');}catch(e){console.log(e instanceof TypeError);console.log(e instanceof Wrapped);}\
          console.log(read('{\"n\":2}'));}",
        &["SyntaxError", "1", "1", "0", "2"],
    );
}

#[test]
fn error_instance_channel_requires_the_v5_selector() {
    use crate::contract_tests::rewrite;
    use varta_common::exceptions as channel;
    let bytes = module("class A extends Error{}export function main():void{throw new A('m');}");
    // Renaming the v5 selector to the (equal-length) v4 name must fail before
    // any guest effect: validation binds the selector to the v32 profile.
    assert_eq!(
        channel::CLASS_ERROR_VERSION.len(),
        channel::BIGINT_VERSION.len()
    );
    let renamed = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7
            && let Some(at) = data
                .windows(channel::CLASS_ERROR_VERSION.len())
                .position(|w| w == channel::CLASS_ERROR_VERSION.as_bytes())
        {
            data[at..at + channel::BIGINT_VERSION.len()]
                .copy_from_slice(channel::BIGINT_VERSION.as_bytes());
        }
        vec![data]
    });
    assert_ne!(renamed, bytes);
    let error = VartaRuntime::new().validate_module(&renamed).unwrap_err();
    assert!(error.to_string().contains("exception channel"), "{error}");
    // Without validation, the v4 name carrying value 5 still fails initialization.
    let runtime = VartaRuntime::new();
    let unchecked = wasmtime::Module::new(&runtime.engine, &renamed).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let error = execution.instantiate(&unchecked).err().unwrap();
    assert!(
        format!("{error:?}").contains("invalid Error channel version"),
        "{error:?}"
    );
}
