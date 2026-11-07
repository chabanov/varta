//! Regression coverage for the unsupported structural-thenable boundary.
use super::*;
use varta_common::language::LanguageProfile;
const PROFILES: [LanguageProfile; 2] = [
    LanguageProfile::CheckedF64Utf16V36,
    LanguageProfile::CheckedF64Utf16V38,
];
fn run(source: &str, expected: &[&str]) {
    for profile in PROFILES {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
            .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
        let result = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
        assert_eq!(
            result.lines,
            expected
                .iter()
                .map(|s| format!("varta log: {s}"))
                .collect::<Vec<_>>(),
            "{profile:?}"
        );
    }
}
/// V38 (`expression_forms`) resolves `await t` and `Promise.resolve(t)` of a
/// statically known thenable (`thenables`); every other resolution path,
/// a `then` getter and every path of earlier profiles keep refusing it.
#[test]
fn static_thenables_are_rejected_on_every_resolution_path() {
    let prelude = "interface Thenable { then: (resolve: (value:number)=>void)=>void } function make():Thenable {return {then:(resolve:(value:number)=>void):void=>{resolve(7);}};}";
    for (expression, admitted) in [
        ("console.log(String(await make()))", true),
        ("console.log(String(await Promise.resolve(make())))", true),
        ("Promise.resolve(1).then(()=>make())", false),
        ("new Promise<Thenable>((resolve)=>resolve(make()))", false),
    ] {
        let source =
            format!("{prelude} export async function main():Promise<void>{{ {expression}; }}");
        for profile in PROFILES {
            let compiled = varta_compiler::compile_typescript_with_profile(&source, profile);
            if admitted && profile == LanguageProfile::CheckedF64Utf16V38 {
                let bytes = compiled.unwrap_or_else(|e| panic!("{e}\n{source}"));
                let result = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
                assert_eq!(result.lines, ["varta log: 7"], "{source}");
                continue;
            }
            let error = compiled.unwrap_err();
            assert!(error.to_string().contains("thenable"), "{error}\n{source}");
        }
    }
    for (class, admitted) in [
        (
            "class T {then(resolve:(value:number)=>void):void{resolve(7);}}",
            true,
        ),
        ("class T {get then():number{return 7;}}", false),
        (
            "class Base {then(resolve:(value:number)=>void):void{resolve(7);}} class T extends Base {}",
            true,
        ),
    ] {
        let source = format!(
            "{class} export async function main():Promise<void>{{console.log(String(await new T()));}}"
        );
        for profile in PROFILES {
            let compiled = varta_compiler::compile_typescript_with_profile(&source, profile);
            if admitted && profile == LanguageProfile::CheckedF64Utf16V38 {
                let bytes = compiled.unwrap_or_else(|e| panic!("{e}\n{source}"));
                let result = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
                assert_eq!(result.lines, ["varta log: 7"], "{source}");
                continue;
            }
            let error = compiled.unwrap_err();
            assert!(error.to_string().contains("thenable"), "{error}");
        }
    }
}
#[test]
fn unknown_thenables_fail_before_the_hook_runs_and_rejections_are_catchable() {
    run(
        r#"class T { then(resolve:(v:number)=>void):void {console.log("must not run");resolve(7);} }
    async function identity(value:unknown):Promise<unknown>{return value;}
    async function attempt(value:unknown):Promise<void>{
        try {await value;} catch(e){console.log(String(e instanceof TypeError));}
        try {Promise.resolve(value);} catch(e){console.log(String(e instanceof TypeError));}
        await new Promise<unknown>((resolve)=>resolve(value)).catch((e)=>{console.log(String(e instanceof TypeError));});
        await Promise.resolve(1).then(()=>value).catch((e)=>{console.log(String(e instanceof TypeError));});
        await identity(value).catch((e)=>{console.log(String(e instanceof TypeError));});
        const pending=Promise.withResolvers<unknown>();
        pending.resolve(value); pending.resolve(2);
        await pending.promise.catch((e)=>{console.log(String(e instanceof TypeError));});
    }
    export async function main():Promise<void>{
        const t = {then:(resolve:(v:number)=>void):void=>{console.log("must not run");resolve(7);}};
        await attempt(t);
        await attempt(new T());
        const d:Record<string,unknown>={}; d["then"] = t.then; await attempt(d);
    }"#,
        &[
            "true", "true", "true", "true", "true", "true", "true", "true", "true", "true", "true",
            "true", "true", "true", "true", "true", "true", "true",
        ],
    );
}
#[test]
fn noncallable_then_absence_and_nested_thenables_remain_ordinary_values() {
    run(
        r#"async function select(v:number|{then:()=>number}):Promise<void>{
            try {await v;console.log("plain");} catch(e){console.log(String(e instanceof TypeError));}
        }
        export async function main():Promise<void>{
        const n = await {then:3}; console.log(n.then);
        const s = await Promise.resolve({then:"text"}); console.log(s.then);
        const nested = await {child:{then:():number=>7}}; console.log(nested.child.then());
        const optional:{then?:unknown;n:number}={n:4}; console.log((await optional).n);
        optional.then=undefined; console.log((await optional).n);
        optional.then=():number=>7;
        try {await optional;} catch(e){console.log(String(e instanceof TypeError));}
        const dictionary:Record<string,unknown>={}; dictionary["then"]=5;
        console.log((await dictionary)["then"]);
        dictionary["then"]=():number=>7;
        try {await dictionary;} catch(e){console.log(String(e instanceof TypeError));}
        delete dictionary["then"]; const empty = await dictionary; console.log(Object.keys(empty).length);
        const maybe:{then?:()=>number;n:number}={n:6}; console.log((await maybe).n);
        maybe.then=():number=>7; try {await maybe;} catch(e){console.log(String(e instanceof TypeError));}
        await select(3); await select({then:():number=>7});
    }"#,
        &[
            "3", "text", "7", "4", "4", "true", "5", "true", "0", "6", "true", "plain", "true",
        ],
    );
}

#[test]
fn resolution_checks_support_forward_class_types_wide_presence_and_typed_dictionaries() {
    let fields = (0..70).map(|i| format!("k{i}:number;")).collect::<String>();
    let values = (0..70).map(|i| format!("k{i}:{i},")).collect::<String>();
    let source = format!(
        r#"
        class Item {{ next():Promise<Item>{{return Promise.resolve(this);}} n:number=7; }}
        export async function main():Promise<void>{{
            const item = await new Item().next(); console.log(item.n);
            const wide:{{{fields} then?:unknown}}={{{values}}};
            const boxed:unknown=wide; await boxed; console.log("absent");
            wide.then=():number=>7;
            try {{await boxed;}} catch(e){{console.log(String(e instanceof TypeError));}}
            delete wide.then; await boxed; console.log("deleted");
            const dictionary:Record<string,()=>number>={{}};
            await dictionary; console.log("empty");
            dictionary["then"]=():number=>7;
            try {{await dictionary;}} catch(e){{console.log(String(e instanceof TypeError));}}
        }}"#
    );
    run(
        &source,
        &["7", "absent", "true", "deleted", "empty", "true"],
    );
}
