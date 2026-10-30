//! V38 `linked_intrinsics`: the measured shadow-stack depth and arena use of
//! every linked operation family over adversarial inputs, pinning
//! `abi::INTRINSIC_STACK_BYTES` and `abi::INTRINSIC_ARENA_BYTES`.
//!
//! The region's stack and reserved arena are painted before `_start` and
//! scanned afterwards. For the operations whose scratch is bounded (their
//! inputs are Numbers or bounded prefixes), the linked `arena_base`
//! accessor is redirected to a high address, so scratch that overflows the
//! reserved arena lands there and is measured too; operations whose scratch
//! grows with their input (case mapping, long parses, BigInt) are measured
//! for stack depth only, as their scratch continues above the heap by
//! design.
use super::*;
use crate::contract_tests::rewrite;
use varta_common::language::LanguageProfile as P;

const V38: P = P::CheckedF64Utf16V38;
const PAINT: u8 = 0xa5;
/// Where redirected overflow scratch starts: above every guest heap of the
/// bounded programs, below the memory budget.
const OVERFLOW_BASE: usize = abi::MAX_MEMORY_BYTES / 2;
/// The margin the constants keep over the measured maximum: twice the
/// measured stack depth and twice the measured bounded scratch.
const MARGIN: usize = 2;

/// Bounded scratch: Number arguments or a parse that stops early.
const BOUNDED: &[(&str, &str)] = &[
    (
        "Math",
        "const xs: number[] = [0.5, -0.5, 1e308, -1e-308, 5e-324, 710, -745, 1e-10];
         let r = 0;
         for (const x of xs) {
           r = r + Math.sin(x) + Math.cos(x) + Math.tan(x) + Math.asin(x / 1e308) + Math.acos(x / 1e308);
           r = r + Math.atan(x) + Math.atan2(x, 3) + Math.sinh(x / 1e306) + Math.cosh(x / 1e306) + Math.tanh(x);
           r = r + Math.asinh(x) + Math.acosh(x + 2) + Math.atanh(x / 1e308) + Math.cbrt(x) + Math.exp(x / 1e306);
           r = r + Math.expm1(x / 1e306) + Math.log(x) + Math.log10(x) + Math.log2(x) + Math.log1p(x) + x ** 0.5 + (x % 7);
         }
         console.log(r);",
    ),
    (
        "Number decimal text",
        "const xs: number[] = [1.7976931348623157e308, 5e-324, -2.2250738585072014e-308, 123456789.98765433, 1e21, 0.1];
         for (const x of xs) {
           console.log(x.toFixed(100).length + x.toExponential(100).length + x.toPrecision(100).length);

         }",
    ),
    (
        "Number radix text, largest",
        "const x = 1.7976931348623157e308;
         console.log(x.toString(2).length + x.toString(3).length + x.toString(7).length + x.toString(36).length);",
    ),
    (
        "Number radix text, smallest normal",
        "const x = -2.2250738585072014e-308;
         console.log(x.toString(2).length + x.toString(3).length + x.toString(7).length + x.toString(36).length);",
    ),
    (
        "Number radix text, subnormal",
        "const x = 5e-324;
         console.log(x.toString(2).length + x.toString(3).length + x.toString(7).length + x.toString(36).length);",
    ),
    (
        "Number radix text, fractions",
        "const xs: number[] = [123456789.98765433, 1e21, 0.1];
         for (const x of xs) {
           console.log(x.toString(2).length + x.toString(3).length + x.toString(7).length + x.toString(36).length);
         }",
    ),
    (
        "parse prefix",
        "const ts: string[] = ['0x' + 'f'.repeat(300), '1'.repeat(400) + 'e-400', '.' + '9'.repeat(800), '  -Infinityx', 'z'.repeat(2000)];
         for (const t of ts) { console.log(parseInt(t) + parseInt(t, 2) + parseInt(t, 36) + parseFloat(t)); }",
    ),
    (
        "BigInt from Number",
        "console.log(BigInt(1.7976931348623157e308).toString(36).length + BigInt(-9007199254740991).toString(2).length);",
    ),
];

