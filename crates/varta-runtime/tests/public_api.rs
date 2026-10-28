//! External-consumer tests: public engine APIs only, without project tooling,
//! a CLI process, project configuration, source files or domain-specific models.
use std::path::{Path, PathBuf};
use varta_common::{ToolInvocationCode, ToolValue};
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource, compile_modules};
use varta_runtime::VartaRuntime;

struct Sources;
#[test]
fn public_engine_supports_nullable_references_and_identity_across_modules() {
    struct ReferenceSources;
    impl ModuleLoader for ReferenceSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function keep(v:{n:number}|number[]|undefined):{n:number}|number[]|undefined{return v;}
export function first(v:{n:number}[]):{n:number}|undefined{return v[0];}
"#
            .into())
        }
    }
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: r#"
import {keep,first} from './values.ts';
export function main():void{
 const object={n:1.5};const alias=first([object]);if(alias!==undefined){alias.n=2.5;}
 console.log(keep(object)===object);console.log(JSON.stringify(alias));const array=keep([3.5]);
 if(Array.isArray(array)){console.log(array[0]);}console.log(first([]));
 console.log(JSON.stringify(varta.json.parse<{n:number}>('{"n":1.25}')));
}"#
            .into(),
        },
        &ReferenceSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V4,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let inspected = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspected.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V4)
    );
    assert_eq!(
        inspected.value_abi_version,
        Some(varta_common::value::REFERENCE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            r#"varta log: {"n":2.5}"#,
            "varta log: 3.5",
            "varta log: undefined",
            r#"varta log: {"n":1.25}"#
        ]
    );
}
#[test]
fn public_engine_supports_presence_and_missing_primitive_reads_across_modules() {
    struct MissingSources;
    impl ModuleLoader for MissingSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function create():{label?:string;n:number}{return {n:1.5};}
export function read(values:string[],index:number):string|undefined{return values[index];}
"#
            .into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource {path:"entry.ts".into(),text:r#"
import {create,read} from './values.ts';
export function main():void {const value=create();console.log(Object.hasOwn(value,"label"));value.label="😀";
 console.log(JSON.stringify(value));console.log(read(["kept"],1));let empty:string|undefined;console.log(empty);}
"#.into()},&MissingSources,varta_common::language::LanguageProfile::CheckedF64Utf16V3).unwrap();
    let runtime = VartaRuntime::new();
    let inspected = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspected.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V3)
    );
    assert_eq!(
        inspected.value_abi_version,
        Some(varta_common::value::MISSING_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 0",
            r#"varta log: {"n":1.5,"label":"😀"}"#,
            "varta log: undefined",
            "varta log: undefined"
        ]
    );
}
#[test]
fn public_engine_executes_primitive_union_arguments_results_and_json_without_tooling() {
    struct UnionSources;
    impl ModuleLoader for UnionSources {
        fn resolve(&self, _: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
            if specifier == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function optional(v:string|number|undefined):string|number|undefined{return v;}
export function label(v:string|number|undefined):string {
 if(typeof v==="number"){return JSON.stringify(v+0.5);}
 return typeof v==="string"?v+"!":"missing";
}"#
            .into())
        }
    }
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: r#"
import {optional,label} from './values.ts';
export function main():void {
 console.log(label(optional(1.5)));console.log(label(optional("😀")));
 const values:(number|string|undefined)[]=[optional(undefined),optional("\ud800")];
 console.log(JSON.stringify(values));console.log(JSON.stringify(optional(undefined)));
}"#
            .into(),
        },
        &UnionSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V2,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspection.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V2)
    );
    assert_eq!(
        inspection.value_abi_version,
        Some(varta_common::value::UNION_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 2",
            "varta log: 😀!",
            r#"varta log: [null,"\ud800"]"#,
            "varta log: undefined"
        ]
    );
}
#[test]
fn public_engine_runs_utf16_values_and_json_across_an_explicit_module_graph() {
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: r#"
import {increment} from './math.ts';
function keep(s:string,n:number):string{return s.charAt(n);}
export function main():void{
 const text=keep("😀",increment(0));console.log(JSON.stringify({text,missing:undefined}));
 console.log(varta.json.parse<string>('"\\ud800"').charCodeAt());
}"#
            .into(),
        },
        &Sources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V1,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspection.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V1)
    );
    assert_eq!(
        inspection.value_abi_version,
        Some(varta_common::value::UTF16_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [r#"varta log: {"text":"\ude00"}"#, "varta log: 55296"]
    );
}
#[test]
fn public_engine_supports_distinct_undefined_and_versioned_json_without_tooling() {
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource {path:"entry.ts".into(),text:r#"
import {increment} from './math.ts';
export function main():void{const unused=void increment(0.5);console.log(JSON.stringify({unused,n:increment(0.5)}));console.log(JSON.stringify(unused));}
"#.into()},&Sources,varta_common::language::LanguageProfile::CheckedF64Utf8V2).unwrap();
    let runtime = VartaRuntime::new();
    let metadata = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        metadata.value_abi_version,
        Some(varta_common::value::UNDEFINED_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [r#"varta log: {"n":1.5}"#, "varta log: undefined"]
    );
}
#[test]
fn public_engine_selects_binary64_for_the_entire_module_graph() {
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: r#"
import {increment} from './math.ts';
export function main():void{
 console.log(increment(0.5));const values=varta.json.parse<number[]>('[1.25,2.5]');
 values[0]=increment(0.5);console.log(JSON.stringify(values));
}"#
            .into(),
        },
        &Sources,
        varta_common::language::LanguageProfile::CheckedF64Utf8V1,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::NUMBER_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .language
            .profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf8V1)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: 1.5", "varta log: [1.5,2.5]"]
    );
}
impl ModuleLoader for Sources {
    fn resolve(&self, importer: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
        if importer == Path::new("entry.ts") && specifier == "./math.ts" {
            Ok("math.ts".into())
        } else {
            Err(CompilerError::Module("unknown virtual dependency".into()))
        }
    }
    fn load(&self, path: &Path) -> Result<String, CompilerError> {
        if path == Path::new("math.ts") {
            Ok("export function increment(value:number):number{return value+1;}".into())
        } else {
            Err(CompilerError::Module("unknown virtual source".into()))
        }
    }
}

