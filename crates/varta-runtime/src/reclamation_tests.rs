use super::*;
use varta_common::abi;
use wasmtime::{Caller, Engine, Extern, Instance, Linker, Module, Store, Val};

#[derive(Default)]
struct State {
    logs: Vec<String>,
}
pub(super) struct Guest {
    store: Store<State>,
    instance: Instance,
}
impl Guest {
    pub(super) fn new(source: &str) -> Self {
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        VartaRuntime::new().validate_module(&bytes).unwrap();
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).unwrap();
        let module = Module::new(&engine, &bytes).unwrap();
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(
                "varta",
                "log_i32",
                |mut caller: Caller<'_, State>, value: i32| {
                    caller.data_mut().logs.push(value.to_string());
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "varta",
                "log_string",
                |mut caller: Caller<'_, State>, ptr: i32, len: i32| {
                    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                        panic!("memory")
                    };
                    let value = std::str::from_utf8(
                        &memory.data(&caller)[ptr as usize..(ptr + len) as usize],
                    )
                    .unwrap()
                    .to_owned();
                    caller.data_mut().logs.push(value);
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "varta",
                "env_get",
                |mut caller: Caller<'_, State>, ptr: i32, len: i32| -> i32 {
                    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                        panic!("memory")
                    };
                    let denied =
                        &memory.data(&caller)[ptr as usize..(ptr + len) as usize] == b"denied";
                    let value = if denied {
                        ""
                    } else {
                        "Збережено 🦀"
                    };
                    memory
                        .write(&mut caller, abi::RESULT_BASE, value.as_bytes())
                        .unwrap();
                    for (name, value) in [
                        (abi::RESULT_LEN, value.len() as i32),
                        (abi::RESULT_STATUS, i32::from(denied)),
                    ] {
                        let Some(Extern::Global(global)) = caller.get_export(name) else {
                            panic!("global")
                        };
                        global.set(&mut caller, Val::I32(value)).unwrap();
                    }
                    if denied { 0 } else { abi::RESULT_BASE as i32 }
                },
            )
            .unwrap();
        let mut store = Store::new(&engine, State::default());
        store.set_fuel((abi::MAX_MEMORY_BYTES * 8) as u64).unwrap();
        let instance = linker.instantiate(&mut store, &module).unwrap();
        Self { store, instance }
    }
    pub(super) fn call(&mut self) -> Result<(), wasmtime::Error> {
        self.instance
            .get_typed_func::<(), ()>(&mut self.store, "_start")
            .unwrap()
            .call(&mut self.store, ())
    }
    pub(super) fn memory_size(&mut self) -> usize {
        self.instance
            .get_memory(&mut self.store, "memory")
            .unwrap()
            .data_size(&self.store)
    }
    pub(super) fn logs(&self) -> &[String] {
        &self.store.data().logs
    }
}

#[test]
fn collection_preserves_expression_arguments_frames_aliases_results_and_control_flow() {
    let source = format!(
        r#"
function churn():string{{
 const piece={:?};
 for(let i=0,guard="guard"+"!";i<100;i=i+1){{
  if(guard!="guard!"){{return "BROKEN_GUARD";}}
  {{const temporary=piece+"x";if(i%3==0){{continue;}}
   switch(i%3){{case 1:{{const nested=piece+"y";break;}}default:{{const other=piece+"z";break;}}}}
  }}
 }}
 let n=0;while(n<50){{const a=piece+"q";n=n+1;if(n<40){{continue;}}break;}}
 return "right"+"!";
}}
function first(a:string,b:string):string{{return a;}}
function bounce(r:VartaResult<string>,n:number):VartaResult<string>{{
 const local=churn();if(n==0){{return r;}}return bounce(r,n-1);
}}
export function main():void{{
 const captured=varta.result.capture(varta.env.get("saved"));
 const alias=captured;let retained=churn();const saved=retained;retained="changed";
 const argument=first("left"+"!",churn());
 const compound=("left"+"!") + churn();
 const record=bounce(captured,2);const denied=varta.result.capture(varta.env.get("denied"));
 console.log(argument);console.log(compound);console.log(saved);console.log(retained);
 console.log(record.value);console.log(alias.value);console.log(record.ok);console.log(record.status);console.log(denied.status);
}}
"#,
        "X".repeat(768)
    );
    let mut guest = Guest::new(&source);
    guest.call().unwrap();
    assert_eq!(
        guest.store.data().logs,
        [
            "left!",
            "left!right!",
            "right!",
            "changed",
            "Збережено 🦀",
            "Збережено 🦀",
            "1",
            "0",
            "1"
        ]
    );
    assert!(guest.memory_size() < abi::OWNED_STRING_BASE + 8 * abi::WASM_PAGE_BYTES);
}

