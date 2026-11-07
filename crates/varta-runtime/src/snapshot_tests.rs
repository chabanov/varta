use super::*;
use varta_common::language::LanguageProfile;

fn run(source: &str) -> Vec<String> {
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .unwrap()
        .lines
}

#[test]
fn snapshot_checks_declared_children_and_isolates_mutable_input() {
    let output = run(r#"
interface Answer { n: number; child: { x: number }; list: { id: number }[]; note?: string }
export function main(): void {
 const input: unknown = JSON.parse('{"before":true,"n":-0,"child":{"x":1,"more":2},"list":[{"id":3,"extra":"kept"}],"after":4}');
 const answer = input as Answer;
 console.log(Object.is(answer.n, -0), answer.child.x, answer.list[0].id);
 console.log((answer as unknown) === input);
 const alias = answer;
 answer.child.x = 5;
 console.log(alias.child.x);
 const original = input as Record<string, unknown>;
 const child = original["child"] as Record<string, unknown>;
 child["x"] = 99;
 console.log(answer.child.x);
 const second = input as Answer;
 console.log(second.child.x, (second as unknown) === (answer as unknown));
 const boxed: unknown = answer;
 console.log((boxed as Answer) === answer);
}
"#);
    assert_eq!(
        output,
        ["true 1 3", "false", "5", "5", "99 false", "true"]
            .map(|line| format!("varta log: {line}"))
    );
}

#[test]
fn snapshot_retains_order_and_nested_extras_through_json_and_enumeration() {
    let output = run(r#"
interface Item { id: number }
interface Answer { value: number; child: Item; list: Item[]; note?: string }
export function main(): void {
 const input: unknown = JSON.parse('{"before":true,"value":1,"child":{"extra":2,"id":3},"list":[{"id":4,"tail":5}],"after":"kept"}');
 const answer = input as Answer;
 console.log(JSON.stringify(answer));
 console.log(JSON.stringify(Object.keys(answer)));
 console.log(JSON.stringify(Object.values(answer)));
 console.log(JSON.stringify(Object.entries(answer)));
 const dictionary = (answer as unknown) as Record<string, unknown>;
 console.log(JSON.stringify(dictionary));
 dictionary["value"] = "incompatible";
 console.log(answer.value);
 answer.note = "added";
 console.log(JSON.stringify(Object.keys(answer)));
 console.log(JSON.stringify(answer));
 console.log(answer);
}
"#);
    let original = r#"{"before":true,"value":1,"child":{"extra":2,"id":3},"list":[{"id":4,"tail":5}],"after":"kept"}"#;
    let expected = [
        original,
        r#"["before","value","child","list","after"]"#,
        r#"[true,1,{"extra":2,"id":3},[{"id":4,"tail":5}],"kept"]"#,
        r#"[["before",true],["value",1],["child",{"extra":2,"id":3}],["list",[{"id":4,"tail":5}]],["after","kept"]]"#,
        original,
        "1",
        r#"["before","value","child","list","after","note"]"#,
        r#"{"before":true,"value":1,"child":{"extra":2,"id":3},"list":[{"id":4,"tail":5}],"after":"kept","note":"added"}"#,
    ];
    assert_eq!(
        &output[..expected.len()],
        expected.map(|line| format!("varta log: {line}"))
    );
    assert!(output[expected.len()..].join("\n").contains("before: true"));
    assert!(!output.join("\n").contains("#snapshot"));
}

#[test]
fn snapshot_keeps_strict_boundaries_and_rejects_invalid_declared_data() {
    let output = run(r#"
interface Counter { kind: 'runs' | 'tokens'; used: number; limit: number | null }
interface Answer { allowed: boolean; counters: Counter[]; note?: string }
function accepts(text: string): boolean {
 try { const raw: unknown = JSON.parse(text); const answer = raw as Answer; return answer.allowed; }
 catch (error) { if (error instanceof TypeError) return false; throw error; }
}
export function main(): void {
 console.log(accepts('{"allowed":true,"counters":[{"kind":"runs","used":1,"limit":null,"extra":true}],"extra":2}'));
 console.log(accepts('{"counters":[]}'));
 console.log(accepts('{"allowed":"true","counters":[]}'));
 console.log(accepts('{"allowed":true,"counters":[{"kind":"other","used":1,"limit":null}]}'));
 console.log(accepts('{"allowed":true,"counters":[{"kind":"runs","used":"1","limit":null}]}'));
 console.log(accepts('{"allowed":true,"counters":[],"note":null}'));
 try { JSON.parse('{"allowed":true,"counters":[],"extra":2}') as Answer; console.log("wrong direct"); }
 catch (error) { if (error instanceof TypeError) console.log("strict direct"); }
 try { varta.json.parse<{ value: number }>('{"value":1,"extra":2}'); console.log("wrong parser"); }
 catch (error) { if (error instanceof TypeError) console.log("strict parser"); }
 const raw: unknown = JSON.parse('{"value":1,"extra":2}');
 try { raw as { value: number } | { label: string }; console.log("wrong union"); }
 catch (error) { if (error instanceof TypeError) console.log("strict union"); }
 const cycle: Record<string, unknown> = {};
 cycle["self"] = cycle;
 try { (cycle as unknown) as { self: unknown }; console.log("wrong cycle"); }
 catch (error) { if (error instanceof TypeError) console.log("cycle refused"); }
 const nonjson: Record<string, unknown> = { value: 1, extra: undefined };
 try { (nonjson as unknown) as { value: number }; console.log("wrong extra"); }
 catch (error) { if (error instanceof TypeError) console.log("nonjson refused"); }
}
"#);
    assert_eq!(
        output,
        [
            "true",
            "false",
            "false",
            "false",
            "false",
            "false",
            "strict direct",
            "strict parser",
            "strict union",
            "cycle refused",
            "nonjson refused"
        ]
        .map(|line| format!("varta log: {line}"))
    );
}

#[test]
fn snapshot_optional_updates_delete_and_readd_preserve_key_order() {
    let output = run(r#"
interface Answer { value: number; note?: string; '1'?: number }
export function main(): void {
 const raw: unknown = JSON.parse('{"before":true,"note":"old","value":1,"after":2}');
 const answer = raw as Answer;
 console.log(delete answer.note);
 console.log(JSON.stringify(answer));
 answer.note = "new";
 answer['1'] = 3;
 console.log(JSON.stringify(Object.keys(answer)));
 console.log(JSON.stringify(answer));
 Object.freeze(answer);
 try { answer.note = "blocked"; } catch (error) { if (error instanceof TypeError) console.log("frozen"); }
 console.log(JSON.stringify(Object.keys(answer)));
}
"#);
    assert_eq!(
        output,
        [
            "true",
            r#"{"before":true,"value":1,"after":2}"#,
            r#"["1","before","value","after","note"]"#,
            r#"{"1":3,"before":true,"value":1,"after":2,"note":"new"}"#,
            "frozen",
            r#"["1","before","value","after","note"]"#,
        ]
        .map(|line| format!("varta log: {line}"))
    );
}

#[test]
fn snapshot_spreads_and_presence_keep_retained_properties() {
    let output = run(r#"
interface A { n: number; child: { x: number }; note?: string }
export function main(): void {
 const raw: unknown = JSON.parse('{"extra":true,"n":1,"child":{"x":2,"tail":3}}');
 const a = raw as A;
 console.log('extra' in a, Object.hasOwn(a, 'extra'), Object.hasOwn(a, 'absent'));
 const key = 'extra';
 console.log(Object.hasOwn(a, key));
 const names: string[] = [];
 for (const name in a) names.push(name);
 console.log(JSON.stringify(names));
 const b = { before: 0, ...a, n: 4, after: 5 };
 console.log(b.n, JSON.stringify(b));
 const c: A = { ...a, n: 6 };
 console.log(c.n, c.child === a.child, JSON.stringify(c));
 const d: Record<string, unknown> = { ...a };
 console.log(JSON.stringify(d));
 console.log(JSON.stringify(Object.assign({}, a)));
}
"#);
    assert_eq!(
        output,
        [
            "true true false",
            "true",
            r#"["extra","n","child"]"#,
            r#"4 {"before":0,"extra":true,"n":4,"child":{"x":2,"tail":3},"after":5}"#,
            r#"6 true {"extra":true,"n":6,"child":{"x":2,"tail":3}}"#,
            r#"{"extra":true,"n":1,"child":{"x":2,"tail":3}}"#,
            r#"{"extra":true,"n":1,"child":{"x":2,"tail":3}}"#,
        ]
        .map(|line| format!("varta log: {line}"))
    );
}

#[test]
fn snapshot_plain_data_checks_do_not_execute_getters_or_fill_holes() {
    let output = run(r#"
export function main(): void {
 let effects = 0;
 const accessor = { get value(): number { effects++; return 1; } };
 try { (accessor as unknown) as { value: number }; console.log('wrong getter'); }
 catch (error) { if (error instanceof TypeError) console.log('getter refused ' + String(effects)); }
 const sparse: unknown[] = [];
 sparse.length = 2;
 try { const raw: unknown = { list: sparse }; raw as { list: unknown[]; note?: string }; console.log('wrong sparse'); }
 catch (error) { if (error instanceof TypeError) console.log('sparse refused'); }
 const array: unknown = [1, 2];
 if (Array.isArray(array)) console.log(JSON.stringify(array));
 const numbers: Record<string, number> = { value: 1, extra: 2 };
 console.log(JSON.stringify(numbers));
 console.log(JSON.stringify((numbers as unknown) as { value: number }));
 const optional: Record<string, unknown> = { value: 1, note: undefined };
 const result = (optional as unknown) as { value: number; note?: string };
 console.log(String(Object.hasOwn(result, 'note')) + ' ' + String(result.note === undefined));
}
"#);
    assert_eq!(
        output,
        [
            "getter refused 0",
            "sparse refused",
            "[1,2]",
            r#"{"value":1,"extra":2}"#,
            r#"{"value":1,"extra":2}"#,
            "true true"
        ]
        .map(|line| format!("varta log: {line}"))
    );
}

#[test]
fn snapshot_previous_profile_and_direct_parse_remain_exact() {
    let source = r#"export function main(): void {
 const raw: unknown = JSON.parse('{"n":1,"extra":true}');
 try { raw as { n: number }; console.log('wrong'); }
 catch (error) { if (error instanceof TypeError) console.log('exact'); }
}"#;
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        ["varta log: exact"]
    );
}

#[test]
fn snapshot_retained_metadata_and_extras_survive_suspended_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let source = r#"
interface A { child: { n: number }; list: { id: number }[] }
async function retained(): Promise<void> {
 const raw: unknown = JSON.parse('{"before":{"deep":[1,2]},"child":{"n":3,"extra":"kept"},"list":[{"id":4,"tail":5}]}');
 const a = raw as A;
 const alias = a;
 for (let i = 0; i < 12; i++) { await Promise.resolve(i); a.child.n++; }
 console.log(alias.child.n, JSON.stringify(a));
}
export async function main(): Promise<void> { await retained(); }
"#;
    let bytes = hook::compile_with_profile(source, LanguageProfile::CheckedF64Utf16V39).unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    instance
        .get_func(&mut *store, "_start")
        .unwrap()
        .call(&mut *store, &[], &mut [])
        .unwrap();
    let collect = instance
        .get_typed_func::<(), ()>(&mut *store, hook::COLLECT)
        .unwrap();
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(
            &mut *store,
            varta_common::function::frame::RESUME_EXPORT,
        )
        .unwrap();
    let mut jobs = 0;
    while let Some(token) = store.data_mut().scheduler.next().unwrap() {
        let remaining = store.get_fuel().unwrap();
        store.set_fuel(abi::FUEL).unwrap();
        collect.call(&mut *store, ()).unwrap();
        store.set_fuel(remaining).unwrap();
        resume.call(&mut *store, (token, 0, 0)).unwrap();
        jobs += 1;
    }
    assert!(jobs >= 12);
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    assert_eq!(
        store.data().output.lines,
        [
            r#"varta log: 15 {"before":{"deep":[1,2]},"child":{"n":15,"extra":"kept"},"list":[{"id":4,"tail":5}]}"#
        ]
    );
}

#[test]
fn snapshot_corrupt_metadata_is_rejected_by_native_and_shared_readers() {
    use varta_common::value::{self, HASHED_DICTIONARY_VALUE_VERSION as V18};
    let source = r#"const raw: unknown = JSON.parse('{"extra":true,"n":1}');
 const saved = raw as { n: number };
 export function main(): void { console.log(JSON.stringify(saved)); }"#;
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let bounds = module.bounds();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    instance
        .get_func(&mut *store, "_start")
        .unwrap()
        .call(&mut *store, &[], &mut [])
        .unwrap();
    let data = instance
        .get_memory(&mut *store, "memory")
        .unwrap()
        .data(&*store)
        .to_vec();
    let word = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
    let count = 3;
    let length = value::record_bytes(V18, count) as u64;
    let (handle, fields) = (abi::OWNED_STRING_BASE..data.len() - 8)
        .step_by(value::HEAP_ALIGNMENT)
        .filter(|ptr| {
            word(ptr + 4) == count
                && word(*ptr)
                    .checked_add(8)
                    .is_some_and(|end| end <= data.len())
        })
        .find_map(|ptr| {
            let handle = ptr as u64 | length << i32::BITS;
            let mut fields = Vec::new();
            value::read_record_in(
                &data,
                handle,
                V18,
                &bounds,
                |field| fields.push(field),
                |_| {},
            )
            .ok()?;
            fields.sort_by_key(|field| field.slot);
            value::snapshot_members(&data, &fields, V18, bounds)
                .ok()
                .flatten()?;
            Some((handle, fields))
        })
        .expect("live snapshot");
    let render = |data: &[u8]| {
        crate::json::render_reference_in(data, value::ValueKind::Record as u32, handle, V18, bounds)
    };
    assert_eq!(render(&data).unwrap(), br#"{"extra":true,"n":1}"#);
    assert!(
        value::snapshot_members(
            &data,
            &fields,
            V18,
            value::ModuleBounds {
                snapshots: false,
                ..bounds
            }
        )
        .unwrap()
        .is_none()
    );
    let order = fields
        .iter()
        .find(|field| {
            &data[field.key..field.key + field.len] == value::SNAPSHOT_ORDER_KEY.as_bytes()
        })
        .unwrap();
    let vector = value::Vector::read_in(&data, order.payload, V18, bounds).unwrap();
    let first = vector.storage + value::ARRAY_META_BYTES;
    let second = first + value::ARRAY_SLOT_BYTES;
    let mut duplicate = data.clone();
    duplicate.copy_within(first..first + value::ARRAY_SLOT_BYTES, second);
    assert!(render(&duplicate).is_err());
    let mut budget = varta_bigint::Budget::new(u64::from(u32::MAX), u64::MAX);
    assert!(
        crate::json::render_diagnostic(
            &duplicate,
            value::ValueKind::Record as u32,
            handle,
            V18,
            bounds,
            &mut budget
        )
        .is_err()
    );
    let mut invalid_kind = fields.clone();
    invalid_kind[order.slot].kind = value::ValueKind::ReferenceUnion;
    assert!(value::snapshot_members(&data, &invalid_kind, V18, bounds).is_err());
    let mut invalid_slot = fields.clone();
    invalid_slot[order.slot].slot = fields.len();
    assert!(value::snapshot_members(&data, &invalid_slot, V18, bounds).is_err());
}

#[test]
fn compiler_owned_snapshot_object_boxes_dictionary_elements() {
    struct NoImports;
    impl varta_compiler::ModuleLoader for NoImports {
        fn resolve(
            &self,
            _: &std::path::Path,
            _: &str,
        ) -> Result<std::path::PathBuf, varta_compiler::CompilerError> {
            unreachable!()
        }
        fn load(&self, _: &std::path::Path) -> Result<String, varta_compiler::CompilerError> {
            unreachable!()
        }
    }
    let source = include_str!("../../varta-compiler/src/prelude/snapshot.ts").to_owned()
        + r#"
 export function main(): void {
 const numbers: Record<string, number> = { value: 1, extra: 2 };
 const data = snapshotObject(numbers as unknown);
 console.log(JSON.stringify(data));
 console.log(JSON.stringify(snapshotClone(numbers as unknown, 64, false)));
 }"#;
    let bytes = varta_compiler::compile_modules_with_profile(
        varta_compiler::ModuleSource {
            path: "<varta>/builtins/snapshot.ts".into(),
            text: source,
        },
        &NoImports,
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap()
    .wasm;
    let output = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .unwrap()
        .lines;
    assert_eq!(
        output,
        [
            r#"varta log: {"value":1,"extra":2}"#,
            r#"varta log: {"value":1,"extra":2}"#
        ]
    );
}

#[test]
fn snapshot_profile_mismatch_is_rejected_before_effects() {
    let mut bytes = varta_compiler::compile_typescript_with_profile(
        "export function main(): void { console.log('BAD_EFFECT'); }",
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap();
    let marker = b"checked-f64-utf16-v39";
    let at = bytes
        .windows(marker.len())
        .position(|part| part == marker)
        .unwrap();
    bytes[at + marker.len() - 1] = b'8';
    let report = VartaRuntime::new().run_with(&bytes, RunOptions::default());
    assert!(report.failure.is_some());
    assert!(report.output.lines.is_empty());
}

#[test]
fn snapshot_fuel_exhaustion_bypasses_guest_exception_handlers() {
    let json = serde_json::json!({ "list": (0..100).collect::<Vec<_>>() }).to_string();
    let source = format!(
        r#"export function main(): void {{
 const raw: unknown = JSON.parse('{}');
 console.log('copy');
 try {{ raw as {{ list: number[]; note?: string }}; console.log('done'); }}
 catch (error) {{ console.log('BAD_CATCH'); }}
 finally {{ console.log('BAD_FINALLY'); }}
}}"#,
        json
    );
    let bytes = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    execution.store.set_fuel(abi::FUEL / 10).unwrap();
    let result = instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ());
    assert!(result.is_err());
    assert_eq!(execution.store.data().output.lines, ["varta log: copy"]);
}

#[test]
fn snapshot_dictionary_holes_utf16_keys_and_later_getters_remain_observable() {
    let output = run(r#"
interface Child { x: number; next?: Child }
interface A { n: number; child: Child; self?: A }
export function main(): void {
 const dictionary: Record<string, number> = {};
 for (let i = 0; i < 40; i++) dictionary['k' + String(i)] = i;
 dictionary['n'] = 7;
 delete dictionary['k3'];
 const fromDictionary = (dictionary as unknown) as { n: number };
 const retained = (fromDictionary as unknown) as Record<string, unknown>;
 console.log(String(retained['k4']) + ' ' + String(Object.hasOwn(retained, 'k3')));
 const raw: unknown = JSON.parse('{"\\ud800":2,"9":3,"n":1,"child":{"x":4,"tail":5}}');
 const a = raw as A;
 console.log(JSON.stringify(Object.keys(a)));
 console.log(JSON.stringify(a));
 let effects = 0;
 a.child = { get x(): number { effects++; return 9; } };
 console.log(JSON.stringify(a));
 console.log('effects ' + String(effects));
 a.self = a;
 try { JSON.stringify(a); console.log('wrong cycle'); }
 catch (error) { if (error instanceof TypeError) console.log('cycle'); }
}
"#);
    assert_eq!(
        output,
        [
            "4 false",
            r#"["9","\ud800","n","child"]"#,
            r#"{"9":3,"\ud800":2,"n":1,"child":{"x":4,"tail":5}}"#,
            r#"{"9":3,"\ud800":2,"n":1,"child":{"x":9}}"#,
            "effects 1",
            "cycle",
        ]
        .map(|line| format!("varta log: {line}"))
    );
}

#[test]
fn snapshot_property_guards_preserve_optional_nullable_data_narrowing() {
    let source = r#"
interface User { user_id: string }
interface Envelope { user?: User | null; total?: number | null }
function accepted(text: string, reverse: boolean): boolean {
 const raw: unknown = JSON.parse(text);
 const me = raw as Envelope;
 if (reverse) return me.user !== null && me.user !== undefined && me.user.user_id === 'fixture';
 return me.user !== undefined && me.user !== null && me.user.user_id === 'fixture';
}
function total(text: string): number {
 const raw: unknown = JSON.parse(text);
 const me = raw as Envelope;
 if (me.total !== null && me.total !== undefined) return me.total + 1;
 return 0;
}
export function main(): void {
 console.log(accepted('{}', false), accepted('{}', true));
 console.log(accepted('{"user":null}', false), accepted('{"user":null}', true));
 console.log(accepted('{"user":{"user_id":"fixture"}}', false), accepted('{"user":{"user_id":"fixture"}}', true));
 console.log(total('{}'), total('{"total":null}'), total('{"total":4}'));
}
"#;
    let expected = ["false false", "false false", "true true", "0 0 5"]
        .map(|line| format!("varta log: {line}"));
    assert_eq!(run(source), expected);
    let old = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap();
    assert_eq!(
        VartaRuntime::new().run_wasm_bytes(&old, &[]).unwrap().lines,
        expected
    );
    assert_eq!(
        run(r#"
interface User { user_id: string }
interface Envelope { user?: User | null }
function accepted(me: Envelope): boolean {
 return me.user !== undefined && me.user !== null && me.user.user_id === 'fixture';
}
export function main(): void {
 const raw: unknown = JSON.parse('{"user":{"user_id":"fixture"}}');
 console.log(accepted(raw as Envelope));
 const wider = { user: { user_id: 'fixture' }, extra: true };
 const me: Envelope = wider;
 console.log(accepted(me));
}
"#),
        ["varta log: true", "varta log: true"]
    );
}

#[test]
fn snapshot_data_guards_do_not_relax_unstable_getter_views() {
    let source = r#"
interface User { user_id: string }
interface Envelope { user?: User | null }
function accepted(me: Envelope): boolean {
 return me.user !== undefined && me.user !== null && me.user.user_id === 'fixture';
}
export function main(): void {
 const raw: unknown = JSON.parse('{"user":{"user_id":"fixture"}}');
 const stable = raw as Envelope;
 const unstable: Envelope = { get user(): User | null | undefined { return null; } };
 console.log(accepted(stable), accepted(unstable));
}
"#;
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("not a member of the expected union"),
        "{error}"
    );
}

#[test]
fn snapshot_data_keys_do_not_invoke_prototype_assignment() {
    let output = run(r#"
interface A { n: number; child: { x: number } }
export function main(): void {
 const raw: unknown = JSON.parse('{"n":1,"__proto__":{"kept":true},"child":{"x":2,"__proto__":[3]},"constructor":"data"}');
 const a = raw as A;
 console.log(JSON.stringify(a));
 console.log(JSON.stringify(Object.keys(a)));
 console.log(JSON.stringify((a as unknown) as Record<string, unknown>));
 console.log(JSON.stringify(a as Record<string, unknown>));
 const copy = { ...a };
 console.log(JSON.stringify(copy));
 console.log(Object.hasOwn(copy, '__proto__'));
 try { Object.assign({}, a); console.log('wrong assign'); }
 catch (error) { if (error instanceof TypeError) console.log('assign setter refused'); }
 const d: Record<string, unknown> = {};
 const key = '__proto__';
 try { d[key] = 1; console.log('wrong set'); }
 catch (error) { if (error instanceof TypeError) console.log('ordinary setter refused'); }
}
"#);
    let json =
        r#"{"n":1,"__proto__":{"kept":true},"child":{"x":2,"__proto__":[3]},"constructor":"data"}"#;
    assert_eq!(
        output,
        [
            json,
            r#"["n","__proto__","child","constructor"]"#,
            json,
            json,
            json,
            "true",
            "assign setter refused",
            "ordinary setter refused"
        ]
        .map(|line| format!("varta log: {line}"))
    );
}
