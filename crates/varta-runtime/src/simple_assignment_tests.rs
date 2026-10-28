use super::*;
use varta_common::language::LanguageProfile;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V14)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}

#[test]
fn simple_assignment_values_keep_original_scalar_types_for_locals_fields_and_elements() {
    let source = r"export function main():void{let value:number|string|undefined=0;const n:number=(value=2.5);const s:string=(value='owned'+'!');const missing:undefined=(value=undefined);console.log(n+1);console.log(s);console.log(missing);console.log(typeof value);const record:{n?:number}={};console.log(record.n=-0);const a:(number|undefined)[]=[1];const result:number=(a[0]=3);console.log(result+1);console.log(JSON.stringify(a));}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["3.5", "owned!", "undefined", "undefined", "-0", "4", "[3]"]),
            "{profile:?}"
        );
    }
}

#[test]
fn simple_assignment_values_update_captured_cells_and_contextual_function_values() {
    let out = run(
        r"export function main():void{let n=0;const write=():number=>(n=n+1);const read=():number=>n;console.log(write());console.log(read());let fn:(x:number)=>number=x=>x;const chosen:(x:number)=>number=(fn=x=>n=n+x);console.log(chosen(3));console.log(read());console.log((fn=x=>x+1)(2));let saved:()=>number=()=>99;{let k=2;saved=()=>k;console.log(k=5);}console.log(saved());let text='before';const prior=():string=>text;console.log(text=prior()+'!');console.log(prior());}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "1", "4", "4", "3", "5", "5", "before!", "before!"])
    );
}

#[test]
fn simple_assignment_values_prepare_receivers_and_numeric_keys_before_rhs_mutations() {
    let out = run(
        r"function record():{n:number}{console.log('record');return {n:0};}function array():number[]{console.log('array');return [0];}function index():number{console.log('index');return 0;}function rhs():number{console.log('rhs');return 2;}export function main():void{console.log(record().n=rhs());console.log(array()[index()]=rhs());let current=[1];const original=current;let at=0;const replace=():number=>{current=[9];at=1;original.push(2,3);return 7;};console.log(current[at]=replace());console.log(JSON.stringify(original));console.log(JSON.stringify(current));let object={n:1};const first=object;const change=():number=>{object={n:10};return 4;};console.log(object.n=change());console.log(first.n);console.log(object.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "record", "rhs", "2", "array", "index", "rhs", "2", "7", "[7,2,3]", "[9]", "4", "4",
            "10"
        ])
    );
}

#[test]
fn simple_assignment_values_chain_all_targets_and_preserve_inner_stores_on_outer_errors() {
    let out = run(
        r"function take(value:number,next:number):void{console.log('BAD_BODY');}function later():number{console.log('BAD_LATER');return 1;}export function main():void{let n:number|string=0;const record:{n?:number}={};const values:(number|undefined)[]=[0];const result:number=(n=record.n=values[0]=5);console.log(result+1);console.log(JSON.stringify(record));console.log(JSON.stringify(values));console.log(typeof n);const a=[1,2];let local=0;const inner={n:0};try{take((a.length=inner.n=local=-1),later());}catch(error){console.log(error instanceof RangeError);}console.log(a.length);console.log(inner.n);console.log(local);const outer={n:0};try{take((outer['missing'+ '']=local=inner.n=8),later());}catch(error){console.log(error instanceof TypeError);}console.log(outer.n);console.log(inner.n);console.log(local);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "6",
            "{\"n\":5}",
            "[5]",
            "number",
            "1",
            "2",
            "-1",
            "-1",
            "1",
            "0",
            "8",
            "8"
        ])
    );
}

