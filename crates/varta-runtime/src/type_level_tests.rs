//! V38 `type_level` (K9 A): conditional, mapped and template literal types,
//! the utility types, type queries, intersections, call/construct
//! signatures, tuples, type predicates, assertion functions, `satisfies`,
//! `never` exhaustiveness and polymorphic `this`. The expected logs of the
//! `type_level_cases` programs were produced by Node 24.20.0 (type
//! stripping, `main()` appended) and are pinned here; every program is
//! rejected by v37.
use super::*;
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn rejected(source: &str, profile: P) -> String {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} accepted\n{source}"))
        .to_string()
}
fn run(source: &str) -> Vec<String> {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source, V38), &[])
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
        .lines
}
fn logs(lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .map(|line| format!("varta log: {line}"))
        .collect()
}
fn case(source: &str, expected: &[&str]) {
    assert_eq!(run(source), logs(expected), "{source}");
    // Every case uses a construct v37 does not admit.
    rejected(source, V37);
}

#[test]
fn conditional_types_distribute_infer_and_recurse() {
    case(
        include_str!("type_level_cases/conditional.ts"),
        &[
            "yes yes no yes/no",
            "5 true",
            "3 plain settled",
            "1 false two true 2",
            "3,2,1 3",
            "43 true arg",
            "x none other string",
            "7 no id",
        ],
    );
}

#[test]
fn mapped_types_keep_and_change_modifiers_and_remap_keys() {
    case(
        include_str!("type_level_cases/mapped.ts"),
        &[
            "1 Ada true",
            "null x undefined",
            "3",
            "4:C:c@example.org",
            "undefined undefined",
            "name",
            "3 true false",
            "3",
            "null p 1||3",
        ],
    );
}

#[test]
fn template_literal_and_intrinsic_string_types() {
    case(
        include_str!("type_level_cases/templates.ts"),
        &[
            "onClick,onKey onKey",
            "btn-sm-1 btn-lg-2",
            "YOU hello name getSize",
            "x a+b+c 3 users root truefalse",
        ],
    );
}

#[test]
fn every_utility_type_resolves() {
    case(
        include_str!("type_level_cases/utilities.ts"),
        &[
            "undefined true n f",
            "p undefined title",
            "3 true",
            "bc a ac",
            "10 18 3 1",
            "12",
            "hi t plain",
            "loaded 19",
            "ABC abc Word word",
        ],
    );
}

#[test]
fn keyof_indexed_access_type_queries_and_intersections() {
    case(
        include_str!("type_level_cases/queries.ts"),
        &[
            "p 5",
            "800x600 copy main",
            "#f00 #0f0",
            "dark 42",
            "Ada 36",
            "a undefined",
            "age,name",
        ],
    );
}

#[test]
fn predicates_assertions_satisfies_and_exhaustiveness() {
    case(
        include_str!("type_level_cases/narrowing.ts"),
        &[
            "A10B",
            "fish 1",
            "bird 2",
            "4",
            "CHECKED",
            "4",
            "TypeError",
            "svc 3 circle,square",
            "0 4 3",
            "snb",
        ],
    );
}

#[test]
fn call_and_construct_signatures_and_polymorphic_this() {
    case(
        include_str!("type_level_cases/signatures.ts"),
        &[
            "2 2.0 2.000",
            "rex: woof",
            "built",
            "b: woof",
            "typed: woof",
            "function 7",
        ],
    );
}

#[test]
fn const_literals_keep_their_literal_types_and_mutable_locations_widen() {
    case(
        include_str!("type_level_cases/literals.ts"),
        &["dark sm lg 3 true sm a p sm,md,xl", "light"],
    );
    // A widened `let` admits other strings; a `const` keeps its literal.
    let narrowed = rejected(
        r#"export function main(): void {
  const size = "sm";
  const other: "lg" = size;
  console.log(other);
}"#,
        V38,
    );
    assert!(
        narrowed.contains("not assignable to literal type 'lg'"),
        "{narrowed}"
    );
    // V37 keeps `string` for the same declaration.
    let v37 = r#"export function main(): void {
  const size = "sm";
  let copy = size;
  copy = "lg";
  console.log(copy);
}"#;
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(v37, V37), &[])
            .unwrap()
            .lines,
        logs(&["lg"])
    );
}