#[test]
fn public_engine_compiles_inspects_and_invokes_without_project_or_cli() {
    let compilation = compile_modules(
        ModuleSource {
            path: "entry.ts".into(),
            text: r#"
import {increment} from "./math.ts";
export {increment};
export function main():void{console.log("MUST_NOT_START");}
varta.tool({name:"math.increment",description:"Increment an integer"},increment);
"#
            .into(),
        },
        &Sources,
    )
    .unwrap();
    assert_eq!(compilation.modules.len(), 2);
    let runtime = VartaRuntime::new();
    let metadata = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        metadata.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedI32Utf8V1)
    );
    assert_eq!(
        metadata.language.origin,
        varta_common::language::LanguageOrigin::Declared
    );
    assert_eq!(metadata.memories, 1);
    assert!(
        metadata
            .imports
            .iter()
            .any(|import| import.module == "varta" && import.name == "log_string")
    );
    assert!(
        metadata
            .exports
            .iter()
            .any(|name| name == "varta.tool.math.increment")
    );
    let catalog = metadata.tool_catalog.unwrap();
    assert_eq!(catalog.tools[0].parameters[0].name, "value");
    let output = runtime
        .invoke_tool_with_policy(
            &compilation.wasm,
            "math.increment",
            br#"{"value":41}"#,
            &[],
            &[],
        )
        .unwrap();
    assert_eq!(output.result.value, ToolValue::Integer(42));
    assert!(output.logs.is_empty());
    assert_eq!(
        runtime
            .invoke_tool(&compilation.wasm, "main", b"{}", &[])
            .unwrap_err()
            .code(),
        ToolInvocationCode::UnknownTool
    );
    assert_eq!(
        runtime
            .invoke_tool(
                &compilation.wasm,
                "math.increment",
                br#"{"value":"41"}"#,
                &[]
            )
            .unwrap_err()
            .code(),
        ToolInvocationCode::InvalidArguments
    );
}

#[test]
fn backend_free_inspection_keeps_actual_imports_and_malformed_code_validation() {
    let wasm = varta_compiler::compile_typescript(
        r#"
export function read(name:string):string{return varta.env.get(name);}
export function main():void{}
varta.tool({name:"env.read",description:"Read an allowed name"},read);
"#,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let metadata = runtime.inspect_module(&wasm).unwrap();
    assert!(
        metadata
            .imports
            .iter()
            .any(|import| import.name == "env_get")
    );
    assert_eq!(
        metadata.tool_catalog.unwrap().tools[0].required_effects,
        ["env.get"]
    );
    let output = runtime
        .invoke_tool_with_policy(
            &wasm,
            "env.read",
            br#"{"name":"DENIED_PUBLIC_API_INPUT"}"#,
            &[],
            &[],
        )
        .unwrap();
    assert_eq!(output.result.value, ToolValue::String(String::new()));
    assert!(
        output
            .logs
            .iter()
            .any(|line| line.contains("capability denied"))
    );
    assert!(
        runtime
            .inspect_module(b"\0asm\x01\0\0\0\x01\x01\xff")
            .is_err()
    );
}

#[test]
fn public_engine_parses_typed_optional_unions_across_virtual_modules() {
    struct JsonSources;
    impl ModuleLoader for JsonSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./decode.ts" {
                Ok("decode.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function decode(text:string):{a?:string;child?:{n:number};items:(number|string)[]}{
 return varta.json.parse<{a?:string;child?:{n:number};items:(number|string)[]}>(text);
}"#
            .into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {decode} from './decode.ts';
export function main():void {
 const value=decode('{"items":[1.25,"😀"],"child":{"n":2.5}}');console.log(Object.hasOwn(value,"a"));
 const child=value.child;if(child!==undefined){child.n=3.5;}console.log(JSON.stringify(value));
}"#.into()},&JsonSources,varta_common::language::LanguageProfile::CheckedF64Utf16V5).unwrap();
    let runtime = VartaRuntime::new();
    let inspected = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspected.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V5)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 0",
            r#"varta log: {"child":{"n":3.5},"items":[1.25,"😀"]}"#
        ]
    );
}

#[test]
fn public_engine_grows_shared_arrays_across_modules_and_json_boundaries() {
    struct VectorSources;
    impl ModuleLoader for VectorSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function decode(text:string):{n:number}[]{return varta.json.parse<{n:number}[]>(text);}
export function append(v:{n:number}[]):{n:number}[]{v.push({n:2.5},{n:3.5});return v;}
"#
            .into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {decode,append} from './values.ts';
export function main():void {
 const values=decode('[{"n":1.25}]');const alias=append(values);console.log(alias===values);
 const removed=alias.pop();if(removed!==undefined){removed.n=4.5;console.log(JSON.stringify(removed));}
 console.log(JSON.stringify(values));console.log(values.length);
}"#.into()},&VectorSources,varta_common::language::LanguageProfile::CheckedF64Utf16V6).unwrap();
    let runtime = VartaRuntime::new();
    let metadata = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        metadata.value_abi_version,
        Some(varta_common::value::VECTOR_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            r#"varta log: {"n":4.5}"#,
            r#"varta log: [{"n":1.25},{"n":2.5}]"#,
            "varta log: 2"
        ]
    );
}

#[test]
fn public_engine_slices_shared_elements_and_reads_relative_indices_across_modules() {
    struct ArraySources;
    impl ModuleLoader for ArraySources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function copy(v:{n:number}[],start:number|undefined,end:number|undefined):{n:number}[]{return v.slice(start,end);}
export function last(v:{n:number}[]):{n:number}|undefined{return v.at(-1);}
"#.into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {copy,last} from './values.ts';
export function main():void {
 const values=varta.json.parse<{n:number}[]>('[{"n":1.25},{"n":2.5}]');const copied=copy(values,undefined,undefined);
 console.log(copied===values);const tail=last(copied);if(tail!==undefined){tail.n=3.5;}
 copied.pop();console.log(JSON.stringify(values));console.log(copied.length);console.log(copy(values,9,undefined).at(-1));
}"#.into()},&ArraySources,varta_common::language::LanguageProfile::CheckedF64Utf16V6).unwrap();
    let runtime = VartaRuntime::new();
    let metadata = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        metadata.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V6)
    );
    assert_eq!(
        metadata.value_abi_version,
        Some(varta_common::value::VECTOR_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 0",
            r#"varta log: [{"n":1.25},{"n":3.5}]"#,
            "varta log: 1",
            "varta log: undefined"
        ]
    );
}

