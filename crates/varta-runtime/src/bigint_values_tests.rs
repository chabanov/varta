use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn bigint_values_coercion_width_accepts_primitive_toindex_and_tobigint() {
    run(
        "export function main():void{console.log(BigInt.asIntN('8','255'));console.log(BigInt.asUintN(true,true));console.log(BigInt.asUintN(false,'999'));console.log(BigInt.asIntN(undefined,'9'));console.log(BigInt.asUintN(null,'9'));console.log(BigInt.asIntN('bad','9'));console.log(BigInt.asIntN(-0.5,1n));console.log(BigInt.asIntN(8.9,'0xff'));console.log(BigInt.asUintN('0x8',-1n));}",
        &["-1", "1", "0", "0", "0", "0", "0", "-1", "255"],
    );
}
#[test]
fn bigint_values_coercion_width_errors_follow_conversion_order_and_missing_args() {
    run(
        "export function main():void{try{BigInt.asIntN(-1,'bad');}catch(e){console.log(e instanceof RangeError);}try{BigInt.asUintN(Infinity,1);}catch(e){console.log(e instanceof RangeError);}try{BigInt.asIntN(1n,'bad');}catch(e){console.log(e instanceof TypeError);}try{BigInt.asIntN(8,'bad');}catch(e){console.log(e instanceof SyntaxError);}try{BigInt.asIntN(0,1);}catch(e){console.log(e instanceof TypeError);}try{BigInt.asIntN();}catch(e){console.log(e instanceof TypeError);}try{BigInt.asUintN(8);}catch(e){console.log(e instanceof TypeError);}try{BigInt.asIntN(8,null);}catch(e){console.log(e instanceof TypeError);}console.log(BigInt(1));}",
        &["1", "1", "1", "1", "1", "1", "1", "1", "1"],
    );
}
#[test]
fn bigint_values_coercion_radix_accepts_numeric_strings_and_undefined_default() {
    run(
        "export function main():void{console.log((255n).toString('16'));console.log((255n).toString(undefined));console.log((255n).toString());console.log((255n).toString('2.9'));try{(1n).toString(2n);}catch(e){console.log(e instanceof TypeError);}try{(1n).toString(null);}catch(e){console.log(e instanceof RangeError);}try{(1n).toString(true);}catch(e){console.log(e instanceof RangeError);}try{(1n).toString('bad');}catch(e){console.log(e instanceof RangeError);}try{(1n).toString(Infinity);}catch(e){console.log(e instanceof RangeError);}}",
        &["ff", "255", "255", "11111111", "1", "1", "1", "1", "1"],
    );
}
#[test]
fn bigint_values_coercion_caches_source_arguments_before_any_semantic_error() {
    run(
        "function width():string{console.log('width');return '-1';}function value():string{console.log('value');return 'bad';}function receiver():bigint{console.log('receiver');return 255n;}function radix():string{console.log('radix');return 'bad';}function fail():number{throw SyntaxError('ignored');}export function main():void{try{BigInt.asIntN(width(),value(),console.log('ignored'));}catch(e){console.log(e instanceof RangeError);}try{receiver().toString(radix(),console.log('ignored'));}catch(e){console.log(e instanceof RangeError);}try{BigInt.asIntN(-1,'bad',fail());}catch(e){console.log(e instanceof SyntaxError);}}",
        &[
            "width", "value", "ignored", "1", "receiver", "radix", "ignored", "1", "1",
        ],
    );
}
#[test]
fn bigint_values_coercion_dispatches_union_members_and_preserves_v28() {
    run(
        "function width(n:bigint|number|string|boolean|null|undefined):bigint{return BigInt.asIntN(n,'255');}function text(n:bigint|number|string|boolean|null|undefined):string{return (255n).toString(n);}export function main():void{console.log(width('8'));console.log(width(true));console.log(width(undefined));console.log(text('16'));console.log(text(undefined));try{console.log(width(1n));}catch(e){console.log(e instanceof TypeError);}try{console.log(text(1n));}catch(e){console.log(e instanceof TypeError);}}",
        &["-1", "-1", "0", "ff", "255", "1", "1"],
    );
    for source in [
        "export function main():void{BigInt.asIntN('8','255');}",
        "export function main():void{console.log((255n).toString('16'));}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err()
        );
    }
}
#[test]
fn bigint_values_updates_preserve_prefix_postfix_results_and_exact_large_values() {
    run(
        "export function main():void{let n=9007199254740993n;console.log(n++);console.log(++n);console.log(n--);console.log(--n);console.log(n);let zero=0n;console.log(zero--);console.log(zero);let sum=0n;for(let i=0n;i<4n;i++){sum+=i;}console.log(sum);}",
        &[
            "9007199254740993",
            "9007199254740995",
            "9007199254740995",
            "9007199254740993",
            "9007199254740993",
            "0",
            "-1",
            "6",
        ],
    );
}
#[test]
fn bigint_values_updates_numeric_unions_return_actual_numeric_types() {
    run(
        "function post(n:bigint|number):bigint|number{return n++;}function pre(n:bigint|number):bigint|number{return --n;}export function main():void{console.log(post(5n));console.log(typeof post(5n));console.log(post(5));console.log(typeof post(5));console.log(pre(5n));console.log(pre(5));const a:(bigint|number)[]=[8n,8];console.log(a[0]++);console.log(++a[1]);console.log(a[0]);console.log(a[1]);}",
        &["5", "bigint", "5", "number", "4", "4", "8", "9", "9", "9"],
    );
}
#[test]
fn bigint_values_updates_prepare_receiver_and_key_once_before_read() {
    run(
        "function source(r:{n:bigint}):{n:bigint}{console.log('source');return r;}export function main():void{let r={n:4n};const original=r;const key=():string=>{console.log('key');original.n=7n;r={n:100n};return 'n';};console.log(source(r)[key()]++);console.log(original.n);console.log(r.n);const a:bigint[]=[5n,9n];let calls=0;const index=():number=>{calls++;a.push(10n);return 0;};console.log(++a[index()]);console.log(calls);console.log(a[0]);console.log(a[2]);}",
        &["source", "key", "7", "8", "100", "6", "1", "6", "10"],
    );
}
#[test]
fn bigint_values_updates_reject_missing_without_publishing_or_storing() {
    run(
        "export function main():void{const a:bigint[]=[5n];delete a[0];try{console.log(a[0]++);}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(a,0));const r:{n?:bigint|undefined}={};try{console.log(++r.n);}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(r,'n'));r.n=8n;const key='n';console.log(r[key]--);console.log(r.n);}",
        &["1", "0", "1", "0", "8", "7"],
    );
}
#[test]
fn bigint_values_updates_preserve_v28_rejection() {
    let source = "export function main():void{let n=1n;n++;}";
    assert!(
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err()
    );
    run(source, &[]);
}
#[test]
fn bigint_values_updates_gc_retains_old_results_and_current_union_targets() {
    let source = "export function main():void{const a:(bigint|number)[]=[9007199254740993n,200];let last:bigint|number=0n;let i=0;while(i<80){last=a[0]++;a[1]--;const text='x'.repeat(20000);i++;}console.log(last);console.log(a[0]);console.log(a[1]);}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Allow the deliberate collection fixture to finish; production budgets stay unchanged.
    execution
        .store
        .set_fuel(1000 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "varta log: 9007199254741072",
            "varta log: 9007199254741073",
            "varta log: 120",
        ]
    );
}
#[test]
fn bigint_values_compound_operators_and_assignment_results() {
    run(
        "export function main():void{let n=17n;console.log(n+=3n);console.log(n-=2n);console.log(n*=3n);console.log(n/=4n);console.log(n%=5n);console.log(n**=3n);console.log(n&=7n);console.log(n|=8n);console.log(n^=3n);console.log(n<<=2n);console.log(n>>=1n);console.log(n);}",
        &[
            "20", "18", "54", "13", "3", "27", "3", "11", "8", "32", "16", "16",
        ],
    );
}
#[test]
fn bigint_values_compound_errors_preserve_target_and_rhs_effects() {
    run(
        "export function main():void{let calls=0;const rhs=():bigint=>{calls+=1;return 0n;};let n=5n;try{n/=rhs();}catch(e){console.log(e instanceof RangeError);}console.log(n);console.log(calls);try{n>>>=1n;}catch(e){console.log(e instanceof TypeError);}console.log(n);let mixed:bigint|number=5n;try{mixed+=2;}catch(e){console.log(e instanceof TypeError);}console.log(mixed);}",
        &["1", "5", "1", "1", "5", "1", "5"],
    );
}
#[test]
fn bigint_values_compound_array_reference_is_evaluated_once_before_rhs() {
    run(
        "export function main():void{const a:bigint[]=[4n];let calls=0;const index=():number=>{calls+=1;return 0;};const rhs=():bigint=>{calls+=10;a[0]=100n;a.push(8n);return 3n;};console.log(a[index()]+=rhs());console.log(a[0]);console.log(a[1]);console.log(calls);const r={n:9n};let key='n';console.log(r[key]*=2n);console.log(r.n);}",
        &["7", "7", "8", "11", "18", "18"],
    );
}
#[test]
fn bigint_values_compound_union_dispatch_sparse_reads_and_failed_store() {
    run(
        "function add(n:bigint|number,rhs:bigint|number):bigint|number{return n+=rhs;}export function main():void{console.log(add(3n,4n));console.log(add(3,4));try{console.log(add(3n,4));}catch(e){console.log(e instanceof TypeError);}const a:bigint[]=[2n];delete a[0];try{a[0]+=1n;}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(a,'0'));const r:{n?:bigint|undefined}={};try{r.n*=2n;}catch(e){console.log(e instanceof TypeError);}console.log(Object.hasOwn(r,'n'));}",
        &["7", "7", "1", "1", "0", "1", "0"],
    );
}
#[test]
fn bigint_values_compound_preserves_older_profile_rejection() {
    let source = "export function main():void{let n=1n;n+=2n;}";
    assert!(
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err()
    );
    run(source, &[]);
}
#[test]
fn bigint_values_arrays_records_optional_unions_and_narrowing() {
    run(
        "export function main():void{const values:bigint[]=[1n,2n];const first=values[0];if(typeof first==='bigint'){console.log(first+2n);}values.push(3n);console.log(values.pop());const object:{n:bigint;maybe?:bigint|undefined}={n:9007199254740993n};object.n=object.n+2n;console.log(object.n);console.log(Object.hasOwn(object,'maybe'));object.maybe=7n;console.log(object.maybe);const mixed=[1n,'x',undefined,2];console.log(mixed[0]);console.log(typeof mixed[0]);}",
        &["3", "3", "9007199254740995", "0", "7", "1", "bigint"],
    );
}
#[test]
fn bigint_values_json_recoverable_type_errors_preserve_other_serialization() {
    run(
        "export function main():void{try{console.log(JSON.stringify(1n));}catch(e){console.log(e instanceof TypeError);}try{console.log(JSON.stringify([1n]));}catch(e){console.log(e instanceof TypeError);}try{console.log(JSON.stringify({n:1n}));}catch(e){console.log(e instanceof TypeError);}const values:(bigint|undefined)[]=[1n];delete values[0];console.log(JSON.stringify(values));console.log(JSON.stringify(undefined));console.log(JSON.stringify([1,undefined]));}",
        &["1", "1", "1", "[null]", "undefined", "[1,null]"],
    );
}
#[test]
fn bigint_values_search_uses_numeric_value_and_keeps_sparse_semantics() {
    run(
        "export function main():void{const a:bigint[]=[1n,2n,1n];console.log(a.includes(BigInt('1')));console.log(a.indexOf(BigInt('2')));console.log(a.lastIndexOf(BigInt('1')));delete a[0];console.log(a.indexOf(1n));console.log(a.includes(undefined));const mixed:(bigint|number|undefined)[]=[1n,1,undefined];console.log(mixed.indexOf(1n));console.log(mixed.indexOf(1));console.log(mixed.includes(BigInt('1')));}",
        &["1", "1", "2", "2", "1", "0", "1", "1"],
    );
}
#[test]
fn bigint_values_union_conversions_truth_and_equality() {
    run(
        "function value(n:bigint|number|string|undefined):bigint|number|string|undefined{return n;}export function main():void{console.log(String(value(12n)));console.log(Number(value(9007199254740993n)));console.log(Boolean(value(0n)));console.log(Boolean(value(1n)));console.log(value(2n)===value(BigInt('2')));console.log(value(2n)===value(2));console.log(typeof value(2n));console.log(Array.isArray(value(2n)));}",
        &["12", "9007199254740992", "0", "1", "1", "0", "bigint", "0"],
    );
}
#[test]
fn bigint_values_throw_catch_finally_and_owned_uncaught_payload() {
    run(
        "function fail():void{throw 12345678901234567890n;}export function main():void{try{fail();}catch(e){console.log(typeof e);if(typeof e==='bigint'){console.log(e+1n);}}finally{console.log('finally');}}",
        &["bigint", "12345678901234567891", "finally"],
    );
    let error = VartaRuntime::new()
        .run_wasm_bytes(
            &compile("export function main():void{throw -9007199254740993n;}"),
            &[],
        )
        .unwrap_err();
    let RuntimeError::Uncaught(exception) = error else {
        panic!("{error:?}")
    };
    let varta_common::exceptions::ThrownValue::BigInt(payload) = exception.value else {
        panic!("{exception:?}")
    };
    let mut budget = varta_bigint::Budget::new(1024, 10000);
    assert_eq!(
        varta_bigint::Integer::from_payload(&payload, &mut budget)
            .unwrap()
            .to_string_radix(10, &mut budget)
            .unwrap(),
        "-9007199254740993"
    );
}
#[test]
fn bigint_values_json_parse_rejects_number_for_bigint_but_handles_closed_unions() {
    run(
        "export function main():void{try{console.log(varta.json.parse<bigint>('123'));}catch(e){console.log(e instanceof TypeError);}const value=varta.json.parse<bigint|number>('123');console.log(typeof value);console.log(value);const object=varta.json.parse<{n:bigint|number;maybe?:bigint|undefined}>('{\"n\":5}');console.log(object.n);console.log(Object.hasOwn(object,'maybe'));}",
        &["1", "number", "123", "5", "0"],
    );
}
#[test]
fn bigint_values_diagnostic_logs_render_graphs_without_json_semantic_failure() {
    run(
        "function value(n:{n:bigint}|undefined):{n:bigint}|undefined{return n;}export function main():void{console.log(value({n:12n}));}",
        &["{\"n\":12n}"],
    );
}
#[test]
fn bigint_values_union_arithmetic_dispatches_present_missing_and_mixed_numeric_pairs() {
    run(
        "function value(n:bigint|number|undefined):bigint|number|undefined{return n;}export function main():void{const a:bigint[]=[1n,2n];console.log(a[0]+a[1]);console.log(value(1n)+value(2n));console.log(value(1)+value(2));console.log(value(undefined)+value(undefined));try{console.log(value(1n)+value(2));}catch(e){console.log(e instanceof TypeError);}console.log(1n===value(1n));console.log(value(1n)===1n);}",
        &["3", "3", "3", "NaN", "1", "1", "1"],
    );
}
#[test]
fn bigint_values_constructor_accepts_primitive_union_inputs_after_ignored_effects() {
    run(
        "function value(n:bigint|number|string|boolean|undefined):bigint|number|string|boolean|undefined{return n;}export function main():void{console.log(BigInt(value('12')));console.log(BigInt(value(12n)));console.log(BigInt(value(true)));try{console.log(BigInt(value(undefined),console.log('ignored')));}catch(e){console.log(e instanceof TypeError);}try{console.log(BigInt(value(1.5)));}catch(e){console.log(e instanceof RangeError);}}",
        &["12", "12", "1", "ignored", "1", "1"],
    );
}
#[test]
fn bigint_values_nullable_json_roots_and_array_callbacks_preserve_type_and_presence() {
    run(
        "function value(n:bigint|number|undefined):bigint|number|undefined{return n;}export function main():void{console.log(JSON.stringify(value(undefined)));try{console.log(JSON.stringify(value(1n)));}catch(e){console.log(e instanceof TypeError);}console.log(JSON.stringify(value(5)));const a:bigint[]=[1n,,3n];console.log(a.reduce((sum:bigint,v:bigint):bigint=>sum+v,0n));const mapped=a.map((v:bigint):bigint=>v+1n);console.log(mapped[0]);console.log(Object.hasOwn(mapped,1));console.log(mapped[2]);const [first,...rest]=a;console.log(first);console.log(rest[1]);}",
        &["undefined", "1", "5", "4", "2", "0", "4", "1", "3"],
    );
}
#[test]
fn bigint_values_arithmetic_evaluates_operands_once_and_preserves_string_addition() {
    run(
        "function left():bigint|number{console.log('left');return 3n;}function right():bigint|number{console.log('right');return 2n;}function text(n:bigint|string):bigint|string{return n;}export function main():void{console.log(left()*right());console.log(text('x')+text(2n));console.log(text(2n)+text('x'));console.log(text(2n)+text(3n));}",
        &["left", "right", "6", "x2", "2x", "5"],
    );
}
#[test]
fn bigint_values_gc_traces_nested_arrays_optional_fields_zero_and_pending_exceptions() {
    let source = "function churn():void{let i=0;while(i<80){const text='x'.repeat(20000);i=i+1;}}function fail():void{try{throw 12345678901234567890n;}finally{churn();}}export function main():void{const a:bigint[]=[0n,9007199254740993n];const object:{a:bigint[];n?:bigint|undefined}={a:a,n:0n};const values:(bigint|undefined)[]=[0n,7n];const saved=values.pop();a.push(3n);churn();console.log(object.a[0]);console.log(object.a[1]);console.log(object.n);console.log(saved);try{fail();}catch(e){if(typeof e==='bigint'){console.log(e+1n);}}}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    execution
        .store
        .set_fuel(1000 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "varta log: 0",
            "varta log: 9007199254740993",
            "varta log: 0",
            "varta log: 7",
            "varta log: 12345678901234567891"
        ]
    );
}