#[test]
fn language_guide_type_level_example() {
    case(
        include_str!("type_level_cases/guide.ts"),
        &["V", "x 3,2,1 800 Ada light 4", "<b></b>"],
    );
}

#[test]
fn named_optional_rest_and_variadic_tuples() {
    case(
        include_str!("type_level_cases/tuples.ts"),
        &["Ada Bob(42) 1 2", "10 5 1 4", "1 true z 3", "p:2 0"],
    );
}

#[test]
fn language_guide_second_type_level_example() {
    case(
        include_str!("type_level_cases/guide_more.ts"),
        &["u1 4 5 4 3"],
    );
}

#[test]
fn class_subtyping_in_containers() {
    case(
        include_str!("type_level_cases/containers.ts"),
        &["BA BB BB", "B 1 C", "BBB", "B BC BC B"],
    );
}

#[test]
fn branded_primitives_local_type_queries_and_class_keys() {
    case(
        include_str!("type_level_cases/brands_and_queries.ts"),
        &[
            "U1 6 2",
            "total 7 4",
            "describe doubled 9 undefined length 1 custom ada:5",
        ],
    );
}

#[test]
fn subclass_unknown_and_this_predicates_narrow_with_checked_reads() {
    case(
        include_str!("type_level_cases/narrowing_more.ts"),
        &["rex woof", "rex woof", "3", "TypeError"],
    );
}

#[test]
fn infer_through_declarations_unions_and_optional_and_trailing_elements() {
    case(
        include_str!("type_level_cases/inference_more.ts"),
        &["boxed 4 5 2 3 7", "1:1", "3:2"],
    );
}

#[test]
fn overloads_generic_signatures_object_this_and_pattern_templates() {
    case(
        include_str!("type_level_cases/signatures_more.ts"),
        &["a 2", "2 s 2", "root 1", "onClickX"],
    );
}

#[test]
fn intersection_infer_generic_this_asserts_this_and_rest_overloads() {
    case(
        include_str!("type_level_cases/admitted_forms.ts"),
        &["1 x q", "4 3 2", "9", "not a square", "a-b-c 3  6 4"],
    );
}

#[test]
fn pattern_template_types_check_their_literals() {
    case(
        include_str!("type_level_cases/patterns.ts"),
        &[
            "Click Key on3 12.5 id-42 HELLO",
            "Hello world ONCLICK none,#fff 7",
            "true yes no yes yes",
            "1 2 #000 color #abc empty",
            r#"1,2.5,1000 {"title":"t"}"#,
        ],
    );
    // Each is an error in tsc 5.x: the literal (or `string`, or another
    // pattern) is not assignable to the pattern.
    for (declaration, wanted) in [
        ("const h: `on${string}` = \"click\";", "`on${string}`"),
        ("const h: `on${string}` = String(1);", "`on${string}`"),
        ("const p: `${number}px` = \"abcpx\";", "`${number}px`"),
        ("const i: `id-${bigint}` = \"id-1.5\";", "`id-${bigint}`"),
        (
            "const l: Uppercase<string> = \"Hello\";",
            "Uppercase<string>",
        ),
        (
            "const t: Capitalize<string> = \"hello\";",
            "Capitalize<string>",
        ),
        (
            "const e: Uppercase<`on${string}`> = \"ONclick\";",
            "`ON${Uppercase<string>}`",
        ),
        (
            "const s = String(1); const h: `on${string}` = `x${s}`;",
            "`on${string}`",
        ),
        (
            "const a: `on${string}` = \"onA\"; const n: `on${number}` = a;",
            "`on${number}`",
        ),
    ] {
        let source = format!("export function main(): void {{\n  {declaration}\n}}");
        let error = rejected(&source, V38);
        assert!(
            error.contains(&format!("is not assignable to literal type {wanted}")),
            "{source}\n{error}"
        );
    }
}