#[test]
fn public_engine_searches_values_and_reference_identity_across_modules() {
    struct SearchSources;
    impl ModuleLoader for SearchSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./search.ts" {
                Ok("search.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function find(v:{n:number}[],needle:{n:number}|undefined,from:number|undefined):number{return v.indexOf(needle,from);}
export function hasNaN(v:number[]):boolean{return v.includes(0/0);}
"#.into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {find,hasNaN} from './search.ts';
export function main():void {
 const v=varta.json.parse<{n:number}[]>('[{"n":1.25},{"n":2.5}]');const alias=v.at(-1);
 console.log(find(v,alias,undefined));console.log(find(v.slice(),alias,-1));console.log(find(v,{n:2.5},undefined));
 console.log(hasNaN([0/0,-0]));console.log([0/0].indexOf(0/0));console.log(["😀"].includes(""+"😀"));
}"#.into()},&SearchSources,varta_common::language::LanguageProfile::CheckedF64Utf16V6).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["1", "1", "-1", "1", "-1", "1"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn public_engine_propagates_primitive_exceptions_across_modules_and_owns_uncaught_values() {
    struct ExceptionSources;
    impl ModuleLoader for ExceptionSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./fail.ts" {
                Ok("fail.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function fail(text:string):number{throw text+'😀';}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {fail} from './fail.ts';
export function main():void{const keep={text:"kept"};try{const v=[1+fail("owned")];console.log(JSON.stringify(v));}catch(e){if(typeof e==="string"){console.log(e);}}console.log(keep.text);}
"#.into()},&ExceptionSources,varta_common::language::LanguageProfile::CheckedF64Utf16V7).unwrap();
    let runtime = VartaRuntime::new();
    let metadata = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        metadata.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V7)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: owned😀", "varta log: kept"]
    );
    let wasm = varta_compiler::compile_typescript_with_profile(
        "export function main():void{throw 'owned'+'😀';}",
        varta_common::language::LanguageProfile::CheckedF64Utf16V7,
    )
    .unwrap();
    let error = runtime.run_wasm_bytes(&wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::String("owned😀".encode_utf16().collect())
    );
}

#[test]
fn public_engine_finalizes_cross_module_returns_and_owns_replacement_exceptions() {
    struct FinalizerSources;
    impl ModuleLoader for FinalizerSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok(r#"
export function value():string{try{return "owned"+"😀";}finally{console.log("module-final");}}
export function fail():void{try{throw "first";}finally{throw "replacement"+"😀";}}
"#
            .into())
        }
    }
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: r#"
import {value,fail} from './values.ts';
export function main():void{try{console.log(value());fail();}catch(e){console.log(e);}finally{console.log("entry-final");}}
"#
            .into(),
        },
        &FinalizerSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V7,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let output = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap();
    assert_eq!(
        output.lines,
        ["module-final", "owned😀", "replacement😀", "entry-final"]
            .map(|s| format!("varta log: {s}"))
    );
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: "import {fail} from './values.ts';export function main():void{fail();}".into(),
        },
        &FinalizerSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V7,
    )
    .unwrap();
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::String("replacement😀".encode_utf16().collect())
    );
}

