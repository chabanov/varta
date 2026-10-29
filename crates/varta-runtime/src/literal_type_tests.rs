use super::*;
use varta_common::language::LanguageProfile as P;
/// The documented v32 bounds (`literals::MAX_LITERAL_MEMBERS`,
/// `MAX_LITERAL_TABLE_MEMBERS` and `MAX_UNION_MEMBERS` in varta-compiler).
const LITERAL_MEMBERS: usize = 4096;
const LITERAL_TABLE_MEMBERS: usize = 65_536;
const UNION_MEMBERS: usize = 16;
fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
fn main_wrapped(source: &str) -> String {
    if source.contains("function main") {
        source.to_owned()
    } else {
        format!("{source}export function main():void{{}}")
    }
}

#[test]
fn literal_types_annotate_bindings_parameters_results_and_fields() {
    run(
        "type Kind='none'|'apiKey';type Level=1|2|3;interface Auth{type:Kind;level:Level;strict:true}\
         function flip(k:Kind):Kind{return k==='none'?'apiKey':'none';}\
         function describe(a:Auth):string{return a.type+':'+String(a.level)+':'+String(a.strict);}\
         export function main():void{let k:Kind='none';console.log(k);k=flip(k);console.log(k);\
         const lv:Level=2;const a:Auth={type:k,level:lv,strict:true};console.log(describe(a));\
         a.type='none';a.level=3;console.log(describe(a));const s:string=a.type;console.log(s.length);\
         const n:number=a.level*2;console.log(n);console.log(JSON.stringify(a));}",
        &[
            "none",
            "apiKey",
            "apiKey:2:true",
            "none:3:true",
            "4",
            "6",
            "{\"type\":\"none\",\"level\":3,\"strict\":true}",
        ],
    );
}
#[test]
fn literal_sets_mix_with_null_numbers_and_other_union_members() {
    run(
        "type Kind='a'|'b';type MaybeKind=Kind|null;type Mixed='x'|'y'|number;\
         function show(m:MaybeKind):string{if(m===null){return 'null';}const k:Kind=m;return k;}\
         function mixed(v:Mixed):string{if(typeof v==='string'){const s:'x'|'y'=v;return s+'!';}return String(v+1);}\
         export function main():void{let m:MaybeKind=null;console.log(show(m));m='b';console.log(show(m));\
         console.log(mixed('x'));console.log(mixed(41));const u:'a'|'b'|undefined=undefined;console.log(u===undefined);\
         const w:Kind|string='zzz';console.log(w);const bools:true|false=false;console.log(String(bools));}",
        &["null", "b", "x!", "42", "1", "zzz", "false"],
    );
}
#[test]
fn literal_guards_narrow_bases_unions_and_literal_sets() {
    run(
        "type Kind='none'|'apiKey'|'oauth';function need(k:'apiKey'):string{return 'key:'+k;}\
         function rest(k:'none'|'oauth'):string{return 'rest:'+k;}\
         function route(k:Kind):string{if(k==='apiKey'){return need(k);}return rest(k);}\
         function parse(s:string):Kind|null{if(s==='none'||s==='oauth'){return s;}if('apiKey'===s){return s;}return null;}\
         function opt(k:Kind|null):string{if(k!==null&&k!=='none'){const r:'apiKey'|'oauth'=k;return r;}return '-';}\
         export function main():void{console.log(route('apiKey'));console.log(route('oauth'));\
         console.log(parse('none'));console.log(parse('apiKey'));console.log(parse('other')===null);\
         console.log(opt('oauth'));console.log(opt(null));console.log(opt('none'));\
         const n:number=2;if(n===2){const two:2=n;console.log(two);}\
         let x:number|null=0;if(x===0){console.log(x===null);console.log(x+1);}\
         const v:string|number|boolean=true;if(typeof v==='string'||typeof v==='number'){console.log(String(v));}else{console.log(v);}}",
        &[
            "key:apiKey",
            "rest:oauth",
            "none",
            "apiKey",
            "1",
            "oauth",
            "-",
            "-",
            "2",
            "0",
            "1",
            "1",
        ],
    );
}
#[test]
fn literal_values_compare_and_operate_as_their_base_primitive() {
    run(
        "type Code='E_ONE'|'E_TWO';type Port=80|443;\
         export function main():void{const c:Code='E_TWO';const s='E_'+'TWO';console.log(c===s);console.log(c!=='E_ONE');\
         console.log(c.slice(2));console.log(c.startsWith('E_'));console.log(c.length);console.log(c<'E_THREE');\
         const p:Port=443;console.log(p+1);console.log(p>80);console.log(p===443);console.log(Math.max(p,100));\
         const t:true=true;console.log(t&&p===443);let total=0;for(const x of [p,80]){if(x!==undefined){total+=x;}}console.log(total);\
         switch(p){case 80:console.log('http');break;case 443:console.log('https');break;}}",
        &[
            "1", "1", "TWO", "1", "5", "0", "444", "1", "1", "443", "1", "523", "https",
        ],
    );
}
#[test]
fn literal_sets_flow_through_arrays_callbacks_and_inference() {
    run(
        "type Mode='r'|'w';type Op=(m:Mode)=>string;\
         function each(ms:Mode[],op:Op):string{let out='';for(const m of ms){if(m!==undefined){out+=op(m);}}return out;}\
         export function main():void{const modes:Mode[]=['r','w','r'];modes.push('w');console.log(modes.length);\
         console.log(modes.includes('w'));console.log(modes.indexOf('w'));const first=modes[0];\
         if(first!==undefined){const m:Mode=first;console.log(m);}\
         console.log(each(modes,(m)=>m==='r'?'R':'W'));const copy=modes;const k=modes[1];console.log(JSON.stringify(copy));\
         const kept=first;if(kept!==undefined){const again:Mode=kept;console.log(again);}\
         const pairs:('a'|1)[]=['a',1];console.log(JSON.stringify(pairs));if(k!==undefined){console.log(k);}}",
        &[
            "4",
            "1",
            "1",
            "r",
            "RWRW",
            "[\"r\",\"w\",\"r\",\"w\"]",
            "r",
            "[\"a\",1]",
            "w",
        ],
    );
}
#[test]
fn optional_literal_fields_read_as_literal_unions() {
    run(
        "type Header='Authorization';interface Auth{type:'none'|'apiKey';header?:Header}\
         function header(a:Auth):string{const h=a.header;if(h===undefined){return 'none';}const x:Header=h;return x;}\
         export function main():void{console.log(header({type:'none'}));console.log(header({type:'apiKey',header:'Authorization'}));\
         const a:Auth={type:'apiKey'};a.header='Authorization';console.log(header(a));console.log(JSON.stringify(a));}",
        &[
            "none",
            "Authorization",
            "Authorization",
            "{\"type\":\"apiKey\",\"header\":\"Authorization\"}",
        ],
    );
}
#[test]
fn base_values_and_non_members_are_not_assignable_to_literal_sets() {
    let prelude = "type Kind='none'|'apiKey';type Port=80|443;interface Auth{type:Kind;port:Port}\
                   function need(k:Kind):void{}function text():string{return 'none';}";
    for body in [
        "const k:Kind='other';",
        "const s:string='none';const k:Kind=s;",
        "let k:Kind='none';k=text();",
        "let k:Kind|null=null;k=text();",
        "let k:Kind|null=null;k='zzz';",
        "need(text());",
        "need('nonee');",
        "const a:Auth={type:text(),port:80};",
        "const a:Auth={type:'none',port:81};",
        "const a:Auth={type:'none',port:80};a.type=text();",
        "const a:Auth={type:'none',port:80};a.port=a.port+1;",
        "const a:Auth={type:'none',port:80};a.type+='x';",
        "const a:Auth={type:'none',port:80};a.port++;",
        "let k:Kind='none';k+='x';",
        "let p:Port=80;p++;",
        "let p:Port=80;p+=1;",
        "let p:Port=80;p=-p;",
        "let t:true=true;t=!t;",
        "const t:true=false;",
        "const ks:Kind[]=['none'];ks.push(text());",
        "const ks:Kind[]=['none'];ks[0]=text();",
        "const ks:Kind[]=[text()];",
        "const ks:Kind[]=['none'];ks.fill(text());",
        "const ks:Kind[]=['none'];const ss:string[]=ks;",
        "const ss:string[]=['none'];const ks:Kind[]=ss;",
        "const f:()=>Kind=()=>text();",
        "const s=text();if(s!=='none'){const k:'none'=s;}",
        "const s=text();if(s==='none'||s==='x'){const k:Kind=s;}",
        "const k:Kind='none';const p:Port=k;",
        "const n:number=80;const p:Port=n;",
        "const a:Auth={type:'none',port:80};const key:string='type';a[key]=text();",
        "const ks:Kind[]=['none'];ks[ks.length]=text();",
        "const ks:Kind[]=['none'];ks.fill(text(),0);",
        "const ks:Kind[]=['none'];const ss:string[]=['x'];Object.assign(ks,ss);",
        "let k:Kind='none';[k]=[text()];",
        "const a:Auth={type:'none',port:80};({type:a.type}={type:text()});",
        "let k:Kind='none';for(k of [text()]){}",
        "let k:Kind='none';k||=text();",
        "const a:Auth={type:'none',port:80};a.type??=text();",
        "let k:Kind='none';k=1>2?'none':text();",
        "const [x=text()]:Kind[]=[];",
        "const {type=text()}:{type?:Kind}={};",
        "const ks:Kind[]=['none'];const r=ks.reduce((acc:Kind,k)=>text(),'none');",
    ] {
        let source = format!("{prelude}export function main():void{{{body}}}");
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(
            [
                "not assignable to literal type",
                "type mismatch",
                "matches no declared",
                "not a member of the expected union",
            ]
            .iter()
            .any(|expected| error.contains(expected)),
            "{source}\n{error}"
        );
    }
}
#[test]
fn literal_refinements_feed_compound_logical_and_checked_object_writes() {
    run(
        "type Kind='none'|'apiKey';interface Auth{type:Kind}function text(v:string):string{return v;}\
         export function main():void{let s:string=text('a');if(s==='a'){s+='b';console.log(s);}\
         let p:number=1;if(p===1){p++;console.log(p);}let k:Kind|null=null;k??='apiKey';console.log(k);\
         const a:Auth={type:'none'};const typed:Auth={type:'apiKey'};Object.assign(a,typed);console.log(a.type);\
         const wide={type:text('none')};try{Object.assign(a,wide);}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         console.log(a.type);const loose:{type:string}={type:'x'};Object.assign(loose,typed);console.log(loose.type);}",
        &[
            "ab",
            "2",
            "apiKey",
            "apiKey",
            "Invalid computed record field value",
            "apiKey",
            "apiKey",
        ],
    );
}
fn wide_alias(name: &str, prefix: &str, count: usize) -> String {
    let members: Vec<_> = (0..count).map(|i| format!("'{prefix}{i}'")).collect();
    format!("type {name}={};", members.join("|"))
}
#[test]
fn wide_literal_unions_stay_constrained_primitives_up_to_the_member_bound() {
    // Generated API code declares error-code unions with hundreds of members.
    let source = format!(
        "{}{}type Both=Code|Extra|null;interface Problem{{code:Code;extra?:Extra}}\
         function code(p:Problem):Code{{return p.code;}}\
         function check(b:Both):string{{if(b===null){{return 'null';}}if(b==='C4091'||b==='X3'){{return 'edge:'+b;}}return b;}}\
         export function main():void{{const p:Problem={{code:'C187'}};console.log(code(p));console.log(check('C4091'));\
         console.log(check('X3'));console.log(check('C0'));console.log(check(null));}}",
        wide_alias("Code", "C", LITERAL_MEMBERS - 4),
        wide_alias("Extra", "X", 4),
    );
    run(&source, &["C187", "edge:C4091", "edge:X3", "C0", "null"]);
    for source in [
        format!(
            "{}export function main():void{{const c:Code='C0';}}",
            wide_alias("Code", "C", LITERAL_MEMBERS + 1)
        ),
        format!(
            "{}{}type All=Code|Extra;export function main():void{{const c:All='C0';}}",
            wide_alias("Code", "C", LITERAL_MEMBERS),
            wide_alias("Extra", "X", 1),
        ),
    ] {
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(
            error.contains("literal type member count exceeds limit"),
            "{error}"
        );
    }
    // Literal members do not count against the structural union member limit.
    let structural = (0..=UNION_MEMBERS)
        .map(|i| format!("{{f{i}:number}}"))
        .collect::<Vec<_>>()
        .join("|");
    let error = rejected(
        &format!("type Many='a'|'b'|{structural};export function main():void{{const m:Many='a';}}"),
        P::CheckedF64Utf16V32,
    );
    assert!(
        error.contains("union member count exceeds limit"),
        "{error}"
    );
    // All interned sets of one compilation share one member budget.
    let mut tables = String::new();
    for i in 0..=LITERAL_TABLE_MEMBERS / LITERAL_MEMBERS {
        tables.push_str(&wide_alias(
            &format!("T{i}"),
            &format!("t{i}_"),
            LITERAL_MEMBERS,
        ));
        tables.push_str(&format!("function use{i}(v:T{i}):void{{}}"));
    }
    let error = rejected(
        &format!("{tables}export function main():void{{}}"),
        P::CheckedF64Utf16V32,
    );
    assert!(
        error.contains("literal type table exceeds limit"),
        "{error}"
    );
}
#[test]
fn literal_type_forms_normalize_and_reject_unsupported_literals() {
    run(
        "type A='x'|'y';type B=A|'z';type Neg=-1|0|1;type Flag=true|false;type Wide='q'|string;\
         function sign(n:number):Neg{if(n<0){return -1;}if(n===0){return 0;}return 1;}\
         export function main():void{const b:B='z';const a:A='y';const c:B=a;console.log(b+c);\
         console.log(sign(-5));console.log(sign(0));const z:Neg=-0;console.log(z);\
         const raw:boolean=1>2;const f:Flag=raw;console.log(String(f));const w:Wide='anything';console.log(w);\
         const s:'é'|'\\u{1F600}'='\\u{1F600}';console.log(s.length);}",
        &["zy", "-1", "0", "0", "false", "anything", "2"],
    );
    for (source, expected) in [
        (
            "type T=`a${string}`;function f(t:T):void{}",
            "template literal type",
        ),
        ("type T=1n;function f(t:T):void{}", "bigint literal type"),
        (
            "function f(x:{kind:'a'}):void{}export function main():void{const y:{kind:string}={kind:'a'};f(y);}",
            "mismatch",
        ),
        (
            "function f(x:'a'|'b'):void{}export function main():void{f('c');}",
            "value 'c' is not assignable to literal type 'a' | 'b'",
        ),
    ] {
        let error = rejected(&main_wrapped(source), P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
#[test]
fn literal_sets_are_not_admitted_as_json_parse_targets() {
    // The schema ABI has no value sets, so parsing would admit non-members into
    // literal-typed places; parse the base primitive and narrow it instead.
    for target in [
        "Kind",
        "Auth",
        "Auth[]",
        "Kind|null",
        "{inner:Auth}",
        "{n:1|2}",
    ] {
        let source = format!(
            "type Kind='none'|'apiKey';interface Auth{{type:Kind}}\
             export function main():void{{const v=varta.json.parse<{target}>('null');}}"
        );
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(
            error.contains("JSON parsing target with a literal type"),
            "{target}\n{error}"
        );
    }
    run(
        "type Kind='none'|'apiKey';interface Wire{type:string}interface Auth{type:Kind}\
         function narrow(w:Wire):Auth|null{const t=w.type;if(t==='none'||t==='apiKey'){return {type:t};}return null;}\
         export function main():void{const ok=narrow(varta.json.parse<Wire>('{\"type\":\"apiKey\"}'));\
         if(ok!==null){console.log(ok.type);console.log(JSON.stringify(ok));}\
         console.log(narrow(varta.json.parse<Wire>('{\"type\":\"other\"}'))===null);}",
        &["apiKey", "{\"type\":\"apiKey\"}", "1"],
    );
}
#[test]
fn literal_types_capture_into_closures_and_callbacks() {
    run(
        "type Mode='r'|'w';function apply(f:(m:Mode)=>Mode,m:Mode):Mode{return f(m);}\
         export function main():void{const fixed:Mode='w';const swap=(m:Mode):Mode=>m==='r'?fixed:'r';\
         console.log(apply(swap,'r'));console.log(apply(swap,'w'));const s:string='r';\
         if(s==='r'){const pick=():Mode=>s;console.log(pick());}\
         const all:Mode[]=['r','w'];console.log(all.filter((m)=>m==='w').length);\
         console.log(JSON.stringify(all.map((m)=>m+'!')));}",
        &["w", "r", "r", "1", "[\"r!\",\"w!\"]"],
    );
}
#[test]
fn literal_types_stay_rejected_in_v31() {
    for source in [
        "type K='a'|'b';function f(k:K):void{}export function main():void{}",
        "function f(k:'a'):void{}export function main():void{}",
        "interface A{type:'none'|'apiKey'}function f(a:A):void{}export function main():void{}",
        "function f(n:1|2):void{}export function main():void{}",
        "function f(t:true):void{}export function main():void{}",
    ] {
        let error = rejected(source, P::CheckedF64Utf16V31);
        assert!(error.contains("unsupported type annotation"), "{error}");
        compile(source, P::CheckedF64Utf16V32).unwrap();
    }
    // Unused declarations remain ignored, and wide unions keep the v31 limit.
    compile(
        "type K='a'|'b';export function main():void{}",
        P::CheckedF64Utf16V31,
    )
    .unwrap();
    let wide = (0..=UNION_MEMBERS)
        .map(|_| "number")
        .collect::<Vec<_>>()
        .join("|");
    let error = rejected(
        &format!("function f(x:{wide}):void{{}}export function main():void{{}}"),
        P::CheckedF64Utf16V31,
    );
    assert!(
        error.contains("union member count exceeds limit"),
        "{error}"
    );
}
#[test]
fn erased_shapes_and_signatures_coexist_with_their_base_twins() {
    // After erasure `{type:Kind}` and `{type:string}` share a physical layout,
    // and `(m:Mode)=>Mode` and `(m:string)=>string` share one signature.
    run(
        "type Kind='a'|'b';interface Lit{type:Kind}interface Wide{type:string}type Mode='r'|'w';\
         function both(l:Lit|null,w:Wide|null):string{return JSON.stringify(l)+JSON.stringify(w);}\
         export function main():void{const l:Lit={type:'a'};const w:Wide={type:'z'};\
         const ls:Lit[]=[l];const ws:Wide[]=[w];const lit=(m:Mode):Mode=>m==='r'?'w':'r';const wide=(m:string):string=>m+m;\
         const higher=(f:(m:Mode)=>Mode):Mode=>f('r');const plain=(f:(m:string)=>string):string=>f('q');\
         const show=():string=>l.type+w.type+String(ls.length+ws.length);\
         console.log(both(l,w));console.log(both(null,w));console.log(lit('r')+wide('x'));\
         console.log(higher(lit)+plain(wide));console.log(show());console.log(JSON.stringify({ls,ws}));}",
        &[
            "{\"type\":\"a\"}{\"type\":\"z\"}",
            "null{\"type\":\"z\"}",
            "wxx",
            "wqq",
            "az2",
            "{\"ls\":[{\"type\":\"a\"}],\"ws\":[{\"type\":\"z\"}]}",
        ],
    );
}
