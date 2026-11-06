use super::*;
use wasm_encoder::{
    CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection, GlobalSection,
    MemorySection, MemoryType, Module, TypeSection,
};
use wasmtime::{Config, Engine, Global, Instance, Mutability, Store, Val};

fn module(body: &[I<'_>]) -> Vec<u8> {
    module_with_resources(body, Some(4), None)
}
fn module_with_resources(
    body: &[I<'_>],
    memory_max: Option<u64>,
    table: Option<(u64, Option<u64>)>,
) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    if let Some((minimum, maximum)) = table {
        let mut tables = wasm_encoder::TableSection::new();
        tables.table(wasm_encoder::TableType {
            element_type: wasm_encoder::RefType::FUNCREF,
            minimum,
            maximum,
            table64: false,
            shared: false,
        });
        module.section(&tables);
    }
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: memory_max,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(7),
    );
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, 0);
    exports.export("memory", ExportKind::Memory, 0);
    module.section(&exports);
    module.section(&wasm_encoder::DataCountSection { count: 1 });
    let mut function = Function::new([(1, ValType::I32)]);
    for op in body {
        function.instruction(op);
    }
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    let mut data = wasm_encoder::DataSection::new();
    data.passive([1, 2, 3, 4, 5, 6, 7]);
    module.section(&data);
    module.finish()
}
fn compare(bytes: &[u8], budgets: &[u64]) {
    compare_with_limit(bytes, budgets, varta_common::abi::MAX_MEMORY_BYTES);
}
fn compare_with_limit(bytes: &[u8], budgets: &[u64], memory_bytes: usize) {
    let mut native_config = Config::new();
    native_config.consume_fuel(true).memory_init_cow(false);
    let native = Engine::new(&native_config).unwrap();
    let native_module = wasmtime::Module::new(&native, bytes).unwrap();
    let browser = Engine::default();
    let metered = instrument_with_memory_limit(bytes, memory_bytes).unwrap();
    let browser_module = wasmtime::Module::new(&browser, &metered).unwrap();
    for &budget in budgets {
        let mut ns = Store::new(
            &native,
            wasmtime::StoreLimitsBuilder::new()
                .memory_size(memory_bytes)
                .build(),
        );
        ns.limiter(|limits| limits);
        ns.set_fuel(u64::MAX).unwrap();
        let ni = Instance::new(&mut ns, &native_module, &[]).unwrap();
        ns.set_fuel(budget).unwrap();
        let nr = ni
            .get_typed_func::<(), i32>(&mut ns, "run")
            .unwrap()
            .call(&mut ns, ());
        let mut bs = Store::new(&browser, ());
        let fuel = Global::new(
            &mut bs,
            wasmtime::GlobalType::new(wasmtime::ValType::I64, Mutability::Var),
            Val::I64(-(budget as i64)),
        )
        .unwrap();
        let bi = Instance::new(&mut bs, &browser_module, &[fuel.into()]).unwrap();
        let br = bi
            .get_typed_func::<(), i32>(&mut bs, "run")
            .unwrap()
            .call(&mut bs, ());
        assert_eq!(
            nr.is_ok(),
            br.is_ok(),
            "budget {budget}: native {nr:?}, browser {br:?}"
        );
        if let (Ok(n), Ok(b)) = (nr, br) {
            assert_eq!(n, b);
        }
        let remaining = fuel.get(&mut bs).i64().unwrap().saturating_neg().max(0) as u64;
        assert_eq!(ns.get_fuel().unwrap(), remaining, "budget {budget}");
        assert_eq!(
            ni.get_memory(&mut ns, "memory").unwrap().data(&ns),
            bi.get_memory(&mut bs, "memory").unwrap().data(&bs),
            "memory after budget {budget}"
        );
    }
}

