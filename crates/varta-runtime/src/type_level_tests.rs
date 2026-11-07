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
