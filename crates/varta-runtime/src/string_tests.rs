use super::*;

#[test]
fn typed_strings_preserve_signatures_slots_recursion_and_scalar_control_flow() {
    let source = r#"
function first(s:string,n:number):string{if(n==0){return s;}return first(s,n-1);}
function second(n:number,s:string):string{return s;}
function count(s:string,n:number):number{return n;}
function show(s:string):void{console.log(s);}
export function main():void{
 let n=20; let text:string="Привіт 🦀";
 const original=text; text="changed";
 {let text="shadow";show(text);}
 for(let i=0;i<2;i=i+1){let s=second(i,original);show(first(s,2));}
 switch(n){case 20:show(text);break;default:show("wrong");}
 console.log(count(text,22)+n);show("");
 first("discarded",1); "discarded literal";
}
"#;
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let runtime = VartaRuntime::new();
    for _ in 0..2 {
        let result = runtime.run_wasm_bytes(&bytes, &[]).unwrap();
        assert_eq!(
            result.lines,
            [
                "varta log: shadow",
                "varta log: Привіт 🦀",
                "varta log: Привіт 🦀",
                "varta log: changed",
                "varta log: 42",
                "varta log: "
            ]
        );
    }
}

#[test]
fn unsupported_string_operations_and_type_errors_remain_explicit() {
    for source in [
        "export function main():void{let s=\"x\";s=1;}",
        "export function main():void{const s=\"x\";s=\"y\";}",
        "function f(s:string):string{return 1;} export function main():void{}",
        "function f(s:string):string{return s;} export function main():void{f(1);}",
        "export function main():void{console.log(\"a\"+1);}",
        "export function main():void{console.log(\"a\"-\"b\");}",
        "export function main():void{console.log(\"a\"*\"b\");}",
        "export function main():void{console.log(\"a\"&&\"b\");}",
        "export function main():void{console.log(\"a\"==1);}",
        "export function main():void{if(\"x\"){}}",
        "export function main():void{switch(\"x\"){default:break;}}",
        "function f(s:void):void{} export function main():void{}",
    ] {
        assert!(
            varta_compiler::compile_typescript(source).is_err(),
            "{source}"
        );
    }
}

#[test]
fn string_helpers_fit_the_full_user_function_budget() {
    let mut source = String::new();
    for index in 1..varta_common::abi::MAX_USER_FUNCTIONS {
        source.push_str(&format!("function f{index}():void{{}}\n"));
    }
    source.push_str("export function main():void{let s=varta.env.get(\"DENIED\");console.log(varta.result.status());console.log(s+\"X\"==\"X\");}");
    let bytes = varta_compiler::compile_typescript(&source).unwrap();
    let result = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    assert_eq!(
        &result.lines[result.lines.len() - 2..],
        ["varta log: 1", "varta log: 1"]
    );
}

#[test]
fn string_arena_grows_to_its_boundary_and_traps_without_overwriting_saved_values() {
    use varta_common::abi;
    use wasmtime::{Caller, Engine, Extern, Linker, Module, Store, Val};
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config).unwrap();
    let mut linker = Linker::new(&engine);
    // A deterministic in-memory host supplies one page per call. Extra fuel
    // lets this test reach the memory boundary; production fuel bounds copies first.
    linker
        .func_wrap(
            "varta",
            "env_get",
            |mut caller: Caller<'_, ()>, _: i32, _: i32| -> i32 {
                let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                    panic!("missing memory")
                };
                if memory.size(&caller) < 2 {
                    memory.grow(&mut caller, 1).unwrap();
                }
                memory
                    .write(
                        &mut caller,
                        abi::RESULT_BASE,
                        &vec![b'X'; abi::WASM_PAGE_BYTES],
                    )
                    .unwrap();
                let Some(Extern::Global(length)) = caller.get_export(abi::RESULT_LEN) else {
                    panic!("missing result length")
                };
                length
                    .set(&mut caller, Val::I32(abi::WASM_PAGE_BYTES as i32))
                    .unwrap();
                abi::RESULT_BASE as i32
            },
        )
        .unwrap();
    let capacity = (abi::MAX_MEMORY_BYTES - abi::OWNED_STRING_BASE) / abi::WASM_PAGE_BYTES;
    for (copies, success) in [(capacity, true), (capacity + 1, false)] {
        let source = format!(
            "export function main():void{{for(let i=0;i<{copies};i=i+1){{let s=varta.env.get(\"name\");}}}}"
        );
        let bytes = varta_compiler::compile_typescript(&source).unwrap();
        let module = Module::new(&engine, &bytes).unwrap();
        let mut store = Store::new(&engine, ());
        store.set_fuel((abi::MAX_MEMORY_BYTES * 2) as u64).unwrap();
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let result = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .unwrap()
            .call(&mut store, ());
        assert_eq!(result.is_ok(), success, "{result:?}");
        if let Err(error) = result {
            assert!(format!("{error:#}").contains("unreachable"), "{error:#}");
        }
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        assert_eq!(memory.data_size(&store), abi::MAX_MEMORY_BYTES);
        let owned = &memory.data(&store)[abi::OWNED_STRING_BASE..];
        assert!(owned.iter().all(|byte| *byte == b'X'));
    }
}