#[test]
fn scalar_control_flow_and_function_label_branches_match_native_fuel() {
    for body in [
        vec![I::I32Const(42), I::End],
        vec![I::I32Const(11), I::Br(0), I::I32Const(99), I::End],
        vec![I::I32Const(11), I::Return, I::I32Const(99), I::End],
        vec![
            I::I32Const(7),
            I::I32Const(1),
            I::BrIf(0),
            I::Drop,
            I::I32Const(99),
            I::End,
        ],
        vec![
            I::I32Const(7),
            I::I32Const(0),
            I::BrIf(0),
            I::Drop,
            I::I32Const(99),
            I::End,
        ],
        vec![
            I::I32Const(0),
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(11),
            I::Else,
            I::I32Const(22),
            I::End,
            I::End,
        ],
        vec![
            I::I32Const(1),
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(11),
            I::Else,
            I::I32Const(22),
            I::End,
            I::End,
        ],
    ] {
        compare(&module(&body), &[0, 1, 2, 3, 4, 5, 1000]);
    }
}
#[test]
fn loop_checks_match_native_fuel_exhaustion() {
    let body = [
        I::I32Const(5),
        I::LocalSet(0),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(0),
        I::I32Eqz,
        I::BrIf(1),
        I::LocalGet(0),
        I::I32Const(1),
        I::I32Sub,
        I::LocalSet(0),
        I::Br(0),
        I::End,
        I::End,
        I::LocalGet(0),
        I::End,
    ];
    compare(&module(&body), &(0..55).collect::<Vec<_>>());
}
#[test]
fn bulk_memory_charges_bytes_before_work_and_preserves_native_boundaries() {
    for size in [0, 1, 20, 128, 129, 1024] {
        compare(
            &module(&[
                I::I32Const(0),
                I::I32Const(65),
                I::I32Const(size),
                I::MemoryFill(0),
                I::I32Const(5),
                I::End,
            ]),
            &[0, 1, 2, 3, 4, 5, 10, 100, 2000],
        );
    }
    compare(
        &module(&[
            I::I32Const(0),
            I::I32Const(65),
            I::GlobalGet(0),
            I::MemoryFill(0),
            I::I32Const(5),
            I::End,
        ]),
        &(0..20).collect::<Vec<_>>(),
    );
    compare(
        &module(&[
            I::I32Const(20),
            I::LocalSet(0),
            I::I32Const(0),
            I::I32Const(65),
            I::LocalGet(0),
            I::MemoryFill(0),
            I::I32Const(5),
            I::End,
        ]),
        &[2, 3, 10, 100],
    );
}
#[test]
fn bulk_size_ssa_tracks_branch_results_and_loop_phis() {
    let fill = [
        I::I32Const(0),
        I::I32Const(65),
        I::LocalGet(0),
        I::MemoryFill(0),
    ];
    for prefix in [
        vec![
            I::I32Const(20),
            I::LocalSet(0),
            I::GlobalGet(0),
            I::If(BlockType::Empty),
            I::Nop,
            I::Else,
            I::Nop,
            I::End,
        ],
        vec![
            I::GlobalGet(0),
            I::If(BlockType::Empty),
            I::I32Const(20),
            I::LocalSet(0),
            I::Else,
            I::I32Const(20),
            I::LocalSet(0),
            I::End,
        ],
        vec![
            I::Block(BlockType::Result(ValType::I32)),
            I::I32Const(20),
            I::Br(0),
            I::End,
            I::LocalSet(0),
        ],
        vec![
            I::Block(BlockType::Empty),
            I::I32Const(20),
            I::LocalSet(0),
            I::GlobalGet(0),
            I::BrIf(0),
            I::I32Const(30),
            I::LocalSet(0),
            I::End,
        ],
        vec![
            I::GlobalGet(0),
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(20),
            I::Else,
            I::I32Const(30),
            I::End,
            I::LocalSet(0),
        ],
    ] {
        let mut body = prefix;
        body.extend(fill.clone());
        body.extend([I::I32Const(5), I::End]);
        compare(&module(&body), &(0..55).collect::<Vec<_>>());
    }
    // The local is read while the loop header is unsealed: even its first
    // constant value must use the native dynamic-size check inside the loop.
    let mut body = vec![I::I32Const(20), I::LocalSet(0), I::Loop(BlockType::Empty)];
    body.extend(fill);
    body.extend([I::End, I::I32Const(5), I::End]);
    compare(&module(&body), &(0..55).collect::<Vec<_>>());
}