#[test]
fn simple_assignment_values_keep_reference_identity_context_and_error_families() {
    let out = run(
        r"export function main():void{let ref:{n:number}|number[]|undefined=undefined;const a:number[]=(ref=[]);a.push(2);console.log(JSON.stringify(ref));const child:{n:number}=(ref={n:7});child.n=8;console.log(JSON.stringify(ref));const holder:{child?:{n:number}}={};const returned=(holder.child={n:9});returned.n=10;console.log(JSON.stringify(holder));const arrays:({n:number}|undefined)[]=[undefined];const selected:{n:number}=(arrays[0]=child);console.log(selected===child);selected.n=11;console.log(JSON.stringify(arrays));console.log(JSON.stringify(ref));let error:Error=Error();const exact:TypeError=(error=TypeError('exact'));console.log(exact.message);const errors:Error[]=[Error()];const typed:RangeError=(errors[0]=RangeError('range'));console.log(typed.message);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[2]",
            "{\"n\":8}",
            "{\"child\":{\"n\":10}}",
            "1",
            "[{\"n\":11}]",
            "{\"n\":11}",
            "exact",
            "range"
        ])
    );
}

#[test]
fn simple_assignment_values_preserve_all_slots_of_result_pairs() {
    let out = run(
        r"function result():VartaResult<string>{let value=varta.result.capture(varta.env.get('VARTA_PAIR_INITIAL'));return value=varta.result.capture(varta.env.get('VARTA_PAIR_RETURN'));}export function main():void{let value=varta.result.capture(varta.env.get('VARTA_PAIR_FIRST'));const copy:VartaResult<string>=(value=result());console.log(copy.status);console.log(value.status);console.log(copy.value===value.value);let cell=varta.result.capture(varta.env.get('VARTA_CELL_FIRST'));const update=():VartaResult<string>=>cell=varta.result.capture(varta.env.get('VARTA_CELL_NEXT'));const cached=update();console.log(cached.status);console.log(cell.status);let number=varta.result.capture(varta.time.now());const numeric:VartaResult<number>=(number=varta.result.capture(varta.time.now()));console.log(numeric.status);console.log(number.status);let empty=varta.result.capture(varta.env.print('VARTA_PAIR_VOID'));const nothing:VartaResult<void>=(empty=varta.result.capture(varta.env.print('VARTA_PAIR_VOID')));console.log(nothing.status);console.log(typeof nothing.value);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (VARTA_PAIR_FIRST)",
            "<capability denied: env> (VARTA_PAIR_INITIAL)",
            "<capability denied: env> (VARTA_PAIR_RETURN)",
            "1",
            "1",
            "1",
            "<capability denied: env> (VARTA_CELL_FIRST)",
            "<capability denied: env> (VARTA_CELL_NEXT)",
            "1",
            "1",
            "<capability denied: time.now> (clock)",
            "<capability denied: time.now> (clock)",
            "1",
            "1",
            "<capability denied: env> (VARTA_PAIR_VOID)",
            "<capability denied: env> (VARTA_PAIR_VOID)",
            "1",
            "undefined"
        ])
    );
}

#[test]
fn simple_assignment_values_v14_resize_current_backing_and_publish_sparse_presence() {
    let out = run(
        r"export function main():void{const a:(number|undefined)[]=[1,,3];console.log(a[5]=undefined);console.log(a.length);console.log(Object.hasOwn(a,5));console.log(Object.hasOwn(a,4));console.log(JSON.stringify(a));const length:number=(a.length=-0);console.log(1/length);console.log(a.length);console.log(a['length']=3);console.log(Object.hasOwn(a,0));console.log(a[2]=4);const current=[1,2,3];const resize=():number=>{current.length=0;current.push(9);return 7;};console.log(current[2]=resize());console.log(JSON.stringify(current));const maximum:number[]=[];console.log(maximum[1023]=2);console.log(maximum.length);console.log(Object.hasOwn(maximum,1023));console.log(Object.hasOwn(maximum,1022));console.log(JSON.stringify(maximum.slice(1022)));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "undefined",
            "6",
            "1",
            "0",
            "[1,null,3,null,null,null]",
            "-Infinity",
            "0",
            "3",
            "0",
            "4",
            "7",
            "[9,null,7]",
            "2",
            "1024",
            "1",
            "0",
            "[null,2]"
        ])
    );
}