#[test]
fn free_blocks_split_and_coalesce_without_reclaiming_live_middle_aliases() {
    let source = format!(
        r#"
export function main():void{{const chunk={:?};const big={:?};
 for(let i=0;i<32;i=i+1){{
  let a=chunk+"a";let b=chunk+"b";let c=chunk+"c";const saved=b;
  a="";c="";let pressure=big+"p";
  let small="s"+"x";b="";pressure="";
  let again=big+"q";
  if(saved!=chunk+"b"){{console.log(0);return;}}
  small="";again="";const more=big+"r";
  if(saved!=chunk+"b"){{console.log(0);return;}}
 }}console.log(1);
}}
"#,
        "C".repeat(2048),
        "Z".repeat(60000)
    );
    let mut guest = Guest::new(&source);
    guest.call().unwrap();
    assert_eq!(guest.store.data().logs, ["1"]);
    assert!(guest.memory_size() <= abi::OWNED_STRING_BASE + 4 * abi::WASM_PAGE_BYTES);
}

#[test]
fn deterministic_assignment_model_survives_repeated_collection_and_scope_escape() {
    let mut source = format!(
        "function churn():void{{const piece={:?};for(let i=0;i<64;i=i+1){{const temporary=piece+\"!\";}}}} export function main():void{{",
        "X".repeat(768)
    );
    let mut model: Vec<String> = (0..8).map(|i| format!("seed{i}!")).collect();
    for i in 0..model.len() {
        source.push_str(&format!("let v{i}=\"seed{i}\"+\"!\";"));
    }
    let mut seed = 0x6a09_e667_u32;
    for step in 0..128 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let destination = (seed as usize >> 8) % model.len();
        let origin = (seed as usize >> 16) % model.len();
        let suffix = format!("t{step}");
        match seed % 5 {
            0 => {
                source.push_str(&format!("v{destination}=v{origin};"));
                model[destination] = model[origin].clone();
            }
            1 if model[origin].len() < 256 => {
                source.push_str(&format!("v{destination}=v{origin}+{suffix:?};"));
                model[destination] = format!("{}{suffix}", model[origin]);
            }
            2 => {
                source.push_str(&format!("v{destination}=\"fresh\"+{suffix:?};"));
                model[destination] = format!("fresh{suffix}");
            }
            3 => {
                source.push_str(&format!(
                    "{{const escaped=v{origin}+{suffix:?};v{destination}=escaped;}}"
                ));
                model[destination] = format!("{}{suffix}", model[origin]);
            }
            _ => {
                source.push_str(&format!("v{destination}=\"\";"));
                model[destination].clear();
            }
        }
        if step % 4 == 0 {
            source.push_str("churn();");
        }
    }
    source.push_str("churn();");
    for (i, value) in model.iter().enumerate() {
        source.push_str(&format!("console.log(v{i}=={value:?});"));
    }
    source.push('}');
    let mut guest = Guest::new(&source);
    guest.call().unwrap();
    assert_eq!(guest.logs(), vec!["1"; model.len()]);
    assert!(guest.memory_size() <= abi::OWNED_STRING_BASE + 4 * abi::WASM_PAGE_BYTES);
}

#[test]
fn long_lived_strings_still_exhaust_the_heap_and_root_stack_is_bounded() {
    // Every recursive frame retains its own large string, so collecting cannot help.
    let source = format!(
        "function keep(n:number):void{{const half={:?};const value=half+half;if(n>0){{keep(n-1);}}console.log(value==half+half);}} export function main():void{{keep(120);}}",
        "X".repeat(abi::WASM_PAGE_BYTES / 2)
    );
    let mut guest = Guest::new(&source);
    let error = guest.call().unwrap_err();
    assert!(format!("{error:#}").contains("unreachable"), "{error:#}");
    assert!(guest.memory_size() <= abi::MAX_MEMORY_BYTES);
    assert!(guest.store.data().logs.is_empty());

    let mut source = "function deep(n:number):void{const owned=\"a\"+\"b\";".to_owned();
    for i in 0..512 {
        source.push_str(&format!("const s{i}=\"static\";"));
    }
    source.push_str("if(n>0){deep(n-1);}} export function main():void{deep(20);console.log(\"AFTER_OVERFLOW\");}");
    let mut guest = Guest::new(&source);
    let error = guest.call().unwrap_err();
    assert!(format!("{error:#}").contains("unreachable"), "{error:#}");
    assert!(guest.store.data().logs.is_empty());
}