#[test]
fn trap_paths_preserve_native_observable_fuel() {
    for body in [
        vec![
            I::I32Const(0),
            I::I32Load(wasm_encoder::MemArg {
                offset: 262144,
                align: 2,
                memory_index: 0,
            }),
            I::End,
        ],
        vec![I::I32Const(1), I::I32Const(0), I::I32DivS, I::End],
        vec![
            I::I32Const(65536),
            I::I32Load(wasm_encoder::MemArg {
                offset: 0,
                align: 2,
                memory_index: 0,
            }),
            I::End,
        ],
        vec![
            I::I32Const(-1),
            I::I32Load(wasm_encoder::MemArg {
                offset: 0,
                align: 2,
                memory_index: 0,
            }),
            I::End,
        ],
        vec![I::Unreachable, I::End],
    ] {
        compare(&module(&body), &[0, 1, 2, 5, 100]);
    }
}

/// Fixed-width SIMD (v38 `simd128`, the compiled JSON decoders' scanner):
/// one unit per operator, a loop of vector steps and an out-of-bounds
/// 16-byte load (a static offset beyond memory, and one straddling its end)
/// meter exactly as native fuel.
#[test]
fn simd128_operators_match_native_fuel() {
    let vector = |offset| wasm_encoder::MemArg {
        offset,
        align: 0,
        memory_index: 0,
    };
    let steps = vec![
        I::Loop(BlockType::Empty),
        I::LocalGet(0),
        I::V128Load(vector(0)),
        I::LocalGet(0),
        I::V128Load(vector(16)),
        I::I8x16NarrowI16x8S,
        I::V128Const(i128::from_le_bytes([b'\\'; 16])),
        I::I8x16Eq,
        I::I8x16Bitmask,
        I::Drop,
        I::LocalGet(0),
        I::V128Load(vector(0)),
        I::V128AnyTrue,
        I::Drop,
        I::LocalGet(0),
        I::I32Const(32),
        I::I32Add,
        I::LocalTee(0),
        I::I32Const(1024),
        I::I32LtU,
        I::BrIf(0),
        I::End,
        I::LocalGet(0),
        I::End,
    ];
    compare(
        &module(&steps),
        &[0, 1, 2, 3, 10, 17, 18, 19, 100, 500, 640, 641, 642, 1000],
    );
    for body in [
        vec![
            I::I32Const(0),
            I::V128Load(vector(262144)),
            I::V128AnyTrue,
            I::End,
        ],
        vec![
            I::I32Const(65528),
            I::V128Load(vector(0)),
            I::V128AnyTrue,
            I::End,
        ],
    ] {
        compare(&module(&body), &[0, 1, 2, 3, 100]);
    }
}

/// Relaxed SIMD is outside the fuel model (the reference engine disables
/// it: its results differ between hosts).
#[test]
fn relaxed_simd_is_rejected() {
    let body = [
        I::V128Const(0),
        I::V128Const(0),
        I::I8x16RelaxedSwizzle,
        I::V128AnyTrue,
        I::End,
    ];
    assert!(
        instrument(&module(&body))
            .unwrap_err()
            .to_string()
            .contains("outside the implemented fuel model")
    );
}

#[test]
fn bulk_copy_init_and_memory_growth_match_native() {
    for body in [
        vec![
            I::I32Const(0),
            I::I32Const(3),
            I::GlobalGet(0),
            I::MemoryCopy {
                src_mem: 0,
                dst_mem: 0,
            },
            I::I32Const(5),
            I::End,
        ],
        vec![
            I::I32Const(0),
            I::I32Const(0),
            I::GlobalGet(0),
            I::MemoryInit {
                mem: 0,
                data_index: 0,
            },
            I::I32Const(5),
            I::End,
        ],
        vec![I::I32Const(1), I::MemoryGrow(0), I::End],
        vec![I::GlobalGet(0), I::MemoryGrow(0), I::End],
    ] {
        compare(&module(&body), &(0..25).collect::<Vec<_>>());
    }
}

