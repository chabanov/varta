//! v32 `generics()`: generic interfaces/aliases and monomorphized generic
//! functions, `keyof`, indexed access and the built-in generic references.
use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    let expected: Vec<String> = expected.iter().map(|s| format!("varta log: {s}")).collect();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
fn exported_functions(bytes: &[u8]) -> usize {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .filter_map(|payload| match payload {
            Ok(wasmparser::Payload::FunctionSection(section)) => Some(section.count() as usize),
            _ => None,
        })
        .sum()
}

#[test]
fn generic_interfaces_and_aliases_instantiate_structurally() {
    run(
        "interface CursorPage<T>{items:T[];cursor:string|null;has_more?:boolean}\
         type Pair<A,B=A>={first:A;second:B};type Maybe<T>=T|null;\
         type Names=CursorPage<string>;\
         function count(p:CursorPage<number>):number{return p.items.length;}\
         function total(p:{items:number[];cursor:string|null;has_more?:boolean}):number{\
         let s=0;for(const n of p.items){s+=n;}return s;}\
         function swap(p:Pair<string,number>):Pair<number,string>{return {first:p.second,second:p.first};}\
         export function main():void{const page:CursorPage<number>={items:[1,2,3],cursor:null};\
         console.log(count(page));console.log(total(page));\
         const names:Names={items:['a'],cursor:'c1',has_more:true};console.log(names.items[0]+names.cursor);\
         const same:Pair<boolean>={first:true,second:false};console.log(String(same.first&&!same.second));\
         const s=swap({first:'x',second:7});console.log(s.first+1);console.log(s.second);\
         let m:Maybe<string>=null;console.log(String(m===null));m='set';console.log(m);\
         console.log(JSON.stringify(page));}",
        &[
            "3",
            "6",
            "ac1",
            "true",
            "8",
            "x",
            "true",
            "set",
            "{\"items\":[1,2,3],\"cursor\":null}",
        ],
    );
}

#[test]
fn generic_functions_monomorphize_with_explicit_and_inferred_arguments() {
    let source = "function identity<T>(x:T):T{return x;}\
         function first<T>(xs:T[],fallback:T):T{const x=xs[0];if(x===undefined){return fallback;}return x;}\
         function wrap<T>(value:T):{value:T}{return {value};}\
         function orElse<T>(value:T|undefined,fallback:T):T{return value===undefined?fallback:value;}\
         function apply<T,U>(x:T,f:(v:T)=>U):U{return f(x);}\
         function empty<T>():T[]{return [];}\
         export function main():void{console.log(identity<number>(41)+1);console.log(identity('s'));\
         console.log(String(identity(true)));console.log(first([3,4],0));console.log(first<string>([],'none'));\
         console.log(wrap(5).value);console.log(JSON.stringify(wrap('w')));\
         const missing:string|undefined=undefined;console.log(orElse(missing,'dflt'));\
         console.log(orElse<number>(2,9));console.log(apply(3,(v:number):string=>'n'+v));\
         const xs:number[]=empty();xs.push(1);console.log(xs.length);\
         console.log(identity(identity<number>(7)));}";
    run(
        source,
        &[
            "42",
            "s",
            "true",
            "3",
            "none",
            "5",
            "{\"value\":\"w\"}",
            "dflt",
            "2",
            "n3",
            "1",
            "7",
        ],
    );
    // One emitted function per distinct instantiation; repeated uses share it.
    let once = compile(
        "function id<T>(x:T):T{return x;}export function main():void{console.log(id(1));}",
        P::CheckedF64Utf16V32,
    )
    .unwrap();
    let shared = compile(
        "function id<T>(x:T):T{return x;}export function main():void{console.log(id(1));\
         console.log(id(2));console.log(id<number>(3));}",
        P::CheckedF64Utf16V32,
    )
    .unwrap();
    let distinct = compile(
        "function id<T>(x:T):T{return x;}export function main():void{console.log(id(1));\
         console.log(id('a'));console.log(id(true));}",
        P::CheckedF64Utf16V32,
    )
    .unwrap();
    assert_eq!(exported_functions(&once), exported_functions(&shared));
    assert_eq!(exported_functions(&once) + 2, exported_functions(&distinct));
}

