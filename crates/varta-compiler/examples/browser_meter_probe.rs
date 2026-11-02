//! Generate paired private images/native observations for the explicit Node
//! differential probe. This is verification tooling, not a packaging command.
use std::path::PathBuf;
use varta_common::{fuel, language::LanguageProfile};
use wasmtime::{Caller, Config, Engine, Linker, Module, Store, Trap};

const BUDGETS: &[u64] = &[0, 1, 2, 50, 100, 2000, 8_000_000];

fn take(caller: &mut Caller<'_, Vec<String>>, amount: u64) -> wasmtime::Result<()> {
    if let Some(left) = caller.get_fuel()?.checked_sub(amount) {
        caller.set_fuel(left)
    } else {
        caller.set_fuel(0)?;
        Err(Trap::OutOfFuel.into())
    }
}

fn main() -> wasmtime::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("usage: browser_meter_probe OUTPUT_DIRECTORY"))?;
    std::fs::create_dir_all(&path)?;
    let source = r#"
        function fib(n:number):number {if(n<2)return n;return fib(n-1)+fib(n-2);}
        export function main():void {
            const values:number[]=[];for(let i=0;i<20;i++)values.push(fib(i%10));
            console.log(JSON.stringify({message:"Привіт 😀",values:values}));
            console.log("x".repeat(300));
            console.log(String(12345678901234567890n * 12345n));
        }
    "#;
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V38,
    )?;
    let metered = varta_compiler::browser_meter::instrument(&bytes)?;
    std::fs::write(path.join("original.wasm"), &bytes)?;
    std::fs::write(path.join("metered.wasm"), &metered)?;
    let mut config = Config::new();
    config.consume_fuel(true).memory_init_cow(false);
    let engine = Engine::new(&config)?;
    let module = Module::new(&engine, &bytes)?;
    let mut linker = Linker::new(&engine);
    linker.func_wrap(
        "varta",
        "log_string16",
        |mut caller: Caller<'_, Vec<String>>, ptr: i32, len: i32| -> wasmtime::Result<()> {
            let before = caller.get_fuel()?;
            take(&mut caller, fuel::CALL)?;
            let memory = caller
                .get_export("memory")
                .and_then(|export| export.into_memory())
                .ok_or_else(|| std::io::Error::other("missing fixture memory"))?;
            let start = usize::try_from(ptr)?;
            let size = usize::try_from(len)?;
            take(&mut caller, fuel::work(size, fuel::BYTE))?;
            let input = memory
                .data(&caller)
                .get(start..start.checked_add(size).ok_or(Trap::MemoryOutOfBounds)?)
                .ok_or(Trap::MemoryOutOfBounds)?;
            let (pairs, odd) = input.as_chunks::<2>();
            if !odd.is_empty() {
                return Err(Trap::MemoryOutOfBounds.into());
            }
            let text = String::from_utf16(
                &pairs
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect::<Vec<_>>(),
            )?;
            caller.data_mut().push(format!("{before}\t{text}"));
            Ok(())
        },
    )?;
    let run = |budget| -> wasmtime::Result<(String, Vec<String>)> {
        let mut store = Store::new(&engine, Vec::new());
        store.set_fuel(u64::MAX)?;
        let instance = linker.instantiate(&mut store, &module)?;
        store.set_fuel(budget)?;
        let result = instance
            .get_typed_func::<(), ()>(&mut store, "_start")?
            .call(&mut store, ());
        let outcome = format!(
            "{}\n{}\n{}",
            result.is_ok(),
            store.get_fuel()?,
            store.data().join("\n")
        );
        Ok((outcome, store.into_data()))
    };
    let baseline_budget = *BUDGETS.last().expect("nonempty probe budget list");
    let (_, baseline_logs) = run(baseline_budget)?;
    let mut budgets = BUDGETS.to_vec();
    // Probe both sides of each call and byte-work charge, including exact
    // payment. Thresholds are observed from native, never browser estimates.
    for line in baseline_logs {
        let (before, text) = line
            .split_once('\t')
            .ok_or_else(|| std::io::Error::other("invalid fixture observation"))?;
        let used = baseline_budget - before.parse::<u64>()?;
        let call = used + fuel::CALL;
        let work = call + fuel::work(text.encode_utf16().count() * 2, fuel::BYTE);
        budgets.extend([used, call - 1, call, work - 1, work, work + 1]);
    }
    budgets.sort_unstable();
    budgets.dedup();
    for &budget in &budgets {
        std::fs::write(path.join(format!("{budget}.golden")), run(budget)?.0)?;
    }
    std::fs::write(
        path.join("budgets.txt"),
        budgets
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    println!(
        "private image: {} -> {} bytes; {} native observations",
        bytes.len(),
        metered.len(),
        budgets.len()
    );
    Ok(())
}