#[test]
fn class_arrays_viewed_as_ancestor_arrays_are_one_array() {
    case(
        include_str!("type_level_cases/shared_arrays.ts"),
        &[
            "2 2 BB 2 true",
            "BCBC BCBC 4",
            "BBCC BB",
            "C C 3",
            "true true 1 true",
            r#"CBCB CBCBC [{"x":1,"z":3},{"x":1,"y":2},{"x":1,"z":3},{"x":1,"y":2}]"#,
            "BBCC 2,2,3,3",
            "4,3,4,- 4 true",
            "4",
            "3",
            "4",
        ],
    );
}

#[test]
fn checked_assertions_test_pattern_template_types() {
    case(
        include_str!("type_level_cases/pattern_checks.ts"),
        &[
            "onClick on onon",
            "12px 1e3px  px 0x1fpx -.5px",
            "id-42 HELLO WORLD Click:3",
            "id-0x1F id-0 id--5 id-0b101",
            "Click:1 Ab:3 \u{c9}: 2 1:1",
            "auto #fff #",
            "onKey 1px,2.5px -",
            "onA  id-7",
        ],
    );
    // A string that does not match the pattern fails the checked assertion
    // with `TypeError`, like every checked `as` (Node continues with the
    // value, so each line would print the values themselves).
    let mismatches = r#"type Handler = `on${string}`;
type Size = `${number}px`;
type Id = `id-${bigint}`;
type Event = `${Capitalize<string>}:${number}`;
interface Config { handler: Handler; sizes: Size[] }
function attempt(read: () => string): string {
  try {
    return read();
  } catch (error) {
    return error instanceof TypeError ? "TypeError" : "other";
  }
}
export function main(): void {
  const values: unknown[] = ["click", "", "On", 3];
  console.log(values.map((value) => attempt(() => value as Handler)).join(" "));
  const sizes: unknown[] = ["px", "abcpx", "Infinitypx", "1pxpx"];
  console.log(sizes.map((value) => attempt(() => value as Size)).join(" "));
  const ids: unknown[] = ["id-042", "id-", "id-1.5", "id-0x"];
  console.log(ids.map((value) => attempt(() => value as Id)).join(" "));
  const events: unknown[] = ["click:1", "A:B:3", "a:2", "A:"];
  console.log(events.map((value) => attempt(() => value as Event)).join(" "));
  console.log(attempt(() => (JSON.parse('{"handler":"key","sizes":[]}') as Config).handler));
  console.log(attempt(() => (JSON.parse('{"handler":"onA","sizes":["2em"]}') as Config).handler));
}"#;
    let refused = "TypeError TypeError TypeError TypeError";
    assert_eq!(
        run(mismatches),
        logs(&[refused, refused, refused, refused, "TypeError", "TypeError"])
    );
}

