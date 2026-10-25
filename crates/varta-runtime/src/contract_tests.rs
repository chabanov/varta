use super::*;
use varta_common::{TOOL_SECTION, ToolCatalog, ToolType};
use wasmparser::{Parser, Payload};

const ECHO: &str = r#"
export function echo(text:string):string{return text;}
export function hidden():number{return 7;}
export function main():void{console.log(echo("normal main"));}
varta.tool({name:"text.echo",description:"Echo text"},echo);
"#;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript(source).unwrap()
}
fn catalog(bytes: &[u8]) -> ToolCatalog {
    VartaRuntime::new()
        .validate_module_and_tools(bytes)
        .unwrap()
        .1
        .unwrap()
}

// Rewrite existing portable sections for hostile-artifact fixtures. No native
// payload or executable test generator enters production validation.
fn rewrite(bytes: &[u8], mut edit: impl FnMut(u8, &[u8]) -> Vec<Vec<u8>>) -> Vec<u8> {
    fn length(bytes: &mut Vec<u8>, mut value: usize) {
        loop {
            let next = (value & 0x7f) as u8;
            value >>= 7;
            bytes.push(next | if value != 0 { 0x80 } else { 0 });
            if value == 0 {
                break;
            }
        }
    }
    let mut out = bytes[..8].to_vec();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Some((id, range)) = payload.unwrap().as_section() {
            for data in edit(id, &bytes[range]) {
                out.push(id);
                length(&mut out, data.len());
                out.extend(data);
            }
        }
    }
    out
}
fn replace_catalog(bytes: &[u8], data: &[u8]) -> Vec<u8> {
    rewrite(bytes, |id, section| {
        let mut section = section.to_vec();
        if id == 0 && section.get(1..1 + TOOL_SECTION.len()) == Some(TOOL_SECTION.as_bytes()) {
            section.truncate(1 + TOOL_SECTION.len());
            section.extend_from_slice(data);
        }
        vec![section]
    })
}

fn export_index(bytes: &[u8], name: &str) -> u8 {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ExportSection(reader) = payload.unwrap() {
            for export in reader {
                let export = export.unwrap();
                if export.name == name {
                    return u8::try_from(export.index).unwrap();
                }
            }
        }
    }
    panic!("missing fixture export")
}

#[test]
fn tools_are_validated_and_ordinary_exports_stay_private() {
    let bytes = compile(ECHO);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let metadata = catalog(&bytes);
    assert_eq!(metadata.tools.len(), 1);
    assert_eq!(metadata.tools[0].name, "text.echo");
    assert_eq!(metadata.tools[0].parameters[0].value_type, ToolType::String);
    let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    assert_eq!(output.lines, ["varta log: normal main"]);
    assert!(
        VartaRuntime::new()
            .validate_module_and_tools(&compile("export function main():void{}"))
            .unwrap()
            .1
            .is_none()
    );
}

#[test]
fn transitive_and_recursive_effects_are_recomputed_from_wasm() {
    let bytes = compile(
        r#"
function nested(n:number):boolean{if(n==0){return varta.env.has("NAME");}return nested(n-1);}
export function check(n:number):boolean{
  for(let i=0;i<n;i=i+1){if(nested(i)){return true;}}
  switch(n){case 2: varta.time.now();break;default: varta.env.get("NAME");}
  return false;
}
export function main():void{}
varta.tool({name:"check",description:"Check"},check);
"#,
    );
    assert_eq!(
        catalog(&bytes).tools[0].required_effects,
        ["env.get", "env.has", "time.now"]
    );
    let mut metadata = catalog(&bytes);
    metadata.tools[0].required_effects.clear();
    let bad = replace_catalog(&bytes, &metadata.to_bytes().unwrap());
    assert!(
        VartaRuntime::new()
            .validate_module(&bad)
            .unwrap_err()
            .to_string()
            .contains("effect mismatch")
    );
}

#[test]
fn malformed_duplicate_oversized_and_misbound_catalogs_are_rejected() {
    let bytes = compile(ECHO);
    let runtime = VartaRuntime::new();
    for data in [
        b"{".to_vec(),
        vec![b' '; varta_common::MAX_TOOL_CATALOG_BYTES + 1],
    ] {
        assert!(
            runtime
                .validate_module(&replace_catalog(&bytes, &data))
                .is_err()
        );
    }
    let mut metadata = catalog(&bytes);
    metadata.tools[0].parameters[0].value_type = ToolType::Integer;
    assert!(
        runtime
            .validate_module(&replace_catalog(&bytes, &metadata.to_bytes().unwrap()))
            .unwrap_err()
            .to_string()
            .contains("signature mismatch")
    );
    metadata.tools[0].parameters[0].value_type = ToolType::String;
    metadata.tools[0].name = "missing".into();
    metadata.tools[0].export = "varta.tool.missing".into();
    assert!(
        runtime
            .validate_module(&replace_catalog(&bytes, &metadata.to_bytes().unwrap()))
            .unwrap_err()
            .to_string()
            .contains("missing tool function")
    );
    for action in [0, 1, 2, 3] {
        let bad = rewrite(&bytes, |id, section| {
            let tools =
                id == 0 && section.get(1..1 + TOOL_SECTION.len()) == Some(TOOL_SECTION.as_bytes());
            let abi = id == 0 && section.get(1..10) == Some(b"varta.abi");
            match (action, tools, abi) {
                (0, true, _) | (2, _, true) => vec![],
                (1, true, _) | (3, _, true) => vec![section.to_vec(), section.to_vec()],
                _ => vec![section.to_vec()],
            }
        });
        assert!(runtime.validate_module(&bad).is_err(), "mutation {action}");
    }
}