/// Scratch that grows with the input; measured for stack depth.
const PROPORTIONAL: &[(&str, &str)] = &[
    (
        "case mapping",
        "let t = 'ﬃßΣa'; for (let i = 0; i < 14; i++) { t = t + t; }
         console.log(t.toUpperCase().length + t.toLowerCase().length);",
    ),
    (
        "long parse",
        "const t = '7'.repeat(100000) + '.5e-99999';
         console.log(parseFloat(t) + parseInt(t, 10) + Number('0b' + '1'.repeat(1023)));",
    ),
    (
        "BigInt multiply and divide, 300000-bit operands",
        "const a = (1n << 300000n) - 12345n;
         const b = (1n << 330000n) / 3n;
         const p = a * b;
         console.log(Number(p >> 600000n) + Number(p % 1000000007n) + Number((p / a) - b));",
    ),
    (
        "BigInt decimal and radix text",
        "console.log(String((1n << 300000n) - 1n).length + (3n ** 50000n).toString(36).length);",
    ),
    (
        "BigInt widths and parsing",
        "const p = (1n << 2000000n) / 7n;
         console.log(BigInt.asIntN(1500000, p).toString(16).length + Number(BigInt('9'.repeat(30000)) % 97n));",
    ),
];

/// The intrinsic region of a linked module: its base and the reserved
/// arena's start.
fn region(bytes: &[u8]) -> (usize, usize, usize) {
    let mut region = abi::StaticRegion::SINGLE_PAGE;
    let mut intrinsics = None;
    let mut data_end = 0;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            wasmparser::Payload::CustomSection(section)
                if section.name() == abi::STATIC_SECTION =>
            {
                region = abi::StaticRegion::read_section(section.data()).unwrap();
            }
            wasmparser::Payload::CustomSection(section)
                if section.name() == abi::INTRINSIC_SECTION =>
            {
                intrinsics = Some(section.data().to_vec());
            }
            wasmparser::Payload::DataSection(reader) => {
                for data in reader {
                    let data = data.unwrap();
                    let wasmparser::DataKind::Active { offset_expr, .. } = data.kind else {
                        continue;
                    };
                    let Ok(wasmparser::Operator::I32Const { value }) =
                        offset_expr.get_operators_reader().read()
                    else {
                        continue;
                    };
                    data_end = data_end.max(value as u32 as usize + data.data.len());
                }
            }
            _ => {}
        }
    }
    let region = region.read_intrinsic_section(&intrinsics.unwrap()).unwrap();
    let base = region.intrinsic_base();
    let data_end = data_end.clamp(base + abi::INTRINSIC_STACK_BYTES, region.heap_base());
    (base, data_end.next_multiple_of(16), region.heap_base())
}

/// `bytes` with the linked `arena_base` accessor (`global.get` of the
/// allocator's heap-end global) returning [`OVERFLOW_BASE`] instead.
fn redirect_overflow(bytes: &[u8], heap: usize) -> Vec<u8> {
    fn leb(out: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            out.push(byte | if value != 0 { 0x80 } else { 0 });
            if value == 0 {
                break;
            }
        }
    }
    // The heap-end global: mutable i32, initialized to the heap base.
    let mut heap_base = 0;
    let mut globals = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            wasmparser::Payload::GlobalSection(reader) => {
                for global in reader {
                    let global = global.unwrap();
                    let init = match global.init_expr.get_operators_reader().read() {
                        Ok(wasmparser::Operator::I32Const { value }) => value as u32 as usize,
                        _ => usize::MAX,
                    };
                    globals.push((global.ty.mutable, init));
                }
            }
            wasmparser::Payload::CustomSection(section)
                if section.name() == abi::INTRINSIC_SECTION =>
            {
                heap_base = 1;
            }
            _ => {}
        }
    }
    assert_eq!(heap_base, 1, "a linked module");
    let mut replaced = 0;
    let out = rewrite(bytes, |id, data| {
        if id != 10 {
            return vec![data.to_vec()];
        }
        let mut reader = wasmparser::BinaryReader::new(data, 0);
        let count = reader.read_var_u32().unwrap();
        let mut section = Vec::new();
        leb(&mut section, u64::from(count));
        for _ in 0..count {
            let size = reader.read_var_u32().unwrap() as usize;
            let body = reader.read_bytes(size).unwrap();
            let parsed = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(body, 0));
            let operators: Vec<_> = parsed
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect();
            let accessor = parsed.get_locals_reader().unwrap().get_count() == 0
                && matches!(
                    operators.as_slice(),
                    [wasmparser::Operator::GlobalGet { global_index }, wasmparser::Operator::End]
                        if globals.get(*global_index as usize).is_some_and(|g| g.0)
                )
                || matches!(
                    operators.as_slice(),
                    [wasmparser::Operator::I32Const { value }, wasmparser::Operator::End]
                        if *value as u32 as usize == heap
                );
            if accessor {
                replaced += 1;
                let mut patched = vec![0x00, 0x41];
                let mut value = OVERFLOW_BASE as i64;
                loop {
                    let byte = (value & 0x7f) as u8;
                    value >>= 7;
                    let done =
                        (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
                    patched.push(byte | if done { 0 } else { 0x80 });
                    if done {
                        break;
                    }
                }
                patched.push(0x0b);
                leb(&mut section, patched.len() as u64);
                section.extend(patched);
            } else {
                leb(&mut section, size as u64);
                section.extend_from_slice(body);
            }
        }
        vec![section]
    });
    // Programs whose linked code never allocates link no accessor.
    assert!(replaced <= 1, "at most one arena_base accessor");
    out
}