#[test]
fn public_engine_transfers_nominal_errors_across_modules_and_owns_uncaught_messages() {
    struct ErrorSources;
    impl ModuleLoader for ErrorSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./errors.ts" {
                Ok("errors.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function make():Error{return Error('owned'+'😀');}export function fail():void{throw make();}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {make,fail} from './errors.ts';
export function main():void{const error=make();try{throw error;}catch(e){if(e instanceof Error){console.log(e===error);console.log(e.message);}}console.log(JSON.stringify([error]));}
"#.into()},&ErrorSources,varta_common::language::LanguageProfile::CheckedF64Utf16V8).unwrap();
    let runtime = VartaRuntime::new();
    let info = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(info.value_abi_version, Some(10));
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["1", "owned😀", "[{}]"].map(|s| format!("varta log: {s}"))
    );
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: "import {fail} from './errors.ts';export function main():void{fail();}".into(),
        },
        &ErrorSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V8,
    )
    .unwrap();
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::Error {
            message: "owned😀".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_preserves_native_error_family_types_across_modules_and_owned_boundaries() {
    struct NativeErrorSources;
    impl ModuleLoader for NativeErrorSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./errors.ts" {
                Ok("errors.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function make():TypeError{return TypeError('owned'+'😀');}export function broader(error:TypeError):Error{return error;}export function fail():void{throw SyntaxError('owned'+'😀');}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {make,broader} from './errors.ts';
export function main():void{const typed=make();const error=broader(typed);console.log(error===typed);console.log(error instanceof Error);if(error instanceof TypeError){console.log(error.toString());}console.log(JSON.stringify([error]));}
"#.into()},&NativeErrorSources,varta_common::language::LanguageProfile::CheckedF64Utf16V9).unwrap();
    let runtime = VartaRuntime::new();
    let info = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(info.value_abi_version, Some(11));
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["1", "1", "TypeError: owned😀", "[{}]"].map(|s| format!("varta log: {s}"))
    );
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: "import {fail} from './errors.ts';export function main():void{fail();}".into(),
        },
        &NativeErrorSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V9,
    )
    .unwrap();
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::SyntaxError,
            message: "owned😀".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_recovers_checked_json_failures_across_modules_and_owns_uncaught_errors() {
    struct JsonSources;
    impl ModuleLoader for JsonSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./decode.ts" {
                Ok("decode.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function read(text:string):{n:number}{return varta.json.parse<{n:number}>(text);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {read} from './decode.ts';
export function main():void{try{read('{');}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}try{read('{"n":false}');}catch(e){if(e instanceof TypeError){console.log(e.name);}}console.log(read('{"n":1.25}').n);}
"#.into()},&JsonSources,varta_common::language::LanguageProfile::CheckedF64Utf16V10).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(11)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["Invalid JSON syntax", "TypeError", "1.25"].map(|s| format!("varta log: {s}"))
    );
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text: "import {read} from './decode.ts';export function main():void{read('{}');}"
                .into(),
        },
        &JsonSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V10,
    )
    .unwrap();
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::TypeError,
            message: "JSON value does not match the checked schema"
                .encode_utf16()
                .collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_repeats_utf16_strings_across_modules_and_owns_range_errors() {
    struct RepeatSources;
    impl ModuleLoader for RepeatSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./repeat.ts" {
                Ok("repeat.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function repeat(value:string,count:number|undefined):string{return value.repeat(count);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {repeat} from './repeat.ts';
export function main():void{console.log(repeat("owned😀",2));try{repeat("x",-1);}catch(e){if(e instanceof RangeError){console.log(e.message);}}console.log(repeat("x",undefined));}
"#.into()},&RepeatSources,varta_common::language::LanguageProfile::CheckedF64Utf16V10).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["owned😀owned😀", "Invalid string repeat count", ""].map(|s| format!("varta log: {s}"))
    );
    let compilation = varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "entry.ts".into(),
            text:
                "import {repeat} from './repeat.ts';export function main():void{repeat('x',1/0);}"
                    .into(),
        },
        &RepeatSources,
        varta_common::language::LanguageProfile::CheckedF64Utf16V10,
    )
    .unwrap();
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::RangeError,
            message: "Invalid string repeat count".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_typed_function_values_and_owns_uncaught_callback_errors() {
    struct CallbackSources;
    impl ModuleLoader for CallbackSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./callbacks.ts" {
                Ok("callbacks.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function apply(fn:(value:number)=>number,value:number):number{return fn(value);}".into())
        }
    }
    let compile = |text: &str| {
        varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: text.into(),
            },
            &CallbackSources,
            varta_common::language::LanguageProfile::CheckedF64Utf16V11,
        )
        .unwrap()
    };
    let compilation = compile(
        "import {apply} from './callbacks.ts';export function main():void{console.log(apply(value=>value+1,41));}",
    );
    let runtime = VartaRuntime::new();
    let info = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        info.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V11)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: 42"]
    );
    let compilation = compile(
        "import {apply} from './callbacks.ts';export function main():void{apply(value=>{throw TypeError('owned😀');},0);}",
    );
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::TypeError,
            message: "owned😀".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_escaped_mutable_closures_and_owns_their_uncaught_errors() {
    struct ClosureSources;
    impl ModuleLoader for ClosureSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./closures.ts" {
                Ok("closures.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function counter(value:number):()=>number{return ()=>{value=value+1;return value;};}export function fail(message:string):()=>number{return ()=>{throw TypeError(message);};}".into())
        }
    }
    let compile = |text: &str| {
        varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: text.into(),
            },
            &ClosureSources,
            varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        )
        .unwrap()
    };
    let runtime = VartaRuntime::new();
    let compilation = compile(
        "import {counter} from './closures.ts';export function main():void{const first=counter(0);const second=counter(10);console.log(first());console.log(first());console.log(second());}",
    );
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .language
            .profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V12)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["1", "2", "11"].map(|s| format!("varta log: {s}"))
    );
    let compilation = compile(
        "import {fail} from './closures.ts';export function main():void{const fn=fail('owned😀');fn();}",
    );
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::TypeError,
            message: "owned😀".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_array_callbacks_and_owns_uncaught_predicate_errors() {
    struct ArraySources;
    impl ModuleLoader for ArraySources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./arrays.ts" {
                Ok("arrays.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function transform(values:number[],offset:number):number[]{return values.map(value=>value+offset).filter(value=>value>11);}export function fail(message:string):void{[1].forEach(value=>{throw TypeError(message);});}".into())
        }
    }
    let compile = |text: &str| {
        varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: text.into(),
            },
            &ArraySources,
            varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        )
        .unwrap()
    };
    let runtime = VartaRuntime::new();
    let compilation = compile(
        "import {transform} from './arrays.ts';export function main():void{const values=transform([1,2,3],10);console.log(JSON.stringify(values));console.log(values.find(value=>value===13));}",
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: [12,13]", "varta log: 13"]
    );
    let compilation =
        compile("import {fail} from './arrays.ts';export function main():void{fail('owned😀');}");
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::TypeError,
            message: "owned😀".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_sparse_array_modules_and_owns_callback_errors() {
    struct SparseSources;
    impl ModuleLoader for SparseSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./sparse.ts" {
                Ok("sparse.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function transform(values:number[],offset:number):number[]{return values.map(value=>value+offset);}export function fail(message:string):void{[1,,].map(value=>{throw TypeError(message);});}".into())
        }
    }
    let compile = |text: &str| {
        varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: text.into(),
            },
            &SparseSources,
            varta_common::language::LanguageProfile::CheckedF64Utf16V13,
        )
        .unwrap()
    };
    let runtime = VartaRuntime::new();
    let compilation = compile(
        "import {transform} from './sparse.ts';export function main():void{const values=transform([1,,3],10);console.log(JSON.stringify(values));console.log(Object.hasOwn(values,1));}",
    );
    let inspected = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspected.value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: [11,null,13]", "varta log: 0"]
    );
    let compilation =
        compile("import {fail} from './sparse.ts';export function main():void{fail('owned😀');}");
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::TypeError,
            message: "owned😀".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_array_mutations_and_owns_uncaught_length_errors() {
    struct MutationSources;
    impl ModuleLoader for MutationSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./mutate.ts" {
                Ok("mutate.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function change(values:number[]):void{delete values[0];values.length=1;values[3]=4;}".into())
        }
    }
    let compile = |text: &str| {
        varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: text.into(),
            },
            &MutationSources,
            varta_common::language::LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap()
    };
    let runtime = VartaRuntime::new();
    let compilation = compile(
        "import {change} from './mutate.ts';export function main():void{const values=[1,2];change(values);console.log(JSON.stringify(values));console.log(Object.hasOwn(values,3));}",
    );
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: [null,null,null,4]", "varta log: 1"]
    );
    let compilation = compile("export function main():void{const values=[1];values.length=NaN;}");
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::RangeError,
            message: "Invalid array length".encode_utf16().collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_collection_reductions_and_owns_empty_reduce_errors() {
    struct CollectionSources;
    impl ModuleLoader for CollectionSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./collections.ts" {
                Ok("collections.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function sum(values:number[]):number{return values.reduce((total,value)=>total+value,0);}".into())
        }
    }
    let compile = |text: &str| {
        varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: text.into(),
            },
            &CollectionSources,
            varta_common::language::LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap()
    };
    let compilation = compile(
        "import {sum} from './collections.ts';export function main():void{const values:number[]=[,2,,4];console.log(sum(values));console.log(values.some(value=>value===4));console.log(values.every(value=>value>0));}",
    );
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: 6", "varta log: 1", "varta log: 1"]
    );
    let compilation = compile(
        "export function main():void{const values:number[]=[,,];values.reduce((total,value)=>total+value);}",
    );
    let error = runtime.run_wasm_bytes(&compilation.wasm, &[]).unwrap_err();
    let varta_runtime::RuntimeError::Uncaught(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.value,
        varta_common::exceptions::ThrownValue::NativeError {
            class: varta_common::value::ErrorClass::TypeError,
            message: "Reduce of empty array with no initial value"
                .encode_utf16()
                .collect(),
            has_message: true
        }
    );
}

