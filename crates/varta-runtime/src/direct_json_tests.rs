//! Direct checked-assertion decoders: compare complete values, ordering,
//! failures, single evaluation and resource behavior with the real fallback.
use super::VartaRuntime;
use crate::codegen_v2_tests::{Run, compile};
use crate::linked_json_tests::{cause, host, run_test_budgets};
use varta_common::language::LanguageProfile as P;
use varta_compiler::json_decoder_test_hook as hook;
const V38: P = P::CheckedF64Utf16V38;
fn reference(source: &str) -> Vec<u8> {
    hook::compile_assertions_reference(source, V38).unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn same(source: &str) -> (Run, Run) {
    let fast = run_test_budgets(&compile(source, V38), 100);
    let slow = run_test_budgets(&reference(source), 100);
    assert_eq!(fast.lines, slow.lines, "{source}");
    assert_eq!(cause(&fast.error), cause(&slow.error), "{source}");
    (fast, slow)
}
const CORPUS: &str = r#"
    type Event = { kind:"item"; id:number; tags:("x"|"y")[] }
               | { kind:"done"; ok:true; n:0|1; note?:string };
    function show(text:string):string {
        try { const event=JSON.parse(text) as Event; return JSON.stringify(event); }
        catch(e) {return e instanceof Error ? e.name+":"+e.message : "wrong";}
    }
    export function main():void {
        const texts=[
            '{"kind":"item","id":3,"tags":["x","y"]}',
            '{"tags":["y"],"id":2,"kind":"item"}',
            '{"n":-0,"ok":true,"kind":"done"}',
            '{"note":"","kind":"done","n":1,"ok":true}',
            '{"kind":"it\\u0065m","id":3,"tags":["x"]}',
            '{"k\\u0069nd":"done","n":1,"ok":true}',
            '{"kind":"done","ok":true,"n":1.0000000000000000000}',
            '{"kind":"other","id":1,"tags":[]}',
            '{"kind":"done","ok":false,"n":1}',
            '{"kind":"done","ok":true,"n":2}',
            '{"kind":"item","id":"x","tags":[]}',
            '{"kind":"item","id":1,"tags":["z"]}',
            '{"kind":"item","id":1,"tags":[],"extra":0}',
            '{"kind":"item","id":1,"tags":[],"id":2}',
            '{"kind":"done","ok":true,"n":1} trailing',
            '{"kind":"item",}', 'null', '[]', '1'
        ];
        for(const text of texts)console.log(show(text));
    }
"#;
#[test]
fn direct_union_values_and_fallbacks_match_the_checked_conversion() {
    let (fast, _) = same(CORPUS);
    assert_eq!(fast.error, None);
    assert_eq!(fast.lines.len(), 19);
    let production = VartaRuntime::new()
        .run_wasm_bytes(&compile(CORPUS, V38), &[])
        .unwrap();
    assert_eq!(production.lines, fast.lines);
    assert_eq!(
        fast.lines[1],
        "varta log: {\"tags\":[\"y\"],\"id\":2,\"kind\":\"item\"}"
    );
    for bytes in [
        hook::compile_generic(CORPUS, V38).unwrap(),
        host(CORPUS),
        hook::compile_collecting(CORPUS, V38).unwrap(),
    ] {
        let out = run_test_budgets(&bytes, 100);
        assert_eq!(out.error, None);
        assert_eq!(out.lines, fast.lines);
    }
}
#[test]
fn direct_union_parsing_evaluates_input_once_and_keeps_nested_values() {
    let source = r#"
        type Node={kind:"leaf";value:1|2}|{kind:"branch";children:Node[]};
        let calls=0;
        function input():string{calls++;return '{"kind":"branch","children":[{"value":2,"kind":"leaf"}]}';}
        function failing():string{calls++;throw new TypeError("input failed");}
        async function asynchronous():Promise<string>{await null;return input();}
        export async function main():Promise<void>{
            const tree=JSON.parse(await asynchronous()) as Node;
            console.log(JSON.stringify(tree));console.log(calls);
            const list=JSON.parse('[{"kind":"leaf","value":1}]') as Node[];
            console.log(JSON.stringify(list));
            try{const bad=JSON.parse(failing()) as Node;console.log("wrong");}
            catch(e){console.log(e instanceof Error?e.message:"wrong");}
            console.log(calls);
        }
    "#;
    let (fast, _) = same(source);
    assert_eq!(fast.error, None);
    assert_eq!(
        fast.lines,
        [
            "varta log: {\"kind\":\"branch\",\"children\":[{\"value\":2,\"kind\":\"leaf\"}]}",
            "varta log: 1",
            "varta log: [{\"kind\":\"leaf\",\"value\":1}]",
            "varta log: input failed",
            "varta log: 2",
        ]
    );
    let out = run_test_budgets(&hook::compile_collecting(source, V38).unwrap(), 100);
    assert_eq!(out.error, None);
    assert_eq!(out.lines, fast.lines);
}
#[test]
fn direct_union_scaling_avoids_rebuilding_an_unknown_graph() {
    for (variants, count) in [(2, 128), (16, 128), (32, 128), (32, 512), (200, 128)] {
        let types = (0..variants)
            .map(|n| format!("{{kind:'v{n}';values:number[]}}"))
            .collect::<Vec<_>>()
            .join("|");
        let values = (0..count)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("{{\"values\":[{values}],\"kind\":\"v{}\"}}", variants - 1);
        let source = format!(
            "type R={types};export function main():void{{\
            const text='{text}'.repeat(2).slice(0,{});const r=JSON.parse(text) as R;\
            console.log(r.values.length);}}",
            text.len()
        );
        let (fast, slow) = same(&source);
        assert_eq!(fast.error, None);
        println!(
            "{variants} variants / {count} values: direct {}, reference {}",
            fast.fuel, slow.fuel
        );
        if count == 512 {
            let out = run_test_budgets(&hook::compile_collecting(&source, V38).unwrap(), 100);
            assert_eq!(out.error, None);
            assert_eq!(out.lines, fast.lines);
        }
        assert!(
            fast.fuel < slow.fuel,
            "{variants} variants: fast {}, reference {}",
            fast.fuel,
            slow.fuel
        );
    }
}

#[test]
fn direct_discriminants_cover_boolean_number_nullable_and_distinct_names() {
    let source = r#"
        type Flag={ok:true;value:string}|{ok:false;error:string};
        type Named={alpha:"a";name:string}|{beta:2;name:string};
        type Value={tag:0;n:0|1|null}|{tag:1;text:""|"é"}|null;
        export function main():void {
            for(const text of ['{"value":"yes","ok":true}','{"error":"no","ok":false}']) {
                const f=JSON.parse(text) as Flag;console.log(f.ok?f.value:f.error);
            }
            for(const text of ['{"name":"x","alpha":"a"}','{"name":"y","beta":2}']) {
                console.log(JSON.stringify(JSON.parse(text) as Named));
            }
            for(const text of ['null','{"n":-0,"tag":0}','{"tag":0,"n":null}','{"text":"","tag":1}',
                '{"tag":1,"text":"é"}','{"tag":1,"text":"\\u00e9"}']) {
                const value=JSON.parse(text) as Value;console.log(JSON.stringify(value));
                if(value!==null&&value.tag===0)console.log(String(Object.is(value.n,-0)));
            }
        }
    "#;
    let (fast, _) = same(source);
    assert_eq!(fast.error, None);
    assert!(
        fast.lines.contains(&"varta log: true".to_owned()),
        "{:?}",
        fast.lines
    );
    let out = run_test_budgets(&hook::compile_collecting(source, V38).unwrap(), 100);
    assert_eq!(out.error, None);
    assert_eq!(out.lines, fast.lines);
}

#[test]
fn direct_decoder_misses_preserve_ambiguous_and_wide_conversion_rules() {
    let source = r#"
        type Overlap={kind:"a";name:string}|{kind:unknown;other:"b";name:string};
        type Ambiguous={value:number;note?:string}|{value:number;tag?:string};
        export function main():void {
            try{const r=JSON.parse('{"other":"b","kind":"a","name":"x"}') as Overlap;
                console.log(JSON.stringify(r));}catch(e){console.log(e instanceof Error?e.message:"wrong");}
            try{const r=JSON.parse('{"value":1}') as Ambiguous;
                console.log(JSON.stringify(r));}catch(e){console.log(e instanceof Error?e.message:"wrong");}
        }
    "#;
    let (fast, _) = same(source);
    assert_eq!(fast.error, None);
    assert_eq!(
        fast.lines[0],
        "varta log: {\"other\":\"b\",\"kind\":\"a\",\"name\":\"x\"}"
    );
    assert!(fast.lines[1].contains("ambiguous"), "{:?}", fast.lines);
    let fields = (0..70)
        .map(|n| format!("f{n}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let wide = format!(
        "type Wide={{kind:'wide';{fields}}};export function main():void{{\
        const r=JSON.parse('{{\"f69\":69,\"kind\":\"wide\",\"f1\":1}}') as Wide;console.log(JSON.stringify(r));}}"
    );
    let (fast, _) = same(&wide);
    assert_eq!(fast.error, None);
    assert_eq!(
        fast.lines,
        ["varta log: {\"f69\":69,\"kind\":\"wide\",\"f1\":1}"]
    );
}

#[test]
fn direct_assertions_do_not_erase_literal_checks_into_the_schema_abi() {
    let source = r#"type E={kind:"a";n:number}|{kind:"b";text:string};
        export function main():void{const x=varta.json.parse<E>('{"kind":"a","n":1}');console.log(x);}"#;
    for profile in [P::CheckedF64Utf16V37, V38] {
        let error = varta_compiler::compile_typescript_with_profile(source, profile)
            .unwrap_err()
            .to_string();
        assert!(error.contains("literal type"), "{error}");
    }
    let old =
        varta_compiler::compile_typescript_with_profile(CORPUS, P::CheckedF64Utf16V37).unwrap();
    let reference = hook::compile_assertions_reference(CORPUS, P::CheckedF64Utf16V37).unwrap();
    assert_eq!(old, reference);
}

#[test]
fn direct_decoder_seeded_inputs_and_mutations_match_the_reference() {
    let mut texts = Vec::new();
    let mut seed = 0x5a17_39cdu32;
    for n in 0..32 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let text = match n % 4 {
            0 => format!(
                "{{\"kind\":\"item\",\"id\":{},\"tags\":[\"x\"]}}",
                seed % 1000
            ),
            1 => format!(
                "{{\"tags\":[\"y\",\"x\"],\"id\":{},\"kind\":\"item\"}}",
                seed % 1000
            ),
            2 => format!("{{\"kind\":\"done\",\"ok\":true,\"n\":{}}}", seed % 2),
            _ => format!(
                "{{\"n\":{},\"note\":\"x\",\"ok\":true,\"kind\":\"done\"}}",
                seed % 2
            ),
        };
        texts.push(text.clone());
        texts.push(
            text.replace("\"item\"", "\"other\"")
                .replace("true", "false"),
        );
        texts.push(format!("{text}x"));
        texts.push(text[..text.len() - 1].to_owned());
    }
    // Sources are ASCII here. Encode the quote/backslash controls for the TS
    // string literal; the JSON input itself remains unchanged.
    let literal = |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
    let data = texts
        .iter()
        .map(|text| literal(text))
        .collect::<Vec<_>>()
        .join(",");
    let start = CORPUS.find("const texts=[").unwrap();
    let end = start
        + CORPUS[start..]
            .find("]; ")
            .unwrap_or_else(|| CORPUS[start..].find("];\n").unwrap())
        + 2;
    let mut source = CORPUS.to_owned();
    source.replace_range(start..end, &format!("const texts=[{data}];"));
    let (fast, _) = same(&source);
    assert_eq!(fast.error, None);
    assert_eq!(fast.lines.len(), texts.len());
}

#[test]
fn unknown_tag_overlap_keeps_valid_alternatives_and_rejects_ambiguity() {
    let source = r#"
        type S={kind:"a";value:number}|{kind:unknown;other:"b";value:number};
        type B={ok:true;value:number}|{ok:unknown;other:"b";value:number};
        type N={code:1;value:number}|{code:unknown;other:"b";value:number};
        type A={kind:"a";alt:unknown}|{kind:unknown;alt:"b"};
        function s(text:string):string{return JSON.stringify(JSON.parse(text) as S);}
        function b(text:string):string{
            const raw:unknown=JSON.parse(text);return JSON.stringify(raw as B);
        }
        function n(text:string):string{return JSON.stringify(JSON.parse(text) as N);}
        function a(text:string):string{
            try{return JSON.stringify(JSON.parse(text) as A);}
            catch(e){return e instanceof Error?e.message:"wrong";}
        }
        export function main():void {
            console.log(s('{"kind":"a","value":1}'));
            console.log(s('{"value":2,"other":"b","kind":"a"}'));
            console.log(b('{"ok":true,"other":"b","value":3}'));
            console.log(n('{"code":1,"value":4,"other":"b"}'));
            console.log(a('{"kind":"a","alt":"b"}'));
            console.log(a('{"kind":"c","alt":"b"}'));
            console.log(a('{"alt":"c","kind":"a"}'));
            console.log(a('{"kind":"c","alt":"c"}'));
        }
    "#;
    let (fast, _) = same(source);
    assert_eq!(fast.error, None);
    assert_eq!(fast.lines.len(), 8);
    assert_eq!(fast.lines[0], "varta log: {\"kind\":\"a\",\"value\":1}");
    assert_eq!(
        fast.lines[1],
        "varta log: {\"value\":2,\"other\":\"b\",\"kind\":\"a\"}"
    );
    assert_eq!(
        fast.lines[2],
        "varta log: {\"ok\":true,\"other\":\"b\",\"value\":3}"
    );
    assert_eq!(
        fast.lines[3],
        "varta log: {\"code\":1,\"value\":4,\"other\":\"b\"}"
    );
    assert!(fast.lines[4].contains("ambiguous"), "{:?}", fast.lines);
    assert_eq!(fast.lines[5], "varta log: {\"kind\":\"c\",\"alt\":\"b\"}");
    assert_eq!(fast.lines[6], "varta log: {\"alt\":\"c\",\"kind\":\"a\"}");
    assert_eq!(fast.lines[7], "varta log: type assertion to A failed");
    for bytes in [
        hook::compile_generic(source, V38).unwrap(),
        host(source),
        hook::compile_collecting(source, V38).unwrap(),
    ] {
        let out = run_test_budgets(&bytes, 100);
        assert_eq!(out.error, None);
        assert_eq!(out.lines, fast.lines);
    }
}

#[test]
fn direct_attempts_preserve_non_schema_type_admission() {
    for field in [
        "object",
        "[number,string]",
        "Map<string,number>",
        "Set<number>",
        "Uint8Array",
        "Error",
        "Promise<number>",
        "()=>number",
        "C",
    ] {
        let source = format!(
            "class C{{value:number=1;}}type R={{kind:'x';data:{field}}};\
             export function main():void{{try{{\
             const value=JSON.parse('{{\"kind\":\"x\",\"data\":{{}}}}') as R;\
             console.log(JSON.stringify(value));}}\
             catch(e){{console.log(e instanceof Error?e.name+':'+e.message:'wrong');}}}}"
        );
        let fast = varta_compiler::compile_typescript_with_profile(&source, V38);
        let generic = hook::compile_generic(&source, V38);
        match (fast, generic) {
            (Ok(fast), Ok(generic)) => {
                let actual = run_test_budgets(&fast, 100);
                let expected = run_test_budgets(&generic, 100);
                assert_eq!(actual.error, None, "{field}: {:?}", actual.error);
                assert_eq!(actual.lines, expected.lines, "{field}");
                assert_eq!(cause(&actual.error), cause(&expected.error), "{field}");
            }
            (Err(actual), Err(expected)) => {
                let actual = actual.to_string();
                assert!(!actual.contains("internal error"), "{field}: {actual}");
                assert_eq!(actual, expected.to_string(), "{field}");
            }
            (actual, expected) => panic!("{field}: {actual:?} vs {expected:?}"),
        }
    }
}