#[test]
fn simple_assignment_values_v14_length_failures_keep_rhs_effects_without_success_values() {
    let out = run(
        r"function take(value:number,next:number):void{console.log('BAD_BODY');}function later():number{console.log('BAD_LATER');return 1;}export function main():void{const a=[1,2];const rhs=():number=>{a[0]=9;return 1.5;};try{take((a.length=rhs()),later());}catch(error){console.log(error instanceof RangeError);}console.log(JSON.stringify(a));for(const n of [-1,NaN,Infinity,-Infinity,4294967296]){if(n!==undefined){try{console.log(a.length=n);}catch(error){console.log(error instanceof RangeError);}}}console.log(a.length);}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "[9,2]", "1", "1", "1", "1", "1", "2"])
    );
}

#[test]
fn simple_assignment_values_numeric_validation_and_bounds_stay_fatal_after_rhs() {
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        for key in ["-1", "0.5", "NaN", "Infinity", "-Infinity", "1024"] {
            let source = format!(
                "function rhs():number{{console.log('rhs');return 2;}}export function main():void{{const a=[1];try{{console.log(a[{key}]=rhs());}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
            );
            let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
            let runtime = VartaRuntime::new();
            let module = runtime.validate_module(&bytes).unwrap();
            let mut execution =
                crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            assert!(
                instance
                    .get_typed_func::<(), ()>(&mut execution.store, "_start")
                    .unwrap()
                    .call(&mut execution.store, ())
                    .is_err()
            );
            assert_eq!(
                execution.store.data().output.lines,
                lines(&["rhs"]),
                "{profile:?}: {key}"
            );
        }
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        let source = "function rhs():number{console.log('rhs');return 2;}export function main():void{const a=[1];try{console.log(a[2]=rhs());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}";
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err()
        );
        assert_eq!(execution.store.data().output.lines, lines(&["rhs"]));
    }
}

#[test]
fn simple_assignment_values_returns_callbacks_and_loop_cells_keep_original_values() {
    let out = run(
        r"function saved(value:{n?:number}):number{try{return value.n=2;}finally{value.n=9;}}export function main():void{const value:{n?:number}={};console.log(saved(value));console.log(value.n);let local=0;console.log(JSON.stringify([1,2].map(item=>{try{return local=value.n=item;}finally{value.n=10;}})));console.log(local);console.log(value.n);let read:()=>number=()=>99;for(let i=0;i<2;i=i+1){read=()=>i;console.log(i=i+1);}console.log(read());const numbers=[0];while((numbers[0]=local=local-1)>0){console.log(local);}console.log(local);}",
    );
    assert_eq!(
        out.lines,
        lines(&["2", "9", "[1,2]", "2", "10", "1", "1", "1", "0"])
    );
}

