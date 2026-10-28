# Varta architecture and implemented contract

This document describes the implemented MVP. Future work is tracked separately in
[development-plan.md](development-plan.md). Varta compiles a deliberately small
TypeScript subset to portable WebAssembly and executes it in a capability-checked
Wasmtime host. It does not provide general JavaScript, Node or npm compatibility.
The [2026-10-27 development checkpoint](development-plan.md#development-checkpoint--2026-10-27)
records the current delivery state and local verification; the contracts below
remain the source of truth for individual features and profile boundaries.

Primitive Tool Contract v1, offline discovery and checked JSON invocation are
implemented below. Broader Application Contracts, live application state and
MCP remain **planned** in the [development plan](development-plan.md#product-direction).
`inspect` reports bytecode, capabilities and an optional public tool catalog.

## Boundaries

Varta supplies application-independent mechanisms. Domain entities, state schemas,
business commands, validation rules and domain permissions belong to separate
TS/JS applications. Reference editor/server/robot scenarios are consumers of the
engine, not built-in application models. Domain fixtures in tests do not define
public engine APIs. Generic state and transaction support remains planned in the
development plan; the current checked invocation API is stateless.

Generic JSON, networking and event-loop primitives belong to the engine. A TS/JS
application should eventually be able to serve JSON directly through Varta APIs;
per-application host wrappers are not the target architecture. Closed records and
bounded JSON serialization and the initial HTTP/1 service/lifecycle profile are
implemented, including runtime-only packaging. Generic shared state and MCP
remain later work.

```text
CLI -> varta-project -> varta-compiler -> validated portable Wasm
                    -> varta-runtime -> checked host adapters
                    -> varta-artifact -> portable container reader/writer
varta-runtime (binary in varta-launcher) -> varta-artifact + varta-runtime (library)
          all layers -> varta-common (schema, ABI, host operation registry)
```

- `varta-common`: strict artifact/contract schemas, operation identities, host
  signatures, ABI constants, diagnostics and editor declarations.
- `varta-compiler`: bounded module graph, syntax parsing, lexical name resolution, subset/type checking,
  typed intermediate representation, Wasm emission and full bytecode validation.
- `varta-runtime`: bounded module validation, Wasmtime execution, resource limits,
  pinned filesystem capabilities, HTTP transport and host result handling.
- `varta-artifact`: bounded portable artifact/container I/O and atomic writes;
  common types and wasmparser for host-ABI/manifest consistency, without a VM or compiler.
- `varta-launcher`: execution-only native companion (`varta-runtime` binary);
  depends on common, artifact and runtime, without project/compiler/SWC/clap.
- `varta-project`: development tooling, project configuration, confined source loading
  and one checked pipeline for check/dev/build; verified launcher selection,
  policy review and inspection.
- `varta-cli`: argument parsing, presentation and process exit codes.

The common/compiler/runtime libraries form the engine; artifact/launcher supply
its independent distribution layer; project/CLI are development tooling.
No engine or distribution crate depends on tooling or CLI, including in tests.
Runtime depends on compiler only for
test fixtures, not in production. CLI calls project tooling rather than runtime
directly. Project inspection consumes `WasmInspection`/`WasmImport` values from
`VartaRuntime::inspect_module` and has no direct Wasmtime dependency. Advanced
runtime validation APIs still return backend modules for specialized embedders.

Project configuration types `VartaConfig` and `CapabilitySpec` are exported by
`varta-project`; callers migrating from `varta-common` must update those imports.
The on-disk configuration, manifest, host ABI and artifact formats are unchanged.
Numeric port-list grants remain rejected. Implemented net.listen permissions use
canonical literal IP:port endpoints, with an exact operation-specific grant type. The compiler's empty-module
fixture helper is not a production API; its regression fixture lives in tests.

A dependency-graph test guards the permitted crate directions. An external-style
integration test compiles/inspects/invokes using public engine APIs, without project
files or CLI. CI separately checks core libraries and builds the runtime companion
without project tooling. Tests and their
small source fixtures remain part of engine verification, not shipped applications.

Workspace code forbids `unsafe`. New dependencies are exact-pinned: `cap-std` and
`cap-fs-ext` supply descriptor-relative, no-follow filesystem operations;
`url` supplies one structured URL interpretation; `ureq` supplies bounded HTTP
framing and timeouts. Implementing these protocols with ambient path checks and
handwritten HTTP caused the audit's permission bypasses. Inbound framing uses
exact-pinned Hyper 1.12.0, hyper-util 0.1.21, http-body-util 0.1.5 and Tokio 1.54.1
with server/HTTP1 and bounded I/O/runtime features. These dependencies provide
maintained protocol parsing, body framing and timers, instead of a new handwritten
server parser. See [Hyper's HTTP1 builder](https://docs.rs/hyper/1.12.0/hyper/server/conn/http1/struct.Builder.html)
and [Tokio runtime bounds](https://docs.rs/tokio/1.54.1/tokio/runtime/struct.Builder.html).

## Compilation contract

Every project command loads and validates `varta.config.json`, resolves the entry
and output relative to that configuration file, checks the supported language and
its types, emits Wasm, and validates both bytecode and host ABI. Deno is not a
prerequisite and its presence cannot change whether a project is accepted.

The pipeline is:

```text
bounded source graph -> per-file SWC AST -> linked, typed IR -> Wasm -> validation
```

The semantic stage assigns a distinct local slot to each lexical binding and
checks parameters, assignments, calls, returns, const writes and uses before
initialization. Function signature interning assigns and emits each signature in
one operation, independent of source declaration order. Branch labels account for
every enclosing Wasm block, loop and if. Each nested switch has distinct temporary
locals; default selection and fallthrough follow source order.

Baseline supported syntax (`checked-i32-utf8-v1`; opt-in extensions follow):

- local source modules with a zero-argument `main(): void` in the entry file;
- synchronous named functions with primitive, `null` and closed-record parameters/results;
- closed object literals, dense homogeneous arrays, nested references, typed field/index reads/writes, `JSON.stringify` and `varta.json.parse<T>`;
- immutable `VartaResult<T>` values with typed `ok`, `status` and `value` fields;
- local `let`/`const` value bindings with initializers and block scoping;
- integer arithmetic, comparisons, boolean literals, `!`, `&&` and `||`;
- if/else, while, for, switch, unlabelled break/continue, return;
- UTF-8 string literals and retained host results, including dynamic host arguments;
- string concatenation with `+` and content equality/ordering comparisons;
- type-only interfaces/aliases and built-in declarations.

Parameters require explicit primitive, inline object or `VartaResult<T>` annotations. Numeric/void function
results may be inferred from the supported return forms; explicit result
annotations are recommended. Value-returning functions must return on every
supported control-flow path. Case-local declarations require an explicit block.
String-returning functions require explicit `: string` result annotations;
result-returning functions require `: VartaResult<T>`; object/null returns also
require explicit annotations. Value types may be inferred from local initializers. Assignment, parameters and
returns require exact types; const and use-before-initialization rules also apply
to strings. Conditions, numeric arithmetic and switch remain number/boolean operations.
String indexing, properties, interpolation, implicit coercion, compound assignment
and string truthiness are not yet implemented and fail explicitly.

Numbers have signed **i32** Wasm semantics: addition/subtraction/multiplication
wrap, division truncates, and invalid division traps. This is intentionally not
JavaScript's floating-point `number` semantics. Booleans log as 0/1. `time.now`
returns Unix seconds representable in i32 and reports invalid input outside that
range rather than wrapping.

Unsupported import/export forms, top-level effects, async/generators, classes, closures, var,
sparse/resizable arrays, dynamic objects, casts, any and unsupported syntax fail
explicitly. They are never silently discarded. Source is capped at 1 MiB with
lexical complexity limits before recursive parsing. Static strings occupy at most
one 64 KiB page shared with object descriptors. Parse errors retain source line/column positions.

### Language profiles and migration

Source semantics and physical host/value ABIs have separate version identities.
Every newly compiled module has one bounded `varta.language` custom section:

```json
{"schemaVersion":1,"profile":"checked-i32-utf8-v1"}
```

`varta-common::language` owns the strict schema, profile registry and typed errors.
Metadata is capped at 1 KiB. Unknown profiles/fields, missing/duplicate JSON fields,
unsupported schema versions, duplicate language sections and duplicate/invalid
host ABI sections fail before Wasmtime compilation or guest instantiation. An
explicit language profile requires exactly one matching host ABI section. All runtime
entry points, including offline inspection and tool invocation, use that check.
Invocation reports `invalid_artifact` for an invalid language profile.

`WasmInspection.language` and development CLI inspection's top-level `language`
report the supported profile and its origin:

| Input metadata | Report and behavior |
| --- | --- |
| Explicit supported language section + ABI v1 | `profile:"checked-i32-utf8-v1", origin:"declared"` |
| Explicit binary64 scalar profile + ABI v2 | `profile:"checked-f64-utf8-v1", origin:"declared"` |
| Explicit undefined-capable profile + ABI v3 | `profile:"checked-f64-utf8-v2", origin:"declared"` |
| Explicit UTF-16 profile + ABI v4 | `profile:"checked-f64-utf16-v1", origin:"declared"` |
| Explicit primitive-union profile + ABI v5 | `profile:"checked-f64-utf16-v2", origin:"declared"` |
| Explicit missing-value profile + ABI v6 | `profile:"checked-f64-utf16-v3", origin:"declared"` |
| Explicit reference-union profile + ABI v7 | `profile:"checked-f64-utf16-v4", origin:"declared"` |
| Explicit checked-JSON profile + ABI v8 | `profile:"checked-f64-utf16-v5", origin:"declared"` |
| Explicit resizable-array profile + ABI v9 | `profile:"checked-f64-utf16-v6", origin:"declared"` |
| Explicit primitive-exception profile + ABI v10 | `profile:"checked-f64-utf16-v7", origin:"declared"` |
| Explicit Error-object profile + ABI v11 | `profile:"checked-f64-utf16-v8", origin:"declared"` |
| Explicit native-error-family profile + ABI v12 | `profile:"checked-f64-utf16-v9", origin:"declared"` |
| Explicit recoverable-JSON profile + ABI v13 | `profile:"checked-f64-utf16-v10", origin:"declared"` |
| Explicit function-value profile + ABI v14 | `profile:"checked-f64-utf16-v11", origin:"declared"` |
| Explicit lexical-closure profile + ABI v15 | `profile:"checked-f64-utf16-v12", origin:"declared"` |
| Explicit sparse-array profile + ABI v16 | `profile:"checked-f64-utf16-v13", origin:"declared"` |
| Explicit array-mutation profile + ABI v17 | `profile:"checked-f64-utf16-v14", origin:"declared"` |
| ABI v1 without a language section (older Varta output) | Same profile, `origin:"legacyHostAbi"`; existing behavior is preserved |
| Neither section (generic portable Wasm) | `profile:null, origin:"unspecified"`; no claim about source-language semantics |
| Unknown/invalid/duplicate profile or ABI | Reject; no fallback to the current profile |
| ABI v2/v3/v4/v5/v6/v7/v8/v9/v10/v11/v12/v13/v14/v15/v16/v17 without an explicit matching profile | Reject; only ABI v1 has a legacy fallback |

The default i32 profile describes the existing checked subset, not full JavaScript. Arithmetic
wraps/truncates/traps as documented above. `-0` becomes integer zero. Floating-point
values and global `undefined`/`NaN`/`Infinity` are unavailable; a local binding with
one of those names remains an ordinary binding. `void` functions have no value,
not a general undefined value. Same-type `==`/`===` compare the same supported
values; mixed-type operators fail type checking. Strings compare by content and
UTF-16 ordering over valid UTF-8; objects/arrays compare by identity. Existing
record ordering, dense array bounds, strict checked JSON and host result rules
remain unchanged. Numeric literal acceptance is based on the parsed numeric value:
an integer-valued source spelling may be accepted even when checked JSON rejects
that spelling (for example, `1.0`). The positive literal operand of unary minus
must itself fit i32; write the minimum via arithmetic or checked JSON instead.

The declaration is validated compatibility metadata, not proof of compiler/source
provenance and not authorization. Arbitrary Wasm can claim a profile; permission,
memory and ABI checks remain independent. Removing all custom sections cannot
make unsafe native payloads acceptable or grant system access. VARTA3/footer,
manifest, host ABI v1, structured values v1/v2, Tool Contract v1 and HTTP v1 keep
their meanings; this change adds metadata without reinterpreting payload slots.

The future ECMAScript profile is specified in the
[K4 migration contract](development-plan.md#k4--ecmascript-value-profile-and-language-expansion).
It is not accepted by this compiler/runtime. A semantics-breaking profile must use
a new mandatory host ABI as well as explicit language metadata: old runtimes can
ignore unknown custom sections, but already reject a future host ABI version.
Missing future-profile metadata must fail, never inherit the legacy fallback.

### Binary64 profile and host ABI v2

The opt-in `checked-f64-utf8-v1` profile implements the first numeric increment of
K4.2. Select it with `"languageProfile":"checked-f64-utf8-v1"` in project config,
or `compile_typescript_with_profile`/`compile_modules_with_profile` in the compiler
API. Omission still selects `checked-i32-utf8-v1`. The whole source graph has one
profile; check/dev/build use the same selection and diagnostics. It is a checked
profile with closed typed aggregates, not the reserved complete `ecmascript-2025-v1` profile.

The physical representation is intentionally explicit:

| Source/internal value | Wasm representation in ABI v2 |
| --- | --- |
| `number` literals, parameters, locals and returns | `f64` scalar, with IEEE-754 binary64 rounding and signed zero |
| `boolean`, `null` and internal control indices | `i32`; null remains a separate static type |
| UTF-8 string handle | Existing immutable packed `i64` slice with exact roots |
| `VartaResult<number>` | `(f64 payload, i32 status)` |
| Other supported captured host results | Existing payload representation plus `i32 status` |

There is no NaN boxing or untyped numeric heap slot. Floating values never enter
the exact root set; string handles remain typed roots. The float scratch local is
distinct from control/status and string temporaries. Tagged layouts need their own checked ABI; the UTF-16 increment uses the separate
ABI v4 described below. No existing descriptor/tag is redefined.
GC stress tests preserve signed zero/NaN and a live string alias through collection.

Finite/fractional/exponent/large source literals, `NaN`/`Infinity`, unary +/- and
numeric +, -, *, /, %, comparisons, !, &&/||, conditions and switch use binary64
semantics. Globals can be shadowed by lexical bindings; TDZ checks still apply.
Functions/modules preserve f64 signatures. Wasm arithmetic/comparisons implement
rounding, non-finite results and signed zero. Numeric truthiness uses `abs(x)>0`,
false for both zeroes and NaN; short-circuiting evaluates each operand once and
returns the selected value. Mixed-type coercion is still rejected. The semantic
reference is [ECMAScript Number operations](https://tc39.es/ecma262/2025/multipage/ecmascript-data-types-and-values.html#sec-ecmascript-language-types-number-type),
implemented using [Wasm floating operations](https://webassembly.github.io/spec/core/exec/numerics.html#floating-point-operations).

`HostOperation::scalar_signature` is the shared physical signature registry;
`signature` derives counts from it. Host ABI v2 adds these internal imports:

| Import | Signature | Behavior |
| --- | --- | --- |
| `log_f64` | `(f64) -> ()` | Bounded diagnostic output, preserving negative zero and naming NaN/Infinity |
| `number_remainder` | `(f64,f64) -> f64` | Truncating floating remainder using Rust's `%`, including zero, subnormal and non-finite cases |
| `process_exit_f64` | `(f64) -> ()` | Same `process.exit` authority; accepts only exact finite signed-i32 codes |

These are ABI imports, not new source APIs. Each consumes the host-call budget;
logging and remainder preserve system status and use no capability. Wasm has no
floating remainder opcode; the intrinsic avoids an inaccurate divide/truncate
formula when the quotient overflows. See [Rust f64 remainder](https://doc.rust-lang.org/std/primitive.f64.html#impl-Rem-for-f64).
Diagnostic finite formatting uses Rust Display, not a new implementation of
ECMAScript Number::toString or Node console formatting. `time.now`, `net.fetch`
and system status retain their exact i32 host values and are widened to f64 in
guest expressions; clock units and error meanings do not change. Exit checks
package/host authority first, then rejects fractional/non-finite/out-of-range codes
with invalid-input status, without trapping or truncating. Authorized valid exit
still ends execution. The old i32 host imports keep their signatures.

Validation requires language/profile and ABI v2 agreement, typed import signatures
and minimum host-ABI availability. Removing language metadata, downgrading the ABI
or mixing old structured-value/JSON/tool/HTTP metadata is rejected. Artifact I/O
also requires manifest ABI to match portable code. Missing Wasm sidecars still
mean zero grants, deriving only the ABI version from the payload header (ABI v1
when unspecified). Invalid artifact failures retain the `invalid_artifact` JSON
invocation code even when rejection occurs during loading.

Typed arrays/records and JSON now use value ABI v3 below. Tool Contract v1 and
HTTP v1 still require future adapters for this profile. UTF-16, undefined,
dynamic values and coercion remain pending. The default profile keeps
all its existing aggregate/JSON/tool/HTTP features and i32 behavior. ABI v1/v2,
the scalar profile names and legacy value layouts now remain distinct frozen
contracts; a future breaking UTF-16/tagged profile must negotiate a new mandatory
ABI so these runtimes reject it before effects. Packaging protocol v2's supported
profile list includes all seventeen checked profiles; update companion and CLI together.

### Undefined profile and host ABI v3

`checked-f64-utf8-v2` is a separate opt-in profile with **host ABI v3/value ABI v4**.
Earlier profiles retain their source and physical contracts. Undefined has its
own static type and descriptor/array **tag 7**. Its canonical payload is zero in
an i32 scalar/eight-byte aggregate slot; the tag distinguishes it from null,
false and zero. It is never a reference. Runtime boundaries reject nonzero
undefined payloads even when the corresponding JSON field would be omitted.

The compiler supports the global `undefined` value, explicit `: undefined`
annotations, parameters/returns, object fields and homogeneous undefined arrays.
Lexical bindings may shadow the global and preserve use-before-initialization
checks. `void expression` evaluates its operand once, discards all physical slots
and produces undefined. Guest/host void calls likewise produce undefined in an
expression; their physical no-result signatures remain unchanged. Explicit void
functions may return undefined (discarded before returning); undefined-returning
functions may return bare or fall through to undefined. Main remains `()->()`.
Captured `VartaResult<void>.value` is undefined; status/permission checks are
unchanged. `log_undefined(i32)->()` validates zero, preserves status and captures
bounded diagnostic output, with no capability.

Null and undefined are falsy. Supported typed `typeof` expressions return the
corresponding strings while evaluating operands once (`typeof null` is object).
Comparisons involving nullish values distinguish strict and loose equality:
null == undefined is true, null === undefined is false, and either is unequal
to other primitive/reference types. Strict comparisons of different supported
static value types are false. Both operands still run in source order. With a
statically nullish left operand, || evaluates/returns its right operand and &&
skips it. General coercion, runtime unions, missing-property/out-of-range values,
uninitialized locals and typeof undeclared identifiers remain outside this
checked increment. The reference is the pinned
[void/typeof operators](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-void-operator)
and [equality operations](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-islooselyequal).

`json_stringify_v4(i32 kind,i32 low,i32 high)->i32` binds value ABI v4. Ordinary
values use the existing bounded text/result-length/owned-snapshot convention.
An undefined root returns canonical zero and clears result length, preserving
system status. Since JSON text cannot be empty, this is distinct from every
successful text result. Intrinsic typing returns Undefined for a statically
undefined input and String for the other admitted value types; no fake empty
string or implicit union conversion is introduced. Object fields with undefined
values are omitted, undefined array entries emit null, and normal null values
still emit null. All keys/slots/payloads are validated, including omitted fields;
omitted fields count toward depth/node limits and preserve constructor effects.
See the [pinned JSON rules](https://tc39.es/ecma262/2025/multipage/structured-data.html#sec-json.stringify).

`json_parse_v4(i32,i32,i32,i32)->i64` requires **schema v3/descriptor v4** and the
same finite Number, closed-shape, complete-validation-before-allocation and
host-free helper contracts. JSON has no undefined value, so undefined targets,
including nested fields/elements, are rejected at compile time; missing fields
remain errors. Legacy imports/schemas/descriptors cannot be substituted. New
imports require host ABI v3, and missing or incompatible profile metadata fails
before instantiation. Launcher maxima are 7/8; default i32 and the v1 binary64
profile continue using their existing versions.

### UTF-16 profile and host ABI v4

`checked-f64-utf16-v1` selects **host ABI v4/value ABI v5/JSON schema v4**.
It inherits binary64/undefined behavior while changing string payloads to UTF-16LE.
The immutable i64 slice handle still packs a byte pointer in the low 32 bits and
**byte length** in the high 32 bits. Length must be even, but pointers need not
be aligned. All u16 values, including lone surrogates and NUL, are admitted.
Literals retain SWC's code units without a lossy Unicode conversion. The compiler
interns actual encoded bytes under the existing 64 KiB static-data budget.
Record keys/descriptors/schema names retain their explicitly validated UTF-8
metadata encoding; surrogate object keys remain unsupported in this closed profile.

Concatenation copies code units verbatim, so separately created leading/trailing
surrogates can form a pair. Comparison loads unsigned little-endian u16 units in
lexical order, without normalization, locale conversion or handle comparison.
`.length` returns the code-unit count as Number. `.charAt(position?)` produces a
one-unit String (possibly a lone surrogate), or empty String out of range.
`.charCodeAt(position?)` returns the unit Number, or NaN out of range. The receiver
and optional numeric position run once in source order. Missing/NaN positions
become zero, fractional positions truncate toward zero, and infinities are out
of range. Other argument types and string bracket access remain rejected rather
than approximating dynamic unions or coercion. The pinned reference is
[ECMAScript 2025 String methods](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.charat).

New pure imports are `log_string16(i32,i32)->()`,
`string_char_at16(i32,i32,f64)->i32` and
`string_char_code_at16(i32,i32,f64)->f64`. Ranges, even byte lengths, 1 MiB string
bytes and execution/output/host-call budgets are enforced. charAt borrows the
existing host buffer/result-length convention; the compiler immediately snapshots
its UTF-16 bytes into the same traced heap. Empty charAt still clears result
length. Pure operations preserve the last system status. Diagnostic logs replace
unpaired units with U+FFFD; this is display-only and cannot authorize a resource.
Use JSON to inspect exact code units.

Encoding-specific privileged imports are `env_has16`, `env_get16`, `env_print16`,
`fs_read16`, `fs_print16`, `net_get16` and `net_fetch16`, with the corresponding
legacy signatures and **same exact capability IDs**. Inputs have both a 64 KiB
stored-byte and UTF-8-conversion limit. Resource conversion rejects lone surrogates,
odd byte lengths and NUL before policy checks or OS calls. It performs no lossy
replacement or normalization. Package/host scope conjunction, pinned filesystem
access and URL validation remain unchanged. UTF-8 host results convert to UTF-16;
invalid UTF-8 fails, and stored results must still fit 1 MiB. Binary I/O and native
non-Unicode resource names need separate APIs. No new capability kind is added.
The runtime rejects legacy string imports under this profile; previous profiles
cannot import these ABI v4 operations.

`json_stringify_v5` / `json_parse_v5` retain their v4 physical signatures but bind
value v5/schema v4 exactly. JSON string values encode paired surrogates as Unicode
scalars and unpaired units as lowercase `\uxxxx`, including nested fields/arrays.
The output becomes an owned UTF-16 string; existing undefined behavior remains.
See [QuoteJSONString](https://tc39.es/ecma262/2025/multipage/structured-data.html#sec-quotejsonstring).
The strict parser checks the physical UTF-16 input before converting to UTF-8
JSON transport: raw lone units in unescaped string-token positions are escaped
losslessly; invalid escapes or units outside strings are rejected. Both physical
and converted input have a 64 KiB cap. serde_json's byte-string visitor preserves
escaped lone surrogates; ordinary transport UTF-8 is validated first. Complete
schema/key/duplicate/traversal validation still precedes guest allocation. Parsed
strings are owned UTF-16 buffers, with exact roots across allocator callbacks.
JSON UTF-8 output and converted UTF-16 output each have their existing 1 MiB cap.

All earlier ABI/profile/layout combinations retain their interpretation. Explicit
profile/ABI agreement, versioned JSON imports/descriptors and launcher payload
probing prevent downgrades before execution. Runtime-only/source-free artifacts
use the same path. Tests cover every u16 round trip, scalar/surrogate ordering,
method edge cases/effect order, nested JSON, malformed encodings/versions,
collection pressure, permission denials, real outbound loopback HTTP, public
engine APIs and native packaging. Tool/HTTP publication in this profile, dynamic
keys/values, unions, string coercion and additional String methods remain pending.

### Primitive unions and host ABI v5

`checked-f64-utf16-v2` uses **host ABI v5/value ABI v6/JSON schema v5**. It retains
the UTF-16/binary64/undefined behavior and admits closed unions of Number, String,
Boolean, Null and Undefined. Members are canonicalized, flattened and deduplicated;
one-member unions collapse to their member. At most 16 syntactic members and 128
interned union types are accepted. No object/array member, any/unknown, assertion
or implicit conversion is introduced. All earlier profiles reject union annotations.

A union uses one **i64 managed handle**, with a 16-byte immutable payload:
`u32 member_kind; u32 count=1; u64 member_payload`. The handle's high word is
16 bytes, low word is an aligned pointer into the owned heap. The ordinary
16-byte block header has `ALLOCATED | HEAP_UNION_FLAG(16)` and a link/reference
mask equal to 1 only for a String member. The unused fourth header word has no
semantic content. `PrimitiveUnion = 8` is a new reference tag supported only in
value v6. Record/array slots remain eight bytes and carry a union-cell handle.
Every primitive cell is separately allocated; aliases copy its handle and
assignments replace handles. There is no observable cell identity or mutation.

Numbers store raw f64 bits, booleans require 0/1, null/undefined require zero,
and strings retain the UTF-16 pointer/byte-length handle. The shared
`PrimitiveBox::read` validates cell size/alignment/range, allocated kind, count,
member tag, scalar canonicality, reference mask and bounded even string ranges.
Cells cannot contain another cell/object/array. Numeric bits never become GC
edges, including NaNs or payloads resembling pointers. The collector traces
union blocks as a single masked child slot alongside records/arrays. Compiler
roots protect the member before allocating its cell, protect cells across calls
and nested allocation, and retain extracted strings across collection. Heap,
root-stack, fuel, deadline and host-call limits remain unchanged.

The compiler widens a supported primitive or a subset union into an expected
union for local initialization/reassignment, parameters, returns, record fields
and explicitly typed array elements. It rejects narrowing without a guard and
structural aggregate conversion. Mixed primitive ternaries infer a union;
identical ordinary value branches retain their physical type. Direct-local
`typeof x ===/!== "number"/"string"/"boolean"/"object"/"undefined"`, `x ===/!== null`
and strict global-undefined comparisons narrow the matching/opposite branch of
an if/ternary, including reversed operands. Here typeof object can only select
Null, because object union members are not admitted. Shadowed undefined stays an
ordinary binding. Guard refinements are keyed by resolved local slots, cannot
escape their branch and do not change the slot's physical type. Extracting a
primitive checks the runtime member tag before loading its payload. Writes to a
refined binding are rejected, preventing stale assumptions in loops; broad flow
analysis and post-return narrowing remain pending. See the
[TypeScript narrowing reference](https://www.typescriptlang.org/docs/handbook/2/narrowing.html).

New pure typed imports are `union_log(i64)->()`, `union_typeof(i64)->i32`,
`union_truthy(i64)->i32`, `union_nullish(i64)->i32`,
`union_strict_equal(i64,i64)->i32` and `union_nullish_equal(i64,i64)->i32`.
All validate canonical cells and preserve system status. typeof returns borrowed
UTF-16 text through result length and immediate owned snapshot. Numeric/boolean
logs retain the existing diagnostic formatting; strings use diagnostic U+FFFD
replacement for lone units. Truthiness distinguishes empty strings, false,
null/undefined, signed zero and NaN. Strict equality compares actual member types,
then f64 numeric equality or UTF-16 content (never cell handles). NaN remains
unequal to itself and +/-0 equal. Union ==/!= is admitted only with a statically
nullish operand, with no other coercion. Both operands evaluate once, in order.
Union &&/||/?? returns the selected boxed operand and skips the other when
appropriate; ?? tests only null/undefined. General string/coercive operators are
rejected until the value is appropriately narrowed. The reference is the pinned
[ECMAScript abstract operations](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-isstrictlyequal).

`json_stringify_v6` and `json_parse_v6` retain their predecessor signatures but
bind value v6/schema v5. Serialization validates and unwraps primitive cells,
including omitted undefined fields, and counts logical values rather than box
implementation nodes. Undefined record fields are omitted, array entries become
null and a root undefined result has zero result length. An undefined-capable
union root therefore has source type String|Undefined; the compiler snapshots
text and boxes the actual result before another call can reuse its buffer.
Strict typed JSON parsing still accepts only supported non-union targets, with
UTF-16 strings and complete validation before guest allocation. Union targets and
nested union fields/elements are explicitly rejected. No ordinary JSON.parse or
mixed JSON parser is claimed.

Explicit profile/host/value/schema/import agreement is validated before effects;
union intrinsics require a value ABI section even without JSON. All earlier
profiles/layouts remain frozen. Public engine APIs, real check/dev/build, independent
runtime, source-free native artifacts, malformed payload/version rejection,
permission/host ceilings and collection pressure are regression tested. Dynamic
object/array unions, missing values, optional/uninitialized bindings, general
coercion, broader narrowing and new-profile tool/HTTP adapters remain next work.

### Missing values and host ABI v6

`checked-f64-utf16-v3` selects **host ABI v6/value ABI v7/JSON schema v6**.
The earlier source profiles retain their rejection/trapping rules. Closed record
annotations admit optional **primitive/primitive-union** fields: x?:T has value
type T|Undefined, while independently retaining the own-property presence bit.
Absent optional initializer fields receive canonical undefined slot values but
are not own properties. An explicit x:undefined is present. Required fields,
unknown keys, duplicate fields, readonly properties and optional reference types
remain checked/rejected. Optional names inherited from Object.prototype are
rejected because absent access would require prototype values. Required own
fields with such names still shadow the prototype normally. Structural conversion
of pre-existing aggregate aliases is not introduced.

Value v7 retains descriptor/key/count and eight-byte slot offsets. The static
16-byte descriptor entry stores `OPTIONAL_FIELD_FLAG = 1<<31` in the slot word;
its remaining bits identify the canonical slot. Every v7 record appends a trailer
after its n slots: `u32 presence; u32 order[n]`. Total payload size is **12+12*n**
bytes (including the original eight-byte metadata), including for zero fields.
The presence mask uses all 32 bits. The first popcount(presence) order entries
are unique present slot IDs; unused order entries must be zero. The heap header
retains the exact reference mask and ordinary record kind flag. Reserved absent
slots remain traced according to their static type; numeric bits and the trailer
are never reference edges. The collector therefore uses the same bounded slot
scan without interpreting presence/order words as handles.

Constructor operands run once in source order before creating absent placeholders.
Own integer keys enumerate first in numeric order; remaining names retain source
insertion order. Assigning an absent declared field stores its typed value, then
appends its slot ID and sets its presence bit. Both steps occur only after RHS
success. Reassignment, including undefined, preserves the original insertion
position. No deletion, sparse object shape, arbitrary property creation, getters,
prototype mutation or rollback of RHS system effects is provided.

`Object.hasOwn(record, literalKey)` is a checked compiler intrinsic reading the
presence mask, not another host permission or MCP entry point. Unknown literal
keys return false while evaluating the receiver once. Shadowed Object bindings
cannot call the intrinsic. General key coercion, dynamic keys and non-record
receivers are rejected in this profile; the v12+ dynamic record own-key contract
below adds checked String lookup separately. See the pinned
[Object.hasOwn](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-object.hasown)
and [optional property](https://www.typescriptlang.org/docs/handbook/2/objects.html#optional-properties)
references. Reading a statically absent record key returns real Undefined after
evaluating the receiver. Known inherited names require prototype lookup and are
rejected, rather than returning a false undefined. Assignments to unknown keys
remain rejected. A direct-local guard is still required before consuming a
union field as a specific primitive; hasOwn alone does not exclude undefined.

Primitive-array numeric reads now have type element|Undefined; string bracket
reads have type String|Undefined. Base and index expressions run once, left to
right, with exact roots across nested calls/collection. Before address conversion,
checks require a finite, exact, nonnegative index below the actual length.
NaN, infinities, negative/fractional and excessive numbers return undefined;
-0 selects zero. In-range primitive payloads preserve full Number bits or the
existing immutable union handle. UTF-16 string indexing returns exactly one unit,
including lone surrogates, through the existing charAt/snapshot/cell machinery.
Out-of-range indexing differs from charAt's empty-string result. There is no
property-key coercion or prototype/negative-index property lookup. Indexed reads
of arrays of objects/arrays are rejected until reference unions exist; length,
construction, serialization and in-range writes still work for these arrays.
Writes retain the exact existing bounds check and cannot resize arrays. The
pinned string-index reference is
[String exotic objects](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-string-exotic-objects).

An annotated let without an initializer is permitted only if its type includes
Undefined; initialization stores a real Undefined value/cell at that declaration.
Use-before-declaration and self-initialization remain errors. Bare/implicit
function return is permitted for an undefined-capable result and produces the
matching physical value; no silent zero/string substitution is made. Const
without an initializer and other uninitialized types remain rejected.

`json_stringify_v7` / `json_parse_v7` retain predecessor physical signatures with
value v7/schema v6 contexts. New serialization validates exact record length,
matching descriptor version/count, key/slot uniqueness, optional tag admissibility,
presence bounds, required presence, canonical absent undefined slots and complete
insertion-order metadata, even for omitted fields. Every reserved slot consumes
traversal budget. JSON output sorts integer-index keys first, then own insertion
order, and omits undefined fields. Array/root undefined behavior remains unchanged.
The strict parser continues to reject optional/union target schemas. Required
record parsing creates a fully present trailer with canonical schema enumeration
order before publishing owned values; descriptor/schema checks and host-free
allocator validation still precede allocation. Older descriptors/imports cannot
be substituted. Pure operations preserve system status and resource authority.

Tests cover missing versus explicit undefined, insertion/reassignment/numeric-key
order, all 32 presence bits, scalar/index/UTF-16 edge cases, declaration/return
semantics, malformed trailers and omitted payloads/traversal budgets, GC pressure,
module/public APIs, source-free native execution and package/host ceilings.
Prototype values, reference unions, resizing/sparse arrays, optional JSON parsing
and external adapters remain future work; no application domain is introduced.

### Reference unions and host ABI v7

`checked-f64-utf16-v4` selects **host ABI v7/value ABI v8/JSON schema v7**. It extends
the checked union set with at most one closed Record shape and one Array element
type, in addition to the existing primitives/null/undefined. Duplicate types
collapse; ambiguous multiple record shapes or array element types are rejected.
This restriction lets a tag guard recover the exact physical shape without an
unchecked assertion or a hidden structural copy. Structural widening of mutable
aggregate aliases and generic/discriminated object unions remain pending.

`ReferenceUnion = 9` is a new reference-only tag, supported exclusively by value
v8. PrimitiveUnion tag 8 and its primitive-only validation retain their meaning.
The cell representation stays immutable tag/count=1/payload with the existing
union heap flag. A reference member stores the original record/array i64 handle;
its header child mask is 1. Primitive Number bits never become edges. Ordinary
value v8 records retain v7's fixed slots/presence/order trailer with descriptor
version 8; arrays retain their typed fixed-length layout and may contain tag 9.
Field/array reference masks include reference cells, and the collector follows
cell -> record/array -> nested children without copying or moving any value.

The shared `ReferenceBox::read` validates the cell and either the frozen primitive
contract or a bounded shallow managed aggregate. This includes aligned/ranged
pointers, exact heap kind, allocated block size, count/length, descriptor version,
key/slot uniqueness, kind-admissibility/reference masks, required/absent presence
and insertion order. Array tags/counts/reference flags are checked. Child graphs
are traversed only by consumers that need them; cyclic graphs cannot cause
recursive type/identity validation. JSON retains its independent complete
traversal and cycle/depth/node/output budgets. Cells cannot contain another cell
as their member. Tag 8 cannot be used to smuggle a reference through a legacy
primitive import or serializer.

Widening a primitive subset or exact reference into an expected reference union
copies the handle/cell, preserving actual identity and all bits. Contextual record
and array literals select the unique relevant union member before final boxing,
including optional/nested fields and ternary/function-return contexts. No source
annotation clones the object. In-range indexed object/array reads box the same
reference, while invalid/out-of-range numeric indices return boxed Undefined.
The inherited v3 rules for exact indices, source order, missing fields and bounded
writes remain in force. Optional fields may now contain reference members; their
absent slot remains canonical Undefined and explicit undefined still sets presence.

`typeof` reports object for Record, Array and Null. A direct-local typeof object
guard therefore retains **all three** candidates rather than conflating null or
an array with a record. Existing null/undefined guards can remove those members.
`Array.isArray(value)` accepts the supported value types and tests actual Array
kind; direct-local calls or their negation refine if/ternary branches. Shadowed
Array bindings cannot call this intrinsic. Static known-kind arguments still run
once, then yield the correct constant result. Other guards, broad flow analysis
and writes to refined local bindings remain outside this increment. Mutation of
an extracted record field/array element is allowed and visible to all aliases.
The reference is the pinned
[Array.isArray algorithm](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.isarray)
and [TypeScript narrowing](https://www.typescriptlang.org/docs/handbook/2/narrowing.html).

New pure imports are `value_log_v2(i64)->()`, `value_typeof_v2(i64)->i32`,
`value_truthy_v2(i64)->i32`, `value_nullish_v2(i64)->i32`,
`value_strict_equal_v2(i64,i64)->i32`, `value_nullish_equal_v2(i64,i64)->i32` and
`value_is_array_v2(i64)->i32`. Canonical cells/aggregate headers are validated before
observation. Scalar/string diagnostic formatting stays unchanged; reference logs
serialize bounded JSON, without a result-buffer alias or system-status reset.
Empty arrays/records remain truthy. Strict reference equality compares the
underlying identity, never cell identity or object contents. Numeric/string rules,
including NaN and signed zero, remain unchanged. Logical operators return the
selected operand and skip RHS effects as before; loose equality remains limited
to statically nullish comparisons. See the pinned
[strict equality rules](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-isstrictlyequal).

`json_stringify_v8` / `json_parse_v8` retain predecessor signatures while binding
value v8/schema v7 exactly. Serialization unwraps tag 9 into the real logical
value and validates every traversed field, including omitted absent/undefined
slots. Repeated aliases are permitted; cycles are rejected. Undefined record,
array and root rules stay unchanged. The strict parser still rejects optional/
union targets and creates ordinary required v8 records/arrays through checked
host-free helpers. Generic mixed JSON decoding is not claimed.

Profile/host/value/schema/import agreement prevents earlier runtimes from
reinterpreting new reference cells. Legacy union intrinsics cannot be substituted
even though their scalar signatures match. New value imports require value ABI
metadata. Package grants, host ceilings, UTF-16 resource conversion, output and
execution budgets remain unchanged. Tests cover aliases/mutation/identity,
null/array/record guards, nullable nested indexing, malformed payloads/versions,
cycles, arrays beyond 32 elements and GC pressure, public module APIs, source-free
native execution and preserved system authority. Broader shape unions, dynamic
objects, resizable/sparse arrays, union JSON parsing and external adapters remain
next work; no application-specific state is added.

### Checked optional/union JSON and host ABI v8

`checked-f64-utf16-v5` selects **host ABI v8/value ABI v8/JSON schema v8**.
It inherits the reference-union profile's source semantics, cell/aggregate layouts,
GC, encoding and permissions. A new pure import
`json_parse_v9(i32 inputPtr,i32 inputBytes,i32 schemaPtr,i32 schemaBytes)->i64`
selects the extended schema; `json_stringify_v8` and v2 value operations are reused.
Import/profile/schema/value agreement is checked, and old runtimes reject ABI v8.
All earlier profiles keep their schema versions and non-union/non-optional parsing.
Tool/HTTP adapters for these newer profiles remain pending.

Schema v8 adds `undefined`, `primitiveUnion { members }`,
`referenceUnion { members }` nodes and an optional boolean on record fields
(default false and omitted when serialized false). Strict backward type edges,
unique member kinds and a maximum of seven union members prevent recursive,
nested or ambiguous schemas. The reference-union kind must agree with whether
Record/Array members occur. Each union permits at most one Record shape and one
Array element type; primitives retain their distinct tags. Optional fields must
admit undefined, directly or through a union. Legacy schemas reject extended
nodes/optional fields and retain their prior type-count ceiling; v8 allows the
bounded union table plus the undefined node. Static memory/schema-byte limits
still apply. Every descriptor key, slot, value kind, optional flag, version and
count must agree with the schema before any guest helper is called.

Parsing selects a union's member using the JSON token kind: object, array,
string, finite Number, boolean or null. It never tries multiple object shapes
or coerces a mismatched value. A bounded borrowed `serde_json::RawValue` preserves
the token until that unique member's checked decoder consumes it, including
duplicate-key detection and lossless UTF-16 escape decoding. Enabling the existing
serde_json dependency's `raw_value` feature adds no new dependency. Nested token
re-scanning is bounded by the existing input/depth limits; each logical value is
counted once, including missing declared fields, independently of wrapper cells.
Duplicate and unknown keys, invalid transport, non-finite numbers, wrong members,
array overflow and input/node/depth/schema budgets still fail closed.

A missing optional field becomes a checked Undefined value in its declared slot;
its presence bit stays clear. Required fields must be present even when their
union includes undefined. No JSON token produces Undefined. The compiler rejects
pure undefined roots, required pure-undefined fields and pure-undefined array
elements; optional undefined-only fields can only be absent. Complete validation
finishes before allocation. Parsed unions create owned tag-8/tag-9 cells with the
selected member tag/count=1/payload and exact String/Record/Array child bit. Every
child is retained before another allocation; no numeric payload becomes a root.
Records contain all typed slots plus the unchanged v8 presence/order trailer.
Present keys use the existing schema-normalized JS enumeration order; absent
slots are excluded and unused order entries are zero. Assignment to an absent
field then appends normally. Parsed objects have fresh identity; copies of their
references preserve aliases and mutation.

Allocator/retainer signatures and host-free call graphs are independently
validated for the new import. Failed input/schema/descriptor checks publish no
partial tree. Execution fuel/deadline, heap/root/output limits, capability grants,
host ceilings and pure-operation system status remain unchanged. Regression tests
cover mixed nested arrays, optional primitive/reference/undefined-only fields,
required union fields, lone surrogates, signed zero, alias mutation, GC pressure,
hostile schemas/descriptors/helpers, validation before allocation, version
substitution, public module APIs and source-free native/Wasm execution. Generic
JSON.parse/revivers, broader shape unions, dynamic keys/prototypes, resizing and
external adapters remain future work.

### Resizable arrays and host ABI v9

`checked-f64-utf16-v6` selects **host ABI v9/value ABI v9/JSON schema v9**.
It inherits checked optional/union JSON, UTF-16 and the closed value types, and
adds direct `array.push(...items)` and `array.pop()` intrinsics on a statically
known/narrowed Array receiver. There is no prototype lookup or generic borrowed
method. Arguments are checked against the exact element type, contextually boxed
when needed, and evaluated once in source order. A temporary rooted argument
array holds all values before the append helper runs, so an argument's nested
push/pop completes before the outer push chooses its length/backing. Zero-argument
push evaluates its receiver and returns the current Number length without mutation.
Pop accepts no arguments and returns the element type joined with Undefined.
These admitted dense-array operations follow the pinned
[push](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.push)
and [pop](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.pop)
value/order rules within the explicit checked bounds.

Array tag 5 now refers to a **stable 16-byte identity cell** with payload
`[element_kind:u32, live_length:u32, storage:i64]`. Its heap kind flag is 32 and
its child mask is exactly 1: the sole edge at payload offset 8 owns the backing.
The backing uses array heap flag 8 and payload
`[element_kind:u32, live_length:u32, slots:capacity*8]`; its packed byte length
encodes capacity. Capacity is zero or a power of two, at most 1024, and never
below the live length. Both headers carry the same kind and live length; unused
slots are canonical zero. The backing reference bit reflects the element kind,
including union cells, and never Number payload bits. Value v9 record descriptors
retain fixed typed slots and the previous presence/order trailer with version 9.
Primitive/reference cell tags 8/9 keep their payload representation, while their
aggregate members are validated under the selected value version.

Construction allocates/zeros a suitable backing, retains it, then publishes the
identity cell. Push first checks total length <=1024, then, when needed, allocates
a geometrically larger backing and copies live slots. Existing array/argument
roots keep both old backings and all children alive across collection. A new
backing is initialized completely before publication; the append and length
updates perform no further allocation. Overflow/failed allocation changes no
target slots or length; argument side effects have already occurred. Old backing
blocks become reclaimable normally. Pop clears the removed slot and updates both
lengths before returning/boxing its actual value. The value is rooted before any
boxing allocation; a returned object retains its original identity. Capacity
is retained after pop and reused by later pushes.

GC recognizes flag 32 as exactly one edge, independent of live length, and traces
backing slots only through their live count/reference bit. Fixed-point tracing
handles chains/aliases without a 32-element cutoff. Index reads/writes resolve
the current backing after evaluating operands; a nested mutation cannot leave a
cached stale backing address. Missing reads retain real Undefined behavior.
Indexed writes remain bounded replacements; holes, indexed extension, writable
length, deletion, spreads, constructors, iterators and methods beyond push/pop/
at/slice/includes/indexOf are deferred.
The closed element types/union-shape restrictions and conservative narrowing stay.

New pure imports `value_log_v3`, `value_typeof_v3`, `value_truthy_v3`,
`value_nullish_v3`, `value_strict_equal_v3`, `value_nullish_equal_v3` and
`value_is_array_v3` retain their v2 scalar signatures but validate value v9.
`Vector::read` checks cell/backing flags, alignment/ranges, masks, matching kind/
length, bounded capacity and zero spare slots without recursively reading children.
Reference observations retain bounded shallow validation; JSON recursively checks
all live elements with its cycle/depth/node/byte budgets using stable identities.
`json_stringify_v9` binds value v9 and `json_parse_v10` binds schema v9; physical
signatures are unchanged. Schemas retain v8's checked DAG/union/optional rules and
create owned zeroed backing/cell pairs through the same validated host-free helpers.
Old JSON/value imports cannot substitute for these operations. Metadata/profile/
ABI agreement and launcher protocol v2 negotiation reject incompatible artifacts.

Append, slice and search are typed internal Wasm helpers, making **ten** compiler
helpers in the full dependency set. Pop and relative at reads are emitted directly; no
system import or capability is added. Package/host permissions, status, fuel/deadline, exact roots and memory
bounds remain enforced. Regression tests cover aliases, nested effect order,
mutating RHS/index expressions, union/reference pop values, JSON, growth to 1024,
zero-length calls, collection beyond 32 elements, detached returned children,
malformed layouts/versions, public APIs, source-free artifacts and host ceilings.
Earlier profiles retain their original inline fixed arrays. New-profile Tool/HTTP
adapters, general sparse/dynamic arrays and full ECMAScript compatibility are pending.

### Relative array access and shallow slicing

The v6 profile also admits direct `array.at(index?)` and
`array.slice(start?,end?)` intrinsics. This is an additive compiler extension:
host/value/schema ABI v9, launcher protocol, heap flags and import signatures
stay unchanged. Existing admitted programs and earlier profiles keep their
semantics; earlier profiles do not gain these methods. The method bodies are
ordinary portable Wasm over the existing managed array representation, and the
runtime companion still validates the complete concrete payload before packaging.
No external adapter or application-specific state is introduced.

Bounds must be Number, Undefined or their closed union. An omitted bound is
Undefined; an explicitly supplied bound, including a void expression, evaluates
once with its ordinary effects. String/boolean/null coercion, spreads, extra
arguments, method extraction and non-array/unnarrowed receivers are rejected.
The semantics use the pinned
[at algorithm](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.at)
and [slice algorithm](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.slice)
for the admitted dense arrays and primitive bounds.

At defaults Undefined to zero, maps NaN to zero, and truncates other finite
indices toward zero while retaining infinities. A negative relative index adds
the current length. An outside index returns real Undefined; an inside read
returns the original element, boxed only when the element is not already a union.
Signed zero, UTF-16 strings, null/undefined and reference identity are preserved.
Bounds are checked before converting to an integer or calculating a slot address.
Numeric bracket reads retain their separate exact-index behavior: a fractional
bracket index remains missing rather than adopting at's truncation.

Slice evaluates receiver, start and end once in order, with prior operands held
on the Wasm stack and exact roots. After all argument effects, the helper reads
the current array length/backing. Undefined start means zero and Undefined end
means length; numeric NaN means zero for either. Truncated relative bounds are
clamped to [0,length], with negative indices relative to the end; the copied
count is max(end-start,0). Infinities/extreme finite values never reach a trapping
integer conversion. Primitive normalization has no user callbacks/coercion hooks.

Every slice, including an empty one or the full range, creates fresh backing and
identity blocks. Only selected canonical slots are copied, preserving the source
element kind and actual reference mask. Scalar bits and member cells remain
unchanged; object/array children are shared. Replacing/pushing/popping slots in
one array cannot resize or replace slots in the other. Mutating an extracted
shared child is visible through either array. New capacity follows the existing
zero/power-of-two policy and the 1024-element bound; spare slots are zero.

The source remains rooted across range copying and both allocations. The copied
backing is complete, marked with its real element-reference bit and explicitly
retained before identity-cell allocation can collect. The returned identity owns
that backing before temporary roots are restored. Allocation failure publishes
no partial result and performs no slice mutation on the source. Reclamation,
fuel/deadline, heap/root/output limits and pure-operation system status remain
unchanged. Regression tests cover optional union bounds, NaN/infinities/fractions,
source order and nested mutations, empty/full fresh identity, shared references,
UTF-16/signed zero, GC pressure, the complete helper dependency set, public module APIs,
source-free native/Wasm execution and denied privileged argument effects.

### Dense array search

V6 also implements direct `includes(value?,fromIndex?)` and
`indexOf(value?,fromIndex?)`, retaining host/value/schema ABI v9. The pinned
[includes](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.includes)
and [indexOf](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.indexof)
rules supply SameValueZero versus strict equality: includes finds NaN, indexOf
returns -1 for it, and both match positive/negative zero. Results are a canonical
Boolean or a binary64 first index/-1. The admitted arrays are dense; holes and
prototype lookup remain unsupported.

The needle can be any supported checked value, independently of the array's
static element type. No coercion, widening or shape cast occurs. Primitive and
reference cells are unwrapped to actual kind/payload; kinds must match. Strings
use the existing UTF-16 comparator, including lone units, empty strings and NULs.
Records/arrays compare underlying identity, not shape, contents or cell identity.
Different record/array types need no union join because no narrowed value escapes.
Captured results and other non-value types remain rejected.

Receiver, needle and starting-index expressions evaluate once in that order before
reading the current length/backing. Omitted needle means Undefined; optional
Number/Undefined starting bounds use the existing normalization. NaN defaults to
zero, fractions truncate, negative offsets are relative to the end, and infinities
are bounded before conversion. Empty/outside searches still execute arguments.
Prior operands remain exactly rooted across later argument allocations, including
needles detached by pop. The scan/comparator allocate nothing and invoke no host
operation or user hook; numeric payload bits never become roots or pointers.

One shared typed search helper plus the existing comparator makes a maximum of
ten helpers. Function indices/dependencies are resolved through the emitter table;
no new import, capability, layout or metadata version is introduced. Fuel/deadline,
heap/root limits and system status remain enforced. Tests cover mixed primitives,
NaN/zero, UTF-16 content, reference identity, optional bounds, argument effects,
GC, all ten helpers, public module APIs, source-free artifacts and host ceilings.
Earlier profiles retain their admitted operations.

### Additive String.repeat operation

The native-family profile (`checked-f64-utf16-v9`) and its successors also support
`String.repeat(count?)` as a pure compiler operation. Host/value/schema versions,
imports/exports, layouts and the ten-helper ceiling are unchanged. The admitted
Number/Undefined rules follow the pinned
[String.prototype.repeat algorithm](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.repeat):
undefined/omitted/NaN become zero, finite fractions truncate toward zero, and a
negative normalized count or either infinity raises RangeError even for an empty
receiver. A finite count on an empty receiver returns empty without conversion to
an integer index. Coercive arguments, non-string/borrowed receivers and boxed
String objects remain unsupported.

Lowering records the receiver, count and typed boxed RangeError expression. Two
hidden i32 copy slots are shared per function activation, allocated only in
functions using repeat, and are never GC roots. Emission evaluates both operands
once before touching these slots, so nested calls/repeats preserve their operands.
The receiver stays exactly rooted through count side effects and allocation.
Invalid counts use the existing guest exception channel and handler/finalizer
unwind path, with the fixed message "Invalid string repeat count". Earlier profiles
remain explicitly unsupported, rather than receiving malformed Error values.

After count validation, an empty/zero result needs no result allocation. Other
results check the complete UTF-16 byte length against the existing 1 MiB string
budget before bounded integer conversion. One result block is allocated; the
first copy comes from the immutable receiver and subsequent bulk copies double
the filled prefix, clipping the final copy. The number of copy instructions is
logarithmic in the repetition count and total copied bytes are linear in output.
No call/allocation occurs during copying; the final owned packed handle is rooted
through the usual expression path. Existing fuel charges and all resource limits
remain fatal and bypass catch/finally. Status and capability authority are unchanged.
Tests cover normalization/infinities, Unicode, nested operand effects, GC, maximum
byte bounds with explicit test fuel, helper-count limits and source-free execution.

### String range contract

V12+ admits String.slice(start?,end?) and String.substring(start?,end?) on a
narrowed String receiver, with zero to two Number/Undefined bounds (including
their union). Earlier profiles, non-string/nullable receivers without narrowing,
coercion, borrowed/computed methods, spread/generic calls and extra arguments
remain rejected. Follow the admitted parts of
[String.prototype.slice](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.slice)
and [String.prototype.substring](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.substring).

Supply zero for omitted/undefined start and length for omitted/undefined end;
normalizing the latter with positive Infinity is equivalent for bounded strings.
Apply ToIntegerOrInfinity: NaN becomes zero and fractions truncate toward zero.
Slice resolves negative integers relative to code-unit length, clamps both bounds
and uses an empty result for end before start. Substring clamps both bounds to
0..length and swaps if start exceeds end. Clamp in floating point before integer
conversion, including infinities and huge finite inputs. The resulting indices
are bounded by the source's existing UTF-16 byte limit.

Lower a shared slice receiver exactly once before selecting array/string
semantics, preserving lifted functions/captures and existing Array.slice rules
(including older profiles). Typed StringRange IR preserves receiver/start/end
order; emission retains earlier operands on the Wasm stack and in exact roots
across later bounds, nested calls and pending-exception checks. Actual operand
effects remain visible to helper validation and tool/capability analysis. A
throwing earlier operand skips all later operands and enclosing effects.

Private StringSlice/StringSubstring helpers receive the rooted source and
normalized typed bounds. They compute unit indices, allocate once for nonempty
results and bulk-copy the exact selected UTF-16 bytes. Never return interior
slices: the nonmoving collector expects a root to identify its owned allocation.
Canonical empty results allocate nothing, after all argument effects. Whole-string
ranges still copy into owned storage. Preserve lone surrogate units, NULs, content
comparison and immutable string ownership. No user/host call or further allocation
occurs during copying. Helper locals are private activation-local scratch and
cannot overwrite live outer receiver/bound operands.

The existing 1 MiB encoded string ceiling, memory/root/frame/type/source/body
budgets, Wasmtime bulk-copy fuel and deadline checks remain active. Resource
failure and authorized exit stay fatal; status and grants are unchanged. No new
host operation/import, capability, helper export, schema or ABI/layout change.
Tests cover bound normalization, UTF-16 edges, effect/throw order, nested calls,
GC/detached sources/child graphs/pairs/saved returns, maximum-byte copying with
explicit test fuel, fatal budgets, arrays/older profiles, virtual modules,
source-free artifacts, exact grants/host ceilings and dependency diagnostics.

### String search contract

V12+ admits String.includes/startsWith/endsWith with a required String needle and
optional Number/Undefined position (including the union), on a narrowed String
receiver. Reject omitted/coercive/regex needles, nullable receivers without
narrowing, other position types, borrowed/computed methods, extra arguments,
spread/generic calls and earlier profiles. Follow the admitted parts of
[String.prototype.includes](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.includes),
[startsWith](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.startswith)
and [endsWith](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string.prototype.endswith).

Default omitted/undefined includes/startsWith positions to zero; endsWith defaults
to length (positive Infinity before clamping is equivalent). NaN becomes zero;
truncate fractions, clamp to 0..code-unit length in floating point, then convert
to a bounded integer. Negatives are not relative to the end. Empty needles match
at every admitted position, including empty sources and clamped endpoints.
Includes walks candidate starts at/after the position, startsWith tests that start
alone, and endsWith tests the candidate ending at its exclusive position. Reject
overlong candidate ranges before subtraction; never read beyond either payload.

Typed StringSearch IR and private StringIncludes/StringStartsWith/StringEndsWith
helpers return canonical 0/1. Compare exact UTF-16 units with aligned unit offsets,
not unaligned byte subsequences. Preserve case, lone surrogates, embedded NULs and
combining sequences without Unicode normalization. Helpers allocate nothing and
make no host/helper calls. Existing exact-root expression wrappers retain receiver
and needle through position effects, nested calls, pending-exception checks and
collection; earlier scalar/physical-pair operands and saved booleans survive
ordinary finalizer/return routing. Layout/effect visitors traverse all three
operands, retaining actual capability effects and host-free helper validation.

Shared includes dispatch lowers the receiver once before selecting string or
array semantics, including lifted functions/captures. Preserve Array.includes
SameValueZero, arbitrary checked needles, sparse presence and relative positions
under its existing profiles. No new host operation/import, capability, helper
export, schema or heap/ABI layout is introduced; status and grants are unchanged.

The initial includes scan compares each bounded candidate until mismatch; worst
case cost is O((n-m+1)*m) code-unit comparisons. Prefix/suffix checks are O(m),
extra helper space is O(1), and no new search-size ceiling is added. All work is
Wasm-fuel metered with existing deadline/string/memory/frame/root/type/source/body
limits. Adversarial repetition can exhaust fuel and remains fatal, bypassing
catch/finally. Tests cover this boundary, maximum strings with explicit test fuel,
UTF-16 unit alignment, effects/throws, GC/pairs/saved results, array/legacy rules,
virtual modules and source-free artifacts with grants/ceilings and diagnostics.

### Array mutation profile and host ABI v17

`checked-f64-utf16-v14` selects **host ABI v17 with unchanged value ABI v12/schema
v12**. The new source profile freezes indexed extension and writable-length rules
that differ from v13's checked existing-index-only writes. Sparse vector/backing,
record, Error, closure and exception layouts, v6 value operations and the
json_stringify_v12/json_parse_v14 boundary remain unchanged. There is no new host
import or capability. Earlier profiles retain their rules and helper ceilings;
launcher protocol v2 now advertises 17/12 and the new profile.

For indexed assignment, receiver, numeric key and RHS evaluate once in that order.
Each live reference is rooted through later operand effects and allocation. A key
or RHS exception stops the requested write; prior operand side effects are not
rolled back. Numeric-domain checks happen after RHS effects: only finite integral
indices 0..1023 are admitted. Negative/nonintegral/nonfinite or out-of-bound writes
fail fatally in this checked subset; arbitrary numeric own-property/coercion rules
remain unsupported. The original receiver and key are retained even if the RHS
reassigns their source variables. If index >= current length, resize creates holes
through index, then the value is written and that index marked present. Current
length/backing are resolved after RHS effects; identity/aliases remain stable.

Length assignment accepts Number and also supports literal `["length"]`. Following
the admitted numeric part of [ArraySetLength](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-arraysetlength),
NaN/infinities, fractions, negatives and values > 2^32-1 raise a recoverable owned
RangeError with fixed message `Invalid array length`, before resize. Negative zero
is accepted as zero. Valid uint32 lengths above 1024 instead exceed the existing
resource bound and trap fatally; they are not reclassified as language errors.
No string/boolean/null/undefined/object coercion is supplied. The receiver/RHS
are evaluated once before validation, and both counts reflect the resulting live
receiver state. New holes remain distinct from present Undefined.

The internal host-free ArrayResize helper checks its bounded length and compares
it with capacity. Growth beyond capacity allocates/zeros a geometric backing,
copies live payloads and the complete bitmap, initializes flags/masks/counts and
publishes the new backing and vector count without intervening allocation or user
code. The old backing stays rooted through allocation. Growth within capacity only
publishes lengths because spare data/bits are already zero. Shrink clears every
removed payload and presence bit before reducing backing/vector counts; capacity
is retained. Removed edges cannot retain children, while separately rooted aliases
stay live. A reference RHS is rooted across growth before publication into the
array. No partial allocated backing is exposed by a recoverable error. Fuel/memory
or other fatal traps abandon execution rather than providing transactional rollback.

Array-only [delete](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-delete-operator-runtime-semantics-evaluation)
evaluates the receiver and computed numeric key once, clears a present slot/bit
without allocation, preserves length and returns canonical true. A missing,
negative/fractional/nonfinite or out-of-range numeric key is absent in this closed
array model and returns true without mutation. Canonical literal index strings
are supported; other literal names are absent. Deleting own nonconfigurable length
(`.length` or `["length"]`) raises owned TypeError `Cannot delete array length`,
matching module strictness. Dynamic strings, private keys, non-array properties,
non-reference operands and general property deletion are unsupported. No inherited
or user-defined own properties/configurability are introduced.

Map/filter/forEach still use initial length and live presence; find still visits
Undefined for missing indices. Deletion/shrink skip later visits where required,
and later extension inside the original range is visible. Pre-predicate values
and detached references stay rooted. Sparse JSON rendering, presence validation,
parsing, status and package/host grants retain their contracts. Helper validation
still rejects any reachable host import through callback/wrapper dispatch.

The new helper raises the v14 internal-function ceiling to 13, while v13 keeps 12
and preceding profiles keep 10. Source function/type/body, memory/root, fuel,
deadline, host-call/output and array length bounds remain. Tests cover evaluation
order, receiver/key reassignment, RHS errors, limits versus RangeError, strict
length deletion, live callbacks, retained aliases/GC, maximum index/length,
independent deterministic mutation models, helper purity/bounds and source-free
module/public/CLI execution. Compound/assignment-expression forms, generalized
keys/coercion, constructors/prototypes and additional collection APIs remain
separate language work.

### Sparse array profile and host ABI v16

`checked-f64-utf16-v13` selects **host ABI v16/value ABI v12/schema v12**. It
inherits v12 functions/closures and Array callbacks while adding explicit index
presence. The new mandatory versions prevent older readers from reinterpreting
backing metadata. Existing profiles keep their source, layout, helper and budget
contracts; v12's missing-map-index trap remains frozen there. Launcher protocol
v2 advertises the new profile and 16/12 maxima; container/manifest formats and
capability IDs are unchanged.

Array identity remains the 16-byte vector cell `[kind:u32,length:u32,backing:i64]`,
heap flag32/mask1. The v12 backing is `[kind:u32,length:u32,payload:i64[capacity],
presence:u8[128]]`: the original eight-byte metadata and slot offsets stay fixed,
and a 1024-bit bitmap follows capacity slots. Bit `index % 8` in byte `index / 8`
marks presence. Allocation flags are **1 | 8 | 1024**, distinguishing these blocks
from old dense storage. Capacity still grows geometrically up to 1024 elements;
zero capacity still owns the bounded bitmap. Packed backing byte length, not a
possibly larger recycled block size, locates the bitmap. Canonical readers require
exact layout/flags/kind/count/mask agreement, power-of-two or zero capacity,
zero spare payloads, zero bits beyond live length and zero payloads for holes.
Present undefined/zero/empty-string payloads remain distinct through their bits.

The collector follows the existing live payload-slot range and actual element
reference mask. Hole payloads are canonical zero and cannot retain children;
bitmap bytes lie outside that range and are never interpreted as references.
Growth copies existing payloads and bitmap before publishing a new backing.
Appending copies argument presence as well as payload; spare bits/payload stay
zero. Removed payloads and bits are cleared before reducing both lengths, and
returned references acquire their normal temporary roots before boxing allocation.
Indexed replacement evaluates/root operands in source order, completes the RHS,
writes the payload and then marks presence without an intervening allocation.
It still requires an existing index within length; indexed extension is pending.

Two internal host-free helpers query/set checked presence. V13 permits at most
12 internal helpers, while older profiles retain 10. Source hasOwn uses Number
indices (nonfinite/fractional/negative/out-of-range values are absent) or canonical
literal index strings. Numeric -0 selects index0; literal `"-0"` is a distinct,
unsupported own property and is absent. The own `"length"` property is true;
other literal names are absent in this closed-array subset. Dynamic strings,
coercive keys and arbitrary own properties remain unsupported. Query base and
numeric key expressions execute once before reading current presence.

Sparse literals are admitted within the existing element/length bounds; all-hole
literals require an explicit array element type. Bracket/at reads return real
Undefined for a hole before loading/unboxing its payload. Writes within length
fill holes. Slice preserves holes at shifted indices in a fresh vector/backing.
Pop decrements length even for a hole, returns Undefined for absence and clears
storage/bitmap before later allocation. Includes compares holes as Undefined,
while indexOf skips them. These behaviors follow the admitted portions of the
[ECMAScript indexed-collection algorithms](https://tc39.es/ecma262/2025/multipage/indexed-collections.html).
No prototype lookups or generic array-like receivers are introduced.

Map snapshots the input length after operand evaluation, allocates one final-length
all-hole output with geometric capacity and fills only indices present at their
turn. Absent indices are retained without invoking the callback. This removes
v12's dense-output restriction in the new profile and avoids per-index temporary
append arrays. Filter/forEach skip holes and find visits them as Undefined;
pre-callback snapshots, current backing lookup, mutation/regrowth and captured
state semantics remain. Partial output stays private until successful return.
Source errors unwind normally, and earlier callback mutations are not rolled back.
Resource/ABI traps and authorized exit remain fatal.

Value v12 retains all preceding tags and scalar/record/Error/closure layouts,
with v12 record descriptors. Versioned v6 value observation/equality imports and
value_is_error_class_v2 validate v12 aggregates before observation, preserve status
and require no new capability. json_stringify_v12 renders holes as null, counting
them against logical traversal/depth/byte limits and validating zero hole/spare
payloads before output. json_parse_v14 binds schema12/value12, validates the whole
closed input/schema before allocation and builds fully present bitmaps for parsed
arrays. JSON null never becomes absence. A round trip may require a nullable schema
and never restores holes. Syntax/schema failures retain owned SyntaxError/TypeError;
metadata/helper/budget failures stay fatal. The primitive/Error exception channel
and family selector remain unchanged; arrays still cannot be thrown as values.
Mixed older imports, profiles, values or schemas are rejected before effects.

Tests cover literal/explicit-undefined distinctions, filling, callback shrinkage,
byte/word bitmap boundaries, growth, shifted slices, pop/search, 1024-length maps,
reference roots under collection, optional/union JSON, malformed/zero/unused bits,
version downgrade attempts, helper effects, status/permissions, public APIs and
source-free portable/native artifacts. V14 above adds delete, writable length and indexed extension; constructors,
species/prototypes and generalized properties remain separate work.

### Additive Array reduction and quantifiers

Reduce/some/every extend the v12 closure profile and all successors through the
same ordinary typed IR and direct function dispatch as map/filter/forEach/find.
No new import, helper, layout, capability, exception selector, protocol or ABI
version is required. V12 stays host v15/value/schema v11; v13 stays host v16/value/
schema v12; v14 stays host v17/value/schema v12. Pre-v12 profiles reject them.

The supported portions follow the ECMAScript 2025
[reduce](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.reduce),
[some](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.some)
and [every](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.every)
algorithms over Varta's closed arrays. Receiver, callback and optional seed
expressions evaluate exactly once, in that order, before the wrapper snapshots
length. Argument failures stop subsequent evaluation and never enter traversal.
The wrapper validates the function signature and closed code identity even when
no invocation will be needed. It reads live presence/backing at each turn, skips
removed indices/holes and ignores appends beyond the initial range. Replacements,
fills and regrowth inside that range are visible. There are no inherited array
properties, getters, coercion hooks or borrowed array-like receivers in this
subset. V12 uses current dense length; v13+ use their presence bitmap.

Some/every accept a callback plus optional thisArg. Fixed callback parameters
form a checked prefix `(element, Number index, receiver)`. Inline arrows infer
parameters and expression results; unannotated blocks infer typed returns as
described in the callback inference contract below.
The shared predicate path applies checked truthiness, including reference/function
values and void-to-Undefined, and rejects VartaResult physical pairs. Some returns
canonical true immediately on a truthy result, otherwise false; every returns
canonical false immediately on a falsy result, otherwise true. Empty/all-hole
arrays produce false/true respectively, without invoking callbacks.

Reduce requires one or two arguments. A seed expression determines the exact
accumulator type U; without a seed U is the element type T. A void seed expression
is evaluated for effects and normalized to Undefined. Explicit Undefined counts
as a supplied seed independently of its value. Reducer signatures take an exact
fixed parameter prefix `(U accumulator, T value, Number index, receiver)` and return
exactly U. Inline arrows receive both parameter and return context, including
union widening and contextual record/array returns; named/local/returned function
values retain invariant signature checks. Seed context now comes from reducer
annotations/signatures or a surrounding aggregate result, as specified below.
Unconstrained/ambiguous seeds still require annotations. Generic call arguments,
Optional/default/rest parameters are specified below; generalized function syntax
remains unsupported.
Typed thisArg is specified below.

With a seed, every visited present entry invokes the reducer and an empty/all-hole
array returns the seed unchanged. Without one, a separate initialization flag
selects the first present value as accumulator without invoking the callback.
Every later present entry receives the previous accumulator and the pre-callback
snapshot of the current value. No present entry constructs and throws an owned
TypeError with the fixed message `Reduce of empty array with no initial value`
through the existing pending-exception channel. No synthetic zero, undefined or
hole value initializes an absent accumulator. The uninitialized Wasm local is
never read or returned on that path.

Array, callback, seed and evolving accumulator use ordinary typed frame roots.
Each current element retains its own root through callback effects, including
removal of its source slot. Update accumulator roots only after a successful
callback, preserving old reference operands across allocation; results transfer
through the existing return-root contract. Records/arrays, strings, unions, Error
objects, function values and multi-slot VartaResult accumulators retain their
ordinary representation. Function-valued aggregates/JSON remain unsupported.

Source throws and callback finalizer overrides use existing completion routing;
no later entry runs after a pending exception. Earlier user effects persist.
Resource/ABI traps and authorized exit remain fatal and bypass guest catch/finally.
Pure traversal preserves system status and grants. Actual host calls still match
both package and host grants, and helper validation follows every wrapper/dispatch
edge to reject host effects. Wrappers consume existing user-function, semantic
type, code-size, frame/root, fuel/deadline and memory budgets; no additional
managed helper is introduced. Tests cover sparse mutations, source order, exact
results/types/arity, empty/malformed callbacks, 1024 entries at default fuel,
collection with detached operands and closure accumulators, source/fatal errors,
helper purity, host ceilings and source-free/public engine execution.

### Additive Array callback operations

The v12 closure profile also implements map/filter/forEach/find as compiler
operations under **host ABI v15/value ABI v11/schema v11**, without new host
imports, helpers, capabilities, layouts or protocol versions. Earlier profiles
reject these methods. They use the existing function/closure representation and
checked direct dispatch, not a parallel callback engine.

The supported traversal follows the bounded dense-array portions of the
[ECMAScript 2025 indexed-collection algorithms](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.map),
including the distinct [find traversal](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-findviapredicate).
Receiver/callback expressions run once in source order. Length is snapshotted
before the first callback; each visited value is loaded from the current backing.
Appends outside that range are ignored. Filter/forEach skip indices removed before
their turn. Find still visits them as undefined. Replacements/regrowth inside the
range are visible. Filter and find snapshot the element before invoking the
predicate; predicate mutations do not cause a second read of that slot. Returning
references preserves the original object identity and its live contents.

These methods take one callback and an optional thisArg (specified below); zero
through three callback
fixed parameters form a checked prefix of `(value, Number index, receiver)`.
Optional/default/rest forms follow the parameter contract below.
The first type is the array element type for map/filter/forEach, and the element
union Undefined for find. This checked find annotation differs from unchecked TS
Array declarations so removal cannot feed an unrepresented value into a callback.
Named functions, returned functions, local values and capture-bearing arrows all
use the same signature checks. Unused trailing arguments have no source-visible
arguments or reflection semantics. Typed this and optional/default/rest parameters
follow their contracts below. Inline arrows infer
parameter types; expressions infer result types. Unannotated inline callback
blocks now infer typed return sites, as specified below. Explicit result
annotations remain authoritative. Generic/async,
prototype/borrowed methods remain unsupported. Optional/default/rest forms and typed
thisArg and function expressions are specified below.

Map returns a fresh array of the callback's checked value type; declared void
results become real Undefined entries. Function and VartaResult elements remain
unsupported. Filter returns a fresh same-element array; predicates accept checked
JS values through truthiness, including references and functions, but not physical
VartaResult pairs. Find returns element|Undefined and stops after the first truthy
predicate; no match returns canonical Undefined. ForEach discards any admitted
result, including physical pairs/functions, and returns source undefined.

Map is deliberately restricted to visits with present dense elements. If an index
is absent at its turn, the existing checked-index trap ends execution. A shorter
or undefined-filled result would change later callback behavior, so no such
substitution is made. Sparse-map support requires a versioned array presence
representation. Removal of a just-read element is supported if no later visited
index is absent. Filter/forEach/find need no sparse output and retain their stated
removal behavior. No species, proxy, prototype or generalized array-like-object
semantics are claimed.

Lowering emits one bounded typed wrapper per method call site, using ordinary IR
loops, array reads/appends, shadow-rooted bindings and FunctionCall dispatch. Its
array and callback parameters stay rooted, and a per-iteration binding retains the
snapshotted element across predicate allocations/removal. Map return operands are
rooted through append allocation; partial results are private until return.
Reference backings are selected after operand effects. Every wrapper checks the
callback handle/signature/closed code target even for empty arrays, without running
user code. Captured environments/cells use the existing entry checks when called.
Pending exceptions stop the loop before append/later effects and propagate through
normal try/finally. A throwing callback's earlier mutations remain observable;
there is no transaction or rollback. Fatal traps/limits and authorized exit bypass
source catch/finally.

Wrappers consume the existing 256 named/lifted-function budget and 65,536-byte
body bound, including their dispatch expansion. The fixed ten managed helpers,
128 semantic function types, 1024 array elements and ordinary memory/root/fuel/
deadline/host-call/output limits remain in force. Pure predicate/value operations
preserve system status while consuming their existing host-call budget where
applicable. Actual privileged calls still require both package and host grants.
JSON helper validation follows the wrapper and callback's direct call edges and
still rejects reachable host imports. Tests cover source order, mutation/removal/
regrowth, empty/maximum arrays, truthiness, types, early returns, exceptions/fatal
limits, malformed callback objects, exact roots under collection, helper effects,
public modules and source-free native/portable execution.

### Contextual callback return and reduce seed inference

This compiler-only increment applies to Array callbacks in v12 and successors.
It changes no runtime ABI, memory representation, host registry operation,
capability, helper count or artifact protocol. Existing artifacts retain their
signatures and behavior. Ordinary arrows outside the Array callback context keep
the existing Number/void default; typed function contexts and explicit return
annotations remain exact and authoritative.

For an unannotated inline map/filter/forEach/find/some/every block, lower the body
once with deferred return checking. Its ordinary lexical scopes, guards, captures,
side effects, generated function indices and resource accounting are resolved by
the existing semantic path, without speculative re-lowering. Traverse only that
body's statement IR to collect typed return sites; nested arrow functions have
separate bodies and do not contribute. All syntactic sites contribute
conservatively, including unreachable/overridden finalizer returns. A bare return
or a possible normal completion adds Undefined if any value-returning site exists.
A body without a value-returning site retains void, including throw-only callbacks.

Identical types retain their exact type, including function or physical-result
values for forEach. Differing return types use the existing closed union interner:
primitive/reference/Error members are admitted under its limits; multiple record
shapes, multiple array element types, function unions and VartaResult unions remain
unsupported. Each return is widened to the chosen type through existing checked
boxing. Bare returns become canonical Undefined; the existing completion checker
inserts an implicit Undefined return when needed. Predicate truthiness, map value
restrictions and forEach result discard are checked afterward without coercion or
relaxation. Explicit reducer return types continue to use their accumulator type.

Finally saved-return locals are deferred until the inferred type is known. Then
allocate an exact typed saved slot per finalizer and enclose it in a lexical root
scope; physical result pairs retain both slots and reference results retain their
exact root. This preserves pending returns across allocating finalizers, finalizer
overrides, nested finalizers, thrown values and every branch exit. Return-expression
boxing is performed before the normal save/unwind path. No new emitter or runtime
exception path is introduced.

For reduce with a seed, contextual type U is selected in this order:

1. An inline reducer's annotated first parameter, or its explicit return annotation
   if that parameter has none.
2. For other function values, the already resolved function signature's first
   required parameter, or its result type for optional/rest-first or zero-parameter
   reducers.
3. Otherwise, an aggregate literal seed may use the surrounding expected result
   type from variable/return/argument positions. A union result selects the matching
   record/array member for this fallback rather than widening the accumulator.
4. With no context, infer the seed expression as before.

Lower the seed once with that expected type, recursively contextualizing nested
arrays/records and optional fields. Require the resulting type to equal U, then
check the callback's existing exact parameter prefix and return contract. Explicit
callback union contexts may widen a scalar seed, but an outer wider scalar result
alone never widens it: a number accumulator used in addition stays Number. Empty
or all-hole arrays without context still fail; no usage-constraint solver, any,
shape coercion or type assertion is supplied. Generic calls remain unsupported;
typed thisArg is defined below. An unseeded reducer still uses the element type and its first-present
initialization rule.

Resolved non-inline callbacks are retained as IR operands and not lowered again.
Runtime order remains receiver, callback, seed once each, then the length snapshot.
Callback/seed throws stop later effects. Existing root transfer, live presence,
function/code/type bounds, checked host effects, status and grants remain intact.
Tests instantiate emitted Wasm and cover inferred unions/fallthrough, nested arrow
isolation, return-slot widths and GC through finalizers, seed contexts/signatures,
argument order, diagnostics/rejected ambiguity, older profile gates, helper purity,
capability ceilings and source-free/public execution.

### Anonymous function expressions and typed this

This additive compiler increment is available in v12 and successors. Anonymous,
synchronous, nongeneric function expressions use the existing managed function/
environment/capture cells, pending-exception channel and direct Wasm dispatch.
No host import/registry operation, capability, heap layout, schema, helper count,
ABI or launcher protocol version is added. Existing compiled artifacts keep their
meaning. Named expression/self bindings and declaration this are implemented
below. Constructors, arguments, bind/apply, generators, async,
function-valued aggregates and object method calls remain separate work.

Function expressions accept exact function-type context and inline Array
callback parameter prefixes. An unannotated result uses the same single-lowering
return inference as callback blocks. Each evaluation creates a fresh identity;
initialized outer bindings use the same shared cells and lexical restrictions
as arrows. The implementation translates the expression's body/positional
parameters into the common lifting input with a nonlexical-this mode, retaining
source spans and checks. A new ordinary function boundary removes outer this;
it must never accidentally capture the surrounding function's receiver.

TypeScript's explicit this annotation is stored separately by the parser and is
not a positional source parameter. A function-type `(this:T, value:V)=>R` has
logical parameters `[V]`, receiver T and result R. `this:void` maps to Undefined;
other receivers must be a checked JS value, including function values, but not a
physical VartaResult pair. Function expressions take T from an explicit this
annotation or expected function type; conflicting annotations fail. Reading this
without this context, or top-level this, fails explicitly. No implicit object-method
or thisArg-based inference is claimed. Named declarations now support an explicit
receiver through the contract below; legacy tool/HTTP signatures stay receiver-free.

This increment follows strict receiver handling: supplied primitives, Null and
Undefined retain their exact values without global-object fallback or boxing.
A direct function-value call supplies Undefined; a receiver contract excluding
it is a compile-time error. Arrow functions retain lexical this through ordinary
capture cells and transitive environments; their own typed dynamic receiver
context is unsupported. A new function expression establishes its own boundary
and can only read its own typed this. Shape/coercion/variance conversions are not
introduced. Explicit receiver mode remains part of exact function-type identity.

Internally a receiver-bearing user function has T as its first physical value
parameter, before positional parameters, followed by its existing environment
slot. This slot uses normal parameter roots/cell capture and contributes to the
32 physical-slot ceiling; physical result pairs in positional arguments retain
their usual width. Internal function types distinguish receiver mode even when
two functions have identical Wasm parameter/results. Closed dispatch matches both
that mode and the complete physical signature/result, and its checked target set
rejects code-index substitution between `(this:Number)=>Number` and
`(value:Number)=>Number`. No physical signature/layout changes apply to old code.

Map/filter/forEach/find/some/every accept exactly one or two arguments. They lower
receiver, callback and optional thisArg once and preserve that runtime order. The
wrapper receives thisArg as its third parameter, roots its actual typed value and
snapshots length only after argument effects. A typed callback receiver must
accept that value; omitted thisArg is Undefined. Each invocation widens the rooted
value to its checked receiver type without reevaluating the source expression.
Arrows and functions lacking typed this ignore the supplied value but still
evaluate its expression, even for empty arrays. Presence, initial-length bounds,
pre-callback snapshots and short circuits remain unchanged. Reduce accepts no
thisArg; its optional second argument stays the seed and callback this is Undefined.

The checked `fn.call(thisArg, ...positional)` intrinsic evaluates the function,
receiver and positional arguments once in order and uses the same dispatch,
rooting and pending-exception path. Only fixed, nonspread arguments are accepted.
Missing thisArg supplies Undefined; logical argument counts/types remain exact.
An unused receiver is explicitly evaluated/discarded at its full physical width
before positional arguments; its imports/effects remain visible to collection and
helper-purity traversal. No ambient reflection or Function.prototype is exposed.

Receiver, callback and later operand allocations use existing exact expression/
frame roots. Escaped arrows retain receiver cells/graphs; nested finalizers use
normal inferred saved-return roots. Source errors unwind and preserve prior user
effects, while resource/ABI traps and authorized exit remain fatal. System status
and both package/host permission checks retain their roles. Tests exercise all
six methods, strict scalar/null/undefined/function receivers, source order and
ignored effects, lexical/escaped capture under GC, sparse mutation/finalizers,
malformed code identity, parameter/fuel limits, helper effects, operation/host
ceilings and source-free/public module execution.

### Named-expression self bindings and declaration receivers

This additive v12+ compiler increment preserves host/value/schema versions,
managed layouts, helpers, imports, capability rules and artifact protocols.
Named function expressions share ordinary expression lifting, exact receiver/
parameter types and managed captures. Their result must be known from an explicit
return annotation or expected function context before lowering the body; no
recursive return-type solver is introduced. Anonymous expression inference keeps
its existing behavior. Forward capture/recursive initialization through an
uninitialized surrounding variable remains unsupported.

Create an immutable private self binding behind the function's parameter/body
scope. It is visible only inside the expression and nested closures; parameters
and body locals can shadow it without changing external bindings. Reserved engine
intrinsic names retain their binding restrictions. Self resolves before a same-
named module declaration or uninitialized outer variable. Assigning the private
name fails statically; reassigning an external alias does not alter it.

Logical function types contain only the explicit receiver and source parameters.
A named expression appends one private Function-typed physical parameter after
those parameters and before the existing environment parameter. Closed dispatch
matches logical signatures without that private suffix, then passes the actual
callee object to named-expression targets. Ordinary/anonymous/declaration targets
receive their unchanged argument list. Self names do not create permanent roots,
self-capturing environment cycles or another heap layout. Fresh expression identity
is unchanged.

The callee roots this incoming handle through the ordinary parameter-root path.
Before environment/cell allocation or user effects, verify the self object's
managed function header, semantic signature, exact code target and equality of
its environment edge with the supplied environment. Existing environment/binding
checks then validate the capture graph. The private name uses that exact handle;
recursive calls therefore retain the original object and its environment. Nested
arrows/functions capture the immutable binding through the usual managed cell,
including escaped/transitive captures. Incoming roots protect the handle while
captured parameter cells are allocated. Shadowing, finally, return/throw and frame
exits use the existing ownership/completion paths. Multi-slot arguments/results
retain both physical slots while self remains one rooted slot.

The self parameter, explicit receiver and environment all count against the
32-physical-slot ceiling. Existing 128 semantic-type, 256-function, capture count,
body size, root-stack, memory, fuel/deadline and host-call limits remain active.
Unbounded recursion fails fatally through those limits; source catch/finally does
not convert it to a recoverable error. Host-free helper analysis still follows
all closed direct-call edges, including recursive cycles and receiver calls.

Top-level named declarations now accept a typed this parameter in v12+; parse it
separately and retain it in the shared compiler signature without adding a logical
source parameter/name. The physical receiver is first, followed by source
parameters, with no closure-environment parameter on the declaration itself.
References/import aliases intern the receiver-aware type and retain lazy canonical
identity with a zero environment. Bare direct calls insert checked Undefined;
reject contracts excluding it. Function.call and Array callbacks provide the
normal checked explicit receiver. The declaration's this can be captured by
returned arrows through ordinary parameter cells. Recursive declarations follow
the same strict-this rule: use name.call(this, ...) when Undefined is excluded.

Main must remain receiver-free. Earlier profiles reject declaration this, and
Tool Contract v1/HTTP v1 reject receiver-bearing handlers rather than silently
erasing a receiver or altering their adapters. No public tool or HTTP boundary is
reinterpreted. Optional/default/rest parameters are implemented below. Constructors/
method receivers, arguments, bind/apply and broader dynamic functions remain separate work.
Tests validate recursion and private scope, alias/import identity, mixed target
calling conventions, receivers and lexical captures, escaped roots under GC,
physical pairs and slot limits, exceptions/finalizers/fatal recursion, forged self
metadata/environment before effects, helper purity, permissions/host ceilings
and public/source-free native execution.

### Optional, default and rest parameter contract

This additive v12+ compiler increment retains the host/value/schema/exception
versions, managed layouts, helpers, capabilities and artifact protocol. Earlier
profiles reject the syntax. Fixed signatures keep their physical types and
behavior. Logical function types now track optional positions and a final rest
mode in addition to receiver, parameter types and result. Closed dispatch requires
those modes to match as well as the complete physical call signature, preventing
rest/fixed code substitution even when both physically take one array handle.
No parameter variance or implicit function-type adaptation is introduced.

The source subset admits identifier parameters with `?`, identifier defaults and
a final typed array rest parameter in declarations, arrows, anonymous/named
expressions and function types. Defaults are implementations rather than part of
a function-type syntax. They follow the supported portions of
[parameter initialization](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-functiondeclarationinstantiation)
and [TypeScript optional/rest parameters](https://www.typescriptlang.org/docs/handbook/2/functions.html#optional-parameters).
Arguments, overloads, unrestricted dynamic arity and generic function syntax
remain unsupported. Bounded spread calls and parameter patterns follow the contracts below.

For optional/default T, the incoming call slot admits T|Undefined using existing
closed union layouts. Optional body bindings keep that union; a default body
binding uses its annotated/contextual T. Defaults fire only for omission/Undefined,
not Null, zero, false or NaN. A type context may supply an unannotated parameter
and its optional/rest mode. Scalar literal defaults infer Number/String/Boolean/
Null (including parentheses/negative numeric literals); other expressions require
an annotation or context. Types outside the existing undefined-union model,
including Function and physical VartaResult pairs, cannot be optional/default
parameters. They remain valid required parameters in extended functions.

Compute minimum arity through the last required fixed position. A default before
a later required parameter still requires that argument position; its public
parameter is required and undefined-capable, matching a required T|Undefined
function-type slot. Optional `?` followed by a required parameter is rejected.
Missing optional tail values become canonical Undefined; extra fixed-call
arguments still fail. A rest parameter must be last and have one checked array
type. Its tail arguments are checked/widened in order and packed into a fresh,
fully present dense array, capped at 1024 elements; empty tails also get fresh
identity. Rest children retain their usual reference identity. Packing occurs
before callee entry, so its bounded allocation/resource failure cannot start
default initializers. Source-supplied argument effects always precede defaults.

Call lowering handles declarations, function values and Function.call through
one argument normalization path. Receiver effects remain before positional
arguments. Named self/receiver/environment slots retain their order and count
toward physical limits; rest contributes one array slot, while required physical
result pairs keep both slots. Logical function types distinguish required versus
optional positions and rest versus ordinary arrays. Default implementation details
are not a variance conversion.

Extended functions reserve incoming ABI locals separately from body bindings.
Root incoming parameter payloads before any entry/capture allocation. Declare
all source parameter names uninitialized, then initialize them left to right.
Earlier bindings and typed this/private self/outer bindings are visible; reading
or capturing self/later uninitialized parameters fails statically under the
existing checked TDZ policy. The body is lowered only afterward, keeping its
lexical declarations out of initializer scope. Defaults lower once through the
ordinary expression/call/exception pipeline; their values are type checked before
publishing that parameter binding. Present values are safely narrowed/unboxed;
optional/rest/required bindings transfer their canonical incoming payloads.

Captured body parameters use ordinary managed cells created only on their
initialization branch. Earlier cells remain live/mutable through later initializer
closures and escaping callbacks. A lexical prologue scope clears incoming ABI
roots after successful transfer, while body binding roots, receiver and private
self remain intact. This avoids retaining stale argument graphs for the rest of
the activation. Strings/records/arrays, closure graphs and required multi-slot
results preserve their exact roots and return/finalizer ownership. Source throws
skip later initializers and body effects; resource traps/authorized exit remain
fatal. Pure normalization/default tests preserve system status; actual initializer
or tail-argument host calls still require both package and host grants, and helper
purity follows their real Wasm call edges.

Array traversal supplies the actual three value/index/array arguments (four for
reduce). A fixed prefix still ignores trailing implicit arguments. Optional extra
positions receive Undefined/defaults; rest captures the remaining values with
checked element types, including the heterogeneous source array if captured.
Empty arrays still validate function metadata but do not execute defaults. Inline
callback defaults use parameter contexts and body result inference as before.
Reduce keeps an exact accumulator result; named optional/rest-first reducers use
their result as seed context, avoiding an accidental Undefined widening from an
optional accumulator parameter. Explicit inline accumulator annotations, including
default patterns, continue to provide seed context.

Main stays parameter-free and legacy Tool/HTTP entry contracts retain their exact
required signatures. Existing type/function/code/frame/root/fuel/deadline/host-call/
memory bounds remain active. Tests cover optional/default/required ordering,
undefined/null distinctions, fresh rest arrays and type/capacity checks, scopes/
captures/self/this, exceptions and fatal initialization, GC with incoming tails
and physical pairs, mode substitution, helper purity, source diagnostics, profile
gates, module/public APIs, host ceilings and source-free native execution.

### Bounded spread-call contract

V12+ accepts checked positional spreads for declarations, typed function values,
closures and `fn.call(explicitThis, ...arrays)`. Each spread operand must be a
narrowed checked Array(T). Empty/all-hole literals receive an element context
when available; absence of values otherwise uses Undefined. String/general
iterables, implicit/spread receivers, host/built-in/Array method spreads, array/
object spread literals and arbitrary union discrimination/coercion remain outside
this increment. Parameter patterns are supported below. Existing profile/entry contracts
and heap/schema/host versions are unchanged.

Evaluate callee, explicit receiver and all positional operand segments once in
source order. Each array spread immediately uses the existing pure shallow slice
helper to snapshot length, presence and elements before later operands run. Thus
later mutation of the source cannot change already-expanded argument values;
referenced children retain their identity and observe ordinary child mutation.
Sparse holes consume canonical Undefined, including present Undefined entries in
the resulting fresh dense rest array. Earlier dense profiles retain their layouts.
Snapshot allocation/operand throws stop later operands, and fatal resource limits
remain fatal. All source effects finish before dynamic count/type validation or
callee initialization, unless an operand itself fails.

A generated wrapper receives rooted callee/receiver/explicit operands/private
snapshots, retaining physical widths (including required Function/VartaResult
pairs). Validate callable metadata before expansion. Sum flattened length and
compare against shared `abi::MAX_SPREAD_ARGUMENTS = MAX_ARRAY_ELEMENTS = 1024`;
excess emits a fatal Wasm trap, bypassing guest catch/finally. Dynamic count below
the last required position or above a fixed signature throws owned TypeError
`Invalid spread argument count`. It cannot run default initializers or target
body code. Each wrapper counts toward the 256 function and 65536-byte body limits;
its operand segments, callee and receiver count toward the 32 physical-slot limit.
Memory/root/fuel/deadline/host-call bounds remain unchanged.

Expand snapshots into typed private fixed slots and one fresh dense rest array,
without a heterogeneous all-arguments object. Missing optional tails initialize
to Undefined; fixed explicit prefixes use known positions, while subsequent
values dispatch by their dynamic position. Exact types and declared union
widening are accepted. An Undefined-capable element may narrow to its compatible
non-Undefined member set; absent Undefined throws TypeError if that destination
requires a present value. Other incompatible element types throw owned TypeError
`Invalid spread argument type`; arbitrary union tag matching is not introduced.
Validate every supplied slot before invoking the existing bound receiver/function
path, so neither defaults nor body effects begin on count/type errors. Supplied
source effects precede these errors. Rest packing uses bounded preallocation and
present writes in sparse profiles, existing bounded push in dense v12.

Generated calls retain exact roots for snapshots, receiver/callee closures,
children, fixed payloads, rest output and physical pairs across allocation,
collection, exceptions and finalizers. Default omission tests and spread
Undefined narrowing use pure guest union-tag checks rather than consuming one
host operation per element. Actual operand/target host effects still require real
package and host grants, retain status semantics and remain visible through the
Wasm call graph to helper-purity validation. No new host import/helper/layout or
ABI version is introduced. Tests cover default-fuel 1024-value calls in v12/v13/
v14, sparse/default behavior, evaluation/snapshot/identity order, errors/fatal
limits, GC/pairs, forged dispatch, helper purity, diagnostics, virtual modules,
source-free Wasm/native launches, status, permission ceilings and authorized exit.

### Parameter destructuring contract

V12+ declarations, arrows, anonymous/named expressions, callbacks and typed
function values accept recursive object/array binding patterns. Function-type
parameters may carry matching aggregate patterns without changing the logical
signature. Source annotations or function/callback context supply the incoming
aggregate type; destructuring never changes call arity, optional/rest modes or
physical receiver/self/environment/result slots. Main and legacy Tool/HTTP entries
retain their contracts. Earlier profiles reject aggregate parameter patterns.

Admit closed record object patterns with shorthand bindings, static aliases,
string/number literal keys (including computed literals), nested defaults and a
final identifier object rest. Array patterns admit nested bindings/defaults,
elisions and final identifier/aggregate rest; final parameter rest can itself be
an array pattern. Dynamic keys are rejected before lowering effects. Primitive
boxing, strings/general iterables, array-as-object patterns, broader prototypes
and unrestricted unions remain outside this increment; local declarations and the
narrower catch object pattern contract are specified below. Shared function constants bound parameter pattern depth
to 64 and nodes (including elisions/wrappers) to 1024 per function; existing parser
complexity, semantic type, physical slot, emitted body, frame/memory/root/fuel/
deadline/host-call limits still apply independently.

The admitted semantics follow supported portions of ECMAScript
[BindingInitialization](https://tc39.es/ecma262/2025/multipage/syntax-directed-operations.html#sec-runtime-semantics-bindinginitialization),
[IteratorBindingInitialization](https://tc39.es/ecma262/2025/multipage/syntax-directed-operations.html#sec-runtime-semantics-iteratorbindinginitialization)
and [property/rest binding](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-destructuring-binding-patterns).
Collect all BoundNames before lowering any initializer, declare them uninitialized,
and reject duplicate/reserved names or own/later reads/captures using real source
spans. Reserve rooted incoming ABI slots separately whenever a pattern needs
initialization. Initialize leaves in parameter/property/element order, updating
the lexical binding only after its value is ready. Earlier names, initialized
this/private self and outer bindings are visible; body declarations are not.
Captured leaves use ordinary shared managed cells, including mutation from later
defaults and escaping/sibling closures. Lexical scopes release only hidden
aggregate/default temporaries and incoming roots after transfer, preserving leaf
roots, captured cells, receiver/self and physical result-pair ownership.

Top-level defaults retain their declared/contextual parameter type and trigger
only on Undefined. Nested defaults remove Undefined from the present type and
join admitted fallback types without Null/false/zero/NaN coercion. Aggregate
literal defaults receive the corresponding available type context; uncontextual
empty literals still need a type. Lower each default once; own/later references
are rejected even on statically present paths. Pure guest Undefined/nullish tag
tests preserve system status and do not spend host calls per binding. A nullable/
undefined-capable aggregate is checked before unboxing; Null/Undefined throws an
owned TypeError `Cannot destructure null or undefined`. Throws stop all later
bindings/defaults/body effects, preserve outer finalizers, and retain existing
fatal budget/authorized-exit behavior.

Object field reads occur at each binding's position, so prior default mutation
is observed. Missing non-inherited closed keys yield Undefined; inherited reads
without a declared own field remain unsupported. Duplicate property keys with
distinct bound names are permitted and re-read in source order. At object rest,
copy the remaining typed payloads into a fresh rooted record, then transfer the
live presence mask and filtered enumeration order through a pure bounded Wasm
statement. No allocation/user code occurs during metadata transfer. Keep canonical
zero unused order entries, optional flags and all 32 presence bits; absent fields
stay absent, explicit Undefined stays present. Child objects keep their identity.
The target owns the existing record layout and can be mutated independently.

Array patterns advance a per-pattern index over live length/backing/presence;
elisions advance without reading the value. Track exhaustion persistently: once
done, later source growth cannot resume iteration. Each element read admits
Undefined for holes/end-of-input; nested aggregate patterns guard that value
before reading. At rest, capture the current remaining length, allocate a fresh
bounded dense array and copy remaining values in order. Sparse v13+ rest types
admit Undefined and publish holes as present Undefined entries; dense v12 retains
the source element type. Rest has no user callbacks while copying and preserves
child identity. No general iterator/prototype protocol is introduced.

The generated prologue uses the existing checked IR, dispatch, managed heap,
shadow roots and exception channel. Pure record metadata transfer and tag tests
add no import/helper/layout/schema/host/exception/artifact ABI version. Actual
default effects remain visible in Wasm helper-purity analysis and require both
package and host grants. Tests validate actual Wasm and cover nesting/renaming/
keys/defaults/rest, live mutation/exhaustion, type inference and callbacks,
TDZ/scopes/cells/self/this, nullish and thrown errors, GC/result pairs, all 32 record
bits, 1024-element rest/default callbacks, bounds/profile gates, helper purity,
virtual modules and source-free artifacts with status/permissions/authorized exit.

### Local lexical destructuring contract

V12+ local const/let declarations, including ordinary for initializers and explicit
case blocks, share parameter pattern lowering. Reuse static-key validation,
recursive defaults/aliases, elisions, object/array rest, live sparse reads and
persistent exhaustion. Root/source/default/copy ownership, nullish owned TypeError,
shallow child identity, record presence/enumeration and fatal resource semantics
remain those of the parameter contract. No second pattern interpreter, host
operation, helper, managed layout or ABI version is introduced. Earlier profiles
reject local aggregate patterns; ordinary identifier declarations keep their
existing annotations, undefined-capable let initialization and physical pairs.

Predeclare every leaf in the enclosing lexical block before lowering statements;
ordinary for initializers own a separate loop scope. Reject duplicate/reserved
names and unsupported dynamic keys before initializer evaluation. Infer the input
aggregate from its initializer, or use the whole-pattern annotation as context
and validate its exact admitted type. Require an initializer for every aggregate
pattern. Do not evaluate an initializer during predeclaration. Evaluate each RHS
once at its declaration position, then initialize leaves in source order using
the existing managed binding-cell path. Earlier initialized leaves/declarators
are visible to later defaults; own/later reads or captures remain checked TDZ
errors, including shadowing an outer name in an RHS. Source identity and the
innermost available expression range are retained in diagnostics.

Keep const/let mutability on each predeclared leaf: const rejects rebinding in the
owner or a captured closure; property/element mutation through an aggregate still
uses ordinary checked setters. Let retains invariant assignment types, shared
capture cells and existing guard safety. Const union guards remain stable through
captures; mutable captured union narrowing is rejected, even when capture appears
after the guard. Separate lexical activations/shadowed blocks own distinct cells,
and escaping aliases retain their live graphs after scope roots clear. Hidden
input/default/rest scopes release only their temporary roots after binding transfer;
no synthetic aggregate binding leaks into source scope.

Ordinary for let loops follow the admitted portions of
[ForBodyEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-forbodyevaluation).
Rebind captured loop leaves after initialization and before the first test, then
before each update. This separates closures created by the initializer from the
first iteration as well as separating subsequent iterations. Copy each leaf's
current value/reference identity using the existing managed cell operation; only
captured cells allocate. Continue/finally reaches rebinding and update, while
break/return/throw preserves escaped prior cells and exits normally through scope
cleanup. Const initializers have no per-iteration environment, matching their
immutable binding lifetime. Repeated body declarations create fresh activation
cells through ordinary initialization. Array for-of is supported below; for-in
remains unsupported; labels are supported below.

Shared `MAX_BINDING_PATTERN_DEPTH/NODES` bound a lexical declaration (all its
declarators) to depth 64/1024 nodes; parameter constants retain aliases to the same
limits. Parser complexity, semantic type, body, frame/root, memory/fuel/deadline
and host-call bounds remain independent. Var/ambient, top-level, catch,
primitive-boxing/string/custom-iterator and broader dynamic-key patterns remain
unsupported. Actual initializer/default host calls remain visible to helper
purity analysis and require real package and host grants; pure binding/rest
operations preserve system status. Tests cover nesting/inference/context, once-only
RHS/order, TDZ/duplicates/diagnostics, mutability/shadowing/closures/this/self,
initializer/iteration environments, finally and fatal paths, GC/physical pairs,
default-fuel 1024-element copies, legacy/profile bounds, virtual modules and
source-free Wasm/native artifacts with granted/denied operations and host ceilings.

### Checked destructuring assignment contract

V12+ admits object/array assignment patterns targeting existing initialized
mutable local/parameter bindings, including captured outer cells. Support
standalone statements, arbitrary enclosing parentheses and ordinary for
initializer/update statements. Nested defaults/patterns, static keys/aliases,
elisions and final object/array rest share the existing checked binding path.
Expression values/chains and checked member targets are implemented below. Other
expression targets, dynamic record keys in patterns, catch and for-in patterns remain
unsupported; diagnostics name these boundaries.
Earlier profiles reject the pattern assignment path. No ABI/import/helper/layout
change or indirect-call/host-effect exception is introduced.

The admitted source order follows the supported portions of
[DestructuringAssignmentEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-runtime-semantics-destructuringassignmentevaluation).
Validate the syntax and shared depth/node limits (64/1024 per assignment) before
lowering. Resolve every leaf against the current lexical scope, enforcing existing
mutability, initialization, capture and guard restrictions with actual target
spans. Do not predeclare names or change binding types. Repeated references are
permitted and write the same slot/cell sequentially. Evaluate/root the complete
RHS once, then traverse the original aggregate even if targets change aliases.
Defaults execute at their position and see previous successful writes or their
own prior binding value. A runtime throw stops later reads/defaults/writes, while
earlier writes remain visible to catch/finally and escaped closures.

The shared traversal carries an explicit initialize/assign mode through every
nested pattern/default/rest. Only assignment leaves emit Set, retaining the
existing managed cell instead of creating a new activation. Hidden source,
default, rest and narrowed-value temporaries still use rooted initialization and
lexical cleanup. Keep source graphs live across overwritten aliases and later
allocations/collection, and transfer new payloads/status pairs through the normal
binding writer. Function and physical result defaults can populate existing
compatible leaves without entering aggregate value tags. This/private self and
transitive/sibling captures retain ordinary semantics.

Leaf types remain invariant, admitting exact values and declared union widening.
For an Undefined-capable value whose non-Undefined members fit the target, emit a
pure guest Undefined test: Undefined throws owned TypeError
`Invalid destructuring assignment value`, and present values narrow/unbox/widen
before Set. Other statically incompatible types fail compilation; arbitrary union
discrimination/coercion is not introduced. Defaults receive the leaf target
type as context and are checked before publication. Homogeneous array targets
provide an element context (including Undefined for fixed slots), while a rest
target retains its exact element type. Opaque Function/result targets do not
invent array element kinds. Ambiguous nested/empty RHS aggregates require an
independently typed source. No record context guesses away unselected RHS fields.

Live array length/backing/presence, persistent exhaustion, present dense rest,
record rest presence/enumeration and shallow identity use the existing traversal.
Nullish nested aggregates throw the existing owned TypeError. All resource limits
and authorized exit remain fatal; no catch/finally conversion is added. Pure
checks/copies preserve system status. Actual RHS/default calls require package
and host grants and remain visible to Wasm helper-purity validation. Tests cover
once-only RHS, order/duplicates/defaults/aliases/rest, context/type/mutability/TDZ/
guards, captured cells/this/self/function/results, partial failures/finalizers,
GC, default-fuel 1024-element copies, bounds/legacy gates, virtual modules and
source-free Wasm/native artifacts with real grants, host ceilings and target
source diagnostics.

### Destructuring member target contract

V12+ Assign mode admits checked record-field and numeric array-element leaves,
including final object/array rest targets. Initialization mode continues to
accept binding patterns only; object binding rest remains an identifier. Resolve
member places through the ordinary checked place lowerer: static record keys,
Number array keys, invariant field/element types, no Error/result mutation or
unresolved nullable base. Const receivers permit child mutation. The shared
64-depth/1024-node pattern budget applies before lowering; receiver/key expressions
retain ordinary parser/source/body/frame bounds.

Follow the admitted portions of
[KeyedDestructuringAssignmentEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-runtime-semantics-keyeddestructuringassignmentevaluation)
and IteratorDestructuringAssignmentEvaluation: prepare each non-pattern reference
before GetV/iterator step and default evaluation. Rest prepares its reference
before copying/draining. Nested patterns first obtain their source value and only
then prepare their own leaves. Do not eagerly prepare all pattern references.
Elisions, live length/backing reads and remembered exhaustion remain unchanged.

Lower a prepared place into hidden rooted receiver/key slots and a typed local,
field, index or length store. Read/default/check the source only after preparation,
then emit the ordinary checked setter against the saved reference. Never lower or
evaluate the member again after default execution. Default values use target
context and the existing pure Undefined check; optional field writes retain
presence/insertion order. Resolve current array backing at commit, so key/default
side effects can resize it without stale addresses. The scope covers reference
preparation, source reads, defaults, rest allocation and the final write; normal
unwinding cleans it. Expression temporary floors still protect preceding operands.

V12 existing-slot writes and v13 hole filling keep their bounds; v14 supports
extension and length targets through the existing setters. Invalid uint32 length
raises owned RangeError. Numeric index validation remains after RHS/default
execution, and valid lengths above 1024 or invalid checked indices remain fatal.
No new layouts/helpers/imports/ABI versions or privileged paths are introduced.
Actual receiver/key/default effects remain visible to helper-purity validation,
package grants and host ceilings. Prior writes survive later recoverable errors;
remaining references/defaults are skipped. Success returns the original RHS.
Tests validate Wasm execution, reference order/identity, GC, current backing,
rest/presence/order, partial/fatal failures, profile gates, public virtual modules
and source-free Wasm/native artifacts with grant denial and dependency diagnostics.

### Array for-of contract

V12+ admits intrinsic for-of over checked arrays (including a checked array type
with null/undefined). Support one const/let declaration without an initializer,
or a checked identifier/member/object/array assignment head. Var/using, for-in,
for-await-of, strings and arbitrary iterables remain unsupported. Earlier
profiles reject the syntax. Head annotations are rejected by the TypeScript parser;
ambiguous empty RHS literals need a typed source. Shared pattern/source/frame
bounds and invariant binding/target types continue to apply.

The admitted order follows
[ForIn/OfHeadEvaluation and ForIn/OfBodyEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-runtime-semantics-forinofheadevaluation-tdznames-expr-iterationkind)
and the intrinsic
[array value iterator](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-%arrayiteratorprototype%.next).
Predeclare lexical head names in their own TDZ scope before lowering the RHS;
reject own/later reads and captures even if an outer name exists. Evaluate/root
one source value. Narrow its unique array member only after a pure nullish check;
nullish values throw owned TypeError `Cannot iterate null or undefined` before
head/body effects. Incompatible non-array sources are compile errors.

Lower through existing Scope/For/Init/Set IR. A hidden numeric index tests live
length at each step, reads the current backing and saves the element before any
assignment-reference/default effects. V12 dense reads retain the declared element
type; v13+ nullable reads visit every index and box holes as Undefined. No snapshot
length, presence filter, user iterator object or return hook is introduced. The
index increases once after normal body/continue; break/return/throw never restart
the iterator. Source-variable reassignment cannot change the rooted array identity.
Growth can add visits, shrink can end them and later writes/deletions are visible.

Each step owns a scope containing the current value and lexical head leaves.
Initialize creates fresh managed cells for captured let and const bindings every
iteration; do not clone the previous iteration or reuse its cells. Shared pattern
lowering preserves TDZ, default/rest semantics and captures. Assignment heads keep
existing cells and use the checked setters; only after the element is rooted do
their receiver/key effects execute. Undefined-capable assignments retain the
existing checked narrowing and partial-write behavior.

Ordinary loop labels and completion/finalizer routing clear iteration/body roots
on continue/break/throw/return, while source roots live until loop exit and escaped
captures own their cells. Finalizers still see the current iteration's bindings
before cleanup; existing saved returns, physical pairs and callback-return
inference remain intact. Fatal resource limits/exit bypass handlers unchanged.
Pure traversal preserves status; actual source/head/default/body effects stay
visible to helper-purity validation, grants and host ceilings. No ABI, import,
helper or heap-layout change. Tests cover live mutation, holes, captures/TDZ,
patterns/targets, finalizer routing, GC/pairs, 1024-element default budgets,
legacy gates, virtual modules and source-free artifacts with real permissions.

### Do-while contract

V12+ admits checked do-while with the existing condition truthiness and lexical
body scopes. Earlier profiles reject it; labels are supported below. Follow
[DoWhileLoopEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-do-while-statement-runtime-semantics-labelledevaluation):
run the body first, then evaluate the condition exactly once after normal or
continue completion. Break/return/throw skip it unless an intervening finalizer
replaces the completion. Nested loops/switches retain ordinary branch targets.

A dedicated typed IR statement retains both body and condition. Lower the body
with an active loop target, restore its lexical scope, then lower the condition.
Emit an outer break block, loop header and inner continue block; continue exits
the inner block to reach the condition, while break exits the outer block. After
condition evaluation/truth conversion, restore the temporary-root floor before
branching to the loop header. Existing branch/finalizer emission performs lexical
cleanup and preserves current bindings through finalizers. Each execution of a
body-block declaration initializes fresh capture cells; outer cells remain shared.

Completion analysis must account for guaranteed first execution. Propagate body
return/throw possibilities; admit normal loop exit only when the body can finish
normally, break or continue. Unconditional first-body returns/throws therefore
satisfy function completion checks, while possible break/continue paths do not.
Callback inference visits do-while bodies and uses that completion result, keeping
exact return types when no normal exit exists and Undefined on possible fallthrough.
Finalizer overrides, deferred saved returns and physical result pairs retain their
existing behavior. Constant-condition infinite-loop inference is not introduced.

Layout/helper/effect visitors traverse body and condition, including transitive
host calls. Pure control flow preserves system status; actual effects require
package grants and any host ceiling. Memory/root, fuel, deadline, host-call and
source/body/frame bounds remain unchanged, and resource traps/authorized exit
bypass guest handlers. No new import/helper/layout/ABI version. Tests cover order,
truthiness, abrupt completion, finalizer overrides, captures/GC, bounded condition
roots, callback inference, legacy gates, dependency diagnostics, virtual modules
and source-free Wasm/native artifacts with grant denial and host ceilings.

### Statement label contract

V12+ supports labels around admitted statements and named break/continue. Follow
[LabelledEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-labelled-statements-runtime-semantics-labelledevaluation).
Labels have a control namespace separate from bindings and a lifetime confined to
one function and the labeled statement. Reject unknown labels, active duplicates
and named continue to non-iteration statements. Sibling reuse is valid. Chains of
labels immediately enclosing a while/do-while/for/array-for-of share its iteration
target; enclosing a block/if/switch does not make a continue target. Label nesting
uses the existing 64-level scope/expression budget. Earlier profiles reject labels.

Lowering assigns unique per-function IDs, retains named branch IDs in typed IR
and resets label state for every lifted function/generated wrapper. Labels do not
create lexical environments. Emission wraps each labeled statement in a block
that is not an unlabeled break target; a map resolves its named break ID to that
block. Iteration label chains bind to the actual loop's continue target: while
header, do-while trailing-condition block, for update block, or for-of step/update.

A source for/for-of takes its pending iteration labels before emitting any
initializer. Generated destructuring/rest loops in an initializer or source
expression therefore cannot consume the label chain. Named targets use stable
active block indices and the existing complete-branch/finalizer machinery; no
source name or guest integer becomes a dynamic branch address. Remove mappings
when the corresponding statement/loop ends. Unlabeled branch selection is unchanged.

Crossing a finalizer routes the exact target through existing completion slots,
runs inner finalizers before outer ones and permits completion overrides. Clear
roots only for scopes actually left: for continue keeps the loop environment
until its update/rebind, for-of clears iteration heads but retains its source,
and named break also clears the loop scope skipped by its enclosing label block.
Escaped cells remain retained by their closures. Saved returns/result pairs,
exception channels, GC and resource/exit bypass behavior keep their contracts.

Completion analysis carries a bounded set of named break/continue IDs alongside
ordinary completion kinds. Inner loops/switches consume only unlabelled branches;
named branches propagate until the matching label. Sequence analysis retains
unreached named paths, and finalizers replace body paths when they cannot complete
normally. Matching break/iteration-continue admits normal completion, preserving
sound function-return and callback-Undefined inference. Visitors traverse label
bodies for returns, layouts, helpers and transitive effects. Real grants, status,
all source/frame/root/resource bounds and existing ABIs remain unchanged.
Tests cover aliases, block/switch labels, all loops, generated rest loops, finalizer
overrides, captures/GC/pairs, invalid targets/depth/legacy gates, virtual modules
and source-free artifacts with grant denial, host ceilings and source diagnostics.

### Catch binding pattern contract

V12+ adds checked object CatchParameter patterns, following the admitted parts of
[CatchClauseEvaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-try-statement-runtime-semantics-catchclauseevaluation).
The exception channel remains the complete primitive/Error-family union; never
infer it from the protected body or admit aggregate throws through a pattern.
Support empty object patterns and static name/message keys with identifier leaves,
aliases and defaults. Literal computed keys use existing key normalization. Reject
other properties, dynamic keys, array/rest patterns, annotations, invalid leaves,
duplicates and older profiles with located diagnostics. Existing source/pattern,
type/frame/root/body and execution budgets remain active.

Predeclare every BoundName uninitialized and mutable under the shared pattern
budget before lowering any default. Initialize leaves in source order with the
existing typed default machinery; direct self/later reads remain in TDZ. Handler
body declarations do not participate in default resolution, but duplicate lexical
names remain errors. Captured leaves receive fresh managed cells per catch entry.
Before clearing the exception channel, the existing TryCatch emitter transfers the
original union into one hidden exact root. Include that root in the complete
handler scope even when there are no named leaves. Clear it and partially
initialized bindings on every normal/branch/return/throw exit.

After channel transfer/clear and before any property/default effect, reject
null/undefined with owned TypeError `Cannot destructure null or undefined`. The
handler catching the original value is already inactive, so binding failures
escape to an outer handler and run applicable finalizers. This applies to empty
object patterns too. Non-null primitive throws supply Undefined for name/message;
Errors use existing family-aware string fields, including inherited empty message.
Both field types are String|Undefined; defaults remove Undefined only through
existing union rules, without casts or narrowing based on the try body.

Internal IsError tests the canonical union member tag without a host call; only
its true branch may read Error payloads. Existing rooted immutable union widening
retains property strings through further allocation. Layout/effect visitors walk
the predicate and full default/handler statements; callback completion inference
accounts for those statements without inventing normal exits. No new host import,
helper export, exception version or heap/ABI layout is introduced. Status, exact
grants, host ceilings, saved exceptions/returns/physical pairs, finalizer override,
named control routing and fatal resource/exit behavior remain unchanged. Tests
cover TDZ/scopes/captures, nullish/default failures, repeated GC/roots, callback
inference, modules, source-free artifacts and dependency diagnostics.

### Dynamic record own-key contract

V12+ admits Object.hasOwn(record,key) with a narrowed closed-record source and a
computed String key. Require exactly two arguments and an unshadowed Object.
Preserve the admitted own-property semantics of
[Object.hasOwn](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-object.hasown):
key expressions run before the presence test. There is no ToPropertyKey coercion,
prototype traversal, getter invocation, payload read or JSON serialization. Unknown
keys return false; present Undefined differs from absence. The result alone does
not refine an optional field value. Required inherited-looking names still refer
to actual own slots. __proto__ lookup does not enable prototype initialization.

Literal String keys retain a direct slot/presence path in v12+. Metadata names are
UTF-8 and cannot contain lone surrogates, so unrepresentable literal names return
false without changing shape constraints. Preserve existing numeric-literal key
rules and every older profile's behavior. Dynamic non-String/union keys and
unchecked/nullish sources without narrowing remain rejected, as do symbols,
generic/spread calls. Array/Error extensions below preserve their earlier
profile boundaries.

V12+ extends computed String own keys to narrowed Error families and Error-only
unions. Evaluate/root receiver and key before comparing the exact UTF-16 key to
message, then mask ERROR_OWN_MESSAGE independently of the Error class bits. An
omitted/Undefined message is absent; an explicit String, including empty, is own.
Name, stack, cause and inherited/unknown names are absent. Literal String keys
retain a direct path, including false for lone-surrogate/__proto__ names. V11 and
earlier retain literal-only Error keys.

V13+ accepts computed String own keys on bounded sparse arrays. The private,
allocation-free helper recognizes length as own even on an empty array; otherwise
it accepts only canonical ASCII decimal indices without leading zeros except 0.
Reject overlong names before scanning; scan at most the decimal width of the
maximum bounded index. Digit checks precede accumulation; the bounded value cannot
overflow. Reuse ArrayPresent for current-length/1024 bounds and the existing bitmap,
so holes differ from explicit Undefined, independent of payload type or capacity.
V12 and earlier retain their array own-key boundary. Numeric array keys keep
existing finite/integer/range checks and numeric -0 semantics. Number|String unions
require narrowing; no ToPropertyKey coercion is added. Array/key roots survive
key calls, resizing/reassignment, exceptions and GC; key mutation is visible on
the captured original receiver before lookup. Visit both expressions in helper
collection and effect analysis. No host import/export, ABI or layout change.

Dynamic lowering uses the existing typed value block with hidden receiver/key
roots, saved temporary floor and initialized Boolean output. Register roots before
source evaluation, then evaluate source and key once in order. Key calls may
mutate presence on the retained record or reassign the caller's source binding;
lookup observes the former and retains the original receiver identity. Empty
shapes still evaluate both arguments. Throws skip later arguments/enclosing work;
handlers/finalizers clear inputs and partial temporaries through existing routes.
Load the canonical Boolean result before cleaning hidden roots without allocation.

Build a balanced decision tree over field names sorted by unsigned UTF-16 units.
Do not use UTF-8 metadata order for String comparisons: supplementary/BMP ordering
differs. Retain original shape slot IDs so high presence bits and JSON/order data
remain unchanged. Equality and order reuse the pure UTF-16 comparator; a matching
leaf reads the existing presence bit, while an unmatched path leaves false.
Only the ordering of the decision tree changes; records retain their layouts.
At most O(log F) comparison nodes are visited for F <= 32 fields, with comparison
cost bounded by String/key-name limits and existing execution fuel. Deduplicated
name literals count against the normal static-data ceiling. Lookup allocates no
managed values beyond ordinary argument evaluation, and adds no host operation,
private helper export, schema or ABI/layout change.

Existing value-block layout/effect visitors cover the entire lowered body;
callback inference uses the resolved Boolean type. Preserve GC/earlier operands/
physical pairs/saved returns, exact status/grants/host ceilings and all fatal
source/type/body/frame/root/memory/fuel/deadline bounds. Tests cover optional
presence, Unicode order and bit 31, key-side mutation/throws, empty shapes,
JSON/rest/callbacks, GC/finalizers/repeated roots, helper purity, older profiles,
virtual modules, source-free artifacts and dependency diagnostics.

### Computed record read contract

V12+ admits `record[key]` reads with a narrowed closed-record receiver and narrowed
String key. The result type is the union of all declared field types and Undefined,
flattened and interned with the existing union representation. Empty shapes return
Undefined. Existing representability limits apply: at most one record shape and
one array element type per union, with admitted primitive/Error members. Reject
unsupported result unions at the access span, without erasure or casts. Context
cannot narrow an arbitrary String key or hide other possible field kinds.

Follow the admitted source-order and own-data-property parts of
[computed property evaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-evaluate-property-access-with-expression-key)
and [OrdinaryGet](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-ordinaryget).
There is no ToPropertyKey coercion, getter or prototype traversal. Lookup of a
declared own field reads its current typed payload only if present; absent optional
fields and unknown non-inherited names return Undefined. Preserve explicit
Undefined presence, Number bits and shallow reference identity. Known inherited
names without a declared own field raise owned TypeError `Inherited record property
access is unsupported`, after key effects. This checked boundary is intentionally
stricter than ordinary prototype lookup; do not return a fabricated Undefined.
Required own inherited-looking fields still read normally. The shared inherited
name table keeps static rejection and dynamic checks aligned. Literal String/Number
access paths, including static inherited-key rejection, retain their earlier rules.

Reject dynamic non-String/union keys, unchecked/nullish sources without narrowing,
dynamic compound assignments/destructuring targets/deletion, symbols,
assertions, index signatures and older profiles. This increment does not add
computed object initializers, destructuring keys, Array/Error computed reads or
general dynamic objects. Array numeric reads retain their own dispatch.
Simple computed record writes are specified separately below.

An existing typed value block roots the receiver and key, initializes a typed
Undefined result after both operands succeed, then dispatches a balanced tree
sorted by exact unsigned UTF-16 units. Retain original UTF-8 shape slot IDs and all
32 presence bits. Add checked inherited-name leaves to the tree; matching own
leaves test presence before loading/widening payloads. Only the selected leaf runs.
Existing union fields reuse their cells; widening scalars/references uses ordinary
bounded union allocations. This is not an allocation-free operation. Deduplicated
name literals and generated bodies count against normal static/body limits.

Evaluate source then key exactly once, including empty shapes and unknown names.
Key mutation is visible on the retained original receiver; source-variable
reassignment does not redirect the read. Register hidden roots before either
operand; preserve source, key, selected child graphs, earlier operands and physical
pairs through allocation/GC. Transfer the result before clearing internal roots,
without intervening allocation. Existing exception routes discard partial values,
skip later effects and preserve outer catch/finally and saved returns. Internal
bodies contain no source returns; callback inference uses the resolved value type.
Layout and transitive-effect visitors traverse the complete value block, including
source/key calls and inherited-key throws. Preserve actual grants, host ceilings
and system status. No new host operation/import/helper export, schema or ABI/layout
version is introduced. All source/type/frame/root/string/memory/fuel/deadline limits
remain active and exhaustion/authorized exits remain fatal. Regressions execute
Wasm, public virtual modules and source-free portable/native artifacts and cover
diagnostic ranges, profile gates, UTF-16 order, type/presence/identity, key mutation,
throws, bit 31, maximum keys, helper purity, repeated GC and finalizer/root cleanup.

### Computed record write contract

V12+ admits `record[key] = rhs` as a simple assignment statement, including outer
parentheses and ordinary for initializer/update positions. Require a narrowed
closed-record receiver and narrowed String key. Literal String/Number paths and
earlier-profile behavior remain unchanged. The value-producing extension is below.
Dynamic-key arithmetic compounds are specified below. Reject increment,
destructuring member targets, computed initializers, dynamic deletion/property
creation, index signatures, non-String/
unnarrowed union keys and unchecked/nullish receivers without narrowing. Array
numeric mutation and Error immutability keep their separate dispatch/contracts.

Preserve admitted [assignment evaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-assignment-operators-runtime-semantics-evaluation)
order: evaluate receiver, key, then RHS exactly once. Register hidden receiver/key/
value roots in a lexical scope before operand evaluation. Save the original
receiver identity and key before RHS effects, including reassignments of their
source bindings. Perform lookup, value checks and the actual store after RHS
success, including empty shapes and unknown/inherited keys. A source/key throw
skips later operands; an RHS throw skips lookup/store. Earlier mutations and system
effects are not rolled back. Existing catch/finally routes clean all partial roots,
preserve saved returns/physical pairs and retain fatal budget/authorized-exit rules.

RHS values must have admitted primitive, exact record/array or Error types, including
existing closed unions; functions and captured result pairs remain unsupported.
If a nonempty shape has no field whose type overlaps the RHS type, reject at the
RHS span before emitting code. Do not join all field types or erase shape/array
information: independently typed fields may have multiple record shapes/array
types even though a single RHS union still admits only one of each. Contextualize
an object/array literal only when exactly one corresponding field member type is
available; otherwise infer normally or require an explicitly typed value. Null/
Undefined, Number bits and reference identity receive no coercion or conversion.

Reuse the balanced UTF-16 lookup tree and original shape slot IDs. Unknown names,
including unshadowed inherited names, throw owned TypeError `Unknown computed record
write key` after RHS effects, rather than creating properties or invoking prototype
setters. Declared own inherited-looking names write normally. On a matched field,
intersect the complete static RHS members with the declared field members. Empty
intersection throws owned TypeError `Invalid computed record field value`.
For partial intersections, a private IsUnionMember IR check tests exact union tags;
Error members additionally check the exact class bits only after the Error tag
matches. Record/array tag checks are safe only after full static shape/element-type
identity matches, since an RHS union has at most one of each. Do not treat Error
families or two different array/record types as interchangeable.

Only the successful branch may unbox/retype the proven subset, widen to the exact
field type, store through the existing SetField path and publish presence. A failed
type check must not store payloads or change presence/order. An absent optional
field becomes own even when storing Undefined; first presence appends insertion
order and replacement preserves position. Integer-index names still enumerate
first numerically. All 32 slots/bits retain their interpretation.
Existing union cells are reused when compatible; scalar/reference widening uses
ordinary bounded allocations. Retain the prepared receiver, key, RHS and child
graphs through checks, allocation and GC, and clear lexical roots on every exit.
No managed shape/array/union metadata, schema or ABI version changes.

IsUnionMember allocates/calls nothing and returns a canonical Boolean; its operand
must remain visible in layout/effect visitors. The full value-block/Init/lookup/store/
throw body is traversed for actual effects and helper purity. Preserve exact grants,
host ceilings and system status, with no new host operation/import/helper export.
Static-data/type/source/body/frame/root/string/memory/fuel/deadline limits remain
active. Tests execute Wasm, public virtual modules and source-free portable/native
artifacts, including profile/literal boundaries, source ranges, UTF-16 ordering,
invariant shapes/arrays, Error families, optional ordering/bit 31, source/key/RHS
effects and throws, GC/partial cleanup/saved returns and pairs, helper purity and
fatal limits/authorized exits. This is a checked assignment boundary, not general
ECMAScript object mutation or prototype support.

### Computed record assignment value contract

V12+ extends computed closed-record assignment to expression positions. Return the
original RHS value and resolved type only after a successful store, following the
admitted [assignment evaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-assignment-operators-runtime-semantics-evaluation)
and its returned RHS value. Dynamic String keys retain the preceding write contract.
Computed literal String/Number keys retain static field lookup, known-field type
checks and unknown/inherited-key compile errors. Require narrowed closed-record
receivers; reject non-String dynamic keys, unnarrowed unions/nullish receivers and
increment. Dynamic-key arithmetic compounds and local/dot/array values are
specified below. Error mutation remains unsupported. Existing statements and destructuring
expressions can consume supported computed-record RHS expressions.

Use existing ValueBlock IR for both dynamic statements and expressions, with the
cached original RHS as output. Literal computed expressions prepare/root the
receiver, evaluate the RHS once, then widen a separate local read for SetField.
Do not pre-widen the cached RHS to the selected field type: a Number stored in an
optional/union field still yields Number; an exact Error retains its family, and
record/array results retain their complete type and identity. Dynamic checks may
unbox/retype only the copy used for storage. An original union result keeps its
original static members even when a runtime store proves a narrower subset. Outer
context may widen the completed result normally, but does not change the selected
field's contract or provide arbitrary RHS narrowing/shape erasure. Aggregate literal
context follows existing unambiguous target rules, including typed empty literals.

Register input roots and save the temporary floor before evaluating any operand.
Evaluate receiver then key then RHS once, with the prepared receiver/key retained
across RHS effects and nested assignments. Chaining evaluates outer references
before inner references and the final RHS, then stores from inner to outer.
An inner failure skips outer stores, remaining arguments and enclosing work;
an outer failure preserves completed inner stores and earlier effects. Do not
roll back partial chains or publish a success result after a throw. Existing
catch/finally routes clean partial inputs and preserve saved results/exceptions.

Every internal statement may release only temporaries above the saved floor,
preserving earlier caller operands/arguments and both slots of physical pairs.
Nested value blocks save/restore the prior floor. Load the original result before
clearing hidden roots, with no intervening allocation; the enclosing expression
wrapper retains a reference result immediately. Keep prepared source/key/RHS child
graphs and escaped results alive through collection, later arguments and finalizers.
Statements discard the full returned value through the normal expression path.

Admit ordinary initializer/argument/return/member/key/operator/conditional/callback/
for-condition positions supported by the corresponding result type. Internal
bodies contain no source returns/finalizers, so callback inference uses the resolved
original RHS type. Existing layout/effect visitors traverse the entire value block,
including operands, nested stores and failures. Preserve all field compatibility,
presence/order/bit-31, literal/legacy, exact grants/status/host-ceiling and fatal
resource/authorized-exit rules. No emitter helper, host operation/import, helper
export, schema or ABI/layout change is introduced. Tests execute Wasm, virtual
modules and source-free portable/native artifacts and cover exact types/bits/
identity, chain order/partial failure, earlier operands/pairs/GC/finalizers, repeated
cleanup, callbacks, profile gates, dependency ranges and actual capability effects.

### Simple assignment value contract

V12+ extends simple `=` expressions to mutable initialized local/parameter bindings,
static declared record fields (dot or literal computed form) and narrowed arrays
with numeric indices. Dynamic record keys retain the preceding checked contract.
Allow ordinary enclosing expression positions and mixed chains with computed
record/destructuring assignment values. Preserve the admitted assignment order
and returned RHS under [assignment evaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-assignment-operators-runtime-semantics-evaluation).
The arithmetic compound extension is below. Reject update operators, const/TDZ targets, writes under guard refinement,
nullish/union receivers without narrowing, incompatible types, unknown record
fields, coercive/String array indices, immutable Error/result fields and older
profiles. Existing statements retain their profile contracts.

Share prepared Target references and storage with destructuring: resolve a local
binding/capture once, or evaluate/root the member receiver and raw numeric index
once before the RHS. Preparation never caches an interior array backing pointer.
After RHS success, perform the store against the prepared original reference and
current storage. Receiver/key-variable reassignment cannot redirect the store;
RHS mutations/resizing remain observable. An RHS failure performs no requested
outer store. Chains preserve completed inner writes on outer failure and skip
remaining outer stores/arguments. Existing catch/finally routes handle partial
roots and retain saved returns, pending exceptions and fatal exits.

Cache the exact resolved RHS type and full physical value in a ValueBlock output;
do not pre-widen it to the target union or narrow it from a runtime field choice.
Use normal non-union context for typed aggregate/function inference. For a union
target, supply only an admitted aggregate-literal member context; other RHS values
resolve independently. Require complete invariant compatibility, then widen only
the storage copy through Set/SetField/SetIndex/ArrayLength. Preserve scalar bits,
Undefined/null distinctions, Error families and actual aggregate/function identity.
Local function values keep existing signatures/captures; local VartaResult values
retain both physical slots and exact String roots. Aggregate slots continue to
reject function/result kinds through their existing type definitions.

Numeric array expressions validate after RHS evaluation, before address conversion
or mutation. Require finite integral 0 <= index < 1024; -0 selects zero. Invalid
domains and out-of-range accesses remain fatal and bypass catch/finally. V12/v13
require index below the current length, including when RHS push/pop changed it.
V14 resolves current cell/backing/length after RHS, may extend up to index 1023,
creates intervening holes and publishes exactly the assigned element's presence.
Keep element invariance, payload bits/child masks and cleared spare slots. Growth/
collection cannot release prepared receivers or detached RHS children.

V14 length/literal-["length"] expressions reuse ArrayLength: non-uint32 values
(negative, fractional, NaN, infinite or above u32::MAX) raise owned RangeError;
valid lengths above 1024 remain fatal. Validate after RHS effects, then resize
with existing truncation clearing/holes rules. Return the original Number without
normalizing its -0 bits. V12/v13 retain rejection of writable length.

Register prepared source/key/output roots and save the temporary floor before
operand evaluation. Share the existing value-block result transfer, cleanup and
full-width discard with computed record values. Keep earlier caller operands,
physical pairs, escaped functions/cells/child graphs and original outputs alive
through nested statements/calls/GC and finalizers. Internal bodies contain no
source returns; callback inference sees the original RHS type. Layout/effect
visitors traverse full preparation, RHS, storage and failure bodies. No new host
operation/import, emitter helper, helper export, schema or ABI/layout is introduced;
status/grants/host ceilings and all source/type/frame/root/memory/fuel/deadline
bounds remain active. Tests cover native/portable source-free artifacts, virtual
modules, source ranges, profile/type/mutability boundaries, original values, mixed
chains, capture/result-pair widths, sparse/current-backing behavior, fatal domains,
length errors, helper purity and repeated GC/finalizer/root cleanup.

### Arithmetic compound assignment contract

V12+ admits +=, -=, *=, /= and %= for mutable initialized local/parameter bindings,
static declared record fields (dot or literal computed names) and numeric array
references. V14 also admits length/literal-["length"] targets. Support statements,
for init/update and existing enclosing expression positions, returning the computed
assigned result. Reject const/TDZ/guard-refined targets, unchecked receivers,
String/coercive array keys, Error/result mutation,
destructuring targets, exponent/bitwise/logical assignments, updates and older
profiles. Ordinary simple assignment and binary-expression contracts remain intact.

Follow admitted [compound evaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-assignment-operators-runtime-semantics-evaluation):
prepare receiver/index once, read/cache its old value, evaluate/cache RHS once,
validate operands, compute and store. A key's effects precede the old-value read;
RHS writes/resizes/reassignments cannot alter that snapshot or redirect the
prepared reference. Nested compounds cache outer old values before evaluating
inner references/RHS and preserve completed inner stores if an outer operation
fails. An RHS throw skips validation/store and retains its original exception;
no requested compound store occurs after any failure. Keep prior mutations and
system effects, and skip later arguments/enclosing work on exceptions or fatal exit.

Prepared targets share existing local/capture checks and setters. Field reads use
their typed slot, including canonical Undefined placeholders for absent optional
fields. Array old-value reads use MaybeIndex with the element|Undefined type and
current presence: holes, explicit Undefined, out-of-range/noncanonical Number keys
produce Undefined without an invalid payload load. Cache complete old union cells
and reference graphs, even for a member that may later fail an operand check.
Length reads capture the current numeric length before RHS effects.

Number/Number is admitted for every listed operator; String/String is admitted
only for +=. Static member intersection determines possible successful result
types. If the target has no admitted kind or RHS has no matching admitted member,
reject at the target/RHS source span. Otherwise read and evaluate both operands
before pure IsUnionMember checks select exact matching Number/String tags.
Reject incompatible runtime kinds with owned TypeError `Invalid compound arithmetic
operands`; do not coerce Boolean/null/Undefined, mixed String/Number or references.
Only validated branches may unbox and apply existing Binary IR. Union arithmetic
in ordinary binary expressions still requires the existing source narrowing.

Return the computed Number/String or closed Number|String result, not the original
RHS or widened storage cell. Preserve full numeric bits, NaN/infinity/-0 and exact
UTF-16 concatenation. A captured old String remains immutable across RHS mutation
and collection. Store a separate compatible/widened copy through existing target
storage; failed operand checks do not alter payloads/presence/order. Missing reads
fail after RHS without attempting a write, so they do not become property creation
or budget-limit conversions. A valid result still faces the ordinary current-store
checks: v12/v13 post-RHS array bounds remain fatal, v14 may recreate/extend an index,
and length results reuse RangeError/non-uint32 and fatal >1024 checks. String/
memory/fuel/deadline exhaustion and authorized exits remain fatal.

Nested ValueBlocks root prepared references, cached old value, RHS and result,
saving/restoring temporary floors to preserve earlier operands and physical pairs.
Register roots before effects and load results before cleanup without allocation.
Type-error paths clear all partial inputs through existing catch/finally routes;
saved returns and escaped/captured graphs survive finalizers. Internal branches
contain no source returns, so callback inference uses the computed result type.
Full layout/effect visitors see old reads, RHS, nested arithmetic, storage and
failure branches. Existing FloatRemainder is pure/status-preserving and String
concatenation uses its existing owned guest helper. No new host operation/helper
export, emitter helper, schema or ABI/layout version is introduced. All current
source/type/frame/root/value/resource limits remain active. Tests execute Wasm,
virtual modules and source-free portable/native artifacts, covering arithmetic/
UTF-16 edges, old-value order, partial chains, sparse/current backing, union/absence
errors, bounds/length limits, GC/pairs/returns, helper purity, source ranges and
operation-specific permissions/host ceilings/status/fatal exits.

### Dynamic record compound assignment contract

V12+ admits the same five arithmetic compound operators for a narrowed closed
record receiver and narrowed String computed key. Literal String/Number keys keep
the exact static path; dynamic keys never coerce or add properties. Reuse existing
computed-read union admission: every declared field plus Undefined must fit one
closed value union, with at most one record shape and one invariant array element
type, and no function/result members. Reject impossible arithmetic at the member
or RHS span; ordinary binary union expressions retain their narrowing rules.

Evaluate/root receiver, then key, once. Read/cache the selected old value with the
existing presence-aware exact UTF-16 lookup before RHS evaluation. Key effects may
change the selected field before the read. RHS effects cannot change that snapshot
or redirect the prepared receiver/key, even if their source bindings are reassigned
or their original storage becomes detached. Cache complete old reference-union
graphs, including values that may later fail operand validation.

Unknown ordinary names and absent optional fields read Undefined. Evaluate RHS
before rejecting these operands with the existing owned arithmetic TypeError;
retain RHS mutations, presence/order changes and its original exception on failure.
An undeclared inherited name raises the existing unsupported-inherited TypeError
during the read, before RHS effects. Declared own inherited-looking names use their
typed slot and presence. No prototype access, coercion or property creation is added.

After successful RHS evaluation, reuse matching Number/Number arithmetic or
String/String += guards, computation and computed result typing. Store through the
same saved reference using the ordinary dynamic-write leaf checks on full declared
types; widen only the storage copy and publish payload/presence/order only after
success. Do not re-read the old field after RHS. Return the computed Number/String
or Number|String value, including exact numeric bits and UTF-16 ownership. Outer
chains save their reference/old value before inner work; inner failure skips outer
stores, while outer failure retains completed inner stores.

Nested ValueBlocks register source/key/old/RHS/result roots, preserve earlier
temporary floors/physical pairs, and transfer results before cleanup without
allocation. GC, callbacks, saved returns and finalizers retain original graphs.
Complete body visitors include read failures, operands, arithmetic and checked
store branches; helper validation must reject host effects in any source/key/RHS
position. Existing status/grants/host ceilings and all compilation/value/memory/
fuel/deadline limits remain active. Fatal exhaustion and authorized exit bypass
guest handlers. No host operation/import, emitter helper, helper export or ABI/
schema/layout version is introduced. Tests cover profiles, exact keys/all field
bits, inherited/missing timing, source/key snapshots, partial chains, strict kinds,
GC/pairs/finalizers, source diagnostics, virtual modules and source-free Wasm/native
artifacts with allowed/denied operations and host ceilings.

### Object.keys contract

V12+ admits the unshadowed Object.keys intrinsic for one checked closed-record
argument, optionally in a null/undefined union with a single record shape. Reject
unsupported source kinds, generic/spread/invalid-arity calls and earlier profiles.
Bounded array inputs are specified below; primitive boxing, Error/result/function
objects and prototype/symbol semantics remain unsupported. Nullish union values throw owned TypeError
`Cannot convert undefined or null to object` before result construction or later
enclosing effects. Argument errors/fatal traps preserve their existing behavior.

Follow the admitted parts of
[Object.keys](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-object.keys)
and [OrdinaryOwnPropertyKeys](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-ordinaryownpropertykeys).
Enumerate present canonical array-index names (0..4294967294) numerically first,
then remaining own string names in live insertion order. Presence differs from
payload Undefined, so absent fields are omitted and explicitly written Undefined
fields included. All admitted record fields are enumerable. Do not read/serialize
payloads or invoke user code during enumeration. Typed JSON/rest inputs retain
existing metadata rules; overwrites do not reorder fields.

Lower to an internal typed value block: root one input, perform pure nullish
checking/narrowing, initialize a fresh String array at the closed shape's bounded
capacity, and compact literal names into its own-key prefix. The collector retains
both UTF-8 layout names and separately encoded UTF-16 result strings. Numeric names
are selected in sorted static order; a bounded walk of the live order trailer
selects non-index names, preserving all 32 presence bits. Root the output before
any further work. No shared source backing or payload edges enter the result.

A pure RecordEnumerate statement writes only inside the allocated output. Clear every
removed tail payload/presence bit while the old length is valid, then publish the
new length in both stable array cell and backing. Prefix slots stay present and
spare capacity remains zero; later growth/push cannot resurrect removed names.
Allocation and collection use existing helpers, layouts and ABI versions.

The value block saves/restores an activation-local temporary floor and registers
input/output/narrowed roots before argument evaluation. Internal statements cannot
drop earlier enclosing operands/arguments; handlers clean failed inputs and publish
no result. Load the success output before clearing hidden roots without allocation,
then retain it through normal enclosing expression/return/finalizer paths. Internal
value blocks contain no source return/finalizer sites; callback inference uses their
resolved result type. Layout/helper/effect visitors traverse their complete bodies.
Argument effects remain visible to host-free helper validation and real grants;
pure enumeration preserves status and existing resource/fatal boundaries. Tests
cover ordering/presence/Unicode, nullable failures, snapshots, tail clearing,
32-field/default-fuel bounds, GC/pairs/earlier operands/results, callback inference,
legacy gates, virtual modules and source-free artifacts with grants/ceilings and
source diagnostics. No host import, guest helper export or ABI/layout change.

### Object.values contract

V12+ adds unshadowed Object.values for one checked closed record or nullable
single-record union, reusing the admitted
[Object.values](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-object.values)
own-value ordering. Numeric index names sort first, other fields retain insertion
order. Presence controls inclusion independently of Undefined payloads. Direct
boxed primitives/Error/result/function inputs and getters/prototypes/symbols
remain unsupported, as do spread/generic/invalid-arity calls and earlier profiles.
Nullish union absence throws the existing owned TypeError before payload preparation
or enclosing effects; no input is coerced through JSON.

Derive the element type from declared field types through the existing closed-union
interner: equal types stay equal, compatible primitives/reference members flatten
into one homogeneous union, and unrepresentable combinations fail compilation.
Preserve the one-record-shape/one-array-element-kind union restrictions and opaque
Function/result exclusions. Optional fields retain Undefined in the inferred type;
outer source nullability does not widen it. Empty records use Undefined[] because
this profile has no Never/Any array element kind. Context does not cast a fixed
inferred result into an incompatible array type.

Key/value intrinsics share one lowering/emission path and RecordEnumerate IR.
Evaluate/root the source once, narrow after the nullish check, then read/widen each
closed-shape payload once into a typed cached slot. Register cache roots before
argument evaluation and keep all cache values alive through output allocation.
The internal value block's temporary floor retains earlier operands/physical pairs;
handlers clean all roots on failure and no partial output is published. Literal
key entries and cached value entries are the only internal ordering operands: no
user calls, boxing allocations or recoverable throws occur during compaction.

Allocate an invariant array of the derived kind, then copy cached entries into
its present own-property prefix in shared enumeration order. Pack Float64 bits
without conversion; preserve canonical primitive/union cells and actual child
masks, never tracing numeric payload bits. Copy reference identity without cloning
children. Clear all removed payloads/bits while the old length is valid, then
publish cell/backing lengths before any later allocation or host observation.
This final clearing phase is allocation-free, including when zero payloads would
not be canonical live union/reference values. Later growth cannot resurrect them.

The output is an independent outer snapshot with shared nested references and
immutable scalar/union values. Existing dense/sparse representations, status,
permissions, helper purity, type/source/body/frame/root/resource budgets and fatal
exit behavior remain intact. Layout/effect visitors traverse preparation plus the
entry expressions; callback inference sees the resolved value-block result type.
Tests cover order/presence/null/UTF-16/numeric edges, shallow identity, typed JSON/
rest/callbacks, union rejection, GC/pairs/returns/cache roots, 32-field default
budgets, legacy gates, virtual modules and source-free artifacts with real grants,
host ceilings and source diagnostics. No ABI/import/helper-export/layout change.

### Array own enumeration contract

V12+ Object.keys/Object.values also accept a checked bounded array or nullable
single-array union. The shared source block evaluates/roots the argument once and
performs the existing nullish TypeError check before later enclosing effects.
Mixed array/record sources, primitive boxing, general keys/prototypes/getters,
spread/generic calls, shadowed Object and earlier profiles remain rejected.
Follow the admitted own-enumerable rules of
[EnumerableOwnProperties](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-enumerableownproperties):
walk indices in ascending order and exclude length. V12 arrays are dense; v13+
presence bits skip holes independently of explicit Undefined payloads.

Object.keys returns String[]. Object.values preserves the invariant source array
element type exactly: skipped holes do not add Undefined or permit aggregate
covariance. No payload is read for keys. Values copy canonical slots, preserving
Float64 bits and actual String/union/Error/record/array child identities. Both
outputs have fresh cells/backing and a dense present prefix, including empty and
all-hole inputs. Their outer snapshots do not mutate source storage.

Typed ArrayEnumerate IR selects private ArrayKeys/ArrayValues Wasm helpers, with
no host imports, helper exports or ABI/layout change. Capture the current source
length/backing once after argument effects; enforce the existing maximum count.
Reserve geometric zeroed capacity from that length using the profile's layout,
with initially zero live length. Root backing before cell allocation and root the
cell before any index-string allocation. Copy one payload, set its presence bit,
then publish both lengths without allocation in between. At every possible GC
point, only the completed prefix is live; numeric bits are never reference edges.
All skipped/spare slots and bits remain zero, so growth cannot resurrect payloads.

Key formatting handles only bounded nonnegative indices: count decimal digits,
allocate one UTF-16 string and write code units directly. It introduces no general
coercion, serialization, capability or status update. The nonmoving source/output
roots protect raw pointers across allocator/collector calls. Helpers save/restore
the temporary-root cursor, so repeated calls do not retain obsolete outputs or
consume unbounded roots. Existing expression/value-block roots preserve earlier
operands, physical pairs, nested calls, saved results and finalizers. Actual
argument effects remain visible to validation and grants; fatal limits/exit stay
fatal. Tests exercise all admitted element kinds, presence/mutation/order, 1024
indices, repeated collection, virtual modules and source-free artifacts with host
ceilings and exact-operation denial. Existing profile budgets remain unchanged.

### Destructuring assignment value contract

V12+ checked pattern assignments are expressions with the actual contextual RHS
type and original value as their success result. Preserve source record/array
identity, unselected fields and sparse presence; defaults/rest traversal changes
only targets, while user-authored mutations observe normal reference identity.
Admit results in local initializers, arguments/receivers, returns, member reads,
callbacks, supported conditional/operator positions and nested pattern assignment
chains. Standalone and for init/update statements lower through the same value
path and discard the result. Computed record and ordinary simple assignment values
are specified above, along with the checked arithmetic compound subset;
other compound/update expressions remain outside this increment,
as do dynamic record keys in patterns, catch and for-in.

Resolve target syntax/bounds/mutability as before, then propagate available result
context to the RHS; absent context retains homogeneous target inference. Evaluate
one RHS into a dedicated hidden rooted slot, retaining its exact source type.
Traverse the shared Assign-mode pattern using a read of that saved slot, and load
that same slot only after all writes succeed. Outer chains begin traversal only
after inner completion and receive the original RHS. Never rebuild the result
from targets or substitute a fresh rest copy. Inferred/annotated results still
pass ordinary invariant aggregate/union checks; context cannot omit unselected
source fields or reinterpret an independently typed source array.

A dedicated IR expression owns its RHS, hidden input/marker and checked traversal
statements. Before evaluation, save the current shadow-root cursor as an
activation-local temporary floor. Statement cleanup inside the expression restores
that floor rather than the function's frame end, retaining earlier operands and
arguments still on the Wasm stack. Nested assignment expressions save/restore
their own floors. The RHS input lives in an ordinary lexical root scope before
any traversal/default allocation; result loading precedes scope cleanup without
an intervening allocation, and the enclosing expression retains it for later
operands/calls or existing return/finalizer transfer. Markers are typed internal
i32 locals outside reference tracing and count toward existing frame limits.

Exception handlers include the assignment input scope even during RHS evaluation.
Throws unwind its hidden roots and older expression temporaries to the existing
handler marker, skip later target/enclosing effects and publish no success result.
Previous successful writes remain visible. Fatal budgets and authorized exit retain
their bypass behavior. Floor restoration is scoped in emission; caller/callee
frames and source finalizer regions retain their existing lifetimes. No native EH,
heap/layout change, host operation, helper or ABI version is introduced.

Collection and effect visitors traverse both RHS and traversal statements, so
schemas/layouts/helpers/canonical references and actual default/RHS host edges
remain visible. Source return/finally statements cannot live in generated traversal
bodies; lifted function bodies and outer callback return inference remain separate,
using the assignment expression's resolved result type. Preserve exact cells,
physical result pairs, status and package/host grant conjunction.

Tests validate original identity/fields/holes and result contexts, nested chains,
arguments/members/conditionals/short circuit, callbacks/returns/finalizers, partial
throws/fatal limits, diagnostics/legacy gates, default-fuel 1024-element arrays,
GC of earlier operands/pairs and later arguments/results, helper purity, virtual
modules and source-free native/Wasm artifacts with grants/status/host ceilings.

### Lexical closures and host ABI v15

`checked-f64-utf16-v12` selects **host ABI v15/value ABI v11/schema v11**. It
inherits v11's exact function types and APIs and adds captures of initialized
lexical parameters, locals and catch bindings. No host import, capability or
aggregate/JSON tag is introduced. The new host version and explicit profile bind
the changed function layout and collector semantics; older profiles retain their
contracts and v11 rejects captures.

A captured binding is one shared managed cell per lexical activation, including
captured parameters. Direct reads/writes in the owner and all sibling/escaped
closures access that same cell. Identity uses the declaring function and unique
lexical slot, so shadowed names and separate calls remain distinct. Captures are
discovered by actual free reads/writes; nested arrows forward the same cells
through intervening environments even when the middle body never reads them.
At most 32 cells are captured by one arrow. Immutable const bindings stay
immutable; referenced object/array contents retain their ordinary mutation rules.

The v12 physical function body is 24 bytes: `[signature:u32, fields=2:u32,
user-code-index:u32, reserved-zero:u32, environment:i64]`. Heap flag **128** now
participates in tracing, with mask **2** for a nonzero environment and **0** for
an empty one. Named references remain canonical lazy roots and have zero
environments; arrow evaluations remain fresh identities. Environment blocks use
flag **256**, body `[kind=0:u32, count:u32, binding-handle:i64...]`, exact lengths
and an all-reference count-bit mask. Each binding uses flag **512**, body
`[semantic-type-id:u32, fields:u32, payload:i64, optional-status:i64]`, with 16
bytes for ordinary values or 24 for VartaResult pairs. Only actual reference
payloads set bit 0; status and numeric/function/type metadata are never reference
edges. Result status occupies a zero-extended u32 slot. Cell type IDs are a
module-internal deterministic table of complete semantic types, including function
signatures. No cell/environment handle is a source value or external JSON pointer.

The collector traverses function → environment → binding → reference payload,
using its existing bounded mark/fixed-point/sweep lifecycle; unreachable cycles
are collectible. Captured numeric locals still root their cell handles. Incoming
reference parameters are rooted before any parameter boxing; incoming capture
cells are checked and rooted before allocations. Declaration initializers finish
before allocating/publishing a new cell. Reassignment updates the existing cell
only after RHS success; returned handles and retained operand roots follow the
ordinary transfer rules. Scope/exception exits clear frame roots without erasing
escaped cells. No guest stack address escapes into an environment.

For `for (let ...)`, the first cell is created on initialization and a fresh copy
is created at the iteration boundary before evaluating update. Continue routes
through that boundary, after any finally body. Break/return/throw leave without
copying. Lexical declarations in loop bodies allocate fresh cells on each entry.
Captured catch bindings allocate a new cell before clearing the exception channel.
This preserves identity/lifetime across repeated entries and escaped closures.

Only lifted arrows in v12 take an extra hidden i64 environment parameter; named
source functions retain their physical signatures. The total, including the
hidden parameter, is limited to 32 physical slots. Closed direct dispatch passes
the environment only to those targets and requires zero for named targets. Entry
checks validate exact lengths, aligned bounded allocations, flags, masks, counts,
reserved words and expected binding type IDs before user effects. Noncanonical
Boolean/null/undefined payloads and status high words are rejected. Existing
function/type/body, root, heap, fuel, deadline and host-call budgets remain fatal.
JSON helper purity still follows direct call edges and rejects reachable host
imports, including calls through capturing callbacks. Package/host grants remain
the only authority for each actual privileged operation.

This increment rejects capture of any binding not initialized at arrow creation,
including self-initialization. Forward/TDZ captures require later ReferenceError
and initialization-state semantics. It also rejects guard refinement of a mutable
captured union, including captures discovered later after a guard: callbacks and
loop back edges must not invalidate a previously checked type. Capturing under an
active refinement is rejected rather than carrying a narrowed snapshot across
mutations. Copying to a separate immutable local can establish a stable value.
Function expressions, private self bindings and typed this are implemented above.
Broader function syntax, function-valued records/arrays, unions/JSON/throws,
arguments and async remain pending. Modern Tool/HTTP
adapters remain pending. Tests cover shared/independent activations, nested
forwarding, shadowing, loops, pairs/references, exception/finally behavior, cyclic
GC, malformed environments/cells, bounds, hidden host effects and public/module/
source-free artifact execution.

### Typed function values and host ABI v14

`checked-f64-utf16-v11` selects **host ABI v14/value ABI v11/schema v11** and
inherits v10's recoverable checked JSON and earlier source operations. The new
host version freezes function-value source semantics; it introduces no host import,
capability, aggregate value tag or JSON schema node. Older profiles reject function
values and arrows. Launcher negotiation includes the new profile and ABI maximum.

Exact, invariant function types `(name:T,...)=>R` admit function-valued locals,
assignment, parameters, returns and calls. Named module functions become stable
objects; imported/reexported aliases of the same definition compare identical.
Every evaluation of an arrow creates a fresh object. Expression and block arrows
can take parameter/result context from an expected function type; otherwise
parameters require annotations and expression results infer their actual type.
Block results retain the existing Number/void default unless explicitly annotated.
There is no result-discard or parameter/result variance conversion. Matching
function types support identity equality; typeof returns `"function"`, and a
function is truthy. Entry `main` cannot be taken as a value or called through one.

Function objects use ordinary managed allocations and packed rooted i64 handles.
Their 16-byte body is `[semantic-signature:u32, reserved-zero:u32,
user-function-index:u32, reserved-zero:u32]`. Heap flags are allocated **1 | 128**,
with zero child-reference mask/link. Function metadata is never traced as pointers.
Named references are allocated lazily and cached in exact permanent root slots
immediately after the 8-byte exception root. Their count is subtracted from the
64 KiB root-stack capacity; arrow objects have only ordinary live-expression,
binding and return roots and can be reclaimed. No environment or captured edge
exists in this increment. Limits include 128 semantic function types, 256 total
named/lifted functions, 32 physical parameter slots and 65,536 bytes per emitted
function body. Dispatch expansion is checked against the same body bound as runtime
validation. Existing memory/fuel/deadline/host-call limits also apply.

Calling evaluates the callee once, checks its length, aligned memory/block bounds,
allocation flags, reserved words and exact semantic signature, then evaluates each
argument once in source order. Emission selects a direct-call branch from the
closed set of non-main user functions with that complete semantic signature. A
same-sized Wasm signature alone does not admit a target. Unknown code indices
trap. Hidden physical slots preserve arguments and multi-slot captured results;
expression roots keep live references across nested allocation. Pending guest
exceptions are checked before using results or continuing effects; traps and
permitted process exit bypass source catch/finally as before.

No Wasm table or indirect call is introduced. Existing JSON helper validation
therefore follows every dispatch branch with its unchanged direct call-graph
analysis and rejects any reachable host import. Function values confer no host
authority: each concrete privileged call still checks package and host grants.
Malformed function cells fail fatally rather than becoming catchable source errors.

Capturing outer locals, function expressions, async/generic arrows, default/rest/
optional parameters, this/arguments and function properties/call/apply/bind are
unsupported. Functions cannot be record fields, array elements, union members,
JSON values or thrown values yet; explicit diagnostics preserve the existing
value/JSON boundaries. These restrictions are frozen for v11. V12 above adds initialized lexical captures
with explicit environments and binding ownership; Array callback APIs and promise
scheduling follow that foundation. Modern
Tool/HTTP adapters remain pending. Tests exercise Wasmtime execution, semantic
signature distinctions, source order, nested/returned functions, exception/finally
behavior, collection, malformed cells, hidden helper effects, public module APIs,
permissions and source-free native/portable artifacts.

### Recoverable checked JSON and host ABI v13

`checked-f64-utf16-v10` selects **host ABI v13/value ABI v11/schema v11**. It
inherits v9 and changes only the declared failure semantics of `varta.json.parse<T>`
through the exclusive `json_parse_v13(i32,i32,i32,i32)->i64` import. Serialization
retains json_stringify_v11; v5 value operations, Error layouts and the immutable
version_v3=3 channel selector retain their meaning. Earlier profiles/imports keep
fatal parse failures. A matching profile/import, value section and validated
host-free allocation/retention exports are required before entry.

The new decoder first validates metadata/descriptors, input ranges and UTF-16
transport. Normalization distinguishes malformed physical byte length (fatal),
unpaired source units outside JSON strings (SyntaxError), and raw/transcoded byte
limits (fatal). An input-byte-bounded iterative RawValue grammar pass checks the
entire JSON source without numeric conversion or Unicode loss. A separate bounded
walk checks whole-input depth/node/array/string limits before schema matching,
including data under unknown fields or a mismatched root type. Only then does the
existing checked schema decoder run. Its typed limit side channel also counts
synthetic optional values; no classification depends on parsing an error message.
All parsing finishes before any decoded guest value is allocated.

Malformed grammar becomes SyntaxError; grammatical JSON outside the strict closed
schema becomes TypeError, including duplicate/unknown/missing fields, wrong kinds,
unsupported keys and numbers outside the checked finite domain. This TypeError
rule is specific to Varta's typed API. Syntax/type messages are fixed trusted text
and never include source input. The additive common JsonDecodeError API separates
syntax, type, schema and typed JsonInputLimit failures. Syntax positions in that
UTF-8 API use the parser's line/byte-column convention; guest messages omit them.
The previous parse_json/UTF-16 conversion APIs retain their error behavior.

On a recoverable failure the host constructs only a bounded owned message, native
Error object and reference cell through the same guest allocator/retainer. It
checks deadline, helper results, complete cell/object data, descriptor consistency
and a canonical empty pending/value/root channel before publication. The reserved
root and value are set before pending=1; result length is cleared and the import
returns an unused physical zero. The compiler checks pending immediately after
this import, before unboxing the result or performing later argument/expression
effects. Existing handlers/finalizers unwind it, and uncaught errors use the same
owned typed boundary. A pre-existing pending channel is rejected rather than
overwritten. No partially decoded user value is published on input failure.

All size/traversal budgets, fuel/deadline, host/output/root/memory exhaustion,
metadata/ABI/channel corruption, allocator/retainer failures, index traps and
authorized exit remain fatal and bypass catch/finally. Post-decode checks also
cover scalar results. Pure decoding/recovery preserves system status, requires no
new capability and cannot expand concrete resource grants or a host ceiling.
General dynamic JSON.parse, serializer error conversion, other language/API
errors and newer-profile Tool/HTTP adapters remain pending.

### Native error families and host ABI v12

`checked-f64-utf16-v9` selects **host ABI v12/value ABI v11/schema v11** and adds
the closed built-in TypeError, RangeError and SyntaxError families. Constructors
with/without new preserve single argument evaluation, checked string/undefined
messages, fresh identity, inherited immutable names and own non-enumerable message
presence. Instanceof Error accepts every supported error; a specific family guard
accepts only that family. The supported class/name/inheritance rules follow the
pinned [native-error algorithms](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-native-error-types-used-in-this-standard).
Prototype editing and arbitrary subclasses are not implemented.

The 16-byte Error layout and tag 10 remain, with a versioned flags word at offset
0: bit 0 is own-message presence; bits above it encode a closed ErrorClass
(0 Error, 1 TypeError, 2 RangeError, 3 SyntaxError). Unknown codes are rejected.
Count 1, exact heap flags/mask and the message slot at offset 8 retain their
meaning. V10 readers continue to accept only class 0; v11 readers validate the
class before observation. Collector edges are unchanged. Name/toString source
operations read the object class, including through a boxed error union, once.
Object.hasOwn masks only the presence bit. JSON validates the full object/message
and emits `{}` for every admitted family.

In v9, the source Error annotation expands to a closed union of all four nominal
classes. A specific native annotation remains one class. Widening/closed unions
preserve membership, sibling parameters reject mismatches, and direct-local guards
partition the members. All family members use the same physical Error tag; their
brand is in the validated object header, not a name/message lookalike record.
Pure `value_*_v5` imports use value v11; `value_is_error_class(i64,i32)->i32`
validates both the cell/object and class argument (0 means the Error base guard).
Earlier operation variants are rejected under v9. `json_stringify_v11` and
`json_parse_v12` select value/schema v11; schemas still cannot construct errors.

The channel selector is immutable i32 `varta.exception.version_v3=3`, replacing
v8's `varta.exception.version=2`. Each profile requires its own selector and rejects
the other, and initialization validates the value before guest effects. This
binds final boundary decoding to the declared representation. Base Error keeps
the existing owned ThrownValue::Error shape; native families return the additive
ThrownValue::NativeError { class, message, has_message }. Full UTF-16 survives
store disposal and bounded escaped diagnostics include the correct class name.
Calls, arrays/records, catch/rethrow, finalizer overrides and GC preserve identity,
roots and class. No capability, fatal-trap conversion or native EH feature is
added. Automatic language/API exceptions, remaining native families, cause/options,
writable error properties, custom subclasses/prototypes and stack traces remain
future work; new-profile Tool/HTTP adapters remain pending.

### Nominal Error objects and host ABI v11

`checked-f64-utf16-v8` selects **host ABI v11/value ABI v10/schema v10**. It inherits
v7 and adds a nominal `Error` type, `Error(message?)`/`new Error(message?)`, direct
local `instanceof Error` narrowing, immutable `name`/`message` reads and `toString()`.
The unshadowed constructor accepts zero or one string/undefined argument (or their
union), once, without coercion. Each call creates a distinct reference. Default
name is inherited `"Error"`; omitted/undefined message has no own property and
reads as empty, while an explicit string, including empty, is own/non-enumerable.
Object.hasOwn accepts literal keys on Error; JSON.stringify validates its message
but emits `{}`. The admitted constructor/presence/toString rules follow the pinned
[ECMAScript Error algorithms](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-error-constructor).
Mutation, cause/options, native error families/subclasses, stack traces, dynamic
properties, prototype editing and arbitrary message coercion remain unsupported.

Value tag **Error = 10** is admitted only in value v10. Its packed i64 handle
points to a **16-byte** managed object with exact allocated/Error heap flags
(Error flag **64**), child mask 1, own-message flag 0/1, count 1 and an eight-byte
UTF-16 message handle at offset 8. An absent message requires canonical zero.
The collector traces the message using the same bounded fixed-point pass as other
values. Error participates in exact roots, arrays, records, optional/reference
unions, strict identity equality, typeof object and truthiness. Plain records
with name/message fields remain records and never pass instanceof Error. Reference
cell readers validate the Error header, mask, lengths, message range and flags;
old primitive/reference readers reject its tag.

The pending/value/root channel remains, but v8 additionally requires an immutable
i32 `varta.exception.version` export equal to **2**, checked before guest effects.
V7 rejects this extra export and keeps primitive-only catches. V8 catches the five
primitive kinds plus Error; aggregate throws remain unsupported. Existing finalizer
completion routing saves Error cells/returns with exact roots. Uncaught Error
values are copied to `ThrownValue::Error { message, has_message }`; full UTF-16
units survive store disposal, while display bounds and escapes diagnostic text.

Pure **v4 value operations** (`value_*_v4`) and `value_is_error` validate value
v10 before observation and preserve system status. `json_stringify_v10` validates
v10 descriptors/children, including hidden Error messages. `json_parse_v11` accepts
schema v10 with the preceding closed optional/union rules and v10 descriptors;
no JSON schema node can manufacture Error. Resizable array readers select the
value version explicitly, so Error elements cannot enter v9 observers. All older
profiles/imports/layouts retain their meaning. These operations add no capability;
fatal execution budgets, checked JSON/index traps and authorized exit remain
outside catch/finally. New-profile Tool/HTTP adapters remain pending.

### Primitive guest exceptions and host ABI v10

`checked-f64-utf16-v7` selects **host ABI v10/value ABI v9/schema v9**, inherits
v6's values/arrays/JSON and adds explicit `throw`/`try/catch/finally` across named calls.
This is the first K4.3 language increment. Throws accept the five checked primitive
kinds (Number, boolean, string, null, undefined), including closed primitive unions.
Every catch binding has their canonical union and requires ordinary guard narrowing
before type-specific operations. Omitted bindings, lexical shadowing, nesting,
rethrow and ordinary return/break/continue are supported. Error objects,
aggregate/unknown throws, catch type annotations/destructuring and stack traces
remain pending. The admitted completion/scope rules follow the pinned
[throw](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-throw-statement)
and [try/catch](https://tc39.es/ecma262/2025/multipage/ecmascript-language-statements-and-declarations.html#sec-try-statement)
algorithms, without claiming general exception compatibility.

The channel uses two mutable global exports:
`varta.exception.pending:i32` and `varta.exception.value:i64`. Pending is 0/1;
value is a canonical managed primitive-cell handle, never the raw numeric payload.
A fixed eight-byte root at the beginning of the existing shadow-root region holds
the same handle during propagation. The root cursor starts after this reservation;
it reduces the available root stack by eight bytes. Value ABI metadata v9, both
typed globals and exported 32-bit memory covering the reservation are mandatory.
Unexpected exception-prefixed exports in other profiles, missing/wrong globals,
missing value metadata and mismatched profile/ABI fail before entry. Native Wasm
start sections are rejected for v7; initial flag/value/root must be zero before
`_start`, so initialization cannot execute guest code ahead of channel checks.
The heap/cell/record/array layouts and JSON/value imports remain unchanged.

Throw evaluates its expression once and widens to the primitive union. It publishes
the cell in the global and reserved root, then marks it pending. Every generated
named-call site checks pending immediately, before completing its expression or
executing later effects. A function without a local handler restores its frame
and returns physical zero placeholders; its caller dispatches the pending value
before observing them. Physical function signatures, including captured-result
pairs, remain typed. The global root keeps the actual String child live across
all frame unwinds; numeric member bits are never traced.

A handler saves the temporary-root cursor at try entry. Exception dispatch clears
abandoned lexical roots, restores that cursor and branches to the nearest active
handler. Landing stores/root-registers the catch binding before clearing pending,
value and the reserved root. The catch's own throws target the outer handler.
Normal completion bypasses the catch. Deferred branch resolution uses the complete
emitted control stack, including expression-level conditionals/logical operators
and switch tests, to discard incomplete operand stacks correctly. Handler scope
cleanup and ordinary control transfers retain exact roots and source order.
The channel is private per instance and adds no host operation or capability.

Finally is an additive compiler operation under the same v7/ABI v10 contract;
it changes no imports, exported globals or heap layouts. Each protected region
has one emitted finalizer, hidden completion/cursor locals, a rooted saved primitive
exception and a typed saved return value (including both captured-result slots).
Normal flow and return/break/continue leaving the region route to that finalizer.
Branches within an enclosed loop/switch stay within the protected body. The
exception landing saves and roots the pending cell before clearing the global
channel, allowing calls and local catches inside the finalizer to run normally.
Normal finalizer completion restores the saved completion; abrupt finalizer
completion replaces it. Its own catch/finalizer are already inactive, so new throws
reach outer handlers and new completions traverse only remaining outer finalizers.
Scope exits clear abandoned roots and restore the temporary cursor before finalizer
execution. Saved return references retain identity, exact bits/code units and
their original evaluation time. Static completion analysis accounts for a finalizer
break/continue overriding a return when checking value-returning functions.

After a normal `_start` return, the runtime checks canonical flag/value/root state.
A pending cell is validated by PrimitiveBox and copied into owned `ThrownValue`:
binary64 bits, boolean, UTF-16 units, null or undefined. `RuntimeError::Uncaught`
contains that value after the instance/store are gone. Error display escapes JSON
string text and truncates to 256 code units; the owned API value remains complete.
Corrupt channels/cells fail as value-ABI errors. This is distinct from recoverable
host status, which is not reset or automatically thrown by guest exception logic.

Fuel/deadline, memory/root/output/host-call limits, invalid JSON/index traps and
authorized process exit abort through the existing execution path. They cannot be
caught by this guest channel and do not execute finally. No native EH/GC feature or dependency is enabled.
Tool/HTTP adapters for newer profiles remain pending, so their legacy boundaries
are not reinterpreted. Tests cover cross-module calls, interrupted expressions and
captured-result pairs, nesting/rethrow/scopes/loops/returns, all primitive values,
GC and repeated catches, malformed initial/final state and exports, fatal limits,
public APIs, source-free artifacts and host-policy/exit behavior.

### Structured values and JSON

The original i32 profile uses structured-value ABI v1/v2. The binary64 profile
uses **v3**, with unchanged eight-byte payload slots and heap ownership metadata
and a new `Float64 = 6` tag. Field/element payloads are raw IEEE-754 bits, using
Wasm reinterpret instructions on store/load. Reference masks and array reference
flags exclude numbers, including NaNs and bit patterns resembling pointers.
Record descriptors carry version 3; arrays carry their element tag. The module's
`varta.values` section binds this layout to the binary64 profile. Legacy descriptors
and imports cannot interpret the new tag; v3 cannot reinterpret the legacy i32 tag.
`inspect` reports the module's actual `valueAbiVersion` separately from host ABI.

Object identity, source evaluation order, field mutation, dense homogeneous arrays,
nested references and exact root lifetimes are retained. Binary64 `.length` widens
the exact count to f64. Subscript conversion checks finite/integral/nonnegative
values below the element budget before converting to i32; the existing address
helper then checks actual length. Fractional, NaN, infinite, negative and excessive
indices trap before load/store, and -0 indexes element 0. Fixed lengths, no holes
and trapping out-of-range access remain the checked profile's explicit restrictions.

Host ABI v2 adds versioned value imports:

| Import | Physical signature | Contract |
| --- | --- | --- |
| `json_stringify_v3` | `(i32 kind,i32 low,i32 high) -> i32 ptr` | Value ABI v3; bounded UTF-8 text transferred through existing result buffer and immediate owned snapshot |
| `json_parse_v3` | `(i32 ptr,i32 bytes,i32 schema_ptr,i32 schema_bytes) -> i64 payload` | Schema v2; complete validated scalar bits or owned reference handle, without splitting Number bits through an i32 return |

Both are pure, capability-free, host-call-budgeted operations that preserve system
status. The result-length global is cleared by v3 parsing; its full result is in
the i64 slot, reinterpreted to f64 for a Number or kept as an owned reference.
Legacy JSON signatures and return/global conventions remain unchanged.

Binary64 serialization uses exact-pinned
[ryu-js 1.0.3](https://docs.rs/ryu-js/1.0.3/ryu_js/) for ECMAScript finite Number
text, including decimal/exponent thresholds and shortest round-trip digits. This
dependency avoids a custom numeric-formatting algorithm. Negative zero emits `0`;
NaN and either infinity emit `null`, including inside aggregates. Existing UTF-8,
cycle, shape/range, depth/node and encoded-output limits still apply. See the
[pinned JSON algorithms](https://tc39.es/ecma262/2025/multipage/structured-data.html#sec-json.stringify).
These serialization rules are distinct from diagnostic `log_f64` formatting.

`varta.json.parse<T>` in this profile compiles **schema v2**, with `float64`
numeric nodes and no legacy `i32` nodes. It accepts finite JSON decimals/exponents,
rounds Number input to binary64 and preserves -0/subnormals. serde_json's existing
parser is built with `float_roundtrip` for correct decimal conversion. Overflowing
numbers (for example 1e400), NaN/Infinity tokens, duplicate/unknown/missing fields,
type mismatches and resource violations fail. This intentionally strict finite
schema API is not unrestricted ECMAScript `JSON.parse`. Schema v1 retains exact
i32 notation/range requirements. Descriptor version, field tags and canonical
slots must agree with the selected schema before and after transfer.

Complete input validation precedes allocation. V3 reuses the validated host-free
allocate/retain exports, helper-signature/effect checks, owned construction and
root/deadline/fuel rules below. Invalid descriptors/schemas expose no partial
guest result. Tests cover high-bit Number returns, formatting boundaries, aliases,
mutations, parsed allocation, collection beyond 32 array entries, invalid indices,
mixed versions, hidden host calls in helpers and source-free native artifacts.

Launcher protocol v2 advertises supported host/value ABI maxima (currently 17/12);
its profile list and actual offline payload probe determine compatibility. Update
CLI and companion together. Container/manifest formats and existing i32 programs
retain their meaning; old scalar-only runtimes reject v3 metadata/new imports.

Both checked numeric profiles support closed object literals with primitive/null
and nested record/array fields. Inline types such as `{count:number;meta:{ok:boolean}}`
are structural: field names/types determine the shape, independent of declaration
order. Assignment, arguments and results require exactly the same shape. Locals
infer shapes; parameters and object-returning functions need inline annotations.
Named aliases/interfaces remain erased and cannot supply object annotations yet.
The default profile retains i32 number and valid UTF-8 string semantics; it does not
silently redefine old scalars as the future binary64/UTF-16 JS profile.

Objects have identity. Copies, parameters, returns and nested fields retain the
same reference, and `===`/`!==` (also `==`/`!=` within this exact-type subset)
compare identity. `const` prevents rebinding but permits typed field mutation.
Field selection and assignment evaluate the base once; assignment evaluates the
RHS afterwards. Dot access and literal string/integer bracket access are supported.
Object fields use identifier/string/unsigned integer keys or shorthand bindings.
Initializer expressions run once in source order. Enumeration puts canonical
array-index keys first in numeric order, then other keys in creation order; type
annotation order does not change enumeration.

There are at most 32 fields per record, 128 structural shapes across the module
graph and 256 UTF-8 bytes per key. In this original profile, missing/new fields, optional/readonly fields,
dynamic/computed keys, spreads, methods/getters/setters, `__proto__`, deletion,
prototypes and general implicit coercion fail explicitly. `undefined` requires the
separate v2 binary64 profile; the later UTF-16 v3 profile above supplies checked optional/missing values.
`null` is a distinct supported value, rather than a missing-field marker.
`VartaResult<T>` remains a distinct immutable two-slot type, not a record field.

`JSON.stringify(value)` accepts exactly one supported value and returns compact
JSON text. It preserves nesting, key order, scalar values, Unicode and escaping;
a record becomes an ordinary JSON object, with no tool envelope. Serialization
needs no capability, consumes one host call, and preserves `result_status`.
Its result uses the bounded host buffer and is immediately snapshotted/rooted in
the managed heap. Checked parsing is described below; general `JSON.parse`,
`toJSON`, replacers and indentation remain pending.
The encoded output is capped at 1 MiB, including escapes; depth is at most 64 and
traversal visits at most 4096 values (including repeated aliases). Cycles, malformed
memory and limit failures trap without publishing a partial result. Saved host
results remain valid. Pure serialization cannot be captured as a system call.

New modules constructing records/arrays or importing serialization advertise
exactly one `varta.values` custom section containing little-endian u32 version 2.
The runtime accepts versions 1 and 2, preserving v1 scalar/record interpretations;
older runtimes reject v2 instead of silently interpreting the new array kind.
Duplicate, malformed and unsupported versions fail validation. This section is required for
`varta.json_stringify(i32 kind,i32 low,i32 high)->i32 result_ptr`. Value tags are
0 null, 1 canonical sign-extended i32, 2 boolean 0/1, 3 packed UTF-8 slice and
4 packed record and (since v2) 5 packed array. The low/high words carry one u64. Null is zero. This versioned
extension leaves host ABI v1 and primitive Tool Contract v1 interpretation intact;
new scalar profiles or structured tool parameters/results require explicit migration.

Record handles are i64, with the payload pointer in low32 and payload byte length
in high32, referencing the same nonmoving heap as strings. A record's 16-byte
block header uses allocated bit 1, mark bit 2 and record bit 4; the link word at
offset 8 holds a reference-field bitmask while allocated, or the next pointer
while free. Payload is `[descriptor_ptr:u32,field_count:u32]` followed by 8-byte
canonical field slots. Static descriptors contain `[version:u32,count:u32]` (version 1 or 2) and
16-byte entries `[key_ptr,key_bytes,slot,value_kind]` in enumeration order.
Canonical slots are sorted by field name; descriptors preserve each literal's
creation order separately. Keys/descriptors share the checked 64 KiB static pool.
The serializer validates every header/range/length/tag, static descriptor/key,
slot uniqueness and duplicate key before reading values. It accepts no native
pointers, code hooks or additional authority.

The collector marks exact roots, then iteratively traces reference masks on
marked records and all reference elements of marked arrays to a fixed point
before sweeping. It follows nested objects, arrays and owned strings, skips static slices, preserves record kind/mask on live blocks and
allocates nothing during tracing. Scans consume guest fuel. Binding and temporary
roots protect aliases, function returns, literal operands and assignment bases
across allocation; replaced edges can be collected once no other root retains
them. Memory, root-stack, per-string and execution limits remain shared.

### Dense arrays

Arrays use homogeneous exact element types: `number[]`, `Array<string>`,
`{text:string}[]`, `number[][]` and equivalent nested forms. Literals infer from
their elements; empty literals need context from a binding, parameter, return or
containing typed field/array. Mixed types, void/captured results, tuples, readonly
arrays and sparse/spread literals fail explicitly. The built-in `Array` type cannot
be redeclared. Existing source/AST complexity limits still apply independently
of the per-array element limit; wide flat literals do not count as deep expression
chains, and each element retains its expression-chain budget.

The first profile supports at most 1024 elements per array and 128 interned array
element types across the source graph. Array length is determined at construction.
`.length` reads it; assigning length, push/pop, arbitrary properties, deletion,
resizing, constructors and iterators are not implemented. Numeric i32 indices can
be computed at runtime; negative, empty or out-of-range access traps. This differs
from general JavaScript, where a missing element may be `undefined` and a write
may extend the array. String indices/coercion and holes are explicitly unsupported.
These restrictions are a stage of the runtime's compatibility profile.

Literals evaluate each element once in source order. Indexed reads evaluate base
then index. Writes evaluate base, index and RHS once in that order, then check the
index and store; an invalid write runs its RHS but writes no array element. Bounds
checks precede address arithmetic. Copies/arguments/returns preserve identity,
`const` permits element replacement and exact-type equality compares references.
Nested arrays/records and mutable aliases retain ordinary shared references.
`JSON.stringify` produces ordinary JSON arrays and shares cycle/depth/node/encoded
byte limits with records; it preserves the last system status and grants no access.
Primitive Tool Contract v1 signatures still reject arrays and objects.

A packed i64 handle carries payload pointer/byte length, as for records. Array
blocks use allocated bit 1, mark bit 2 and array bit 8. The allocated link word is
0 for scalar elements or 1 for reference elements; it is not a 32-element mask.
Payload is `[element_kind:u32,length:u32]` followed by 8-byte canonical slots.
The serializer validates the block, exact payload size, count, element kind (even
for empty arrays), every value and recursion budgets. The collector follows all
reference slots, preserves kind/link metadata on live blocks and reuses dead
storage. Mixed array/record graphs share exact binding/temporary roots and the
same bounded heap; strings and returned JSON remain snapshotted and immutable.
No system import or capability was added; array indexing uses one typed internal
Wasm helper within the seven-helper ceiling.

### Checked JSON parsing

`varta.json.parse<T>(text)` requires exactly one explicit supported type argument
and one string. The compiler derives the schema from primitive/null, inline record
and dense-array types, including nested combinations. It checks the actual runtime
JSON against this schema. It supports the same closed shapes and exact i32/UTF-8
profile as literals; `any`, void, VartaResult, optional/readonly fields, unions,
tuples, named object aliases and arbitrary generic types remain unsupported.

Input must contain one complete JSON value, with no trailing data. Every declared
record field is required, unknown fields fail and duplicate keys are rejected
after JSON escape decoding. Types do not coerce. Numbers must use plain signed i32
integer notation; fractions, decimal/exponent forms, negative zero and values
outside i32 range fail instead of rounding or truncating. Invalid UTF-8 and lone
escaped surrogates fail. Encoded input (including whitespace/escapes) is capped at
64 KiB, depth at 64, visited values at 4096 and each array at 1024 elements.

The checked extension **normalizes object enumeration to schema order**: canonical
integer-index keys first, then other field names in UTF-8 byte order,
independent of input key order. Arrays retain their element order. Parsed values
have ordinary identity, aliases, mutation and collection behavior. Each decoded
object/array is freshly allocated; scalar strings are immutable owned slices.
Dynamic ECMAScript JSON.parse, revivers and general exception handling remain
pending. This API is deliberately schema-driven and has no unchecked cast mode.

Complete syntax/type/field/limit validation precedes the first guest allocation.
Failure traps with a `checked JSON` diagnostic and publishes no partial value.
Named invocation maps this to `execution_failed`; its fresh-store behavior lets
subsequent calls run independently. Invalid outer tool arguments still fail at the
existing invocation boundary before instantiation. Parsing is pure, requires no
capability and preserves `result_status`; it cannot be captured as a privileged
system call. A future recoverable parse-result API needs a separate value/error
contract rather than fabricating a successful fallback record.

The compiler embeds a version-1 `varta_common::json::JsonSchema` as bounded JSON
in the shared 64 KiB static pool, alongside strings and record descriptors. It
contains `version`, `root` and a flat `types` table. Types describe null, i32,
boolean, string, record (`descriptor`, sorted `fields` with `name`/`ty`) or array
(`element` type index). At most 260 type nodes are permitted; child indices must
strictly precede their parent, making cycles/forward references invalid without
recursive schema validation. Duplicate/unknown schema members, unsupported
versions, invalid roots, field ordering/count/key sizes and excessive depth fail.
The runtime also verifies every static record descriptor's version/count,
slot permutation, field name and physical value kind against the checked schema.
Schema bytes cannot supply native pointers or system authority.

The registered import is
`varta.json_parse(i32 input_ptr,i32 input_bytes,i32 schema_ptr,i32 schema_bytes)->i32`.
It requires exported bounded ABI memory, mutable result globals, exactly one host
ABI section, structured-value ABI metadata and no Wasm start function. A compiled
parser module exports `varta.value_abi.allocate(i32 bytes)->i32 payload_ptr` and
`varta.value_abi.retain(i64 handle)->i64 handle`. Both must be guest functions with
the exact signatures and no transitive host imports or indirect calls, including
when there is no tool catalog. Unknown/unused reserved value-ABI exports fail.
The runtime validates allocation alignment, header flags, sizes and memory ranges,
and requires retention to preserve the handle before accepting it.

After validation, the host owns the checked input tree before any callback can
grow Wasm memory. It builds children first through the guest allocator and retains
each child before allocating its parent. The same fuel, memory, root-stack and
deadline limits apply; parsing consumes one host call. Parent record reference
masks and array element metadata use the existing collector contract. No separate
host bump heap or borrowed result-buffer references are published. The import
returns the low 32 value bits; reference byte length goes in `env_result_len` and
scalar success sets it to zero. Compiler expression roots discard temporary roots and retain the resulting
owned value with no intervening allocation.
Existing scalar/record/array layouts, host ABI v1, tool v1 and artifact versions
retain their interpretation; old runtimes reject the unknown new import.

### String values and ownership

This section describes the frozen UTF-8 profiles. The UTF-16 profile uses the
same handle/ownership/collector machinery with the encoding-specific imports
and even byte lengths specified above.

A string is an immutable UTF-8 slice represented internally as one i64: the high
32 bits hold byte length, and the low 32 bits hold its memory pointer. User
function signatures and local slots preserve i32 number/boolean and i64 string
types; signatures are interned by complete Wasm types, not just arity. Host
imports still receive the ABI v1 borrowed i32 pointer/length pair.

Literal strings refer to deduplicated static data. Every string-producing host
call immediately snapshots its result into a separate compiler-owned heap
before evaluating another expression. Variables and aliases copy the immutable
slice handle; reassignment changes the handle, and parameters/returns can share
the same slice through nested, recursive and cross-module calls. They never
retain an alias into the reusable host buffer. Empty/failed results use an empty
slice and allocate nothing; copying does not clear or replace the host status.

Host snapshots and nonempty concatenation results share a bounded, nonmoving
string heap. Before growing memory, the allocator collects unreachable strings
and reuses their storage. It grows memory only in whole Wasm pages, within the
existing 8 MiB limit. Its private metadata cannot be changed by source code or
host result writes. Heap exhaustion or failed growth traps before a new slice is
published; no live value is overwritten. Copying and collection consume guest
fuel, so the fuel budget can stop allocation before the memory boundary. Each run
creates a fresh instance and heap.

Dynamic strings passed to host calls follow exactly the existing input bounds,
UTF-8 checks and operation/resource authorization. A string value conveys no
authority. No new host import, capability, manifest or artifact version is needed.

### String operators

The byte-level implementation below is for UTF-8 profiles; UTF-16 storage and
direct code-unit comparison are defined in the ABI v4 section above.

`string + string` concatenates UTF-8 content into a new immutable slice. Both
operands are evaluated exactly once, left to right, including nested user/host
calls. If either operand is empty, the result shares the other immutable slice
without allocating; the other expression is still evaluated. Nonempty operands
are copied into one heap allocation, with length overflow, per-value size,
heap and memory-growth checks before publishing the new slice. Existing aliases,
captured results and source operands retain their original content. There is no
implicit conversion: `"count=" + 1` is a type error. Use `s = s + suffix` for
reassignment in these UTF-8 profiles. V12+ UTF-16 profiles add the checked arithmetic
compound contract above, including String/String +=.

`==`, `===`, `!=` and `!==` compare string content, not pointers. Both operands
must be strings, so loose and strict equality have the same behavior within this
subset. `<`, `<=`, `>` and `>=` use lexicographic **UTF-16 code-unit order**, with a
shorter prefix sorting first. They are case-sensitive and perform no Unicode
normalization or locale collation: `"é"` differs from `"e\u0301"`, and `"🙂"`
sorts before `"\uE000"`. Strings remain stored as valid UTF-8; isolated surrogate
values remain unsupported. Embedded NUL bytes are ordinary content, not terminators.

Comparison allocates nothing and does not call the host or change result status.
The helper compares valid UTF-8 bytes with EE/EF lead bytes ordered above F0–F4,
preserving UTF-16 order around BMP/supplementary boundaries without materializing
a UTF-16 copy. Matching handles can return immediately; other comparisons visit
the common byte prefix and stop at the first difference. Loops consume guest fuel.
Neither concatenation nor comparison resets `varta.result.status()`, and saved
`VartaResult<T>` pairs remain unchanged.

Each runtime string value is capped at **1 MiB of UTF-8 bytes**. Concatenations and
host snapshots share the bounded 8 MiB memory budget and reclamation rules.
Copying, collection and comparisons
may exhaust the default fuel budget before reaching the byte/memory limits.
Size/allocation/fuel failures stop execution rather than becoming a captured host
failure. Helpers for snapshot, concatenation, comparison, allocation, collection
and temporary roots use signatures and function indices resolved from typed tables.
Programs using only literals/comparisons need no managed heap. Host ABI and artifact versions
remain unchanged, and constructed paths/URLs still undergo the original resource
authorization and input checks.

### String memory reclamation

The compiler maintains exact reference roots in a bounded shadow stack. A function
frame reserves one root slot for each string, record, array or `VartaResult<string>` binding,
including parameters. Assignments replace that slot; aliases retain independent
slots. Lexical scope exit clears its bindings, including exits through `break`
and `continue`. Function return removes the frame after evaluating its return
value. A binding is conservatively retained until overwrite, scope exit or return,
even if its last source use was earlier; there is no last-use optimization yet.

Expression roots protect evaluated operands and call arguments while later
operands or nested calls allocate. Each expression restores its temporary-root
mark and roots its resulting reference payload; complete expression statements and
assignments remove consumed temporaries. Callee parameters are rooted before any
allocation. Return transfer has no intervening allocation between removing the
callee frame and rooting the caller's result. Numeric/boolean/null values and result
statuses are never mistaken for pointers. Static literals and empty strings do
not require heap marking. `VartaResult<string>` roots only its immutable payload.

Allocated blocks have a 16-byte header and 8-byte alignment. Allocation first
searches the free list and splits sufficiently large blocks. If appending would
grow committed memory, it runs one mark/sweep pass: mark blocks referenced by the
active roots and transitive record/array edges, free unmarked blocks, coalesce adjacent free blocks, and trim a
free trailing run from the logical heap. Allocation retries before requesting
additional pages. The collector neither moves live data nor changes host status;
all scans, free-list traversal and zeroing use guest fuel. Live data, allocation
metadata or fragmentation can still exhaust memory; no compaction is implemented.

The root stack is 64 KiB (8192 packed slice slots) across active frames and
temporaries. A function whose fixed reference frame exceeds that bound is rejected
with a source diagnostic. Dynamic frame/temporary overflow traps before writing
past the root region. Programs requiring owned strings, records or arrays initially commit 18 Wasm
pages for static data, host buffer and roots; literal/comparison-only programs
retain the one-page minimum. Actual heap blocks grow memory on demand. Freed
blocks are reused during the run, while committed Wasm pages retain their
high-water size until the instance is dropped. The host ABI, raw pointer/length
representation and portable artifact versions remain compatible with old code.

## Local modules

The compiler accepts named function imports (`import { f as g } from "./lib.ts"`),
direct function exports (`export function f(...)`) and local export lists
(`export { f as g }`). Local export lists may forward an imported function.
Each module has a separate function scope; private helpers with the same name in
different files do not collide. Imports must name exported functions, and their
parameter/result types are checked at each call. Duplicate or reserved bindings,
duplicate exports and unresolved exports fail before emission. Type aliases and
interfaces retain their existing erased behavior; they are not importable values.

The entry module must declare its own `main(): void` with no parameters. An
imported binding or an export alias cannot supply the entry point. A dependency
may declare an ordinary function named `main`, with its own signature. Recursion
within a file is supported; calls to the entry function remain rejected.

The dependency graph is a DAG: cycles, including self-imports, fail with an import
site diagnostic. There are no top-level execution or initialization semantics.
Dependencies are visited in sorted path order, once per resolved identity, and
linked before their dependents. Function slots are then allocated in module order
and declaration order, independent of import declaration order. Modules are
linked at compile time: running a built artifact never loads source files.
All linked host calls use the application's manifest and optional host ceiling;
an imported module cannot introduce permissions of its own. Source imports read
build inputs and do not grant guest filesystem access.

The project loader requires explicit relative `.ts` paths starting with `./` or
`../`. It normalizes dot segments lexically and rejects paths outside the
canonical directory containing the configuration file. Absolute/bare paths,
URLs, backslashes, controls, query strings and fragments are rejected; there is
no extension probing, directory/index lookup, npm resolution or network loading.
Parent segments are allowed when the normalized path stays inside the root.
The root directory is pinned once; every dependency is opened relative to that
descriptor with no-follow at every component. Both external and internal
symlinks are rejected. Inputs must be bounded regular UTF-8 files. The explicitly
configured entry retains support for legacy layouts outside the config directory;
this does not expand the boundary for discovered dependencies.

Limits: 64 unique modules including the entry, 256 import declarations, 32 modules
on the active dependency path, 1 MiB per source, 4 MiB aggregate source and 256
user functions across the graph. Existing per-file lexical/semantic complexity
limits still apply. Default, namespace, side-effect, type-only and attributed
imports, export-from/export-star/default exports and dynamic imports fail
explicitly. Unused imports still resolve and validate their dependency.

Embeddings use `compile_modules(ModuleSource, &dyn ModuleLoader)`. The loader
separates stable identity resolution from bounded text loading and owns its
source-access policy; the compiler performs no filesystem or network I/O.
Single-source `compile_typescript[_with_path]` remains available and requires an
explicit loader when source imports dependencies. Parse-only helpers do not link.
The result returns Wasm plus dependency metadata. `CheckedProject.modules` and
successful `check --json` expose `modules` as dependency-first `{path, imports}`
records, with sorted, deduplicated import paths. This metadata is for build-time
inspection and is not persisted into Wasm or manifests. ABI and artifact format
are unchanged.

## Source diagnostics

The project service resolves the configured entry to its canonical file path and
passes that identity into the compiler. Public `compile_typescript_with_path`
and `parse_typescript_with_path` accept source text and a diagnostic path without
reading that path; the original in-memory helpers retain `input.ts` as their
compatibility name. Naming a source does not change emitted Wasm bytes.

Parsing, lexical resolution, subset checking and type checking share one source
context per file. Nested failures keep the innermost available span rather than being
relabeled as the containing call or function. Function and expression spans also
survive typed IR lowering, so static-data emission errors identify the source
literal that exceeds the budget. This is compile-time source metadata; Wasm
runtime source maps and stack-trace symbolication are not implemented yet.

Diagnostics have stable category codes: `syntax_error`, `type_error`,
`module_error`, `unsupported_syntax`, `invalid_program` and `internal_error`. Missing
modules, cycles and path-policy failures point to the import site; dependency
syntax, type and static-data failures retain the dependency's filename and range.
Type mismatches
include expected/actual scalar types where available. The shared diagnostic
record contains the filename, message, a half-open source range, and an optional
bounded source excerpt. Lines and columns are **one-based Unicode scalar**
positions, not UTF-16 editor offsets; `byte_offset` is zero-based UTF-8 in the
original supplied text. A leading BOM is excluded from character columns but
included in byte offsets. CRLF, non-ASCII identifiers and end-of-file positions
preserve these conventions. Excerpts are windowed to 160 source characters around
the error, expand tabs for display and replace control characters; full ranges
remain available even when the displayed excerpt is truncated. Source-wide size
limits report the file start without copying an oversized source into the parser.

`check --json` preserves `status` and the human-readable `error` string and adds a
`diagnostics` array. Successful checks return an empty array; non-source failures,
such as an unreadable configuration file, also return an empty array rather than
inventing an entry-file location. The compiler currently stops at the first
failure. One diagnostic object for `let x: string = 7` is:

```json
{
  "code": "type_error",
  "message": "type mismatch for x: expected string, found number",
  "file": "/work/app/src/main.ts",
  "range": {
    "start": { "line": 2, "column": 19, "byte_offset": 49 },
    "end": { "line": 2, "column": 20, "byte_offset": 50 }
  },
  "snippet": {
    "text": "  let x: string = 7;",
    "highlight_start": 18,
    "highlight_width": 1
  }
}
```

Human-facing failures use `file:line:column: message` followed by the excerpt and
a caret. JSON stdout remains valid JSON; human diagnostics go to stderr. All
project commands use the same source validation path and return nonzero on these
failures before producing artifacts.

## Capabilities and trust

All host access is denied unless the **exact operation** is granted. A grant for
`env.has` cannot authorize `env.get`; `fs.read` cannot authorize `fs.print`.
Logging has no system-access capability but is output-budgeted.

Configuration shorthand:

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

Only `strict` is implemented. `compat` is rejected until it has an actual
behavioral contract. Unknown operations, unknown fields, duplicate manifest
grants, wrong value types and unsafe patterns are errors. Empty lists and false
are valid explicit denials; omitted grants also deny. Policy complexity is capped
at 128 grants and 256 patterns, with at most 64 patterns per grant and 512 bytes
per pattern. A wildcard comparison also fails closed when the product of
pattern/input byte lengths exceeds 65,536, bounding work outside Wasm fuel.

Canonical manifest:

```json
{
  "schemaVersion": 1,
  "abiVersion": 1,
  "policy": "strict",
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] },
    { "id": "time.now", "enabled": true }
  ]
}
```

`entry` and `target` metadata are optional. Legacy unversioned JSON manifests use
schema/ABI version 1 defaults but must pass the same strict grant validation.
Legacy string grants and implicit `enabled: true` are no longer accepted.

The embedding application can supply trusted grants directly, or use
`run_wasm_bytes_with_policy(bytes, requested, ceiling)` to constrain package
requests with a separate trusted host ceiling. The two policies are independently
validated and checked for every concrete operation/resource. Wildcard patterns
are never flattened into an approximate intersection. A ceiling can only restrict
requested permissions, never introduce an operation or resource the package did
not request.

The CLI preserves declared-manifest behavior when `--policy` is absent. Supply
`--policy host-policy.json` to constrain `run`, `dev`, `inspect`, `validate` or the
packaged application's own invocation. The host file uses the same strict
manifest schema. An explicit `{"capabilities":[]}` denies all privileged access;
a missing, unreadable or malformed policy is an error, never a fallback to the
package's permissions. Policy paths are resolved relative to the invocation's
working directory. Host policy is local execution input and is never embedded or
written into the artifact; `init`, `check` and `build` reject this flag.

```json
{
  "schemaVersion": 1,
  "abiVersion": 1,
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] },
    { "id": "time.now", "enabled": false }
  ]
}
```

```sh
varta run dist/app.wasm --policy host-policy.json
varta dev --policy host-policy.json
varta inspect dist/app.wasm --policy host-policy.json --json
./dist/app --policy host-policy.json
```

Inspection and JSON validation include an `authorization` object with the mode,
`combination: "allOf"`, complete requested and host-ceiling scopes, and one review
decision per requested operation. `denied` means an omitted/false/empty filter;
`granted` applies to enabled boolean operations; `runtime_check` means that the
scoped filters must both accept the actual resource. It does not promise that
arbitrary wildcard scopes overlap or that every call will succeed. A null
`hostCeiling` means no ceiling was supplied; an empty array is an explicit denial.
Validation still checks artifact/ABI/schema/declarations, not application behavior:
a valid deny-all policy can validate successfully while its operations are denied
at execution. `inspect` without a host policy continues to describe the declared
permissions and labels its authorization mode `declared`.

A native launcher itself has the authority of the OS user launching that
executable. Rebuild existing launchers to make their own CLI accept `--policy`;
the portable artifact format and ABI are unchanged.

### Filesystem

Patterns must be absolute and cannot contain traversal. `*` matches within a
single path component, `**` can span separators, and `?` matches one non-separator
character. Root/component boundaries remain intact: `/data/**` cannot match
`/data-secret/file`.

At execution setup, explicitly granted directory roots are opened and pinned as
capability descriptors. With a host ceiling, these descriptors come from the
host's scopes. Requested paths must also pass the package's lexical scopes before
any file is opened; a package-supplied root alias cannot acquire authority outside
the pinned host root. Each guest path is matched relative to the corresponding
root and opened through those descriptors, with **no-follow at every component**.
Symlinks below the granted root are rejected, including links to other files
inside that root. Directory and file opens do not reuse a checked ambient path.
This closes the canonicalize/open race. Root authority is acquired once at setup;
a declared root alias is resolved at that time.

Only regular files are read. Reads are nonblocking at open, capped before the
result is allocated unboundedly, and checked for UTF-8 where the ABI returns text.
OS filesystem operations remain synchronous; a stalled remote filesystem is not
made cancellable by the guest's fuel budget.

### Network

HTTP only is implemented. HTTPS grants and unsupported schemes are rejected.
URLs are parsed before authorization; scheme, normalized hostname and effective
port must match exactly. Wildcards are permitted only after the authority; `*`
can match path/query characters and `?` retains its URL query-separator meaning.
Control characters, whitespace, credentials and fragments are rejected. Encoded
paths are conservatively rejected to avoid server-specific decoding aliases.

The same parsed URL is passed to the transport. Automatic redirects and ambient
proxies are disabled, so a permitted response cannot silently grant another
request target. The HTTP client bounds headers and body and applies an end-to-end
remaining deadline including name resolution, connect and body reading. There is
no handwritten request-line/header construction.

## Tool Contract v1 and offline discovery

The entry module may explicitly publish an exported named function:

```ts
export function echo(value: string): string { return value; }
export function main(): void {}
varta.tool({ name: "text.echo", description: "Return supplied text" }, echo);
```

`varta.tool` is a compile-time form, accepted only as a direct top-level call in
the entry module. It emits no guest initialization or object allocation. Its two
arguments must be a literal object with exactly `name` and `description` string
fields and a named function binding explicitly exported by that module. A local
function or an imported/re-exported function may be the handler; exporting a
function alone never publishes it. Dependencies cannot publish tools implicitly.
`main` cannot be a tool. Spreads, computed fields, dynamic metadata, extra fields,
duplicate fields/names and inline callbacks fail with source diagnostics.

The compiler derives ordered named parameters and the result from the checked
function signature. Supported input types are `number` (signed i32), `boolean`
and `string`; `void` is additionally supported as a result. Optional/default/rest
parameters, objects, arrays, `VartaResult` and generic tool signatures are not
supported. Parameter names use ASCII JS identifier characters, at most 64 bytes.
Tool names start with an ASCII letter and contain ASCII letters/digits, `_`, `-`
or `.`, at most 64 bytes. Descriptions are nonblank UTF-8, at most 2048 bytes,
without NUL. There are at most 64 tools and 16 parameters per tool.

One `varta.tools` custom section contains strict JSON `ToolCatalog` metadata,
bounded to 256 KiB, with independent `contractVersion: 1` and
`invocationAbiVersion: 1`. Unknown/duplicate JSON fields, unsupported versions,
noncanonical ordering and invalid limits/types fail closed. The catalog is sorted
by tool name; parameters retain signature order. Applications with no tools omit
the section. Host ABI v1, manifest schema v1 and container format v1 are unchanged.

`inspect` adds `toolCatalog` (null for no-tool artifacts). It projects canonical
types into closed JSON input schemas with every named field required, and output
schemas wrapping the result as `{ "value": ... }`; void maps to null. i32 bounds
are explicit. String schemas describe a character ceiling, while a separate UTF-8
byte ceiling remains authoritative. Schemas are generated, not separately stored
or accepted as a second source of type authority. Version 1 exposes no declared
domain errors (`domainErrors: []`); typed application errors belong to C2.

Each tool exports `varta.tool.<name>` with the checked physical Wasm signature.
Artifact validation requires the listed exports and matching signatures, rejects
unlisted reserved exports and recomputes each handler's transitive privileged
host operations from its direct call graph, including recursive calls. The exact
sorted set must match `requiredEffects`. Unprivileged logging is excluded.
Handlers must be guest-defined functions; string-transfer helpers cannot call
any host imports. Indirect/ref calls are outside tool ABI v1. Tool modules require exactly one
valid `varta.abi` section and cannot have a Wasm start function. Code validation
does not instantiate the module, call `_start`/handlers or run a native launcher.
All artifact commands validate contracts; `validate` additionally reviews grants.

Catalog limits currently equal the shared runtime ceilings: 64 KiB serialized
input, 1 MiB serialized output, 1 MiB per UTF-8 string, 8 MiB guest memory,
1,000,000 fuel, 1024 host calls and a five-second deadline. They are fixed in
contract v1, not configurable grants. The JSON invocation boundary below enforces
input/output envelopes. Metadata is public and untrusted:
it is neither an authenticity proof nor an authorization decision. Type/ABI checks
cannot prove semantic equivalence between same-signature implementations. Runtime
capability enforcement remains necessary for every actual operation.

### Tool invocation ABI v1 ownership

The low-level ABI uses a **fresh instance per invocation**. The shared dispatcher
validates inputs, enforces budgets, instantiates without calling `_start`, and uses:

| Value/operation | Physical representation |
| --- | --- |
| number | i32, signed integer bounds |
| boolean | i32, host must accept/produce only 0 or 1 |
| string | i64 immutable handle: high 32 bits UTF-8 byte length, low 32 bits pointer |
| void result | no Wasm result; JSON projection is null |
| `varta.tool_abi.copy_string` | `(i32 ptr, i32 len) -> i64`, copies into the owned string heap |
| `varta.tool_abi.retain_string` | `(i64 handle) -> i64`, retains a copied input in the root stack |

Modules with any string tool parameters/results export both helpers and bounded
32-bit ABI memory. For each string argument, the host writes validated UTF-8 into
the reusable host buffer, copies it, and immediately retains the returned handle
**before another allocation**. This prevents the next input or a host call from
overwriting it and prevents collection during later argument transfer. These
helpers are trusted-host primitives, not tools for untrusted callers. The host
must never accept raw pointers/handles from external JSON.

The host calls the selected tool once, validates the result type/boolean/range,
bounds-checks and UTF-8-validates any string, and copies it into host-owned memory
before another guest allocation or dropping the instance. Result handles are not
retained after return; reading them immediately is required. Every invocation,
including traps, ends by dropping the instance; helpers do not support persistent
instances or releasing host roots between repeated calls. Input roots, allocator
metadata and helper work consume the existing memory/root/fuel budgets. The shared
dispatcher implements these obligations, including validation of copy/retain
results before passing transferred inputs to the handler.

### Checked JSON invocation

`VartaRuntime::invoke_tool` and `invoke_tool_with_policy` accept portable code, a
published tool name, raw JSON parameter bytes and trusted package/host grants.
The project APIs `invoke_tool` and `invoke_tool_with_policy` use an artifact's
validated manifest and, optionally, a `HostPolicy`. The CLI exposes these through
`varta invoke <artifact> <name> --params '<object>' [--policy <file>] [--json]`.
The default parameter object is `{}`. Native containers are read as portable
artifacts; their launchers are not executed. Source files are unnecessary.

Dispatch order is bounded request/name and grant checks, full portable-code and
catalog validation, exact name lookup in the catalog, complete parameter parsing,
fresh store creation, argument transfer, handler call and result validation. No
lookup falls back to an arbitrary export. All arguments are validated before
instantiation; a bad later argument cannot allow an earlier guest effect. Tool
modules cannot contain a Wasm start function, and invocation never calls `_start`.

Parameters must form exactly one JSON object, with every declared field present,
no extras and no duplicate decoded keys (including escaped spellings of a key).
Object field order is independent of physical argument order. The parser rejects
trailing data, arrays/objects/null in primitive fields, invalid UTF-8, lone escaped
surrogates, numeric strings, integer booleans and out-of-range numbers. Integer
input uses exact plain signed i32 JSON tokens; decimal, exponent and negative-zero
forms are rejected instead of rounding external values through floating point.
This is a deliberately narrower wire profile than JSON Schema's mathematical
integer interpretation. Input bytes include JSON whitespace/escapes. JSON recursion
limits remain enabled and strings also retain their independent UTF-8 byte cap.

Argument copying, retaining and the handler share one store and fuel/deadline
budget. Two string arguments cannot alias the reusable input buffer. Copy-helper
results are bounds/length/UTF-8 checked and compared with the supplied input;
retention must preserve the handle. Returned strings are bounds/byte-limit/UTF-8
checked and copied before any further guest allocation. Returned booleans must be
exactly 0 or 1. Integer and void results preserve their specified representations.

`ToolInvocationOutput` contains `result: { value: <primitive-or-null> }` and
separately captured `logs`. Serialized result size is checked with a bounded
counting writer, so escaped output cannot cause an oversized temporary JSON
allocation. Logs remain subject to the existing separate output budget. There
is no store, borrowed guest pointer or handle in a successful host result.

`ToolInvocationError` has stable codes: `invalid_arguments`, `unknown_tool`,
`invalid_artifact`, `invalid_policy`, `invalid_result`, `execution_failed` and
`guest_exit`. Traps and resource exhaustion are invocation failures, not host
status values. An authorized guest exit, including exit code 0, does not fabricate
a successful tool result. The ordinary run/start path retains its exit-code and
captured-log behavior. Domain/revision errors remain future application work.

Concrete host operations still use the package/host conjunction and descriptor
scopes. Denials and I/O failures keep their recoverable host statuses; a handler
may deliberately return a fallback after checking `VartaResult`. Successful tool
execution is not proof that every attempted host operation succeeded. No rights
are accepted in the JSON arguments. The API is a trusted local embedding surface;
caller identities, per-caller tool subsets and remote authentication are not yet
implemented.

With `--json`, CLI success is one `{status:"ok",result,logs}` record; invocation
failure is `{status:"failed",code,error,diagnostics:[]}` and exit code 1. Errors
reading an artifact/policy before dispatch retain ordinary project error handling
and may have no invocation code. Without `--json`, stdout is the result envelope
and logs go to stderr. Failures expose no partial result and are never retried.
Already-performed authorized host effects are not rolled back. Each call has its
own budgets; shared state, atomic application commits and aggregate queue/accounting
are not part of this dispatcher milestone.

## HTTP service v1

The compiler recognizes exactly one entry-module top-level
`varta.http.serve({hostname:"127.0.0.1",port:8080}, handler)` declaration. Options
must be literal IP and u16 port values, with no unknown/duplicate fields; DNS
hostnames and dynamic registration are not part of this profile. The named handler
may be local or imported and has signature
`(VartaHttpRequest) => VartaHttpResponse`. It need not be an exported source
function or AI tool. Entry `main():void` still performs bounded bootstrap; a
controlled exit from main skips service startup. Registration does not execute at
source top level and adds no application routes/models to the engine.

The built-in structural transport types are:

```ts
interface VartaHttpHeader { name: string; value: string }
interface VartaHttpRequest {
  method: string; target: string; body: string; headers: VartaHttpHeader[];
}
interface VartaHttpResponse {
  status: number; body: string; headers: VartaHttpHeader[];
}
```

The request target retains path/query syntax; no route normalization, decoding or
authentication is supplied. Header names are normalized by the HTTP library and
repeated values remain entries. Bodies are UTF-8 text; handlers can use checked
JSON parsing. Response body bytes are sent directly, with no tool/MCP envelope.
The application chooses content type and performs identity/domain validation.
The initial profile supports synchronous stateless requests, not shared guest
state, closures/promises or standard web Request/Response classes.

`net.listen` requires `ListenEndpoints`, serialized as manifest `patterns` and
configured as `"net":{"listen":["127.0.0.1:8080"]}`. Entries must be canonical
SocketAddr strings, including `[::1]:8080` for IPv6. There are no globs, DNS names,
bare numeric port lists or boolean fallback. Empty lists deny. Exact requested
endpoint and exact host ceiling must both permit the bind **before any socket is
opened**. An outbound URL grant never permits listening; all-interface binds
such as `0.0.0.0:8080` need that explicit endpoint. Port 0 authorizes an OS-selected
port for that IP only; it does not match an explicit nonzero-port grant. The actual
address is reported through a readiness event.

A bounded `varta.http` custom section holds strict `HttpContract` v1: canonical
`bind`, the reserved handler `export`, sorted `requiredEffects` (handler effects
plus net.listen), and immutable `limits`. Duplicate/malformed sections, unknown
members, unsupported versions, changed limits, misbound exports or effect mismatch
fail offline validation. The metadata must occur with the internal
`varta.http_service() -> ()` feature import; this no-op marker gives old runtimes
an explicit unknown-import failure. It grants no authority. Host ABI v1 and
structured value ABI v2 are required, with no Wasm start function.

The compiler creates one internal string adapter with signature `(i64)->i64`:
checked parse of the request record, one call to the typed source handler, then
bounded JSON serialization of the response record. It exports
`varta.http_abi.handle`, `varta.http_abi.copy_string(i32,i32)->i64` and
`varta.http_abi.retain_string(i64)->i64`. Copy/retain must be host-free transitively,
including without a tool catalog; indirect calls are rejected. Ordinary source
exports do not become network handlers. HTTP metadata never turns a function into
an AI tool. Named tool invocation does not start the service or execute main.

After bootstrap drops its store/deadline, the host owns one listener and reactor.
Every HTTP request gets a fresh store with existing fuel, five-second execution,
8 MiB memory and host-call/output limits. Input copying, retention, checked parsing
and the handler share that budget; main is not rerun. The response is copied and
validated before the store is released. There is no instance reuse after a trap.
A malformed handler response, guest exit, fuel exhaustion or checked JSON failure
produces a generic 500, with details only in trusted diagnostic events. Already
performed authorized effects are not rolled back; later requests can proceed.

Hyper owns HTTP1 parsing/framing. The initial reactor accepts one connection at a
time and closes it after one response. There is at most one guest worker, guarded
by a permit that survives connection cancellation, and no unbounded worker queue.
Bounds are 32 headers / 4 KiB header data, 1 KiB target, 8 KiB raw body, 64 KiB
encoded request record, and 1 MiB encoded response record/body. Header reads have
a two-second timeout, body reads two seconds and a connection ten seconds. Body
and header overflow, malformed framing, unsupported upgrades/CONNECT/trailers and
invalid UTF-8 fail before guest dispatch. Status must be 200–599; 204/205/304 require
an empty body. Response header names/values use the HTTP library's validation;
connection/framing/hop-by-hop headers are forbidden. The runtime controls length
and connection closure. HEAD suppresses content and reports representation length
where permitted. TLS, HTTP2, streaming and keep-alive remain pending.

`varta.http.stop()` / `varta.http_stop() -> ()` closes only the owning service.
The request context carries its unforgeable shutdown handle and requested endpoint;
the operation checks net.listen again. An outside-service call returns invalid-input
status and cannot stop another instance. Shutdown stops acceptance first and lets
the current response finish within connection bounds. The synchronous embedding
API can use `run_observed` with `RuntimeEvent::Listening {address,shutdown}`;
`HttpShutdown::shutdown` is idempotent and also wakes an idle listener. Observer
callbacks are trusted synchronous sinks and must return promptly; slow sinks apply
backpressure. CLI/project layers report readiness and stream request output, with
no library printing. Without an observer, captured lifetime output is capped at
1 MiB and overflow stops the service with an error. Each request retains its own
output ceiling regardless of observer. The independent runtime companion and
packaged executable serve source-free HTTP through this same runtime event path.

## ABI v1 and errors

Imports live under `varta`. `HostOperation` in `varta-common` owns their names,
capability identifiers and scalar Wasm signatures. Pointer-taking imports require
an exported non-shared 32-bit `memory`. String-returning imports also require
exported mutable i32 globals `env_result_len` and `result_status`. Invalid import
signatures, result globals and advertised ABI versions fail before instantiation.

- Input strings are borrowed for the call and checked for bounds, length and UTF-8.
- Static strings occupy `[0, 65536)`.
- Host string results use a host-owned reusable buffer starting at 65536.
- Result pointers/bytes remain valid until the next host operation; there is no
  unbounded host bump allocation and no guest-controlled host heap pointer.
- Compiled string values snapshot those borrowed bytes into immutable execution
  storage. The layout is static data `[0, 65536)`, reusable host results
  `[65536, 1114112)`, exact reference roots `[1114112, 1179648)`, then the managed
  string/record/array heap `[1179648, 8388608)`, including block headers. Unreachable blocks are
  reused during execution; the entire instance is released when execution ends.
- Every privileged system operation first clears result length/status. Failed string calls
  return pointer 0 and length 0. No previous result length can be reused.
- Logging and pure JSON serialization/parsing preserve the last system status. Serialization
  replaces result bytes/length; compiled code snapshots retained results immediately.
- `varta.result.status()` reads the structured status without clearing it.

Status codes:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Capability denied |
| 2 | Invalid input or UTF-8 |
| 3 | I/O failure |
| 4 | Result too large |

`net.fetch` returns the HTTP status or -1 on failure. String failures return an
empty result plus the status above. Human-readable diagnostics remain available.
Guest exit is a controlled runtime outcome, never `std::process::exit` inside the
library. The CLI alone translates the result into a process exit code. Runtime,
compilation and artifact errors return nonzero CLI status; JSON errors stay
machine-readable on stdout, with diagnostics on stderr.

## Programmatic host results

`varta.result.capture` is a compiler intrinsic that executes one direct privileged
host call and immediately pairs its value with that call's status:

```ts
function read(path: string): VartaResult<string> {
  return varta.result.capture(varta.fs.read(path));
}

export function main(): void {
  const result = read("/data/config.txt");
  if (result.ok) {
    console.log(result.value);
  } else {
    console.log(result.status);
  }
}
```

The immutable fields are `ok: boolean` (status equals zero), `status: number`
(the ABI codes 0–4 above) and `value: T`. `T` is exactly `string`, `number`,
`boolean` or `void`. Locals can infer their result type; function parameters and
returns use the built-in global `VartaResult<T>` annotation. Copies, aliases,
whole-value reassignment, nested/recursive calls and cross-module parameters and
returns preserve both components. Later host calls, logging and reads of the
legacy `varta.result.status()` cannot change a previously captured result.

The argument is evaluated once. It must lower directly to one privileged host
operation, optionally in parentheses; its own arguments may contain expressions
and calls. For nested host arguments, the result describes the outer call.
Capturing a variable, arbitrary expression, user-function call, status inspection
or logging call is rejected: these would not reliably identify one fresh host
status. Put capture inside a user wrapper and return its `VartaResult<T>` instead.
All existing time, environment, filesystem, network and process operations are
supported. Logging and pure JSON serialization/parsing are excluded because they preserve system status.

A successful empty string, false or zero has `ok == true`; a failed call keeps its
original fallback value with `ok == false`. Fallbacks are empty string, false and
zero, except `net.fetch`, whose failure value remains -1. A void result has no
consumable `.value`; it can expose `.ok` and `.status`, and discarding `.value` has
no effect. Check `.ok` before using a value as a successful result. HTTP response
codes, including 4xx/5xx, are successful `net.fetch` return values when the request
itself completes; transport failures are host errors. Existing human diagnostics
still accompany host failures.

Result values are represented as two Wasm values: payload then i32 status. String
payloads retain the existing immutable i64 slice representation and snapshot
ownership. Number/boolean payloads use i32; void uses an unused zero i32 payload.
Each logical result local/parameter maps to two physical slots, and a result
return uses two Wasm results. Field selection evaluates its base once and keeps
the selected component; `.ok` compares the saved status with zero. Captured results require
no heap allocation or extra host call. The existing 32 physical Wasm-parameter
limit remains, so a result consumes two of those slots. Raw host imports, global
exports, ABI v1, capabilities and artifact formats remain unchanged.

The built-in type cannot be redeclared or fabricated through object literals.
Nested result types, arbitrary generics, destructuring of a result pair, computed fields, field
writes and implicit conversion/comparison of a whole result are unsupported.
The shared editor declarations expose readonly fields and the same capture API.
This provides explicit branching on recoverable host failures, without introducing
general exceptions. Closed objects are provided separately. Fuel/deadline/call/output exhaustion, Wasm traps
and retained-string allocation failure still stop execution. An authorized
`process.exit` still exits with its requested code and produces no result;
a denied exit returns a failed `VartaResult<void>`.

## Resource limits

| Resource | Default limit |
| --- | --- |
| Guest linear memory | 8 MiB, enforced by Store limits including memory.grow |
| Managed string/record/array storage | Within the 8 MiB memory budget, after static page, 1 MiB host buffer and 64 KiB root stack; includes block metadata |
| Reference roots | 8192 handle slots across active frames and temporaries |
| One string value | 1 MiB of UTF-8 bytes |
| Object fields / shapes / key bytes | 32 / 128 across graph / 256 UTF-8 bytes |
| Array elements / element types | 1024 per array / 128 across graph |
| JSON serialization | 1 MiB encoded, depth 64, 4096 visited values per call |
| Checked JSON parsing | 64 KiB encoded input, depth 64, 4096 values; schemas at most 260 types / 64 KiB in the shared static pool |
| Guest instructions | 1,000,000 fuel units |
| Execution/HTTP deadline | 5 seconds |
| Host calls | 1024 per execution |
| Captured output | 1 MiB |
| Host input string | 64 KiB; narrower path/URL/name limits also apply |
| One host result | 1 MiB |
| HTTP response headers | 64 KiB |
| Wasm file | 4 MiB |
| Manifest/config | 256 KiB |
| Wasm signatures | 256 distinct complete parameter/result signatures |
| Function parameters | 32 physical Wasm values; each captured result uses two |
| Defined functions | 256 user functions, one HTTP adapter and at most ten compiler memory/string/array helpers |

Preflight limits also bound signatures, function count/body size, memories,
tables and globals before native compilation. Store budgets apply before Wasm
start functions execute. Fuel/epoch interruption bound guest execution; HTTP uses
the remaining deadline. Output is captured with a budget and returned to the
caller. Errors and guest exit do not terminate an embedding process.

## Artifact format and validation

Standalone Wasm has a strict sidecar `*.manifest.json`. A missing sidecar means
zero capabilities and the payload's declared host ABI (v1 when absent); an
unreadable or malformed sidecar is an error. Manifest/code ABI mismatch is an
error. Embedded
manifests are authoritative even when empty and never fall back to a sidecar.

`build --single` emits:

```text
native launcher | portable Wasm | manifest JSON | fixed 32-byte footer
```

The little-endian footer contains Wasm length (u64), manifest length (u64), format
version 1 (u32), reserved zero flags (u32), and `VARTA3\0\0` (8 bytes). It is read
at a fixed EOF position with checked arithmetic and capped lengths. Payload bytes
are never scanned for markers. Outputs are written through temporary files and
renamed into place.

Native AOT payloads, `VARTA1`/`VARTA2` containers and public native deserialization
are removed. Rebuild existing native artifacts. Native launchers currently compile
embedded portable Wasm at startup. Reintroducing cached/AOT machine code requires
a separate, authenticated provenance design; checksums alone are insufficient.

### Independent runtime companion and packaging

`varta-artifact::Artifact` owns portable/container I/O. The tooling re-export
`varta_project::Artifact` remains available; its I/O methods now return the shared
`VartaError`. The container layout/version is unchanged. Existing VARTA3 containers
with a development-CLI native prefix remain readable by both loaders; their own
native argument interface remains whatever was built into that prefix. Embedded
manifests always win over sidecars. This does not restore VARTA1/VARTA2 or AOT.

`varta build --single` resolves only the sibling `varta-runtime` executable, or
explicit `--launcher PATH`; it never searches cwd/PATH or copies the development
CLI as a fallback. Install/build both binaries. Plain Wasm builds need no companion.
The new executable is the independent runtime plus portable payload, not compiler
or source tooling. `CheckedProject::build(true, path)` follows the same validation.

The native companion is a trusted build input. Packaging first opens a bounded
regular file (512 MiB maximum) and copies it into private staging (0700 on Unix).
It closes/syncs the snapshot before executing it and rejects a recognized capsule,
including malformed capsules, so an already packaged application's main cannot
be used as a probe. `--runtime-info` must return strict launcher protocol v2
metadata matching package version, OS, architecture, pointer width, host ABI,
value ABI, HTTP contract and supported language-profile list. Protocol v1
companions must be rebuilt/updated for new builds; their existing VARTA3 payloads
remain readable. `--validate` then validates the staged portable Wasm
and manifest without instantiation, main, tool handlers or binding a listener.
Each subprocess has a five-second deadline and 4 KiB stdout/stderr limits. Only
after both probes pass are the public output files written. Probe failures leave
existing outputs untouched; staging is removed on success and error.

The protocol checks compatibility; it is not authentication or a sandbox for
arbitrary native inputs. Dependency guards independently enforce the repository
launcher's production graph. Native code executes with the builder/launcher's OS
authority; package authenticity remains separate planned work. Packaging currently
requires a runnable same-OS/CPU companion, not a cross-target executable. Linux
and macOS are the CI matrix; Windows and cross-target distribution are unverified.

The small runtime argument parser provides:

| Context | Interface |
| --- | --- |
| Packaged self | `[--policy PATH]`, `--invoke NAME [--params JSON] [--json] [--policy PATH]`, `--inspect [--json] [--policy PATH]` |
| Explicit portable/native artifact | `run PATH [--policy PATH]`, `invoke PATH NAME [--params JSON] [--json] [--policy PATH]`, `inspect PATH [--json] [--policy PATH]` |
| Build negotiation | `--runtime-info`, `--validate PATH` (bytecode/manifest validation only) |
| Help/version | `--help`, `--version` |

No source/project commands are included. Bare runtime execution without an artifact
fails. Unknown, repeated and misplaced flags fail. Invocation retains the checked
JSON envelope and typed error codes, never runs main, and applies the exact same
host/package conjunction. Inspection validates without executing and emits
`{artifact, manifest, module, toolCatalog, hostCeiling, combination:"allOf"}`.
`module` is the shared `WasmInspection`; top-level `toolCatalog` supplies generated
JSON input/output schemas for discovery. Development CLI inspection retains its existing format.
Internal `--validate` performs code/ABI validation, while the development CLI's
`validate` additionally reviews privileged-import declarations as described below.

`check`, `dev` and `build` share semantic/type/bytecode/ABI checks. Artifact `run`
and `inspect` fully validate bytecode and ABI too. `validate` additionally performs
policy review, requiring a declaration for every privileged import; this does not
mean that false or empty declarations grant access. Missing declarations are safe
to execute because the runtime denies those calls. Inspection includes full grant
scopes, enablement, versions, imports and exports.