#[test]
fn public_engine_runs_inferred_callbacks_and_contextual_reduce_seeds() {
    struct InferredSources;
    impl ModuleLoader for InferredSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./collect.ts" {
                Ok("collect.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function collect(values:number[]):number[]{return values.reduce((total,value)=>{total.push(value);return total;},[]);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {collect} from './collect.ts';export function main():void{const values:number[]=[1,,3];console.log(JSON.stringify(collect(values)));console.log(JSON.stringify(values.map(value=>{try{return {n:value};}finally{const local=1;}})));console.log(values.some(value=>{return value===3?"found":"";}));}
"#.into()},&InferredSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    let inspected = runtime.inspect_module(&compilation.wasm).unwrap();
    assert_eq!(
        inspected.language.profile,
        Some(varta_common::language::LanguageProfile::CheckedF64Utf16V14)
    );
    assert_eq!(
        inspected.value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: [1,3]",
            "varta log: [{\"n\":1},null,{\"n\":3}]",
            "varta log: 1"
        ]
    );
}

#[test]
fn public_engine_supports_typed_this_function_expressions_and_lexical_capture() {
    struct ThisSources;
    impl ModuleLoader for ThisSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./functions.ts" {
                Ok("functions.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function make(offset:number):(this:{n:number},value:number)=>number{return function(value){return this.n+offset+value;};}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:r#"
import {make} from './functions.ts';export function main():void{const context={n:10};const fn=make(2);console.log(JSON.stringify([1,2].map(fn,context)));console.log(fn.call(context,4));const factory=function(this:{n:number}):()=>number{return ()=>this.n;};const arrow=factory.call(context);context.n=20;console.log(arrow.call({n:99}));}
"#.into()},&ThisSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: [13,14]", "varta log: 16", "varta log: 20"]
    );
}

#[test]
fn public_engine_supports_named_expression_self_and_declaration_receiver_aliases() {
    struct NamedSources;
    impl ModuleLoader for NamedSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./functions.ts" {
                Ok("functions.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function add(this:{n:number},value:number):number{return this.n+value;}export {add as alias};export function make(offset:number):(depth:number)=>number{return function self(depth:number):number{if(depth===0){return offset;}return self(depth-1)+1;};}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {add,alias,make} from './functions.ts';export function main():void{const context={n:10};console.log(add.call(context,2));console.log(add===alias);console.log(JSON.stringify([1,2].map(add,context)));console.log(make(7)(3));}".into()},&NamedSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 12",
            "varta log: 1",
            "varta log: [11,12]",
            "varta log: 10"
        ]
    );
}

#[test]
fn public_engine_supports_optional_default_and_rest_function_parameters() {
    struct ParameterSources;
    impl ModuleLoader for ParameterSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./parameters.ts" {
                Ok("parameters.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function sum(first:number=1,...values:number[]):number{return values.reduce((total,value)=>total+value,first);}export function optional(value?:number):number|undefined{return value;}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {sum,optional} from './parameters.ts';export function main():void{console.log(sum());console.log(sum(undefined,2,3));console.log(optional());console.log(JSON.stringify([1,2].map((value,index,array,extra=10)=>value+extra)));}".into()},&ParameterSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            "varta log: 6",
            "varta log: undefined",
            "varta log: [11,12]"
        ]
    );
}

#[test]
fn public_engine_expands_checked_spread_arguments_across_virtual_modules() {
    struct SpreadSources;
    impl ModuleLoader for SpreadSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./spread.ts" {
                Ok("spread.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function summary(prefix:string='default',...values:number[]):string{return prefix+JSON.stringify(values);}export function total(this:{base:number},...items:number[]):number{return items.reduce((sum,n)=>sum+n,this.base);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {summary,total} from './spread.ts';export function main():void{console.log(summary(...[]));console.log(summary('items',...[1,2],3));console.log(total.call({base:10},...[1,2]));}".into()},&SpreadSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: default[]",
            "varta log: items[1,2,3]",
            "varta log: 13"
        ]
    );
}

#[test]
fn public_engine_destructures_parameters_across_virtual_modules() {
    struct PatternSources;
    impl ModuleLoader for PatternSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./patterns.ts" {
                Ok("patterns.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function summary({prefix='default',...copy}:{prefix?:string;flag?:boolean},[first=1,...tail]:number[]=[]):{prefix:string;first:number;tail:(number|undefined)[];copy:{flag?:boolean}}{return {prefix:prefix,first:first,tail:tail,copy:copy};}export function factory({text}:{text:string}):()=>string{return ()=>text;}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {summary,factory} from './patterns.ts';export function main():void{console.log(JSON.stringify(summary({})));const options:{prefix?:string;flag?:boolean}={prefix:'set',flag:false};console.log(JSON.stringify(summary(...[options],[2,,3])));console.log(factory({text:'closure'+'!'})());}".into()},&PatternSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: {\"prefix\":\"default\",\"first\":1,\"tail\":[],\"copy\":{}}",
            "varta log: {\"prefix\":\"set\",\"first\":2,\"tail\":[null,3],\"copy\":{\"flag\":false}}",
            "varta log: closure!"
        ]
    );
}

#[test]
fn public_engine_runs_local_binding_patterns_across_virtual_modules() {
    struct LocalSources;
    impl ModuleLoader for LocalSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./locals.ts" {
                Ok("locals.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function snapshot():{label:string;first:number;tail:(number|undefined)[];copy:{flag?:boolean}}{const {name:label='local',...copy}:{name?:string;flag?:boolean}={flag:false};let [first=1,...tail]:number[]=[2,,3];first=first+1;return {label:label,first:first,tail:tail,copy:copy};}export function reader():()=>number{let saved:()=>number=()=>99;for(let {i=0,read=()=>i}:{}={};i<2;i=i+1){saved=read;if(i===0){i=1;}}return saved;}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {snapshot,reader} from './locals.ts';export function main():void{console.log(JSON.stringify(snapshot()));console.log(reader()());}".into()},&LocalSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: {\"label\":\"local\",\"first\":3,\"tail\":[null,3],\"copy\":{\"flag\":false}}",
            "varta log: 0"
        ]
    );
}

#[test]
fn public_engine_runs_checked_destructuring_assignment_across_virtual_modules() {
    struct AssignmentSources;
    impl ModuleLoader for AssignmentSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./assign.ts" {
                Ok("assign.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function snapshot():{first:number;tail:(number|undefined)[]}{let first=0;let tail:(number|undefined)[]=[];[first,...tail]=[2,,3];return {first:first,tail:tail};}export function reader():()=>number{let value=1;const read=()=>value;({value}={value:3});return read;}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {snapshot,reader} from './assign.ts';export function main():void{console.log(JSON.stringify(snapshot()));console.log(reader()());}".into()},&AssignmentSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: {\"first\":2,\"tail\":[null,3]}", "varta log: 3"]
    );
}