#[test]
fn invalid_and_unmodeled_images_fail_before_execution() {
    assert!(instrument(b"not wasm").is_err());
    let mut start = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    start.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    start.section(&functions);
    start.section(&wasm_encoder::StartSection { function_index: 0 });
    let mut code = CodeSection::new();
    let mut function = Function::new([]);
    function.instruction(&I::End);
    code.function(&function);
    start.section(&code);
    assert!(
        instrument(&start.finish())
            .unwrap_err()
            .to_string()
            .contains("start function")
    );
}

#[test]
fn private_meter_images_cannot_be_instrumented_again() {
    let bytes = instrument(&module(&[I::I32Const(42), I::End])).unwrap();
    assert!(
        instrument(&bytes)
            .unwrap_err()
            .to_string()
            .contains("reserved meter")
    );
}

#[test]
fn private_memory_limits_match_native_store_growth_and_never_widen_existing_caps() {
    for maximum in [None, Some(2), Some(100)] {
        let bytes =
            module_with_resources(&[I::I32Const(2), I::MemoryGrow(0), I::End], maximum, None);
        compare_with_limit(
            &bytes,
            &[0, 1, 2, 10, 100],
            3 * varta_common::abi::WASM_PAGE_BYTES,
        );
        let image =
            instrument_with_memory_limit(&bytes, 3 * varta_common::abi::WASM_PAGE_BYTES).unwrap();
        for payload in wasmparser::Parser::new(0).parse_all(&image) {
            if let Payload::MemorySection(memories) = payload.unwrap() {
                for memory in memories {
                    assert_eq!(memory.unwrap().maximum, Some(maximum.unwrap_or(3).min(3)));
                }
            }
        }
    }
}

#[test]
fn private_tables_are_bounded_and_invalid_resource_ceilings_reject() {
    for maximum in [None, Some(16), Some(4096)] {
        let bytes = module_with_resources(&[I::I32Const(1), I::End], None, Some((1, maximum)));
        let image = instrument(&bytes).unwrap();
        wasmtime::Module::new(&Engine::default(), &image).unwrap();
        for payload in wasmparser::Parser::new(0).parse_all(&image) {
            if let Payload::TableSection(tables) = payload.unwrap() {
                for table in tables {
                    assert_eq!(
                        table.unwrap().ty.maximum,
                        Some(
                            maximum
                                .unwrap_or(varta_common::abi::MAX_TABLE_ELEMENTS as u64)
                                .min(varta_common::abi::MAX_TABLE_ELEMENTS as u64)
                        )
                    );
                }
            }
        }
    }
    let bytes = module_with_resources(&[I::I32Const(1), I::End], None, Some((2048, None)));
    assert!(instrument(&bytes).is_err());
    let bytes = module(&[I::I32Const(1), I::End]);
    assert!(instrument_with_memory_limit(&bytes, 0).is_err());
    assert!(instrument_with_memory_limit(&bytes, 1).is_err());
    assert!(
        instrument_with_memory_limit(&bytes, varta_common::budget::MAX_MEMORY_BYTES as usize + 1)
            .is_err()
    );
}

