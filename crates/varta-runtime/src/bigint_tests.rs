use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source);
    let output = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(
        output.lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn bigint_literals_and_arithmetic_execute_without_number_precision_loss() {
    run(
        "export function main():void{const a:bigint=9007199254740993n;console.log(a+2n);console.log(0x1_0000_0000n);console.log(-7n/3n);console.log(-7n%3n);console.log(2n**100n);console.log(~0n);console.log(7n&3n);console.log(1n<<65n);console.log(-3n>>1n);}",
        &[
            "9007199254740995",
            "4294967296",
            "-2",
            "-1",
            "1267650600228229401496703205376",
            "-1",
            "3",
            "36893488147419103232",
            "-2",
        ],
    );
}
#[test]
fn bigint_constructor_conversions_truth_and_typeof() {
    run(
        "export function main():void{console.log(BigInt(' 0xff '));console.log(BigInt(9007199254740992));console.log(BigInt(true));console.log(BigInt(false));console.log(BigInt(''));console.log(String(123456789012345678901234567890n));console.log(Number(9007199254740993n));console.log(typeof 1n);console.log(Boolean(0n));console.log(Boolean(-1n));console.log(!0n);if(2n){console.log('yes');}}",
        &[
            "255",
            "9007199254740992",
            "1",
            "0",
            "0",
            "123456789012345678901234567890",
            "9007199254740992",
            "bigint",
            "0",
            "1",
            "1",
            "yes",
        ],
    );
}
#[test]
fn bigint_native_errors_are_recoverable_and_finalizers_execute() {
    run(
        "export function main():void{try{console.log(1n/0n);}catch(e){console.log(e instanceof RangeError);}finally{console.log('finally');}try{console.log(2n**-1n);}catch(e){console.log(e instanceof RangeError);}try{console.log(+1n);}catch(e){console.log(e instanceof TypeError);}try{console.log(1n>>>1n);}catch(e){console.log(e instanceof TypeError);}try{console.log(BigInt('1.5'));}catch(e){console.log(e instanceof SyntaxError);}try{console.log(BigInt(1.5));}catch(e){console.log(e instanceof RangeError);}try{console.log(BigInt(null));}catch(e){console.log(e instanceof TypeError);}}",
        &["1", "finally", "1", "1", "1", "1", "1", "1"],
    );
}
#[test]
fn bigint_comparison_is_numeric_not_handle_identity_or_number_rounding() {
    run(
        "export function main():void{console.log(123n===BigInt('123'));console.log(9007199254740993n>9007199254740992);console.log(9007199254740992<9007199254740993n);console.log(1n==1);console.log(1n===1);console.log(1n!=1);console.log(1n!==1);console.log(1n>NaN);console.log(1n<=NaN);console.log(-1n>-1.5);console.log(1.5>1n);console.log('x'+12n);console.log(12n+'x');}",
        &[
            "1", "1", "1", "1", "0", "0", "1", "0", "0", "1", "1", "x12", "12x",
        ],
    );
}
#[test]
fn bigint_locals_functions_closures_and_operand_effects() {
    run(
        "function add(a:bigint,b:bigint):bigint{return a+b;}function left():bigint{console.log('left');return 1n;}function right():bigint{console.log('right');return 2n;}export function main():void{let x=2n;const f=():bigint=>x;x=5n;console.log(add(f(),7n));console.log(left()+right());try{console.log(left()+3);}catch(e){console.log(e instanceof TypeError);}console.log(BigInt('42',console.log('ignored')));}",
        &["12", "left", "right", "3", "left", "1", "ignored", "42"],
    );
}
#[test]
fn bigint_old_profiles_reject_new_source_and_structured_admission_remains_explicit() {
    for profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .copied()
        .filter(|p| !p.bigint())
    {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{console.log(1n+2n);}",
                profile
            )
            .is_err()
        );
    }
    for source in [
        "export function main():void{const a=[1n];}",
        "export function main():void{console.log(JSON.stringify(1n));}",
        "export function main():void{throw 1n;}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err(),
            "{source}"
        );
    }
}
#[test]
fn bigint_methods_widths_and_short_circuiting() {
    run(
        "export function main():void{console.log((-255n).toString(16));console.log((123n).valueOf(console.log('ignored')));console.log(BigInt.asIntN(8,255n));console.log(BigInt.asUintN(8,-1n));console.log(BigInt.asIntN(0,123n));console.log(BigInt.asIntN(9007199254740991,-1n));try{console.log(BigInt.asUintN(-1,2n));}catch(e){console.log(e instanceof RangeError);}try{console.log((1n).toString(37,console.log('radix ignored')));}catch(e){console.log(e instanceof RangeError);}console.log(0n||5n);console.log(1n&&7n);console.log(0n??8n);}",
        &[
            "-ff",
            "ignored",
            "123",
            "-1",
            "255",
            "0",
            "-1",
            "1",
            "radix ignored",
            "1",
            "5",
            "7",
            "0",
        ],
    );
}
#[test]
fn bigint_loose_and_relational_primitive_comparisons() {
    run(
        "export function main():void{console.log(1n==true);console.log(0n==false);console.log(0n==null);console.log(0n!=undefined);console.log(1n=='1');console.log('0x10'==16n);console.log(1n=='1.0');console.log(2n<'3');console.log('1'<2n);console.log(0n<undefined);console.log(0n>=undefined);console.log(0n>=null);console.log(1n>true);console.log(1n>='oops');}",
        &[
            "1", "1", "0", "1", "1", "1", "0", "1", "1", "0", "0", "1", "0", "0",
        ],
    );
}
#[test]
fn bigint_pure_operations_preserve_denial_status_and_limits_remain_fatal() {
    run(
        "export function main():void{varta.env.get('VARTA_BIGINT_DENIED');console.log(varta.result.status());console.log(2n+3n);try{console.log(1n/0n);}catch(e){console.log(e instanceof RangeError);}console.log(varta.result.status());}",
        &[
            "<capability denied: env> (VARTA_BIGINT_DENIED)",
            "1",
            "5",
            "1",
            "1",
        ],
    );
    for source in [
        "export function main():void{try{console.log(1n<<9007199254740993n);}catch(e){console.log('caught');}finally{console.log('finally');}}",
        "export function main():void{try{console.log(1n<<1000000n);}catch(e){console.log('caught');}finally{console.log('finally');}}",
    ] {
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&compile(source)).unwrap();
        let mut execution =
            crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(
            error
                .root_cause()
                .downcast_ref::<varta_bigint::Error>()
                .is_some_and(|e| e.is_resource()),
            "{error:?}"
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
}
#[test]
fn bigint_roots_preserve_closure_cells_and_saved_finally_returns_through_gc() {
    let source = "function churn():void{let i=0;while(i<80){const text='a'.repeat(20000);i=i+1;}}function saved():bigint{const value=123456789012345678901234567890n;try{return value;}finally{churn();}}export function main():void{let x=9007199254740993n;const get=():bigint=>x;churn();console.log(get());x=x+2n;churn();console.log(get());console.log(saved());}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Independent forced-collection stress only; production/default-limit tests
    // above retain the real one-million-fuel budget.
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
            "varta log: 9007199254740993",
            "varta log: 9007199254740995",
            "varta log: 123456789012345678901234567890"
        ]
    );
}
#[test]
fn bigint_new_missing_arguments_and_ignored_effects_keep_native_errors() {
    run(
        "export function main():void{try{console.log(new BigInt(console.log('new argument')));}catch(e){console.log(e instanceof TypeError);}try{console.log(BigInt());}catch(e){console.log(e instanceof TypeError);}try{console.log(BigInt('bad',console.log('conversion ignored')));}catch(e){console.log(e instanceof SyntaxError);}console.log(Number.isFinite(1n));console.log(Number.isNaN(1n));console.log(1n===undefined);}",
        &[
            "new argument",
            "1",
            "1",
            "conversion ignored",
            "1",
            "0",
            "0",
            "0",
        ],
    );
}
#[test]
fn bigint_builtin_names_obey_lexical_shadowing() {
    run(
        "function BigInt(value:number):number{return value+1;}export function main():void{console.log(BigInt(4));}",
        &["5"],
    );
    run(
        "export function main():void{const BigInt=(n:number):number=>n+2;console.log(BigInt(3));}",
        &["5"],
    );
    for source in [
        "export function main():void{const BigInt=3;console.log(BigInt.asIntN(8,255n));}",
        "export function main():void{const BigInt=3;const f=():void=>{console.log(BigInt.asIntN(8,255n));};f();}",
        "export function main():void{const BigInt=3;const f=():void=>{new BigInt();};f();}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V28).is_err(),
            "{source}"
        );
    }
}