#[test]
fn public_engine_returns_assignment_rhs_values_across_virtual_modules() {
    struct ValueSources;
    impl ModuleLoader for ValueSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function assign(source:{n:number;extra:string}):{n:number;extra:string}{let a=0;let b=0;return ({n:a}=({n:b}=source));}export function assignArray(source:number[]):number[]{let n=0;return ([n=7]=source);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {assign,assignArray} from './values.ts';export function main():void{const source={extra:'kept',n:2};const original=assign(source);console.log(original===source);console.log(JSON.stringify(original));const input:number[]=[,2];const output=assignArray(input);console.log(output===input);console.log(Object.hasOwn(output,0));console.log(JSON.stringify(output));}".into()},&ValueSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .inspect_module(&compilation.wasm)
            .unwrap()
            .value_abi_version,
        Some(varta_common::value::SPARSE_VALUE_VERSION)
    );
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            "varta log: {\"extra\":\"kept\",\"n\":2}",
            "varta log: 1",
            "varta log: 0",
            "varta log: [null,2]"
        ]
    );
}

#[test]
fn public_engine_assigns_checked_member_targets_across_virtual_modules() {
    struct MemberSources;
    impl ModuleLoader for MemberSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./members.ts" {
                Ok("members.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function assign(source:{n:number;extra:string},output:{n:number},values:number[]):{n:number;extra:string}{({n:output.n}=source);[values[2]]=[output.n];return ({n:output.n}=source);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {assign} from './members.ts';export function main():void{const source={extra:'kept',n:2};const output={n:0};const values:number[]=[];console.log(assign(source,output,values)===source);console.log(output.n);console.log(JSON.stringify(values));console.log(Object.hasOwn(values,0));}".into()},&MemberSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            "varta log: 2",
            "varta log: [null,null,2]",
            "varta log: 0"
        ]
    );
}

#[test]
fn public_engine_runs_for_of_across_virtual_modules() {
    struct IterationSources;
    impl ModuleLoader for IterationSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./iteration.ts" {
                Ok("iteration.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function count(values:number[]):number{let visited=0;for(const value of values){console.log(value);visited=visited+1;}return visited;}export function first(rows:{n?:number}[]):number{for(const {n=7} of rows){return n;}return 0;}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {count,first} from './iteration.ts';export function main():void{const values:number[]=[1,,3];console.log(count(values));console.log(first([{}]));}".into()},&IterationSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            "varta log: undefined",
            "varta log: 3",
            "varta log: 3",
            "varta log: 7"
        ]
    );
}

#[test]
fn public_engine_runs_do_while_across_virtual_modules() {
    struct LoopSources;
    impl ModuleLoader for LoopSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./loops.ts" {
                Ok("loops.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function spin(limit:number):number{let n=0;do{n=n+1;try{if(n<limit){continue;}}finally{console.log(n);}}while(n<limit);return n;}export function always():string{do{return 'once';}while(false);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {spin,always} from './loops.ts';export function main():void{console.log(spin(2));console.log(always());}".into()},&LoopSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            "varta log: 2",
            "varta log: 2",
            "varta log: once"
        ]
    );
}

#[test]
fn public_engine_runs_labeled_loop_branches_across_virtual_modules() {
    struct LabelSources;
    impl ModuleLoader for LabelSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./labels.ts" {
                Ok("labels.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function visit():number{let n=0;outer:do{n=n+1;for(const value of [1,2]){try{if(n<2){continue outer;}break outer;}finally{console.log(n);}}}while(n<3);return n;}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {visit} from './labels.ts';export function main():void{console.log(visit());}".into()},&LabelSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        ["varta log: 1", "varta log: 2", "varta log: 2"]
    );
}

#[test]
fn public_engine_runs_object_keys_across_virtual_modules() {
    struct KeySources;
    impl ModuleLoader for KeySources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./keys.ts" {
                Ok("keys.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function names(value:{a?:number;b:number;'2'?:number}):string[]{return Object.keys(value);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {names} from './keys.ts';export function main():void{const value:{a?:number;b:number;'2'?:number}={b:1};value.a=undefined;value['2']=2;const keys=names(value);console.log(JSON.stringify(keys));for(const key of keys){console.log(key);}console.log(keys===names(value));}".into()},&KeySources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: [\"2\",\"b\",\"a\"]",
            "varta log: 2",
            "varta log: b",
            "varta log: a",
            "varta log: 0"
        ]
    );
}

#[test]
fn public_engine_runs_object_values_across_virtual_modules() {
    struct ValueSources;
    impl ModuleLoader for ValueSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./values.ts" {
                Ok("values.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function enumerate(source:{a?:number;b:string;'2'?:number}):(number|string|undefined)[]{return Object.values(source);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {enumerate} from './values.ts';export function main():void{const source:{a?:number;b:string;'2'?:number}={b:'x'};source.a=undefined;source['2']=2;const items=enumerate(source);console.log(JSON.stringify(items));console.log(Object.hasOwn(items,2));for(const value of items){console.log(value);}}".into()},&ValueSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: [2,\"x\",null]",
            "varta log: 1",
            "varta log: 2",
            "varta log: x",
            "varta log: undefined"
        ]
    );
}

#[test]
fn public_engine_runs_array_enumeration_across_virtual_modules() {
    struct EnumerationSources;
    impl ModuleLoader for EnumerationSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./enumerate.ts" {
                Ok("enumerate.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function keys(source:number[]|null):string[]{return Object.keys(source);}export function values(source:number[]):number[]{return Object.values(source);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {keys,values} from './enumerate.ts';export function main():void{const source:number[]=[,2,,4];console.log(JSON.stringify(keys(source)));const items=values(source);console.log(JSON.stringify(items));console.log(Object.hasOwn(items,0));source[1]=9;items.push(6);console.log(JSON.stringify(items));console.log(JSON.stringify(source));}".into()},&EnumerationSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: [\"1\",\"3\"]",
            "varta log: [2,4]",
            "varta log: 1",
            "varta log: [2,4,6]",
            "varta log: [null,9,null,4]"
        ]
    );
}

