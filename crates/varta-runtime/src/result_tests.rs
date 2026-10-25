use super::*;

#[test]
fn captured_results_preserve_multi_value_slots_signatures_aliases_and_control_flow() {
    let source = r#"
function read(name:string):VartaResult<string>{return varta.result.capture(varta.env.get(name));}
function forward(n:number,r:VartaResult<string>,tail:string):VartaResult<string>{if(n==0){return r;}return forward(n-1,r,tail);}
function inspect(r:VartaResult<string>,n:number):number{return r.status+n;}
export function main():void{
 let r=read("DENIED");const saved=r;const text="tail";let n=20;
 for(let i=0;i<2;i=i+1){let local=forward(i,saved,text);if(!local.ok){console.log(local.status);}}
 switch(saved.status){case 1:console.log(inspect(saved,21)+n);break;default:console.log(999);}
 {const r=varta.result.capture(varta.time.now());console.log(r.ok);console.log(r.value);}
 r=forward(2,saved,text);console.log(r.value);console.log(saved.status);
 read("OTHER");varta.result.capture(varta.process.exit(7)).value;
 console.log(varta.result.capture(varta.env.has("BOOL")).status);
}
"#;
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    let observed: Vec<_> = output
        .lines
        .iter()
        .filter(|line| !line.contains("<capability denied:"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        observed,
        [
            "varta log: 1",
            "varta log: 1",
            "varta log: 42",
            "varta log: 0",
            "varta log: 0",
            "varta log: ",
            "varta log: 1",
            "varta log: 1"
        ]
    );
    assert_eq!(output.exit_code, 0);
}

#[test]
fn immediate_result_fields_execute_the_host_call_once_and_distinguish_successful_false_and_zero() {
    use wasmtime::{Caller, Engine, Extern, Linker, Module, Store, Val};
    let source = r#"
function status(r:VartaResult<boolean>):number{return r.status;}
export function main():void{
 const r=varta.result.capture(varta.env.has("X"));
 console.log(r.ok);console.log(r.value);console.log(status(r));
 console.log(varta.result.capture(varta.env.has("Y")).status);
 const zero=varta.result.capture(varta.time.now());
 const http=varta.result.capture(varta.net.fetch("http://unused/"));
 console.log(zero.ok);console.log(zero.value);console.log(zero.status);
 console.log(http.ok);console.log(http.value);console.log(http.status);
}
"#;
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).unwrap();
    #[derive(Default)]
    struct State {
        calls: usize,
        values: Vec<i32>,
    }
    fn set_status(caller: &mut Caller<'_, State>, status: i32) {
        let Some(Extern::Global(global)) = caller.get_export(varta_common::abi::RESULT_STATUS)
        else {
            panic!("missing status")
        };
        global.set(caller, Val::I32(status)).unwrap();
    }
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "varta",
            "env_has",
            |mut caller: Caller<'_, State>, _: i32, _: i32| -> i32 {
                caller.data_mut().calls += 1;
                let status = if caller.data().calls == 1 { 0 } else { 1 };
                set_status(&mut caller, status);
                0
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "varta",
            "time_now",
            |mut caller: Caller<'_, State>| -> i32 {
                set_status(&mut caller, 0);
                0
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "varta",
            "net_fetch",
            |mut caller: Caller<'_, State>, _: i32, _: i32| -> i32 {
                set_status(&mut caller, 0);
                404
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "varta",
            "log_i32",
            |mut caller: Caller<'_, State>, value: i32| {
                caller.data_mut().values.push(value);
            },
        )
        .unwrap();
    let mut store = Store::new(&engine, State::default());
    let instance = linker.instantiate(&mut store, &module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    assert_eq!(store.data().calls, 2);
    assert_eq!(store.data().values, [1, 0, 0, 1, 1, 0, 0, 1, 404, 0]);
}