#[derive(Debug, Default, Clone, Copy)]
struct Footprint {
    stack: usize,
    reserved: usize,
    overflow: usize,
}

/// Run `source` and measure the stack depth, reserved-arena use and (when
/// `redirect`) the overflow scratch.
fn footprint(source: &str, redirect: bool) -> Footprint {
    let source = format!("export function main(): void {{ {source} }}");
    let bytes = varta_compiler::compile_typescript_with_profile(&source, V38)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    let (base, arena, heap) = region(&bytes);
    let bytes = if redirect {
        redirect_overflow(&bytes, heap)
    } else {
        bytes
    };
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let budgets = varta_common::budget::Budgets {
        fuel_per_turn: 100_000 * abi::FUEL,
        host_calls_per_turn: u32::MAX,
        ..varta_common::budget::Budgets::PRODUCTION
    };
    let mut execution =
        crate::execution::Execution::with_test_budgets(&runtime.engine, budgets).unwrap();
    execution.allow_test_wall_clock(std::time::Duration::from_secs(600));
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let memory = instance.get_memory(&mut *store, "memory").unwrap();
    let stack = base..base + abi::INTRINSIC_STACK_BYTES;
    let reserved = arena..arena + abi::INTRINSIC_ARENA_BYTES;
    memory.data_mut(&mut *store)[stack.clone()].fill(PAINT);
    memory.data_mut(&mut *store)[reserved.clone()].fill(PAINT);
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start
        .call(&mut *store, &[], &mut [])
        .map_err(|e| crate::resources::classify(&instance, &mut *store, RuntimeError::from(e)))
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    let data = memory.data(&*store);
    // The stack grows down from its top; the reserved arena up from its start.
    let stack_low = data[stack.clone()]
        .iter()
        .position(|&b| b != PAINT)
        .unwrap_or(stack.len());
    let reserved_high = data[reserved.clone()]
        .iter()
        .rposition(|&b| b != PAINT)
        .map_or(0, |at| at + 1);
    let overflow = if redirect {
        data.get(OVERFLOW_BASE..)
            .and_then(|tail| tail.iter().rposition(|&b| b != 0))
            .map_or(0, |at| at + 1)
    } else {
        0
    };
    Footprint {
        stack: stack.len() - stack_low,
        reserved: reserved_high,
        overflow,
    }
}

#[test]
fn linked_operations_fit_the_region_stack_and_reserved_arena_with_margin() {
    let mut stack = 0;
    let mut scratch = 0;
    for (name, source) in BOUNDED {
        let used = footprint(source, true);
        println!("{name}: {used:?}");
        assert_eq!(
            used.overflow, 0,
            "{name}: bounded scratch overflowed the reserved arena"
        );
        stack = stack.max(used.stack);
        scratch = scratch.max(used.reserved);
    }
    for (name, source) in PROPORTIONAL.iter().rev() {
        let used = footprint(source, false);
        println!("{name}: {used:?}");
        stack = stack.max(used.stack);
    }
    println!("maximum stack {stack} bytes, maximum bounded scratch {scratch} bytes");
    assert!(
        stack * MARGIN <= abi::INTRINSIC_STACK_BYTES,
        "linked code needs {stack} stack bytes: keep INTRINSIC_STACK_BYTES at least {MARGIN}x"
    );
    assert!(
        scratch * MARGIN <= abi::INTRINSIC_ARENA_BYTES,
        "bounded linked scratch needs {scratch} bytes: keep INTRINSIC_ARENA_BYTES at least {MARGIN}x"
    );
}