#[test]
fn public_engine_runs_catch_patterns_across_virtual_modules() {
    struct CatchSources;
    impl ModuleLoader for CatchSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./catch.ts" {
                Ok("catch.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function read(flag:boolean):string{try{if(flag){throw TypeError('owned'+'😀');}throw 1;}catch({name='primitive',message='fallback'}){return name+':'+message;}finally{console.log('finally');}}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {read} from './catch.ts';export function main():void{console.log(read(true));console.log(read(false));try{try{throw null;}catch({message='BAD'}){console.log(message);}}catch({name}){console.log(name);}}".into()},&CatchSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: finally",
            "varta log: TypeError:owned😀",
            "varta log: finally",
            "varta log: primitive:fallback",
            "varta log: TypeError"
        ]
    );
}

#[test]
fn public_engine_runs_string_ranges_across_virtual_modules() {
    struct RangeSources;
    impl ModuleLoader for RangeSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./ranges.ts" {
                Ok("ranges.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function cut(source:string,start:number|undefined,end:number|undefined):string{return source.slice(start,end);}export function between(source:string,start:number,end:number):string{return source.substring(start,end);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {cut,between} from './ranges.ts';export function main():void{const text='A😀B';console.log(JSON.stringify(cut(text,1,2)));console.log(between(text,3,1));console.log(cut(text,-1,undefined));console.log(JSON.stringify([1,2,3].slice(-2)));console.log(JSON.stringify([1,2].map(n=>cut('abc',n,undefined))));}".into()},&RangeSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: \"\\ud83d\"",
            "varta log: 😀",
            "varta log: B",
            "varta log: [2,3]",
            "varta log: [\"bc\",\"c\"]"
        ]
    );
}

#[test]
fn public_engine_runs_string_searches_across_virtual_modules() {
    struct SearchSources;
    impl ModuleLoader for SearchSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./search.ts" {
                Ok("search.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function contains(text:string,needle:string,position:number|undefined):boolean{return text.includes(needle,position);}export function begins(text:string,needle:string,position:number):boolean{return text.startsWith(needle,position);}export function finishes(text:string,needle:string,position:number):boolean{return text.endsWith(needle,position);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {contains,begins,finishes} from './search.ts';export function main():void{const text='A😀B';console.log(contains(text,'\\ud83d',1));console.log(begins(text,'😀',1));console.log(finishes(text,'B',4));console.log([NaN].includes(NaN));console.log(JSON.stringify(['abc','xyz'].map(s=>contains(s,'b',undefined))));}".into()},&SearchSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: 1",
            "varta log: 1",
            "varta log: 1",
            "varta log: 1",
            "varta log: [true,false]"
        ]
    );
}

#[test]
fn public_engine_runs_dynamic_own_keys_across_virtual_modules() {
    struct OwnSources;
    impl ModuleLoader for OwnSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./own.ts" {
                Ok("own.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function own(source:{a?:number;b:number},key:string):boolean{return Object.hasOwn(source,key);}".into())
        }
    }
    let compilation=varta_compiler::compile_modules_with_profile(ModuleSource{path:"entry.ts".into(),text:"import {own} from './own.ts';export function main():void{const source:{a?:number;b:number}={b:1};console.log(JSON.stringify(['a','b','missing'].map(key=>own(source,key))));source.a=undefined;console.log(JSON.stringify(['a','b','missing'].map(key=>own(source,key))));console.log(JSON.stringify(Object.keys(source)));console.log(own(source,'__proto__'));console.log(JSON.stringify(source));}".into()},&OwnSources,varta_common::language::LanguageProfile::CheckedF64Utf16V14).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compilation.wasm, &[])
            .unwrap()
            .lines,
        [
            "varta log: [false,true,false]",
            "varta log: [true,true,false]",
            "varta log: [\"b\",\"a\"]",
            "varta log: 0",
            "varta log: {\"b\":1}"
        ]
    );
}

#[test]
fn public_engine_runs_array_and_error_own_keys_across_virtual_modules() {
    struct OwnSources;
    impl ModuleLoader for OwnSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./own.ts" {
                Ok("own.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function arrayOwn(a:(number|undefined)[],key:string):boolean{return Object.hasOwn(a,key);}export function errorOwn(e:Error,key:string):boolean{return Object.hasOwn(e,key);}".into())
        }
    }
    let source = "import {arrayOwn,errorOwn} from './own.ts';export function main():void{const a:(number|undefined)[]=[1,,undefined];console.log(JSON.stringify(['0','1','2','3','length'].map(key=>arrayOwn(a,key))));a[1]=undefined;console.log(JSON.stringify(['0','1','2','3','length'].map(key=>arrayOwn(a,key))));console.log(JSON.stringify(['message','name','__proto__','stack',''].map(key=>errorOwn(TypeError(''),key))));console.log(errorOwn(Error(),'mess'+'age'));console.log(JSON.stringify(Object.keys(a)));}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V13,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &OwnSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            [
                "varta log: [true,false,true,false,true]",
                "varta log: [true,true,true,false,true]",
                "varta log: [true,false,false,false,false]",
                "varta log: 0",
                "varta log: [\"0\",\"1\",\"2\"]"
            ]
        );
    }
}

#[test]
fn public_engine_runs_computed_record_reads_across_virtual_modules() {
    struct ReadSources;
    impl ModuleLoader for ReadSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./read.ts" {
                Ok("read.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function read(source:{a?:number;b:string},key:string):number|string|undefined{return source[key];}".into())
        }
    }
    let source = "import {read} from './read.ts';export function main():void{const source:{a?:number;b:string}={b:'owned'};console.log(JSON.stringify(['a','b','missing'].map(key=>read(source,key))));source.a=2.5;console.log(JSON.stringify(['a','b','missing'].map(key=>read(source,key))));try{read(source,'constructor');}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(Object.keys(source)));}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &ReadSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            [
                "varta log: [null,\"owned\",null]",
                "varta log: [2.5,\"owned\",null]",
                "varta log: 1",
                "varta log: [\"b\",\"a\"]"
            ]
        );
    }
}