#[test]
fn pattern_index_signatures_admit_only_their_keys() {
    case(
        include_str!("type_level_cases/pattern_keys.ts"),
        &[
            "1 onClick,onFocus,onBlur 8 true",
            "onA=5&a=1",
            r#"1 none {"data-x":"1","data-y":"2"}"#,
            "main Main 3",
            "aria-live",
            "saved doc",
        ],
    );
    // Each is an error in tsc 5.x (TS2353, TS2339, TS7053, TS2741).
    let prelude = "type H = Record<`on${string}`, number>;\n\
                   type P = Record<\"id\" | `aria-${string}`, string>;\n";
    for (statement, wanted) in [
        (
            "const h: H = { foo: 1 };",
            "object literal may only specify known properties, and foo does not exist in type H",
        ),
        (
            "const h: H = { onA: 1 }; console.log(h.foo);",
            "property foo does not exist on type H",
        ),
        (
            "const h: H = { onA: 1 }; h.zz = 3;",
            "property zz does not exist on type H",
        ),
        (
            "const h: H = { onA: 1 }; const k = String(1); console.log(h[k]);",
            "an expression of type string cannot index type H",
        ),
        (
            "const p: P = { \"aria-x\": \"1\" }; console.log(p.id);",
            "missing the required declared property id",
        ),
        (
            "const h: H = { onA: 1 }; delete h.off;",
            "property off does not exist on type H",
        ),
    ] {
        let source = format!("{prelude}export function main(): void {{\n  {statement}\n}}");
        let error = rejected(&source, V38);
        assert!(error.contains(wanted), "{source}\n{error}");
    }
}

#[test]
fn overloads_with_type_and_this_parameters_resolve_per_call() {
    case(
        include_str!("type_level_cases/generic_overloads.ts"),
        &["[a] 42 true", "3 #2 x", "> hits=2 @ada", "3 4px -1"],
    );
    // Each is an error in tsc 5.x (TS2769 no overload matches, TS2684 the
    // `this` context).
    let prelude = "interface Pick { <T>(value: T, fallback: T): T; (count: number): string; }\n\
                   interface Counter { count: number }\n\
                   interface Counts { (this: Counter): number; (this: Counter, by: number): number; }\n";
    for statement in [
        "const pick = ((v: unknown, f?: unknown) => v) as Pick; console.log(pick(\"a\", 1));",
        "const pick = ((v: unknown, f?: unknown) => v) as Pick; console.log(pick(true));",
        "const count = function (this: Counter, by?: number): number { return this.count; } as Counts; \
         const other = { size: 1, count }; console.log(other.count(1));",
    ] {
        let source = format!("{prelude}export function main(): void {{\n  {statement}\n}}");
        let error = rejected(&source, V38);
        assert!(error.contains("no overload of"), "{source}\n{error}");
    }
}

#[test]
fn generic_construct_signatures_construct_every_used_instantiation() {
    case(
        include_str!("type_level_cases/generic_constructors.ts"),
        &[
            "Box(3) Box(ada) true 6",
            "Box(3) Box(ada) true 6",
            "one 1",
            "2 3",
            "3 x true false function",
        ],
    );
    // Each is an error in tsc 5.x (TS2345, TS2322).
    let prelude = "class Box<T> { value: T; constructor(value: T) { this.value = value; } }\n\
                   class NumberBox { value: number; constructor(value: number) { this.value = value; } }\n\
                   interface BoxConstructor { new <T>(value: T): Box<T>; }\n";
    for (statement, wanted) in [
        (
            "const make: BoxConstructor = Box; console.log(new make<number>(\"x\").value);",
            "mismatch",
        ),
        (
            "const make: BoxConstructor = NumberBox; console.log(new make(\"x\").value);",
            "mismatch",
        ),
    ] {
        let source = format!("{prelude}export function main(): void {{\n  {statement}\n}}");
        let error = rejected(&source, V38);
        assert!(error.contains(wanted), "{source}\n{error}");
    }
}

#[test]
fn collections_keyed_by_shared_class_arrays_hash_the_instances() {
    case(
        include_str!("type_level_cases/shared_keys.ts"),
        &["3", "2 3 false", "2 true", "1 3", "1 2", "1 2"],
    );
}