#[test]
fn instances_compose_with_nested_generics_closures_classes_and_literals() {
    run(
        "type Box<T>={value:T};type Mapper<T,U>=(value:T)=>U;interface Page<T>{items:T[];cursor:string|null}\
         function box<T>(value:T):Box<T>{return {value};}\
         function twice<T>(value:T):Box<Box<T>>{return box<Box<T>>(box(value));}\
         function map<T,U>(xs:T[],f:Mapper<T,U>):U[]{const out:U[]=[];for(const x of xs){if(x!==undefined){out.push(f(x));}}return out;}\
         function count<T>(page:Page<T>):number{const items:T[]=page.items;return items.length;}\
         function counter<T>(seed:T):()=>T{let last:T=seed;return ():T=>last;}\
         type Kind='a'|'b';function echo<T extends string>(value:T):T{return value;}\
         class Shelf{label:string;constructor(label:string){this.label=label;}\
         wrap(n:number):Box<string>{return box(this.label+n);}}\
         export function main():void{console.log(twice(3).value.value);\
         console.log(JSON.stringify(map([1,2],(n:number):string=>'#'+n)));\
         console.log(count<string>({items:['x','y'],cursor:null}));const page:Page<number>={items:[1],cursor:'c'};console.log(count(page));console.log(counter('c')());\
         const k:Kind=echo<Kind>('b');console.log(k);console.log(echo('free'));\
         console.log(new Shelf('s').wrap(1).value);console.log(depth(['p','q','r'],0));console.log(even(4));}\
         function depth<T>(xs:T[],i:number):number{return i>=xs.length?i:depth(xs,i+1);}\
         function even<T>(n:T):boolean{return odd(n);}function odd(n:number):boolean{return n===0?false:even<number>(n-1);}",
        &[
            "3",
            "[\"#1\",\"#2\"]",
            "2",
            "1",
            "c",
            "b",
            "free",
            "s1",
            "3",
            "0",
        ],
    );
}

/// The language-guide example for v32 generics.
#[test]
fn language_guide_generics_example_runs() {
    run(
        "interface CursorPage<T> {\n  items: T[];\n  cursor: string | null;\n  has_more?: boolean;\n}\n\
         interface Agent {\n  id: string;\n  name: string;\n}\ntype AgentKey = keyof Agent;\n\
         function collect<T>(pages: CursorPage<T>[]): T[] {\n  const out: T[] = [];\n\
           for (const page of pages) {\n    if (page === undefined) continue;\n\
             for (const item of page.items) if (item !== undefined) out.push(item);\n  }\n  return out;\n}\n\
         export function main(): void {\n\
           const pages: CursorPage<Agent>[] = [\n    { items: [{ id: 'a1', name: 'Ada' }], cursor: 'next' },\n\
             { items: [{ id: 'a2', name: 'Lin' }], cursor: null },\n  ];\n\
           const agents = collect(pages);\n  console.log(agents.length);\n\
           const ids = collect<string>([{ items: ['x', 'y'], cursor: null }]);\n  console.log(JSON.stringify(ids));\n\
           const key: AgentKey = 'name';\n  console.log(key);\n\
           const flags: Record<'read' | 'write', boolean> = { read: true, write: false };\n\
           const patch: Partial<Agent> = { name: 'renamed' };\n\
           console.log(JSON.stringify(flags) + JSON.stringify(patch));\n}\n",
        &[
            "2",
            "[\"x\",\"y\"]",
            "name",
            "{\"read\":true,\"write\":false}{\"name\":\"renamed\"}",
        ],
    );
}

#[test]
fn keyof_constraints_and_indexed_access_follow_record_fields() {
    run(
        "interface Params{limit:number;cursor:string;q?:string}\
         function get<T extends object,K extends keyof T>(source:T,key:K):T[K]{return source[key];}\
         function names<T extends object,K extends readonly (keyof T & string)[]>(source:T|undefined,keys:K):number{\
         if(source===undefined){return 0;}return keys.length;}\
         type Key=keyof Params;type Limit=Params['limit'];type Element=string[][number];\
         export function main():void{const p:Params={limit:2,cursor:'c'};\
         const k:Key='cursor';console.log(k);const l:Limit=5;console.log(l);const e:Element='e';console.log(e);\
         console.log(names(p,['limit','cursor']));const none:Params|undefined=undefined;\
         console.log(names(none,['q']));}",
        &["cursor", "5", "e", "2", "0"],
    );
}