#[test]
fn public_engine_runs_computed_record_writes_across_virtual_modules() {
    struct WriteSources;
    impl ModuleLoader for WriteSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./write.ts" {
                Ok("write.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function put(source:{n:number;label:string;extra?:number},key:string,item:number|string|undefined):void{source[key]=item;}export function putError(source:{type:TypeError;range:RangeError},key:string,item:TypeError|RangeError|number):void{source[key]=item;}".into())
        }
    }
    let source = "import {put,putError} from './write.ts';export function main():void{const source:{n:number;label:string;extra?:number}={n:1,label:'old'};put(source,'n',2.5);put(source,'label','new');put(source,'extra',3);console.log(JSON.stringify(source));try{put(source,'label',3);}catch(error){console.log(error instanceof TypeError);}try{put(source,'missing',4);}catch(error){console.log(error instanceof TypeError);}const errors={type:TypeError('old'),range:RangeError('old')};putError(errors,'type',TypeError('new'));try{putError(errors,'type',RangeError('wrong'));}catch(error){console.log(error instanceof TypeError);}console.log(errors.type.message);console.log(JSON.stringify(Object.keys(source)));}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &WriteSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            [
                "varta log: {\"n\":2.5,\"label\":\"new\",\"extra\":3}",
                "varta log: 1",
                "varta log: 1",
                "varta log: 1",
                "varta log: new",
                "varta log: [\"n\",\"label\",\"extra\"]"
            ]
        );
    }
}

#[test]
fn public_engine_runs_record_assignment_values_across_virtual_modules() {
    struct AssignmentSources;
    impl ModuleLoader for AssignmentSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./assign.ts" {
                Ok("assign.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function assign(target:{n?:number},key:string,item:number):number{return target[key]=item;}export function chain(a:{n?:number},b:{n?:number},key:string,item:number):number{return a[key]=b['n']=item;}export function setChild(target:{child?:{text:string}},key:string,item:{text:string}):{text:string}{return target[key]=item;}".into())
        }
    }
    let source = "import {assign,chain,setChild} from './assign.ts';export function main():void{const a:{n?:number}={};const b:{n?:number}={};const result:number=chain(a,b,'n',3);console.log(result+1);console.log(JSON.stringify(a));console.log(JSON.stringify(b));const holder:{child?:{text:string}}={};const child={text:'owned'+'!'};console.log(setChild(holder,'child',child)===child);const exact:number=assign(a,'n',4.5);console.log(exact);console.log(JSON.stringify(holder));try{chain(a,b,'missing',8);}catch(error){console.log(error instanceof TypeError);}console.log(a.n);console.log(b.n);}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &AssignmentSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            [
                "varta log: 4",
                "varta log: {\"n\":3}",
                "varta log: {\"n\":3}",
                "varta log: 1",
                "varta log: 4.5",
                "varta log: {\"child\":{\"text\":\"owned!\"}}",
                "varta log: 1",
                "varta log: 4.5",
                "varta log: 8"
            ]
        );
    }
}

#[test]
fn public_engine_runs_simple_assignment_values_across_virtual_modules() {
    struct AssignmentSources;
    impl ModuleLoader for AssignmentSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./simple.ts" {
                Ok("simple.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function assign(target:{n?:number},values:(number|undefined)[],item:number):number{let local:number|string=0;return local=target.n=values[0]=item;}export function setFunction():()=>string{let fn:()=>string=()=>'';const text='owned'+'!';return fn=()=>text;}".into())
        }
    }
    let source = "import {assign,setFunction} from './simple.ts';export function main():void{const target:{n?:number}={};const values:(number|undefined)[]=[undefined];console.log(assign(target,values,3)+1);console.log(JSON.stringify(target));console.log(JSON.stringify(values));const selected=setFunction();console.log(selected());let number=0;console.log(number=target.n=values[0]=4.5);try{console.log(target['missing'+ '']=number=values[0]=8);}catch(error){console.log(error instanceof TypeError);}console.log(number);console.log(target.n);console.log(JSON.stringify(values));}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &AssignmentSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            [
                "varta log: 4",
                "varta log: {\"n\":3}",
                "varta log: [3]",
                "varta log: owned!",
                "varta log: 4.5",
                "varta log: 1",
                "varta log: 8",
                "varta log: 4.5",
                "varta log: [8]"
            ]
        );
    }
}

#[test]
fn public_engine_runs_compound_assignments_across_virtual_modules() {
    struct CompoundSources;
    impl ModuleLoader for CompoundSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./compound.ts" {
                Ok("compound.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function numeric(record:{n?:number},values:number[],amount:number):number{let local=1;return local+=record.n*=values[0]+=amount;}export function append(record:{s:string},suffix:string):string{return record.s+=suffix;}".into())
        }
    }
    let source = "import {numeric,append} from './compound.ts';export function main():void{const record:{n?:number}={n:2};const values=[4];console.log(numeric(record,values,3));console.log(JSON.stringify(record));console.log(JSON.stringify(values));const text={s:'old'};console.log(append(text,'new'));console.log(record.n%=5);const missing:{n?:number}={};try{numeric(missing,values,1);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &CompoundSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            [
                "varta log: 15",
                "varta log: {\"n\":14}",
                "varta log: [7]",
                "varta log: oldnew",
                "varta log: 4",
                "varta log: 1",
                "varta log: 0",
                "varta log: [8]"
            ]
        );
    }
}

#[test]
fn public_engine_runs_record_compounds_across_virtual_modules() {
    struct ComputedCompoundSources;
    impl ModuleLoader for ComputedCompoundSources {
        fn resolve(&self, _: &Path, name: &str) -> Result<PathBuf, CompilerError> {
            if name == "./computed.ts" {
                Ok("computed.ts".into())
            } else {
                Err(CompilerError::Module("unknown virtual dependency".into()))
            }
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Ok("export function numeric(record:{n?:number},key:string,values:number[],amount:number):number{return record[key]*=values[0]+=amount;}export function append(record:{s:string},key:string,suffix:string):string{return record[key]+=suffix;}".into())
        }
    }
    let source = "import {numeric,append} from './computed.ts';export function main():void{let local=1;const record:{n?:number}={n:2};const values=[4];console.log(local+=numeric(record,'n',values,3));console.log(JSON.stringify(record));console.log(JSON.stringify(values));const text={s:'old'};console.log(append(text,'s','new'));console.log(record['n'+'']%=5);const missing:{n?:number}={};try{numeric(missing,'n',values,1);}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(missing,'n'));console.log(JSON.stringify(values));}";
    for profile in [
        varta_common::language::LanguageProfile::CheckedF64Utf16V12,
        varta_common::language::LanguageProfile::CheckedF64Utf16V14,
    ] {
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: "entry.ts".into(),
                text: source.into(),
            },
            &ComputedCompoundSources,
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compilation.wasm, &[])
                .unwrap()
                .lines,
            ["15", "{\"n\":14}", "[7]", "oldnew", "4", "1", "0", "[8]"]
                .map(|s| format!("varta log: {s}")),
            "{profile:?}"
        );
    }
}