#[test]
fn callable_objects_hold_own_properties() {
    case(
        include_str!("type_level_cases/callable_objects.ts"),
        &[
            "5 none function",
            "count,label true 5",
            "Hi, Ada",
            r#"count {"n":1} [null]"#,
            "2 ms calls,unit",
            "6px 2 suffix,scale",
            "8px 10px true",
            "true true calls,unit,extra",
            "suffix",
            "scale",
        ],
    );
    // Each is an error in tsc 5.x (TS2741, TS2339).
    let prelude = "interface Counter { (step: number): number; count: number }\n";
    for (statement, wanted) in [
        ("const c: Counter = (step: number) => step;", "mismatch"),
        (
            "const c = ((s: number) => s) as Counter; console.log(c.missing);",
            "property missing does not exist",
        ),
        (
            "const c = ((s: number) => s) as Counter; c.other = 1;",
            "property other does not exist",
        ),
    ] {
        let source = format!("{prelude}export function main(): void {{\n  {statement}\n}}");
        let error = rejected(&source, V38);
        assert!(error.contains(wanted), "{source}\n{error}");
    }
    // A function held by `unknown` asserted to another signature throws
    // `TypeError`, like every checked assertion (Node calls it: `xpx`).
    let mismatch = r#"export function main(): void {
  const held: unknown = (value: number) => `${value}px`;
  try {
    const wrong = held as (value: string) => string;
    console.log(wrong("x"));
  } catch (error) {
    console.log(error instanceof TypeError ? "TypeError" : "other");
  }
}"#;
    assert_eq!(run(mismatch), logs(&["TypeError"]));
}

/// Symbol-keyed own properties of a function share its own-property
/// storage; the string-key enumerations skip them (Node 24.20.0 output).
#[test]
fn callable_objects_hold_symbol_keyed_properties() {
    let source = r#"const TAG: unique symbol = Symbol("tag");
interface Tagged {
  (n: number): number;
  [TAG]: string;
  count: number;
}
export function main(): void {
  const f = ((n: number) => n + 1) as Tagged;
  f[TAG] = "t";
  f.count = 2;
  console.log(`${f[TAG]} ${f.count} ${f(1)}`);
  console.log(Object.keys(f).join(","));
  console.log(`${"count" in f} ${JSON.stringify([f])}`);
  for (const key in f) {
    console.log(key);
  }
}"#;
    case(source, &["t 2 2", "count", "true [null]", "count"]);
}

/// Intentional deviations from Node (recorded in the type-level contract):
/// a lying type predicate throws `TypeError` at the narrowed read, where
/// Node continues with the mistyped value; and an element of a shared class
/// array read through a name whose element type does not admit it (an `A`
/// stored through `A[]` and read through `B[]`) throws `TypeError`.
#[test]
fn checked_predicates_and_shared_array_reads_deviate_from_node() {
    let lying = r#"class Animal { name: string = "tom"; }
class Dog extends Animal { tag: string = "dog"; }
function lies(animal: Animal): animal is Dog { return animal.name.length > 0; }
export function main(): void {
  const tom: Animal = new Animal();
  try {
    if (lies(tom)) {
      console.log(String(tom.tag));
    }
  } catch (error) {
    console.log(error instanceof TypeError ? "TypeError" : "other");
  }
}"#;
    // Node prints `undefined`.
    assert_eq!(run(lying), logs(&["TypeError"]));
    let shared = r#"class A { n: number = 1; }
class B extends A { tag: string = "b"; }
class C extends A { }
function addA(list: A[]): void { list.push(new A()); }
function report(read: () => string): void {
  try {
    console.log(read());
  } catch (error) {
    console.log(error instanceof TypeError ? "TypeError" : "other");
  }
}
export function main(): void {
  const bs: B[] = [new B()];
  addA(bs);
  report(() => `${bs.length} ${bs[0].tag}`);
  report(() => String(bs[1].tag));
  const mixed = [new B(), new C()];
  addA(mixed);
  report(() => String(mixed[2] instanceof B));
  const loose: (A | undefined)[] = mixed;
  loose.push(undefined);
  report(() => String(mixed[3] instanceof C));
}"#;
    // Node prints `2 b`, `undefined`, `false`, `false`.
    assert_eq!(
        run(shared),
        logs(&["2 b", "TypeError", "TypeError", "TypeError"])
    );
}