#[test]
fn simple_assignment_values_reject_constants_tdz_stale_guards_and_invariant_type_mismatches() {
    for source in [
        "export function main():void{const n=0;console.log(n=1);}",
        "export function main():void{const result=(n=1);let n=0;}",
        "export function main():void{let n:number|string=1;if(typeof n==='number'){console.log(n=2);}}",
        "export function main():void{let n=0;console.log(n='wrong');}",
        "export function main():void{const a=[1];console.log(a[0]='wrong');}",
        "export function main():void{const a=[1];console.log(a['0']=2);}",
        "export function main():void{const value={n:1};console.log(value.missing=2);}",
        "export function main():void{const value={n:1};console.log(value.n='wrong');}",
        "export function main():void{const value=TypeError('x');console.log(value.message='new');}",
        "export function main():void{let n=0;console.log(n**=2);}",
        "export function main():void{let n:number|string=1;const result:string=(n=2);}",
        "export function main():void{let a:number[]=[];console.log(a=['wrong']);}",
        "export function main():void{let child={n:1};console.log(child={s:'wrong'});}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V3,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        for target in ["local", "value.n", "a[0]"] {
            let source = format!(
                "export function main():void{{let local=0;const value={{n:0}};const a=[0];console.log({target}=2);}}"
            );
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        for target in ["a.length", "a['length']"] {
            let source =
                format!("export function main():void{{const a=[0];console.log({target}=2);}}");
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
}

#[test]
fn simple_assignment_values_gc_keeps_inputs_results_cells_pairs_and_escaped_functions() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}
        function rhs():{text:string}{churn();return {text:'owned'+'!'};}
        function keep(first:{text:string},captured:VartaResult<string>,value:{text:string},after:number):string{churn();console.log(captured.status);return first.text+value.text;}
        function saved():{text:string}{let local:{text:string}|undefined=undefined;try{return local=rhs();}finally{churn();}}
        function make():()=>string{let fn:()=>string=()=>'';const text='closure'+'!';return fn=()=>text;}
        export function main():void{
            let local:{text:string}|undefined=undefined;const holder:{child?:{text:string}}={};const values:({text:string}|undefined)[]=[undefined];
            console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_SIMPLE_GC')),local=holder.child=values[0]=rhs(),churn()));
            churn();console.log(JSON.stringify(holder));console.log(JSON.stringify(values));console.log(JSON.stringify(local));console.log(saved().text);
            const fn=make();churn();console.log(fn());
            let text:string|undefined=undefined;const strings:{text?:string}={};const slots:(string|undefined)[]=[undefined];let result='';
            for(let i=0;i<7000;i=i+1){result=(text=strings.text=slots[0]='retained'+'!');}churn();console.log(result);console.log(text);console.log(strings.text);console.log(slots[0]);
        }";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        lines(&[
            "<capability denied: env> (VARTA_SIMPLE_GC)",
            "1",
            "earlier!owned!",
            "{\"child\":{\"text\":\"owned!\"}}",
            "[{\"text\":\"owned!\"}]",
            "{\"text\":\"owned!\"}",
            "owned!",
            "closure!",
            "retained!",
            "retained!",
            "retained!",
            "retained!"
        ])
    );
}

#[test]
fn simple_assignment_values_visit_prepared_array_and_local_effects_in_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for (source, key, rhs, expression) in [
        (
            "console.log('source');return [1];",
            "return 0;",
            "return 2;",
            "source()[key()]=rhs()",
        ),
        (
            "return [1];",
            "console.log('key');return 0;",
            "return 2;",
            "source()[key()]=rhs()",
        ),
        (
            "return [1];",
            "return 0;",
            "console.log('rhs');return 2;",
            "source()[key()]=rhs()",
        ),
        (
            "return [1];",
            "return 0;",
            "console.log('rhs');return 2;",
            "local=rhs()",
        ),
    ] {
        let bytes = compile(&format!(
            "function source():number[]{{{source}}}function key():number{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{let local=0;const result=({expression});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
        ));
        let fake = export_index(&bytes, "_start") - 1;
        let name = varta_common::value::VALUE_ALLOCATE;
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                let at = data
                    .windows(name.len())
                    .position(|s| s == name.as_bytes())
                    .unwrap()
                    + name.len()
                    + 1;
                assert!(fake < 128 && data[at] < 128);
                data[at] = fake;
            }
            vec![data]
        });
        let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{error}"
        );
    }
}

#[test]
fn simple_assignment_values_fatal_rhs_limits_skip_later_work_and_handlers() {
    for target in ["local", "record.n", "array[0]", "array.length"] {
        let bytes = compile(&format!(
            "function rhs():number{{while(true){{}}return 2;}}function take(value:number,next:number):void{{console.log('BAD_BODY');}}function later():number{{console.log('BAD_LATER');return 1;}}export function main():void{{let local=0;const record={{n:0}};const array=[0];try{{take(({target}=rhs()),later());}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
        ));
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err()
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
}