#[derive(Default)]
struct Observed {
    meter: Option<Global>,
    calls: Vec<(u64, String)>,
}
fn run_program(bytes: &[u8], browser: bool) -> (u64, Vec<(u64, String)>) {
    use wasmtime::{Caller, Linker};
    const FUEL: u64 = 10_000_000;
    let mut config = Config::new();
    config.consume_fuel(!browser).memory_init_cow(false);
    let engine = Engine::new(&config).unwrap();
    let bytes = if browser {
        instrument(bytes).unwrap()
    } else {
        bytes.to_vec()
    };
    let module = wasmtime::Module::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, Observed::default());
    let mut linker = Linker::new(&engine);
    if browser {
        let global = Global::new(
            &mut store,
            wasmtime::GlobalType::new(wasmtime::ValType::I64, Mutability::Var),
            Val::I64(-(FUEL as i64)),
        )
        .unwrap();
        store.data_mut().meter = Some(global);
        linker.define(&store, MODULE, COUNTER, global).unwrap();
    } else {
        store.set_fuel(u64::MAX).unwrap();
    }
    linker
        .func_wrap(
            "varta",
            "log_string16",
            |mut caller: Caller<'_, Observed>, ptr: i32, len: i32| -> wasmtime::Result<()> {
                let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                let units = memory.data(&caller)[ptr as usize..(ptr + len) as usize]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| u16::from_le_bytes([s[0], s[1]]))
                    .collect::<Vec<_>>();
                let text = String::from_utf16(&units).unwrap();
                let remaining = match caller.data().meter {
                    Some(global) => global
                        .get(&mut caller)
                        .i64()
                        .unwrap()
                        .saturating_neg()
                        .max(0) as u64,
                    None => caller.get_fuel()?,
                };
                let cost = varta_common::fuel::CALL
                    + varta_common::fuel::work(len as usize, varta_common::fuel::BYTE);
                let left = remaining.checked_sub(cost).expect("test host charge fits");
                match caller.data().meter {
                    Some(global) => global.set(&mut caller, Val::I64(-(left as i64)))?,
                    None => caller.set_fuel(left)?,
                }
                caller.data_mut().calls.push((FUEL - remaining, text));
                Ok(())
            },
        )
        .unwrap();
    let instance = linker.instantiate(&mut store, &module).unwrap();
    if !browser {
        store.set_fuel(FUEL).unwrap();
    }
    instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    let remaining = match store.data().meter {
        Some(global) => global
            .get(&mut store)
            .i64()
            .unwrap()
            .saturating_neg()
            .max(0) as u64,
        None => store.get_fuel().unwrap(),
    };
    (
        FUEL - remaining,
        std::mem::take(&mut store.data_mut().calls),
    )
}
#[test]
fn real_v38_linked_intrinsics_recursion_and_host_charges_match_native() {
    let bytes = crate::compile_typescript_with_profile(
        r#"
      function fib(n:number):number{if(n<2)return n;return fib(n-1)+fib(n-2);}
      export function main():void{
        const values:number[]=[];for(let i=0;i<20;i++)values.push(fib(i%10));
        console.log(JSON.stringify({message:"Привіт 😀",values:values}));
        console.log("x".repeat(300));
        console.log(String(12345678901234567890n * 12345n));
      }
    "#,
        varta_common::language::LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap();
    let native = run_program(&bytes, false);
    let browser = run_program(&bytes, true);
    assert_eq!(native, browser);
    assert_eq!(native.1.len(), 3);
}

/// The compiled JSON decoders' `simd128` scanner and string builder on long
/// strings with escapes, non-ASCII text, indentation and a rejected input:
/// the metered image charges exactly the native fuel.
#[test]
fn real_v38_simd_json_decoders_match_native() {
    let bytes = crate::compile_typescript_with_profile(
        r#"
      interface Event { role: string; text: string; tags: string[] }
      export function main():void{
        let t = "[\n";
        for (let i = 0; i < 24; i++) {
          if (i > 0) t += ",\n";
          t += '  { "role": "assistant", "text": "line ' + String(i) + '\\n\\"quoted\\" Привіт 😀 \\u00e9' + "x".repeat(i * 7) + '", "tags": [ "a" , "b\\\\c" ] }';
        }
        t += "\n]";
        const events = JSON.parse(t) as Event[];
        console.log(String(events.length) + events[23].text.slice(0, 30));
        try { JSON.parse(t + "x") as Event[]; } catch (e) { console.log("rejected"); }
      }
    "#,
        varta_common::language::LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap();
    let native = run_program(&bytes, false);
    let browser = run_program(&bytes, true);
    assert_eq!(native, browser);
    assert_eq!(native.1.len(), 2);
}

#[test]
fn unsupported_memory64_rejects_before_potentially_overflowing_byte_arithmetic() {
    let mut module = Module::new();
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 0,
        maximum: Some(1_u64 << 48),
        memory64: true,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let bytes = module.finish();
    wasmparser::Validator::new().validate_all(&bytes).unwrap();
    assert!(
        instrument(&bytes)
            .unwrap_err()
            .to_string()
            .contains("64-bit")
    );
}
