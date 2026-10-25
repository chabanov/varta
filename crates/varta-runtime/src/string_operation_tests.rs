use super::*;
use varta_common::abi;
use wasmtime::{Engine, Instance, Module, Store};

fn fuel_engine() -> Engine {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    Engine::new(&config).unwrap()
}

#[test]
fn string_comparisons_match_utf16_content_order_across_unicode_boundaries() {
    let samples = [
        "",
        "\0",
        "a",
        "aa",
        "b",
        "A",
        "\u{7f}",
        "\u{80}",
        "\u{7ff}",
        "\u{800}",
        "\u{d7ff}",
        "\u{e000}",
        "\u{efff}",
        "\u{f000}",
        "\u{ffff}",
        "\u{10000}",
        "\u{10ffff}",
        "é",
        "e\u{301}",
        "🙂",
        "x🙂",
        "x\u{e000}",
    ];
    let pairs: Vec<_> = samples
        .iter()
        .flat_map(|a| samples.iter().map(move |b| (*a, *b)))
        .collect();
    let runtime = VartaRuntime::new();
    for chunk in pairs.chunks(64) {
        let mut source = "function compare(a:string,b:string):void{console.log(a==b);console.log(a!=b);console.log(a===b);console.log(a!==b);console.log(a<b);console.log(a<=b);console.log(a>b);console.log(a>=b);} export function main():void{".to_owned();
        let mut expected = Vec::new();
        for (a, b) in chunk {
            source.push_str(&format!("compare({a:?},{b:?});"));
            let order = a.encode_utf16().cmp(b.encode_utf16());
            for value in [
                a == b,
                a != b,
                a == b,
                a != b,
                order.is_lt(),
                order.is_le(),
                order.is_gt(),
                order.is_ge(),
            ] {
                expected.push(format!("varta log: {}", i32::from(value)));
            }
        }
        source.push('}');
        let bytes = varta_compiler::compile_typescript(&source).unwrap();
        Module::new(&Engine::default(), &bytes).unwrap();
        assert_eq!(runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines, expected);
    }
}

#[test]
fn concatenation_preserves_aliases_nested_calls_utf8_and_empty_values() {
    let source = r#"
function join(a:string,b:string):string{return a+b;}
export function main():void{
 let a="Привіт";const saved=a;const b="🦀";
 const combined=join(a," ")+join(b,"!");a="changed";
 console.log(combined);console.log(saved);console.log(a);
 console.log((""+combined)==(combined+""));console.log(""+"");
 console.log(("a"+"b")+("c"+"d"));console.log("a\0"+"b");
 const denied=varta.result.capture(varta.env.get("DENIED"));
 const value=denied.value+"fallback";const equal=value=="fallback";
 console.log(varta.result.status());console.log(denied.status);console.log(equal);
}
"#;
    let bytes = varta_compiler::compile_typescript(source).unwrap();
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
            "varta log: Привіт 🦀!",
            "varta log: Привіт",
            "varta log: changed",
            "varta log: 1",
            "varta log: ",
            "varta log: abcd",
            "varta log: a\0b",
            "varta log: 1",
            "varta log: 1",
            "varta log: 1"
        ]
    );
}

#[test]
fn concatenation_enforces_the_per_value_limit_and_empty_concats_do_not_allocate() {
    use crate::reclamation_tests::Guest;
    let mut baseline = Guest::new(
        "export function main():void{let s=\"X\";for(let i=0;i<20;i=i+1){s=s+s;}console.log(s);}",
    );
    baseline.call().unwrap();
    for (iterations, success) in [(20, true), (21, false)] {
        let source = format!(
            "export function main():void{{let s=\"X\";for(let i=0;i<{iterations};i=i+1){{s=s+s;}}for(let i=0;i<100;i=i+1){{s=\"\"+s;s=s+\"\";}}console.log(s);}}"
        );
        let mut guest = Guest::new(&source);
        let result = guest.call();
        assert_eq!(result.is_ok(), success, "{result:?}");
        if let Err(error) = result {
            assert!(format!("{error:#}").contains("unreachable"), "{error:#}");
        }
        if success {
            assert_eq!(guest.logs()[0].len(), abi::MAX_STRING_BYTES);
            assert!(guest.logs()[0].bytes().all(|byte| byte == b'X'));
            assert_eq!(guest.memory_size(), baseline.memory_size());
        } else {
            assert!(guest.logs().is_empty());
        }
        assert!(guest.memory_size() <= abi::MAX_MEMORY_BYTES);
    }
}

#[test]
fn concatenation_reuses_dead_loop_values_without_overwriting_static_or_host_memory() {
    let engine = fuel_engine();
    let capacity = (abi::MAX_MEMORY_BYTES - abi::OWNED_STRING_BASE) / abi::WASM_PAGE_BYTES;
    for (copies, success) in [(capacity, true), (capacity * 2, true)] {
        let source = format!(
            "export function main():void{{const half={:?};for(let i=0;i<{copies};i=i+1){{const s=half+half;}}}}",
            "X".repeat(abi::WASM_PAGE_BYTES / 2)
        );
        let bytes = varta_compiler::compile_typescript(&source).unwrap();
        let module = Module::new(&engine, &bytes).unwrap();
        let mut store = Store::new(&engine, ());
        store.set_fuel((abi::MAX_MEMORY_BYTES * 2) as u64).unwrap();
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let result = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .unwrap()
            .call(&mut store, ());
        assert_eq!(result.is_ok(), success, "{result:?}");
        if let Err(error) = result {
            assert!(format!("{error:#}").contains("unreachable"), "{error:#}");
        }
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        assert!(memory.data_size(&store) <= abi::OWNED_STRING_BASE + 2 * abi::WASM_PAGE_BYTES);
        let data = memory.data(&store);
        assert!(
            data[..abi::WASM_PAGE_BYTES / 2]
                .iter()
                .all(|byte| *byte == b'X')
        );
        assert!(
            data[abi::RESULT_BASE..abi::STRING_ROOT_BASE]
                .iter()
                .all(|byte| *byte == 0)
        );
    }
}

#[test]
fn string_allocation_failure_and_long_comparisons_remain_bounded() {
    let engine = fuel_engine();
    let bytes =
        varta_compiler::compile_typescript("export function main():void{let s=\"a\"+\"b\";}")
            .unwrap();
    let module = Module::new(&engine, &bytes).unwrap();
    let limits = wasmtime::StoreLimitsBuilder::new()
        .memory_size(abi::OWNED_STRING_BASE)
        .build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|limits| limits);
    store.set_fuel(abi::FUEL).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let error = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .unwrap()
        .call(&mut store, ())
        .unwrap_err();
    assert!(format!("{error:#}").contains("unreachable"), "{error:#}");
    assert_eq!(
        instance
            .get_memory(&mut store, "memory")
            .unwrap()
            .data_size(&store),
        abi::OWNED_STRING_BASE
    );

    let source = format!(
        "export function main():void{{const base={:?};const a=base+\"a\";const b=base+\"b\";const lower=a<b;}}",
        "X".repeat(abi::WASM_PAGE_BYTES / 2 - 1)
    );
    let bytes = varta_compiler::compile_typescript(&source).unwrap();
    let error = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap_err();
    assert!(error.to_string().contains("fuel"), "{error}");
    let module = Module::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel((abi::MAX_MEMORY_BYTES * 2) as u64).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    assert_eq!(
        instance
            .get_memory(&mut store, "memory")
            .unwrap()
            .data_size(&store),
        abi::OWNED_STRING_BASE + 2 * abi::WASM_PAGE_BYTES
    );
}