#[test]
fn string_invocation_abi_copies_and_roots_inputs_across_buffer_reuse_and_collection() {
    let bytes = compile(
        r#"
export function combine(a:string,b:string):string{
  for(let i=0;i<160;i=i+1){let transient=a+b;}
  return a+b;
}
export function main():void{}
varta.tool({name:"combine",description:"Combine"},combine);
"#,
    );
    catalog(&bytes);
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &bytes).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let copy = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, varta_common::TOOL_COPY_STRING)
        .unwrap();
    let retain = instance
        .get_typed_func::<i64, i64>(&mut store, varta_common::TOOL_RETAIN_STRING)
        .unwrap();
    let inputs = ["A".repeat(48 * 1024), "Привіт🦀".repeat(1000)];
    let handles: Vec<_> = inputs
        .iter()
        .map(|input| {
            memory
                .write(&mut store, abi::RESULT_BASE, input.as_bytes())
                .unwrap();
            let value = copy
                .call(&mut store, (abi::RESULT_BASE as i32, input.len() as i32))
                .unwrap();
            retain.call(&mut store, value).unwrap()
        })
        .collect();
    memory
        .write(
            &mut store,
            abi::RESULT_BASE,
            &vec![b'X'; abi::MAX_INPUT_BYTES],
        )
        .unwrap();
    let function = instance
        .get_typed_func::<(i64, i64), i64>(&mut store, "varta.tool.combine")
        .unwrap();
    let result = function.call(&mut store, (handles[0], handles[1])).unwrap() as u64;
    let mut output = vec![0; (result >> i32::BITS) as usize];
    memory
        .read(&store, result as u32 as usize, &mut output)
        .unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), inputs.concat());
}

#[test]
fn integer_boolean_void_and_no_argument_handlers_keep_their_physical_signatures() {
    let bytes = compile(
        r#"
export function choose(n:number,flag:boolean):number{if(flag){return n;}return 0;}
export function positive(n:number):boolean{return n>0;}
export function empty():void{}
export function main():void{}
varta.tool({name:"choose",description:"Choose"},choose);
varta.tool({name:"positive",description:"Test"},positive);
varta.tool({name:"empty",description:"Do nothing"},empty);
"#,
    );
    let metadata = catalog(&bytes);
    assert_eq!(
        metadata.tools[0].parameters[0].value_type,
        ToolType::Integer
    );
    assert_eq!(
        metadata.tools[0].parameters[1].value_type,
        ToolType::Boolean
    );
    assert_eq!(metadata.tools[1].result, ToolType::Void);
    assert_eq!(metadata.tools[2].result, ToolType::Boolean);
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &bytes).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let choose = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "varta.tool.choose")
        .unwrap();
    assert_eq!(choose.call(&mut store, (i32::MIN, 1)).unwrap(), i32::MIN);
    assert_eq!(choose.call(&mut store, (i32::MAX, 0)).unwrap(), 0);
    let positive = instance
        .get_typed_func::<i32, i32>(&mut store, "varta.tool.positive")
        .unwrap();
    assert_eq!(positive.call(&mut store, 1).unwrap(), 1);
    assert_eq!(positive.call(&mut store, -1).unwrap(), 0);
    instance
        .get_typed_func::<(), ()>(&mut store, "varta.tool.empty")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
}

#[test]
fn abi_entry_points_and_start_functions_cannot_hide_host_effects() {
    let bytes = compile(
        r#"
export function transfer(p:number,len:number):string{return varta.env.get("NAME");}
export function echo(text:string):string{return text;}
export function main():void{}
varta.tool({name:"transfer",description:"Read"},transfer);
varta.tool({name:"echo",description:"Echo"},echo);
"#,
    );
    let index = export_index(&bytes, "varta.tool.transfer");
    let bad = rewrite(&bytes, |id, section| {
        let mut section = section.to_vec();
        if id == 7 {
            let name = varta_common::TOOL_COPY_STRING.as_bytes();
            let start = section
                .windows(name.len())
                .position(|bytes| bytes == name)
                .unwrap();
            let slot = start + name.len() + 1; // function kind, then a one-byte fixture index
            assert!(section[slot] < 128 && index < 128);
            section[slot] = index;
        }
        vec![section]
    });
    assert!(
        VartaRuntime::new()
            .validate_module(&bad)
            .unwrap_err()
            .to_string()
            .contains("helpers cannot call host imports")
    );

    let mut hidden = catalog(&bytes);
    hidden.tools.pop();
    assert!(
        VartaRuntime::new()
            .validate_module(&replace_catalog(&bytes, &hidden.to_bytes().unwrap()))
            .unwrap_err()
            .to_string()
            .contains("unlisted")
    );

    let index = export_index(&bytes, "_start");
    let mut before = 8;
    let mut with_start = bytes.clone();
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Some((id, range)) = payload.unwrap().as_section() {
            if id == 10 {
                // Insert a valid start section before code. Validation must reject
                // it without instantiating even though its signature is valid.
                with_start.splice(before..before, [8, 1, index]);
                break;
            }
            before = range.end;
        }
    }
    wasmtime::Module::new(&wasmtime::Engine::default(), &with_start).unwrap();
    assert!(
        VartaRuntime::new()
            .validate_module(&with_start)
            .unwrap_err()
            .to_string()
            .contains("no Wasm start function")
    );
}