#[test]
fn built_in_generic_references_map_onto_closed_types() {
    run(
        "interface User{name:string;age:number}\
         function touch(u:Partial<User>):string{const n=u.name;if(n===undefined){return 'anon';}return n;}\
         function size(xs:ReadonlyArray<number>):number{return xs.length;}\
         function label(u:Readonly<User>):string{return u.name+u.age;}\
         function sum(xs:readonly number[]):number{let s=0;for(const x of xs){s+=x;}return s;}\
         type Flags=Record<'read'|'write',boolean>;\
         export function main():void{console.log(touch({}));console.log(touch({name:'ann'}));\
         console.log(size([1,2]));console.log(label({name:'b',age:3}));console.log(sum([1,2,3]));\
         const f:Flags={read:true,write:false};console.log(JSON.stringify(f));}",
        &[
            "anon",
            "ann",
            "2",
            "b3",
            "6",
            "{\"read\":true,\"write\":false}",
        ],
    );
}

#[test]
fn generics_reject_out_of_scope_forms_with_reasons() {
    let v32 = P::CheckedF64Utf16V32;
    for (source, message) in [
        (
            "type Rec=Record<string,number>;export function main():void{const r:Rec={a:1};}",
            "dynamic keys",
        ),
        (
            "function f(p:Promise<number>):void{}export function main():void{}",
            "async suspension",
        ),
        (
            "interface List<T>{v:T;next:List<T>|null}export function main():void{const l:List<number>={v:1,next:null};}",
            "recursive type declaration",
        ),
        (
            "function id<T>(x:T):T{return x;}export function main():void{const g=id;}",
            "generic function as a value",
        ),
        (
            "function make<T>():T[]{return [];}export function main():void{console.log(make().length);}",
            "cannot infer type argument T",
        ),
        (
            "function f<T extends string>(x:T):T{return x;}export function main():void{f<number>(1);}",
            "does not satisfy its constraint",
        ),
        (
            "function same<T>(a:T,b:T):T{return a;}export function main():void{same(1,'a');}",
            "argument type mismatch",
        ),
        (
            "function f<T>(g:(x:T)=>T):void{}export function main():void{f((x:number):string=>'a');}",
            "conflicting inferences",
        ),
        (
            "async function f<T>(x:T):Promise<T>{return x;}export function main():void{}",
            "async or generator generic function",
        ),
        (
            "class Box<T>{v:T;constructor(v:T){this.v=v;}}export function main():void{}",
            "generic class",
        ),
        (
            "export function main():void{const f=<T,>(x:T):T=>x;}",
            "async/generic/generator arrow",
        ),
        (
            "function deep<T>(x:T):number{return deep<T[]>([x]);}export function main():void{deep(1);}",
            "depth",
        ),
        (
            "type K=keyof {};export function main():void{const k:K='a';}",
            "keyof an empty object type",
        ),
        (
            "function unused<T>(x:T):any{return x;}export function main():void{}",
            "unsupported type annotation",
        ),
        (
            "function id<T>(x:T):T{return x;}function id2(x:number):number{return x;}\
             export function main():void{id2<number>(1);}",
            "generic call",
        ),
        (
            "function id<T>(x:T):T{return x;}function id(x:number):number{return x;}\
             export function main():void{}",
            "duplicate or reserved function name",
        ),
        (
            "function id<T>(x:T):T{return x;}export function main():void{const xs=[1];id(...xs);}",
            "spread argument to a generic function",
        ),
        (
            "interface Page<T>{items:T[];cursor:string|null}function count<T>(p:Page<T>):number{return p.items.length;}\
             export function main():void{count({items:[1],cursor:null});}",
            "were inferred from the arguments",
        ),
        (
            "type Box<T>={v:T};export function main():void{const b:Box={v:1};}",
            "requires a type argument for T",
        ),
        (
            "type Plain={v:number};export function main():void{const b:Plain<number>={v:1};}",
            "not generic",
        ),
        (
            "type Both={a:number}&{b:number};export function main():void{}\
             function f(x:Both):void{}",
            "intersection",
        ),
    ] {
        let error = rejected(source, v32);
        assert!(error.contains(message), "{source}\n{error}");
    }
}

#[test]
fn v31_keeps_rejecting_generic_declarations_references_and_calls() {
    let v31 = P::CheckedF64Utf16V31;
    for (source, message) in [
        (
            "interface Page<T>{items:T[]}export function main():void{}",
            "generic type declaration",
        ),
        (
            "type Box<T>={v:T};export function main():void{}",
            "generic type declaration",
        ),
        (
            "function f(x:Readonly<{a:number}>):void{}export function main():void{}",
            "generic type reference",
        ),
        (
            "function id<T>(x:T):T{return x;}export function main():void{}",
            "async, generator, generic or decorated function",
        ),
        (
            "type K=keyof {a:number};export function main():void{const k:K='a';}",
            "type annotation",
        ),
    ] {
        let error = rejected(source, v31);
        assert!(error.contains(message), "{source}\n{error}");
    }
}
