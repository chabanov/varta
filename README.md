# Varta

Varta compiles a small, checked TypeScript subset to WebAssembly and runs it in a
capability-checked Wasmtime host. System access is denied unless the exact
operation and resource are declared. Applications can be distributed as a single
native executable containing portable Wasm and its runtime.

Status: MVP / active development. The [architecture contract](docs/architecture.md)
describes the implemented language, trust boundaries, ABI and limits. The
[development plan](docs/development-plan.md) tracks audit fixes and next milestones.

[Repository](https://github.com/chabanov/varta) · [Contributing](CONTRIBUTING.md) ·
[Security](SECURITY.md) · [Changelog](CHANGELOG.md) · [MIT License](LICENSE.md)

Developed by **Squad AI, Inc.** Architecture and technology by **Pavlo Chabanov**.

## Repository scope

This repository develops the Varta engine and its development tools. Application
entities, business rules and application-specific permissions belong to separate
TS/JS projects built on the engine's public APIs.

| Directory | Responsibility |
| --- | --- |
| `crates/varta-common` | Engine contracts, ABI, permissions, diagnostics and shared values |
| `crates/varta-compiler` | Checked TS subset and portable Wasm generation |
| `crates/varta-runtime` | Validation, inspection, bounded execution and JSON tool invocation |
| `crates/varta-artifact` | Portable Wasm/manifest and native container I/O, without VM or compiler |
| `crates/varta-launcher` | The execution-only `varta-runtime` binary distributed with applications |
| `crates/varta-project` | Development tooling: project configuration/source loading, packaging and policy files |
| `crates/varta-cli` | Commands and output, calling the project tooling layer |
| `docs`, `.github` | Implemented specifications, core roadmap and verification |
| `target` | Ignored, generated Rust build output |

Engine and distribution crates cannot depend on project tooling or CLI, including
through tests. Runtime distribution has no TS compiler, SWC or development command
parser in its production dependencies. Build these components independently:

```sh
cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked
cargo build -p varta-launcher --bin varta-runtime --locked
cargo test -p varta-runtime --test public_api --locked
```

The [public API test](crates/varta-runtime/tests/public_api.rs) compiles in-memory
TS modules, inspects the resulting bytecode and invokes a published tool using
only engine APIs. It needs no project directory, configuration or CLI process.
`VartaConfig` and `CapabilitySpec` are tooling types exported by `varta-project`;
their JSON format is unchanged. Backend-independent bytecode metadata is returned
by `VartaRuntime::inspect_module` as `WasmInspection`.

Reference applications remain deferred until the generic core mechanisms exist.
Embedded TS snippets in tests verify the engine; they are not application modules.

## Platform direction

Varta targets typed, contract-driven TS/JS applications for desktop, server and
robot environments. The intended architecture compiles supported source into
portable Wasm and exposes explicitly selected application operations to external
AI agents through MCP. UI, API and agent callers share the application's command
and query layer, business rules and state; connecting another agent does not
create another application state.

The target is a self-contained JS/TS runtime in the Deno/Bun category. The current
small language subset is an implementation stage, not the product's final scope.
Language semantics, memory management, scheduling, standard APIs, modules/packages
and distribution all belong to the core roadmap. Full compatibility is not yet
implemented; the current i32 numeric profile in particular is not standard JS
Number behavior. See the [runtime target](docs/development-plan.md#runtime-target-and-compatibility).

New modules explicitly declare their selected language profile (default `checked-i32-utf8-v1`).
`inspect` reports that profile and whether it was declared or inferred from an
older host ABI v1 artifact. Unknown profiles fail before execution. Generic Wasm
without language/ABI metadata reports an unspecified source language. An opt-in
binary64 profile with typed aggregates and JSON is implemented below. A further
opt-in profile adds distinct `undefined`; `checked-f64-utf16-v1` adds UTF-16 strings.
`checked-f64-utf16-v2` adds closed primitive unions and runtime type checks.
`checked-f64-utf16-v3` adds presence-aware optional fields and missing primitive reads.
`checked-f64-utf16-v4` extends unions to closed records/arrays and adds Array.isArray.
`checked-f64-utf16-v5` adds strict JSON parsing for optional fields and unions.
`checked-f64-utf16-v6` adds bounded resizable arrays with push/pop, at/slice and includes/indexOf.
`checked-f64-utf16-v7` adds explicit primitive throw/try-catch across calls.
`checked-f64-utf16-v8` adds nominal Error objects with checked messages and guards.
`checked-f64-utf16-v9` adds typed TypeError, RangeError and SyntaxError families.
`checked-f64-utf16-v10` makes checked JSON syntax/type failures catchable.
`checked-f64-utf16-v11` adds typed function values and capture-free arrows.
`checked-f64-utf16-v12` adds escaped closures over initialized lexical bindings,
plus map/filter/forEach/find/reduce/some/every as additive compiler operations.
`checked-f64-utf16-v13` adds sparse array presence and hole-preserving map results.
`checked-f64-utf16-v14` adds bounded indexed extension, delete and writable length.
General dynamic objects and external adapters remain pending. The
[migration contract](docs/development-plan.md#k4--ecmascript-value-profile-and-language-expansion)
preserves the existing artifact and tool semantics.

Primitive Tool Contract v1, offline discovery and checked JSON invocation are
implemented. A broader Application Contract and a long-lived application
service, MCP bridges, package signatures and desktop/database/hardware adapters
remain planned. Closed typed objects, `null` and bounded `JSON.stringify` are
implemented. Dense typed arrays and `varta.json.parse<T>` are also implemented.
Inbound HTTP/1 now supports checked synchronous handlers with isolated request
instances. General dynamic JSON parsing and broader web APIs remain pending;
`net.fetch`/`net.get` are outbound clients.
The [active delivery sequence](docs/development-plan.md#delivery-sequence) now
has delivered the first structured values/JSON, HTTP and independent runtime
packaging profiles. Broader language semantics and standard APIs remain core work;
generic application state and MCP follow those foundations.
A developer-written external server wrapper is an integration option, not the
acceptance target for an HTTP server on Varta.

Portable Wasm requires a compatible host; native launchers target a supported
OS/CPU. Source-free compilation does not guarantee that business logic cannot be
analyzed or modified. Authenticity requires verification by a trusted loader,
while concrete actions always remain subject to host and caller permissions.

## Quickstart

Build from a local checkout of this repository. CI uses Rust **1.98.0** on Linux
and macOS; native compiler/linker tools are required. See
[development setup](CONTRIBUTING.md#development-setup) for cloning and toolchain
installation. Windows and cross-target native packaging are not yet verified.

```sh
cargo install --path crates/varta-launcher --locked
cargo install --path crates/varta-cli --locked
varta init my-app
cd my-app
varta check
varta dev
varta build --single
./dist/app
```

Both installed binaries must share a directory. In this repository, `cargo build`
builds both; use `cargo run --bin varta -- <command>`. No external
TypeScript checker is required: check, dev and build share the same subset/type
and Wasm validation pipeline.

`build --single` uses the sibling `varta-runtime` binary, or an explicit trusted
`--launcher PATH`. It checks the native target, versions and bytecode before
writing outputs, without starting the guest application. It never falls back to
embedding the development CLI. Native packages target the build machine's OS/CPU;
this is not cross-compilation. The packaged program needs no source files,
configuration, development CLI or companion binary at deployment.
Companion negotiation now uses protocol v2 with supported language profiles;
rebuild/install both binaries together. Existing VARTA3 payloads remain readable.

The execution-only interface is:

```sh
./dist/app [--policy host-policy.json]
./dist/app --inspect --json
./dist/app --invoke toolName --params '{"name":"value"}' --json
varta-runtime run dist/app.wasm [--policy host-policy.json]
varta-runtime inspect dist/app --json
varta-runtime invoke dist/app.wasm toolName --params '{}' --json
```

Inspection validates code and reports schemas without starting the application.
Invocation calls only the published tool. An optional `--policy PATH` after the
command narrows package permissions for execution/invocation; it also appears in
inspection output. Standalone runtime inspection reports `manifest`, `module`,
`toolCatalog` (with generated JSON schemas) and `hostCeiling`; development CLI
inspection keeps its existing presentation.

```ts
export function main(): void {
  console.log("hello from varta");
  console.log(varta.time.now());
  console.log(varta.result.status());
}
```

## Binary64 numbers (opt-in)

Add `"languageProfile":"checked-f64-utf8-v1"` to your project configuration:

```json
{
  "entry": "src/main.ts",
  "target": "native",
  "out": "dist/app",
  "languageProfile": "checked-f64-utf8-v1",
  "capabilities": {}
}
```

```ts
export function main(): void {
  console.log(7 / 2);             // 3.5
  console.log(2147483647 + 1);    // 2147483648
  console.log(1 / -0);            // -Infinity
  console.log(-5.5 % 2);          // -1.5
}
```

This profile supports fractional/large numbers, NaN/Infinity, signed zero, numeric
comparisons/truthiness, functions, modules and control flow. It uses host ABI v2;
numbers occupy f64 slots and strings retain their existing UTF-8 representation.
Use the normal check/dev/build/run commands, including source-free native builds.
`process.exit` accepts only an exact finite i32 code and still requires permission.

Typed objects, dense fixed-length arrays, `JSON.stringify` and checked
`varta.json.parse<T>` now work with binary64 through structured-value ABI v3.
Numeric fields/elements retain signed zero and full precision. JSON emits
ECMAScript decimal notation, maps NaN/infinities to null and serializes -0 as 0.
Checked parsing accepts finite fractional/exponent numbers, with duplicate/unknown/
missing-field rejection. Overflowing numbers are rejected by this strict API.
Array indices must be exact nonnegative integers within bounds; invalid indices trap.

Tool/HTTP adapters, UTF-16 and general mixed-type coercion remain pending for
the newer profiles. Distinct undefined requires the v2 profile below. Omit `languageProfile` to keep the existing i32 profile and
its structured-value/server/tool features. Diagnostic number logs preserve -0 and
name NaN/Infinity; their finite formatting is not a Number::toString API.

## Reference unions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v4"` to combine primitives with a
closed record shape and an array element type:

```ts
function label(value: { n: number } | number[] | undefined): string {
  if (Array.isArray(value)) { return JSON.stringify(value); }
  return typeof value === "object" ? JSON.stringify(value.n) : "missing";
}
export function main(): void {
  const object = { n: 1.5 };
  const values = [object];
  const found = values[0];
  if (found !== undefined) { found.n = 2.5; }
  console.log(JSON.stringify(object)); // {"n":2.5}
  console.log(found === object);       // 1
  console.log(label([1.5, 2.5]));       // [1.5,2.5]
  console.log(values[9]);              // undefined
}
```

Nullable indexed reads now work for arrays of objects and arrays. Optional record
fields may also contain these references. Boxing retains the existing object;
mutation through an alias is visible and strict equality compares identity.
Objects/arrays are truthy even when empty. typeof reports object for both, including
null; exclude null separately when present. Direct-local `Array.isArray` and its
negation narrow if/ternary branches. Contextual literals, primitive subsets and
short-circuit operations retain the same evaluation order and owned references.

The checked increment permits **one record shape and one array element type per
union**, plus primitives/null/undefined. Multiple record shapes or array element
types, assertions and writes to guard-refined bindings remain rejected. JSON
serializes nested references with cycle/depth/node/byte checks; diagnostic union
logs use JSON for record/array values. Strict typed JSON still accepts only
non-union/non-optional targets; parsed required records use the new layout.
Tool/HTTP adapters remain pending.

Host ABI v7/value ABI v8/schema v7 uses reference-union tag 9 and versioned pure
imports. Primitive tag 8 retains its old meaning. Reference cells preserve exact
GC edges and never trace numeric bits. Earlier profiles keep their behavior.

## String repetition (opt-in)

`checked-f64-utf16-v9` and later additionally support `String.repeat(count?)`:

```ts
export function main(): void {
  console.log("ab".repeat(3)); // ababab
  try { "x".repeat(-1); }
  catch (error) {
    if (error instanceof RangeError) { console.log(error.message); }
  }
}
```

Counts accept Number, undefined and their union. Omitted/undefined/NaN counts
produce an empty string; fractions truncate toward zero. A negative normalized
count or either infinity raises RangeError, including on an empty receiver.
UTF-16 code units, lone surrogates and embedded NULs are copied exactly. Receiver
and count run once in source order, and live values survive collection.

The operation uses one result allocation and bulk-copy doubling for nonempty
results. It adds no host import, capability, ABI change or compiler helper.
Existing string/memory/fuel/deadline limits remain fatal; count coercion, borrowed
methods and non-string receivers remain unsupported.

## Array mutations (opt-in v14)

Select `"languageProfile":"checked-f64-utf16-v14"` for bounded array mutation:

```ts
export function main(): void {
  const values = [1, 2, 3];
  delete values[1]; // length remains 3
  values[5] = 6;   // intervening indices are absent
  console.log(JSON.stringify(values)); // [1,null,3,null,null,6]
  values.length = 1;
  values.length = 3;
  console.log(JSON.stringify(values)); // [1,null,null]
  try { values.length = 1.5; }
  catch (error) {
    if (error instanceof RangeError) { console.log(error.message); }
  }
}
```

Shrinking clears removed values and presence bits; growth creates holes. Delete
returns true for an index, including absence, and keeps length. Deleting the
nonconfigurable length property raises TypeError. Numeric delete keys and literal
keys are supported; dynamic strings/coercion and arbitrary own properties remain
unsupported. Indexed writes accept finite integral indices 0..1023, evaluate the
receiver/key/RHS once in order and check the numeric write domain after RHS effects.
A thrown key/RHS stops the requested mutation. Earlier RHS mutations remain.

Length accepts Number only: negative, fractional, nonfinite or above-uint32 values
raise RangeError. Negative zero sets length to zero. A valid uint32 length above
1024 exceeds the runtime resource bound and remains fatal; execution/ABI limits
also remain fatal. Earlier profiles retain their prior mutation restrictions.
V14 selects host ABI v17 with unchanged value/schema v12 and v6 value/JSON imports.
No new capability or host import is added. See the
[array mutation contract](docs/architecture.md#array-mutation-profile-and-host-abi-v17).

## Sparse arrays (opt-in v13)

Select `"languageProfile":"checked-f64-utf16-v13"` to represent absent indices:

```ts
export function main(): void {
  const values: (number | undefined)[] = [1, , undefined, 3];
  console.log(Object.hasOwn(values, 1)); // false
  console.log(Object.hasOwn(values, 2)); // true
  console.log(JSON.stringify(values)); // [1,null,null,3]
  values[1] = 2;
  console.log(Object.hasOwn(values, 1)); // true
  const mapped = [1, , 3].map(value => value * 2);
  console.log(JSON.stringify(mapped)); // [2,null,6]
  console.log(Object.hasOwn(mapped, 1)); // false
}
```

Reads and at return undefined for absent indices. Map preserves length and holes,
including indices removed before their turn; filter/forEach skip holes, while find
visits them as undefined. Slice copies presence into fresh storage; pop removes the
last index even if absent. Push appends present arguments, and assigning within
length fills a hole. Includes treats a hole as undefined; indexOf skips it.
Object.hasOwn accepts Number indices or literal keys; dynamic strings/coercion are
unsupported. Arrays containing only holes need an element type annotation.

JSON writes null for holes and explicit undefined. Checked parsing creates present
entries, so JSON cannot preserve absence; its schema must admit the serialized
nulls. Earlier profiles retain their layouts and v12's missing-map-index trap.
V13 uses host ABI v16/value ABI v12/schema v12 with versioned value/JSON imports;
permissions, recoverable JSON errors and fatal resource limits retain their roles.
The length bound remains 1024. Delete, indexed extension and writable length are
provided by v14 above. Constructors, prototypes and broader array-like behavior
remain pending. See the
[sparse array contract](docs/architecture.md#sparse-array-profile-and-host-abi-v16).

## Array callbacks (v12)

`checked-f64-utf16-v12` supports `map`, `filter`, `forEach` and `find` through
ordinary typed functions and closures:

```ts
export function main(): void {
  const values = [1, 2, 3];
  const offset = 10;
  console.log(JSON.stringify(values.map(value => value + offset))); // [11,12,13]
  console.log(JSON.stringify(values.filter(value => value > 1))); // [2,3]
  console.log(values.find(value => value === 2)); // 2
  let sum = 0;
  values.forEach(value => { sum = sum + value; });
  console.log(sum); // 6
}
```

Fixed callbacks take zero to three parameters: a checked prefix of `(value, index,
array)`. Optional/default/rest forms are described below. The receiver and callback are evaluated once, then the initial length is
saved. Later appends beyond that length are ignored; replacements and regrowth
within it are read at their turn. Filter/forEach skip removed entries. Find visits
removed entries as undefined, so its callback value is checked as `T | undefined`;
use a guard before arithmetic/property access. Find returns the value read before
the callback, or undefined if no predicate succeeds. Map/filter return fresh arrays;
filter retains the value read before the predicate, including reference identity.

Map infers expression results and requires an array-supported value type; void
callbacks produce undefined entries. Filter/find apply checked truthiness; host
result pairs cannot serve as predicates. ForEach ignores callback results and
returns undefined. Unannotated inline callback blocks infer their result from
typed return sites, including supported unions, bare returns and possible
fallthrough to undefined. Explicit annotations remain checked. A block without a
value-returning site keeps void semantics. An optional thisArg is supported as
described below; borrowed/prototype methods remain unsupported.

Arrays remain dense in v12. If map encounters an index removed before its turn,
it fails with the existing fatal checked-index trap: sparse mapped results are
not implemented. It never silently compacts or substitutes undefined for a hole.
Recoverable callback exceptions stop iteration normally; resource/ABI traps and
authorized exit stay fatal. No ABI/import/permission changes are required. Existing
1024-element, function/code-size and execution bounds apply. See the
[array callback contract](docs/architecture.md#additive-array-callback-operations).

## Array reduce, some and every (v12 and later)

These methods use the same typed functions and closures, including in v14:

```ts
export function main(): void {
  const values = [1, 2, 3];
  console.log(values.reduce((total, value) => total + value, 0)); // 6
  console.log(values.reduce((total, value) => total + value)); // 6
  console.log(values.some(value => value > 2)); // true
  console.log(values.every(value => value > 0)); // true
  console.log(values.reduce((text, value) => text + "!", "seed")); // seed!!!
}
```

All three snapshot length after their argument expressions and inspect live
entries at each turn. They skip removed indices and, from v13, holes. Some stops
at the first truthy predicate, every at the first falsy predicate. Empty/all-hole
arrays return false for some and true for every without invoking the predicate.
Predicates support the same checked truthiness and fixed-parameter prefix as
filter, with optional/default/rest parameters and typed thisArg described below.

Reduce accepts a callback plus an optional initial value. Its fixed parameters
form a checked prefix of `(accumulator, value, index, array)`, and its result must have
the accumulator type. That type comes from the seed expression, or the element
type without a seed. Inline reducer arrows infer parameters and use that type
for expression/block returns. An annotated accumulator parameter, an annotated
inline return type, or a named/local/returned reducer signature provides context
for the seed, including empty arrays, nested literals and closed optional records.
Without that context, an aggregate seed can use the expected aggregate result
from a variable, return or argument position. Scalar seeds keep their own type
when only the surrounding result is wider. Explicit undefined is a supplied seed;
a void seed expression evaluates once and supplies undefined.

Without a seed the first present element initializes the accumulator without a
callback. No present element raises a catchable TypeError with message
`Reduce of empty array with no initial value`. With a seed an empty/all-hole
array returns it unchanged. Accumulators retain references, function values and
physical VartaResult pairs through allocations and exceptions. Existing limits,
capabilities, status and ABI versions are unchanged. See the
[reduction contract](docs/architecture.md#additive-array-reduction-and-quantifiers).

For example, these empty seeds now receive their types from context:

```ts
export function main(): void {
  const collected: number[] = [1, 2].reduce((items, value) => {
    items.push(value);
    return items;
  }, []);
  const nested = [1, 2].reduce((state: { items: number[] }, value) => {
    state.items.push(value);
    return state;
  }, { items: [] });
  console.log(JSON.stringify(collected)); // [1,2]
  console.log(JSON.stringify(nested)); // {"items":[1,2]}
  console.log(JSON.stringify([1, 2].map(value => { return { n: value }; })));
}
```

Inference is limited to the existing checked types. Multiple incompatible record
shapes/array element types, function/physical-result unions and a seed with no
inferable type or context still require a different declaration or an explicit
annotation. Return analysis is conservative: all lowered return sites contribute,
including ones in finalizers; a possible normal exit contributes undefined.
Ordinary arrows outside these contextual array callbacks retain their earlier
Number/void defaults.

## Lexical closures (opt-in)

Select `"languageProfile":"checked-f64-utf16-v12"` for lexical captures:

```ts
function counter(value: number): () => number {
  return () => { value = value + 1; return value; };
}
export function main(): void {
  const next = counter(0);
  console.log(next()); // 1
  console.log(next()); // 2
}
```

Closures share live bindings with their outer scope and sibling closures. Separate
calls own separate bindings. Nested closures can forward captures, and escaped
strings, objects, arrays, function values and captured host-result pairs stay
rooted. Each iteration's captured `let` binding is separate, including `continue`.
Managed tracing also reclaims unreachable cycles involving closures and bindings.

This increment captures bindings initialized before arrow creation. Captures
before initialization (including self-initialization), mutable captured union
narrowing and captures under an active guard refinement are explicitly rejected.
Function expressions, private self names and typed this are now available below.
Outer-binding recursive initialization, function-valued aggregates/JSON and async
remain pending. The profile uses host ABI v15, unchanged value/schema v11,
and retains all preceding system permissions and fatal execution limits. See the
[closure contract](docs/architecture.md#lexical-closures-and-host-abi-v15).

## Anonymous functions and typed this (v12 and later)

Anonymous `function` expressions share the existing managed closures, exact
signatures and return inference. Parameter types come from annotations or a
function/Array callback context:

```ts
export function main(): void {
  const context = { offset: 10 };
  const add = function (this: { offset: number }, value: number) {
    return this.offset + value;
  };
  console.log(add.call(context, 2)); // 12
  console.log(JSON.stringify([1, 2].map(add, context))); // [11,12]

  const reader = function (this: { offset: number }): () => number {
    return () => this.offset;
  };
  const read = reader.call(context);
  context.offset = 20;
  console.log(read.call({ offset: 99 })); // 20: lexical this
}
```

A typed `this` parameter is separate from positional arguments. In this increment,
read `this` through an explicit function annotation or a function-type
context containing `this`. Calls retain primitive/null/undefined values exactly;
there is no global-object substitution or primitive boxing. Bare calls supply
undefined and require a receiver type admitting it. `this: void` uses undefined.

Map/filter/forEach/find/some/every accept an optional second argument as thisArg.
Receiver, callback and thisArg evaluate once in order before the length snapshot,
including empty arrays. Arrows and functions with no typed receiver ignore thisArg
while preserving its effects; arrows capture a surrounding typed this. Reduce's
second argument remains its seed, with undefined as the callback's this value.
`fn.call(thisArg, ...arguments)` is a checked intrinsic with exact argument types
and counts; call with no arguments supplies undefined to a zero-arity function.

Named expressions and declaration this are implemented below. Implicit this
inference from an object method/thisArg alone, top-level this, constructors,
arguments, bind/apply, async/generator functions and function-valued aggregates
remain unsupported. Optional/default/rest parameters are supported below. Arrow conversion to a dynamic-this function type
is also rejected. Explicit this is part of the invariant function type and counts
toward the physical parameter limit together with the closure environment.
No host operation, capability, heap layout or ABI version changes. See the
[typed this contract](docs/architecture.md#anonymous-function-expressions-and-typed-this).

## Named function expressions and declaration receivers (v12 and later)

A named expression owns a private immutable self name. Recursion follows that
same function object even after another variable is reassigned:

```ts
function add(this: { offset: number }, value: number): number {
  return this.offset + value;
}
export function main(): void {
  const factorial = function fact(n: number): number {
    if (n <= 1) return 1;
    return n * fact(n - 1);
  };
  console.log(factorial(5)); // 120
  console.log(add.call({ offset: 10 }, 2)); // 12
  console.log(JSON.stringify([1, 2].map(add, { offset: 10 }))); // [11,12]
}
```

Named expressions require a return annotation or an expected function type,
including reduce's accumulator context. Their name is visible only inside the
function and nested closures; parameters/body locals may shadow it. Assignment
to the private name is rejected. Escaped closures retain its original identity
and captured environment. Capturing an uninitialized outer variable still fails:
use the private name for recursion instead of the surrounding variable.

Declaration this remains separate from positional parameters and part of the
exact type. References/import aliases preserve the declaration's canonical
identity; call uses an explicit checked receiver, while a bare call supplies
undefined and requires a receiver type accepting it. A declaration receiver can
be captured lexically by returned arrows. Main, Tool Contract v1 and HTTP v1 keep
their existing receiver-free entry contracts. No ABI, import or heap-layout change
is required. See the [self-binding contract](docs/architecture.md#named-expression-self-bindings-and-declaration-receivers).

## Optional, default and rest parameters (v12 and later)

Declarations, arrows, function expressions, typed function values and call support
these parameter forms:

```ts
function sum(first: number = 1, ...values: number[]): number {
  return values.reduce((total, value) => total + value, first);
}
function optional(value?: number): number | undefined { return value; }
export function main(): void {
  console.log(sum()); // 1
  console.log(sum(undefined, 2, 3)); // 6
  console.log(optional()); // undefined
  const greet = function (name = "world") { return name; };
  console.log(greet()); // world
  console.log(JSON.stringify([1, 2].map((value, index, array, extra = 10) => value + extra)));
  // [11,12]
}
```

An optional parameter has type `T | undefined` in the body. A default runs only
for omission or explicit undefined, preserving null/zero/false/NaN as ordinary
values when admitted by the type. All supplied argument expressions run first;
initializers then execute left to right on every call. They can use initialized
earlier parameters, outer bindings, typed this and a named expression's self name.
Self/later parameter reads and captures before initialization are rejected;
function-body declarations are not visible to initializers.

Types come from annotations/function context, or scalar literal defaults
(number/string/boolean/null, including parenthesized/negative numbers). Other
default expressions require an annotation or context. Function/physical-result
optional/default parameters remain unsupported because their undefined unions
are not part of the checked value model. Required function/result-pair parameters
still work beside defaults.

A default before a later required parameter does not allow omission of that
required position: use an explicit undefined argument. A rest parameter must be
last and have a checked array type. Surplus arguments form a fresh dense array
of at most 1024 elements; empty rest is a fresh empty array. Children retain their
ordinary identity. Spread calls and parameter destructuring are supported below.

Function types retain exact optional/rest modes; parameter variance/adaptation
remains unsupported. Array callbacks receive their real value/index/array (and
reduce accumulator) arguments: fixed prefixes discard unused tails, optional
extra parameters get undefined/defaults, and rest captures all remaining arguments
with checked element types. A rest callback must admit the heterogeneous array
argument if it captures it. Empty traversals do not run defaults. For a named
reducer with an optional/rest first parameter, result type supplies the seed
context so an undefined-capable parameter does not widen a numeric accumulator.
No host operation, helper, layout or ABI version changes. See the
[parameter contract](docs/architecture.md#optional-default-and-rest-parameter-contract).

## Spread call arguments (v12 and later)

Checked function calls accept narrowed arrays interleaved with explicit arguments:

```ts
function sum(first: number = 1, ...values: number[]): number {
  return values.reduce((total, value) => total + value, first);
}
export function main(): void {
  const values = [2, 3];
  console.log(sum(...values)); // 5
  console.log(sum(undefined, ...values, 4)); // 10
  console.log(sum(...[])); // 1
}
```

Declarations, function values/closures and `fn.call(explicitThis, ...values)` use
this path. The callee, receiver and argument operands evaluate once in source
order. Each spread takes a shallow snapshot before the next operand runs: later
array replacement/deletion/resizing cannot change its values, while mutations to
referenced children remain visible. Sparse holes become explicit undefined values;
they can trigger defaults, and rest arrays contain present entries for those holes.

The flattened argument count is capped at 1024. Exceeding that resource bound is
fatal; invalid dynamic arity or incompatible argument types throw an owned
TypeError before any default initializer or function body starts. Supplied
argument effects have already occurred. Checked types must match each destination
or widen to its declared union; undefined-capable spread elements can narrow to a
required type after an undefined check. Broader union discrimination/coercion is
not supported. Explicit function and physical-result arguments can coexist with
array spreads without entering an aggregate.

Only checked arrays are admitted. String/general iterable spread, spread thisArg,
spread to host/built-in/Array methods and array/object spread literals remain
unsupported. Parameter destructuring is supported below. A generated call wrapper and its evaluated
operand segments count toward the existing function/body/32 physical-slot limits.
The implementation adds no import, heap layout, helper or ABI version. See the
[spread contract](docs/architecture.md#bounded-spread-call-contract).

## Destructured function parameters (v12 and later)

Declarations, arrows, function expressions, callbacks and typed function values
accept nested object/array parameters, renaming, defaults, elisions and rest:

```ts
function describe(
  {name: label = "item", child: {n}, ...other}:
    {name?: string; child: {n: number}; enabled?: boolean},
  [first = 1, ...tail]: number[] = []
): void {
  console.log(label);
  console.log(n);
  console.log(first);
  console.log(JSON.stringify(tail));
  console.log(JSON.stringify(other));
}
export function main(): void {
  describe({child: {n: 3}, enabled: false}, [2, , 4]);
}
```

The incoming aggregate needs an annotation or function/callback context. Its
original type remains the call signature; destructured names are local bindings.
Defaults run only for undefined, in parameter/property/element order, after all
supplied arguments. Earlier bindings, this and private self are visible; own/later
reads or captures and duplicate names are rejected. Nested defaults infer their
present/default value types within the existing checked union model. Top-level
parameter defaults retain the annotation/context rules above.

Object patterns use closed records and static keys, including computed string/
number literals. Reads observe earlier default mutations. Object rest is a fresh
shallow record preserving present fields, absent versus explicit undefined, child
identity and the source's current enumeration order. Array patterns read live
elements in sequence; elisions advance, missing/holes yield undefined, and an
exhausted iterator stays exhausted even if a later default grows the source.
Array rest is fresh and dense. In sparse v13+, its element type admits undefined;
dense v12 keeps the source element type. Individual array bindings also admit
undefined unless a default removes it.

Nullish aggregate inputs throw owned TypeError before later bindings/body code.
Resource limits remain fatal. A function's parameter patterns are bounded to 1024
nodes and depth 64, alongside existing source/type/body/frame limits. Destructured
rest parameters and nested array rest patterns are supported; object rest must
end in an identifier. Dynamic keys, primitive boxing, string/custom iterables,
array-as-object patterns and catch destructuring remain unsupported.
Local const/let patterns are supported below. No ABI, import, managed layout or helper changes. See the
[parameter pattern contract](docs/architecture.md#parameter-destructuring-contract).

## Local const and let destructuring (v12 and later)

Local lexical declarations use the same checked object/array binding mechanism
as parameters, including aliases, nested patterns, defaults, elisions and rest:

```ts
export function main(): void {
  const {name: label = "item", ...other}: {name?: string; enabled?: boolean} =
    {enabled: false};
  let [first = 0, ...tail]: number[] = [1, , 3];
  first = first + 1;
  console.log(label); // item
  console.log(first); // 2
  console.log(JSON.stringify(tail)); // [null,3]
  console.log(JSON.stringify(other)); // {"enabled":false}
}
```

The initializer runs once. Its type is inferred, or supplied by an aggregate
annotation on the whole pattern; empty arrays still need an element context.
All names in the enclosing block start uninitialized, so own/later reads and
captures are rejected even across declarators/statements. Names initialize left
to right and earlier leaves are visible in later defaults. Const prevents binding
reassignment, including through closures; referenced children remain mutable.
Let supports the existing checked assignments and shared capture cells. Nested
blocks restore shadowed bindings, and escaped closures retain their own activation.

Patterns also work in ordinary `for` initializers. For let, the first iteration
and subsequent updates use distinct captured cells; a closure created in the
initializer keeps its original bindings. Continue/finally follows the normal
update path, while break/return/throw leaves the loop scope. Const loop bindings
retain one environment. For-in/of remains unsupported.

The parameter rules for live array reads, persistent exhaustion, dense rest,
object rest presence/order/identity, nullish TypeError and fatal resource limits
also apply. Shared depth/node limits are 64/1024 per lexical declaration, alongside
existing source/body/frame limits. Initializers are required for destructuring.
Var/ambient, top-level and catch patterns remain unsupported; checked assignment
statements are supported below. No new
host operation, heap layout, helper or ABI version is introduced. See the
[lexical binding contract](docs/architecture.md#local-lexical-destructuring-contract).

## Destructuring assignment statements (v12 and later)

Existing initialized mutable locals/parameters can be updated with checked
object/array assignment patterns:

```ts
export function main(): void {
  let name = "old";
  let first = 0;
  let tail: (number | undefined)[] = [];
  const read = () => name;
  ({name = "item"} = {});
  [first = 1, ...tail] = [2, , 3];
  console.log(read()); // item
  console.log(first); // 2
  console.log(JSON.stringify(tail)); // [null,3]
}
```

The RHS evaluates once before any target write. Nested patterns, static aliases/
keys, defaults, elisions and object/array rest reuse the shared binding machinery.
Writes update the existing captured cells in source order. Repeated targets are
allowed; later defaults can read earlier writes or the target's previous value.
Earlier successful writes remain visible if a later default or checked read
throws. Const, uninitialized/unknown targets, incompatible types and writes under
an active union guard are rejected with source diagnostics.

Exact types and declared union widening are accepted. When an array/optional
field can yield undefined but a compatible target requires a present value, a
pure guest check throws owned TypeError `Invalid destructuring assignment value`
for undefined before writing that target. Defaults use the target type as context,
including empty aggregate/function/result defaults. Homogeneous array targets can
provide RHS element context; ambiguous nested/empty inputs still need a typed
source. Aggregate and array types remain invariant, without arbitrary union
narrowing/coercion. Nullish aggregate errors, live array reads/exhaustion, shallow
rest identity/presence/order and fatal resource behavior keep their contracts.

Standalone statements (including parentheses), expression results/chains and
ordinary for initializer/update statements are supported. Checked member targets
are supported below; dynamic record keys, catch and for-in/of patterns remain
unsupported. Shared pattern
limits also apply per assignment. No new import/helper/layout or ABI version is
introduced. See the [assignment contract](docs/architecture.md#checked-destructuring-assignment-contract).

## Destructuring assignment values (v12 and later)

A successful assignment expression returns its original RHS, preserving identity,
all source fields, array holes and the actual contextual source type:

```ts
export function main(): void {
  let n = 0;
  const source = {n: 2, extra: "kept"};
  const original = ({n} = source);
  console.log(original === source); // 1
  console.log(original.extra); // kept
  const values: number[] = [, 3];
  const same: number[] = ([n = 7] = values);
  console.log(same === values); // 1
  console.log(Object.hasOwn(same, 0)); // 0
  console.log(n); // 7
}
```

Use assignment values in local initializers, arguments, returns, member reads,
callbacks, admitted operators/conditionals and nested assignment chains. Each
RHS evaluates once; outer targets wait for inner assignments to succeed. Defaults
only update targets and do not fill source holes/properties. Expected aggregate
context flows into the RHS, so a declared result type is checked against the whole
source rather than only selected fields. Existing inference/type restrictions
still apply to ambiguous aggregate literals and mixed operators.

Earlier operands/arguments, the saved RHS and the returned value remain rooted
across defaults, collection and later argument evaluation. A failed traversal
publishes no success result, skips later enclosing work and preserves prior writes
through catch/finally. Existing mutability/type restrictions, resource
budgets, status and real grants remain active. No import/helper/layout/ABI change.
See the [value contract](docs/architecture.md#destructuring-assignment-value-contract).

## Member targets in destructuring assignment (v12 and later)

Assignment patterns can write checked record fields and numeric array elements,
including nested targets, defaults and final object/array rest:

```ts
export function main(): void {
  const output = {n: 0};
  const values: number[] = [];
  ({value: output.n = 7} = {});
  [values[2]] = [output.n]; // v14 permits indexed extension
  console.log(output.n); // 7
  console.log(JSON.stringify(values)); // [null,null,7]
}
```

Each receiver and array index evaluates once, in source order, before reading
that source property/element or executing its default. Rest targets evaluate
before copying. Nested targets wait until their containing source value is read.
Receivers/indices stay fixed and rooted across defaults, even if a default
reassigns the original variable or resizes the array; the write uses the saved
receiver and its current backing. Later references observe earlier writes.
An exception skips remaining work and preserves successful writes.

Const prevents rebinding the variable; its mutable fields/elements remain valid
targets. Exact types, union widening, checked Undefined narrowing and target-typed
defaults reuse ordinary checked writes. The assignment still returns its original
RHS. Only admitted static record keys and Number array indices are supported;
Error/result properties, unresolved nullable receivers and coercive properties
remain rejected. Empty/ambiguous RHS arrays may need an element annotation.

V12 replaces existing array elements; v13 can also fill holes within length.
V14 extends arrays with holes and admits `.length` targets, using the existing
RangeError for invalid lengths. Numeric index validation follows default/RHS
effects. Resource violations, including lengths above 1024, stay fatal.
Declaration/parameter patterns still bind names. No imports, layouts or ABI
versions change. See the [member contract](docs/architecture.md#destructuring-member-target-contract).

## Typed function values (opt-in)

Select `"languageProfile":"checked-f64-utf16-v11"` for typed function values:

```ts
function apply(fn: (value: number) => number, value: number): number {
  return fn(value);
}
export function main(): void {
  console.log(apply(value => value + 1, 41)); // 42
}
```

Functions can be stored, reassigned, passed and returned with exact parameter and
result types. Named function references retain their identity across import aliases;
each arrow evaluation creates a fresh identity. Arrow parameters can use the
expected function type, and expression/block bodies are supported. Local calls
preserve argument order, errors, permissions and resource limits.

This increment supports arrows without captured outer variables. Capturing locals,
function expressions, function-valued object/array fields, optional/rest/default
parameters, async functions and generic functions remain unsupported. Functions
cannot enter JSON, unions or the exception channel. Use explicit annotations for
block-arrow results other than the existing default Number/void inference. The
profile uses host ABI v14 with unchanged value/schema v11, and inherits v10 APIs;
modern Tool/HTTP adapters remain pending. See the [function contract](docs/architecture.md#typed-function-values-and-host-abi-v14).

## Recoverable checked JSON errors (opt-in)

Select `"languageProfile":"checked-f64-utf16-v10"` to catch failures of
`varta.json.parse<T>`:

```ts
export function main(): void {
  try {
    const value = varta.json.parse<{n: number}>('{"n":');
    console.log(value.n);
  } catch (error) {
    if (error instanceof SyntaxError) { console.log(error.message); }
    else if (error instanceof TypeError) { console.log(error.message); }
  }
}
```

Malformed JSON produces SyntaxError. Valid JSON that violates the closed schema
(including unknown/duplicate/missing fields or unsupported numeric values) produces
TypeError. This is Varta's checked API, not unrestricted JavaScript JSON.parse.
Messages are fixed and do not echo input. Calls, rethrow and finally use the same
owned Error model; failed parsing stops later expression/argument effects.

Input/transcoding size, array/node/depth/string limits, fuel, deadlines, memory,
root/output/host-call budgets, invalid ABI/schema/helpers and authorized exit
remain fatal. Only declared syntax/type failures enter catch. System status and
capability grants retain their meaning. The opt-in requires host ABI v13 and
json_parse_v13, retaining value/schema v11 and existing serializers/value imports.
Earlier profiles retain fatal JSON failures. General JSON.parse, serializers and
other API error conversions remain further core work.

## Native error families (opt-in)

Select `"languageProfile":"checked-f64-utf16-v9"` for `TypeError`, `RangeError`
and `SyntaxError`, with or without `new`. Each accepts the checked string/undefined
message rules of Error. An instance matches its own family and `instanceof Error`,
while siblings remain distinct. Its immutable `name`, `toString()`, logging and
uncaught diagnostics preserve the family.

```ts
function fail(): void { throw new TypeError("invalid input"); }
export function main(): void {
  try { fail(); }
  catch (error) {
    if (error instanceof TypeError) { console.log(error.message); }
  }
}
```

In this profile an `Error` annotation accepts all four supported classes; specific
family annotations and closed family unions retain their restrictions. Guard a
base Error before passing it to a TypeError parameter. Arrays, records, calls,
rethrow, finally and collection preserve identity and class. JSON still emits `{}`.
Host ABI v12/value ABI v11/schema v11 keep earlier profiles unchanged.

These constructors currently create explicit guest exceptions. Existing checked
JSON/index failures and execution limits remain fatal; they are not automatically
converted into these classes. Other native families, custom subclasses, writable
properties, cause/options and stack traces remain pending.

## Error objects (opt-in)

Select `"languageProfile":"checked-f64-utf16-v8"` for the first checked Error
model, inheriting v7 exceptions/finally and the preceding value features:

```ts
function fail(): void { throw new Error("invalid input"); }
export function main(): void {
  try { fail(); }
  catch (error) {
    if (error instanceof Error) { console.log(error.message); }
  }
}
```

`Error(message?)` and `new Error(message?)` accept string, undefined or their
union, evaluating the argument once. Each call creates a fresh object with an
immutable `name` of `"Error"` and checked UTF-16 `message`. `toString()` and logging
use `Error` or `Error: message`. `Object.hasOwn(error,"message")` distinguishes
an omitted/undefined argument from an explicit empty string. The message is
non-enumerable, so JSON.stringify emits `{}`. Arrays, records, optional fields,
unions and function calls retain Error identity and references through collection.
Catch values include Error and primitives; narrow before reading properties.

Host ABI v11/value ABI v10/schema v10 reject earlier interpretations of these
objects. Uncaught errors expose an owned typed message and presence flag, with
bounded escaped diagnostics. Earlier profiles retain their behavior.

This increment rejects property mutation, coercive message arguments, causes,
subclasses/native error families, prototype editing and stack traces. Typed JSON
parsing cannot construct Error objects. Resource/JSON/index traps and authorized
process exit still bypass catch/finally; system failures retain result/status APIs.

## Primitive exceptions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v7"` for explicit `throw` and
`try/catch/finally`, including exceptions crossing named function/module calls:

```ts
function checked(value: number): number {
  if (value < 0) { throw "negative value"; }
  return value;
}
export function main(): void {
  try {
    console.log(checked(-1));
  } catch (error) {
    if (typeof error === "string") { console.log(error); }
  }
  console.log("continued");
}
```

Throw supports Number, boolean, string, null, undefined and their primitive
unions. A catch binding has that checked primitive union; narrow it before
arithmetic/string operations. `catch { ... }`, nested handlers, rethrow, return,
break and continue are supported. Unfinished expressions and later argument/
system effects are skipped. Caught values retain exact bits/code units and remain
alive through collection. A pure catch does not reset the last system status.

`finally` runs once after normal completion, return, throw, break or continue
leaving its protected region. Return values are saved before finalization; a new
abrupt completion in the finalizer replaces the old one. Nested finalizers run
from inside out, retaining saved values through collection.

This increment rejects Error objects, aggregate throws, catch
annotations/destructuring and general unknown/any values. JSON validation failures,
index/resource traps and authorized process exit remain execution boundaries;
they bypass both catch and finally.
Capability denial still uses the existing status/result API; application code may
explicitly throw after inspecting it.

Host ABI v10 keeps value/schema v9 and adds a checked, rooted exception channel.
Uncaught values become an owned typed `RuntimeError::Uncaught`; rendered string
diagnostics are escaped and limited to 256 UTF-16 units. Native and portable
artifacts use the same behavior. Earlier profiles retain their original contracts.

## Resizable arrays (opt-in)

Select `"languageProfile":"checked-f64-utf16-v6"` for dense arrays whose length
can change through `push` and `pop`:

```ts
export function main(): void {
  const values: (number | string)[] = [];
  const alias = values;
  console.log(values.push(1.25, "😀")); // 2
  console.log(alias.pop());            // 😀
  console.log(JSON.stringify(values)); // [1.25]
  console.log(alias === values);       // 1
  alias.pop();
  console.log(values.pop());           // undefined
}
```

Array identity stays stable while backing storage grows geometrically. Aliases,
record fields, function/module boundaries and parsed JSON arrays see the same
updates. All push arguments evaluate once from left to right before appending;
`push()` returns the current length. `pop()` removes the last element and returns
its actual value, or undefined when empty. Removed references are cleared, while
returned objects remain alive through ordinary roots.

The current bound remains **1024 elements**. A push exceeding it traps before
changing the target, after argument evaluation. Indexed writes still replace
existing elements only; assigning length, holes, deletion, spreads, constructors,
iterators and array methods beyond push/pop/at/slice/includes/indexOf remain unsupported. Closed
element/union types and conservative guard narrowing still apply.

`at(index?)` supports negative indices from the end and returns the element or
undefined when outside the array. `slice(start?, end?)` creates a fresh array with
independent storage and the same element type. Object/array elements keep their
original references, so changing a nested object is visible through both arrays.

```ts
export function main(): void {
  const values = [{ n: 1.25 }, { n: 2.5 }];
  const copied = values.slice(-1);
  const last = copied.at(-1);
  if (last !== undefined) { last.n = 3.5; }
  copied.pop();
  console.log(JSON.stringify(values)); // [{"n":1.25},{"n":3.5}]
  console.log(copied.length);          // 0
}
```

Bounds accept number, undefined or number|undefined. Fractions truncate toward
zero, NaN acts as zero, and infinities are handled without integer conversion
traps. Slice clamps negative/outside bounds; undefined end selects the current
length. Receiver/start/end expressions evaluate once before selecting the range,
so their mutations use the current array storage. Boolean/string/null coercion,
spread arguments and borrowed methods remain unsupported. These are additive
compiler operations within the same v6 profile and unchanged ABI/layout.

`includes(value?, fromIndex?)` returns a boolean; `indexOf(value?, fromIndex?)`
returns the first matching index or -1. Search values may be any supported checked
value; different kinds do not match. Strings compare UTF-16 contents and objects/
arrays compare identity. Includes finds NaN, while indexOf does not; both match
the two signed zeroes. Omitting the value searches for undefined.

```ts
export function main(): void {
  const values = [0 / 0, -0, 2.5];
  console.log(values.includes(0 / 0)); // 1
  console.log(values.indexOf(0 / 0));  // -1
  console.log(values.indexOf(0));      // 1
  console.log(values.includes(2.5, -1)); // 1
}
```

The optional starting index follows the numeric/undefined bound rules above.
Receiver, value and starting-index expressions execute once before the scan,
even when the array is empty. Search itself allocates nothing and preserves status.

Host ABI v9/value ABI v9/schema v9 isolate the new storage format. JSON uses
`json_stringify_v9`/`json_parse_v10`; earlier profiles retain fixed-length arrays.
Tool/HTTP adapters for the newer profiles remain pending.

## Typed JSON with optional fields and unions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v5"` to decode a closed schema with
optional fields and mixed values:

```ts
export function main(): void {
  const value = varta.json.parse<{
    child?: { n: number };
    items: (number | string | null)[];
    label?: string;
  }>('{"items":[1.25,"😀",null],"child":{"n":2.5}}');
  console.log(Object.hasOwn(value, "label")); // 0: absent
  const child = value.child;
  if (child !== undefined) { child.n = 3.5; }
  console.log(JSON.stringify(value));
  // {"child":{"n":3.5},"items":[1.25,"😀",null]}
}
```

All input and schema checks finish before guest allocation. Each union selects
its unique JSON-kind member, without coercion or speculative object-shape matching.
The existing limit of one record shape and one array element type per union stays.
Missing optional fields read as undefined but remain absent; later assignment
sets presence normally. Required fields must be supplied even when their type
includes undefined. JSON has no undefined token. Duplicate/unknown keys, wrong
members, missing required fields, non-finite numbers and budget violations trap.
UTF-16 lone units and signed zero survive decoding; record enumeration is normalized
to schema order, with absent fields excluded until assigned.

Host ABI v8 introduces `json_parse_v9` and JSON schema v8 while retaining value
ABI v8 and `json_stringify_v8`. Earlier profiles retain their parsing rules.
This remains the strict typed API; generic JSON.parse, revivers, broader shapes,
resizable arrays and new-profile Tool/HTTP adapters remain pending.

## Optional fields and missing values (opt-in)

Select `"languageProfile":"checked-f64-utf16-v3"` for optional primitive fields,
own-property checks and missing reads:

```ts
export function main(): void {
  const value: { label?: string; n: number } = { n: 1.5 };
  console.log(Object.hasOwn(value, "label")); // 0
  console.log(value.label);                  // undefined
  value.label = "😀";
  console.log(JSON.stringify(value));        // {"n":1.5,"label":"😀"}
  value.label = undefined;
  console.log(Object.hasOwn(value, "label")); // 1
  console.log(JSON.stringify(value));        // {"n":1.5}
  const absent = [1.5][9];
  console.log(absent ?? "missing");          // missing
  console.log(JSON.stringify("😀"[0]));       // "\ud83d"
  let pending: string | undefined;
  console.log(pending);                      // undefined
}
```

An absent field differs from an own property whose value is undefined. New fields
first assigned after construction join insertion order; overwriting them keeps
their place, and integer-index keys still enumerate first. `Object.hasOwn` accepts
closed records and literal keys. Optional field values must be primitives/unions;
readonly fields, deletion, arbitrary new keys and prototype lookup remain unsupported.
Reads of statically absent literal record keys return undefined, except inherited
prototype names, which are rejected. Optional inherited names are also rejected.

Numeric primitive-array and UTF-16 string reads return `T | undefined`. Indices
must be exact nonnegative integers within the actual length; other numeric values
return undefined without truncation. Minus zero selects index zero. Array writes
still require an in-range index; resizing/sparse writes and object/array-element
reads that need reference unions remain unsupported. String/number key coercion
is not supplied. Earlier profiles keep their existing rejection/trapping behavior.

An annotated `let` without an initializer is admitted only when its type includes
undefined; declaration-before-use checks remain. Undefined-capable function returns
may be bare or fall through. Host ABI v6/value ABI v7/schema v6 separate this
behavior and the presence/order record trailer from all older artifacts. Strict
JSON parsing still rejects optional/union targets, while required record parsing
produces the new layout. Tool/HTTP adapters remain pending.

## Primitive unions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v2"` for unions of `number`, `string`,
`boolean`, `null` and `undefined`, including parameters, returns, mutable bindings,
record fields and explicitly typed array elements:

```ts
function label(value: number | string | undefined): string {
  if (typeof value === "number") { return JSON.stringify(value + 0.5); }
  return typeof value === "string" ? value + "!" : "missing";
}
export function main(): void {
  let value: number | string | undefined = 1.5;
  console.log(label(value)); // 2
  value = "😀";
  console.log(label(value)); // 😀!
  const values: (number | string | undefined)[] = [value, undefined, -0];
  console.log(JSON.stringify(values)); // ["😀",null,0]
}
```

Runtime `typeof`, strict equality, truthiness and union `&&` / `||` / `??` preserve
actual values and short-circuit effects. Mixed primitive ternaries infer a union.
Direct-local `typeof`, null and global-undefined strict guards narrow `if` and
ternary branches. Narrowing stays inside the guarded branch; writes to its refined
binding are currently rejected, including loop back edges. Arithmetic and system
calls require the appropriate narrowed type. Other loose/coercive union comparisons,
object/array union members, `any`/`unknown`, assertions and uninitialized bindings
remain unsupported. Use explicit union annotations for mixed array elements.

Host ABI v5/value ABI v6 stores immutable tagged primitive cells in the traced
heap. Numeric payloads retain all bits; only string payloads create a child edge.
JSON omits union fields whose current value is undefined, nulls such array entries,
and returns a real `string | undefined` for an undefined-capable union root.
Schema v5 retains strict parsing of supported non-union targets; parsing a union
or a container with union fields is explicitly rejected. Tool/HTTP adapters remain
pending. All earlier profiles and the default i32 profile keep their behavior.

## UTF-16 strings (opt-in)

Select `"languageProfile":"checked-f64-utf16-v1"` for binary64 numbers,
undefined and UTF-16 string values, including lone surrogate code units:

```ts
export function main(): void {
  const text = "😀";
  console.log(text.length);                    // 2
  console.log(text.charCodeAt(0));              // 55357
  console.log(JSON.stringify(text.charAt(0)));  // "\ud83d"
  console.log("\ud83d" + "\ude00" === text);     // 1
  console.log(JSON.stringify(varta.json.parse<string>('"\\ud800"')));
  // "\ud800"
}
```

Length and character methods count UTF-16 code units. `charAt` returns an empty
string outside the range; `charCodeAt` returns NaN. Positions accept numbers,
truncate toward zero and default to zero; implicit argument coercion is unsupported.
Content comparisons use unsigned UTF-16 code-unit order. JSON escapes unpaired
surrogates and checked parsing preserves escaped or raw lone units in string values.

Host ABI v4/value ABI v5 and JSON schema v4 keep this representation separate
from every UTF-8 profile. Files, environment values and HTTP response bodies are
validated as UTF-8 and converted to UTF-16. Resource names must convert exactly:
lone surrogates and NUL fail before OS access, without replacement or normalization.
Diagnostic logs replace unpaired units with U+FFFD; use JSON for lossless inspection.

The 1 MiB string limit counts stored bytes (at most 524288 code units), and the
64 KiB static budget also counts UTF-16 bytes. JSON input has a 64 KiB physical
and converted-text limit; output has both UTF-8 JSON and stored UTF-16 limits.
Closed record keys/schema metadata remain valid UTF-8; surrogate keys, string
bracket access, dynamic unions and Tool/HTTP adapters remain unsupported.

## Objects and JSON

For distinct `undefined`, select `"languageProfile":"checked-f64-utf8-v2"`.
It keeps binary64/UTF-8 and adds the `undefined` type, global value, `void`, typed
`typeof`, nullish comparisons and values from void calls. For example:

```ts
export function main(): void {
  const u = undefined;
  console.log(u === null);                 // 0
  console.log(u == null);                  // 1
  console.log(JSON.stringify({u, n:1.5})); // {"n":1.5}
  console.log(JSON.stringify([u, u]));     // [null,null]
  console.log(JSON.stringify(u));          // undefined
}
```

Root `JSON.stringify(undefined)` returns an actual undefined value. Object fields
are omitted, array elements become null, and empty strings/null/zero keep their
distinct meanings. This profile requires host ABI v3/value ABI v4; earlier
profiles retain their behavior. Dynamic unions, missing-property values,
uninitialized locals and generic coercion remain pending; checked JSON cannot
parse an undefined target because JSON has no such value.

The following structured-value features work in either numeric profile, using
versioned field tags and JSON schemas. Default i32 behavior remains unchanged.

```ts
export function main(): void {
  const value = { count: 1, label: "ready", meta: { ok: true, empty: null } };
  const alias = value;
  alias.count = 2;
  console.log(JSON.stringify(value));
  // {"count":2,"label":"ready","meta":{"ok":true,"empty":null}}
}
```

Objects preserve identity, aliases, nested references and mutable typed fields.
Inline object annotations work across functions/modules; field order does not
change the type. Unreachable objects and their contained strings are collected.
`JSON.stringify(value)` emits compact JSON with bounded depth, traversal and
encoded bytes. Serialization is pure and needs no system permission.

These profiles have closed shapes with at most 32 fields, their selected numeric representation,
valid UTF-8 strings, booleans, nested objects/arrays and `null`. Optional fields,
dynamic keys/shapes, prototypes, methods and named object type aliases
are not supported yet. Undefined fields require the v2 binary64 profile above.
Checked JSON parsing is available below; dynamic parsing,
replacers and indentation remain pending.
In the default i32 profile, Tool Contract v1 accepts primitive signatures; a tool may build an
object internally and return its serialized string. See the
[structured value contract](docs/architecture.md#structured-values-and-json).

## Typed arrays

```ts
function total(values: number[]): number {
  let result = 0;
  for (let i = 0; i < values.length; i = i + 1) {
    result = result + values[i];
  }
  return result;
}
export function main(): void {
  const values = [1, 2, 3];
  const alias = values;
  alias[1] = 7;
  const empty: string[] = [];
  console.log(total(values)); // 11
  console.log(JSON.stringify({ values, empty })); // {"values":[1,7,3],"empty":[]}
}
```

Dense arrays have one exact element type and preserve identity across aliases,
functions and modules. Both `T[]` and `Array<T>` annotations work, including nested
arrays/objects and typed empty arrays. The initial profile has a fixed length of
at most 1024 elements: indexed replacement is supported; push/pop, resizing,
sparse arrays, tuples and mixed element types remain unsupported. An invalid
index traps instead of producing JavaScript `undefined` or creating a new property.
Indices must be exact nonnegative integers; binary64 indices are checked before
integer conversion. String-key coercion is not implemented. Values are
traced by the collector and serialize as ordinary JSON arrays. See the
[array contract](docs/architecture.md#dense-arrays).

## Checked JSON parsing

```ts
export function main(): void {
  const value = varta.json.parse<{ count: number; labels: string[] }>(
    '{"labels":["ready"],"count":1}'
  );
  value.count = value.count + 1;
  console.log(JSON.stringify(value)); // {"count":2,"labels":["ready"]}
}
```

The explicit supported type generates a real runtime schema. Parsing rejects
malformed JSON, duplicate/unknown/missing fields, wrong types and limit violations
before allocating guest values. It produces ordinary owned values, with the same
mutation, aliases, collection and cross-module behavior as literals. It needs no
system permission and preserves the last system status.

The initial checked profile accepts at most 64 KiB of encoded input, depth 64,
4096 values and 1024 elements per array. The default profile requires plain signed
i32 integer notation and rejects fractional/exponent/negative-zero forms. The
binary64 profile accepts finite JSON numbers and preserves negative zero. Object fields are
normalized to schema order (integer-index keys first, other keys in canonical name
order). This checked extension has no reviver, dynamic target, optional fields or
general `JSON.parse` API yet. Invalid input traps; named invocation reports
`execution_failed` without a partial result. See the
[parsing contract](docs/architecture.md#checked-json-parsing).

## HTTP server

```ts
function handle(request: VartaHttpRequest): VartaHttpResponse {
  return {
    status: 200,
    headers: [{ name: "content-type", value: "application/json" }],
    body: JSON.stringify({ ok: true, target: request.target })
  };
}
export function main(): void {}

varta.http.serve({ hostname: "127.0.0.1", port: 8080 }, handle);
```

Declare the exact listener in `varta.config.json`:

```json
"capabilities": { "net": { "listen": ["127.0.0.1:8080"] } }
```

`varta dev`, `varta run` and `build --single` artifacts start this service after
`main` completes. The CLI reports the listening address on stderr. Port 0 chooses
an OS-assigned port and requires an explicit `127.0.0.1:0` grant. Host policies must
also permit the same requested endpoint. Outbound grants never authorize binding.

The top-level declaration accepts literal IP/port options and one named handler,
including an imported function. HTTP publication is independent of AI tools/MCP.
Request records contain `method`, `target` (path/query), UTF-8 `body` and a header
array. Response records contain `status`, UTF-8 `body` and headers. The body above
is sent as ordinary JSON, without a tool envelope. Authentication, routes and
payload rules belong in the TS handler.

The first profile processes one connection at a time and closes it after one
response. Each request gets a fresh Wasm instance, 8 MiB memory, fuel and a
five-second execution budget. `main` is not rerun. A failed handler yields a generic
500 and subsequent requests can proceed. Bodies are limited to 8 KiB, headers to
32 / 4 KiB, targets to 1 KiB and encoded responses to 1 MiB. Streaming, keep-alive,
shared guest state, general async JS, TLS and HTTP/2 remain pending.

`varta.http.stop()` in the owning handler stops acceptance after the current reply
finishes. Embedders can use `run_observed` and the shutdown handle in
`RuntimeEvent::Listening`. The CLI streams bounded request logs; the collecting
runtime API retains at most 1 MiB of lifetime output. See the
[HTTP contract](docs/architecture.md#http-service-v1).

## Public tool contracts

Publish selected exported functions with a compile-time entry-module declaration:

```ts
export function echo(value: string): string {
  return value;
}
export function main(): void {}

varta.tool({
  name: "text.echo",
  description: "Return the supplied text"
}, echo);
```

Build normally, then read the catalog without running the application:

```sh
varta build --single
varta inspect dist/app.wasm --json
varta inspect dist/app --json
```

`toolCatalog` includes generated input/output schemas, required system operations
and limits. Metadata lives inside Wasm and survives native packaging and removal
of source files. Ordinary exports do not become tools. Imported functions may be
published through an explicit entry-module export. Parameters support `number`
(signed i32), `boolean` and `string`; results also support `void`. Output schemas
wrap the result as `{ "value": ... }`.

Catalog versions, sizes, handler signatures and transitive system requirements are
validated on check/build/dev/run/invoke/inspect/validate. Publishing a tool grants no
permissions. Generic application state and MCP remain planned; this example declares
a stateless tool. See the
[tool contract](docs/architecture.md#tool-contract-v1-and-offline-discovery).

## Invoke a tool

```sh
varta invoke dist/app.wasm text.echo --params '{"value":"Hello"}' --json
varta invoke dist/app text.echo --params '{"value":"Hello"}' --json
```

Both commands return one JSON record:

```json
{"status":"ok","result":{"value":"Hello"},"logs":[]}
```

Invocation runs only the published handler, with a fresh instance and the existing
memory, fuel, time and host-call limits. It does not call `main`. All parameters
are checked before guest execution: one object, exact required fields, no duplicate
or unknown fields, no coercion, signed i32 integers, booleans and UTF-8 strings.
The input is limited to 64 KiB, including JSON whitespace/escaping. Integer input
uses plain integer notation; decimal/exponent and negative-zero forms are rejected.
The encoded `{ "value": ... }` result is limited to 1 MiB; logs have their own
existing output limit. Strings are copied into host-owned results.

`--policy host-policy.json` also restricts invocation. Concrete system operations
still require both package and host grants. A handler can recover from denied
operations through `VartaResult`; that does not grant access. Boundary failures
return exit code 1 and, with `--json`, a stable `code` such as `invalid_arguments`,
`unknown_tool`, `invalid_result` or `execution_failed`. File/configuration errors
before dispatch keep the ordinary CLI error handling. Without `--json`, stdout
contains only `{ "value": ... }` and application logs go to stderr.

This local interface does not provide remote authentication, persistent application
state or rollback of host effects. Failed calls are not retried automatically.

## Diagnostics

`check`, `build` and `dev` report the actual source file and the failing source
range, including errors inside nested expressions:

```text
/path/to/app/src/main.ts:2:19: type mismatch for x: expected string, found number
  |   let x: string = 7;
  |                   ^
```

`varta check --json` returns structured `diagnostics` with category, filename,
line/column range, UTF-8 byte offsets and a bounded excerpt. Unicode, CRLF and BOM
positions are supported. See the [diagnostic contract](docs/architecture.md#source-diagnostics)
for coordinate conventions and public compiler APIs.

## Local modules

Split functions across files using explicit relative `.ts` paths:

```ts
// src/math.ts
export function twice(n: number): number { return n * 2; }

// src/main.ts
import { twice as double } from "./math.ts";
export function main(): void { console.log(double(21)); }
```

Named function imports/exports and local export aliases are supported. Private
functions keep their own file scope. The entry file declares `main(): void`;
dependencies are linked into the Wasm and native artifacts at build time.
`varta check --json` includes the resolved dependency graph in `modules`.

Dependencies must stay within the configuration directory; symlinks and cycles
are rejected. Default/namespace imports, export-from, dynamic imports, npm and
URL imports are not supported. See the [module contract](docs/architecture.md#local-modules)
for path rules and graph limits.

## Project and permissions

```text
varta.config.json
src/main.ts
varta.d.ts
dist/app.wasm
dist/app.manifest.json
dist/app                 # build --single
```

Example `varta.config.json`:

```json
{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": { "now": true },
    "env": { "get": ["APP_NAME"] },
    "fs": { "read": ["/data/**"] },
    "net": { "fetch": ["http://example.com/public/**"] }
  }
}
```

Omitted grants, empty lists and false deny access. Permissions for read/print,
get/has and fetch/get are independent. Paths retain directory boundaries and are
opened through pinned descriptors with symlinks rejected. Network authority is
exact; URLs are validated before I/O, with redirects and ambient proxies disabled.
Only the strict profile and HTTP transport are implemented.

`inspect --json` shows full permission scopes. Without a host policy the CLI uses
the package's declared grants. To restrict an untrusted package, supply a separate
host policy in the manifest format:

```json
{
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] }
  ]
}
```

```sh
varta run dist/app.wasm --policy host-policy.json
varta inspect dist/app.wasm --policy host-policy.json --json
# A newly built native launcher accepts the same option:
./dist/app --policy host-policy.json
```

An operation must satisfy both the package request and the host policy. Empty
host capabilities deny everything; malformed or missing policy files fail before
execution. The host cannot add unrequested rights, and the package cannot override
the host ceiling. Inspection shows both filters and distinguishes unconditional
denials from resource checks performed at runtime. `--policy` also works with
`dev` and `validate`; it is never saved into a build. Native executables themselves
run with the launching user's OS authority.

## Language and execution

The subset includes local named-function modules, typed primitive/object functions,
lexical let/const bindings, integer arithmetic, boolean operations, conditions,
loops and switch. Numbers use signed i32 semantics by default or binary64 with the
explicit profile above. Strings work as variables,
parameters, explicit function results and dynamic host inputs:

```ts
function read(name: string): string { return varta.env.get(name); }
export function main(): void {
  const name = read("APP_NAME");
  const other = read("OTHER_NAME");
  console.log(name); // Still holds the first result.
  console.log(other);
}
```

Host results are copied into immutable, bounded storage. Unreachable strings are
collected and their storage is reused during execution; aliases and saved results
remain valid. Every read still needs the corresponding capability. Strings support `+`,
`==`, `!=`, `===`, `!==`, `<`, `<=`, `>` and `>=`:

```ts
export function main(): void {
  const message = "Привіт" + " 🦀";
  console.log(message == "Привіт 🦀"); // 1
  console.log("apple" < "banana");   // 1
}
```

Comparisons use content and UTF-16 code-unit order, without locale sorting or
normalization. Both operands must be strings; numbers are not converted to text.
One string is limited to 1 MiB of stored bytes (UTF-8 or UTF-16LE for the selected
profile). Allocations share the memory/fuel
budgets. Unused expression values, overwritten bindings and out-of-scope locals
can be reclaimed. Live values stay in place; committed memory pages remain until
the run ends. See the [reclamation contract](docs/architecture.md#string-memory-reclamation).
See the [operator contract](docs/architecture.md#string-operators) and
[ownership contract](docs/architecture.md#string-values-and-ownership).
Unsupported constructs, top-level effects and type mismatches fail
explicitly. There is no general Node/npm compatibility or async execution yet.

Runtime defaults include 8 MiB guest memory, instruction fuel, a 5-second
execution/HTTP deadline, and bounded host calls, I/O results and output. Host
failures clear result lengths and expose status through `varta.result.status()`:
0 success, 1 denied, 2 invalid input, 3 I/O failure, 4 result too large.

## Programmatic host errors

Capture a direct system call to keep its value and status together:

```ts
export function main(): void {
  const result = varta.result.capture(varta.env.get("APP_NAME"));
  if (result.ok) {
    console.log(result.value);
  } else {
    console.log(result.status);
  }
}
```

The readonly `ok`, `status` and `value` fields survive later system calls. Results
can be passed or returned as `VartaResult<string>`, `VartaResult<number>`,
`VartaResult<boolean>` or `VartaResult<void>`. Capture the system call inside a
user wrapper before returning it. Existing permission checks still apply, and
resource exhaustion still ends execution. The old API and status inspection stay
available. See the [result contract](docs/architecture.md#programmatic-host-results).

## Commands

- `init <name>`: create a project without overwriting an existing directory
- `check [--config] [--json]`: validate language, types, config and generated Wasm
- `dev [--config] [--policy]`: check, compile and execute
- `build [--config] [--single] [--launcher PATH]`: produce validated Wasm and an optional runtime-only executable; `--launcher` requires `--single`
- `run <path> [--policy]`: execute portable Wasm or a packaged artifact
- `invoke <path> <name> [--params JSON] [--json] [--policy]`: call one published tool without running `main`
- `inspect [path] [--json] [--policy]`: show validated code and full capability metadata
- `validate [path] [--json] [--policy]`: check code, ABI, manifest and import declarations

Failures return nonzero exit codes. `validate` adds policy review: every privileged
import needs a declaration, even though omitted permissions remain safe to execute
because the runtime denies them.

Native packaging now uses a fixed VARTA3 footer and portable Wasm. Legacy VARTA1/
VARTA2 and native AOT payloads are rejected; rebuild old artifacts. This avoids
accepting untrusted machine code through Wasmtime deserialization.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, scope and contribution guidelines.

The workspace separates common schemas/ABI, compiler, runtime, artifact I/O,
runtime launcher, project services and CLI. Workspace code forbids unsafe. Run:

```sh
cargo fmt --all -- --check
cargo check -p varta-common -p varta-compiler -p varta-runtime -p varta-artifact --lib --locked
cargo build -p varta-launcher --bin varta-runtime --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p varta --test smoke --locked
```
