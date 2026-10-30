# Varta architecture and implemented contract

This document is the normative contract of the implemented compiler and runtime.
Planned work, including agreed designs that are not implemented yet, lives in
[development-plan.md](development-plan.md); its [status summary](development-plan.md#status)
records the current stage and verification. Varta compiles a checked TypeScript
subset, selected per module by an explicit language profile, to portable
WebAssembly and executes it in a capability-checked Wasmtime host. It does not
provide general JavaScript, Node or npm compatibility.

How this document is organized:

- [Version matrix](#version-matrix): every supported profile with its host ABI,
  value ABI, exception export and JSON imports, generated from the registries.
- [Compilation contract](#compilation-contract) and
  [Baseline value model](#baseline-value-model): the default `checked-i32-utf8-v1`
  profile and the rules shared by every profile.
- [Opt-in language profiles](#opt-in-language-profiles), in ascending order. A
  profile inherits its predecessor's contract unless its section states otherwise;
  additive contracts appear under the first profile that admits them.
- System boundaries: [modules](#local-modules), [diagnostics](#source-diagnostics),
  [capabilities](#capabilities-and-trust), [tools](#tool-contract-v1-and-offline-discovery),
  [HTTP](#http-service-v1), [ABI](#abi-v1-and-errors), [limits](#resource-limits) and
  [artifacts](#artifact-format-and-validation).
- Non-normative appendices: the [compatibility inventory](#developer-compatibility-inventory)
  and [runtime internals](#runtime-internals-non-normative).
- Third-party hosts (JavaScript, other Wasm engines): the separate
  [embedding contract](embedding.md) restates the host boundary per ABI version,
  with tables generated from the same registries.

Profile sections keep the negotiation details (launcher maxima, profile counts)
that applied when each increment landed. Current values appear only in the
generated tables, which a workspace test compares with the code.

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

## Version matrix

Source semantics, host ABI and value ABI have separate version identities (see
[Language profiles and migration](#language-profiles-and-migration)); several
profiles share a host or value ABI. This table, the launcher summary and the
[limits table](#resource-limits) are generated from the `varta-common` registries
and compared with them by `crates/varta-common/tests/documentation.rs`. When the
registries change, replace a block with the test's expected output.

<!-- generated:version-matrix:begin (checked by crates/varta-common/tests/documentation.rs) -->
| Profile | Host ABI | Value ABI | Exception export | JSON stringify | JSON parse |
| --- | --- | --- | --- | --- | --- |
| `checked-i32-utf8-v1` | 1 | 2 | — | `json_stringify` | `json_parse` |
| `checked-f64-utf8-v1` | 2 | 3 | — | `json_stringify_v3` | `json_parse_v3` |
| `checked-f64-utf8-v2` | 3 | 4 | — | `json_stringify_v4` | `json_parse_v4` |
| `checked-f64-utf16-v1` | 4 | 5 | — | `json_stringify_v5` | `json_parse_v5` |
| `checked-f64-utf16-v2` | 5 | 6 | — | `json_stringify_v6` | `json_parse_v6` |
| `checked-f64-utf16-v3` | 6 | 7 | — | `json_stringify_v7` | `json_parse_v7` |
| `checked-f64-utf16-v4` | 7 | 8 | — | `json_stringify_v8` | `json_parse_v8` |
| `checked-f64-utf16-v5` | 8 | 8 | — | `json_stringify_v8` | `json_parse_v9` |
| `checked-f64-utf16-v6` | 9 | 9 | — | `json_stringify_v9` | `json_parse_v10` |
| `checked-f64-utf16-v7` | 10 | 9 | `varta.exception.version` = 2 | `json_stringify_v9` | `json_parse_v10` |
| `checked-f64-utf16-v8` | 11 | 10 | `varta.exception.version` = 2 | `json_stringify_v10` | `json_parse_v11` |
| `checked-f64-utf16-v9` | 12 | 11 | `varta.exception.version_v3` = 3 | `json_stringify_v11` | `json_parse_v12` |
| `checked-f64-utf16-v10` | 13 | 11 | `varta.exception.version_v3` = 3 | `json_stringify_v11` | `json_parse_v13` |
| `checked-f64-utf16-v11` | 14 | 11 | `varta.exception.version_v3` = 3 | `json_stringify_v11` | `json_parse_v13` |
| `checked-f64-utf16-v12` | 15 | 11 | `varta.exception.version_v3` = 3 | `json_stringify_v11` | `json_parse_v13` |
| `checked-f64-utf16-v13` | 16 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v14` | 17 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v15` | 18 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v16` | 19 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v17` | 20 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v18` | 21 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v19` | 22 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v20` | 23 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v21` | 24 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v22` | 25 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v23` | 26 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v24` | 26 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v25` | 26 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v26` | 26 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v27` | 26 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v28` | 27 | 12 | `varta.exception.version_v3` = 3 | `json_stringify_v12` | `json_parse_v14` |
| `checked-f64-utf16-v29` | 28 | 13 | `varta.exception.version_v4` = 4 | `json_stringify_v13` | `json_parse_v15` |
| `checked-f64-utf16-v30` | 28 | 13 | `varta.exception.version_v4` = 4 | `json_stringify_v13` | `json_parse_v15` |
| `checked-f64-utf16-v31` | 28 | 13 | `varta.exception.version_v4` = 4 | `json_stringify_v13` | `json_parse_v15` |
| `checked-f64-utf16-v32` | 28 | 13 | `varta.exception.version_v5` = 5 | `json_stringify_v13` | `json_parse_v15` |
| `checked-f64-utf16-v33` | 29 | 13 | `varta.exception.version_v5` = 5 | `json_stringify_v13` | `json_parse_v15` |
| `checked-f64-utf16-v34` | 30 | 15 | `varta.exception.version_v6` = 6 | `json_stringify_v14` | `json_parse_v16` |
| `checked-f64-utf16-v35` | 31 | 16 | `varta.exception.version_v7` = 7 | `json_stringify_v15` | `json_parse_v17` |
| `checked-f64-utf16-v36` | 32 | 17 | `varta.exception.version_v8` = 8 | `json_stringify_v16` | `json_parse_v18` |
| `checked-f64-utf16-v37` | 33 | 17 | `varta.exception.version_v8` = 8 | `json_stringify_v17` | `json_parse_v18` |
| `checked-f64-utf16-v38` | 34 | 17 | `varta.exception.version_v8` = 8 | `json_stringify_owned` | `json_parse_v18` |
<!-- generated:version-matrix:end -->

<!-- generated:launcher-summary:begin (checked by crates/varta-common/tests/documentation.rs) -->
Launcher protocol **2** advertises host ABI **34**, value ABI **17**, HTTP contract **1** and **41** language profiles (the rows above).
<!-- generated:launcher-summary:end -->

The `varta` tool and the `varta-runtime` companion must be rebuilt together
whenever the supported profile list or these maxima change.

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
lexical complexity limits before recursive parsing: delimiter nesting at most 64
and at most 256 consecutive chain tokens without a sibling separator, because
operator, member and unary chains build nested AST nodes. Flat sibling lists do
not deepen the AST and reset the run: commas inside `[...]`/`{...}` (call
arguments keep the chain budget) and `|`/`&` members at the top level of a
`type` alias outside generic angle syntax. Template text is one token per head,
middle or tail, and each substitution is one nesting level; the check rescans
template text after a substitution like the parser, so template contents and
later comments never count as code. There is no per-file token count;
overall size is bounded by the per-file and per-graph byte limits. Static strings occupy at most
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
| Explicit supported profile + its matching host ABI ([version matrix](#version-matrix)) | `profile:"<name>", origin:"declared"` |
| ABI v1 without a language section (older Varta output) | Same profile, `origin:"legacyHostAbi"`; existing behavior is preserved |
| Neither section (generic portable Wasm) | `profile:null, origin:"unspecified"`; no claim about source-language semantics |
| Unknown/invalid/duplicate profile or ABI | Reject; no fallback to the current profile |
| Any host ABI above v1 without an explicit matching profile | Reject; only ABI v1 has a legacy fallback |

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

## Baseline value model

These contracts define the default `checked-i32-utf8-v1` value model. Later
profiles build on them; a profile section states each change.

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

Launcher protocol v2 advertises supported host/value ABI maxima (currently 24/12);
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

## Opt-in language profiles

Profiles are listed in ascending order. Each section names what the profile adds;
its selectors are in the [version matrix](#version-matrix).

### `checked-f64-utf8-v1` — Binary64 numbers

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

### `checked-f64-utf8-v2` — Distinct undefined

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

### `checked-f64-utf16-v1` — UTF-16 strings

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

### `checked-f64-utf16-v2` — Primitive unions

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

### `checked-f64-utf16-v3` — Missing values

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

### `checked-f64-utf16-v4` — Reference unions

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

### `checked-f64-utf16-v5` — Checked optional and union JSON

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

### `checked-f64-utf16-v6` — Resizable arrays

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

#### Relative array access and shallow slicing

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

#### Dense array search

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

### `checked-f64-utf16-v7` — Primitive exceptions

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

### `checked-f64-utf16-v8` — Error objects

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

### `checked-f64-utf16-v9` — Native error families

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

#### Additive String.repeat operation

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

### `checked-f64-utf16-v10` — Recoverable checked JSON

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

### `checked-f64-utf16-v11` — Typed function values

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

### `checked-f64-utf16-v12` — Lexical closures

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

#### String range contract

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

#### String search contract

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

#### Additive Array callback operations

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

#### Additive Array reduction and quantifiers

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

#### Contextual callback return and reduce seed inference

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

#### Anonymous function expressions and typed this

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

#### Named-expression self bindings and declaration receivers

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

#### Optional, default and rest parameter contract

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

#### Bounded spread-call contract

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

#### Parameter destructuring contract

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

#### Local lexical destructuring contract

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
captures; earlier profiles reject mutable captured union narrowing, even when capture appears
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

#### Checked destructuring assignment contract

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

#### Destructuring member target contract

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

#### Destructuring assignment value contract

V12+ checked pattern assignments are expressions with the actual contextual RHS
type and original value as their success result. Preserve source record/array
identity, unselected fields and sparse presence; defaults/rest traversal changes
only targets, while user-authored mutations observe normal reference identity.
Admit results in local initializers, arguments/receivers, returns, member reads,
callbacks, supported conditional/operator positions and nested pattern assignment
chains. Standalone and for init/update statements lower through the same value
path and discard the result. Computed record and ordinary simple assignment values
are specified above, along with the checked arithmetic/logical compound subset and
Number updates. Dynamic record keys in patterns, catch and for-in remain outside
this increment.

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

#### Array for-of contract

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

#### Do-while contract

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

#### Statement label contract

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

#### Catch binding pattern contract

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

#### Dynamic record own-key contract

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

#### Computed record read contract

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

#### Computed record write contract

V12+ admits `record[key] = rhs` as a simple assignment statement, including outer
parentheses and ordinary for initializer/update positions. Require a narrowed
closed-record receiver and narrowed String key. Literal String/Number paths and
earlier-profile behavior remain unchanged. The value-producing extension is below.
Dynamic-key arithmetic compounds and Number updates are specified below. Reject
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

#### Computed record assignment value contract

V12+ extends computed closed-record assignment to expression positions. Return the
original RHS value and resolved type only after a successful store, following the
admitted [assignment evaluation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-assignment-operators-runtime-semantics-evaluation)
and its returned RHS value. Dynamic String keys retain the preceding write contract.
Computed literal String/Number keys retain static field lookup, known-field type
checks and unknown/inherited-key compile errors. Require narrowed closed-record
receivers; reject non-String dynamic keys and unnarrowed unions/nullish receivers.
Dynamic-key arithmetic compounds, Number updates and local/dot/array values are
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

#### Simple assignment value contract

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

#### Arithmetic compound assignment contract

V12+ admits +=, -=, *=, /= and %= for mutable initialized local/parameter bindings,
static declared record fields (dot or literal computed names) and numeric array
references. V14 also admits length/literal-["length"] targets. Support statements,
for init/update and existing enclosing expression positions, returning the computed
assigned result. Reject const/TDZ/guard-refined targets, unchecked receivers,
String/coercive array keys, Error/result mutation,
destructuring targets and older
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

#### Dynamic record compound assignment contract

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

#### Logical assignment contract

V12+ admits &&=, ||= and ??= for mutable initialized local/parameter bindings,
static declared record fields, dynamic narrowed String record keys and numeric
array elements. V14 also admits checked length. Preserve existing literal-key
diagnostics, full static type compatibility, const/TDZ/guard-refinement checks,
dynamic-read union admission and selected-field type checks. RHS types are checked
even when a runtime branch would skip. Reject result-pair targets, Error/result
field mutation, String/coercive array keys, logical patterns and earlier
profiles. Function bindings keep signature/arrow context and closure identity.

Prepare/root receiver and key/index once, then read/cache the old value once.
Select the RHS/store branch for truthy old values with &&=, falsy values with ||=,
or only Null/Undefined with ??=. Existing checked truth rules treat zero/-0/NaN,
false, empty String and Null/Undefined as falsy; records/arrays/functions/Errors are
truthy. No coercion or source-local guard refinement is introduced. Reuse existing
pure truth/nullish operations, including exact union tags and UTF-16 lengths.

The skipped branch returns the original old value and performs no RHS effects,
write checks, payload changes or optional/sparse presence/order publication.
The selected branch evaluates RHS once, then stores a compatible/widened copy
through the saved reference and returns the original RHS type/value only after
success. RHS rebinding or target mutation cannot redirect the reference. Exceptions
retain their original values/effects and skip the requested store/later operands;
outer failures retain completed inner writes. A skipped outer assignment evaluates
no inner receiver/key/RHS at all. Fatal RHS resource limits and authorized exit
run only if selected, then retain their fatal behavior through catch/finally.

Result types join the RHS with possible skipped old members. &&= excludes
always-truthy reference/function/Error members from its skipped type; ||=/??=
exclude Null/Undefined. Keep Number/Boolean/String members that may be partially
truthy, and never narrow RHS from runtime field or tag choices. Unbox/retype old
union cells only in a proven skipped branch. Same-type function results stay
function handles without inventing unsupported function unions. Preserve exact
Number bits, UTF-16 strings, reference identity and complete original graphs.

Optional/dynamic reads retain their existing missing semantics. Unknown ordinary
record keys and absent fields can skip &&= as Undefined; selected dynamic writes
validate the complete selected type after RHS and unknown names raise TypeError
without property creation. Undeclared inherited names fail during the old-value
read, before RHS, while declared own names remain supported. Dynamic keys retain
the bounded all-fields-plus-Undefined union and exact UTF-16 lookup.

Numeric arrays use MaybeIndex for their old read, so missing/noncanonical reads
can skip &&= without attempting a fatal write. Selected writes validate raw indices
after RHS against the current backing/length/presence: v12/v13 require current
in-range indices, v14 can extend/recreate valid indices with holes. Invalid writes
stay fatal. V14 selected length stores keep RangeError/non-uint32 and fatal >1024
checks; skipped length writes do not evaluate or validate an invalid RHS value.
Keep all compilation, source/type/frame/root/value/memory/fuel/deadline budgets.

Nested ValueBlocks root prepared inputs, old values and branch results, preserving
earlier temporary floors/physical pairs and transferring results before cleanup
without allocation. Saved returns, callbacks and captured graphs survive GC and
finalizers. Layout/effect visitors traverse all branch bodies, including RHS/store
effects in runtime-skipped branches; helper validation must detect host effects in
each source/key/RHS position. No new host operation/import, emitter helper, helper
export or ABI/schema/layout version. Existing status/grants/host ceilings persist.
Regressions cover truth/nullish edges, result-type pruning, optional/sparse presence,
prepared identity, selected field checks, function context, partial chains, GC/pairs/
finalizers, fatal skips and failures, source ranges, virtual modules and source-free
Wasm/native artifacts with branch-specific operation permissions/host ceilings.

#### Number update contract

V12+ admits prefix/postfix ++ and -- for mutable initialized locals/parameters,
declared static record fields, dynamic narrowed String record keys and numeric
array elements; v14 also admits checked length. Parenthesized simple references
work. Reject const/TDZ/guard-refined writes, targets with no Number member,
function/result-pair targets, Error/result fields, coercive keys and older
profiles. Dynamic reads keep their all-fields-plus-Undefined union restrictions;
literal fields retain exact static types and diagnostics.

Prepare/root receiver and key/index once, in order, then read the old value once.
Key effects may change the field/element before that read or rebind its source
variable, but cannot redirect the saved reference. A static target without Number
fails at its own source span. Otherwise pure IsUnionMember validates the actual
Number tag before unboxing; a non-Number raises owned TypeError `Invalid update
operand` without a requested write. Strings (including numeric text), Boolean,
Null/Undefined, references and Errors are never coerced. Inherited non-own names
retain their read-time TypeError; unknown ordinary names/absent fields read
Undefined and fail Number validation. Do not create properties or publish missing
optional presence/order on a failed update.

Cache the checked old binary64 Number, compute old + 1 or old - 1 through existing
Binary IR, and store a compatible/widened copy using the original prepared
reference and existing checked setters. Keep separate old/new Number slots.
Only after a successful store does prefix return the new value or postfix return
the original old value; both have exact Number result types even for optional or
union targets. Postfix preserves the old Number bits, including negative zero.
Arithmetic keeps existing NaN/infinity/fraction/large-number rounding behavior.
A failed store exposes neither expression result, skips later operands/enclosing
work and retains prior effects and completed updates. Mixed assignment/logical/
update chains retain normal evaluation order and skipped logical branches run no
inner update effects. Original source/key exceptions survive catch/finally.

Array old reads use MaybeIndex: holes, explicit Undefined, out-of-range and
noncanonical Number indices fail the Number check before any write attempt.
This neither extends a v14 array nor turns a write budget trap into a catchable
error. Successful stores retain current backing/presence/length checks. V14
length growth/shrink preserves holes and backing invariants; a decrement from
zero raises RangeError without publishing either a store or postfix old result,
while growth above 1024 remains fatal. Fuel/memory/deadline/source/type/frame/root/
value budgets and authorized exits retain their fatal behavior.

Use nested ValueBlocks and existing temporary floors/root transfer for prepared
receivers/keys, full old union graphs, earlier arguments/physical result pairs and
saved returns through allocation, calls, callbacks, GC and finalizers. Captured
bindings update their original cells; for updates operate on the existing fresh
iteration cell sequence, preserving earlier closure captures and continue paths.
All layout/effect visitors traverse source/key/read/tag-check/store/failure bodies.
Host-free helper controls using prefix/postfix must validate; a host effect in
either source or key must remain detectable. Numeric updates introduce no host
arithmetic, operation/import, emitter helper, helper export or ABI/schema/layout
version. Preserve system status, operation grants and host ceilings. Regressions
cover binary64 edges, prefix/postfix results, profile/type boundaries, optional/
sparse absence, exact keys/all field bits, nested order/partial effects, captures/
fresh loop cells, roots/pairs/saved returns, source ranges, fatal limits and
virtual/source-free Wasm/native execution with allowed/denied operations.

#### Number bitwise contract

V12+ admits &, |, ^, ~, <<, >> and >>> for already narrowed binary64 Number
operands. Check each operand's resolved type before ordinary union/missing-value
dispatch and report a type error at the offending operand span. No implicit
ToNumber conversion of String/Boolean/Null/Undefined/references/opaque results is
added. Union, optional and array-read operands require explicit source narrowing,
as in ordinary arithmetic. Older profiles remain rejected; bitwise compounds are
specified below.
Use existing typed Binary IR and a shared numeric operator classifier; lower ~x
to x xor -1, retaining its source effects exactly once.

Follow the Number parts of
[ToInt32](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-toint32),
[ToUint32](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-touint32)
and [Number shifts/bitwise operations](https://tc39.es/ecma262/2025/multipage/ecmascript-data-types-and-values.html#sec-numeric-types-number-leftShift).
Finite values truncate toward zero and wrap modulo 2^32; nonfinite values and both
zeroes map to positive zero. Signed operations interpret the resulting word as
two's complement; >>> keeps the unsigned word. Shift counts use the unsigned low
five bits. Return ordinary binary64 Number values: signed 32-bit range except >>>'s
0..4294967295. Do not introduce an integer value type or change numeric aggregate
slots, roots, JSON descriptors or external signatures.

Evaluate both binary operands once, left to right, before conversion. Emit inline
ToUint32 by reinterpreting binary64 bits, extracting the encoded exponent and
masked fraction, and restoring the hidden significand bit only in the admitted
normal exponent range. Exponents below the bias truncate to zero; unbiased
exponents >=84 describe multiples of 2^32 and yield zero, as do NaN/infinity.
For remaining exponents, shift the significand right to discard fractional bits
or left to position integer bits, then wrap the low word and apply the original
sign modulo 2^32. Bound shifts before emitting them; never depend on wrapping a
large shift count to repair an out-of-range exponent. No trapping/saturating
float-to-integer cast, host remainder, allocation or guest helper call is used.

Cache the already evaluated RHS in a declared f64 scratch slot, convert the LHS
using existing i64/i32 scratch slots, then convert the saved RHS. No user code or
allocation runs during scratch reuse. Apply Wasm i32 AND/OR/XOR/shift operations;
Wasm masks shift counts to five bits. Convert the result back to f64 with signed
conversion except for >>>. Numeric bits temporarily stored in an i64 scratch must
never become shadow roots or heap pointers. Nested bitwise/assignment/update
expressions and earlier String/aggregate/result-pair operands retain their normal
stack/root lifetimes; body visitors must still see every operand effect.

Conversion adds no host operation/import, emitter helper, helper export or ABI/
schema/layout version. Preserve pure-operation system status, exact permissions
and host ceilings. Original operand exceptions skip later effects; fuel/memory/
deadline exhaustion and authorized exit remain fatal through catch/finally. All
existing source/type/code/frame/root/value/resource budgets stay active.

Regressions execute every operator through Wasmtime, narrowed unions/callbacks,
nested effects/GC/finalizers/physical pairs, precise dependency ranges, virtual
modules and source-free Wasm/native artifacts. An independent truncation/remainder
oracle checks all 2048 binary64 encoded exponent ranges, both signs, four fraction
patterns and 1024 seeded payloads, totaling 17408 inputs and 121856 results. Fresh
small batches keep the ordinary fuel ceiling; production limits are unchanged.
Pure helper controls must validate while left/right/unary host effects remain
detectable. Allowed/denied operand operations, unchanged status and fatal exit are
covered across development, portable and native execution.

#### Bitwise compound assignment contract

V12+ admits &=, |=, ^=, <<=, >>= and >>>= for mutable initialized locals/parameters,
declared static/dynamic record fields and numeric array elements; v14 also admits
checked length. Reuse the arithmetic compound reference/cache/store pipeline,
existing Number bitwise conversion and complete selected-field types. Keep
const/TDZ/guard-refinement, coercive-key, function/result/Error-field and earlier
profile restrictions. Dynamic records retain the bounded computed-read union and
exact UTF-16 lookup, while literal keys retain precise static checks.

Evaluate/root receiver and key/index once, then read/cache the old value before
RHS. A key's effects precede this read. Evaluate/cache RHS once before validating
either operand. Require actual Number tags on both sides, using pure union-member
checks before unboxing; no String/Boolean/Null/Undefined/reference coercion is
introduced. Static target/RHS types without a possible Number pair fail at their
own span. Otherwise incompatible runtime members raise owned TypeError `Invalid
compound bitwise operands` after RHS success, with no requested store. Preserve
the original RHS exception and its prior effects on failure. Ordinary bitwise
expressions still require source narrowing.

Compute through the existing full-range ToInt32/ToUint32 operations, with masked
shift counts and signed results except >>>'s unsigned result. Return exactly the
computed Number after successful storage, widening only a storage copy. Values
through 4294967295 remain ordinary binary64 Number slots/cells. Do not re-read an
old value, redirect the cached receiver/key or validate raw RHS as an array length.
Nested compounds cache outer references/old values before inner work; inner
failure skips outer stores and later operands, while outer failure retains
completed inner writes and effects.

Unknown ordinary record names, absent optional fields and explicit Undefined
produce invalid old operands. Evaluate RHS, then throw without property creation
or presence/order publication. Undeclared inherited names fail during the read,
before RHS; declared own inherited-looking names work normally. Numeric arrays
retain MaybeIndex read semantics: holes/out-of-range/noncanonical reads fail the
Number check after RHS without attempting a write or converting a budget trap.
On success, existing setters select current backing/presence/length: v12/v13
post-RHS out-of-range writes stay fatal, v14 may recreate/extend valid indices.
Selected length stores retain RangeError/non-uint32 and fatal >1024 behavior
against the computed result. Source/key/RHS exhaustion and authorized exit remain
fatal through catch/finally, including when an operand would later fail its tag
check. No resource limit is raised.

Existing nested ValueBlocks/floors root original receivers/keys, complete old/RHS
graphs and returned values, preserving earlier physical pairs and saved returns
through GC/calls/callbacks/finalizers. Visitors must see all read/check/compute/
store/failure branches. Pure helper controls must pass; isolated host effects in
source, key or RHS must remain detectable. Conversion itself has no host effects
or allocation, preserving system status and exact grants/host ceilings. No new IR
kind, host operation/import, helper export or ABI/schema/layout version.

Regressions cover all six operators and numeric edges, original-reference/old
snapshots, strict union kinds, absence/inherited timing, exact keys/all field bits,
mixed partial chains, sparse/current storage/length checks, GC/pairs/returns,
source diagnostics, helper purity, fatal domains, virtual modules and source-free
portable/native artifacts with allowed/denied operations and host ceilings.

#### Object.keys contract

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

#### Object.values contract

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

#### Array own enumeration contract

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

### `checked-f64-utf16-v13` — Sparse arrays

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

### `checked-f64-utf16-v14` — Array mutations

`checked-f64-utf16-v14` selects **host ABI v17 with unchanged value ABI v12/schema
v12**. The new source profile freezes indexed extension and writable-length rules
that differ from v13's checked existing-index-only writes. Sparse vector/backing,
record, Error, closure and exception layouts, v6 value operations and the
json_stringify_v12/json_parse_v14 boundary remain unchanged. There is no new host
import or capability. Earlier profiles retain their rules and helper ceilings;
launcher protocol v2 negotiates the profile and current advertised maxima; v14's
pair remains 17/12. The v15 power profile below raises the host maximum to 18.

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

### `checked-f64-utf16-v15` — Number exponentiation

`checked-f64-utf16-v15` selects **host ABI v18 / value ABI v12 / JSON schema v12**.
It inherits v14's source features, heap/exception layouts and v6 value/checked-JSON
import selectors. The default stays `checked-i32-utf8-v1`; v1–v14 artifacts retain
their declared contracts, and their compilers keep rejecting ** and **=.
Mandatory language/ABI18 metadata prevents old runtimes from silently accepting
the new profile even if they ignore a future language section. A v15 module needs
the matching ABI18 declaration even when no power import is emitted.

The single new registry operation is `FloatPower`, import
`varta.number_power : (f64,f64)->f64`, minimum ABI18. It is pure: no capability,
guest pointers, allocation, result buffer, system-status reset or pending-channel
mutation. Shared registry names/signatures/minimum ABI drive compiler, runtime
and artifact checks. Unknown imports, wrong physical signatures, profile/ABI
mismatches and downgrade attempts fail before guest effects. No manifest permission,
value/schema/container version or resource-limit change is introduced.

Plain ** requires narrowed Number operands and reports incompatible types at
their own source ranges. Standard parsing preserves right associativity and the
unparenthesized-unary-base early error. Compound **= shares prepared arithmetic
references: root/cache receiver/key and old value before RHS, evaluate/cache RHS,
then validate actual Number tags before native math. Statically impossible Number
pairs fail at target/RHS spans; other runtime kinds raise owned TypeError `Invalid
compound exponentiation operands` after RHS without coercion or a requested store.
Missing/optional/sparse reads, inherited record timing, original exceptions and
partial inner writes retain existing contracts. Store the computed Number through
current backing/presence/length checks, widening only storage copies. Computed NaN,
infinity or fractional length results raise RangeError; valid lengths above 1024
remain fatal. Ordinary numeric results may be nonfinite, as in earlier profiles.

Follow [Number::exponentiate](https://tc39.es/ecma262/2025/multipage/ecmascript-data-types-and-values.html#sec-numeric-types-number-exponentiate):
check exponent NaN before either exponent zero, then base NaN. Handle signed
infinity/zero using exponent sign and odd-integral parity. Infinite exponents use
the absolute base's relation to one, with either ±1 producing NaN. A finite
negative base with a nonintegral exponent produces NaN. This preserves NaN**0=1,
1**NaN=NaN and the ECMAScript behavior that differs from IEEE pow for unit bases.
Ordinary finite power uses `abs(base).powf(exponent)` and restores a negative sign
only for an odd integral exponent. Parity checks operate on finite binary64 values
without narrowing integer casts, including the 2^53 odd/even boundary. Preserve
signed zero on underflow and signed infinities on overflow. Finite ordinary results
are platform implementation-approximated; cross-platform last-bit identity or
universal correct rounding is not promised. No BigInt, ToNumber coercion
or exponent-sized guest loop is added.

The runtime charges one existing host-operation unit before math and rechecks
the same budget/deadline afterward without a second increment. Native scalar math
does no IO or input-sized allocation. Existing host-call, fuel, deadline, memory,
source/type/frame/root/value/output limits remain active. Budget exhaustion and
authorized exit are fatal through catch/finally, including during RHS evaluation
before a later operand error. Pure math preserves prior status and grants/host
ceilings. Host-free JSON allocation/retention helpers must still reject any
reachable power import; purity does not exempt it from the no-host-call rule.

The compiler uses existing Binary IR and a resolved registry import index, and all
body visitors see both operands and compound read/check/store branches. Existing
temporary floors/root transfer preserve original graphs, earlier physical pairs,
captured cells, callback results and saved returns through GC/calls/finalizers.
No new helper export or Wasm EH mechanism is introduced. Launcher protocol v2
advertises v15 with ABI18/value12; its current maximum26/value12 also includes v16/v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27.
Native packaging
requires a matching rebuilt trusted companion and probes the private snapshot;
portable and fixed-footer native container formats stay unchanged.

Tests cover special-case bits/parity/subnormals/overflow, independent sqrt accuracy,
one-call accounting, plain narrowing/compound Number guards, prepared effects,
sparse/length/JSON/closure inheritance, partial chains/GC/pairs/finalizers, strict
old-profile and hostile metadata/signature rejection, helper purity, exact source
ranges, virtual modules and source-free portable/native artifacts with operation
grants, host ceilings, status and fatal exits/budgets.

#### Checked Math intrinsic subset

The v15 compiler also admits direct unshadowed `Math.abs`, `Math.floor`,
`Math.ceil`, `Math.trunc`, `Math.round`, `Math.sqrt`, `Math.sign`, `Math.fround`, `Math.f16round`
and `Math.clz32` calls with exactly one narrowed Number argument, `Math.imul`
and `Math.pow` with exactly two, and `Math.min/max/hypot` with zero or more. The result is Number.
This additive source subset
retains v15/host18/value12/schema12 and all artifact/container negotiation.
Older profiles reject all fifteen. No new registered host import, helper export, capability,
dependency, layout or runtime budget is added.

The numeric semantics follow the admitted Number part of
[ECMAScript Math](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-math.round),
without ToNumber coercion. Abs clears the sign, including -0. Floor/ceil/trunc
preserve nonfinite values and both zeroes, including negative-zero results for
ceil/trunc of values between -1 and 0. Sqrt uses binary64 Wasm sqrt, preserves -0,
and returns NaN for negative nonzero inputs. NaN payload/sign are unspecified.

Round returns the closest integral Number with ties toward +Infinity, retaining
-0 for [-0.5,0). Lower it by caching the completed argument once, computing
`nearest(x)`, and adding one only when `x - nearest(x) == 0.5`. This adjusts ties
where Wasm's ties-to-even selected the lower integer. All other values, including
nonfinite values, zeroes, integral large numbers and halfway neighbors, keep the
nearest result. Do not use `floor(x + 0.5)`: addition rounding changes some non-ties
and loses negative zero. Scratch locals are used only after all argument effects
finish; nested intrinsics cannot overwrite an outer unfinished computation.

Sign preserves NaN and both zeroes; otherwise return one with the argument's
sign using Wasm copysign. Fround demotes f64 to f32 with ties to even, then promotes
back to f64, retaining signed overflow/underflow and binary32 subnormals. Clz32
uses the existing exact full-range ToUint32 lowering followed by i32.clz and
unsigned f64 conversion. Imul evaluates both arguments before scratch use,
converts each with the same ToUint32 lowering, multiplies modulo 2^32 and converts
the signed i32 result to Number. Do not multiply the f64 arguments first: that
loses product bits above the binary64 exact-integer range.

Min/max start with +Infinity/-Infinity and fold each completed argument using
Wasm f64.min/max. These instructions propagate NaN and choose -0/+0 on zero ties.
The accumulator stays on the Wasm stack across later argument calls and GC; no
scratch slot or argument buffer is retained. Every argument still evaluates once
in source order after NaN; thrown arguments skip later arguments and propagate
normally. The empty call returns its initial infinity; single -0 remains -0.
Folding keeps operand-stack usage independent of argument count; source and
execution budgets continue to bound the call without a new resource limit.

Only a direct dot call is admitted. Reject shadowing (including parameters,
captures, imports and TDZ bindings), computed names, aliases/borrowed methods,
non-callable properties as callees, other methods, generic calls, spread arguments
and incorrect fixed arity.
Non-Number and unnarrowed-union arguments produce a type diagnostic at the argument.
Arguments evaluate once, retain their host effects and exception channel, and
preserve earlier roots/pairs and saved returns through allocation/finalizers.
Compiler collection/effect visitors recurse through every argument in typed Math IR.

The fourteen guest-only operations allocate no values, preserve system status and
consume guest fuel without consuming a host-call allowance. Math.pow instead
collects/emits the existing FloatPower/number_power host operation (minimum ABI18),
with exactly the special-case bits, finite approximation, one-call/deadline budgets
and status behavior of **. Math.pow evaluates its two arguments once left to right;
no new operation registry entry, permission, helper export or profile is required.
Host-free JSON helper validation admits pure guest-only argument trees but rejects
Math.pow, including nested aliases through user functions, and all reachable host
effects within arguments. Operator/method differential tests cover special input
pairs and verify one host allowance per Math.pow call.
Existing tool/HTTP binary64 adapter restrictions remain. Tests execute the operations
over signed zero, subnormals, infinities, NaN, half-integer neighbors and every
binary64 exponent with representative significands, with fresh bounded stores;
they also cover types/shadowing/profile gates, roots/pairs/finalizers, argument
effects, fatal fuel/exit, source ranges, modules and source-free artifacts.
Binary32 halfway/subnormal/overflow boundaries, all-exponent integer conversions,
signed low-bit products and variadic NaN/exception ordering have dedicated checks.

#### Scaled variadic Math.hypot

V15 `Math.hypot` admits zero or more narrowed Numbers and follows the admitted
numeric part of [ECMAScript hypot](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-math.hypot).
Evaluate every argument once in order before selecting a result. Any ±Infinity
returns +Infinity even if another input is NaN; otherwise NaN propagates. Empty
or all-zero calls return +0. Finite results remain implementation-approximated;
universal correct rounding or bit identity with native libraries is not promised.

Lower through existing ValueBlock/statement IR. Cache absolute input values in
hidden f64 locals; these immutable Number snapshots survive later argument calls,
scratch use and GC. After all inputs finish, find the maximum and track NaN, then
select Infinity/NaN/zero before numeric accumulation. For the finite positive case,
the internal BinadeScale Math mode clears significand bits to obtain a power of
two, clamped to MIN_NORMAL for subnormal maxima. It is not a source Math method.
Divide inputs by this exact scale rather than by the maximum magnitude: that
avoids ratio rounding for relevant inputs and final non-power-of-two multiplication
rounding near overflow. Squared ratios stay bounded; tiny irrelevant terms may
underflow under the documented finite-approximation contract.

Dekker splitting with 2^27+1 tracks each scaled square's product error. Compensated
summation includes those errors, then a square-root residual correction accounts
for the rounded root square and remaining sum correction. Rescale the corrected
root by the power of two. Hidden working locals are unique per lowered call;
the shared ValueBlock root floor protects earlier outer operands/pairs while
arguments allocate, throw or run finalizers. No new IR value representation,
host import/export, heap allocation, helper export, dependency, ABI version,
permission or resource limit is added. Source/frame/fuel budgets bound variadic
work; source Math type/shadowing/spread/generic/borrowing gates remain unchanged.

Tests check all binary64 exponents and representative significands against an
independent native implementation (sampled finite error bounded to 3 ULPs), exact
scaled 3/4/5 triples through subnormals, compensated sub-ULP sums and special cases.
Near-overflow goldens are independently generated at 200 and 400 decimal digits:
native libm itself can spuriously overflow there, so those cases use the stronger
oracle instead of assuming libm is exact. Argument effects after Infinity/NaN,
exceptions, pairs/GC/finalizers, helper purity, old profiles, source-free/public APIs
and operation-specific grants/host ceilings/fatal exit are covered.

#### Direct binary16 rounding

V15's `Math.f16round` takes one narrowed Number, following the admitted numeric
part of [ECMAScript f16round](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-math.f16round).
Round binary64 directly to binary16 using ties to even, then return the exact
rounded value as binary64. NaN payload/sign are unspecified; infinities and both
zeroes retain their values/signs. Non-Number/unnarrowed-union inputs, incorrect
arity, spread, generics, borrowing/computed calls and shadowed Math are rejected
under the existing checked Math admission rules.

Emission caches the completed argument once, then uses only MVP guest instructions.
If abs(x)<2^-14, compute nearest(x*2^24)*2^-24. These power-of-two rescalings are
exact, including binary64 subnormal inputs, and retain negative-zero underflow.
Otherwise, for finite abs(x)<65520, extract the binary64 exponent e, compute
nearest(x*2^(10-e))*2^(e-10), and return the exact rounded half value. Both scale
factors are constructed from bounded exponent bits. This rounds the half's
11-bit significand without binary32 conversion or an intermediate inexact addition.
At/above the overflow midpoint 65520, multiply x by Infinity to produce signed
infinity; NaN also takes that branch and propagates. No host math, helper export,
allocation, new local type, ABI/value/layout change, permission or limit is added.

Scratch is used only after all argument effects finish; nested calls, exceptions,
GC roots, result pairs and finalizers retain the ordinary expression behavior.
Pure JSON helpers can use the operation, but reachable argument host effects
still fail validation. Tests use an independent integer-significand oracle over
all 65,536 binary16 encodings, every finite adjacent-half midpoint and its two
binary64 neighbors (both signs), overflow neighbors, all binary64 exponents and
representative significands, and the binary32 double-rounding counterexample.
Fresh bounded stores keep production fuel unchanged. CLI/public-API tests cover
modules, source-free Wasm/native artifacts, argument ranges, grants and fatal exit.

#### Number predicate intrinsics

V15 admits direct unshadowed `Number.isFinite`, `Number.isNaN`, `Number.isInteger`
and `Number.isSafeInteger` with Boolean results. They implement the non-coercing
[ECMAScript Number predicates](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.isfinite)
over all admitted input types. Missing first arguments and non-Numbers return
false. Additional arguments evaluate once in source order and are ignored, with
their effects/errors/fatal limits preserved. Spread, generics, computed names,
borrowed methods, other Number methods and shadowed Number bindings are rejected.
Older profiles reject the operations; v15/ABI18/value12/schema12 remains unchanged.

Numeric inputs use f64 checks: finite means abs(x)<Infinity; NaN means x!=x;
integer additionally requires trunc(x)==x, excluding infinities. Safe integers
must be integral with abs(x)<=9007199254740991. Both zeroes are finite/integral/safe.
Unnarrowed union cells are checked for the Float64 tag before loading their payload
bits; other tags return false. Non-Number values are discarded using their full
physical width, including both slots of captured result pairs. No coercion, trap
on non-Numbers or direct-local type refinement is introduced.

The predicate depends only on the immutable first Number/type, so its canonical
i32 Boolean can be computed after the first argument and kept on the Wasm stack
while ignored arguments execute. This preserves the original first value through
later binding changes, nested scratch use, allocations and pair-returning calls.
An exception in an ignored argument discards the saved Boolean normally; no result
escapes before all arguments finish. Collection/effect visitors traverse every
argument in the typed NumberPredicate IR, and JSON helper validation rejects any
reachable host call in first or ignored argument trees.

No host import, helper export, capability, dependency, ABI/layout version or budget
is added. Predicates preserve status and use guest fuel without host allowance.
Tests validate and execute all binary64 exponents/representative significands,
safe-integer boundaries, all admitted non-Number/union types, missing/ignored
arguments, exceptions, pairs/GC/finalizers, host-free helpers, shadowing/old profiles,
modules/source-free artifacts and exact grants/host ceilings/fatal exit/fuel.

#### Numeric constant properties

V15 admits all eight Math numeric constants (`E`, `LN10`, `LN2`, `LOG10E`,
`LOG2E`, `PI`, `SQRT1_2`, `SQRT2`) and eight Number constants (`EPSILON`,
`MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `MAX_VALUE`, `MIN_VALUE`, `NaN`,
`NEGATIVE_INFINITY`, `POSITIVE_INFINITY`). They follow the Number values in the
[ECMAScript Math/Number contracts](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-value-properties-of-the-math-object).
MIN_VALUE is binary64 bits 1, not MIN_POSITIVE (the smallest normal); safe-integer
bounds are ±9007199254740991. NaN payload/sign are unspecified. Exact finite bit
patterns, both static property syntaxes and special values have execution tests.

Only an unshadowed Math/Number identifier (optionally parenthesized), followed by
a dot name or computed literal String key (optionally parenthesized), is intrinsic.
Lexical bindings, including outer captures, function declarations and TDZ bindings,
take precedence and follow ordinary member rules. In particular, a local record
named Math with its own PI can be read/written normally; it is never folded to π.
Dynamic keys, namespace values/aliases, descriptors and unknown properties remain
unsupported. Older profiles reject intrinsic reads but keep their own lexical
record behavior. Unsupported builtin writes/updates/deletion and destructuring
targets have a read-only type diagnostic at the complete member range; the compiler
does not silently store into constants or claim general property-descriptor support.

Reads lower to existing Float64 literal IR and f64.const, with no host imports,
allocations or helper exports. All write paths share a member-base check before
ordinary reference preparation. ABI18/value12/schema12, grants/status, runtime
budgets, old artifacts and launcher negotiation remain unchanged. Source-free
module/native/public-API tests and dependency-file diagnostic ranges cover the path.

### `checked-f64-utf16-v16` — Native Math

`checked-f64-utf16-v16` selects **host ABI19 / value ABI12 / JSON schema12**.
It inherits v15's language/heap/exception/closure/sparse-mutation contracts and
keeps the existing JSON/value import selectors. Default i32 semantics and every
older profile remain unchanged. Matching language/ABI19 metadata is mandatory
even without a native Math import. Runtime/artifact validation rejects unknown,
downgraded, mismatched or duplicate declarations before guest effects. Launcher
protocol v2 now advertises maximum26/value12 and includes v16/v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27; packaging requires
the matching rebuilt trusted companion. Container/manifest formats and all resource
limits remain unchanged; no binary64 tool/HTTP adapter is introduced.

The common NativeMath inventory owns nineteen unary operations plus atan2:
acos/acosh/asin/asinh/atan/atanh/cbrt/cos/cosh/exp/expm1/log/log1p/log10/log2/
sin/sinh/tan/tanh and atan2. Each maps to a distinct typed `varta.number_<name>`
import, minimum ABI19. Unary signatures are `(f64)->f64`; atan2 is `(f64,f64)->f64`
with y then x. No capability or result globals/buffer are required. Shared registry
names, scalar signatures and minimum ABI drive compilation, runtime linking and
artifact checks. Compiler Math IR collection registers the exact operation; emission
uses its resolved index, never a literal function/import number.

Only direct unshadowed dot calls with exactly one/two narrowed Numbers are admitted.
Arguments evaluate once in order; exceptions skip later arguments and the native
call. Older profiles reject all twenty new methods. Generic/spread/computed calls,
borrowing, coercion and unnarrowed unions remain unsupported. Native math introduces
no new value type, allocation, guest helper export or dependency. Math.random is
separate v17/ABI20 entropy operation; its name is not in the deterministic registry.

Semantics follow the admitted numeric part of the
[ECMAScript Math algorithms](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-function-properties-of-the-math-object).
NaN, signed zero, infinities and domain boundaries use explicit required results;
NaN payload/sign are unspecified. Inverse trig domain errors yield NaN; acosh(1)
and log/log2/log10(1) give +0; atanh(±1) gives signed infinity; logs of both zeroes
give -Infinity and log1p(-1) gives -Infinity. Zero-preserving methods return the
original zero; cos/cosh/exp of zero give 1. atan2 handles signed axes, both infinite
arguments and quadrant constants before finite native atan2. Ordinary finite math
uses the platform f64 library and remains implementation-approximated; universal
correct rounding or cross-platform last-bit equality is not claimed.

Each call ticks one existing host allowance and checks the same budget/deadline
after math without another increment. Status is not reset and no host result buffer
is reused. Authorized exit or budget exhaustion stays fatal through catch/finally.
Purity is not permission to call from guest JSON allocation/retention helpers:
every reachable host call there is rejected, including nested native Math. Compiler
collection/effect walks retain every argument effect, earlier roots/pairs, nested
calls and saved returns through GC/finalizers.

Tests cover all operation names/signatures/ABI gates, exact special-value bits,
atan2 quadrants, domain boundaries, tiny-input accuracy, known finite values and
binary64 exponent-wide domain/range invariants with fresh bounded stores. Host-call
accounting, before/after deadlines, old-profile rejection/inheritance, hostile
metadata/signatures/import names, helper purity, GC/pairs, source ranges, virtual
modules and source-free CLI/runtime/native execution have regression coverage.

### `checked-f64-utf16-v17` — Math.random

`checked-f64-utf16-v17` selects **host ABI20 / value ABI12 / JSON schema12** and
inherits every v16 source/value/heap/exception/closure/sparse-mutation feature.
Default i32 and old profiles retain their contracts. Matching language and ABI20
metadata is mandatory even when the source uses no random call. Unknown/mismatched,
downgraded and missing declarations fail before execution. Launcher protocol v2
advertises maximum26/value12 and includes v17/v18/v19/v20/v21/v22/v23/v24/v25/v26/v27; both trusted native build companions must be
rebuilt together. Container/manifest formats and resource limits are unchanged.

The one new shared registry operation is `MathRandom`, import
`varta.number_random : () -> f64`, minimum ABI20, no capability or result globals.
Randomness is intrinsic runtime entropy, with no guest-directed file, environment
or network access. No Tool/HTTP binary64 adapter or permission is introduced.
The compiler resolves this operation's collected index, with no literal import
indices. Only direct unshadowed Math.random dot calls are admitted; spread,
generics, borrowing and computed calls remain unsupported. There are no required
arguments or conversions. All extra admitted arguments evaluate once in order,
then their complete physical values are discarded. Failure skips later effects
and the entropy call. Existing roots retain earlier operands/pairs/returns through
argument GC and finalizers.

The strategy follows the numeric range/distribution requirement in
[ECMAScript 2025 Math.random](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-math.random).
Each successful call reads one fresh 64-bit OS word through runtime-only pinned
[getrandom 0.3.4](https://docs.rs/getrandom/0.3.4/getrandom/), discards its low eleven
bits and exactly scales the remaining integer by 2^-53. Results are positive-sign
finite multiples of 2^-53 in [0,1); zero is +0, and the largest value is 1-2^-53.
Uniform input words give equal probability to each lattice point, without a
rounded division that could produce 1. The public Math API promises neither
cryptographic use nor deterministic replay.

There is no production PRNG state, cached entropy, seed setter or persisted seed.
Every Execution uses the OS provider; module/runtime reuse, HTTP execution and
native packaging cannot copy a guest seed or resume an entropy cache. Independent
executions consume fresh words from the OS, with no same-seed restart strategy.
The locked dependency supplies platform entropy implementations rather than a
handwritten syscall/ambient-file fallback. It does not broaden the tested platform
matrix. Test scripts/fault injection live only in each test store and are absent
from production builds; no environment variable or public engine option overrides
entropy.

Before reading entropy the host ticks one existing allowance and checks the
execution deadline; after reading it checks the same budget/deadline without a
second charge. OS failure is a fatal runtime error, never a fixed/time-based seed,
recoverable script exception or successful sample. Limits/authorized exit remain
fatal through catch/finally. No system status reset, result buffer reuse or pending
exception mutation occurs. As with all host operations, before/after checks do not
promise preemption inside an OS call. Host-free JSON allocation/retention helpers
reject every reachable random import.

Tests check exact endpoints and the 53-bit mapping, fresh OS sample ranges,
store-local scripted sequence isolation, exact one-call accounting and exhaustion,
pre/post entropy deadline checks, injected entropy failures with no fallback, ignored
argument kinds/effects/throws, pairs/GC/saved returns, profile/ABI/signature/helper
rejection, virtual modules and source-free portable/native paths. Statistical
thresholds and sequence-inequality assertions are not used as correctness gates.

### `checked-f64-utf16-v18` — Number output

`checked-f64-utf16-v18` selects **host ABI21 / value ABI12 / JSON schema12**,
inheriting v17's admitted language, Math/random, JSON, heap, closure, exception and
sparse-mutation features. The default remains i32. Matching language and ABI21
metadata is mandatory even without a log call. Unknown/mismatched/downgraded
metadata fails before execution. Launcher protocol v2 advertises maximum26/value12
and v18; native packaging requires the matching rebuilt companion. Container and
manifest formats, value layouts, helper exports and resource limits are unchanged.

The two new shared registry imports are `LogFloatV2`, `varta.log_f64_v2 : (f64)->()`,
and `ValueLogV7`, `varta.value_log_v7 : (i64)->()`, minimum ABI21 with no capability
or result globals. V18 remaps scalar LogFloat and every older union/reference log
to these exact names. Non-log operations and JSON/value selectors are unchanged.
Older profiles keep their registered log contracts; their raw Number text is not
silently changed by a newer runtime. V18 rejects the older log names, and earlier
profiles reject the new imports. Scalar logging needs no guest result buffer;
union logging reads a checked value ABI12 cell. No string result is allocated in
the guest and the compiler resolves ordinary collected host indices.

Canonical Number text follows the radix-10 portion of
[ECMAScript 2025 Number::toString](https://tc39.es/ecma262/2025/multipage/ecmascript-data-types-and-values.html#sec-numeric-types-number-tostring).
Both zeroes produce `0`; NaN and infinities use `NaN`, `Infinity`, `-Infinity`.
Finite nonzero values use the shortest decimal significand that rounds back to the
same binary64 value, with nearest/even tie selection. Fixed notation applies for
magnitudes [1e-6,1e21); otherwise scientific notation uses lowercase e, explicit
exponent sign and no exponent leading zeroes. Pinned ryu-js1.0.3 supplies this
formatting, already used in checked JSON serialization; no dependency is added.
A scalar Number's text needs at most 25 ASCII bytes, including a negative sign.
There is no locale influence or platform libm dependence in this conversion.

The new union log changes only its Float64 case. Other primitive, String, Error,
record and array observations retain existing validation and rendering, including
Boolean logs as 0/1. Diagnostic aggregate JSON retains its previous canonical
numbers and nonfinite/null rules. JSON.stringify is unchanged. This increment
does not itself introduce String() (added for primitives in v19), Number.prototype.toString, arbitrary-radix conversion,
coercion, Number wrappers or first-class method borrowing.

Each log ticks one existing host allowance and retains existing deadline/output
limits. Canonical formatting is bounded; no system status reset, result buffer
reuse or pending-exception mutation occurs. Argument effects run before logging;
throws skip the log. Budgets and authorized exit stay fatal through catch/finally.
Earlier arguments, result pairs, roots and saved returns remain valid through GC.
Host-free JSON allocation/retention helpers reject every reachable log import.

Tests cover exact special cases and threshold/shortest-digit goldens, all binary64
exponents and decimal-power neighbors, bit-preserving parsing, independent shorter
significand rejection and the 25-byte bound. Actual scalar/union Wasm calls traverse
all exponent classes in fresh bounded stores and charge exactly one call each.
Earlier profiles preserve scalar/union text. Non-Number values, JSON, buffers/status,
GC/pairs/finalizers, source ranges, hostile imports/signatures/metadata, helper
purity, virtual modules and source-free portable/native paths are covered.

### `checked-f64-utf16-v19` — Primitive String conversion

`checked-f64-utf16-v19` selects **host ABI22 / value ABI12 / JSON schema12** and
inherits every v18 feature. Default/older profiles retain their contracts. Matching
language and ABI22 declarations are mandatory even for no-argument String() or an
unused conversion surface. Unknown/mismatched/downgraded declarations fail before
execution. Launcher protocol v2 advertises maximum26/value12 and v19; native
packaging requires matching rebuilt companions. Container/manifest formats, value
layouts, helper exports, dependencies and all resource limits remain unchanged.

The new shared registry operation is `PrimitiveString`,
`varta.primitive_to_string : (i64)->i32`, minimum ABI22, no capability, returning
UTF-16 through the existing pointer/result-length globals and owned Snapshot path.
Result globals are required, but system status is not reset. The physical input
is a checked immutable primitive-union cell using the existing value12 layout;
PrimitiveBox validation rejects invalid sizes, heap/trace flags, tags, payloads,
string lengths and ranges before reading text. Reference/object cells are rejected.
The compiler collects/resolves this host index without literal import numbers.

The admitted behavior follows the primitive part of
[ECMAScript 2025 String(value)](https://tc39.es/ecma262/2025/multipage/text-processing.html#sec-string-constructor-string-value)
and [ToString](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-tostring).
A missing argument produces an empty String literal with no host call; present
Undefined produces "undefined", Null "null", Boolean "true"/"false". Numbers reuse
the canonical v18 formatter, including zero/nonfinite/scientific rules. String
values preserve all UTF-16 units, without lossy Unicode scalar conversion or NUL
termination. No Symbol/BigInt value type, wrapper constructor, object-to-primitive
protocol, Number.prototype.toString method or radix conversion is added here.

Only direct global String calls are added. Ordinary lexical/imported functions
named String retain normal dispatch/TDZ behavior. Generics, spread, borrowing and
new String are unsupported. The first argument must have only admitted primitive
members; a reference-containing union needs narrowing. Extra arguments may be any
otherwise admitted values, including physical result pairs, objects and functions.
All arguments evaluate once in order before conversion. A hidden local caches the
original first value; ignored argument effects/throws never change that snapshot.
The compiler uses one shared five-member primitive union plus existing hidden
locals/root accounting, with no raised type/frame/source/fuel limits.

A ValueBlock retains the first value and final String across ignored-argument GC,
outer operands, calls, result pairs and saved-return/finalizer lifetimes. Each
present conversion ticks one existing allowance before decoding and checks the
budget/deadline again after transfer. Number text is bounded; Strings are copied
at most the unchanged 1 MiB encoded ceiling. The host copies input before writing
the reusable result area, and guest Snapshot retains output before another host
call. Empty strings and overlapping input/result areas are safe. Status and pending
exception channels remain unchanged. Bad cells/transfers and exhausted limits stay
fatal through catch/finally. Host-free JSON helpers reject reachable conversion
imports despite their lack of a capability.

Tests cover all primitive/union results, absent versus Undefined/empty input,
canonical Number text, all 65536 UTF-16 units, invalid cells, the exact maximum
string, result-buffer reuse, saved first-value ordering, ignored pair/effect/throw
paths, lexical/TDZ and old-profile boundaries. GC/earlier roots/saved returns,
fatal budgets, metadata/signatures/helper purity, dependency ranges, virtual
modules and source-free portable/native paths retain regression coverage.

### `checked-f64-utf16-v20` — Primitive Number conversion

`checked-f64-utf16-v20` selects **host ABI23 / value ABI12 / JSON schema12**,
inheriting all v19 features. Matching language/ABI23 metadata is mandatory even
without a host conversion call. Invalid/mismatched/downgraded declarations fail
before effects. Default/earlier profiles retain their contracts. Launcher protocol
v2 advertises maximum26/value12 and v20; native packaging requires matching rebuilt
companions. Dependencies, helper exports, container/value/manifest layouts and all
resource limits remain unchanged.

The new shared operation is `PrimitiveNumber`,
`varta.primitive_to_number : (i64)->f64`, minimum ABI23, no capability, result buffer
or result globals. It reads the existing checked immutable primitive cell layout
with PrimitiveBox validation. The same cached-argument ValueBlock used for String
conversion prepares inputs, using one shared five-primitive union and existing
locals/root budgets. The first value is saved before all ignored arguments run;
conversion follows their complete, ordered evaluation. Throws skip later arguments
and conversion. Ordinary local/declaration/imported functions named Number retain
normal dispatch and TDZ behavior.

The admitted semantics follow the primitive part of
[ECMAScript 2025 Number(value)](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number-constructor-number-value)
and [StringToNumber](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-tonumber-applied-to-the-string-type).
No argument yields guest +0 without an import; present Undefined yields NaN, Null
+0, Boolean 0/1. Number inputs retain finite/nonfinite values and signed-zero bits;
NaN payloads are unspecified. No object-to-primitive protocol, wrapper, BigInt or
Symbol value support is added here. Reference-containing unions need narrowing.
new Number and generic/spread/borrowed calls remain unsupported. This conversion
supplies the numeric foundation for radix arguments in the v21 method; it does not
itself add that method or general implicit coercion.

UTF-16 strings trim only the pinned ECMAScript WhiteSpace/LineTerminator set,
including FEFF and excluding 0085/180E/200B. Empty trimmed strings yield +0; remaining
non-ASCII units, including lone surrogates, make numeric syntax invalid. The whole
remaining input must match decimal integer/fraction/exponent syntax, signed exact
Infinity, or unsigned 0x/0b/0o integer syntax. Internal whitespace, separators,
partial parses, signed nondecimal prefixes and wrong/trailing digits yield NaN.
Grammar is checked before Rust decimal parsing, preventing extra host spellings
such as inf from becoming valid. Decimal parsing uses locale-independent binary64
nearest/even conversion, retaining overflow and signed underflow.

Prefixed power-of-two integers scan every digit, retain the leading 53 bits plus
guard/sticky information, and round once to nearest/even. Exponent packing handles
carry and overflow without intermediate floating accumulation, u128 truncation or
an arbitrary-precision dependency. Even a value already certain to overflow must
check its remaining digits: invalid trailing text yields NaN, never Infinity.
Input stays under the existing 1 MiB encoded limit; the temporary ASCII buffer
holds at most half that many bytes. Work is linear in string length, with constant
numeric state; the decimal conversion uses Rust's bounded input parser.

The host ticks one existing allowance before validation/parsing and checks the
same deadline/budget afterward without another charge. Bad cells and exhausted
limits stay fatal through catch/finally; syntactically invalid numeric strings
instead return NaN normally. Status, result buffers/length and pending exceptions
are never reset or mutated. Guest roots retain strings, cells, earlier operands,
pairs and saved returns through argument GC/finalizers. Host-free JSON helpers
reject all reachable conversion imports. Removing result-global exports from an
otherwise valid Number-only fixture does not prevent execution.

Tests cover all primitive/union cases, absent/Undefined distinctions, grammar and
all UTF-16 whitespace units, decimal halfway/underflow/overflow goldens, exhaustive
small prefixes and independent u128 casts, CPython >128-bit integer goldens,
binary64 exponent/threshold round trips and numeric identity including -0. Exact
maximum strings, invalid suffixes after overflow, effects/status/buffer/pair/GC
lifetimes, old bindings/profiles, hostile metadata/signatures, helper purity,
virtual modules and source-free portable/native paths are covered.

### `checked-f64-utf16-v21` — Number radix text

`checked-f64-utf16-v21` selects **host ABI24 / value ABI12 / JSON schema12** and
inherits v20. Matching language/ABI24 metadata is mandatory even without a radix
call; unknown/mismatched/downgraded declarations fail before effects. Old/default
profiles retain their contracts. Launcher protocol v2 advertises maximum26/value12
and v21; both trusted native companions must be rebuilt. Container/manifest/value
layouts, helper exports and resource limits remain unchanged.

The new shared operation is `NumberToString`,
`varta.number_to_string : (f64,f64)->i32`, minimum ABI24, no capability. It returns
UTF-16 through existing result-length/status exports and the owned Snapshot path;
status is preserved. The physical radix is an already checked integer Number in
[2,36]; noncanonical raw host arguments fail fatally. Compiler collection resolves
its index normally. Existing primitive conversion and log/JSON selectors are unchanged.

The admitted semantics follow
[ECMAScript 2025 Number.prototype.toString](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.prototype.tostring).
Only direct dot calls on narrowed Number receivers are added. Cache receiver first,
then the first radix, evaluate/discard all ignored arguments, and only then coerce
and validate the cached radix. Missing/Undefined selects 10. Other admitted
primitive members use existing Number conversion (already numeric values need no
extra host call), then ToIntegerOrInfinity: truncate, with NaN mapped to zero.
Outside [2,36], throw a boxed, rooted RangeError through the existing exception
channel before checking receiver zero/NaN/infinity cases. Argument exceptions skip
later arguments and formatting; a receiver exception skips every argument.
Reference-containing radix unions need narrowing until ToPrimitive exists.

Decimal output reuses pinned ryu-js1.0.3. Other bases use lowercase digits and
fixed notation. Exact positive binary64 values and neighbor midpoints are integers
scaled by 2^1075; the largest finite value uses the virtual next value 2^1024.
Even significands include midpoint endpoints and odd ones exclude them. Normalize
against radix powers, then find the shortest precision with an integer inside the
rounding interval; choose the nearest integer, ties to even, inside that interval.
Strip insignificant trailing zeroes and place the point/leading/trailing zeroes.
No intermediate floating accumulation or approximate logarithm chooses digits.
Both zeroes print 0; valid-radix NaN/infinities use canonical names.

Native exact arithmetic adds direct runtime use of num-bigint0.4.8, already pinned
in the lockfile via the compiler; no new package version is introduced and BigInt
is not added to the language. IEEE-754 bounds limit normalization to 1074 steps,
precision search to 55 digits, scratch integers to below 2500 bits and text to at
most 1077 ASCII bytes (negative least-subnormal binary output). The native output
is UTF-16 encoded before transfer. Existing 1 MiB string/memory/fuel/frame/type and
host/deadline limits are unchanged. Before/after deadline checks do not promise
preemption during a bounded native call.

Each formatting import ticks one host allowance and rechecks budget/deadline after
transfer. Primitive String/Boolean/Null radix coercion can consume its existing
additional Number-conversion allowance. Numeric/default/Undefined radices do not.
Host failures and budgets remain fatal through catch/finally; language RangeError
is catchable. Result buffers are reused only after Snapshot owns output; system
status and pending channels are not reset. ValueBlock roots retain cached radices,
earlier operands/pairs and saved returns through GC/finalizers. Host-free JSON
allocation/retention helpers reject reachable formatter imports.

The checked surface does not add Number wrappers, general prototypes/descriptors,
borrowed/computed/spread/generic calls, String/Boolean.toString or other Number
methods. Existing Error.toString admission is unchanged. Tests cover every radix,
all exponent classes in representative even/odd bases, exact special/edge goldens,
1077-byte extremes and independent rational parsing back to binary64. Shorter
significand candidates fail to reconstruct the value. Actual Wasm calls test typed
linking, one-call accounting and owned text. Argument/coercion/RangeError order,
GC/pairs/status/returns, fatal budgets, old profiles, hostile metadata/signatures,
helper purity, dependency ranges, virtual modules and source-free portable/native
packaging have regression coverage.

### `checked-f64-utf16-v22` — Number precision

`checked-f64-utf16-v22` inherits v21 with mandatory **host ABI25 / value ABI12 /
schema12** metadata. Launcher protocol2 advertises maximum26/value12 and v22;
both native companions require rebuilding. Default/older profiles retain their
contracts; value, schema, container, permission and resource layouts are unchanged.

Semantics follow ECMAScript 2025
[toFixed](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.prototype.tofixed),
[toExponential](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.prototype.toexponential),
[toPrecision](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.prototype.toprecision)
and [valueOf](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.prototype.valueof).

Direct narrowed Number receivers admit toFixed, toExponential, toPrecision and
valueOf. Receiver and all arguments are cached/evaluated before conversion;
ignored extras retain effects, exceptions and complete result-pair disposal.
Precision admits primitive members only; reference-containing unions need narrowing.
Number conversion precedes truncation (NaN ->0). Missing/Undefined selects fixed
zero digits or shortest exponential/precision text. Fixed/exponential accept
0–100 fraction digits; precision accepts 1–100 significant digits. Fixed throws
boxed/rooted RangeError before receiver nonfinite handling; exponential/precision
convert first but return canonical NaN/infinity before checking the range.

`NumberToFixed`, `NumberToExponential`, `NumberToPrecision` register
`number_to_fixed`, `number_to_exponential`, `number_to_precision`, each
`(f64,f64)->i32`, minimum ABI25, capability None, owned UTF-16 result globals.
Physical precision is a canonical integer or infinity; raw NaN/fractions fail
fatally. Exponential/precision reserve -1 for shortest/default formatting; source
explicit -1 still throws on finite receivers. Fixed requires [0,100] even for
nonfinite receivers. The other imports accept canonical out-of-range precisions
only when the receiver is nonfinite. Compiler validation owns catchable errors.

Fixed uses pinned ryu-js1.0.3 (canonical notation at absolute value >=1e21).
Requested significant digits use exact BigUint ratios scaled by 2^1075, decimal
normalization and nearest rounding, with ties increasing the absolute significand.
Carry adjusts the exponent before notation selection. Precision uses exponential
notation for exponent <-6 or >=precision. Defaults reuse canonical shortest text;
exponential normalizes it to one leading digit and a signed exponent. Both zeroes
format unsigned; negative nonzero values keep their sign when rounded to zero.
IEEE-754 bounds limit normalization to 324 steps, scratch integers below 2500 bits
and text to 123 ASCII bytes (246 UTF-16 bytes), including sign/point/exponent.
No package or guest limit is added. Before/after native budget checks do not imply
preemption within a bounded formatting call.

Each formatter consumes one host allowance, checks deadline before/after transfer
and preserves system status/pending exceptions. Primitive precision conversion may
add its existing Number-conversion call. Snapshot owns results before buffer reuse;
ValueBlock roots retain cached arguments, pairs and saved returns across GC and
finalizers. Host failures/budgets remain fatal through catch/finally. Host-free
JSON helper validation rejects reachable formatter imports.

valueOf uses a guest ValueBlock returning the cached Number unchanged after all
ignored argument effects. It adds no import or host allowance; guest fuel/deadline
checks still apply. NaN remains NaN and -0 retains its sign. Wrappers, ordinary
prototype objects, borrowing, computed/generic/spread calls and locale formatting
are not admitted. Tests cover independent Python Decimal rounding goldens, all
exponent classes, output bounds, raw imports, metadata/helper rejection,
roots/status/budgets, diagnostics and source-free/public API paths.

### `checked-f64-utf16-v23` — Numeric prefix parsing

`checked-f64-utf16-v23` inherits v22 and requires matching **host ABI26 / value
ABI12 / schema12** metadata even without parser calls. Unknown/downgraded or
mismatched declarations fail before effects. Both native companions must be
rebuilt; protocol2 advertises v23 and maximum26/value12. Other profiles, default
i32 behavior, value/schema/container formats and resource limits are unchanged.

The direct admitted forms are global parseInt/parseFloat and unshadowed
Number.parseInt/parseFloat, following ECMAScript 2025
[parseInt](https://tc39.es/ecma262/2025/multipage/global-object.html#sec-parseint-string-radix),
[parseFloat](https://tc39.es/ecma262/2025/multipage/global-object.html#sec-parsefloat-string)
and their [Number aliases](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-number.parsefloat).
Global declaration/local/closure bindings and TDZ take ordinary precedence.
Cache input, optional radix and all ignored extras before conversion. Input and
integer radix admit the five primitives and their unions; references need narrowing
until ToPrimitive exists. Ignored arguments may have any admitted type, including
captured result pairs. Argument exceptions skip later operands and parsing.
Missing input produces guest-known NaN and adds no host call.

New shared operations `NumberParseInt` / `number_parse_int:(i64,i64)->f64` and
`NumberParseFloat` / `number_parse_float:(i64)->f64` require ABI26, no capability,
no result globals or buffer. Inputs are validated primitive cells. Input ToString
precedes radix ToNumber: exact UTF-16 String data is borrowed during the call;
Number uses canonical text (including -0 ->0), other primitives their literal
spellings. Int32 radix uses exact binary64 significand/exponent truncation and
modulo2^32, with NaN/infinities ->0. Missing radix uses a canonical Undefined cell.

Parsers skip the exact ECMAScript leading WhiteSpace/LineTerminator set only.
parseInt handles sign, then default radix10 / optional0x or explicit16 prefix;
other bases2–36 retain their ordinary digits. Consume the longest valid prefix;
invalid base/no digits returns NaN, zero retains the sign. NUL, lone surrogates,
whitespace and other nonmatching units terminate a prefix rather than invalidate
an earlier one. No implicit octal/binary prefix is recognized. Exact bounded
BigUint accumulation rounds nearest/even to binary64; above 1024 value bits an
already overflowing prefix determines infinity regardless of suffix. A final
multiplication needs at most 1030 bits, including overflow detection. No approximation
option is used for general radices or long decimal integers.

parseFloat recognizes signed Infinity prefixes or decimal digits/fraction with an
optional complete exponent. Missing exponent digits roll back to the mantissa.
Select grammar before Rust binary64 parsing; Rust-specific inf/NaN spellings,
separators and hexadecimal floating syntax are not admitted. ASCII scratch is at
most half the existing 1 MiB UTF-16 ceiling. Primitive ToString scratch is bounded
to 25 ASCII Number bytes or fixed words. Native work is bounded by input size and
IEEE integer width; before/after deadlines do not imply native-call preemption.

Each present-input operation ticks exactly one host allowance and rechecks budgets
before returning, preserving result buffers, system status and pending exceptions.
Parsing failure is ordinary NaN, never a recoverable host status. Host failures,
malformed cells, deadlines and limits are fatal through catch/finally. ValueBlock
roots retain cached inputs, radices, earlier pairs and saved returns through GC;
JSON allocation/retention helper validation rejects reachable parser imports.
Wrappers, general object coercion, first-class aliases and computed/spread/generic
calls are not added. Tests cover independent CPython integer rounding goldens,
all bases/exponents, Unicode units, maximum strings, malformed cells, metadata/
signature/helper checks, roots/status/budgets and source-free/public API paths.

#### Guest truth conversion and logical operators (v23)

V23 conditions, ternaries, loops and unary ! now lower ordinary values through
ToBoolean IR. Logical assignment tests and existing filter/find/some/every
predicate callbacks share that conversion path. References are true; exact
primitive and actual-union-tag rules are as specified below. No native truth
allowance or conversion allocation is needed. Older profiles retain their source
admission and versioned native truth imports; old compiled artifacts retain their
physical behavior. Host ABI26/value12/schema12 and launcher negotiation are unchanged.

Logical &&/||/?? use a rooted ValueBlock: evaluate/cache original left once, test
its actual value, then execute only the selected branch. &&/|| test ToBoolean;
?? uses the existing pure IsNullish tag operation, so zero/NaN/empty strings remain
nonnullish. Return the selected operand with its complete Number bits or reference
identity. No shared emitter scratch holds the cached value across RHS execution.
The existing [ECMAScript 2025 logical semantics](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-binary-logical-operators)
are the target, within the explicit checked value/type representation.

Selected-left types remove members that cannot reach that branch: || and ??
exclude Null/Undefined, while && excludes always-true reference members. Narrow
an actual union member only inside the tested branch, then join with RHS type.
Thus String|Undefined || String and String|Null|Undefined ?? String return String.
Known literals and statically uniform type outcomes choose a branch while preserving left evaluation,
without inventing a union; selected function values remain ordinary function values.
Otherwise use the existing checked union layout and selected-branch boxing. This
retains single-record-shape/single-array-element-type and no-function-union bounds.
Opaque VartaResult pairs require explicit fields. Both source operands still need
valid syntax/types even when one is statically unselected.

Root cached input and output through selected allocations, previous operands/pairs,
saved returns and exceptions/finalizers. Root floors restore exactly at ValueBlock
exit; neither numeric payloads nor stale operands become reference edges. Pure
tests preserve system status, pending exceptions and result buffers. RHS effects
retain normal grants and fatal host limits; skipped effects consume nothing.
All guest work remains fuel/deadline metered under unchanged limits. JSON helper
validation allows host-free control flow and still rejects reachable RHS host
calls, including calls in a runtime-conditional branch. Independent exponent/sign
checks use fresh bounded stores and no raised production limits; explicit GC stress
uses test fuel only. Source-free/public APIs, old profiles, diagnostics and helper
purity have regression coverage.

Opt-in v24 adds direct-local truth refinement and RHS guard propagation. This runtime integration does not add general object coercion,
new value kinds, arbitrary shapes, wrappers or the remaining array built-ins.
Opt-in v25 separately adds checked Array.findIndex/findLast/findLastIndex;
these methods remain unavailable in v24 and earlier profiles.

#### Boolean and global numeric predicates (v23)

The opt-in v23 profile additionally admits direct Boolean/isFinite/isNaN calls,
with unchanged host ABI26/value ABI12/schema12. Other profiles and existing
artifact interpretation remain unchanged. No new operation, capability, helper
export, dependency or launcher maximum is introduced; package matching remains26.

Semantics follow ECMAScript 2025
[Boolean](https://tc39.es/ecma262/2025/multipage/fundamental-objects.html#sec-boolean-constructor),
[ToBoolean](https://tc39.es/ecma262/2025/multipage/abstract-operations.html#sec-toboolean),
[isFinite](https://tc39.es/ecma262/2025/multipage/global-object.html#sec-isfinite-number)
and [isNaN](https://tc39.es/ecma262/2025/multipage/global-object.html#sec-isnan-number).
Global declarations, locals, closures and TDZ use ordinary precedence before
intrinsic dispatch. Only direct calls are admitted. Cache first input, evaluate
all ignored arguments (any admitted type, complete result pairs included), then
convert the cached original input. Argument throws skip later effects/conversion.

Boolean accepts all ordinary checked values and their admitted unions: false for
Undefined/Null, false, +/-0, NaN and zero-length String; true for other Number,
String, Boolean and every object/array/function/Error value. Opaque VartaResult
pairs require explicit fields and are not implicit ordinary objects. Missing input
is false; void-call arguments retain effects before their Undefined conversion.
No object ToPrimitive or user callbacks run during ToBoolean.

New typed IR ToBoolean returns canonical i32 0/1 using guest instructions only.
F64 abs(x)>0 handles numeric NaN/zero rules; String tests the packed UTF-16 byte
length. Direct references discard their handle then return1. Union paths inspect
actual immutable tags: Float64/String/Boolean read their proper payload; Null and
Undefined are false; admitted reference members are true. No numeric bits are
traced or dereferenced. The operation allocates nothing, imports nothing and does
not consume host allowance, even for strings/unions. V23 control-flow/logical/unary lowering shares this guest operation; older
profiles retain their contracts.

Global numeric predicates admit primitives and primitive-only unions. After all
argument effects, narrowed F64 values feed the existing guest NumberPredicate
operation directly. Other primitives widen to a checked primitive cell and use
the existing ABI23 PrimitiveNumber import, then apply the guest test. Missing input
is guest NaN: isFinite false / isNaN true, with no import. Null/Boolean/String and
Undefined thus have numeric conversion rather than Number.is* type-test semantics.
Objects/reference-containing unions require narrowing until ToPrimitive exists.

ValueBlock roots cache input handles/cells before ignored allocation, mutation and
calls. Earlier pairs and saved Boolean returns retain exact roots/cleanup through
exceptions/finalizers. Pure conversion preserves result buffers, status and pending
channels. Numeric coercion consumes one existing host allowance and before/after
deadline checks; budgets stay fatal through catch/finally. Guest-only paths retain
Wasm fuel and deadline enforcement. Host-free JSON helpers admit pure Boolean
conversion and reject reachable native numeric coercion. Tests cover all numeric
exponents/signs, union/reference truth tables, raw guest host ceilings, effects/
shadowing/TDZ/GC/status, helper purity and source-free/public API paths.

### `checked-f64-utf16-v24` — Truth guards

`checked-f64-utf16-v24` inherits v23 and **shares host ABI26 / value ABI12 /
schema12**. Source semantics and physical ABI are versioned separately: no new
host operation/import, value/schema/container layout, runtime dependency, capability or
resource limit is added. Both native companions must advertise v24 in protocol2's
supported profile list even though the host maximum remains26/value12. An older
same-ABI companion without v24 fails negotiation. Unknown profiles, missing
language metadata or ABI mismatches fail before effects. Relabelling compatible
Wasm as another supported ABI26 profile cannot prove source provenance; metadata
is a declaration, as before.

Only direct local identifiers gain truth guards. Positive branches remove Null/
Undefined; negative branches remove always-true Record/Array/Error members. Both
outcomes retain potentially true/false Number, String and Boolean classes. Negative
truth is therefore not the complement of a class-level positive filter. Existing
parentheses/! handling and typeof/Array/Error guards compose normally. On a true
&& path, combine both true guards; on a false || path, combine both false guards.
Do not assume either fact for the other compound outcome. Impossible filters stay
conservative rather than inventing a Never value kind.

Before lowering the selected RHS, temporarily apply the left's positive truth for
&&, negative truth for ||, or Null|Undefined membership for ??. Parenthesized locals
are supported. Restore the previous refinement map after RHS lowering, including
errors; runtime still caches the original left and evaluates only selected effects.
Selected-operand result typing and rooted ValueBlock behavior remain v23's contract.
The RHS can read narrowed record/array/String/Error members or pass a narrowed value
to a typed callee. Guards on properties, arbitrary call results or aliases are not
inferred. No truth-based inference through Boolean(...) calls is added.

If/ternary guards scope to their selected branch; normal joins combine their
resulting facts as described above. While and for guards scope to the body;
for updates retain true-test facts only where body/continue effects allow them.
Join false-test and matching break facts outside the loop. Do-while's first body is unguarded because its test has not run yet.
Write revisions now invalidate changed slots and possible mutable-capture effects,
as specified above; const capture proofs remain stable. If filtering changes no
members, add no refinement/capture mark. V23's previous source admission remains
unchanged. Initializer, abrupt-exit/call/property precision and richer dataflow
remain required work.

Refinement affects compile-time types and existing checked guest unboxing only;
ToBoolean/IsNullish remain host-free. Preserve full numeric bits, actual reference
children and roots across RHS calls/allocations, prior pairs, saved returns and
finalizers. Status, pending exceptions, buffer ownership, fatal host limits and
Wasm fuel/deadline enforcement are unchanged. Tests cover both truth outcomes,
compound/RHS/loop scopes, mutation/capture invalidation, redundant guards, all numeric
exponents, metadata negotiation, GC/status, dependency ranges and source-free/
public API execution. No conformance/platform/security completion claim follows.

#### Write-aware refinements and captures (v24)

V24 extends guard admission with compile-time write revisions. Existing profiles
through v23 retain readonly guard/capture rules; the same ABI26/value12/schema12,
heap/cell layouts, helper exports and native operation registry are used. No
permission, runtime dependency or production budget is added or raised.

Each binding slot has a compile-time revision. A committed simple, pattern,
compound, logical, update or existing-head for-of store increments it and removes
that slot's refinement. Prepare references and evaluate RHS before invalidating
the target, preserving reads such as value=read(value) and cached old operands.
Stores still validate/widen to the declared type. Preserve the observed value
type before widening; a strict subset of the declared union becomes the new fact
after a successful store. Simple/pattern/compound/update stores participate;
logical assignment targets receive the selected old/new result type. Prepared
reads use current facts while caching the old value before RHS effects. Initializer
inference is not added, and no physical cell/storage type changes.

Snapshots contain both facts and revisions. Alternative if/ternary branches start
with the same facts. Join types from normally completing paths using the existing
reachability/target analysis above; absent facts mean the full declared upper bound. Return,
throw and jump paths do not contribute to a normal join. Joined subunions remain
inside the same bounded type arena. Optional RHS/default execution retains saved
facts only for slots not written by any potentially executed path. Loop/label/
switch exits use the completion-targeted joins above.
Repeated writes must change revisions even when a slot was already written before
the snapshot. Catch/finalizer lowering retains conservative invalidation from any
potentially executed prefix. A handler starts from try-entry facts minus possible
body mutations, not the type at the end of the body. Normal try/catch paths join;
finalizers start from facts safe at every body/handler prefix. A finalizer's own
committed stores can establish new normal-exit facts. No stale outer fact is
restored after an inner write. Switch selection tests lower before case bodies;
case entry joins direct selection with normal fallthrough. Completion-targeted
switch/label exits now use the matching break snapshots as described above.

A bounded SWC AST visitor records potential assignment/update/pattern/for-head
writes and user-code calls before lowering repeated code. Binding patterns,
parameters, block/loop/catch scopes and named functions account for lexical
shadows. Nested function bodies are conservatively included. For code lowered in
v24, for bodies precede updates so update reads see body invalidations. Do-while
tests and for updates join normal fallthrough with their matching continues,
which may bypass a later store. Loop facts
are cleared for potential back-edge mutations before the first lowered read;
condition guards can re-establish them on each iteration. Analysis depth follows
the existing64 nesting budget; exhaustion drops facts conservatively rather than
traversing unbounded input or raising a runtime limit. Type syntax has no runtime
effects and is skipped.

Guard inference blocks slots potentially written during the test. Calls without a closed effect proof
block/invalidate mutable captured cells, including captures
materialized by inline callbacks. Known function summaries and current non-reentrant native operations preserve
unaffected cells; unknown member/function calls stay conservative. This classification
must be revisited when object coercion/hooks, callbacks, await/yield or host reentry expand.
Uncaptured locals cannot be mutated by a callee under the admitted lexical model.
Fresh guards after a call can narrow its new current value.

Captures retain the full declared cell type and binding identity. Mutable closures
do not inherit a creation-site refinement; their own body must check the current
value. Const bindings may inherit stable proven types through transitive captures,
using the existing checked unboxing and environment roots. No refined physical
cell representation is invented. Original operands/arguments evaluated before a
mutation and saved returns own their earlier values independently of the rewritten
binding; GC tests include mutation and collection during later arguments.

The compiler adds pinned
[swc_ecma_visit29.0.0](https://crates.io/crates/swc_ecma_visit/29.0.0), compatible
with the existing AST29/Common26 stack; its transitive packages were already
locked. The runtime companion's normal dependency graph remains free of compiler/
visitor dependencies. Loop fixed points/first-iteration splitting, more precise
exceptional edges, initializer inference, call effects and property/alias refinements
remain unfinished analysis work. All previous root,
status/pending/buffer, fatal fuel/deadline/host, ABI and source-free boundaries
remain enforced. Regression tests cover assignment families, repeated revisions,
branch joins, test effects, lexical shadowing, loop back edges, callback reentry,
const/mutable captures, exception/finalizer paths, GC and diagnostics/public APIs.

#### Completion-targeted local flow (v24)

V24 snapshots also track whether a path can reach the next statement. If/ternary
joins ignore unreachable paths; unreachable source still undergoes syntax/type
checking. Literal truth (including successful void expressions) can rule out an
edge without skipping operand evaluation. This does not infer arbitrary constants
or introduce a Never value/physical type. Literal analysis uses the existing64
depth bound; effect traversal borrows the AST instead of cloning unchecked trees.

A function-local stack distinguishes loops, switches and named labels. Break and
continue capture facts at their own target. Only directly wrapping iteration labels
bind named continues to a loop; nested loops cannot consume an outer continue.
Label exits join normal fallthrough with matching named breaks. Switch exits join
matching breaks, final fallthrough and a possible no-match edge when there is no
default. Case selection/fallthrough input remains conservative as specified below.
Dead writes/breaks do not add executable exit paths.

While/for exits combine their false-test guard with matching breaks. Literal-true
or omitted for tests have no false edge. Do-while test and for update input joins
normal body fallthrough with continues targeting that loop; break/return paths do
not contribute. For-of retains a possible empty/exhausted-input edge. Literal-false
tests need no repeated-body invalidation. Other repeated code still conservatively
invalidates potential mutations before lowering; no general loop fixed point or
first-iteration path splitting is claimed.

For initializers execute once, before repeated-part analysis. Analyze only the
body/update/test afterward: an initializer assignment must not preserve a type
across a later mutation and the next test. For-of evaluates/caches its source once,
then analyzes the repeated head/body. Existing fresh let cells, rebind positions,
checked presence errors, exact roots and cached source identity are preserved.

Finally transforms pending break/continue snapshots created inside its try/catch
region, leaving earlier sibling exits untouched. Discard facts for slots potentially
changed during the finalizer and overlay facts guaranteed on normal finalizer
completion. Apply nested finalizers in unwind order. A finalizer with no normal
completion cancels those pending exits; its own breaks/continues remain separate.
Normal try continuation requires both a normal body/handler path and normal
finalizer completion. Handler/finalizer entry still uses conservative prefix facts;
unproved calls invalidate mutable captured cells; proven effects above can target
only the cells actually reachable through the known callee.

This is compile-time analysis over existing IR/guest control routing. No native
operation, runtime dependency, ABI26/value12/schema12, capability or production
budget changes. Prior profiles retain their source rules. Regression coverage
includes safe/unsafe joins, repeated tests, named/switch exits, finalizer writes/
overrides/throws, captures, GC and source-free/public-module execution. More precise
call/property/initializer inference, loop fixed points, exceptional edges and
compiler scaling measurements remain open work.

#### Proven call effects for local cells (v24)

V24 tracks compiler-only function provenance for direct FunctionRefs and immutable
local function bindings, including const aliases and conditional target sets. Known
proofs transfer through immutable captures by binding identity. Mutable function
variables, opaque returned functions and unproved values keep the reentry fallback.
No instance/environment alias identity or whole-program recursive fixed point is
inferred.

Each lifted closure records writes to captured binding identities plus symbolic stable callback inputs and an unknown-
reentry flag. Known calls propagate their target summaries, including effects through
a closure that captures only another function. Writes to the callee activation's own
locals do not become writes to caller bindings. Unknown self/callee calls propagate the fallback flag even when the externally
affected cell is not directly captured by the intermediary function. Stable callback
inputs propagate symbolically as specified above; unproved inputs remain unknown. Only matching visible mutable caller slots
lose their type facts; unknown reentry still invalidates all mutable captured cells.

Module declarations have no lexical caller environment under the current admitted
module model (no mutable module globals, prototype hooks or retained host callbacks).
They can reach caller cells only through supplied function values. Primitive or
callback-free structured arguments cannot supply those cells. For function arguments,
known closure/module proofs bound reachable effects; opaque function arguments keep
the fallback. Function-valued aggregates remain unsupported, and future opaque
containers must be treated as potential reentry. This rule concerns binding cells:
record/array mutation is not a promise of unchanged properties or alias graphs.

Analyze invocation effects after lowering arguments; do not restore facts invalidated
by argument effects. The callee is evaluated/cached before arguments under the existing
IR rules. Immutable proof identity cannot be reassigned during later argument work.
Direct calls, const aliases, .call and stable spread targets share the analysis;
synthesized/unknown callback paths stay conservative. Borrowed AST loop/test analysis
uses known target identities where available and respects lexical shadows, rather
than invalidating every cell for every user call.

Checked non-reentrant Number/String/Error methods and array operations without
callbacks preserve binding facts; array methods may still mutate heap contents.
Existing global native operations retain their status/capability contracts. Revisit
these classifications whenever globals, prototypes, getters/coercion, callback APIs,
await/yield or host reentry expand. Graph reachability memoizes visited types and
uses the existing64 depth bound; exhausted function/type proofs fall back to unknown.

Summaries/provenance live only in the compiler. Physical captures, emitted operation
registry, ABI26/value12/schema12, native distribution, capabilities and production
budgets are unchanged. Tests cover transitive/selected targets, module callbacks,
argument writes, shadowing, recursion/fallbacks, loops/guards, GC, denial status and
source-free/public-module execution. Precise higher-order callback-argument binding, returned/mutable function
provenance, property/alias effects, initializer inference
and production/conformance criteria remain open work.

#### Stable callback-parameter effects (v24)

Lifted-closure summaries can retain symbolic callback-input origins: defining
function plus physical argument position. At a proven invocation, substitute the
actual callback's known cell effects. Forwarding through known functions and const
aliases preserves those origins, including nested closures that capture only another
function. Unused callback inputs do not become invoked inputs merely because they
were supplied. Module declarations retain the conservative supplied-callback bound
from the contract below; no order-dependent whole-module fixed point is introduced.

Register original ABI inputs before source parameter initialization, then map them
to the initialized source binding. This preserves positions with default/rest prologues
and explicit Function receivers. Default expressions may invoke an earlier initialized
callback. Ignored .call receivers do not shift callback positions inside the actual
callee signature; an explicit Function receiver is a real callback input at position0.

A symbolic input denotes the original function only if no source store replaces its
binding. Validate direct stores and captured stores from nested lifted functions,
including an escaping mutator that could be invoked through another callback. A
read-only capture is allowed; a possible replacement anywhere in the function keeps
the unknown-reentry fallback. Const copies propagate their input origin, but this
is deliberately not a general point-sensitive mutable-function provenance analysis.
The proof relies on closed lexical binding cells and the absence of dynamic eval/
external environment-cell mutation; revisit it when those facilities expand.

Separate committed source writes from flow invalidation revisions. A symbolic call
still invalidates mutable captured facts while checking its own function body, since
the callback is unknown there. That conservative invalidation is not invented as a
direct write in the exported summary. Summaries retain actual captured writes,
transitive writes and symbolic inputs; callers bind them without restoring facts
lost during earlier argument evaluation. Unknown self/returned/mutable targets and
unbound higher-order callback arguments continue to use the broad fallback.

Borrowed AST guards/loops bind known and inline callback effects with the same
receiver/argument positions. Literal-function traversal shares the existing64 proof
depth budget across nested analysis instead of resetting it. The enclosing effect visitor visits literal bodies once;
normal-call fallback expands a literal only when there is no enclosing traversal.
Exhaustion is unknown;
source complexity/expression limits remain unchanged. Literal callback effects
respect parameter/block/self-name shadows and can conservatively include uncalled
nested bodies. Synthesized array/spread callback paths and opaque argument values
may still lose precision. Function-valued aggregates are not added.

This changes compiler analysis only: ABI26/value12/schema12, physical parameter/
closure layout, call evaluation order, permissions/status, exact roots and production
budgets are unchanged. Tests cover reads/writes, forwarding/captures, replacement,
prologues/receivers, guards/loops, argument effects, depth, GC and source-free/public
module execution. Higher-order argument binding, mutable/returned function analysis,
property/alias effects, initializer inference and readiness criteria remain open.

### `checked-f64-utf16-v25` — Predicate array searches

`checked-f64-utf16-v25` inherits v24 and adds direct Array.findIndex,
Array.findLast and Array.findLastIndex for narrowed checked arrays. The source
feature has its own profile; physical host ABI26/value12/schema12 are shared.
V25 introduced 28 supported profiles; v27 expands the exact protocol2 list to 30. Rebuild both native
companions; a shared physical ABI does not replace profile negotiation or prove
source provenance. Default compilation still selects checked-i32-utf8-v1.

The algorithms follow ECMAScript 2025
[FindViaPredicate](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-findviapredicate):
findIndex traverses ascending indices, while findLast/findLastIndex traverse
in reverse. The range is captured once before predicate invocation; indexed
values are read live, including holes/deleted entries as Undefined. Each call
receives value/index/original array and uses guest ToBoolean on its result.
The first match returns its cached value or index; failure yields Undefined or -1.
Later appended indices are outside the captured range. Writes, deletion and
truncation inside that range remain visible on later reads.

The compiler reuses ordinary typed function calls and rooted iteration frames.
Find-family callbacks accept element|Undefined; omitted/mistyped predicates or
incompatible receivers are compile errors. Corrupted function metadata remains
a fatal validation failure, including on empty arrays. Explicit thisArg binds a
declared function receiver; arrows retain lexical receivers. Callback expression
lowering precedes thisArg lowering so captured-cell changes invalidate later
argument facts. This order fix also applies to existing callback methods.

New methods evaluate extra source arguments for effects. A rooted value block
caches the array, callback and supplied thisArg before those effects; the generated
wrapper snapshots length afterwards. Extra values, including captured result
pairs, are discarded without changing physical callback arity. Original matched
references stay rooted when callbacks delete slots or trigger collection.
No new IR operations, host imports, dependencies, permissions, helper exports,
container formats or production resource limits are required.

Regression coverage includes dense/sparse arrays, mutation, empty searches,
callback defaults/arity/receiver/truth, maximum independent bounded traversals,
exact Number bits, forced collection, denied operations, system-status retention,
fatal fuel exhaustion and host-effect rejection in managed helpers. Public engine,
module/source diagnostics and source-free CLI/companion/container paths exercise
the integration. Generic array-like/borrowed prototype methods, prototype lookup,
dynamic callable values and unsupported Function unions remain separate work.

### `checked-f64-utf16-v26` — Reverse array folds

`checked-f64-utf16-v26` inherits v25 and adds direct Array.reduceRight on
narrowed checked arrays. Source/profile versions advance independently of the
unchanged host ABI26/value12/schema12. Both native companions advertise the exact
30-profile list under protocol2. Default compilation remains checked-i32-utf8-v1;
shared physical ABI compatibility does not establish source provenance.

The fold follows ECMAScript 2025
[Array.prototype.reduceRight](https://tc39.es/ecma262/2025/multipage/indexed-collections.html#sec-array.prototype.reduceright):
it captures the range before callback invocation, traverses descending indices,
skips absent positions and reads live present values. A supplied initial value
starts the accumulator; otherwise the rightmost present element supplies it and
does not trigger a callback. Empty/all-hole unseeded inputs throw TypeError.
Callbacks receive accumulator/value/index/original array with Undefined this.
Deletion, truncation and replacement affect subsequent visits; appended indices
beyond the captured length are ignored.

Shared typed reduction lowering now distinguishes fold kind from direction.
The wrapper retains the existing presence checks, accumulator-ready state,
per-item roots, validated calls and scoped exception/finalizer cleanup. Reverse
iteration reuses the guest loop machinery already used by reverse searches;
no new IR, imports, helper exports, dependencies or physical layouts are needed.
Seed inference reuses callback parameter/result annotations and contextual
aggregate literals. The accumulator and callback result have the same admitted
type; function/shape/union limits remain explicit. An explicit Undefined seed is
a supplied value, distinct from absence. Arrows retain lexical receivers;
ordinary functions must accept Undefined as their receiver. Missing/mistyped
callbacks and mismatched seed/result types are source diagnostics.

Argument expressions execute source/callback/seed/ignored-tail order. A rooted
value block caches effective operands before ignored expressions; the generated
wrapper snapshots length afterwards. Extra captured-result pairs are discarded
without expanding physical callback parameters. Callback metadata is validated
even on empty arrays; corrupt handles remain fatal and cannot become recoverable
empty-fold errors. Callback side effects remain visible to managed-helper
validation, grants and host policy. Pure folding preserves system status.

Tests cover sparse/dense and explicit Undefined values, noncommutative order,
mutation/regrowth, contextual aggregate seeds, graph identity, exact Number bits,
receiver/default/arity rules, thrown/fatal paths, ignored pairs and collection.
Independent maximum-size folds use bounded parsed inputs under default fuel;
repeated push-based preparation and multiple operations share the whole-program
fuel budget. Forced-collection stress uses explicit test fuel only. Public API,
module diagnostics and source-free CLI/companion/native artifacts cover deployment
paths. Production limits stay unchanged. Generic array-like objects, inherited
indices/prototypes, borrowed methods and dynamic accumulator coercion remain
separate CORE-02/03 work. Existing reduce and earlier profiles keep their admission.

### `checked-f64-utf16-v27` — Array literal inference

`checked-f64-utf16-v27` inherits v26 and adds inference for array initializers
without an explicit array context. Host ABI26/value12/schema12, helper exports,
container formats, capabilities and default source/type/frame/root/memory/fuel/
function-body limits remain unchanged. Both rebuilt native companions advertise
the exact 30-profile protocol2 list; the default remains checked-i32-utf8-v1.
Shared physical compatibility is separate from source admission/provenance.

The compiler lowers effectful elements once in source order, collects their
admitted member types, flattens existing unions and interns one final element
union. Existing widening boxes each scalar/reference as needed and reuses already
boxed union values. One structural record shape and one nested array element type
remain the reference-union bounds. Function and opaque result elements are still
rejected. Ordinary void-call values become Undefined after their effects.
Homogeneous initializers keep their direct element type.

Earlier compatible aggregate contexts remain available for literals, calls,
conditionals and destructuring results. Context-only record/array layout hints
remain separate from actual type constraints; branches can contribute new scalar
members without manufacturing extra union descriptors or replaying source. Literal empty/all-hole array peers contain no binding reads or expression
effects, so their type can be completed from a later typed array peer without
revisiting other source expressions. Their final IR position remains unchanged.
Pure outer empties and empty peer groups without array evidence still require an
annotation. General backward inference through arbitrary expressions, record
shape/field joining and recursive/dynamic empty types remain separate work.

The runtime behavior follows the admitted portion of ECMAScript 2025
[ArrayAccumulation](https://tc39.es/ecma262/2025/multipage/ecmascript-language-expressions.html#sec-runtime-semantics-arrayaccumulation):
initializer values are observed in order and elisions retain absent positions.
Type evidence from holes does not broaden writes to accept Undefined. Existing
missing reads supply Undefined independently. Explicit array annotations take
precedence, and mutable array aliases retain invariant element descriptors;
no existing array is copied or reinterpreted as a different element kind.

The unchanged array/union emitter roots earlier boxes and child graphs while
later expressions allocate, mutate bindings or throw. Tests cover primitive and
reference mixtures, existing union inputs, peer order, contextual records, Number
bits, callbacks/destructuring/spread calls, aliased writes, saved returns, GC,
host denials/status, fatal paths and managed-helper effect validation. Sparse
maximum-length and bounded dense literals pass; wide boxed literals retain the
64 KiB body limit, as in earlier explicitly typed profiles. Independent stress
fuel is confined to the forced-collection test. Module/public/source-free CLI and
native-companion paths retain the same deployment contract.

The checked compatibility inventory advances its pinned profile to v27 after an
observed unexpected-support result for the unchanged `[1, undefined]` source.
Review promotes that case and adds four positive primitive/peer/alias/context cases:
40 total, 28 passes and 12 known unsupported, with 7 feature rows still untested.
Earlier profiles retain their original literal admission rules. This closes the
specific inference gap and does not close CORE-01/02/15 or establish general
ECMAScript, dynamic object or Test262 support.

### `checked-f64-utf16-v28` — BigInt scalars

V28 admits opaque scalar BigInt with exact binary leaves. It retains value/schema12
and does not add a structured value tag. Parsed literals reuse SWC's unsigned
base-2^32 digits and are copied into managed flag2048 leaves. Parameters, returns,
locals and closure cells use rooted i64 handles, never f64 or untraced numeric bits.
Ten shared pure host operations cover binary/unary arithmetic, Number/string
construction, text/Number output, exact BigInt/Number/string comparisons and typed
signed/unsigned widths. Unknown selectors or noncanonical managed inputs are fatal.

Checked operations return (result pointer, native error class). The compiler
stores both, snapshots the shared result buffer only on success, labels BigInt
blocks as leaves and clears the allocation reserved word. Failure produces a zero
placeholder and a rooted TypeError/RangeError/SyntaxError throw before later source
effects. Privileged result_status is preserved. Every host call ticks the ordinary
call/deadline limits; integer work uses remaining guest fuel and the existing
payload capacity, then returns remaining fuel to the store. Formatting/UTF-16
output work is charged as well. Capacity/work/deadline failures bypass catch/finally.

Operands and ignored trailing arguments are evaluated once before conversion;
comparison never rounds a BigInt to Number. Invalid string comparisons produce
unordered results instead of throwing constructor SyntaxError. Methods currently
require Number/Undefined radix or explicit Number width and BigInt value. BigInt
aggregates, unions, thrown values, JSON semantics, compound/update syntax and full
coercive built-ins remain pending; a conformance array gap records that boundary.
The default and all historical profiles retain their prior admission and ABI.

#### BigInt mathematical and binary foundation

The internal `varta-bigint` crate supplies immutable mathematical integers with
explicit caller-provided result-bit and work budgets. It implements string/UTF-16
parsing, arithmetic, signed bitwise/shift operations, Number conversion and exact
comparison, radix formatting and signed/unsigned width conversion. Resource errors
remain distinct from recoverable semantic errors. V28 consumes this crate for
opaque scalar values; no structured BigInt tag is admitted yet.
The binary transport is defined in `varta-common::bigint`: an eight-byte
sign/count header followed by canonical little-endian base-2^32 digits. Sign 0
requires no digits; signs 1/2 require a nonzero highest digit. Payload length is
exact and bounded by the existing 1 MiB ceiling. Managed blocks reserve leaf flag
2048, retain the existing 16-byte allocation header and have a zero child mask and
reserved word. `BigIntView` validates payload/block ranges, allocation flags,
alignment and canonicality before borrowing digits. `varta-bigint` imports and
exports this format with explicit capacity/work checks and no decimal bridge.
Large left shifts also charge destination work before allocating it. Only declared
semantic failures map to native error classes: malformed binary transport and
resource failures cannot enter the recoverable channel.

An internal runtime fixture exercises this leaf with the actual generated
allocator/retainer/collector: roots preserve it, digit bits do not retain another
block, and dropping the root permits reclamation. This fixture does not admit
BigInt in a source profile or change old value/host versions. V28 source emission,
version negotiation and runtime arithmetic now consume that representation as
specified below. Aggregate/exception/JSON integration remains future work.

### `checked-f64-utf16-v29` — BigInt graphs

Tag11 denotes the canonical managed BigInt leaf from v28. It is a reference in
record slots, vector backings and immutable reference cells, including zero; the
leaf itself has no children. Old value versions reject tag11, old primitive cells
remain primitive-only, and v28 source still rejects structured BigInt. V29 adds
versioned value observation/conversion hosts and uses content equality for canonical
BigInt payloads in array searches without importing a host into managed helpers.

Primitive arithmetic involving BigInt unions first caches both source operands.
A bounded conditional dispatch unboxes actual primitive kinds, preserves string
addition and Number results, and throws native TypeError for mixed numeric kinds.
Generated branches retain the existing 64 KiB body, depth and root budgets. No
effectful source expression is evaluated again to infer or dispatch a type.

Stringify v13 returns (pointer, error class); the compiler snapshots only successful
results, including boxed String|Undefined roots. The encoder validates the bounded
graph before reporting BigInt TypeError. V13 record headers, trace masks and
metadata use the shared strict aggregate validator, including direct JSON roots.
Valid BigInt marks a deferred semantic failure, while corrupt later children, cycles and budgets remain fatal. Diagnostic
aggregate rendering instead emits metered decimal BigInt with n suffix. Parse
v15/schema13 admits a BigInt node for type checking, but no JSON token decodes to
it. Number|BigInt selects Number; absent optional fields and empty typed arrays
still produce canonical metadata. Decoder helpers stay host-free.

Exception channel4 selects reference-reader v13 and rejects old channel markers.
It retains BigInt through pending exceptions, catches and finally, then copies the
full canonical binary payload for uncaught errors before dropping guest memory.
Bounded hexadecimal diagnostics avoid unbounded decimal conversion in error paths.
The existing runtime ceilings and historical profile interpretations persist.

#### V29 primitive BigInt width/radix conversion

Pure ABI28 bigint_width_v2 has (i32,i64,i64)->(i32,i32) for signed flag and
canonical v13 width/value cells. bigint_to_string_v2 has
(i64,i64)->(i32,i32) for the BigInt receiver and primitive radix cell. Both
reuse checked pointer/error-class results, successful-only snapshots and rooted
native exceptions; resource/malformed failures remain fatal and pure result_status
is untouched. Validate all transported operands before returning semantic errors.
The compiler caches receiver/arguments/extras before conversion and roots every
boxed argument before allocating another. ToIndex width precedes ToBigInt value;
Number values are TypeError while Boolean/String are converted. Radix defaults
only on Undefined, then uses implicit ToNumber (BigInt is TypeError) and the
existing bounded radix formatting. Parsing, binary import and UTF16 encoding
consume guest fuel with deadlines checked. V28 typed imports and source admission
remain frozen; this primitive path does not implement object/prototype/Symbol
coercion. Primitive update ToNumeric is specified below.

#### V29 primitive update ToNumeric and checked stores

After caching a prepared reference/read once, updates retain BigInt and convert
other admitted primitive members through the existing metered PrimitiveNumberV2.
Postfix returns the converted old numeric value, including signed zero/NaN.
The increment/decrement unit matches Number or BigInt. Before writing, narrow
computed numeric unions to members accepted by declared storage; incompatible
actual members throw rooted TypeError. Only after that successful store can a
postfix old union be unboxed to its proven numeric kind. Dynamic record writes
still check each selected field and retain key/order/presence rules.
Number-capable missing fields/array entries can store NaN and publish presence;
BigInt-only storage rejects the Number result without mutation. String-only
storage rejects a statically incompatible Number update. Arrays remain invariant.
Old/new numeric results and source values stay rooted across boxing, native
conversion and checked stores; all existing fuel/body/root/heap limits remain.
V28 and older update contracts stay noncoercive. Object ToPrimitive and general
inferred storage widening are separate unfinished language/value work.

#### V29 primitive compound assignment coercion

Cache the prepared read and RHS once before conversions. Addition tests actual
String tags, using PrimitiveStringV2 for concatenation. Numeric paths normalize
primitive operands once with the update ToNumeric helper and then dispatch at
most two-by-two Number/BigInt combinations, avoiding a six-by-six conversion
expansion. The shared union helper meters String/Number conversions through v2
imports and excludes mixed-numeric throwing pairs from result type joins. If
all pairs throw, an unreachable result retains left-compatible physical types;
no fake BigInt result can turn a Number-target TypeError into static rejection.

checked_assignment_result is shared by updates and compounds. It caches the
computed value, accepts only members compatible with declared storage and throws
before an incompatible store. Dynamic records first narrow to successful field
result types, then preserve selected-field guards. Earlier RHS mutations remain
on failure; target presence/order changes only after a successful store.
Root old values, converted strings and outputs across calls/boxing/GC; all ABI,
value/schema versions and body/root/fuel/heap ceilings remain unchanged. V28 and
older contracts remain frozen. Object ToPrimitive, inferred storage widening
and full primitive comparison/equality remain separate work.

#### V29 ordinary arithmetic and unary conversion

primitive_arithmetic serves ordinary and compound arithmetic/bitwise/shift
expressions. Cache source operands before conversion, retain direct Number
operations and existing String values, branch on actual String tags for addition
and normalize remaining primitive unions before numeric-kind dispatch. Ordinary
reads never perform the compound store step. All pure imports, ABI28/value13/
schema13/channel4 selectors and resource ceilings remain unchanged.

Primitive unary +, - and ~ first normalize numeric kinds. Minus/not retain BigInt.
Unary plus has Number success type; its BigInt path evaluates the operand and
uses checked BigIntUnary(Plus) before an unreachable Number result. Canonical
leaf/work validation therefore precedes native TypeError, and malformed/resources
remain fatal without catch/finally. Root cached union inputs and outputs across
conversion/native snapshots, preserving signed zero and once-only effects.
Historical profiles remain frozen. Object/Symbol ToPrimitive and complete
comparison/equality remain unfinished independently of these arithmetic paths.

#### V29 primitive comparison and equality

Pure ABI28 primitive_equal has (i32,i64,i64)->i32 for strict flag and canonical
value13 cells; primitive_compare has (i64,i64)->f64 returning -1/0/1 or NaN.
Both cells are validated before semantic short circuits, and all native scanning,
parsing and integer work consumes guest fuel. Malformed/resources remain fatal
without catch/finally. Abstract equality handles nullish pairs, Boolean numeric
conversion, String/Number and exact BigInt/String/Number cases. BigInt/String
ordering uses integer grammar; invalid strings yield unordered, while two Strings
compare UTF16 units, including lone surrogates. Relational operators compare the
order result directly against zero, preserving NaN false in <=/>=.

Cache left/right source effects before boxing and root both cells across calls.
Scalar Number/String comparisons, same-kind Boolean equality, known-nullish
checks and existing BigInt paths retain fast execution, avoiding new native call
ceilings for ordinary loops. Older profiles and object/reference handling retain
their prior contracts. Full object/Symbol ToPrimitive remains unfinished.

### `checked-f64-utf16-v30` — Object operations

V30 reuses the v29 host, value, schema and exception selectors (see the
[version matrix](#version-matrix)) and adds the checked object operations below.

#### Closed property deletion profile v30

V30 reuses v29 host/value/schema/exception selectors. RecordDelete evaluates the
cached object and canonical absent value before writes, overwrites the field,
compacts the live order trailer with bounded scratch locals, zeros its unused
entry and clears presence. The descriptor's static reference mask stays intact:
absent union fields still own canonical undefined cells, while escaped old cells
are immutable and remain independently rooted. Repeated absent deletes skip cell
allocation. Required checked fields throw TypeError; this preserves closed-layout
invariants without claiming configurable/dynamic object support. Array primitive
keys become strings, length is guarded, exact ArrayStringHasOwn selects a present
canonical index and only then numeric deletion runs. Receivers/key effects occur
once, resource ceilings and old profiles stay unchanged, and protocol2 advertises
33 profiles. Full descriptor/prototype/Symbol semantics remain separate work.

#### Object.entries profile v30

Lowering caches the receiver and evaluates ignored arguments before snapshots.
It shares record/array key and value enumeration, with no user callbacks between
snapshots. Each dense pair contains the widened key and value; a singleton outer
array is passed to ArrayPush. The pair element type is an admitted String/value
union, and the result is an invariant array of pair arrays. Roots retain source,
key/value snapshots and the output across allocations and collection. Pairs are
fresh; child identities remain shared. Own-key order and absence follow the same
rules as keys/values. Nullish inputs throw after argument evaluation; Number,
Boolean and BigInt wrappers yield empty arrays. String enumeration and primitive-union dispatch use the guest path described
below. Mixed aggregate shapes, Symbol and generic object coercion remain
unfinished CORE-02/07 work. V29 source
admission stays frozen, with unchanged ABI/layouts and production budgets.

#### String own-property enumeration (v30)

For String and nullable String, lowering roots the original value, checks nullish
absence, snapshots UTF16 length and raises RangeError above 1024 units. It builds
a rooted dense String array using one-unit guest slices for values, or empty
placeholders followed by the existing ArrayKeys helper for decimal key names.
No per-unit host conversion occurs. Object.entries shares this key/value path.
Primitive unions test the String member before unboxing; other non-nullish
primitive kinds produce empty arrays. Nullish dispatch throws after ignored
argument effects. V30 keys/values adopt missing/ignored argument semantics;
older profiles retain their exact call admission. Mixed aggregate types,
Symbol and general object conversion remain pending. No ABI or budget changed.

#### Object.is and SameValue (v30)

Lowering roots both arguments before evaluating ignored extras, normalizes void
calls/missing arguments to Undefined, and dispatches union members without
coercion or joining incompatible aggregate shapes. NumberSameValue uses cached
numeric expressions only: both-NaN OR equal raw binary64 bits. This treats NaN
payload/sign differences as equal while distinguishing signed zeros. String
comparison uses the existing guest UTF16 helper; BigInt comparison reuses the
metered exact path. Managed references compare packed identity handles across
admitted reference kinds. Native result pairs require field selection rather
than pretending their physical status/value pair is a JavaScript object. No host
ABI/layout/grant or production budget changes. V29 admission remains unchanged.

#### Checked Object.assign (v30)

Cache arguments in lexical roots before validating the target; dispatch supported
record/array target unions, then process sources sequentially. Nullish sources
skip, as do non-String primitive wrappers and checked functions/Errors without
enumerable own fields. Records snapshot ordered keys and select individual live
field reads through the existing UTF16 lookup tree. This avoids imposing one
all-values union on unrelated nested shapes. Each source snapshot occurs after
prior writes, preserving source/target aliases. Arrays copy live present indices
under a captured length; String values use guest UTF16 enumeration. No user
getters/setters/proxies are admitted along these paths.

Stores reuse checked record guards, extending array writes and Number length
validation. Earlier writes persist after semantic exceptions; resource ceilings
retain fatal behavior. Return the target identity through rooted union widening.
For a plain fresh record literal with statically known record sources, enrich
its descriptor before its original allocation: added fields are optional with
canonical Undefined cells; existing literal fields widen only to an admitted
union. Rewrite initializer slot mapping in original evaluation order and update
only the hidden target local type. Static reference masks are exact from allocation;
no escaped record migrates or changes layout. Unknown indexed record keys retain
existing checked-write guards. Primitive target boxing, descriptors, Symbol and
general property creation remain future CORE-02 work. ABI/layout versions and
all production limits remain unchanged.

### `checked-f64-utf16-v31` — Named types

V31 reuses the v30 host, value, schema and exception selectors (see the
[version matrix](#version-matrix)), inherits every v30 operation and adds named
type annotations. Earlier profiles keep ignoring interface/alias bodies and keep
rejecting named references, type-only imports and `.js` specifiers.

- **Declarations.** Top-level `interface` and `type` declarations, exported or
  not, form a per-module type namespace separate from function bindings. An
  interface body follows the existing closed type-literal rules (property
  signatures, `?` optional fields under the presence rules, no readonly/computed/
  method members, at most `value::MAX_RECORD_FIELDS` fields). An alias may name any
  admitted type expression, including unions, arrays, function types, Error
  families and other named types.
- **Resolution.** A bare identifier reference resolves in the referencing module,
  following TypeScript's module scoping: a local or imported type named like an
  Error family shadows that built-in *type* in annotations, while the `Error`
  values and `instanceof` keep their built-in meaning. Otherwise built-ins
  (`Array`, `VartaResult`, HTTP types, Error families) are matched first. Import
  bindings are followed to the defining declaration, which is
  lowered once in its defining module and memoized. Declaration order does not
  matter. Named types are structural aliases: an interface and an identical
  type literal intern to the same record shape; there is no width or optional-
  field subtyping between distinct shapes.
- **Rejected with diagnostics.** Recursive declarations (direct or through other
  names), type parameters on declarations or references, `interface … extends`,
  duplicate type names in one module, unknown names (`unsupported type reference Name`) and type declarations inside function
  bodies. Generic built-ins such as `Record<K, V>` are rejected as generic type
  references.
- **Modules.** `import type { T }`, `import { type T }`, `import { T }` of an
  exported type, `export interface`/`export type` declarations and
  `export type { T as U }` / `export { T }` lists of local or imported types are
  admitted. A type-only import must name an exported type; a type-only export
  must name a type binding. Type-only bindings never become callable values, and
  a name may be both an exported function and an exported type. Re-exports with
  `from`, `export *`, default/namespace imports and side-effect imports remain
  rejected.
- **Project specifiers.** With v31 the project loader also accepts relative
  `./x.js` specifiers and resolves them to the `./x.ts` source (TypeScript's ESM
  convention); a `.js` file is never read. All other loader rules are unchanged.

Named types are compile-time only: they add no host import, value tag, layout,
helper or catalog entry, and artifacts carry no type names. All budgets are
unchanged.

### `checked-f64-utf16-v32` — Pilot batch (in development)

V32 reuses the v31 host, value and schema versions and the v31 JSON selectors
(see the [version matrix](#version-matrix)); [Error subclasses](#error-subclasses-error_subclasses)
select exception channel v5. V32 inherits every v31 feature. It collects the
[uarp-sdk pilot](development-plan.md#pilot-acceptance-package-uarp-sdk) M1–M3
features, each gated by its own `LanguageProfile` selector: module constants,
literal types, classes, module re-exports, template literals, default exports,
interface inheritance, generics and Error subclasses. The features compose:
constants, classes, generic functions and Error classes link through re-exports
and default exports; literal types apply to class fields, constants and
template substitutions; class bodies read module constants; generic interfaces
extend generic bases (`interface B<T> extends A<T>`); generic functions accept
and return Error-class instances. Apart from the exception channel selector,
none of them adds a host import, value tag, layout field or host/value ABI
version, and production budgets are unchanged. The compiler-internal
[suspension foundation](#suspension-foundation-k44-block-1) is not admitted by
v32; its frames carry the v32 catch union, including Error-class instances.

#### Module constants (`module_constants()`)

Top-level `const` and `export const` declarations become module-scope immutable
value bindings. V31 and earlier profiles keep rejecting them (`unsupported
top-level declaration`) and keep rejecting `as const`.

- **Declarations.** Each declarator binds an identifier, has an initializer and
  may carry a type annotation; one declaration may hold several declarators.
  Top-level `let`/`var` (`unsupported top-level let or var declaration`),
  destructuring and declarations without an initializer are rejected. Constants
  share one value namespace with the module's functions and value imports: a
  repeated name fails (`duplicate module binding`, `duplicate binding` or
  `duplicate or reserved import binding`). Type names stay a separate namespace.
- **Scope and modules.** A constant is readable from every function of its
  module, including nested arrows (which capture it like any immutable binding)
  and parameter defaults; parameters and locals shadow it. Assignment to a
  constant is rejected like assignment to any `const` binding. `export const`,
  export lists (`export { X as Y }`, also forwarding an imported constant) and
  named imports (`import { X as Y } from './m.ts'`) link constants across
  modules next to functions and v31 types. Constants and functions form one
  value namespace, so [re-exports](#module-re-exports-module_reexports)
  (`export { X } from`, `export { X as Y } from`, `export *`) forward constants
  under the same rules; a re-exported constant keeps its stored identity and
  `as const` readonly marking.
- **Initializer subset.** Initializers are limited to a pure subset: string,
  number, boolean, null and BigInt literals; unary `-`/`+` on numeric or BigInt
  literals; `undefined`, `NaN` and `Infinity`; references to constants declared
  earlier in the module or imported; array literals without holes or spread;
  object literals with identifier/string keys and shorthand properties; binary
  `+`; and one outermost `as const`. Other forms fail with `unsupported <construct>
  in a module constant initializer` (calls and `new`, member access, template
  literals, functions and classes, assignment, other operators, spread, nested
  `as const`, references to functions). Because `new` is a call, no constant can
  hold a class instance; a class-typed constant may only hold `null` or
  `undefined` through a union annotation. Constructors, methods, getters and
  field initializers read module constants like function bodies. A reference to a later constant or to the
  constant itself fails with `binding used before initialization`. The subset has
  no calls, so initialization cannot run user code, read capabilities or observe
  another constant before it is initialized; its only effects are allocation and
  execution budget. Values are checked by the ordinary rules, with the annotation
  as the expected type; unannotated arrays and records use the v27 inference.
- **Folded and stored values.** A `number`, `boolean`, `null` or `undefined`
  constant must be a literal, a negated literal or a reference to another such
  constant; it is folded and inlined at every use and needs no storage. A
  non-literal scalar initializer such as `1 + 2` is rejected. Every other constant
  (strings, BigInts, arrays, records, union values) is *stored*: it is evaluated
  exactly once per instance and held in a permanent slot of the shadow-root
  region, placed after the exception root and the canonical function roots. The
  root cursor starts after these slots, so they reduce the available root stack,
  and a program whose reservations exceed the region is rejected. The collector
  traces these slots for the instance lifetime and never moves their values, so
  every read yields the same identity: mutating a stored array or record (unless
  readonly) is visible through every reference, as in JavaScript.
- **Initialization order.** Stored constants are initialized eagerly by
  synthesized initializer functions that `_start` calls before the first
  statement of `main`: modules in dependency order (dependencies first, the order
  of the module metadata), and declarations in source order within a module.
  Large modules are split into several bounded initializers; each counts toward
  the user function limit. Initializers are not exported, are not function-value
  dispatch targets and are omitted for modules whose constants all fold. Tool
  and HTTP entry points do not run `main`: binary64 profiles already reject tool
  and HTTP contracts, and a program combining stored constants with tool or HTTP
  entry points is rejected explicitly.
- **`as const`.** `as const` is admitted around a whole `const` or `let`
  initializer, at module scope or inside a function. The value keeps its
  ordinary element and field types (literal and tuple types are not inferred) and
  the binding becomes readonly. Writes reached from that binding through member
  chains are rejected with ``cannot modify readonly `as const` value X``:
  assignment, compound, logical and update assignment, `delete`, `length` writes,
  destructuring targets, `push`/`pop`/`reverse`/`fill` and `Object.assign`
  targets, including inside closures and through imports. Readonly is a static
  property of the binding, not a runtime freeze: a value passed to a parameter or
  aliased by another binding follows the ordinary mutation rules, matching
  JavaScript, where `as const` does not freeze. `as const` elsewhere stays
  unsupported.

Module constants add no host import, value tag, record or array layout, global,
helper, export or catalog entry; the reserved permanent root slots are the only
memory-layout change. All budgets are unchanged.

#### Literal types (`literal_types()`)

- **Forms.** String, number (including `-n`) and boolean literal types and unions
  of them, written inline or through named aliases, interface fields, imports,
  arrays and function types. Literal members of one primitive kind merge into a
  single constrained primitive, a value set: `'a' | 'b'` is one string set,
  `true | false` is `boolean`, and a base primitive absorbs its literals
  (`'a' | string` is `string`). String members compare as profile-encoded code
  units; number members compare as binary64 values with `-0` equal to `0`.
  NaN, bigint and template literal types are rejected.
- **Bounds.** These compile-time limits are explicit constants in
  `varta-compiler` (`lower::literals`): one set holds at most
  `MAX_LITERAL_MEMBERS` (4096) members, sized for generated API code; all sets
  interned by one compilation, guard refinements included, hold at most
  `MAX_LITERAL_TABLE_MEMBERS` (65,536) members, and a refinement that would exceed
  it is skipped rather than rejected. Literal members do not count against the
  structural union limit `MAX_UNION_MEMBERS` (16), which still bounds the other
  members, so a union annotation has at most 4096 + 16 syntactic members. A union
  that mixes a set with other members (`Kind | null`, `'a' | number`) is an
  ordinary structural union within the existing value-ABI union limits.
- **Representation.** Literal types are compile-time only. Before emission every
  set erases to its base primitive (String, Float64 or Boolean slot); union boxes,
  record layouts, array descriptors, JSON schemas, the capture catalog and function
  signatures use erased types, and function types that coincide after erasure are
  merged. No host import, value tag, layout, helper, catalog or ABI version
  changes, and production budgets are unchanged.
- **Assignability.** A constant, or a literal-typed value whose set is a subset,
  is assignable to a literal set; a literal-typed value is assignable to its base
  primitive and to unions containing the base or a superset. A base-typed value is
  not assignable to a set without narrowing. The check applies to bindings,
  assignments (simple, compound, logical and destructuring), arguments, returns,
  record literal fields and field writes, array literals, element writes,
  `push`/`fill` and callback results. Without a literal context a literal-typed
  value reads as its base primitive: comparisons, arithmetic, string methods,
  concatenation and `JSON.stringify` see the base. An unannotated `let`/`const`
  keeps the literal type of a literal-typed initializer; a fresh constant
  (`const s = 'a'`) infers the base type.
- **Classes and templates.** Class fields, constructor and method parameters
  and results may have literal types; class shapes erase like record shapes, a
  literal field takes its base placeholder before construction and is checked on
  every write, and `===` guards on a field read narrow a local copy. A
  literal-typed template substitution is converted as its base primitive.
- **Composite types.** Records, arrays, unions and function types that contain
  literal sets are distinct from their base twins and compared exactly:
  `{ type: Kind }` and `{ type: string }` are not assignable to each other, nor are
  `Kind[]` and `string[]`, and an annotated callback parameter must match its
  context. `Object.assign` and computed record writes select fields by static
  type: a literal-typed source may store into a base or superset field, while a
  base-typed source into a literal field throws the existing
  `TypeError: Invalid computed record field value`.
- **Narrowing.** `x === c`, `c === x` and `!==` with a string, number (including
  `-n`) or boolean constant refine a direct local: the positive branch narrows a
  base, a containing set or a matching union member to the singleton, and the
  negative branch removes the constant from literal sets. Refinements follow the
  existing write-aware flow rules (`&&`, `!`, exits and joins); v32 also refines
  `a || b`, and a failed `a && b`, by joining the two guarded states. `typeof`
  guards treat literal members as their base. A `switch` discriminant reads as its
  base, so number and boolean sets switch as before; string `switch` remains
  unsupported and case bodies are not narrowed.
- **JSON input.** `varta.json.parse<T>` rejects target types that contain a literal
  set anywhere (`unsupported JSON parsing target with a literal type`). Schema13
  cannot express value sets, and accepting them would place unchecked non-members
  in literal-typed storage. Parse the base type and narrow it; strict membership
  validation needs a future schema version. JSON output is unchanged.
- **Earlier profiles.** V31 and earlier keep rejecting literal type annotations with
  `unsupported type annotation` and keep the 16-member union limit; unused
  declarations remain ignored.

#### Classes (`classes()`)

Top-level class declarations with nominal instances, single inheritance and
closed-world method dispatch. The slice targets the pilot's `APIResource` and
generated resource classes; classes extending the native errors follow the
[Error subclasses](#error-subclasses-error_subclasses) rules.

- **Declarations.** A top-level `class` (exported or not) registers its name in
  the v31 per-module type namespace, so it is usable in annotations and in
  `import`/`import type`/`export { C }` lists like an interface. The same name is
  the class value for `new C(...)`, `instanceof C` and `extends C` unless a
  lexical binding shadows it. Because the value is resolved through the type
  namespace, `export class`, export lists and
  [re-exports](#module-re-exports-module_reexports) (`export { C as D } from`,
  `export *`) forward the type and the value together. Known gap: a class
  reached only through `import type`/`export type` is still usable as a value,
  which TypeScript rejects; the program semantics are unaffected. A class may not share its name with a module
  function, another type or a built-in (`Array`, `VartaResult`, HTTP types,
  `Object`/`Math`/`Number`/`String`/`Boolean`/`BigInt`, Error families).
- **Members.** Instance fields with a type annotation or a literal initializer
  (whose literal type is the field type), optionally `readonly` and
  `public`/`protected`/`private`; methods; `get` accessors; static methods; at
  most one constructor; `extends` of another declared class (in any module of the graph);
  `super(...)` and `super.m(...)`. Field types follow record field rules (no
  function-valued fields) and may name other classes, including the class itself
  through a union such as `List | null`.
- **Layout.** Each class owns one private record shape (`RecordShape.class`),
  never interned with structural shapes and counted against
  `value::MAX_RECORD_SHAPES`. Its slots list the inherited fields first, then the
  own fields in declaration order (at most `value::MAX_RECORD_FIELDS`), so every
  subclass instance is slot-compatible with each ancestor. An instance is an
  ordinary managed record block (heap record flag, per-instance reference mask,
  all-present presence trailer) whose static descriptor is the canonical layout
  of its most-derived class. The collector traces the instance by its own header,
  so subclass-only reference fields are traced exactly whatever the static type.
  No heap flag, value tag, host import, helper or value/host ABI version is added.
- **Identity and `instanceof`.** The descriptor offset of the most-derived class
  is the class identity. `v instanceof C` folds to true (after evaluating `v`)
  when the static class of `v` is `C` or a subclass, compares the descriptor
  against `C` and its descendants when `C` is a proper subclass of the static
  class, and folds to false for unrelated classes and non-class values. On a union
  with a class member it tests the union tag, then the descriptor. A class that is
  never constructed has no descriptor and never matches.
- **Subtyping and unions.** Class types are nominal. A subclass instance widens
  to an ancestor class (or to a union holding one) in every contextual position
  with the payload unchanged; there is no compatibility with structural records
  or interfaces and no implicit downcast. A class is a record-tagged union member:
  a union holds at most one record or class member. `instanceof C` narrows a union
  binding to its class member when that member is `C` or a subclass; a downcast
  test does not narrow (except to an Error class, see
  [Error subclasses](#error-subclasses-error_subclasses)). Identity comparison of related classes uses their common
  ancestor; `typeof` is `"object"` and instances are truthy.
- **Construction.** `new C(args)` checks the arguments against `C`'s initializer
  signature, allocates the instance with placeholder slots (zero handles, which
  the collector skips, zero scalars and empty strings) and calls the initializer
  `(this: C, ...params) => void`. A base initializer runs the own field
  initializers in declaration order, then the constructor body. A derived
  constructor runs the statements before its `super(...)` statement, the base
  initializer, its own field initializers and then the rest of its body. An
  omitted constructor is `constructor() {}` for a base class and forwards the base
  initializer's fixed and optional parameters for a derived class; a base with a
  rest parameter requires an explicit derived constructor.
- **Definite initialization.** No placeholder is ever observable. In constructor
  bodies, `this` may only write declared fields and read fields already
  definitely assigned: fields with initializers, inherited fields after
  `super(...)`, and fields assigned by an earlier top-level `this.f = …`
  statement. It may not escape, be captured by an arrow, receive a method or
  getter call, or reach `super.m`, because an override could observe a subclass
  placeholder. Statements before `super(...)`, parameter defaults and field
  initializers cannot use `this`; field initializers cannot name constructor
  parameters or top-level constructor bindings. A derived constructor has exactly
  one top-level `super(...)` statement. Every own field without an initializer
  must be assigned by a top-level `this.f = …` statement; assignments inside
  branches or loops are writes but do not count. `return` and `new.target` in a
  constructor are rejected, except the `error_subclasses()` form `new.target.name`.
- **Methods and dispatch.** A method is a function whose receiver is its class
  (the v12 typed-receiver ABI: the first physical parameter); calls evaluate the
  receiver and then the arguments once, in source order. An override keeps the
  inherited parameter types, optional/rest shape, result, accessibility and member
  kind; private members cannot be overridden. Classes form a closed world over
  the module graph: after every module is laid out, each root method that a
  descendant overrides gets one synthesized dispatcher that tests the receiver's
  descriptor against each implementation's classes and forwards all physical
  parameter slots. Methods without overrides, `super.m(...)` and `super(...)` are
  direct calls. A getter has no parameters and a non-void result, is invoked by a
  property read and dispatches like a method; it is not an own data property, so
  JSON omits it. A static method has no receiver (`this` is rejected inside it)
  and is called directly as `C.m(...)`; subclasses inherit and may redeclare
  statics, and the call resolves statically against the named class.
- **Access.** `private` members are accessible only inside the declaring class
  body (including nested closures), `protected` members inside the declaring class
  and its subclasses through receivers of the enclosing class or its subclasses,
  and `readonly` fields are written only by the declaring class's constructor
  through `this`. These are compile-time checks with no runtime representation.
- **Value operations.** `JSON.stringify` serializes the runtime instance's own
  fields through its descriptor in slot order; JSON parsing into a type containing
  a class is rejected because parsing cannot construct nominal instances. Record
  operations (`Object.keys`/`values`/`entries`/`assign`/`hasOwn`, spread,
  destructuring, deletion, dynamic keys), logging, structural assignment from
  object literals and method values without a call are rejected.
- **Effects.** Constructors, methods and getters are user code. A member call
  whose method name is declared by any class is no longer treated as a native
  effect-free intrinsic by guard flow analysis, `new` of a class counts as an
  unknown call there, and calls whose parameters can carry function values widen
  captured-cell writes. Class fields cannot hold function values, so receivers do
  not carry callbacks.
- **Capture catalog.** A captured class-typed binding is catalogued as the
  record-tagged `class` leaf of the [typed private capture
  catalog](#typed-private-capture-catalog); its fields are compile-time checked,
  so class field cycles never become catalog cycles.
- **Rejected with diagnostics.** Generic, abstract, decorated (parser) and
  `implements` classes (generic classes, `implements` and `#private` members
  are admitted by v37 [class generics](#class-generics-class_generics)); index signatures; `static` fields, blocks and
  accessors; static method values without a call; setters; `#private` fields and methods; auto-accessors; constructor parameter
  properties; optional, definite (`!`), `declare` and overriding fields;
  function-valued fields; `toJSON` methods; explicit `this` parameters on methods;
  private/protected constructors and overloads; class expressions and class
  declarations inside functions; `extends` of non-class expressions or undeclared
  names (Error families only under `error_subclasses()`); recursive inheritance;
  and unions with more than one record or class member.

Profiles before v32 keep rejecting class declarations (`unsupported top-level
declaration`), class expressions and nested class statements with their previous
messages. Production budgets are unchanged: each class uses one record shape, one
initializer function and one function per method, plus one dispatcher per
overridden root method, all within `abi::MAX_USER_FUNCTIONS`. Pending for the
pilot: `#private` fields (excluded from JSON), static fields (module-level state)
and setters; `class … extends Error` is covered by
[Error subclasses](#error-subclasses-error_subclasses).

#### Module re-exports (`module_reexports()`)

Gated by `module_reexports()`. V32 admits re-export declarations with
ECMAScript linking semantics, applied separately to each binding namespace:
the value namespace (functions and module constants) and the type namespace
(named types and classes, whose value follows the type binding).

- **Explicit re-exports.** `export { x } from "./m.ts"`, `export { x as y } from`,
  `export type { T } from` and `export { type T } from` forward an export of the
  dependency under the given name. A value form forwards every namespace in which
  the source exports the name and must find it in at least one
  (`module m.ts does not export function or constant x`); a type-only form
  forwards only a type (`module m.ts does not export type T`). Re-exported names
  are **not** bound in the re-exporting module: calling, reading or annotating
  with them there, or listing them in a local `export { … }`, fails like any
  unknown name.
- **Star re-exports.** `export * from "./m.ts"` forwards every export of the
  dependency except `default`, transitively through chained stars. Explicit
  exports of the module (local or re-exported, in any namespace) shadow star
  names. A name that several star sources provide with distinct bindings is
  ambiguous and excluded; two sources providing the same original binding (the
  same function slot, constant declaration or defining type declaration) are
  not ambiguous; a function and a constant of one name from different stars
  are ambiguous, because they share the value namespace. Ambiguity
  is never an error by itself: only an import or explicit re-export naming the
  ambiguous name fails (`module m.ts exports x ambiguously through export *`),
  and ambiguity propagates through further star re-exports.
- **Graph.** Every re-export source is a graph dependency: it is resolved by the
  loader, visited in sorted order, linked before the re-exporter, included in the
  module metadata, subject to cycle detection, and counted against the module,
  depth and import-declaration limits, which are unchanged.
- **Rejected with diagnostics.** `export * as ns from` (namespace re-export),
  `export {} from` (side-effect re-export), string export names,
  `export type * from` and attributed re-exports. `default` in re-export lists
  follows [default exports](#default-exports-default_exports). V31 and
  earlier keep `re-export or type-only export` for `export … from` and
  `module export form` for `export *`.

Re-exports are resolved entirely at compile time: no host import, value tag,
layout or artifact field changes.

#### Template literals (`template_literals()`)

Selector `template_literals()`. V32 admits untagged template literal expressions
`` `head${a}middle${b}tail` ``; v31 and earlier profiles keep rejecting every
template with `unsupported expression`.

- **Type.** A template literal is a `string` value in every expression context
  (bindings, arguments, returns, record fields, array elements, comparisons,
  conditions). An unannotated parameter whose default is a template infers
  `string`, like a string literal default.
- **Evaluation.** Substitutions are evaluated exactly once, left to right. Each
  value is converted with ECMAScript ToString immediately after it is evaluated
  and before the next substitution runs, so a throwing or exiting substitution
  aborts before any later substitution. Strings pass through unchanged. Number,
  Boolean, `null`, `undefined` (including void calls) and closed primitive unions
  use the [v19 primitive String conversion](#checked-f64-utf16-v19--primitive-string-conversion)
  through the profile's existing `primitive_to_string_v2` import (canonical v18
  Number text); BigInt values use the existing decimal BigInt text operation.
  The parts are then joined with guest string concatenation, which shares the
  checked heap, exact roots and the 1 MiB per-value limit of `+`; exceeding the
  limit traps exactly like an oversized concatenation.
- **Text parts.** Cooked template text is encoded like string literals: escapes
  (`\u{…}`, `\x…`, `\``, `\${`, line continuations) are exact, a raw CR LF or CR
  line terminator cooks to LF, and lone surrogates are kept as single UTF-16 code
  units. Empty parts emit no concatenation.
- **Rejected with diagnostics.** A substitution whose type has a non-primitive
  member (records, arrays, functions, Error values, reference unions and class
  instances) fails with a type error at the substitution: it would need
  ToPrimitive, which is not admitted. Literal-typed values are primitives and
  convert as their base type. Tagged templates, including `String.raw`,
  fail with `unsupported tagged template literal (including String.raw)`.

Template literals add no host import, value tag, layout, helper or catalog entry;
artifacts are indistinguishable from the equivalent `String(…)` and `+` program.
All budgets are unchanged.

#### Default exports (`default_exports()`)

Selector `default_exports()`. A default export is an ordinary binding exported
under the name `default` (ECMAScript semantics), so it links through the value
and type [export tables](#module-re-exports-module_reexports) like any other name.
V31 and earlier keep `module export form` for `export default`,
`default or namespace import` for `import x from` and `string or default export
name` for `default` in export lists.

- **Declarations.** `export default function f(…) {…}`, `export default class C
  {…}` and `export default interface I {…}` declare the ordinary top-level
  function, class or interface `f`/`C`/`I` and export it as `default`; the local
  name stays usable in the module. Anonymous `export default function (…) {…}`
  and `export default class {…}` bind ECMAScript's unspellable local name
  `*default*`. Under [generics](#generics-generics) the declaration may be a
  generic function or interface; generic classes stay rejected.
- **Expressions.** `export default x` where `x` is a module binding (function,
  class, module constant, named type or import) exports that binding in every
  namespace in which it exists, like `export { x as default }`; naming a local
  `const` or class declared later fails (`default export of x precedes its
  declaration`), because ECMAScript would read it in its temporal dead zone. Any
  other expression (including global values such as `undefined`) becomes the
  [module constant](#module-constants-module_constants) `*default*` and follows
  the constant initializer subset and its diagnostics.
- **Lists, imports and re-exports.** `export { x as default }`, `import x from`,
  `import x, { y } from`, `import type T from`, `import { default as x } from`,
  `export { default } from` and `export { default as x } from` are admitted.
  `export *` never forwards `default`. A module has at most one default export
  (`duplicate export: default`); importing a missing one fails with `module m.ts
  has no default export`.
- **Rejected with diagnostics.** `import * as ns from` (`namespace import`),
  `export * as ns from`, `export { default }` without `from` (parser), string
  export names and the `export v from` proposal.

Default exports are resolved at compile time and add no host import, value tag,
layout, catalog entry or artifact field. Budgets are unchanged.

#### Interface inheritance and merging (`interface_extends()`)

Selector `interface_extends()`. V31 and earlier keep `interface extends` and
`duplicate type declaration` for these forms.

- **Inheritance.** `interface B extends A, C { … }` is the closed record type
  whose fields are B's own members plus every field of each base. A base is a
  type reference resolved like an annotation in the declaring module: a local or
  imported interface or an alias of a closed record type, so bases from other
  modules and re-exports work and cycles fail with `recursive type declaration`.
  The result is an ordinary structural record shape (`interface B extends A {}`
  is the shape of `A`); there is no width subtyping between B and A, no new
  layout and no runtime representation.
- **Generic interfaces.** Under [generics](#generics-generics) an interface
  with type parameters may extend bases written with type arguments
  (`interface CursorPage<T> extends Page<T>`). Each instantiation binds the
  parameters, then resolves the own members and the base references in that
  frame, and is memoized like any generic named type. Generic call inference
  through such an interface reads its own members and each base reference;
  the inferred arguments are checked against the full instantiated type.
- **Redeclaration.** An own member may redeclare an inherited field with a type
  assignable to it under the profile's assignability (identical, a literal or
  union member of the base type, a subclass) and may make an optional field
  required; making a required field optional or an incompatible type fails with a
  type error at the member. Two bases providing one field must agree exactly
  unless B redeclares it.
- **Declaration merging.** Several `interface X` declarations in one module merge
  their members and `extends` clauses; a repeated member must be identical. All
  declarations must be exported or all local; one export binding is recorded.
- **Rejected with diagnostics.** Bases that are classes (nominal instance types:
  `interface A extends class C`), non-record types, qualified or computed bases,
  declaration merging of generic interfaces
  (`declaration merging of generic interface X`), and merging an interface
  with a class (`declaration merging of interface X with class X`): merged
  members would be class fields its constructor never initializes. The pilot's
  `interface UarpClient extends Resources {}` beside `class UarpClient` is
  rejected this way. Like every named type, an interface is resolved when it is
  referenced; an unreferenced invalid declaration is not diagnosed.

#### Generics (`generics()`)

Selector `generics()`. V32 admits generic interfaces, generic type aliases and
generic top-level functions, implemented by monomorphization: every distinct
type-argument list is an ordinary closed type or an ordinary emitted function,
so nothing generic reaches emission. V31 and earlier keep `unsupported generic
type declaration`, `unsupported generic type reference` and the existing
generic-function and generic-call diagnostics.

- **Type parameters.** A declaration lists parameters with optional `extends`
  constraints and defaults; defaults and constraints may name earlier parameters
  (a constraint may also name its own parameter). Only the declaration's own
  parameters are in scope in its body: a named type referenced from a generic
  body never sees the caller's parameters. `in`/`out`/`const` modifiers and
  duplicate names are rejected. A constraint is checked with the ordinary
  argument assignability (literal subsets, class upcasts, union members);
  `object` admits records, class instances, arrays and functions, `unknown`
  admits everything.
- **Generic types.** `interface CursorPage<T> { … }` and `type Pair<A, B = A> = …`
  are instantiated per distinct argument list, in the defining module, and the
  result is interned structurally: `CursorPage<number>` is the same shape as the
  equivalent type literal or interface. A reference must supply every parameter
  without a default (`type X requires a type argument for T`); type arguments on
  a non-generic name are rejected. Recursion through instantiation, direct or
  through other names and with any arguments, stays `recursive type declaration`.
  Instances are memoized by declaration and argument list.
- **Generic functions.** A call `f<A, B>(…)` uses the explicit arguments
  (missing trailing ones take defaults); a call without type arguments infers
  them from the argument types. Arguments are lowered exactly once, in source
  order: an argument whose parameter type is already determined is checked
  against it contextually; otherwise it is lowered without context (a bare type
  parameter with a lowerable constraint uses the constraint as context, so
  `keys: K` with `K extends readonly (keyof T & string)[]` takes literal keys)
  and matched structurally against the parameter type: bare parameters, arrays,
  `readonly` arrays, function types, record type literals, generic named types
  (through their declaration body) and unions with exactly one generic member
  (`T | undefined` binds `T` to the remaining members). A contextual result type
  determines parameters the arguments left open. Conflicting inferences, and
  parameters that remain undetermined without a default, are rejected with a
  diagnostic asking for explicit type arguments; an argument whose
  context-free type fixed an instantiation it does not match (for example
  `{ cursor: null }` against `string | null`) reports that the type arguments
  were inferred. Each distinct instantiation is one function with its own index,
  lowered in the defining module with that module's functions and constants, and
  repeated instantiations share it. Instances count against the user function
  limit (`varta_common::abi::MAX_USER_FUNCTIONS`) and each body against the Wasm
  body limit; instantiations requested from instance bodies nest at most
  `MAX_INSTANTIATION_DEPTH` (32, `varta-compiler` `lower::generics`) deep, which
  rejects polymorphic recursion. Generic functions link through imports, export
  lists, re-exports and [default exports](#default-exports-default_exports)
  (`export default function f<T>…`, including the anonymous form) in the value
  namespace like functions and constants; a default-imported generic function
  keeps the generic call and `generic function as a value` rules.
- **Checking.** A generic function body is checked once per instantiation, like
  any function with those concrete types. A generic function that is never
  instantiated is only checked for its declaration form and for explicit `any`;
  its body produces no code.
- **Type operators.** `keyof R` of a closed record type is the string literal
  set of its field names (`keyof` of an empty record, a class or any other type
  is rejected). `R[K]` with a string literal set `K` is the field type (the union
  of field types for several keys); `A[number]` of an array type is its element.
  An intersection collapses when each member is a subtype of the narrowest one
  (`keyof T & string`); other intersections and `typeof` type queries are rejected.
- **Built-in generic references.** `Readonly<T>` is `T`, `ReadonlyArray<T>` and
  `readonly T[]` are `T[]`: readonly stays binding-level intent, so writes through
  such annotations are not rejected and behave as in JavaScript. `Partial<R>` of
  a closed record is the record with every field optional. `Record<K, V>` with a
  string literal key set is the closed record with one required `V` field per
  key. `Record<string, V>` and other non-literal keys (dynamic keys need a map
  value representation) and `Promise<T>` (asynchronous values need the suspension
  foundation) are rejected with those reasons; a declared or imported type of the
  same name shadows a built-in generic. `Array<T>` is unchanged.
- **Rejected with diagnostics.** Generic classes, class methods, arrows,
  function expressions and callback types; async or generator generic
  functions; generic functions with a `this` parameter; a generic `main`; generic
  functions used as values (`generic function as a value`) and spread arguments
  to them.

Generics add no host import, value tag, layout, helper or catalog entry; an
instantiation is indistinguishable from the hand-written specialized function.
All budgets are unchanged.

#### Error subclasses (`error_subclasses()`)

Selector `error_subclasses()`. V32 admits [classes](#classes-classes) whose
`extends` clause names `Error`, `TypeError`, `RangeError` or `SyntaxError`
(the v9 families), and subclasses of such classes in any module. V31 and earlier
profiles keep rejecting class declarations; without the selector `extends` of a
native error keeps `class extends a built-in Error class`.

- **Representation.** An instance is an ordinary class instance record; no value
  tag, heap flag or layout is added and the 16-byte Error object is unchanged. Each
  family has one synthetic root class that is never constructed and owns no
  function: the Error root declares the writable fields `message: string`,
  `name: string` and `cause` (slots 0–2 of every Error class), and the TypeError,
  RangeError and SyntaxError roots extend it, mirroring the native prototype
  chain. Roots are not named in the type namespace, so `Error`, `new Error(…)` and
  `instanceof Error` on native values keep their v9 meaning. Identity, dispatch,
  tracing and collection are the class rules: the descriptor of the most-derived
  class is the identity, and the collector traces `message`, `name`, `cause` and
  subclass fields through the instance's reference mask.
- **Construction.** `super(message?, options?)` to a root is lowered inline: both
  arguments are evaluated once, in order; `message` must be `string`,
  `undefined` or their union (no coercion) and is stored as given or as the empty
  string; `name` is set to the family name (the inherited default, so `toString()`
  of `class A extends Error {}` is `Error: …`); `options` must be `undefined`, an
  object type or their union, and `cause` is its `cause` property when the object
  has one and `undefined` otherwise (InstallErrorCause; a missing `cause` field
  reads `undefined`). An implicit constructor of a direct subclass forwards
  `(message?, options?)`, where `options` is `{ cause?: <catch union> }`, so
  `new A('m', { cause: caught })` accepts any caught value. Constructor parameter
  defaults, readonly fields and the definite-initialization discipline are the
  class rules.
- **`new.target.name`.** In a constructor body, its parameter defaults and its
  `super(...)` arguments, `new.target.name` is the name of the most-derived class
  being constructed, selected by testing the receiver's descriptor against each
  descendant of the enclosing class. Bare `new.target`, other properties, field
  initializers, closures, methods and functions keep rejecting it. The form is
  admitted in any class constructor under the selector.
- **Members.** `message`, `name` and `cause` are public writable fields of every
  Error class; an assignment such as `this.name = new.target.name` is an
  ordinary field write, and subclasses cannot redeclare them (`duplicate class
  member`). `toString()` implements `Error.prototype.toString` over the current
  `name` and `message` (an empty part drops the `: ` separator). `cause` has the
  catch-union type; it accepts any primitive, BigInt, native error or Error class
  instance. The `unknown` annotation is not admitted, so an options parameter
  names a concrete cause type such as `{ cause?: string }`; a non-throwable cause
  type is rejected (`Error cause must be a primitive, BigInt or Error value`).
- **`instanceof` and narrowing.** An Error-class instance is `instanceof Error`
  and `instanceof` its own family (statically, or by descriptor for the root of a
  union), and of every class in its chain. On a union that may hold an instance,
  `instanceof Error`/a family combines the v9 host class test with the class test
  against the family root; positive narrowing keeps the members that may match and
  negative narrowing removes only certain matches. For Error classes only,
  `x instanceof C` also narrows a binding whose class member is an ancestor of `C`
  to `C` (a downcast proven by the descriptor test). On a union of native errors
  and one Error class (for example after `instanceof Error`), `name`, `message`
  and `toString()` test the class member first; other properties need a narrower
  type. Unions join Error classes to their nearest common ancestor, so a union
  keeps one class member (`cond ? new A() : new B()` has the common base type).
- **Exceptions.** `throw` accepts Error-class instances. When the program
  declares an Error class, the catch union additionally holds the Error root, so
  every caught instance keeps its identity and subclass. Finally, rethrow,
  cross-module propagation and catch object patterns (`catch ({ name, message })`
  reads the fields of an instance) work as for native errors. Error classes
  link through default exports and re-exports like other classes, and generic
  functions instantiate over them (`function raise<E extends UarpError>(e: E)`);
  generic Error subclasses are rejected as `generic class`.
- **Channel v5.** V32 selects the immutable i32 export
  `varta.exception.version_v5 = 5` instead of v4; each profile requires its own
  selector, so v31 modules (v4) and v32 modules (v5) reject each other's selector
  and initialization checks the value before guest effects. Channel v5 carries the
  channel-v4 values plus a Record cell. After `_start` returns with a pending
  Record, the runtime validates it as a value-13 record and requires its descriptor
  to declare non-optional String fields `name` and `message`; it copies both into
  the owned `ThrownValue::ErrorInstance { name, message }`. Display follows
  `Error.prototype.toString` with bounded escaped text (for example
  `APITimeoutError: "Request timed out"`). Channel-v4 decoding keeps rejecting
  Record cells. A forged record with those fields can only change its own
  diagnostic; validation keeps every access in bounds.
- **Rejected with diagnostics.** `super(...)` to a root with more than two or
  spread arguments; non-string messages; non-object options; `JSON.stringify` of
  any value that may reach an Error-class instance (own-property enumerability of
  `message`/`cause` is not modelled); `stack`; and the record operations,
  direct logging and ToPrimitive conversions that classes already reject. A
  native-error `Error` annotation does not accept an Error-class instance.
  Logging a caught union prints an instance member as its fields, like other
  class members of unions, not as a stack trace.

Error subclasses add no host import, value tag, heap layout or budget. The
version matrix records the channel selector; all other v31 versions are shared.

### `checked-f64-utf16-v33` — Suspension and generators

V33 is registered and inherits every v32 feature and selector, including
exception channel v5 and the v32 JSON selectors; its value and schema versions
are those of v32 (see the [version matrix](#version-matrix)). It is the first
profile with its own host ABI after v29: the K4.4 design admits the exported
`varta_resume` dispatcher only under a new host ABI, so runtimes that predate
frame admission reject v33 modules before compilation or any guest effect, and
v32 and earlier profiles keep their host ABI. The v33 host ABI adds no host
import; every operation available under the v32 host ABI keeps its signature.

Two selectors gate the K4.4 work streams:

- `resumable_frames()`: runtime admission of the
  [suspension foundation](#suspension-foundation-k44-block-1), that is
  continuation frames, function catalog version 2 in preflight and the
  `varta_resume` export (K4.4 step 1), specified
  [below](#runtime-admission-resumable_frames);
- `generators()`: synchronous `function*`, `yield` and the generator
  `next`/`throw`/`return` protocol over those frames (K4.4 step 2), specified
  [below](#synchronous-generators-generators).

A v33 module without a generator compiles and runs exactly like a v32 module
apart from its declared profile and host ABI. Generators add no value tag, so
the value and schema versions stay those of v32, and production budgets are
unchanged.

#### Runtime admission (`resumable_frames`)

V33 is the only profile whose modules may carry the frame surface of the
[suspension foundation](#suspension-foundation-k44-block-1): function catalog
version 2 with `frames` rows, the frame heap kind and the exported
`varta_resume` dispatcher. Runtime preflight and artifact validation extract the
same facts from the portable payload and apply one shared rule set
(`varta_common::function::admission`) before compilation, instantiation or any
guest effect. The runtime reports a failure as the typed
`RuntimeError::Frames`; artifact I/O reports it as an invalid artifact.

- **Catalog reader.** A `resumable_frames()` profile reads catalog versions 1
  and 2 (v35 to v38, the `frame_liveness()` profiles, read versions 1 and 3 instead, see
  [frame liveness](#frame-liveness-frame_liveness)). Every other profile keeps
  the version 1 reader, which never accepts versions 2 or 3; a frame catalog
  under such a profile fails as a profile that does not admit resumable frames. Duplicate catalog sections, unknown fields,
  out-of-range binding or type ids and the frame budgets (`MAX_FRAME_BINDINGS`
  slots and `MAX_SUSPENSION_STATES` states per layout, unique resume codes below
  `MAX_USER_FUNCTIONS`) fail in the reader. The catalog value version must still
  equal the profile's value ABI.
- **Earlier profiles.** Under v32 and every earlier profile, a `varta_resume`
  export of any kind fails, also next to a version 1 catalog, and so do frame
  layouts.
- **Presence.** Under v33, frame layouts and the dispatcher occur together:
  layouts without the export fail, and the export without layouts (no catalog or
  a version 1 catalog) fails. A v33 module without suspension points has neither.
- **Dispatcher.** `varta_resume` is a guest-defined function export with exactly
  `(i64, i32, i64) -> i32`. Its own body contains no `call_indirect`,
  `return_call_indirect`, `call_ref`, `return_call_ref`, table or element
  instruction, and each direct call targets a guest-defined function other than
  the dispatcher itself, never a host import.
- **Layouts.** The dispatcher's distinct direct call targets with the
  resume-body signature `(i64, i32, i64) -> ()`, in first-call order, are the
  resume bodies. Their number equals the number of layouts, and layout id *n*
  (its row position) is resumed by body *n*. Resume bodies are not exported. A
  layout's resume code names its creator at function index `imports + code`: a
  guest-defined function that is neither the dispatcher nor a resume body and
  returns exactly one `i64` resume token. Yielded and result bindings are
  single-slot cells. The Return input class matches the result: `Float64`,
  `Boolean` or `Undefined` for a result of that type, `Undefined` for a void
  result and none otherwise; a generator row (`generator: true`) takes `Guest`
  instead (see [generators](#synchronous-generators-generators)). Every layout
  names the same exception binding, a union cell holding the profile's catch
  union.
- **Envelope.** The frame constants (five fields, edge mask 28 that excludes the
  control and status words, at most 8 segments of 32 bindings) are checked when
  `varta-common` is compiled. Saved cells and envelopes keep their dynamic checks
  in the dispatcher and in restoration.
- **Classification.** The dispatcher is effectful executable code. It is not a
  tool, value ABI, JSON or HTTP helper, so the host-free helper checks and JSON
  helper purity validation are unchanged and do not apply to it. Admission
  grants no call authority and never invokes guest code.
- **No scheduler in v33.** Under v33 the runtime calls only the entry point
  and the published tool or HTTP exports. It never calls `varta_resume`, and it
  owns no frame roots or generations; generator frames are resumed by guest
  code alone. The v34 [microtask scheduler](#microtask-scheduler-promises) is
  the first caller of `varta_resume`.
- **Test producer.** Besides generators, frame modules come from the
  compiler's test-only lowering hook: compiler unit tests, and the
  `suspension-test-hook` cargo
  feature that only the runtime and launcher dev-dependencies enable. While the
  hook is enabled on a thread, emitted frame modules also export
  `varta.test.frame.<layout>` creators and `varta.test.collect`; admission gives
  these names no meaning.

#### Synchronous generators (`generators`)

V33 admits synchronous generator functions over the frames above. Generators
need no event loop and no new host import; every resumption is a guest call.

- **Declarations.** Top-level `function*` declarations, exported or not,
  including generic ones (each instantiation is its own generator function),
  with a required return annotation `Generator<T>`, `Generator<T, TReturn>` or
  `Generator<T, TReturn, TNext>`. TypeScript defaults omitted parameters to
  `any`; Varta reads an omitted `TReturn` as `void` and an omitted `TNext` as
  `undefined` (a `void` `TNext` is also `undefined`), which can only reject
  programs that would need `any`. `T` is not `void`, and no type argument is a
  generator, `any` or `VartaResult`. Parameters are fixed: optional and rest
  parameters, a `this` parameter, generator methods, generator function
  expressions, async generators and a generator function used as a value are
  rejected with construct-specific diagnostics. Calling a generator function
  binds its arguments into a new frame and returns the generator object without
  running the body.
- **`yield`.** `yield e` checks `e` against `T`; bare `yield` yields
  `undefined`, which `T` must accept. The expression has type `TNext`. `return e`
  completes with `TReturn`; a `TReturn` other than `void` or `undefined` needs a
  return on every path. `yield` may appear in every normalized expression
  position of the [suspension foundation](#suspension-foundation-k44-block-1),
  inside `try`, `catch` and `finally` bodies, loops, labelled statements and
  `switch` case bodies. `yield*` fails as `unsupported yield* delegation`;
  `yield` inside an intrinsic value block (for example a `String(...)` call, a
  template literal or a generator method call), in a `switch` case test or in a
  `do-while` condition fails as `unsupported suspension point inside
  <construct>`.
- **Representation.** A generator object is the handle of its continuation frame
  (type `generator`, one `i64` slot, no value tag). It may be held in locals,
  parameters, results and closure cells and saved in other frames (capture type
  `generator`). Arrays, records, unions, `JSON.stringify`, `console.log`,
  `typeof`, equality and tool/HTTP signatures reject it
  (`unsupported generator inside an array`, `... a union`, `... an object`,
  `cannot log generator` and the existing boundary diagnostics). The value ABI,
  schemas and launcher value maximum stay those of v32.
- **IteratorResult.** `next`, `return` and `throw` produce the closed record
  `{ done: boolean, value: T | TReturn | undefined }`, also written
  `IteratorResult<T, TReturn>`. `undefined` is always a member because a
  completed generator answers `{ done: true, value: undefined }`; a `void`
  `TReturn` contributes `undefined`. It is an ordinary closed record: narrowing
  `value` needs an ordinary guard, not the `done` discriminant. A generator whose
  `T | TReturn | undefined` is not a checked union (for example two record
  shapes) is rejected.
- **Protocol.** `g.next(v)` takes `TNext`, `g.return(v)` takes `TReturn`
  (`undefined` for `void`) and `g.throw(e)` takes any throwable value. Omitting
  the argument of `next` or `return` requires that type to accept `undefined`.
  The ECMAScript states hold. On a not-started generator `next` ignores its
  argument and starts the body, `return(v)` completes it with
  `{ done: true, value: v }` and `throw(e)` completes it and throws `e`, without
  running the body. On a suspended generator the argument becomes the `yield`
  result, a `return` completion (running enclosing `finally` blocks, which may
  yield again) or an exception thrown at the `yield`. On a completed generator
  `next` returns `{ done: true, value: undefined }`, `return(v)` returns
  `{ done: true, value: v }` and `throw(e)` throws `e`. Any call from inside the
  running generator throws `TypeError("Generator is already running")` in the
  caller. An uncaught exception completes the generator and propagates from the
  call.
- **`for...of`.** A generator source resumes with `undefined` on every step, so
  `TNext` and `TReturn` must accept `undefined`. `break`, `return`, a labelled
  `break` or `continue` to an outer statement and a throw from the body close the
  generator with `return()`, whose `finally` blocks run before the loop exits. A
  throw from the body keeps precedence over a throw from closing; an exception
  or completion of the generator itself does not close it. Spread and
  `Array.from` over generators are not admitted.
- **Frames.** Generator layouts are frame catalog rows with `generator: true`
  and the input class `Guest` for every state and for the Return input; the
  catalog rejects `Guest` on other rows and any other class on a generator row.
  The creator registers the new frame in the live table only while it allocates
  the environment and cells, then releases the slot and returns the frame handle
  (`ptr | 48 << 32`). A generator frame is retained by its generator object like
  any other managed reference and collected with it; it neither occupies nor is
  limited by the `MAX_LIVE_FRAMES` table. Because it is never in the table,
  `varta_resume` fails its ownership check for every completion kind, and the
  dispatcher's branch for a generator layout traps before any effect (it keeps
  the direct call so admission pairs bodies and layouts).
- **Step functions.** Each generator type has one closed step function
  `(generator:i64, completion:i32, value:i64) -> outcome:i32` with the dispatcher
  signature; it is not exported. It checks the running and completed status and
  the not-started state before any effect, marks the frame running and calls the
  resume body chosen by layout id and resume code among that type's layouts, by
  direct call, without a table or indirect call. The outcome (yielded, returned,
  done, done with the input, running, throw the input) drives lowered protocol
  code that builds the IteratorResult or throws; yielded and returned values are
  read from the frame's completion cell. The generator handle and the input stay
  expression-rooted across the step, and the exception channel is empty at every
  suspension.
- **Completions in handlers.** A `catch` body that suspends runs after its
  `try` statement: the catch binds the exception and sets a hidden flag, so the
  handler keeps its own completions (it was never protected by its own catch)
  and its states are re-entered directly. A suspending `finally` body saves its
  pending completion kind in a hidden local when it begins and restores it on
  resumption; the pending exception and return value are ordinary saved locals.
  A `switch` whose case bodies suspend evaluates the discriminant and the case
  tests once, in order, into a saved case index, and the emitted switch matches
  that index against constant tests, so a resumption re-enters the selected case
  and falls through to the target case without re-evaluating a test. These rules
  apply to every suspension point, including the test-only hook's.
- **Budgets.** The foundation's frame budgets apply per generator function: 256
  saved bindings and 256 suspension states. At most `MAX_GENERATOR_TYPES` (64)
  generator types exist per module. Saved bindings are cells, so a program that
  keeps many generators suspended pays for their cells within the ordinary
  memory and fuel limits.

### `checked-f64-utf16-v34` — Pilot batch: dynamic records, unknown and async

V34 is registered and inherits every v33 feature and selector, including
resumable frames, synchronous generators, exception channel v5 and the v33 JSON
selectors. It declares two new version identities (see the
[version matrix](#version-matrix)):

- **Host ABI** `abi::SCHEDULED_FRAMES_VERSION`. The async stream adds a runtime
  microtask scheduler that calls `varta_resume` for promise jobs and `await`,
  so a runtime without that scheduler must reject v34 modules before
  compilation or any guest effect. The v34 host ABI adds the value15 readers
  below and the five [scheduler imports](#microtask-scheduler-promises); every
  operation available under the v33 host ABI keeps its signature, and v33 and
  earlier profiles keep their host ABI.
- **Value ABI** `value::DYNAMIC_VALUE_VERSION` (value15), the value layouts
  and tags of this batch: dynamic-key records (index signatures and
  `Record<string, T>`), records wider than `value::MAX_RECORD_FIELDS` (whose
  presence trailer is a layout), recursive alias graphs and promise values. A
  v34 module declares it in its `varta.values` section; a module declaring any
  other value ABI with the v34 profile, a v33 module declaring the v34 value
  ABI, an unknown value ABI and a missing value section all fail validation
  before instantiation. Earlier profiles keep value ABI 13.

Seven selectors gate the batch's work streams (subsections below specify the
implemented ones):

- records stream: `index_signatures()` (`{[k: string]: T}` and
  `Record<string, T>` as a dynamic-key record kind), `recursive_types()`
  (self-referential aliases such as a JSON value union) and `wide_records()`
  (records wider than 32 fields);
- unknown stream: `unknown_type()` (`unknown`, usable after narrowing by
  `typeof`, `===`, `instanceof` or a checked assertion) and
  `checked_assertions()` (`as` and `!` as runtime-checked conversions, never an
  unchecked reinterpretation);
- async stream (K4.4 steps 3 and 4): `promises()` (base `Promise<T>`
  semantics over the microtask scheduler) and `async_functions()`
  (`async`/`await` on the continuation frames).

Until a stream adds its subsection here, a v34 module compiles and runs exactly
like a v33 module apart from its declared profile, host ABI, value layout 15
and the value15 readers, and production budgets of every profile are unchanged.

#### Value layout 15 (`dynamic_values()`)

V34 stamps and reads the value15 layouts: `LanguageProfile::value_layout`
equals `value_abi` for every profile, so v34 record descriptors, record sizes,
presence trailers and closure/frame catalogs carry version 15, and the runtime
checks a v34 capture catalog against it. Value15 keeps every value13 layout and
tag with exactly its value13 meaning — `ValueKind::supported_by(15)` admits the
same tags as value13 (BigInt, Error, unions, sparse vectors, records with
presence/order trailers; never the value14 function tag) — and adds the batch's
own layouts, specified in the subsections below. Value13 readers stay frozen:
they reject descriptors, catalogs and cells stamped 15, and the value15 readers
reject 13, so no graph is read under the other layout.

Because every value-graph reader checks the stamped version, v34 binds its own
pure, capability-free operations to value15. Each mirrors its value13
counterpart's signature, result convention, budgets and recoverable errors
(`HostOperation::value13_counterpart`) and requires host ABI
`abi::SCHEDULED_FRAMES_VERSION`:

| Value15 import | Value13 counterpart |
| --- | --- |
| `value_log_v9` | `value_log_v8` |
| `value_typeof_v8`, `value_truthy_v8`, `value_nullish_v8`, `value_strict_equal_v8`, `value_nullish_equal_v8`, `value_is_array_v8` | the `_v7` operations |
| `value_is_error_class_v4` | `value_is_error_class_v3` |
| `json_stringify_v14`, `json_parse_v16` | `json_stringify_v13`, `json_parse_v15` |
| `primitive_to_string_v3`, `primitive_to_number_v3`, `bigint_from_value_v2` | `primitive_to_string_v2`, `primitive_to_number_v2`, `bigint_from_value` |

`LanguageProfile::host_operation` maps every value13 reader to its value15
replacement for v34 only, so the runtime rejects a v34 module importing a
value13 reader and a v33 module importing a value15 reader (whose minimum host
ABI also exceeds v33's). `json_parse_v16` reads **JSON schema 15**
(`json::DYNAMIC_SCHEMA_VERSION`), which has the schema-13 grammar plus the
batch's new types, and materializes value15 graphs. Thrown values use
**exception channel v6** (`varta.exception.version_v6` = 6): channel-v5
contents (primitives, BigInt, native errors, class instances with String
`name`/`message`) read as value15 cells, records and Error objects.

The switch changes no observable behaviour: tests compile the same programs
under v33 and v34 (JSON stringify and checked/recoverable parse, diagnostics,
String/Number/BigInt conversion, thrown and uncaught class instances,
generators, closures) and require identical, Node-pinned output, and the whole
conformance inventory produces identical reports under both profiles. Older
profiles keep exactly their layouts, operations and rejections.

#### Wide records (`wide_records()`)

V34 admits closed records, interfaces, inline object types, object literals and
class instances with up to `value::MAX_WIDE_RECORD_FIELDS` fields. The ceiling
is a layout limit of value ABI 15, chosen to cover the pilot's widest
interfaces (55 and 60 fields) with a margin and to keep a record's slot set in
one `u128`; it is not a raised budget of an older value ABI. V33 and earlier
keep `value::MAX_RECORD_FIELDS` and their `object field count exceeds limit`
diagnostic, and a v34 shape beyond the new ceiling fails with the same
diagnostic. All other bounds (key bytes, shapes, body bytes, JSON depth and
visits, memory, fuel) are unchanged.

A record with at most 32 fields keeps exactly the value13 layout. A wider
record (`value::is_wide_record`) uses the same payload `[descriptor, count]`
and eight-byte canonical slots, followed by a trailer of
`value::presence_words(count)` presence words (slot `s` is bit `s % 32` of word
`s / 32`) and then `count` insertion-order words (`value::order_offset`).
`value::record_bytes(15, count)` is the payload size. Its block keeps the
record heap flag, but the link word is zero: the 32-bit link-word edge mask
cannot describe it. Instead its static descriptor
(`value::descriptor_bytes`) carries, after the entries, a slot-indexed edge
bitmap of `presence_words(count)` words with the bit set for every managed
reference slot. The collector reads that bitmap one word per 32 slots for a
record block wider than 32 fields; the value15 readers (`ReferenceBox`, the
JSON encoder, capture/frame validation and the JSON parse writer) require the
bitmap to equal the descriptor's field kinds, the link word to be zero, the
presence set to stay inside the shape and the order words to list exactly the
present slots. Checked JSON schema 15 admits record types up to the same
ceiling.

Presence, insertion order, `Object.keys`/`values`/`entries`, `Object.hasOwn`,
optional reads and writes, `delete`, rest destructuring, JSON stringify and
checked parse behave exactly as for narrow records. A `delete` with a static
identifier key under v34 lowers to that field's leaf instead of one leaf per
field, so wide shapes stay within the per-function body budget; computed keys
keep the full lookup.

#### Index signatures (`index_signatures()`)

V34 admits dynamic-key records: a type literal or interface whose only member
is `[key: string]: T`, and `Record<string, T>` (string-literal key unions keep
mapping `Record<K, T>` to closed records). `T` is any value type except
functions and generators, including unions and recursive aliases. Number or
symbol keys, readonly or static signatures and a signature combined with
declared fields (a mixed object type, including through `extends`) fail with a
diagnostic naming the construct. V33 and earlier keep rejecting index
signatures (`object type member`) and `Record<string, T>` (`Record with
non-literal keys`).

**Layout (value15 tag `ValueKind::Dictionary` = 13).** A dictionary is a
managed two-slot record block, stamped value15, whose canonical slots hold its
key vector (`value::DICTIONARY_KEYS_FIELD`, slot 0, dense `string[]`) and value
vector (`value::DICTIONARY_VALUES_FIELD`, slot 1, dense `T[]`), both required
and present. Keys are distinct and stored in ECMAScript own-key order:
canonical array indices (`value::array_index`, at most `value::MAX_ARRAY_INDEX`)
ascending, then the other keys in insertion order. A dictionary holds at most
`value::MAX_DICTIONARY_ENTRIES` entries, the array element bound of its
vectors. The tag selects the dictionary meaning; the collector traces the
block as an ordinary record (both slots are edges) and needs no new heap flag.
`value::Dictionary::read` validates the record block and descriptor, the two
vectors, equal lengths within the bound, presence of every entry, distinct
keys and their order; `ReferenceBox`, the JSON encoder, the value15 log/typeof/
truthiness readers and capture/frame validation (`CaptureType::Dictionary`)
use it. Value13 and every older reader reject the tag.

**Operations** lower to ordinary IR over the two vectors inside a rooted value
block, evaluating the receiver, then the key, then any right-hand side, once:

- `d[k]`, `d.name`, `d["name"]`: the stored value or `undefined`, typed
  `T | undefined`. An absent key naming an inherited `Object.prototype` member
  throws `TypeError: Inherited record property access is unsupported`, as for
  closed computed reads; keys must be narrowed strings (no ToPropertyKey).
- `d[k] = v` (statement or expression; value `v`): replaces an own entry in
  place, or inserts at its own-key position (the private `ArrayIndexKey` helper
  classifies array-index keys without allocation). An absent `__proto__` key
  throws `TypeError` instead of reaching the prototype setter; an insertion
  beyond the entry bound throws `RangeError: Dictionary entry limit exceeded`.
- `delete d[k]` removes an own entry, keeping the order of the rest, and is
  `true`; `Object.hasOwn(d, k)` tests own keys; `k in d` also answers `true` for
  inherited `Object.prototype` member names.
- `Object.keys`, `Object.values` and `Object.entries` return fresh arrays in
  own-key order; object literals with a dictionary context take static keys
  (values run in source order, entries are stored in own-key order). Spreads,
  computed literal keys, compound assignment targets and `for...in` remain
  rejected.
- Dictionaries are object values: identity equality, `typeof` `"object"`,
  always truthy, union members (at most one dictionary per union; an object
  literal takes the dictionary member when the union has no record shape),
  `Array.isArray`/`typeof` narrowing, parameters, results, closures and
  generator frames. `JSON.stringify` writes an object of the entries in stored
  order and omits `undefined` values.

Lookups are linear in the entry count and run inside the ordinary fuel,
memory and host-call budgets; no budget changes. **JSON schema 15** adds
`dictionary` types (`descriptor` of the storage record, `value` type):
checked parsing accepts any keys (decoded to exact UTF-16), rejects duplicates
after escape decoding, bounds entries by `value::MAX_DICTIONARY_ENTRIES`
(`JsonInputLimit::DictionaryEntries`), orders entries as above and builds the
two vectors and storage record through the guest allocator; the runtime checks
the storage descriptor's layout first. A union may not contain both a record
shape and a dictionary (a JSON object would be ambiguous).

#### Recursive types (`recursive_types()`)

V34 resolves non-generic type aliases and interfaces that refer to themselves
when the cycle passes through a record field, an array element or a dictionary
value, for example `type JsonValue = null | boolean | number | string |
JsonValue[] | { [key: string]: JsonValue }`, `interface Tree { children: Tree[] }`
and `interface Link { next: Link | null }`. When a declaration is reached again
while it is being resolved, it receives a reserved type of the kind its body
declares (a record for an interface or object type, a dictionary for an index
signature, a reference union for a union); references resolve to it, and the
finished body fills the reservation, so the interned type graph is cyclic but
finite. Direct cycles without such a step stay rejected with `recursive type
declaration` (`type A = B; type B = A`), a recursive alias as a direct union
member is rejected (`type A = string | A`), and recursive generic declarations
are rejected. V33 and earlier keep rejecting every self-reference.

Recursive graphs are ordinary values: their runtime JSON depth (64) and visit
(4096) bounds and the encoder's cycle check are unchanged, and the collector is
already iterative. Checked JSON schema 15 admits forward and self type
references (decoding enforces the depth and visit bounds instead of static
schema depth); earlier schemas keep strict backward edges. Compiler type walks
over the graph visit each type once. Closure capture catalogs and generator
frame catalogs stay acyclic type DAGs: when a recursive type is reached again
while its catalog node is being described, the back-edge gets a shallow node
of the same value kind whose children are validated by their consumers — a
union closing the cycle (`JsonValue`) is described as `CaptureType::Unknown`
(any canonical cell) and a record reached again (`Tree`) as
`CaptureType::Class` (any record aggregate). Recursive values may therefore
be captured by closures and kept in generator and async frames. A dictionary
or array that contains itself without an intervening union or record
(`type Nest = { [key: string]: Nest }`) has no shallow node, and capturing it
or keeping it in a frame slot fails with a diagnostic.

#### Unknown (`unknown_type`)

Selector `unknown_type()`. V34 admits the `unknown` keyword in every annotation
position (locals, parameters, results, record and class fields, optional fields,
array elements, generic arguments). V33 and earlier profiles keep rejecting it
as `unsupported type annotation`.

- **Representation.** An `unknown` value is a reference-union cell: the same
  immutable `kind/count/payload` cell, union heap flag and reference mask as a
  closed reference union, under the profile's value layout. Its slot tag in
  record descriptors, arrays and capture catalogs is `ReferenceUnion`, and the
  function catalog names it `CaptureType::Unknown`, which validates any canonical
  cell through `ReferenceBox` (children are validated by their consumers). No
  value tag, layout, host import or budget is added. Under v34 the cells use
  value layout 15, and every operation on them (`typeof`, truthiness, nullish
  and strict equality, `Array.isArray`, logging, String conversion, JSON) goes
  through `LanguageProfile::host_operation` to its value15 reader.
- **Admitted values.** Every value whose kind has a cell tag: Number, String,
  Boolean, BigInt, `null`, `undefined`, records (including wide records),
  dictionaries (value15 tag `Dictionary`), arrays, native errors, Error and
  other class instances, and closed unions of these. Function values (no cell
  tag in value13 or value15), generators, promises and `VartaResult` pairs are
  rejected (`function value is not assignable to unknown`; a promise names its
  missing value tag and asks to await it). Widening boxes a scalar or
  reference into a fresh cell without copying the referenced object; a closed
  union keeps its cell. A union with an `unknown` member is `unknown`.
- **Operations.** As in TypeScript, only operations that accept any value apply
  to an unnarrowed `unknown`: assignment and passing to `unknown`, `===`/`!==`
  (the v4 strict-equality host operation: identity for references, content for
  strings, numeric rules for numbers), `==`/`!=` against `null`/`undefined`,
  `typeof`, truthiness, `!`, `&&`/`||`/`??` (selecting a whole operand),
  `instanceof`, `Array.isArray`, `console.log`, template substitution and
  `String(x)`, and `JSON.stringify` (result `string | undefined`). Property
  access, indexing, calls, arithmetic, relational operators, `in`,
  `Object.hasOwn`, `Number(x)`, `switch` and `varta.json.parse<unknown>` are
  rejected with a diagnostic naming the narrowing to apply. `JSON.stringify` of
  an `unknown` is rejected when the program declares an Error class, because the
  cell may hold an Error instance (the v32 rule).
- **String conversion.** `String(x)` and template substitution convert a
  primitive member with the v19 primitive conversion, a native error or Error
  instance with `Error.prototype.toString`, and any other record or class
  instance as `[object Object]`. An array, and a record of a program whose
  classes declare a `toString` method, would need user dispatch: they throw
  `TypeError: String conversion of … held by unknown is not supported`.
- **Narrowing.** The existing guard and flow analysis narrow a direct local:
  `typeof x === "string" | "number" | "boolean" | "bigint" | "undefined"`,
  `x === null`, `x === undefined`, `x === <literal>` (to the literal type),
  `x instanceof C` for a class (descriptor test of `C` and its subclasses) and
  `x instanceof Error`/a native family (the native members of the family plus
  the family's Error-class root when the program declares one). Only positive
  branches narrow; negated guards, `typeof x === "object"`, `Array.isArray(x)`
  and truthiness keep `unknown`, because `object`, `any[]` and `{}` have no
  checked representation. In particular they do not narrow to a dictionary:
  a value15 dictionary, a record and a class instance all answer `"object"`,
  and `k in x` stays rejected on `unknown`; a checked `as Record<string, T>`
  recovers a dictionary instead. Assignment does not narrow `unknown`, as in
  TypeScript. User type predicates (`x is T`) remain rejected. A narrowed read
  unboxes the cell after its tag was tested; reads of record and class
  members are justified by the descriptor test, never by the tag alone.
- **Caught values.** A `catch (e)` or `catch (e: unknown)` identifier binding
  has type `unknown` (TypeScript's `useUnknownInCatchVariables`); the caught
  cell is reused unchanged. Catch object patterns keep reading the v12 catch
  union. Earlier profiles keep the catch union and reject the annotation.
  `throw x` for `x: unknown` is a checked conversion to the catch union: a
  caught value always passes, so rethrowing preserves the value and identity;
  another record or array throws
  `TypeError: thrown value is not a primitive, BigInt or Error value`, because
  the exception channel carries no plain records.
- **Error `cause`.** The Error root's `cause` field and the implicit options
  `{ cause?: unknown }` have type `unknown`, so `super(message, { cause })`
  accepts any admitted value and `error.cause` reads `unknown`.
- **Roots and collection.** An `unknown` local, parameter, result, field,
  element, capture or generator-frame slot is an exact handle root like any
  union; the collector traces the cell's single masked child, then the record,
  array, string, BigInt or Error behind it. Numeric payload bits are never
  edges.

#### Checked assertions (`checked_assertions`)

Selector `checked_assertions()`. V34 admits `x as T` and the non-null assertion
`x!`; `as const` keeps its v32 meaning and angle-bracket `<T>x` stays rejected
(`angle-bracket type assertion`). V33 and earlier profiles keep rejecting `as`
and `!` as `unsupported expression`.

- **Upcasts are free.** When the operand already is a `T` (same type, literal
  subset, union widening, class upcast or widening into `unknown`), the
  assertion is the ordinary checked widening and runs no test. An object or
  array literal operand is typed contextually by `T`, as in TypeScript, so
  `{ a: 1 } as { a: number; b?: string }` builds the asserted shape.
- **Admission.** Other assertions must be TypeScript-comparable: some member of
  `T` must equal or relate to some member of the operand (equal physical type,
  a literal of the base, related classes, native errors and Error classes), and
  `unknown` overlaps everything. Disjoint primitives are rejected like
  TypeScript's TS2352 (`neither type sufficiently overlaps`); distinct closed
  record or array types are rejected because no value of one can have the
  other's layout (`build the value at the asserted type`).
- **Checked targets.** The operand is boxed into a cell when it is not one, held
  in a rooted hidden local, tested once, then unboxed (non-union `T`) or retagged
  (union `T`). The test is a short-circuit chain over `T`'s members:
  primitives, `null`, `undefined`, BigInt and native error classes by member
  tag (and the native class word); a literal type by tag and comparison with
  each literal of its set (cost linear in the set, at most
  `literals::MAX_LITERAL_MEMBERS`); a structural record by its tag and its
  exact canonical descriptor, so a structurally compatible record of another
  closed shape fails; a class by its tag and the descriptors of the class and
  its subclasses; an array whose element type is number, string, boolean,
  bigint, `null` or `undefined` by its tag and its element-kind word (constant
  time; the elements are already typed by that kind); a dictionary
  (`Record<string, T>` or an index signature) whose value type is one of those
  primitives by its `Dictionary` tag and the element-kind word of its value
  vector (constant time; keys are always strings); `unknown` always. `x as
  Error` or a native family also accepts the family's Error-class instances and
  yields the narrowed family union, like `instanceof`. Arrays and
  dictionaries of records, unions, arrays, dictionaries or `unknown`, function
  types, generators and promises are rejected with
  `unsupported type assertion to …`, because the element kind cannot prove
  them in constant time (a `Record<string, unknown>` view of a dictionary of a
  closed union would let writes break the other alias's member set).
- **Failure.** A failed test throws a recoverable
  `TypeError("type assertion to <T> failed")`, where `<T>` is the source text of
  the asserted type (at most 64 characters). This is a deliberate deviation:
  TypeScript erases `as`, so Node never throws there; Varta never reinterprets
  bits instead.
- **Non-null.** `x!` is free when `x` cannot be `null` or `undefined`; it is
  rejected when `x` is always nullish; otherwise it tests the cell and throws
  `TypeError("non-null assertion failed: value is null or undefined")`, and its
  type is the operand without `null`/`undefined` (`unknown` stays `unknown`).
- **Evaluation.** The operand runs once, before the test; the test and the
  conversion allocate nothing except the boxing cell and, on failure, the
  `TypeError`. No host import, layout or budget is added.
#### Microtask scheduler (`promises`)

The runtime owns one explicit, bounded microtask queue per execution; one
execution is one isolate (Wasm instance, Store, memory, globals and queue). A
queue entry is the resume token of a live continuation frame and nothing else:
every promise job is a scheduled frame (see
[promise objects](#promise-objects-promises)), and the runtime runs a job only
by calling the exported `varta_resume` with the token, a `Normal` completion
and the `undefined` input. The dispatcher's ownership, envelope, status and
generation checks therefore reject a late, duplicate or forged token with a
trap before any effect.

- **Turn order.** The entry point is the one task of an execution. After
  `_start` returns with an empty exception channel, the runtime runs the
  microtask checkpoint: it takes jobs in FIFO order, including jobs queued
  while it drains, until the queue is empty. Reactions of one promise are
  queued in registration order when it settles; a reaction registered on a
  settled promise is queued at once. This is the ECMAScript Jobs order, so
  event logs match Node for the same program. V34 and v35 have no timers or
  I/O tasks, so the checkpoint ends the execution; v36 adds timer tasks after
  it (see [Timers](#timers-timers)), still with no I/O tasks.
- **Imports.** Five internal imports exist only under the v34 host ABI
  (minimum `SCHEDULED_FRAMES_VERSION`), take no capability, are never callable
  from source and are matched by exact name and signature:
  `promise_react (serial:f64, token:i64) -> f64` registers a reaction of a
  pending promise and returns its serial (allocating one for serial zero);
  `promise_settle (serial:f64) -> ()` queues that promise's reactions;
  `scheduler_enqueue (token:i64) -> ()` queues one job;
  `promise_unhandled (reason:i64) -> f64` records a rejection without a
  handler and returns its tracking id; `promise_handled (id:f64) -> ()`
  withdraws it. The runtime keys reactions by the serials it issued and never
  retains a guest reference. Unknown or non-integer serials and ids, a token
  held twice and every bound below are fatal `SchedulerError`s.
- **Roots and generations.** A token names a frame in the live-frame table,
  the external root table of the
  [suspension foundation](#suspension-foundation-k44-block-1). Frames stay
  rooted while they are suspended, queued or registered as reactions, and
  leave the table when they complete. A scheduled frame issues its next
  generation at the suspension point, before it registers its token, so the
  registered token is the only one that can resume that suspension; the
  dispatcher issues no further generation for scheduled layouts.
- **Budgets.** The per-execution fuel, deadline, memory and host-call ceilings
  apply to the whole execution, including every job, and are never renewed by
  `await` or the checkpoint. Scheduler imports are bookkeeping, not host
  effects: they consume guest fuel and check the deadline but are not charged
  to the host-call budget. Rationale: every `await` and every reaction costs
  at least one scheduler import, so charging them to the 1024-call budget would
  cap an ordinary sequential async loop at a few hundred iterations, and the
  budget exists to bound effects and host work per call, which these imports
  do not have — they reach no capability, OS call or guest-controlled host
  allocation beyond the named scheduler tables, and each is a constant or
  logarithmic table update (`promise_unhandled` additionally copies a bounded
  prefix of one thrown value, see below). Every await loop
  stays bounded without them: each iteration resumes guest code through
  `varta_resume`, which consumes fuel; epoch interruption enforces the
  deadline inside guest code and each import checks it again; and the
  per-execution job count is bounded below. The host memory held by
  unhandled reports is bounded by named constants, independently of fuel
  and guest input (see the bullet below). Held tokens (queue and
  reactions together), jobs per
  execution, and serials and tracking ids per execution are bounded by the
  scheduler rows of the [limits table](#resource-limits); the live-frame table
  bounds the frames pending at once. Exceeding any bound, the fuel or the
  deadline terminates the execution; a fatal trap or exhaustion inside a job
  is never converted into a rejection.
- **Unhandled rejections.** Rejecting a promise that has no handler reports
  its reason, which the runtime validates and renders at once with the
  uncaught-exception readers; adding a handler later in the checkpoint
  withdraws the report (HostPromiseRejectionTracker). If reports remain after
  the checkpoint, the execution terminates with
  `SchedulerError::UnhandledRejection`, naming the earliest remaining reason
  and the count; the CLI exits nonzero, as Node does.
- **Bounded reports.** The readers copy at most
  `MAX_REJECTION_REASON_UNITS` UTF-16 code units of each string field of a
  reason (a String reason, an Error message, a class instance's `name` and
  `message`; a BigInt is rendered from its own bounded payload) and the
  rendered text keeps at most `MAX_REJECTION_REASON_BYTES` bytes; a cut
  reason ends with `REJECTION_TRUNCATION_MARKER` (` [reason truncated]`) in
  the diagnostic. At most `MAX_RETAINED_REJECTION_REASONS` outstanding
  reports keep their rendered reason; every report keeps one tracking bit
  per issued id (at most `MAX_PROMISE_SERIALS` bits), so withdrawal and the
  count stay exact. When the earliest remaining report was made while the
  retained reasons were full, the diagnostic names it as
  `<reason not retained>`. These bounds change no guest-observable behaviour
  (the same reports are made and withdrawn, and the same executions
  terminate), only the diagnostic text, so they apply to every scheduler
  profile (v34 to v38). Values are in the [limits table](#resource-limits).
- **Entry points.** Only `main` gets a checkpoint. Tool and HTTP contracts are
  rejected in every binary64 profile, so async tool handlers and HTTP adapters
  cannot be declared; a request-scoped isolate with its own checkpoint is the
  next step for HTTP.

#### Promise objects (`promises`)

`Promise<T>` is a built-in parametric type: an opaque handle (one `i64` slot,
no value tag) to a guest-heap record with private fields `handled`, `locked`,
`reason`, `serial`, `state`, `tracked` and `value`, interned per `T`. Like a
generator object it may be held in locals, parameters, results, closure cells
and frames (capture type `promise`) and compared by identity with `===`;
arrays, records, unions, `typeof`, logging, JSON and tool/HTTP signatures
reject it (`unsupported promise inside an array`, ...). `T` is any one-slot
value type or `void`; `Promise<Promise<T>>` and function, generator and
`VartaResult` values are rejected, and literal types erase to their base
primitive.

- **Resolution.** `new Promise<T>(executor)` (a type argument or a
  `Promise<T>` context is required) runs the executor synchronously with
  resolve and reject closures that share the promise's one-time `locked` flag;
  an exception from the executor rejects unless the promise already resolved.
  `resolve` takes `T` (optionally nothing for `Promise<void>`); `reject` takes
  any throwable value. `Promise.resolve(v)` returns an internal promise
  unchanged and otherwise creates a fulfilled one; `Promise.reject(e)` creates
  a rejected one (its type argument or context selects `T`, else `void`).
- **Reactions.** `then(f, r)`, `catch(r)` and `finally(f)` create the derived
  promise and start a synthesized job frame that awaits the receiver; when it
  resumes it calls the handler and settles the derived promise. A handler
  result fulfills, an exception rejects, and a returned promise is adopted. A
  missing handler (absent, `undefined` or `null`) passes the value or reason
  through. The derived type joins both handler results (`void` contributes
  `undefined`; results that do not form one checked union are rejected), and
  handlers that return promises must return the derived type. `finally`
  follows ECMAScript: the derived promise adopts
  `PromiseResolve(f()).then(() => value)` or
  `PromiseResolve(f()).then(() => { throw reason; })`, so a throwing or
  rejecting callback overrides the original outcome.
- **Adoption.** Resolving a promise with an internal promise queues an
  adoption job (NewPromiseResolveThenableJob) that awaits the source and
  settles the target alike: two extra jobs, as in Node. Resolving a promise
  with itself rejects it with
  `TypeError("Chaining cycle detected for promise #<Promise>")`. Arbitrary
  structural thenables, getters, subclassing and `Promise.withResolvers` are
  not admitted; neither are the combinators `Promise.all`, `allSettled`,
  `race` and `any`, which need promises inside arrays (a value tag) and fail
  by name.
- **Representation.** Field slots are read and written by ordinary record
  operations. The unset `value` and `reason` slots of a pending promise hold
  zero bits, which the collector skips and lowering never reads before the
  promise settled. Every frame that awaits a promise roots it. The settle,
  adopt and resolve helpers and the job frames are synthesized once per
  promise or handler type, for at most the compiler's `MAX_PROMISE_TYPES`
  promise types per module.

#### Async functions (`async_functions`)

- **Declarations.** Top-level `async function` declarations, exported or not,
  including generic ones (each instantiation is its own async function), with
  a required `Promise<T>` return annotation; `async function main():
  Promise<void>` is an entry point. The body returns `T`, or a `Promise<T>` to
  adopt. Async arrows, function expressions and methods, a `this` parameter,
  optional and rest parameters, `for await`, async generators and top-level
  `await` fail with construct-specific diagnostics (async iteration is K4.4
  step 6, admitted by v36 [async generators](#async-generators-async_generators)).
- **Split.** The declaration's own function index becomes a wrapper: it
  creates the promise, starts the body frame through the dispatcher (the body
  runs synchronously until its first `await`) and returns the promise, so
  calls and function values of the async function reach the wrapper. The body
  takes the promise as an extra parameter. Its statements run inside a
  labelled block whose `return` stores the value and leaves the block, so
  enclosing `finally` blocks run first; then the promise is fulfilled or
  adopts the returned promise, and an uncaught exception rejects it. A body
  without `await` is called directly.
- **`await`.** `await e` evaluates `e`, wraps a non-promise in a fulfilled
  promise (PromiseResolve: one job either way) and suspends. The suspension
  point marks the promise handled, withdraws its unhandled report and
  registers the frame's token as a reaction or queues it. On resumption the
  frame reads the settled promise from its saved local: a rejection is thrown
  at the `await`, so `try`/`catch`/`finally` work across it; a fulfillment is
  the value (`undefined` for `Promise<void>`). `await` appears in every
  normalized position of the
  [suspension foundation](#suspension-foundation-k44-block-1); inside an
  intrinsic value block (string concatenation, template literals, `String(...)`
  and other intrinsic calls) it fails as `unsupported await inside <construct>`.
  For write-aware refinements an `await` is an unknown reentry that may write
  captured mutable bindings.
- **Frames.** Async bodies and promise jobs are frame catalog rows with
  `generator: false`, an `Undefined` input for every state and no result
  binding; `varta_resume` reaches them. Their dispatcher branches trap if an
  exception escaped (the body catches every one) and report the outcome
  without issuing a generation. The scheduler sends only `Normal` completions
  with `undefined`.

### `checked-f64-utf16-v35` — Pilot batch: Web types, mixed records, async methods and frame liveness

V35 is registered and inherits every v34 feature and selector, including the
value15 layouts (`dynamic_values()`), the microtask scheduler and the frame
catalog. It declares two version identities of its own (see the
[version matrix](#version-matrix)), both introduced by the async stream's
[value layout 16](#value-layout-16-promise_values):

- **Host ABI** `abi::PROMISE_VALUES_VERSION`: the value16 readers are new
  imports. Every other v34 operation keeps its signature and binding.
- **Value ABI and layout** `value::PROMISE_VALUE_VERSION` (value16), value15
  plus the promise tag; `LanguageProfile::value_layout` equals `value_abi`.

An older runtime that does not list v35 rejects the unknown profile name in
`varta.language` validation, before compilation or any guest effect. As for
every profile, a v35 declaration with a downgraded or missing host ABI section,
a value ABI other than value16 or a missing value section fails validation
before instantiation.

Each ABI is bumped at most once per batch: the async stream owns both v35
bumps and the other streams rebase onto them, adding value16 variants of any
layout-bound operation they need. V34 and earlier profiles keep their host ABI,
value ABI and layouts.

Seven selectors gate the batch's work streams, each specified by a subsection
below:

- Web stream: `web_headers()` (an engine-provided typed `Headers` class,
  implemented in the guest and requiring no capability) and `abort_signals()`
  (the `AbortController`/`AbortSignal` data model, without timers); both admit
  the [engine Web types](#engine-web-types-web_headers-abort_signals) below;
- records stream: `mixed_index_records()` (interfaces and object types that
  combine declared fields with a string index signature, plus `for…in`,
  compound assignment and spread on dictionaries) and `unknown_json()`
  (`JSON.parse` and checked parsing that produce `unknown`, and narrowing
  `unknown` to dictionaries);
- async stream: `async_methods()` (async class methods, async arrow functions
  and async function expressions) and `promise_values()` (promises as tagged
  values in arrays, unions and `unknown`, and
  `Promise.all`/`allSettled`/`race`/`any`);
- frames stream: `frame_liveness()` (liveness-based continuation frame slots,
  an internal change of the frame layout of v35 modules; v34 and earlier frames
  keep their layout, and any frame catalog change the runtime reads is admitted
  for v35 only).

A v35 program that uses none of the selectors' constructs runs exactly like
the same program under v34, apart from its declared profile, value layout 16
and the [frame liveness](#frame-liveness-frame_liveness) layout below. The
streams compose on these layouts: mixed records, `unknown` JSON graphs and the
Web types read and write value16 through the value16 host readers and JSON
schema 16, promises may live in dictionaries and mixed records, and async
closure bodies use liveness frames (`varta-runtime` `v35_cross_stream_tests`,
`async_method_tests`). Production budgets of every profile are unchanged.

#### Value layout 16 (`promise_values`)

V35 stamps and reads the value16 layouts: record descriptors, record sizes,
presence trailers, closure and frame catalogs and exception cells carry version
16, and the runtime checks a v35 capture catalog against it. Value16 keeps every
value15 layout and tag with exactly its value15 meaning (dictionaries, wide
records and recursive graphs included; `value::dynamic_layout` names the
family) and adds one tag, `ValueKind::Promise` = 14, specified with
[promise values](#promise-values-promise_values). Value15 readers stay frozen:
they reject descriptors, catalogs and cells stamped 16 and the promise tag, and
the value16 readers reject 15.

V35 binds its own pure, capability-free operations to value16. Each is one
more row of the runtime's per-layout registration tables, mirroring its value15
counterpart's signature, result convention, budgets and recoverable errors
(`HostOperation::value15_counterpart`), and requires host ABI
`abi::PROMISE_VALUES_VERSION`:

| Value16 import | Value15 counterpart |
| --- | --- |
| `value_log_v10` | `value_log_v9` |
| `value_typeof_v9`, `value_truthy_v9`, `value_nullish_v9`, `value_strict_equal_v9`, `value_nullish_equal_v9`, `value_is_array_v9` | the `_v8` operations |
| `value_is_error_class_v5` | `value_is_error_class_v4` |
| `json_stringify_v15`, `json_parse_v17` | `json_stringify_v14`, `json_parse_v16` |
| `primitive_to_string_v4`, `primitive_to_number_v4`, `bigint_from_value_v3` | `primitive_to_string_v3`, `primitive_to_number_v3`, `bigint_from_value_v2` |

`LanguageProfile::host_operation` maps every value15 reader (and the value13
reader it replaced) to its value16 replacement for v35 only, so the runtime
rejects a v35 module importing a value15 reader and a v34 module importing a
value16 reader (whose minimum host ABI also exceeds v34's). `json_parse_v17`
reads **JSON schema 16** (`json::PROMISE_SCHEMA_VERSION`), the schema-15
grammar materializing value16 graphs; JSON never produces a promise. Thrown
values use **exception channel v7** (`varta.exception.version_v7` = 7): the
channel-v6 contents read as value16 cells, records and Error objects; a promise
is never thrown.

The switch changes no observable behaviour: `varta-runtime` `value16_tests`
compile the value15, unknown, generator and promise cases under v34 and v35 and
require identical output, and check that v35 imports only value16 readers and
exports channel v7, that swapped readers and channels are rejected before guest
effects and that a value15-stamped descriptor fails under v35.

#### Async methods and closures (`async_methods`)

V35 admits `async` on class instance methods (including overrides in
subclasses and `super.m()` calls), on async arrows and on async function
expressions, named ones included. V34 already compiled `static async` methods
as receiver-free async functions; v35 keeps that. Every such function has an
explicit `Promise<T>` result or one fixed by its context (a typed binding, a
parameter such as `() => Promise<T>`, or a `then`/`catch` handler). Without
either, an expression-bodied async arrow takes `T` from its value (a promise
value is adopted) and a block-bodied one infers `T` like other callbacks,
where a returned `Promise<U>` contributes `U`. Async functions, methods and
closures of v35 also admit optional, default and rest parameters: callers pad
and gather them as for synchronous calls, and defaults run in the body before
its first `await`, so an exception there rejects the promise. A `Promise<T>`
parameter pattern of a generic function now infers `T` from a promise
argument. Generic methods and generic classes stay rejected as for every
method; async generators, `for await` and top-level `await` stay rejected.

- **Split.** Each one is split like an async function declaration: the
  wrapper keeps the source function's index, receiver, self binding, captures
  and parameter shape, so calls, virtual dispatch, `super` calls and function
  values reach it; it creates the promise, starts the body frame and returns
  the promise. The body takes the wrapper's physical parameters (a method's
  `this` is its first, plain parameter) followed by the promise, and settles
  the promise on every completion.
- **Closure frames.** The body of an async closure is itself an environment
  function with the closure's captures. The wrapper forwards its own
  environment to it (the only new emission path, `Schedule` of an environment
  function), and the frame creator stores each captured binding cell in the
  frame as a shared slot. Captured bindings are therefore the very cells the
  enclosing function and other closures use: writes on either side of an
  `await` are seen by both, as in JavaScript. Frame slot allocation, the
  dispatcher and the frame catalog format are unchanged; the body frame is an
  ordinary scheduled frame (`generator: false`, `undefined` inputs).
- **Roots.** A method's receiver, an async closure's captured cells and every
  saved local are frame slots, rooted while the frame is suspended, queued or
  registered as a reaction; `varta-runtime` `async_method_tests` forces a
  collection before every job and checks the logs.
- **Budgets.** Async methods and closures draw on the one execution budget
  like every job: `await` never renews fuel, deadline or host calls, and
  exhaustion inside their jobs is fatal, never a rejection. A rejected promise
  nobody handles terminates the execution after the checkpoint.

#### Promise values (`promise_values`)

Under value layout 16 a promise is a tagged value (`ValueKind::Promise` = 14)
whose payload is the handle of its private record block. V35 therefore admits
`Promise<T>` as an array element, a record, interface or class field, a
dictionary value, a union member (at most one promise type per union, like
record shapes) and an `unknown` value. Promises compare by identity (`===`,
`includes`, `indexOf`; `Object.is` stays rejected); `typeof` is `"object"`;
they are truthy; a promise in a
union or `unknown` cell carries the promise tag, which the value16 readers
validate as a record block. `JSON.stringify` writes `{}` (a promise has no own
enumerable properties). `console.log` prints `Promise { <pending> }`,
`Promise { value }` or `Promise { <rejected> reason }`, with the settled value
in the notation of logged aggregates (a string prints `"x"` where Node prints
`'x'`). An element read `xs[i]` of a `Promise<T>[]` is `Promise<T> | undefined`,
as for every reference element. Checked JSON parsing, `Array.isArray`,
`instanceof` and checked assertions do not accept promise types, and earlier
profiles keep every v34 rejection.

- **Awaiting and resolving cells.** `await x` and `Promise.resolve(x)` of a
  union holding one promise member and one other member test the cell's tag:
  the promise member is awaited or returned itself (PromiseResolve), the other
  member through a fulfilled promise, so `await` takes exactly one job either
  way and yields the join of `T` and the other member. `Promise.resolve`
  requires the other member to be a `T`. A union with several non-promise
  members must be narrowed first. `Promise<T>` whose `T` may hold a promise is
  rejected (a promise never fulfills with a promise).
- **Unknown.** A promise held by `unknown` has no static `T`. Awaiting it,
  `Promise.resolve` of it and fulfilling a `Promise<unknown>` with it throw a
  recoverable `TypeError("a promise held by unknown cannot be adopted; keep it
  typed as Promise<T>")` instead of silently keeping the promise as a value.
  This is a documented deviation: JavaScript would adopt it.
- **Executor resolve.** `resolve` of `new Promise<T>` keeps its v34 type
  `(value: T) => void`, so resolver values stored in typed bindings keep
  working; resolving with a promise is a type error (`Promise.resolve(p)` or
  returning `p` from a handler or async function adopts it).
- **Combinators.** `Promise.all`, `allSettled`, `race` and `any` take one
  array of `Promise<T>` (an array literal of promises of one type, or a
  `Promise<T>[]` value; iterables, tuples of different types and non-promise
  elements are rejected). The call snapshots the array, creates the result
  promise and starts one synthesized job frame per element in index order;
  each job awaits its element, which registers one reaction as the
  combinator's `then` does in ECMAScript, and updates a private state record
  (`items`, `remaining`, `result`), so job order matches Node. `all` fulfills
  with `T[]` when the last element fulfills and rejects with the first
  rejection; `race` settles like the first element to settle; `any` fulfills
  with the first fulfillment and, when every element rejected, rejects with a
  native `Error("All promises were rejected")` — Varta has no `AggregateError`
  class (documented deviation: no `errors` property, `instanceof
  AggregateError` is unavailable); `allSettled` fulfills with records
  `{ status: string; value?: T; reason?: unknown }` whose present fields match
  Node's results (`value` absent when rejected, `reason` absent when
  fulfilled), so reading `r.value` is typed `T | undefined`. Over an empty
  array `all` and `allSettled` fulfill at once with `[]`, `any` rejects at once
  and `race` stays pending. `Promise.withResolvers` is not admitted: its result
  holds functions in record fields, which records do not admit.
- **Roots and budgets.** Promise slots of arrays, records, dictionaries and
  cells are managed edges like any reference; the combinators' state records
  and every element job frame root the promises until they settle.
  `varta-runtime` `promise_value_tests` forces a collection before every job.
  The combinators add no host import and draw on the one execution budget;
  each element costs one live frame and one reaction, within the scheduler
  bounds.

#### Frame liveness (`frame_liveness`)

Every frame-bearing v35 module (generators, async bodies, promise jobs and
the test hook's frames) uses liveness layouts instead of the cell layouts of
the [suspension foundation](#suspension-foundation-k44-block-1). No source
form, host import, value tag or ABI changes; guest-observable behaviour is
identical (the generator, promise and async suites run under both v33/v34 and
v35 against the same pinned Node logs). Earlier profiles keep the cell
layout, and their modules and catalogs are byte-identical to those produced
before this change (pinned by module digests in `varta-runtime`
`frame_liveness_tests`).

- **Saved sets.** After normalization and state numbering, a backward
  dataflow analysis over the structured body computes, for each suspension
  point, the locals that may be read after resumption before they are
  written: through the normal continuation, the exception continuation (a
  `Throw` completion is raised at the point, so the innermost handler's or
  finalizer's live-in is included) and the return continuation (a `Return`
  completion runs enclosing finalizers). Loops are iterated to a fixed
  point; `break`, `continue` and labelled branches reach their target's
  live-in, through every enclosing finalizer. The analysis only
  over-approximates: a statement-level `Init`/`Set` of an unshared local is
  the only kill; a write inside an expression is a use; every statement may
  throw; a finalizer may continue to any enclosing target and keeps its
  completion, exception and result locals live; a closure-shared local is
  never killed and every mention of it is a use (a statement-level `Set`
  included, because it writes through the existing cell), so its closure cell
  is always the saved binding. A local mentioned only inside one statement
  operand (call, value-block or helper scratch) is never live across a point,
  because operands are evaluated whole after normalization. The state
  machine adds the hidden locals its resumption path reads itself: the
  selected case of a suspending `switch` and the kept completion of a
  suspending finalizer, for the states they cover. State 0 saves the
  parameters the creator binds and, for an async closure body
  ([async methods](#async-methods-and-closures-async_methods)), its captured
  environment cells as shared entries.
- **Segments.** A liveness frame keeps the frame envelope and its segment
  table (kind 1) and holds saved values directly in segments of kind
  `FRAME_SLOT_SEGMENT_KIND` (3), at most 32 entries each and
  `MAX_FRAME_SEGMENTS` per frame. A state's saved locals occupy entries from
  zero in local order: one entry per one-slot value or closure cell, two for
  a two-slot value (its second entry is the `u32` status word). States reuse
  the same entries, so the frame holds the widest state, and no frame cell is
  allocated for an unshared value. Shared parameters still get a closure
  cell from the creator; captured environment cells are stored as they are,
  never copied, so the body and the closure share one binding.
- **Exact GC maps.** A segment's edge mask is the GC map of the state the
  frame is suspended at: the bits of the entries holding a managed reference
  (a handle or a closure cell). Saving stores the entries, then the masks;
  resumption validates and restores the target state's entries and clears
  the masks before the body runs, so a running or completed frame retains
  nothing through its segments (only its completion cell), and dead values of
  other states are never traced. Nothing allocates between storing entries and
  publishing or clearing masks. The creator publishes the state-0 map before
  it allocates any shared parameter's cell.
- **Restoration checks.** The resume body selects the target state's saved
  set with one `br_table`; any other state traps. Before using an entry it
  checks every segment the state touches: handle length, allocation bounds,
  flags, kind, entry count and an edge mask equal to that state's map. Saved
  Booleans must be 0 or 1, `null`/`undefined` 0, status words below 2^32, and
  a restored closure cell passes the binding-cell checks of the cell layout.
- **Budgets.** `MAX_FRAME_BINDINGS` bounds the entries of the widest state
  (parameters included) rather than every local of the function; the state,
  generator-type and live-frame budgets are unchanged.
- **Catalog version 3.** Liveness rows (`frame::FRAME_LIVENESS_CATALOG_VERSION`)
  add `capacity` (entries the segments hold, at most `MAX_FRAME_BINDINGS` and
  at least every state's entries) and, per state, `saved` (the saved slots in
  entry order, each a binding id and whether it is a shared cell); `slots`
  lists the creator's state-0 parameters and captured cells. `FunctionCatalog::from_liveness_bytes`
  reads versions 1 and 3 only, `from_frame_bytes` versions 1 and 2 only, and a
  version 2 row with liveness fields is invalid. Runtime preflight and
  artifact validation pick the reader by profile
  (`admission::frame_catalog_version`): v35 requires version 3 for frame
  layouts, v33 and v34 require version 2, and earlier profiles refuse both.
  A v35 module relabelled as v33/v34, or a v34 module relabelled as v35, fails
  with `FrameAdmissionError::FrameLayoutVersion`; relabelled as v32 it fails
  with `ProfileForbids(LivenessCatalog)`. Runtimes that predate liveness
  frames read v35 catalogs with the version 2 reader and reject version 3
  before any guest effect, so no host ABI bump is needed: the runtime never
  interprets frame memory, and the dispatcher, resume-body and creator
  signatures are unchanged.
- **Completed frames.** A completed frame leaves the live-frame table as
  before and, with its masks cleared, retains only its completion cell until
  it becomes unreachable. A scheduler-owned retention policy beyond that is
  still pending.
- **Measured costs.** Guest fuel under the production budgets, from
  `varta-runtime` `frame_liveness_tests` (marginal cost of ten more units;
  deterministic, not wall-clock):

  | Workload | v34 cell frames | v35 liveness frames |
  | --- | --- | --- |
  | `await i` in a loop, per `await` | 3002 | 1637 |
  | string loop with an `await`, per iteration | 6395 | 3005 |
  | `yield` in a `for...of` loop, per `yield` | 1805 | 1109 |
  | creating and suspending a generator holding two strings | 9634 | 4751 |
  | such generators suspended at once within 1M fuel | 57 | 141 |

#### Mixed index records (`mixed_index_records()`)

Selector `mixed_index_records()`. V35 admits object types that declare fields
next to one string index signature, such as the pilot's
`interface ProblemDocument { type?: string; status?: number; [key: string]: unknown }`:
type literals, interfaces (including merged declarations and `extends` of a
record, dictionary or mixed base) and aliases of them. It uses v35's
[value layout 16](#value-layout-16-promise_values) and JSON schema 16 and adds
no value tag, layout, host import, JSON schema version or budget of its own;
v34 and earlier keep
rejecting the shape (`index signature combined with declared fields (a mixed
object type)`).

- **Admission.** Declared members are property signatures (required or
  optional; readonly, computed and method members stay rejected). As in
  TypeScript (TS2411), every declared type — including the `undefined` of an
  optional field — must be assignable to the index type `T`. Merged and
  inherited index signatures must have the same `T` (a narrower one would need
  another storage). A declared field must be checkable from `T` (the tests of
  [checked assertions](#checked-assertions-checked_assertions), plus the tag of
  a member of a closed `T` union); for example `items: Item[]` next to
  `[key: string]: unknown` is rejected. Number and symbol index signatures and
  class index signatures stay rejected.
- **Representation.** A mixed record is a value16 dictionary of `T`
  (`ValueKind::Dictionary`): exactly the storage record, descriptor and layout
  of `Record<string, T>`. Declared fields and dynamic keys share one entry list
  in ECMAScript own-key order, so JSON stringify and checked parse,
  `Object.keys`/`values`/`entries`, `in`, `Object.hasOwn`, `delete` and
  collection behave exactly as for dictionaries. The declared fields are a
  compile-time view (`DictionaryType::declared`), never a layout.
- **Declared reads.** `p.status` (or `p["status"]`) looks the key up and
  converts the stored `T` to the declared type: free when `T` already is that
  type, otherwise the checked test of an assertion. Because a dynamic-key
  write, an alias typed as the plain dictionary or a `delete` may change the
  entry, a mismatch throws `TypeError: property <name> does not hold its
  declared type`, and an absent key reads `undefined` when the declared type
  admits it and otherwise throws `TypeError: declared property <name> is
  missing`. This is a deliberate deviation: TypeScript erases the types and
  JavaScript returns whatever is stored; Varta never reinterprets it. Other
  keys read `T | undefined` as for dictionaries.
- **Writes and deletes.** A declared key is written at its declared type and
  stored as `T`; other keys are written at `T`. `delete` admits optional
  declared fields and dynamic keys; a required declared field is rejected
  (TS2790). An object literal with a mixed context must provide every required
  declared field; values of declared keys are typed by their declarations.
- **Assignability.** A mixed record is assignable to the plain dictionary of
  the same `T` (a retag). A plain dictionary or another mixed record of the
  same `T` is assignable to a mixed record when TypeScript relates them: each
  required target field is declared required by the source, and each other
  target field accepts the source's declaration of that key or its index type.
- **Checked parsing.** `varta.json.parse<M>` and `JSON.parse(text) as M` parse
  the input as a dictionary of `T` with JSON schema 16 and then validate the
  root's declared fields: a missing required field or a value of another type
  throws the checked-schema `TypeError` (`JSON value does not match the checked
  schema`). Declared fields of a parse target must be parse-stable: primitive,
  literal or nullish members, members of a closed `T` union, or (under
  `unknown`) `Record<string, unknown>`. Mixed records nested inside a parse
  target are rejected.

The selector also completes dictionary operations:

- **`for (const key in value)`** over dictionaries (including mixed records
  and `object` views) and closed, non-class records: `const`/`let` and
  identifier assignment heads (no `var` or destructuring). The loop snapshots
  `Object.keys(value)` first, so keys added by the body are not visited (as in
  V8), and skips a snapshotted key the body deleted before its visit (as the
  specification requires). Arrays and class instances are rejected; iterate
  arrays with `for…of`.
- **Updates of entries.** Compound (`d[k] += v`, all arithmetic and bitwise
  operators), logical (`&&=`, `||=`, `??=`) and update (`d[k]++`, `--d[k]`)
  assignments evaluate the receiver and key once, read the entry before the
  right-hand side (`undefined` when absent, so `+=` stores `NaN` exactly as
  JavaScript does; a declared key uses the checked declared read) and store the
  result like an assignment, typed by the declared field or `T`.
- **Spread.** An object literal with a dictionary or mixed context accepts
  `...source` members for dictionaries and closed non-class records. It starts
  empty and applies its members in source order, as `CopyDataProperties` and
  `CreateDataProperty` do: a spread copies the source's own keys in its own-key
  order (a record's present fields in their run-time order), and a later write
  of an existing key keeps its position. Value types must relate as for
  assignment. Spreads without a dictionary context and spreads into closed
  records remain rejected.
- **Number keys.** A `number` key on a dictionary is converted exactly like
  `ToPropertyKey` (`String(n)`), so `d[1]` is `d["1"]` and `d[-0]` is
  `d["0"]`. Numeric index signatures stay rejected.
- **`Record<string, T>` inference.** A generic parameter typed
  `Record<string, T>` or `{ [key: string]: T }` infers `T` from a dictionary
  argument's value type or from the union of an object literal's (widened)
  field types; the literal is then copied into the instantiated dictionary in
  its key order.
- **Shared operations.** V34 inlines every dictionary read, write and delete,
  and a write is several kilobytes of Wasm. V35 lowers each of these operations
  once per dictionary value type into a synthesized function with the same body
  and calls it with the receiver, key and value evaluated in source order, so
  long runs of dictionary operations stay within the per-function body budget.
  V34 modules keep their inline bytes.
- **Class fields.** Optional class fields (`x?: T`) have type `T | undefined`
  and are defined as `undefined` when not initialized. A subclass may redeclare
  an inherited field, as the pilot's `readonly cause?: unknown` does: the slot
  is shared, so the redeclared type must equal the inherited type; a private
  field cannot be redeclared, accessibility may not become stricter and
  `readonly` may be added but not dropped. A redeclaration defines the field
  again after `super(...)` returns (ES2022 `[[Define]]` semantics), so
  `cause?: unknown` reads `undefined` as in Node, while `declare` makes it
  type-only. The redeclaring class's constructor may assign a readonly field.

#### JSON to unknown (`unknown_json()`)

Selector `unknown_json()`. V35 parses JSON into `unknown`, checks dictionary
assertions by their storage descriptor and narrows `unknown` to TypeScript's
`object`. It reads and writes v35's value layout 16 through the value16 host
readers and adds no value tag, layout, host import, JSON schema version or
budget of its own; v34 and earlier keep rejecting the global `JSON.parse` (`unsupported
call`), `varta.json.parse<unknown>` and `in` on `unknown`.

- **`JSON.parse(text)`.** The standard global (one string argument; a reviver
  is rejected) returns `unknown`. TypeScript declares it `any`, which Varta
  never admits; `unknown` keeps the program checked, so the result must be
  narrowed or asserted before use. The returned graph holds `null`, booleans,
  numbers (binary64), strings, `unknown[]` arrays and `Record<string, unknown>`
  dictionaries. The compiler describes it to JSON schema 16 as a reference
  union of `null`, `undefined` (for missing optional positions), boolean,
  number, string, an array of itself and a dictionary of itself, built from the
  existing schema nodes; the host decoder is unchanged.
- **Same grammar as checked parsing.** Malformed input throws `SyntaxError`.
  Duplicate keys and numbers outside the finite binary64 domain (`1e400`) are
  outside schema 16 and throw the checked-schema `TypeError`, where Node keeps
  the last duplicate or returns `Infinity`; inputs beyond the JSON depth,
  visit, entry or byte limits stay fatal, as for every checked parse. A
  `__proto__` key is an own key, as with `JSON.parse`.
- **`varta.json.parse<T>`** admits `unknown` anywhere in `T` (fields, array
  elements, dictionary values, the root).
- **`JSON.parse(text) as T`.** When `T` is a checked parse target, the
  assertion is the checked parse at `T`, which proves the whole graph (a record
  type, a mixed record, `JsonValue`, …) instead of only the root of an
  `unknown` graph; a mismatch throws the checked-schema `TypeError`. For any
  other `T` it is the assertion from `unknown`.
- **Dictionary assertions.** `x as Record<string, T>`, an index signature or a
  mixed record is tested by the `Dictionary` tag and the exact descriptor of
  the dictionary's storage record, for any `T`: storage shapes are interned by
  their value type, so the descriptor proves `T` exactly and a dictionary of
  another value type (or a record or instance of the same layout, which
  carries another tag) fails. This replaces v34's element-kind test, which
  only covered primitive `T` and could not tell `Record<string, string>` from a
  dictionary of string literals; v34 keeps its test. In v35 a member of a
  closed source union that is an array, dictionary, record or class is also
  tested by its tag alone.
- **Narrowing to `object`.** `typeof x === "object"` proves `object | null`
  for an `unknown` binding, and a following `x !== null` or truthiness test
  proves `object`; `x !== null && typeof x ===
  "object"` and `x && typeof x === "object"` work in either order.
  `Array.isArray(x)` and `instanceof` keep the fact. The fact describes the
  binding's `unknown` cell, which ordinary reads still see as `unknown`. Only
  the operations TypeScript admits on `object` use it: `key in x`,
  `Object.keys`/`values`/`entries`/`hasOwn` and `for…in`. Each converts the
  cell with the checked dictionary test above; every JSON object passes, while
  an array, record, class instance or Error held by the cell throws
  `TypeError: object held by unknown is not a Record<string, unknown>
  dictionary` (a deviation: JavaScript would enumerate it). `Object.values` and
  `entries` type their values `unknown` (TypeScript: `any`).
- **`in` guards.** A positive `"key" in x` with a string literal key (not an
  inherited `Object.prototype` name) refines `x` to an `object` view: a
  dictionary of `unknown` that declares `key: unknown`, so `x.key` reads
  `unknown` (`undefined` after a later delete). Further guards add keys; flow
  joins keep `object`. Undeclared property reads and dynamic keys on `object`
  stay rejected, because TypeScript rejects them or types them `any`.

#### Engine Web types (`web_headers`, `abort_signals`)

V35 provides `Headers` (with the `HeadersInit` alias), `AbortController`,
`AbortSignal` and `DOMException` as **engine preludes**: typed TypeScript
modules embedded in `varta-compiler` (`src/prelude/headers.ts`,
`src/prelude/abort.ts`) and compiled with the program through the shared module
graph, lowering and emission pipeline. They are ordinary nominal classes, so
`instanceof`, fields, parameters, unions, `unknown`, closures, generator and
async frames, the collector and the exception channel treat them like program
classes. They add no host import, value tag, layout or capability of their
own and run on v35's host ABI and value layout 16 like any program class. The
alternative, compiler
intrinsics for every member, was rejected: it would duplicate class layout,
dispatch, rooting and diagnostics for each type.

- **Loading.** `modules::engine` gives a module an implicit import of a
  prelude export when the module references that name as an identifier or type
  name (property names, object keys and strings never count) and neither
  declares nor imports it at the top level. `web_headers()` admits the
  `Headers`/`HeadersInit` prelude and `abort_signals()` the
  `AbortController`/`AbortSignal`/`DOMException` prelude; earlier profiles never
  load one and keep their existing diagnostics. A program that names none of
  these types loads no prelude, so its module is byte-identical to one compiled
  without preludes. Each prelude is a graph unit counted against the module and
  source budgets, and its functions and classes count against the
  user-function and record-shape limits like program declarations. Prelude paths
  (`<varta>/web/…`) are reserved: an import that a host loader resolves to one
  is a module error, and module metadata (`Compilation::modules`,
  `varta check --json`) lists only program sources.
- **Prelude intrinsics.** Header names and values are checked per UTF-16 code
  unit. `charCodeAt` is a host call, and the production host-call and fuel
  budgets would be exhausted after a few headers, so prelude modules (and only
  prelude modules) may call five guest helpers that lower to `ir::TextUnits`:
  `varta.web.indexAboveByte`, `indexNonToken`, `indexForbiddenValue` (the index
  of the first code unit above U+00FF, outside RFC 9110 `tchar`, or NUL/LF/CR,
  or -1), `asciiLowercase` and `trimHttpWhitespace` (TAB, LF, CR, SP). They read
  the immutable string in place, make no host call and allocate only their
  result. In program modules these names are ordinary unknown host calls.
- **Headers.** Names and values are WebIDL ByteStrings (`TypeError` "Cannot
  convert argument to a ByteString …" with Node's index and value); a name must
  be a non-empty HTTP token and a value, after stripping leading and trailing
  HTTP whitespace, may not contain NUL, LF or CR (`TypeError`
  `Headers.<method>: "<input>" is an invalid header name|value.`). Names are
  ASCII-lowercased. The list is a chain of entries kept sorted by name with
  equal names in insertion order; only the per-name order is observable.
  `get` joins a name's values with `", "` and returns `null` when absent;
  `set` replaces the first value and removes the rest; `getSetCookie`,
  `has`, `delete` and `append` follow the Fetch standard. The constructor
  accepts nothing, another `Headers`, `Record<string, string>` or `string[][]`
  and converts every entry before it fills the list, with Node's messages for
  pairs that are not of length 2 and for holes. Iteration (`for…of`, `entries`,
  `keys`, `values`, `forEach`) uses the sort-and-combine list, with each
  `set-cookie` value as its own entry. `forEach(callback)` rereads the list
  after the callback mutates it, like the WebIDL pair iterator.
- **Headers deviations.** `entries`, `keys` and `values` return fresh arrays of
  the current list rather than live iterators (no `next()`), and `for…of` over
  a `Headers` iterates the `entries` snapshot (always the `Headers`
  implementation, as `[Symbol.iterator]` would). Without tuple types a pair is
  `string[]`, so destructured names and values are `string | undefined`, and a
  `[string, string][]` annotation is rejected. Iterable inits other than
  arrays, `forEach`'s `thisArg` and `String(headers)` (`[object Object]`, Node:
  `[object Headers]`) differ. The header list is a guest chain bounded by
  memory and fuel, while iteration snapshots are arrays and share the array
  element limit.
- **AbortController and AbortSignal.** `new AbortController()` owns a fresh
  `signal`. `abort(reason?)` aborts once: an omitted or `undefined` reason
  becomes `new DOMException('This operation was aborted', 'AbortError')`, and
  later calls change nothing. `signal.aborted`, `signal.reason` (`unknown`,
  `undefined` before the abort) and `signal.throwIfAborted()` (throws the
  reason) follow the DOM standard, and `AbortSignal.abort(reason?)` returns an
  aborted signal. `DOMException` is an `Error` subclass with
  `constructor(message = '', name = 'Error')` and the legacy `code` getter; the
  default reason is therefore `instanceof Error` and `instanceof DOMException`,
  reports as `AbortError: "…"` when uncaught, and its `name` is writable (a
  read-only accessor in Node). `new AbortSignal()` is rejected at compile time
  (Node throws `TypeError: Illegal constructor`).
- **Pending members.** `addEventListener`, `removeEventListener`,
  `dispatchEvent` and `onabort` are rejected with an `unsupported abort event
  listeners` diagnostic: a signal would have to store listener functions, and
  value layout 16 has no function-valued object field (function values live
  only in locals, parameters, results, closure cells and frames). Admitting
  them needs a value-layout change, which v36 makes
  ([abort listeners](#abort-listeners-abort_signals-with-function_fields)).
  `AbortSignal.timeout` and `AbortSignal.any` are rejected as well: v35 has no
  timers and no listeners. V36 admits `AbortSignal.timeout` (see
  [Timers](#timers-timers)) and `AbortSignal.any` (see
  [abort listeners](#abort-listeners-abort_signals-with-function_fields)).
- **Value operations.** `JSON.stringify` of a value that may reach an engine
  Web type instance (including `unknown` in a program that loads a prelude) is
  rejected, because the instances keep private state where JavaScript has no
  own enumerable property. Private prelude members are compile-time private.
- **Callbacks.** Under `web_headers()` a function literal whose context is an
  exact function type may declare fewer parameters than that type, as in
  TypeScript (`headers.forEach((value, name) => …)`); the missing trailing
  parameters are synthesized as ignored bindings, so the literal has the
  context's exact type. Extra parameters, function values with fewer
  parameters and earlier profiles keep the exact-count rule.
- **Costs.** A typical request's header work (a record init, eight appends,
  `set`, `delete`, `get`, `has`, one `for…of` and one `forEach` over ten
  entries) stays under a fifth of the production fuel with one host call
  (`Array.isArray` distinguishes a record init from a sequence), a bound that
  `web_headers_tests` pins; `abort` makes one host call for its `undefined`
  reason check. Production budgets are unchanged.

### `checked-f64-utf16-v36` — Pilot batch: checked declared fields, function fields, async generators and timers

V36 inherits every v35 feature and selector, including the
promise tag of value layout 16 (`promise_values()`), the microtask scheduler
and frame catalog version 3 (`frame_liveness()`). Wherever a v35 subsection
above admits a feature, layout or binding for v35 only, it applies to v35 and
v36 (with the value17 readers below in place of the value16 ones); v34 and
earlier profiles stay frozen. V36 declares two version identities of its own
(see the [version matrix](#version-matrix)):

- **Host ABI** `abi::TIMERS_VERSION` (32), for v36 alone. Every v35 operation
  other than the value16 readers keeps its signature, binding and minimum ABI.
  The batch's host imports, the thirteen value17 readers
  ([value layout 17](#value-layout-17-function_fields)) and the four
  [timer operations](#timers-timers), all have host ABI 32 as their minimum;
  they are disjoint, and v35 binds none of them.
- **Value ABI and layout** `value::FUNCTION_FIELD_VALUE_VERSION` (value17), for
  v36 alone; `LanguageProfile::value_layout` equals `value_abi`, so
  descriptors, catalogs, frames and exception cells are stamped 17
  ([value layout 17](#value-layout-17-function_fields)). Every stream's
  records use it, including the async generators' state and request records,
  whose promise fields keep the value16 promise tag.

An older runtime that does not list v36 rejects the unknown profile name in
`varta.language` validation, before compilation or any guest effect. A v36
module relabelled as v35 fails before instantiation: at its value17 capture
catalog, record descriptors or value section, or at its host ABI section
(host ABI 32 against v35's 31) when it has none of those; as for every profile, a v36 declaration
with a downgraded or missing host ABI section, a value ABI other than value17
or a missing value section fails validation before any guest effect.

Each ABI is bumped at most once per batch. The host ABI bump was made at
registration and the value-layout bump by the function-fields stream; every
stream adds its imports under host ABI 32 and stamps value17. V35 and earlier
profiles keep their host ABI, value ABI and layouts.

#### Value layout 17 (`function_fields`)

V36 stamps and reads the value17 layouts: record descriptors, record sizes,
presence trailers, closure and frame catalogs and exception cells carry version
17, and the runtime checks a v36 capture catalog against it. Value17 keeps every
value16 layout and tag with exactly its value16 meaning (`value::dynamic_layout`
and `value::promise_layout` name the families that include it) and admits the
function tag `ValueKind::Function` = 12 as a reference kind of record and class
fields, array elements, dictionary values, reference-union and `unknown`
cells (`value::function_layout` is true for value14 and value17). The payload
is the packed handle of the existing v12+ closure (heap flag 128, 24 bytes,
optional environment edge); the guest collector traces it through the same
exact edge masks as every other reference. Value16 readers stay frozen: they
reject descriptors, catalogs and cells stamped 17 and the function tag, and the
value17 readers reject 16.

A value17 reader validates a function cell through its closure envelope
(`function::ClosureView::read_envelope`: block header, size and flag, the
signature/reserved words, the environment edge and every binding cell's
envelope). Captured payloads stay opaque and no reader invokes or decodes a
function, so unlike value14 (whose transport needs the owner capture catalog)
no catalog proof is required and no call authority is implied. Observations:
`typeof` is `"function"`, truthiness is `true`, nullish tests are `false`,
`Array.isArray` is `false`, strict equality is handle identity,
`JSON.stringify` treats a function like `undefined` (an omitted field or
dictionary entry, `null` in an array, an absent top-level result), a callable
`toJSON` field is a fatal error (the host cannot call guest code), and a logged
function shows as `[Function (anonymous)]` (Node prints the function's name
where it has one; closures carry no source name).

V36 binds its own pure, capability-free operations to value17. Each is one more
row of the runtime's per-layout registration tables, mirroring its value16
counterpart's signature, result convention, budgets and recoverable errors
(`HostOperation::value16_counterpart`), and requires host ABI
`abi::TIMERS_VERSION`:

| Value17 import | Value16 counterpart |
| --- | --- |
| `value_log_v11` | `value_log_v10` |
| `value_typeof_v10`, `value_truthy_v10`, `value_nullish_v10`, `value_strict_equal_v10`, `value_nullish_equal_v10`, `value_is_array_v10` | the `_v9` operations |
| `value_is_error_class_v6` | `value_is_error_class_v5` |
| `json_stringify_v16`, `json_parse_v18` | `json_stringify_v15`, `json_parse_v17` |
| `primitive_to_string_v5`, `primitive_to_number_v5`, `bigint_from_value_v4` | `primitive_to_string_v4`, `primitive_to_number_v4`, `bigint_from_value_v3` |

`LanguageProfile::host_operation` maps every value16 reader (and the value15
and value13 readers it replaced) to its value17 replacement for v36 only, so
the runtime rejects a v36 module importing a value16 reader and a v35 module
importing a value17 reader (whose minimum host ABI also exceeds v35's).
`json_parse_v18` reads **JSON schema 17** (`json::FUNCTION_FIELD_SCHEMA_VERSION`),
the schema-15 grammar materializing value17 graphs; JSON never produces a
function or a promise. Thrown values use **exception channel v8**
(`varta.exception.version_v8` = 8): the channel-v7 contents read as value17
cells, records and Error objects; a function is never thrown.

The switch changes no observable behaviour of a v35 program: `varta-runtime`
`value17_tests` compile the value15, unknown, generator, promise, async-method,
v35 cross-stream and Web cases under v35 and v36 and require identical output,
check that v36 imports only value17 readers and exports channel v8, that
swapped readers and channels are rejected before guest effects, that a
value16-stamped descriptor fails under v36, and that value16 readers reject a
function cell that value17 readers accept, while tampered closure envelopes
fail.
Four selectors gate the batch's work streams:

Four selectors gate the batch's work streams; `async_generators()` is
specified [below](#async-generators-async_generators) and the others admit
nothing yet:

- records stream: `checked_declared_fields()` (declared fields of mixed
  records that are checkable from `unknown`, for example arrays of optional
  records, so checked assertions and JSON narrowing reach them; see
  [checked declared fields](#checked-declared-fields-checked_declared_fields));
- functions stream: `function_fields()` (function-valued record and class
  fields, and optional or defaulted function-typed parameters);
- async stream: `async_generators()` (`async function*` generators and
  `for await…of`), specified [below](#async-generators-async_generators);
- timers stream: `timers()` (`setTimeout`/`clearTimeout` and timer-driven
  promises), specified in [Timers](#timers-timers).

All four streams are implemented; the subsections below specify them, and
the cross-stream combinations (timer callbacks and abort listeners held in
function fields, `for await` over generators that sleep on timers, declared
reads under optional chains) need no further rule. Production budgets of every
profile are unchanged.

#### Function fields (`function_fields`)

Under value layout 17 a function value (a v12+ closure, including a named
function reference, an arrow, a function expression and an async closure) is a
tagged value. V36 therefore admits a function type wherever a value type is
admitted:

- **Containers.** Record and interface fields (optional ones included), class
  fields with or without an initializer, array elements (`(() => void)[]`,
  `push`, `includes`/`indexOf` by identity, `map` over them), dictionary values
  (`Record<string, (x: number) => number>`), `unknown` cells and union members.
  A union holds at most one function type (as it holds at most one record
  shape or array type), so `(() => void) | undefined` and
  `((x: number) => number) | string` are admitted and
  `(() => void) | ((x: number) => void)` is rejected. Element and dictionary
  reads have the usual `T | undefined` type.
- **Calls through fields.** `object.field(args)` on a record or class-instance
  path whose field holds a function reads the field once and calls it without
  a receiver: a function field is never called as a method. A field whose
  function type declares a `this` parameter is rejected at such a call (the
  call would bind `this` in JavaScript); arrows capture their lexical `this`
  as before. A field that may be `undefined` (an optional field or a dictionary
  entry) must be narrowed or called with `?.()`; calling it directly is a type
  error naming both remedies.
- **Optional and defaulted parameters.** `cb?: (x: number) => void`,
  `fn: (() => void) | undefined` and `random: () => number = Math.random` are
  ordinary optional, union and defaulted parameters. A function literal whose
  context is a union with one function member takes that member as its
  contextual type and is boxed into the union.
- **Narrowing.** `if (f)`, `f !== undefined`, `f !== null` and
  `typeof f === "function"` narrow a union to its function member (and the
  negations to the rest); a function is always truthy. On `unknown`,
  `typeof x === "function"` tests the cell's tag but proves no callable type,
  and a checked `as` to a function type is rejected: the signature is a
  module-private fact that no runtime test of an `unknown` cell can check.
- **Built-in functions as values.** `Math.random` (unshadowed) is admitted as
  a `() => number` value: one synthesized wrapper with a canonical identity
  (`Math.random === Math.random`). It is the only built-in function value; any
  other built-in method read as a value stays rejected.
- **Identity, `typeof` and logging.** Strict equality of function values
  (also through unions and `unknown`) is reference identity (a fresh arrow evaluation is a fresh function),
  `typeof` is `"function"` and logging shows `[Function (anonymous)]`
  ([value layout 17](#value-layout-17-function_fields)).
- **JSON.** `JSON.stringify` drops function fields and dictionary entries,
  writes `null` for function elements and returns `undefined` for a function
  (its static result is `string | undefined` when a union may hold one). A
  value whose type may reach a record or class field named `toJSON` holding a
  function is rejected at compile time (the host serializer cannot call guest
  code); an `unknown` cell holding such a record fails at run time. Checked
  parsing cannot build a function: a parse target with a function field is
  rejected. `String(f)`, template substitution and `throw f` of a function stay
  rejected.
- **Optional chains.** `x?.p`, `x?.[i]`, `x?.m(args)`, `f?.(args)` and longer
  chains (`x?.a.b`, `x?.a?.b()`) evaluate the object of the first optional link once;
  when it is `undefined` or `null` the rest of the chain, including its
  arguments, is not evaluated and the result is `undefined`, otherwise the rest
  is the ordinary member read or call on the narrowed value. A truthy
  `x?.p` condition narrows `x` to its non-nullish members, as in TypeScript.
  Parentheses end a chain.
- **Rejection reasons.** A `reject(reason)` argument of type `unknown` (for
  example `signal.reason`) is checked like a thrown `unknown` value: a
  primitive, BigInt or Error value is admitted, anything else throws
  `TypeError`.
- **`Promise.withResolvers<T>()`** returns the record
  `{ promise, resolve, reject }` (in that own-key order) of a new pending
  promise and the same resolve/reject closures that `new Promise` passes to its
  executor; the type argument is required. Resolving with another promise
  (`resolve(promise)`) stays outside the subset, for the executor as for
  `withResolvers`. A promise of a function (`Promise<() => void>`, also as an
  async function result or an async generator's `TReturn`) stays rejected
  (`unsupported Promise value type function`); records with function fields
  may still be promised and yielded by async generators.

Physical representation, collection and safety are unchanged from v12
closures: a field, element or cell holds the closure's packed handle, its slot
is an exact edge of the containing record, vector or cell (record edge masks
and wide-record edge bitmaps, vector reference element kinds and the union
cell's reference flag), and the collector traces the closure's environment and
binding cells from there. Every call through a function value keeps the
checked closed dispatch (signature word, code index among the signature's
targets, environment presence); no indirect call or table is introduced and
host-free helper analysis is unaffected. Earlier profiles keep rejecting every
construct above with their own diagnostics and compile to unchanged bytes.

##### Abort listeners (`abort_signals` with `function_fields`)

V36 compiles its own abort prelude (the v35 prelude source with listeners;
v35 programs keep the v35 source and bytes). `AbortSignal` gains
`addEventListener(type, listener, options?)`,
`removeEventListener(type, listener, options?)` and an `onabort` field, where
a listener is `() => void` and `options` is `boolean` (capture) or
`{ once?: boolean; capture?: boolean }`:

- A listener is identified by its type, function identity and capture flag;
  adding it again changes nothing, removing it removes that entry only.
- When a controller aborts its signal (once), `onabort` runs first, then the
  `abort` listeners registered when the dispatch starts, in registration
  order: a `once` listener is removed before it runs, and a listener removed by
  an earlier listener is skipped. Listeners of other types never run.
  Listeners added to an aborted signal (including `AbortSignal.abort()`) never
  run.
- Deviations from Node: listeners receive no `Event` argument and `this` is
  not bound (DOM events are not modelled); `onabort` runs before the
  listeners, while Node runs it at the position of its first assignment among
  them; an exception thrown by a listener propagates out of `abort()`, while
  Node reports it as uncaught after the dispatch. `dispatchEvent` stays
  pending.
- `AbortSignal.any(signals)` (`signals: AbortSignal[]`) follows the DOM
  "create a dependent abort signal" steps that Node 24 implements: the result
  is already aborted with the reason of the first aborted signal in the list;
  otherwise it is a *dependent* signal of every listed signal that is not
  itself dependent, and of the sources of every listed dependent signal (so
  nesting `any` never chains dependents). When a source aborts, its dependent
  signals that are not aborted take the same reason first, then the source's
  `onabort` and listeners run, then each such dependent signal's, in the order
  the dependents were created. A dependent signal has no controller; sources
  keep their dependents reachable (Node holds them weakly, which is not
  observable without collection).
- `AbortSignal.timeout(ms)` ([timers](#timers-timers)) is a separate prelude
  module loaded only where it is called; it aborts through a controller of this
  prelude, so its signal works with listeners and `AbortSignal.any`. Programs
  that do not call it import no timer operation and need no grant.

#### Checked declared fields (`checked_declared_fields()`)

Selector `checked_declared_fields()`. V36 admits every declared field of a
mixed record indexed by `unknown` whose type is *convertible*, converts such
values for declared reads, checked parsing and checked assertions from
`unknown`, and completes the record operations the pilot's problem documents
use. It adds no value tag, layout, host import, JSON schema version or budget;
v35 and earlier keep their diagnostics (`declared property … is not checkable
from it` and the others listed below).

- **Convertible types.** Primitives, literal types, `null`, `undefined`,
  BigInt, native errors, class instances, `unknown`, and structural records,
  arrays (dense or resizable), dictionaries, mixed records and closed unions
  whose members are convertible, recursively (recursive types included).
  Functions, generators, promises and `object` views are not; TypeScript's
  TS2411 rule already rejects them next to `[key: string]: unknown`. Mixed
  records of another index type `T` keep v35's rule (a declared field is a
  member of `T`, proven by its tag).
- **Exact values.** A conversion first tests whether the stored `unknown` cell
  provably holds a value whose static type is the declared type; that value is
  then returned itself, so identity and mutation are those of JavaScript.
  Primitives, literals, errors and class instances are proven by their tag
  (and value set or class family); a structural record by its tag and any
  descriptor of its shape (a record literal keeps its source key order in its
  layout, so a shape may have several descriptors; v35 and earlier keep the
  canonical-descriptor test); a dictionary or mixed record by its storage
  descriptor. An array of primitives is proven by its element kind. An array
  of records, dictionaries or arrays is proven only when its element kind
  matches, it is non-empty, it has no holes and every element is exact. An
  empty array, or an array of union cells, does not record its element type
  (`Other[]`, `unknown[]` and `T[]` share the representation), and returning
  it as `T[]` would let a write through one alias break the other alias's
  element type, so it is never exact.
- **Built values.** Otherwise the cell is converted from its JSON-graph form
  into a fresh value: a `Record<string, unknown>` dictionary becomes a
  structural record (each key must be a declared field, as in checked
  parsing; values are converted to the field types and stored in the
  dictionary's key order, which JSON stringify preserves; required fields must
  be present) or a dictionary of the target value type (keys in order, values
  converted); an array whose element kind is a union cell or one of the
  target element's kinds becomes a fresh dense array of converted elements
  (exact elements are shared, holes read as `undefined`). A union target
  dispatches on the cell's tag to its array member or, for a dictionary, to
  its dictionary member and else its record member. A mixed record target is
  built as the dictionary of its index type; its declared fields stay a
  checked view. Anything else throws `TypeError(message)`.
- **Declared reads.** `p.errors` (and `p["errors"]`, destructuring and the
  update operators) looks the key up and converts the entry as above. A built
  value is written back under the same key (the entry keeps its position), so
  later declared reads return it and mutations through it persist. Failures
  throw `TypeError: property <name> does not hold its declared type`, an
  absent required field `TypeError: declared property <name> is missing`. Each
  declared type has one shared read function (`(d, key, wrong, missing) -> T`)
  so a function may read many declared fields within the body budget.
- **Checked parsing.** `varta.json.parse<M>` and `JSON.parse(text) as M`
  accept every convertible declared field of a root `M` indexed by `unknown`;
  after parsing, each such field is converted and written back, and a mismatch
  throws the checked-schema `TypeError` (`JSON value does not match the checked
  schema`). Mixed records nested inside a parse target remain rejected.
- **Checked assertions.** `x as T` for `x: unknown` and a target with a
  record, array or dictionary member converts as above (no write-back: the
  result is the exact value or a fresh one). `x as M` from a plain
  `Record<string, T>` to a mixed record `M` of the same storage, which
  TypeScript admits as comparable, is a free retag (declared reads are
  checked).
- **Writes, deletes, literals, spreads.** As in v35: declared writes and
  object-literal values are typed by the declaration and stored as `unknown`;
  `delete` admits optional declared fields; spreads copy entries. Class
  fields, getters and subclasses hold mixed records like any value.
- **Record completions.** Object binding patterns over dictionaries and mixed
  records (`const { error, message } = body`) read each key as `d[key]` does,
  declared keys through their checked read; rest elements stay rejected, and
  `object` views keep rejecting patterns. `typeof a.b === "string" ? a.b : x`
  (and `!==` with the path in the alternate) narrows a record or dictionary
  property path: the property is read once, tested by its tag and read
  narrowed (records and dictionaries have no accessors, so one read is
  indistinguishable from JavaScript's two). `a ?? []` and `a || {}` type the
  literal by `a`'s one array or object member. `let x: unknown;` starts as
  `undefined`. A value-returning function may end in a `switch` with a
  `default` clause when no clause breaks and the last clause does not fall off
  the end.
- **Costs.** An exact array of records costs a descriptor test per element on
  each read; a build is linear in the size of the converted value; both are
  ordinary guest work bounded by fuel and memory, and the JSON depth and
  entry limits bound parsed inputs. Each array, record and dictionary target
  synthesizes at most three functions (build, exactness, read), counted
  against the user-function limit.
- **Deviations from Node.** TypeScript erases these types, so Node returns
  whatever is stored and never throws at a read or `as`. Varta instead throws
  the `TypeError`s above for values that do not fit (including extra keys of a
  converted record, which checked parsing also rejects); two reads of an empty
  array (or another non-exact value) return different arrays; the first
  declared read of a JSON value replaces the entry, so a reference taken
  earlier through a plain `Record<string, unknown>` alias is no longer the
  stored value; and assertions of JSON graphs return fresh values.

#### Timers (`timers`)

K4.4 step 5. `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval` and
`AbortSignal.timeout` run on a runtime event loop that extends the
[microtask scheduler](#microtask-scheduler-promises) of the same execution.
V35 and earlier profiles keep rejecting the timer globals as unknown calls
(`unsupported call`), and `AbortSignal.timeout` keeps its v35 pending-member
diagnostic there.

- **Source subset.** `setTimeout(callback, ms?, ...args)` and
  `setInterval(callback, ms?, ...args)` are direct calls of the unshadowed
  globals; they return the timer id, a `number`. The callback is a
  contextually typed arrow or function expression (async ones included) or
  any typed function value without a `this` type; its fixed parameters must
  accept the extra arguments, parameters without an argument must be
  optional (so `resolve` of a `Promise<void>` qualifies), extra arguments
  beyond them are evaluated and ignored, and a rest parameter receives them.
  Each extra argument is a one-slot value. Its result is discarded. `ms` is a
  `number` or `undefined`. Arguments evaluate in source order. Type
  arguments, spread arguments, string callbacks and non-function callbacks
  are rejected. `clearTimeout(id?)`/`clearInterval(id?)` take at most one
  argument, converted with `Number` (so `undefined` and `null` are no-ops)
  and are interchangeable, as in Node and browsers. Under `timers()` an arrow
  expression body in a `void` context discards its value, as TypeScript
  allows, so `new Promise<void>((resolve) => setTimeout(resolve, ms))` is the
  sleep pattern.
- **Ids, not `Timeout` objects.** A timer id is a positive integer issued
  per execution, as in browsers and the DOM typings; Node returns a
  `Timeout` object (with `ref`, `unref`, `refresh`, `hasRef` and
  `[Symbol.toPrimitive]`). Numbers keep the value model unchanged (an id can
  live in records, arrays and unions) and are type-safe, since clearing never
  dereferences them: an id that names no armed timer of this execution,
  including one already fired or cleared, is ignored. No other execution's
  timers are reachable.
- **Frames.** A timer is a scheduled continuation frame of a synthesized
  function `(promise, callback, ...args)` that first awaits an
  engine-internal pending `Promise<void>` no source code can reach. Starting
  a timer creates the promise, starts the frame (it suspends at once, so its
  resume token becomes the promise's only reaction) and calls
  `timer_start` with the promise's reaction serial. When the timer fires or
  is cleared, the runtime queues that reaction without settling the promise
  record (its state stays pending, its unset value slot reads as
  `undefined`); the resumed frame calls `timer_take`, which answers whether
  the timer fired, runs the callback inside a `try` only if it did, and then
  completes (a timeout, a cleared timer) or awaits the same promise again (an
  interval). The frame therefore roots the callback, its captures and its
  arguments exactly while the timer is pending, through the live-frame table;
  the runtime keeps serials and ids, never a guest reference. An exception
  that escapes a callback reaches `timer_uncaught`, which renders it with the
  uncaught-exception readers and ends the execution with the same
  `UncaughtException` as one escaping `main`; later timers never run.
- **Imports.** Four host ABI 32 imports, matched by exact name and signature:
  `timer_start (serial:f64, delay:f64, flags:i32) -> f64` arms a timer and
  returns its id (flags `TIMER_INTERVAL` and `TIMER_UNREF`, any other bit is
  fatal); `timer_clear (id:f64) -> ()`; `timer_take (serial:f64) -> i32`;
  `timer_uncaught (reason:i64) -> ()`. `timer_start` is a host effect: it is
  charged to the host-call budget and requires the capability below. The other
  three are scheduler bookkeeping like the v34 scheduler imports: no
  capability, not charged to the host-call budget, deadline-checked. A serial
  that is not a pending reaction of exactly one token, a second timer on one
  serial, `timer_take` for a timer that neither fired nor was cleared and
  every bound below are fatal `SchedulerError`s.
- **Capability.** Arming a timer requires the exact boolean grant
  `time.timer` (`"time": { "timer": true }` in `varta.config.json`), denied by
  default and constrained by a host ceiling like every grant. Rationale: like
  `time.now` and `process.exit`, a timer is authority over the host rather
  than guest computation: it holds the execution (and its thread) for wall
  time that fuel does not bound, up to the deadline, which no ungranted
  program could do before v36. `time.now` does not imply it and it does not
  imply `time.now`; clearing needs no grant. The check precedes the timer's
  creation: without the grant `timer_start` ends the execution with
  `SchedulerError::TimerDenied` (`<capability denied: time.timer>`) before
  the timer exists, so no callback runs and nothing waits. A denial is fatal
  rather than a status because timer code has no result to check, and a
  silently unarmed timer would leave a pending promise that quietly ends the
  execution early. Inspection and `validate` list `timer_start` with
  `time.timer` through the shared registry, so a module's declared
  permissions are reviewable offline.
- **Event loop.** After `main` returns, the runtime repeats: run the
  microtask checkpoint to quiescence; apply the unhandled-rejection policy
  (so, as in Node, a rejection is fatal before the next timer even if a later
  timer would handle it); take the armed timer with the smallest
  (due time, sequence) and fire it; until no armed timer keeps the loop
  alive. Only one timer fires per turn, so each callback's microtasks run
  before the next timer, which is Node's order since Node 11. Due times use
  a loop clock in milliseconds that starts at 0 with the execution and
  advances only to the due time of the timer that fires; a timer started in
  a turn is due at the loop time plus its delay, and the sequence number
  orders equal due times by creation. Timer order therefore depends only on
  the program, never on how fast guest code ran, which keeps executions
  reproducible and the timer order free of a timing side channel. Before a
  timer fires the runtime sleeps until the wall clock reaches the execution
  start plus its due time, so a callback never runs early (it can run late if
  guest code was slow). Delays follow Node: a delay that is not a number in
  `1..=TIMER_MAX_DELAY_MS` (2^31 - 1, Node's `TIMEOUT_MAX`) is 1 ms (Node also
  prints a `TimeoutOverflowWarning` for too large delays), fractions are
  kept, and an interval re-arms at its fire time plus its delay. A cleared
  timer wakes its frame through the microtask queue; the frame completes
  without calling the callback, so the release is unobservable and finishes
  in the next checkpoint.
- **Deadline and budgets.** Fuel, memory, the host-call ceiling and the
  deadline `abi::DEADLINE_SECONDS` cover the whole execution, including every
  callback; waiting consumes no fuel, but callbacks do. A timer due at or after
  the deadline can never fire, so when it is the next timer the execution ends
  at once with `SchedulerError::TimerDeadline` ("execution deadline exceeded:
  the next timer is due at least N ms after the execution started, beyond the
  5 s deadline") instead of sleeping until the deadline; an interval that is
  never cleared ends this way too. If guest code already used the time, the
  wait stops at the deadline with "execution deadline exceeded". At most
  `MAX_PENDING_TIMERS` timers are pending (armed, fired and not yet taken,
  or cleared and not yet released) at once; each also holds one token
  counted against `MAX_PENDING_CONTINUATIONS` and one live frame, every
  firing is one job against `MAX_EXECUTION_JOBS`, and ids share the serial
  bound `MAX_PROMISE_SERIALS`. A loop that clears timers without reaching a
  checkpoint keeps their slots until the next checkpoint releases them.
  Production budgets are unchanged.
- **`AbortSignal.timeout(delay)`.** An engine prelude
  (`<varta>/web/abort_timeout.ts`, admitted by `timers()`) loaded only when a
  module names the static member `AbortSignal.timeout` (and does not bind
  `AbortSignal` itself); the call binds to the prelude function under the
  dotted name, which no source declaration can spell. It validates the delay
  like Node's `validateUint32` (a `RangeError` "The value of \"delay\" is out
  of range. …" for a non-integer or a value outside 0..2^32-1) and returns the
  signal of a fresh controller that a timeout aborts with
  `new DOMException('The operation was aborted due to timeout',
  'TimeoutError')`. The timer is unreferenced (`TIMER_UNREF`), as in Node: it
  fires while other work keeps the loop alive, and an execution whose only
  remaining timers are unreferenced ends without waiting (and without a
  deadline error). The prelude calls the prelude-only intrinsic
  `varta.web.unrefTimeout(callback, ms)`; in program modules that name is an
  ordinary unknown call. Programs that do not name `AbortSignal.timeout`
  load no timer code, so they need no `time.timer` grant. The signal runs its
  abort listeners and aborts its `AbortSignal.any` dependents like any other
  ([abort listeners](#abort-listeners-abort_signals-with-function_fields)).
- **Entry points and isolation.** Only `main` gets the event loop, so
  timers are tied to one execution: they never outlive its store, and a
  guest exit (`process.exit` inside a callback) or any fatal error ends the
  execution with its timers. Tool invocations and HTTP contracts are rejected
  in every binary64 profile, so a request handler cannot start a timer; a
  request-scoped isolate with its own loop (pending timers cancelled when the
  response ends) is the planned model when async HTTP lands. Packaged
  artifacts (`build --single`) and the `varta-runtime` companion run the same
  loop; the launcher's host-ceiling flag constrains `time.timer` like every
  grant.
- **Tests.** Runtime tests use a test-only virtual clock (compiled only under
  `cfg(test)`) that advances loop time without sleeping, so ordering, deadline
  and bound tests are fast and cannot flake; production code always waits for
  the wall clock, and one test checks that a production wait lasts at least
  the delay.

Until a stream adds its subsection here, a v36 module compiles and runs exactly
like a v35 module apart from its declared profile and host ABI, and production
budgets of every profile are unchanged. A v36 program that uses no construct of
an implemented selector compiles to the same module as before that selector
was implemented.

#### Async generators (`async_generators`)

V36 admits async generator functions and `for await…of` (K4.4 step 6) over the
frames, scheduler and promises of v34/v35. They add no host import, value tag,
layout, frame catalog version or budget: async generator bodies are generator
rows of frame catalog version 3 and their awaits are ordinary scheduled frames.
V35 and earlier profiles keep rejecting `async function*` (`unsupported async
generator function`), `for await` (`unsupported for await...of (async
iteration is not implemented yet)`) and the annotations below.

- **Declarations.** Top-level `async function*` declarations, exported or not,
  generic ones (each instantiation is its own async generator function) and
  instance and static async generator methods of classes (including overrides),
  with a required return annotation `AsyncGenerator<T, TReturn, TNext>`.
  `AsyncIterableIterator`, `AsyncIterable` and `AsyncIterator` with the same
  parameters denote the same type, because async generator objects are the
  only async iterables of the subset; generic calls infer `T`, `TReturn` and
  `TNext` from them. Omitted parameters default as for `Generator` (`TReturn`
  `void`, `TNext` `undefined`), and no type argument may be or hold a promise
  (`yield` and `return` await their operands). Optional, default and rest
  parameters are admitted; parameter defaults run during the call, before the
  object is returned, so an exception there throws from the call as in
  ECMAScript. Async generator function expressions and object methods,
  computed member names such as `[Symbol.asyncIterator]`, a `this` parameter
  and an async generator entry point are rejected with construct-specific
  diagnostics.
- **Representation.** An async generator object is the continuation frame of
  an internal *body generator*: one `i64` handle of type `generator` (capture
  type `generator`) without a value tag, so arrays, records, unions, JSON,
  logging, `typeof` and equality reject it as they reject synchronous
  generators. From the body's start point on, the frame's completion cell holds
  the object's private *state record* `{ head, queue, state }`: `state` is
  `[[AsyncGeneratorState]]` (suspended-start, suspended-yield, executing,
  draining-queue, completed) and `queue` an array of request records
  `{ kind, next, promise, returned, thrown }`. The step function of a body type
  does not clear that cell while the body runs — the only difference from a
  synchronous step function — so the protocol reads the state record at any
  time, including for a request made from inside the running body. The state
  and request records and the result promises are guest-heap records with
  ordinary value16 descriptors that no reader can reach. Each async generator
  type takes two of the module's `MAX_GENERATOR_TYPES` generator types (the
  object type and its body type).
- **Calls.** The declaration keeps its function index as a wrapper: it creates
  the state record (suspended-start, empty queue), creates the body frame with
  the arguments and the state record, steps it once through the parameter
  prologue to the start point and returns the frame.
- **Protocol.** `g.next(v)`, `g.return(v)` and `g.throw(e)` follow
  `AsyncGenerator.prototype.next`, `return` and `throw`: each creates a promise
  of `IteratorResult<T, TReturn>`, records a request (AsyncGeneratorEnqueue)
  and returns the promise. A suspended generator is resumed synchronously
  (AsyncGeneratorResume); requests made while it executes or drains wait in the
  queue. A completed generator answers `next` with `{ value: undefined, done:
  true }` and `throw(e)` with a rejection at once; `throw(e)` completes a
  not-started generator without running it; `return(v)` on a not-started or
  completed generator awaits `v` in one job (AsyncGeneratorAwaitReturn). Result
  records are created value first, as CreateIteratorResultObject does, so
  `JSON.stringify` prints Node's key order. Argument rules are those of
  synchronous generators: `next()` and `return()` without an argument need
  `TNext` and `TReturn` to accept `undefined`.
- **Body.** `await` has the async function semantics (a rejection is thrown at
  the `await`; `try`/`catch`/`finally` work across it), but the body frame is
  not a scheduled frame: its suspension point starts a *relay job*, a
  scheduled frame of a synthesized function per body type and promise type,
  that awaits the promise and then steps the body, which reads the settled
  promise from its own saved local. `yield e` awaits `e`, fulfills the front
  request with `{ value, done: false }` (AsyncGeneratorCompleteStep) and
  suspends unless another request is queued; the front request then decides
  the result of the `yield`: its value, a throw at the `yield`, or a `return`
  that awaits the value and runs enclosing `finally` blocks, which may `await`
  and `yield` again (AsyncGeneratorUnwrapYieldResumption). `return e` awaits
  `e`. When the body completes it completes the front request with the return
  value (or rejects it with an uncaught exception), drains the queue
  (AsyncGeneratorDrainQueue) and returns the state record, so no exception
  leaves the body. `yield` is admitted in the positions of synchronous
  generators, and a checked `x as T` assertion may contain `await` or `yield`
  in its operand under v36; other intrinsic value blocks keep rejecting
  suspension points by construct name.
- **`yield*`.** `yield* inner` delegates to an async generator whose `TNext`
  equals the delegating generator's and whose values and return values it can
  yield and return (the async kind of the ECMAScript `yield*` evaluation).
  Each received request is forwarded as `next(v)`, `throw(e)` or `return(v)`
  (a return request's value awaited first, as AsyncGeneratorUnwrapYieldResumption
  does) and the request's promise awaited; a value the delegate yields is
  passed on without another await (AsyncGeneratorYield); a done result ends
  the delegation with the delegate's return value as the value of `yield*`, or
  completes a forwarded `return` with it, running the delegating generator's
  `finally` blocks. Delegation to arrays, synchronous generators and other
  values is rejected (`unsupported yield* over a value other than an async
  generator`).
- **`for await`.** In async functions and async generator bodies, `for await
  (x of s)` over an async generator awaits `next()` for every step and, on
  `break`, `return`, a labelled exit or a throw from the body, closes it by
  awaiting `return()` (AsyncIteratorClose; a throw from the body keeps
  precedence and closing errors are then ignored). `TNext` must accept
  `undefined`; any `TReturn` closes, because return requests and the body's
  completion carry `TReturn | undefined`. Over a checked array or a
  synchronous generator it follows CreateAsyncFromSyncIterator: each value and
  the final done result take two jobs; a rejected value closes a synchronous
  generator in the first job and throws in the second; a throwing `next()`
  throws after one job; closing takes one job for an array (no `return`
  method) and, for a synchronous generator, a synchronous `return()` and two
  jobs (one when `return()` throws). Holes of sparse arrays are awaited as
  `undefined`. Other sources are rejected (`for await...of requires an async
  generator, a generator or a checked array`), and `for…of` over an async
  generator requires `for await`.
- **Scheduler, roots and budgets.** A generator suspended at its start point or
  a `yield` holds no live-frame slot; it is retained by its object and the
  frames that save it, and collected with them (`varta-runtime`
  `async_generator_tests` collects 256 abandoned generators with a collection
  forced before every job). Each pending `await` of a body holds its relay frame
  in the live-frame table until the promise settles, as an async function's
  `await` holds its frame, so the `MAX_LIVE_FRAMES` bound applies to pending
  awaits, not to suspended generators. Relay and return jobs are resumed only
  by the scheduler through `varta_resume`; the dispatcher branch of a body
  layout traps, as for synchronous generators. Every job draws on the one
  execution budget: exhaustion inside a body or a job is fatal and never a
  rejection, and a request promise rejected without a handler terminates the
  execution after the checkpoint like any unhandled rejection. Production
  budgets are unchanged. Measured with them, a `for await` step over an async
  generator costs about 10.0k fuel (the consumer's `await`, the `yield`'s
  `await` through a relay and the protocol records), an `await` in a body about
  3.3k (1.7k in an async function) and a created, started and abandoned
  generator about 12.7k.
- **Deviations.** No other object implements the async iteration protocol
  (there is no `Symbol`); an `AsyncIterable<T>` parameter accepts async
  generator objects whose `TReturn` and `TNext` match its own; `IteratorResult`
  values are narrowed by ordinary guards, not by `done`; and requests on a
  value that is not an async generator are compile-time errors rather than
  rejected promises. Behaviour tests pin Node 24 logs (`varta-runtime`
  `async_generator_cases`).

### `checked-f64-utf16-v37` — Pilot batch: language core, class generics, collections and binary text, RegExp and net.request

V37 inherits every v36 feature and selector, including value
layout 17 (`function_fields()`) with the value17 host readers, exception
channel v8, JSON schema 17 (`json_parse_v18`; serialization moves to
`json_stringify_v17`, [below](#batch-integration-v37)), the
microtask scheduler, frame catalog version 3 (`frame_liveness()`), async
generators and the timer operations with their `time.timer` grant. Wherever a
v36 subsection above admits a feature, layout or binding for v36 only, it
applies to v36 and v37; v35 and earlier profiles stay frozen. V37 declares one
version identity of its own (see the [version matrix](#version-matrix)):

- **Host ABI** `abi::NET_REQUEST_VERSION` (33), for v37 alone. Every host
  import added in this batch uses host ABI 33 as its minimum: the collections
  stream's `string_to_upper_case16`, `string_to_lower_case16` and `date_now`
  ([collections](#collections-and-binary-text-collections)), the six
  `net.request` operations behind `fetch`, `json_stringify_v17`
  ([batch integration](#batch-integration-v37)), and `timer_start_v2`, which v37
  binds instead of `timer_start` for its recoverable denial
  ([permission broker](#permission-broker-v37)). Every other v36 operation,
  including the value17 readers and the other timer operations, keeps its
  signature, binding and minimum ABI.
- **Value ABI and layout** `value::FUNCTION_FIELD_VALUE_VERSION` (value17),
  shared with v36. V37 adds no value tag or layout:
  `LanguageProfile::value_layout` equals `value_abi`, so descriptors,
  catalogs, frames and exception cells are stamped and read exactly as under
  v36.

An older runtime that does not list v37 rejects the unknown profile name in
`varta.language` validation, before compilation or any guest effect. A v37
module relabelled as v36 declares host ABI 33 against v36's 32 and fails at its
host ABI section before instantiation; as for every profile, a v37 declaration
with a downgraded or missing host ABI section, a value ABI other than value17
or a missing value section fails validation before any guest effect.

Each ABI is bumped at most once per batch. The host ABI bump is already made at
registration and every stream adds its imports under it. The collections
stream owned the batch's value-layout bump and needed none: its maps, sets,
byte arrays and builtin classes are private storage that no host reader sees,
and tuples are ordinary arrays. Value 18 (a new `value` constant,
`value_abi`, `value_layout`, value18 variants of every layout-bound reader with
their minimum host ABI 33, and the launcher maximum) is therefore not
introduced; a later stream that needs a value tag introduces it as its own
early foundation commit. V36 and earlier profiles keep their host ABI, value
ABI and layouts.

Five selectors gate the batch's work streams, and every stream is
implemented in its subsection below:

- language-core stream: `language_core()`, implemented below
  ([language core](#language-core-language_core)): TDZ-safe closure capture
  before declaration, deferred `let x: T;` initialization, dense `for…of`
  typing, property-path narrowing, `switch` on strings and function
  assignability fixes;
- class-generics stream: `class_generics()` (generic methods and classes,
  interface method signatures, `implements` and `#private` members;
  [implemented](#class-generics-class_generics));
- collections stream: `collections()` (`Map`/`Set`, tuples, `Array.join`,
  `String.fromCharCode`, a `Date` subset, `Uint8Array`,
  `TextEncoder`/`TextDecoder`, `crypto.getRandomValues` and base64;
  [implemented](#collections-and-binary-text-collections));
- RegExp stream: `regexp()` (`RegExp` and the regex string methods;
  [implemented](#regexp-regexp));
- net-request stream: `net_request()` (asynchronous `fetch`, `Request`,
  `Response`, `Headers`, `URL` and `ReadableStream` over the exact
  `net.request` capability, and `Symbol.asyncIterator` for async iterables;
  [implemented](#network-requests-net_request)).

The streams were developed in parallel and integrated together; the
[batch integration](#batch-integration-v37) subsection states the rules that
govern their combination. Production budgets of every profile are unchanged.

#### Batch integration (v37)

These rules hold for every v37 module and decide how the streams combine.
V36 and earlier keep their bytes and diagnostics.

- **Unreachable functions.** Every function and class member of a loaded
  engine prelude used to take a function slot. V37 lowering allocates against
  a compile-time linking cap of four times `abi::MAX_USER_FUNCTIONS`, then
  keeps only the functions reachable from `main`, the tool exports, the HTTP
  handler and the module-constant initializers through direct calls, function
  values, scheduled frames and async-generator relays (`lower::prune`),
  renumbers them densely in their original order and enforces
  `abi::MAX_USER_FUNCTIONS` on the result (`too many functions across
  modules: N reachable functions exceed the limit of 256`). Unused program
  functions take no slot either. The emitter derives dispatch targets, frame
  layouts and catalogs from the kept functions only. The runtime limit on the
  function section is unchanged.
- **Private state in JSON.** After lowering, v37 gives private state a
  descriptor key starting with `value::PRIVATE_KEY_PREFIX` (`#`): rewritten
  `#private` fields, every field of an engine prelude class instance, and the
  Error slots of class instances that are not own enumerable properties
  (`message` and `cause` always, `name` unless a constructor of a non-prelude
  class in the chain assigns `this.name` in a top-level statement, and none
  that a subclass field redeclares). V37 source cannot spell an object key
  with that prefix: object literals, type literals and interfaces reject
  such keys by name. `JSON.stringify` imports `json_stringify_v17` (host ABI
  33, same signature and contract as `json_stringify_v16`, no capability),
  which skips `#`-keyed fields while still charging their traversal. Prelude
  instances therefore serialize as `{}` (`Headers`, `AbortSignal`,
  `DOMException`, `Request`, `Response`), Error subclass instances as their
  own properties and `#private` classes without their private fields, as in
  Node, and v37 lifts the v35/v36 rejections of `JSON.stringify` for prelude,
  Error-subclass and `#private` instances and for `unknown` once such classes
  exist. One deviation remains: an assigned `name` keeps the Error slot's
  insertion position, so it precedes the subclass's own fields in the output.
  `URL` (whose JSON form is its `toJSON` string) and RegExp match arrays
  (arrays) stay rejected for `JSON.stringify` and are not assignable to
  `unknown`. `#private` instances stay outside `unknown`, because other host
  readers see descriptor keys. The exception channel reads a thrown
  instance's `#name`/`#message` when it has no own `name`/`message` key, so
  earlier profiles read exactly the fields they read before.
- **Index reads in `undefined` contexts.** The language core's dense reads
  (`xs[i]`, `s[i]`, `for…of` variables) check for holes, except where
  JavaScript's `undefined` is wanted: a read whose expected type has an
  `undefined` member (a `T | undefined` return, parameter or annotated
  binding), the left operand of `??`, an inferred `const`/`let` binding or a
  `for…of` variable whose name the enclosing block compares with `undefined`
  (`x === undefined`, `x != undefined`, `typeof x === 'undefined'`, either
  operand order), and every element of `unknown[]` (`unknown` holds
  `undefined` itself). Those reads produce `T | undefined` unchecked. A
  `Uint8Array` read out of range follows the same rule.
- **Unions of collections.** Records, classes and arrays may share a union
  (language core); a `Map`, `Set`, `Uint8Array` or tuple takes part only as
  the single member of its union tag, and `instanceof Map`/`Set`/`Uint8Array`
  is not modelled. Union members may hold collections in their fields and
  stay traced through every collection.
- **`instanceof` over several classes.** On a union of several classes,
  `x instanceof C` tests every member that is `C` or a subclass by
  descriptor, and narrows to all of them (or removes all of them); otherwise
  the nearest ancestor member is tested. `#x in v` uses the same test, also
  inside generic class instances.
- **Class fields.** A readonly field with a string, number or boolean literal
  initializer has that literal type, as TypeScript infers it, so it
  discriminates class unions; a field initialized by `new Map<K, V>()`,
  `new Set<T>()` or `new Uint8Array(n)` has that collection type.
- **`split`.** String `split` has one dispatch path: the separator is lowered
  once; a RegExp separator splits through the RegExp prelude, every other
  separator through the text prelude. Own methods and function fields named
  `split` dispatch as usual.
- **Prelude bindings.** A module that binds the name of a prelude value
  export locally (a parameter named `fetch`) keeps its own binding and does
  not import the global; prelude classes keep their load triggers.
- **`fetch` inputs.** `fetch` and `new Request` accept `string | URL |
  Request`; a `URL` contributes its `href`.
- **`implements AsyncIterable<T>`.** A class may implement `AsyncIterable<T>`
  when it declares `[Symbol.asyncIterator]()` returning exactly that async
  generator type.
- **Recoverable clock denials.** See the
  [permission broker](#permission-broker-v37): `Date.now()` and `new Date()`
  throw a `NotAllowedError` without the `time.now` grant.

#### Language core (`language_core`)

Selector `language_core()`. V37 admits the everyday TypeScript patterns below
with no value tag, layout, host import, JSON schema, frame catalog or budget
change; every v37 module of this stream imports exactly what the same v36
operations import. V36 and earlier keep their bytes and diagnostics (frozen
by digest in `varta-runtime` `language_core_tests`).

- **Captures before initialization (temporal dead zone).** A closure may
  capture a `let`/`const` binding (simple identifier declarators only) or a
  nested function declaration of its block before the binding's
  initialization runs. A syntactic scan of each block finds the bindings
  that a function body at or before their declaration position mentions
  (nested function declarations count as positioned at the block entry);
  only those reserve a Boolean flag binding. When a closure actually
  captures one before its initialization, the declaring block allocates the
  binding cell with the all-zero payload of its type and the cleared flag
  cell at its entry; the initialization stores into that cell and sets the
  flag, and the capturing closure captures both cells. Each read through such
  a closure, and each simple, compound, logical or update assignment through
  it, tests the flag first and throws an Error-class instance of the Error
  root with `name` `"ReferenceError"` and Node's message ``Cannot access 'x'
  before initialization``; a simple assignment evaluates its right side
  before the test. The zero payload is never observed. A program with such a
  block creates the Error root before any function body, so every catch
  union agrees. The binding needs a type at the capture: its annotation, a
  function initializer whose parameters are annotated and whose result is
  annotated or a block body without value returns (`void`), or a literal;
  otherwise the capture is rejected with `capture before initialization of a
  binding whose type is inferred from its initializer`. Direct reads in the
  declaring function before the initialization stay the compile-time error
  `binding used before initialization`, and destructured names, loop heads
  and catch parameters keep `capture before binding initialization`.
- **Nested function declarations.** `function f(...) {...}` in a block is a
  constant binding of that block, initialized at the block entry, in source
  order, by the equivalent function expression (named, for direct
  recursion, when it has a return annotation). Mutually recursive and
  forward-referencing declarations use the captures above. Generators,
  generic and ambient declarations keep the function-expression rules.
- **Deferred initialization.** `let x: T;` without an initializer, where `T`
  does not admit `undefined`, declares `x` with the zero payload of `T` and
  starts it *unassigned*. As in TypeScript's definite assignment analysis,
  every read, and every compound, logical or update assignment, requires an
  assignment on every path to it (`binding used before being assigned: x`).
  The unassigned set is flow state: branch and `switch` joins take the union
  of the reachable paths, a `catch` handler and a `finally` block start from
  the set at `try` entry, loops keep their entry set, and an assignment in a
  loop test counts for its body. A closure may capture `x` only where it is
  definitely assigned (JavaScript would read `undefined` through an earlier
  closure), and `x` may not be captured before its declaration.
- **Dense element typing.** `for (const x of xs)` over `T[]` binds `x: T`, and
  `xs[i]` and `s[i]` (strings) read `T` and `string`, as TypeScript types them
  without `noUncheckedIndexedAccess` (contexts that admit `undefined` read
  like JavaScript; see [batch integration](#batch-integration-v37)). The read
  is checked: a hole or an index
  out of range throws `TypeError` (`array element is missing (a hole or an
  index out of range)`, `array element is a hole` in `for...of`, `string
  index out of range`) where Node reads `undefined`; this is a documented
  deviation, so a value of type `T` is never missing. Element types that
  admit `undefined` read as before, without a test, and `at` keeps
  `T | undefined`. A read whose context admits `undefined` (an expected type
  with an `undefined` member, such as a `T | undefined` return, parameter or
  annotated binding, or the left operand of `??`) reads `T | undefined`
  unchecked, as Node does. Comparing a function value with `null` or `undefined` is the
  constant TypeScript admits (`f !== undefined` is `true`).
- **Property-path narrowing.** A guard on a property path (an identifier or
  `this`, then data fields of records and class instances; a getter is never
  a component) narrows later reads of the same path: `!== undefined`,
  `=== null`, `typeof`, truthiness, literal comparisons, `switch` clauses and a
  truthy optional chain `a.b?.c` (every object of the chain is not nullish).
  Facts are flow state; a join keeps a path only when every reachable path
  has a fact for it, with the union of the narrowed types. Records are
  mutable and aliased, so every fact is dropped by any construct that may
  write a property or run other code: calls other than the checked
  intrinsics that never reach user code or properties (`Object.*` and
  `JSON.stringify` drop facts too), `new`, `await`, `yield`, getters, tagged
  templates, assignments and updates to anything but a plain binding,
  `delete`, the entry of a loop whose test, update or body contains such a
  construct, every `for...of`/`for...in` entry, and `catch`/`finally` entries.
  A write to the root binding drops the facts rooted at it. A guard whose
  test contains such a construct creates no path fact, and facts never enter
  closures. A literal comparison of `x.kind` also narrows `x` itself when
  it is a union of record shapes (below).
- **`switch` by strict equality.** A discriminant that is not a number or
  Boolean (strings, literal unions, `typeof x`, unions) is evaluated once
  into a hidden binding; each clause value is compared with `===` in source
  order until one matches, `default` is selected when every test fails
  wherever it appears, and clauses fall through until `break`. A clause value
  whose type shares no member with the discriminant's is the type error
  `switch case type mismatch`. When the discriminant is an identifier, a
  member path or `typeof` of one and every clause value is a literal (so no
  test can change it), each clause is entered under the facts of
  `discriminant === value` (joined with the previous clause's fallthrough),
  later clauses and the end under the facts of every earlier test failing. Number and Boolean switches keep their
  lowering.
- **Function assignability to `void`.** A function value whose result is any
  value converts to a function type with the same receiverless fixed-arity
  parameters and a `void` result, where a value of that type is expected
  (arguments, annotated bindings, record and class fields, returns): the
  value is evaluated once into a hidden binding and wrapped in a synthesized
  closure that calls it and discards the result, so the closed call dispatch
  still sees exact signatures. Where a union is expected (an optional
  function field), the conversion targets its function member. An arrow or function expression with an
  annotated non-`void` result in a `void` context keeps its own type and is
  converted the same way. Mismatches between two function types name both
  signatures (`expected (p0: number) => void, found (p0: string) => void`).
- **Return analysis.** A loop without a test (or with the literal `true`)
  completes normally only through a `break` that targets it, so
  `for (;;) { ... return x; }` ends a value-returning function.
- **Class fields.** A field without an initializer whose type admits
  `undefined` starts as `undefined`, as TypeScript (with class fields
  defined) does; other fields must still be assigned in the constructor. A
  field initialized by `new C(...)` of a declared class without type
  arguments has type `C`. An arrow created in a constructor after
  `super(...)` may read the fields assigned before it and write any field
  through `this.field`; `this` itself still never escapes, and methods are
  still not called on it.
- **Unions of record shapes, classes and arrays.** A union may hold several
  records, interfaces and classes (all share the record tag of a union
  cell) and several arrays whose element kinds differ (`string[] |
  number[]`, `string | Item[]`; `A[] | B[]` of two record types is rejected
  because arrays are told apart by element kind). No value tag or layout
  changes: the emitted member test of such a union adds, after the tag, the
  payload's layout descriptor (any layout of the record shape, or of a class
  and its subclasses) or the array's element kind. Lowering adds field reads
  of a field every member declares (one read per member, with the union of
  the field types), method calls on unions of classes, `"key" in x`
  (`HasOwn` for records, declared members for classes), indexed reads of
  unions of arrays, object and array literals typed by the member their
  discriminant or first element selects, and narrowing: `x.kind === "lit"`
  (and `!==`, `switch`) keeps the members whose `kind` can hold the literal
  or drops those whose `kind` is exactly it, for bindings and property
  paths; `in`, `instanceof`, `typeof`, `Array.isArray` and truthiness filter
  members as before. Capture and frame catalogs describe such a union with
  the shallow `unknown` cell node, as for recursive unions. Promise and
  function members stay limited to one per union.
- **`never`.** A guard that excludes every remaining member of a binding
  (or a single record by its discriminant) leaves `never`; joins keep it
  only when every path proved it. `never` annotates bindings and parameters;
  a `never` value converts to any type, and a `never` binding's
  initialization is followed by a trap (the code is unreachable), so
  `default: { const unreachable: never = x; }` checks exhaustiveness.
- **Checked conversion of record unions.** `JSON.parse(text) as T`,
  `unknown` assertions and declared reads to a union with several record
  members select the member by a literal discriminant: each record member
  needs a required literal-typed field whose values no other member's same
  field admits (otherwise the conversion is rejected at compile time); the
  object's value of that field picks the member, which is then built, and an
  object no discriminant selects throws `TypeError` (Node keeps any parsed
  object). Host-schema checked parsing (`varta.json.parse<T>`) still rejects
  literal types.
- **Module variables.** Module-level `let` and `var` declarations (`var` is
  treated as `let`: initializers run before any other code, so hoisting is
  not observable) live in permanent root slots, a primitive boxed in a union
  cell so the slot holds a handle. Functions, closures and importing modules
  never copy them: every read loads the slot and every write stores it, so
  exports are live bindings; importers may not assign them. Simple,
  compound, logical and update assignments are admitted; destructuring and
  loop-head targets are rejected. Guard facts on a module variable are
  invalidated by any call, like those of mutable captured bindings. A `let`
  without an initializer needs a type that admits `undefined`. Initializers
  stay the checked pure subset of module constants, plus arrow and function
  expressions (module constants and variables may hold functions; their
  bodies run later). An initializer may not reference a later declaration;
  class expressions stay unsupported.
- **Self-referencing timers.** `const id = setInterval(() => clearInterval(id),
  ms)` (and `setTimeout`) captures `id` before its initialization: the timer
  handle's type is known from the call, so the capture uses the temporal
  dead zone above; `new C(...)` initializers of declared classes are typed
  the same way.
- **Timer handles.** `ReturnType<typeof setTimeout>` and
  `ReturnType<typeof setInterval>` (the built-in timers, unless a type named
  `ReturnType` is declared) are `number`, the handle type of these timers;
  other `ReturnType` queries stay unsupported.

#### Class generics (`class_generics`)

V37 admits generic methods, generic classes, method signatures in object
types, `implements` and ES private names. Everything is resolved at compile
time: no value tag, layout, host import, helper, catalog entry or budget is
added, an instantiation is indistinguishable from the hand-written specialized
method or class, and v36 and earlier profiles keep rejecting every construct
below with their diagnostics and compile to unchanged bytes.

- **Generic methods.** Instance, static, `async` and async generator methods
  (synchronous generator methods stay rejected for every class) may declare
  type parameters with constraints and defaults
  (`async request<T>(spec: RequestSpec): Promise<T>`). They follow the
  [generic function](#generics-generics) rules per call: explicit type
  arguments, or inference from the arguments (lowered once, in source order,
  after the receiver) and then from the contextual result type, so
  `return this.client.request(spec)` in a method declared
  `(): Promise<User>` instantiates `request<User>`. Calls through `this`, any
  class-typed receiver, `super.m<T>()` (a direct call) and `C.m<T>()` are
  admitted. Each distinct type-argument list is one method function of the
  declaring class with the class receiver, lowered in that class's module and
  member context, and counts against `abi::MAX_USER_FUNCTIONS`;
  instantiations requested from instance bodies nest at most
  `MAX_INSTANTIATION_DEPTH` deep. A subclass may override an inherited generic
  method with a generic method of the same accessibility and number of type
  parameters. An overridden generic method is called through one dispatcher
  per root class and type-argument list, synthesized after every body; every
  overriding implementation is instantiated with the same arguments and must
  then have the root's parameter, shape and result types (the override is
  checked per instantiation, not once generically as in TypeScript). A generic
  method may not override or be overridden by a non-generic member. Rejected:
  generic methods used as values, spread arguments to them, and type-parameter
  defaults or constraints of a method that name the class's own parameters.
- **Generic classes.** `class Page<T> { … }` is a template; every distinct
  argument list (`Page<number>`, from an annotation, `new Page<number>(…)`,
  `new Page(items)` inferred from the constructor's parameters, a contextual
  `const p: Page<number> = new Page(…)`, or `extends Page<T>`) is an ordinary
  nominal class with its own record shape, descriptor, initializer and
  methods, laid out in the declaring module with the arguments bound and
  memoized by (template, arguments). `Page<number>` and `Page<string>` are
  unrelated classes; instances are admitted wherever class instances are
  (fields, arrays, unions, `unknown`, `JSON.stringify`). A reference must
  supply every parameter without a default (`class P requires a type argument
  for T`). Generic classes may extend declared or generic classes
  (`class CursorPage<T> extends Page<T>`), and declared classes may extend
  instances (`class Numbers extends Page<number>`, which may override its
  methods). Static members belong to the template (`Page.empty()`); as in
  TypeScript they cannot name the class's type parameters. Generic methods of
  a generic class see the class's arguments. Because instances can be created
  while bodies are lowered, their member functions are allocated after every
  declared class and dispatcher and lowered into the generated function table;
  dispatchers are synthesized after the last body, so instance shapes created
  late join the descriptor tests of the methods they inherit. Members of an
  instance are checked when the instance is created; a template that is never
  instantiated is only checked for its static members. `value instanceof Page`
  tests the instantiation the static type holds (or descends from) and narrows
  like a class test. Rejected: overriding an inherited method in a generic
  class (its instances are created after dispatch is fixed), generic Error
  subclasses, `instanceof` of a generic class on a value not typed as one of
  its instantiations (including `unknown`: instantiations may still appear in
  later bodies), and inference without the class's own constructor.
- **Method signatures.** In an interface or object type, `m(params): R` and
  `m?(params): R` are the function fields `m: (params) => R` and
  `m?: ((params) => R) | undefined` of the [function fields](#function-fields-function_fields)
  rules, so `interface Body { text(): Promise<string>; close?(): void }` is
  satisfied by object literals, arrows and async functions, and
  `body.close?.()` works. A method signature needs a return annotation.
  Object literals admit method shorthand (`{ async text(): Promise<string> { … } }`),
  lowered as a function expression stored in the field. **`this` binding:**
  calls through a function field never pass a receiver (JavaScript passes the
  object), so an object-literal method whose body (or a nested arrow) uses
  `this` or `super` is rejected; methods that do not use `this` behave
  exactly as in JavaScript. Generic method signatures (`json<T>(): T`) and
  generic object-literal methods are rejected: a generic method has no
  function value. Class instances stay nominal: assigning one to an
  interface-typed binding remains a type error.
- **`implements`.** `class C implements I, J<T>` checks, when the class (or
  instance) is laid out, that every member of each clause, resolved like the
  annotation in the class's module, is provided by a public field (of an
  assignable type), getter (of an assignable result) or method (matching the
  function type: same shape and parameter count, interface arguments
  assignable to the method's parameters, result assignable to the
  interface's) of the class or its ancestors; optional members may be
  missing. It has no runtime effect and makes no instance assignable to the
  interface. Clauses that name classes, built-in iteration protocols
  (`AsyncIterable<T>`) or anything other than a closed object type are
  rejected.
- **`#private` members.** Instance fields (`#count = 0`, `readonly #fetch:
  Fetcher`), methods (`#send()`, `async #send()`, `#m<T>()`) and getters
  (`get #size()`) are admitted. Before layout every `#x` member and every
  `obj.#x` inside the class body becomes a `private` member named `#x`, so
  private names never collide with public members (`#count` and `count`
  coexist) and their scope is the class body, including nested closures,
  with the class-scoped `private` check (another instance of the class may be
  read: `other.#step`). A private field is an ordinary slot, initialized and
  checked by the constructor discipline like any field. `#x in value` tests
  the brand of the class whose body declares `#x` (exactly `value
  instanceof C`, so subclass instances pass) and narrows like it; its operand
  must have an object type (TypeScript rule; JavaScript throws for
  primitives). Private fields are not properties in JavaScript; v37's
  `json_stringify_v17` skips them ([batch integration](#batch-integration-v37)),
  while other host readers see every slot, so a value whose type may reach an
  instance of a class with private fields cannot become `unknown`. Rejected: static private members, private setters, a subclass
  redeclaring an inherited private name (in JavaScript a distinct member),
  quoted members named like private names (`'#x'`) and computed `obj['#x']`
  accesses of class members.

A class field initialized by `new C(…)` or `new C<A>(…)` without an annotation
(the pilot's `readonly #controller = new AbortController()`) takes that class
type; a generic class there needs explicit type arguments. Abstract classes and
methods stay rejected.

#### Collections and binary text (`collections()`)

V37 admits `Map`, `Set`, tuples, the string and array built-ins below,
`Uint8Array`, the UTF-8 `TextEncoder`/`TextDecoder`, `btoa`/`atob`,
`crypto.getRandomValues`/`crypto.randomUUID` and a `Date` subset. V36 and
earlier profiles keep rejecting every form with their earlier diagnostics and
keep their bytes: no v37 prelude loads there and no tuple or collection type
exists (`v36_modules_keep_their_bytes` pins the digests of v36 programs
recorded before this stream).

**No value tag or layout.** The stream adds no value tag, layout, JSON schema,
exception channel or launcher maximum: v37 keeps value ABI/layout 17. A map, a
set and a byte array are private guest storage, like v34 promises before
value16 tagged them. The compiler rejects, at compile time, every value that
may hold one where a host value reader would see it: `console.log` (directly
or through a union), `JSON.stringify` and conversion to `unknown`
(`unsupported logging/JSON.stringify/conversion to unknown of a value holding
a Map, Set, Uint8Array or another v37 builtin object`). Instances of the v37
builtin classes (`TextEncoder`, `TextDecoder`, `Date` and the preludes'
internal classes) are `opaque` the same way; unlike the v35 Web types they
never enter `unknown`, so they do not make `JSON.stringify` of `unknown`
conservative. A later value layout can tag them; nothing in layout 17 changes.

**Engine builtin preludes.** The implementation is typed guest code in five
reserved prelude modules compiled with the program through the shared module
pipeline: `<varta>/builtins/text.ts` (string and array methods),
`<varta>/builtins/collections.ts` (`Map`/`Set`), `<varta>/builtins/binary.ts`
(`Uint8Array`, text codecs, `crypto`), `<varta>/builtins/base64.ts` and
`<varta>/builtins/date.ts`. A prelude loads when any module of the program
names one of its exports or static members, or one of its load triggers
(`Prelude::loads`: `Map`, `Set`, `ReadonlyMap`, `ReadonlySet`, `Uint8Array`,
or a member property such as `.padStart`, `.split` or `.join`); a trigger
imports the module with no binding. The compiler calls prelude functions by
name for receivers of the matching static type (`lower::engine`), so a module
that only receives a set through another module's interface reaches them
without naming `Set`. Module metadata never lists a prelude.

Prelude modules alone may call these engine intrinsics (in program modules the
names are ordinary unknown calls). None reads a capability:

| Intrinsic | Lowering |
| --- | --- |
| `varta.text.unit(s, i)` | Guest read of one UTF-16 code unit (NaN outside the string) |
| `varta.text.fromUnits(units)` | Guest allocation of a string from at most `MAX_ARRAY_ELEMENTS` code units |
| `varta.text.indexNonAscii(s)`, `varta.text.asciiUppercase(s)` | Guest scans like the v35 `varta.web.*` intrinsics |
| `varta.text.toUpperCase(s)`, `varta.text.toLowerCase(s)` | Host operations `string_to_upper_case16`/`string_to_lower_case16` |
| `varta.text.identity(r)` | The stable address of a managed reference (the heap never moves) |
| `varta.array.get(a, i)` | A checked dense element read that traps outside the array instead of yielding `undefined` |
| `varta.collections.hash(k)`, `varta.collections.key(k)` | Selected by the key's static type (below) |
| `varta.bytes.view(b)`, `varta.bytes.adopt(a)` | Reinterpret a `Uint8Array` as its `number[]` storage and back |
| `varta.date.now()` | Host operation `date_now` |

**Host operations.** Three imports, all with minimum host ABI
`abi::NET_REQUEST_VERSION` (33), so a module that imports one fails host ABI
validation under any older host ABI before instantiation:

- `string_to_upper_case16(i32, i32) -> i32` and
  `string_to_lower_case16(i32, i32) -> i32`: pure and capability-free, one
  host call each; they read a bounded UTF-16 range and return the converted
  string through the result buffer (snapshotted at once). Default Unicode
  case conversion without locale, applied to every run of well-formed UTF-16
  as a whole (context rules such as the final sigma see the run); lone
  surrogates are copied unchanged. The result is bounded by the 1 MiB string
  limit. The text prelude converts ASCII-only text with guest scans and calls
  the host only for text above U+007F.
- `date_now() -> f64`: whole milliseconds since the Unix epoch. The
  [permission broker](#permission-broker-v37) checks the exact `time.now`
  grant before the clock is read, as it does for `varta.time.now`. A denial
  is recoverable, like every v37 permission denial: the runtime logs
  `<capability denied: time.now> (Date.now)` and returns NaN, which no clock
  reading produces, and the `Date` prelude throws a `NotAllowedError`
  `DOMException` from `Date.now()` or `new Date()`; the run continues.
  `time.now` never authorizes timers, and `time.timer` never authorizes the
  clock.

**Map and Set.** `Map<K, V>`, `Set<T>`, `ReadonlyMap<K, V>` and
`ReadonlySet<T>` (the readonly names denote the same types) are nominal types
(`Type::Collection`) whose storage is the prelude's own generic interfaces
`MapStore<K, V>` and `SetStore<T>`, instantiated through the generics
pipeline so the prelude source is the single layout definition:
`{ index: CollectionIndex; keys: K[]; values: V[] }` and
`{ index: CollectionIndex; keys: T[] }`, with `CollectionIndex =
{ buckets, chain, hashes, serials: number[]; size, serial: number }`. The
types are erased to that record before emission; storage fields are exact heap
edges, so keys, values and nested collections are traced like any record.

- *Keys.* `string`, `number`, `boolean`, `bigint` and every reference type
  (records, class instances, arrays, tuples, functions, collections) are
  admitted; a union or `unknown` key type is rejected (`Map key type …`).
  Literal types key as their base primitive. Keys compare by SameValueZero
  (`NaN` equals `NaN`, `-0` is stored as `+0`); references compare by
  identity.
- *Hashing.* A chained hash index with a power-of-two bucket count finds a
  key in expected constant time: strings hash by FNV-1a over their code units
  (cost linear in the key's length), numbers by value (every `NaN` and both
  zeros share a hash), booleans by value and references by their stable heap
  address. BigInt keys compare by value but share one chain (a documented
  linear cost, bounded by the entry ceiling).
- *Order and deletion.* Entries live in insertion order in parallel vectors.
  `delete` negates the entry's insertion serial; an insertion into full
  vectors first compacts the deleted entries away. A collection holds at most
  `MAX_ARRAY_ELEMENTS` (1024) entries, deleted ones included until
  compaction; inserting into a full collection of live entries throws
  `RangeError: Map maximum size exceeded` (`Set …`). V8's bound is 2^24.
- *Live iteration.* A cursor remembers the serial of the last visited entry
  and resumes after it (directly when no entry moved, by binary search after
  compaction), so iteration visits entries added during iteration, skips
  deleted ones, revisits a deleted and re-added key at its new position and
  continues after `clear()` (whose new entries get later serials), as in
  ECMAScript.
- *Operations.* `new Map()`/`new Set()` with type arguments or a contextual
  type (a binding annotation, a parameter, `left ?? new Set()`); `new
  Set(array)`, `new Map(entries)` with `[K, V][]` entries (an array literal of
  pair literals infers `K` and `V` like TypeScript) and copies of another
  collection of the same kind; `has`, `get`, `add`/`set` (returning the
  collection, so calls chain), `delete`, `clear`, `size`, `forEach` with a
  `(value, key)` callback, `keys`, `values` and `entries`, `for…of` over a
  map (`[key, value]` tuples) or a set, `[...collection]`,
  `[...collection.keys()]` and `Array.from` of either. `keys`, `values` and
  `entries` return the prelude's generators, so they iterate with `for…of`
  and spread, not as `Iterator` objects with helper methods. A `forEach`
  callback's third (collection) parameter, `thisArg`, the ES2025 set methods
  and `Map.groupBy` are rejected with their names. Stored module constants
  may be initialized with `new Set(…)`/`new Map(…)` of admitted values.

**Tuples.** `[A, B]`, `readonly [A, B]` and trailing optional elements
`[A, B?]` (`Type::Tuple`); rest elements and labels with rest are rejected. A
tuple is physically the dense array of the union of its element types (plus
`undefined` with optional elements), so logs, JSON, `unknown` and
`Array.isArray` see an ordinary array, and the element union must be
admissible (at most one record shape, array type and function type). A tuple
literal needs a tuple context (an annotation, a parameter or return type, an
element of a typed array of tuples, a union with one tuple member); without
one an array literal stays an array. A read at a literal index and
destructuring yield the position's own type through a checked union unbox; a
literal index past the tuple is a type error, and other indexes read the
physical element. `length`, `for…of`, `JSON.stringify` and the array methods
see the physical array; a tuple is assignable to its physical array type.
Element and property assignment through a tuple are rejected. Under v37,
`Object.entries` returns `[string, T][]`, physically the same pair arrays as
before.

**Strings and arrays.** `padStart`/`padEnd` (ToIntegerOrInfinity of the
length; the filler repeats and truncates), `toUpperCase`/`toLowerCase`
(above), `split` with an optional string separator and limit (ToUint32; a
RegExp separator belongs to the RegExp stream), `concat`, `codePointAt`,
`String.fromCharCode` (ToUint16) and `String.fromCodePoint` (`RangeError:
Invalid code point …`), and `Array.prototype.join` over primitive elements
(`undefined`, `null` and holes join as empty strings; object elements are
rejected because object-to-string conversion is not modelled). A class
instance receiver keeps ordinary method dispatch.

**Uint8Array.** A fixed-length dense `number[]` of byte values in eight-byte
slots, at most `MAX_ARRAY_ELEMENTS` (1024) elements; a larger length throws a
`RangeError`. `new Uint8Array()`, `new Uint8Array(length)` (`RangeError:
Invalid typed array length` for a negative or fractional length), `new
Uint8Array(numbers)` and `new Uint8Array(bytes)` (a copy); index reads like
array reads (`number | undefined` outside); index writes convert with ToUint8
and are ignored outside the array; `length`/`byteLength`; `set(source,
offset?)`, `slice`, `fill`, `indexOf`, `includes`, `at` and `join`;
`for…of` yields `number`; `[...bytes]` and `Array.from(bytes)` copy to
`number[]`. `subarray` (a view sharing the buffer), `ArrayBuffer`,
`DataView` and the other typed arrays are not modelled and are rejected.

**Text codecs and base64.** `TextEncoder.encode(input?)` writes UTF-8, a lone
surrogate as U+FFFD; output beyond the 1024-byte `Uint8Array` bound throws a
`RangeError`. `new TextDecoder(label?, { fatal?, ignoreBOM? })` accepts the
WHATWG UTF-8 labels only (`RangeError: The "…" encoding is not supported`
otherwise; Node with full ICU accepts more); `decode(bytes?, { stream? })`
runs the WHATWG UTF-8 decoder with maximal-subpart U+FFFD replacement or, when
`fatal`, `TypeError: The encoded data was not valid for encoding utf-8`;
streaming keeps a partial sequence across calls and a leading BOM is skipped
unless `ignoreBOM`. `btoa`/`atob` follow the HTML standard (forgiving base64
over Latin-1 code units) and throw a `DOMException` named
`InvalidCharacterError`; they load the abort prelude for `DOMException`.

**Random values.** `crypto.getRandomValues(bytes)` fills and returns the same
array from the `Math.random` entropy path (fresh OS entropy per sample, no
capability, the accepted decision): six bytes from each 48-bit slice of a
sample, so 16 bytes cost three host calls. `crypto.randomUUID()` formats 16
such bytes as an RFC 9562 version 4 UUID. `globalThis.crypto` and feature
detection through `typeof crypto.randomUUID` are not modelled.

**Date.** `Date` is an engine prelude class over one private time value
(UTC milliseconds, TimeClip). `new Date()` reads the clock (`time.now`), `new
Date(number)` clips and `new Date(string)` parses; `Date.now()` and
`Date.parse(text)` are static members that are also function values. There is
no time zone: `Date.parse` reads the ECMAScript date time string format
(including V8's lowercase `t`/`z`, a space separator, any number of fraction
digits and day overflow up to 31; a date-time without an offset is UTC, where
V8 uses local time) and the RFC 9110 IMF-fixdate, and returns NaN for every
other form (V8 also accepts implementation-defined legacy forms).
`getTime`, `setTime`, `toISOString` (`RangeError: Invalid time value`,
six-digit signed years outside 0–9999) and the `getUTC*` accessors exist;
local-time accessors, `toString`, `toJSON` and arithmetic through `valueOf`
are not modelled, and `JSON.stringify` of a `Date` is rejected (opaque).

**Costs.** Operations run in guest code under the unchanged production
budgets. In a debug measurement a `Set.add` of a number costs about 5 000
fuel and a `has` about 1 400 (the 1 000 000 fuel budget runs a few hundred
operations next to other work), string hashing adds work linear in the key,
and no operation makes a host call except case conversion of non-ASCII text,
`String(…)` conversions inside `join` of non-string elements, random values
and the clock.

#### RegExp (`regexp()`)

Selector `regexp()`. V37 admits regular expression literals, `new RegExp`
with constant arguments, `RegExp.prototype.exec`/`test`, `lastIndex`, match
arrays and the regex string methods `search`, `match`, `matchAll`, `replace`,
`replaceAll` and `split` (with RegExp and string patterns). It adds no value
tag, layout, host import, capability or budget: patterns are compiled ahead
of time into matcher programs, and a guest matcher runs them in time linear
in the input. V36 and earlier keep rejecting regex literals and these
methods with their existing diagnostics and bytes.

- **Design: ahead of time, guest only.** The compiler parses every pattern
  (`varta-compiler` `regexp`) and compiles it to a *matcher program*, a static
  string of UTF-16 code units in the data section (deduplicated like other
  literals). One guest helper, `RegExpExec`, runs any program: a Pike VM that
  steps all threads of a Thompson NFA in lockstep over the input and drops a
  thread that reaches an instruction another thread already reached at the
  same position in the same empty-check state. A run therefore executes at
  most `O(program × input)` instructions (times at most 16 empty-check
  states per instruction), all metered as ordinary guest fuel, and no pattern can
  backtrack catastrophically (ReDoS). Under the [thin-host decision](development-plan.md#thin-host-guest-intrinsics)
  the matcher is pure guest code: hosts implement nothing and every host
  (Wasmtime, a browser applier, Raspberry Pi 5) runs the same semantics. The
  alternatives were rejected: a host regex import would be reimplemented per
  host and drift from ECMAScript (and the host engines backtrack); a
  per-pattern Wasm matcher would multiply code size; a matcher written in TS
  prelude code costs about 100 fuel per guest loop step and would exhaust the
  production fuel on a few hundred characters.
- **Run-time patterns are rejected.** `new RegExp(pattern, flags)` is admitted
  only when both arguments are string literals (or templates without
  substitutions); it is compiled at compile time like a literal, with
  `source` escaped as V8's EscapeRegExpPattern (`/` and line terminators,
  `(?:)` for the empty pattern). A non-constant pattern, `new RegExp(regexp)`,
  `RegExp(...)` without `new`, `RegExp.escape` and Annex B `compile` are
  compile-time errors. Admitting them would need a guest pattern compiler run
  under its own budget, so that untrusted text could choose the program; the
  compile-time path keeps every program's size, scratch memory and step bound
  known and reviewed before deployment. This is a deliberate v37 decision, not
  a permanent limit.
- **Syntax.** The ECMAScript 2025 Pattern grammar (§22.2.1); without `u` the
  web-compatibility grammar of Annex B.1.2 (literal `]`, `{`, `}`, identity
  escapes, legacy octal escapes, `\c` forms, `\8`/`\9`, class ranges with class
  escapes). Early errors are compile-time `invalid` diagnostics with V8's text
  (`Invalid regular expression: /(/: Unterminated group`, `Invalid flags
  supplied to RegExp constructor 'gg'`); SWC itself reports duplicated literal
  flags. Flags `g i m s u y` are admitted.
- **Admitted subset.** Characters, `.`, classes and negated classes with
  ranges, `\d \D \s \S \w \W`, control/hex/Unicode escapes (`\u{…}` and
  surrogate pairs under `u`), `^ $` (with `m`), `\b \B`, greedy and lazy
  `* + ? {n} {n,} {n,m}`, alternation, capturing, non-capturing and named
  groups. Under `u` the matcher reads code points: a surrogate pair is one
  character for `.`, classes and literals, a lone surrogate matches itself,
  and a `lastIndex` inside a pair backs up to the pair as V8 does.
- **Rejected constructs** fail with `unsupported RegExp …: outside the
  checked-f64-utf16-v37 linear-time RegExp subset`, naming the construct:
  backreferences `\1` and `\k<name>` (they need backtracking), lookahead and
  lookbehind assertions, Unicode property escapes `\p{…}`/`\P{…}` (no property
  tables), pattern modifiers `(?ims-ims:…)`, duplicate named groups, escapes in
  group names, the `d` (`hasIndices`) and `v` (`unicodeSets`) flags,
  quantified groups that can match empty nested more than four deep, and
  programs larger than 16384 code units (counted repetitions are expanded, so
  `a{100000}` is rejected) or whose matcher scratch exceeds 256 KiB.
- **Matching semantics.** Leftmost-first priority equals ECMAScript's
  backtracking order: alternatives and greedy quantifiers prefer their first
  branch, lazy quantifiers the exit. Every iteration of a quantified group
  clears its captures first (RepeatMatcher step 4). An iteration that
  consumes nothing is rejected as RepeatMatcher's empty check does: every
  optional iteration (`*`, `+` after the first, `?`, `{n,m}` beyond `n`) of a
  body that can match empty records its start in a register and fails if it
  ends there (`CHECK`), so `/(a?)?/.exec('')` is `['', undefined]` as in Node.
  The visited set is keyed by instruction and by which enclosing empty-checked
  iterations started at the current position, which with the instruction
  decides every later check; threads in equal states have equal futures, so
  the earlier (higher-priority) one is kept, and a later iteration of an
  enclosing loop is explored before an inner lazy loop continues, as
  backtracking does (`/(.*?)+?!/.exec('ab!')` captures `b`). Programs without
  captures (see below) cannot observe empty iterations and need no checks. Case-insensitive matching is resolved at
  compile time: a character or class widens to every character with the same
  Canonicalize value (toUppercase per code unit without `u`, simple case
  folding with `u`, from the Rust standard library's Unicode tables; the
  Turkic dotted and dotless I fold to themselves), and `\w`/`\b` add U+017F and
  U+212A under `u`+`i`. The matcher compares characters exactly.
- **Programs.** A program is a header (flags, capture and register slots,
  instruction, thread and stack bounds, an ASCII first-unit bitmap, up to two
  direct first units, the closure index, the empty-check nesting), three-unit
  instructions whose first unit holds the opcode (`MATCH`, `CHAR`, `ANY`,
  `CLASS`, `JUMP`, `SPLIT`, `SAVE`, `RESET`, `CHECK`, `ASSERT`) and the
  instruction's empty-check depth,
  sorted class range tables and precomputed epsilon closures: for each
  instruction after a character, the compiler stores the threads it adds with
  their slot writes, so the matcher follows only closures that contain an
  assertion or empty check at run time. Each pattern has a second, *lean*
  program without group captures, empty checks or resets that yields the same
  match bounds; `test`, `search`, global `match`, `split` without groups and
  `replace` with a template that cannot reference a group use it. A pattern
  that cannot match empty skips input positions whose first code unit cannot
  start a match; a pattern that starts with `^` (without `m`) matches only at
  position 0.
- **Resources.** One run allocates one managed scratch block whose size the
  header fixes (at most 256 KiB, checked at compile time) and returns the
  capture slots as a string; the block is garbage afterwards and no
  allocation happens during matching. Inputs, results and replacements are
  ordinary strings under the existing per-value and memory limits. Measured
  with the production budgets (`regexp_tests`): a run costs about 3 000 fuel
  of fixed work (including the RegExp object of a literal), a search for one
  or two possible first units about 23 fuel per skipped unit and a class-led
  search about 35, while nested quantifiers such as `/^(a+)+$/` cost about
  1 450 fuel per unit (every live thread steps per unit). Doubling an
  adversarial input at most doubles its fuel; a 100 000-unit adversarial
  input exhausts the production fuel and fails the execution instead of
  hanging. Production budgets are unchanged.
- **Engine prelude.** `RegExp`, `RegExpExecArray` and `RegExpMatchArray` are
  classes of the engine prelude `<varta>/text/regexp.ts`, loaded like the
  [Web types](#engine-web-types-web_headers-abort_signals) when a module names
  one of them, contains a regex literal or has a member named `match`,
  `matchAll`, `replace`, `replaceAll`, `search` or `split` (a program's own
  methods of those names then load it too, without changing their meaning).
  A program that does none of these is byte-identical to one compiled without
  preludes. The prelude adds about 28 functions and three classes to the user
  function and record-shape budgets. Prelude modules (only) may call the guest
  operations `varta.regexp.exec(program, input, start, sticky)`,
  `varta.regexp.slot(captures, slot)` and `varta.regexp.unit(text, index)`
  (`ir::RegExpIntrinsic`); in program modules they are unknown calls. Every
  evaluation of a literal constructs a new RegExp through the private static
  `RegExp.compiled`, like ECMAScript.
- **RegExp objects.** `source`, `flags` (canonical order), `global`,
  `ignoreCase`, `multiline`, `dotAll`, `unicode`, `sticky`, `hasIndices` and
  `unicodeSets` are read-only fields; `lastIndex` is a writable number read
  with ToLength by global and sticky operations. `exec` follows
  RegExpBuiltinExec: global and sticky runs start at `lastIndex`, set it to the
  match end and reset it to 0 on failure; sticky runs match only at the start.
  `test` is `exec` without building the array. RegExps are nominal values in
  fields, unions, closures and `instanceof`.
- **Match arrays.** `exec` and `matchAll` produce `RegExpExecArray` (`index:
  number`, `input: string`), `match` produces `RegExpMatchArray` (`index` and
  `input` are `undefined` for a global match, which lists every match), and
  both have `length` and `groups` (a `Record<string, string | undefined>` in
  group order when the pattern has named groups, otherwise `undefined`).
  `m[0]` is a `string`; other elements are `string | undefined` (a group that
  did not take part, or an index past the end). Array destructuring and
  `for…of` read the elements. Match arrays are read-only and have no array
  methods (`slice`, `map`, … are rejected with a named diagnostic);
  `JSON.stringify` rejects them (their JSON form is an array) and they are
  not assignable to `unknown`.
- **String methods.** `search` runs from 0 and leaves `lastIndex` alone;
  `match` without `g` is `exec`, with `g` it collects every match string or
  returns `null`; `matchAll` requires `g` (`TypeError` as in Node), copies the
  RegExp with its `lastIndex` and returns the `RegExpExecArray`s as a fresh
  array; `replace`/`replaceAll` (which requires `g`) follow
  RegExp.prototype[@@replace] including empty-match advancement by code point
  under `u`, GetSubstitution for `$$ $& $\` $' $n $nn $<name>`, and replacer
  functions; `split` follows @@split (a sticky splitter resumed at every
  position, captures spliced in, the limit as ToUint32). A string, a pattern
  string or no separator selects the string forms of `split`, `replace` and
  `replaceAll`; `search`, `match` and `matchAll` compile a string literal
  argument as `new RegExp(text)` (`'g'` for `matchAll`). Arguments are
  evaluated once, receiver first.
- **Replacer functions.** A replacer (a function literal or a binding of a
  function type) is adapted at compile time to the prelude's
  `(match: RegExpExecArray) => string`: its parameters become constants read
  in the order ECMAScript passes them — the match, each capture, the
  position, the input and, with named groups, the groups object. A capture
  parameter is `string | undefined`, or `string` when annotated so, which
  asserts that the group took part (`TypeError` otherwise). The group count
  is known only for a pattern compiled in place (a literal or constant `new
  RegExp`); a RegExp value from elsewhere admits a replacer of the match
  alone. String-pattern replacers receive `(match, offset, input)`.
- **Deviations.** Besides the rejected constructs: case folding uses Rust's
  Unicode tables, not ICU; `matchAll` returns an array, not an iterator;
  match arrays are read-only without array methods; `split` throws
  `TypeError` for a capture that did not take part, because its result is
  typed `string[]` (JavaScript inserts `undefined`); `RegExp.prototype.toString`
  is not provided (it cannot be declared beside the inherited `toString`);
  `Symbol.match`/`replace`/`split` protocols, RegExp subclassing semantics,
  legacy static properties (`RegExp.$1`) and `exec` on objects other than
  strings are not modelled. Behaviour tests pin Node 24 output
  (`varta-runtime` `regexp_cases`, `regexp_tests`).

#### Network requests (`net_request`)

V37 adds asynchronous HTTP(S) requests behind the WHATWG `fetch` API, over one
exact capability, `net.request`, which replaces `net.get`/`net.fetch` for new
code (those keep their v1–v36 contracts and stay importable only by the
profiles that already had them). V36 and earlier profiles keep rejecting
`fetch`, `Request`, `Response`, `ReadableStream`, `URL` and `URLSearchParams`
as unknown names and keep their bytes and diagnostics.

- **Capability.** `net.request` is a URL-scoped grant with an optional method
  list. Project form: `"net": {"request": ["https://api.example.com/v1/**"]}`
  (every method) or `"net": {"request": {"urls": [...], "methods": ["GET",
  "POST"]}}`; manifest form: `{"id": "net.request", "patterns": [...],
  "methods": [...]}`, where `methods` is accepted for `net.request` only and
  absent means every method. Patterns follow the [network rules](#network)
  with `http://` and `https://`: exact scheme, normalized host and effective
  port, wildcards only after the authority, no credentials, fragments, control
  characters, whitespace or encoded paths, at most `net::MAX_URL_BYTES` per
  URL. Methods are RFC 9110 tokens matched exactly; the six methods `fetch`
  normalizes (`DELETE GET HEAD OPTIONS POST PUT`) must be written in uppercase,
  `CONNECT`, `TRACE` and `TRACK` are never grantable, duplicates are errors and
  at most `net::MAX_GRANT_METHODS` are listed. An empty URL list or an empty
  method list denies every request. With a host ceiling, the URL and method of
  every request must satisfy both sides; a ceiling never adds a request. The
  operation is distinct from `net.fetch`, `net.get` and `net.listen` (exact
  operation ids; none authorizes another). Inspection and `validate` list
  `net_request_start` with `net.request` through the shared registry.
- **Check before effect.** `net_request_start` parses the URL and asks the
  [permission broker](#permission-broker-v37) before anything is resolved or
  connected; transfers run only while the event loop waits, so no socket
  exists before the decision. A denial is recoverable: the request completes at
  once as denied, the runtime logs `varta log: <capability denied:
  net.request> (METHOD URL)`, and `fetch` rejects with a catchable
  `DOMException` whose `name` is `NotAllowedError` and whose message is that
  text. The program continues.
- **Async model.** The runtime holds request ids, completions and promise
  serials, never a guest reference. `fetch` starts a request and waits on
  `varta.net.watch(id, resolve)`, a prelude-only intrinsic that schedules a
  frame awaiting an engine-internal pending `Promise<void>` (exactly like a
  timer frame) and registers its one reaction with `net_request_watch`. Each
  armed operation (the response head, then one body chunk per `net_body_read`)
  completes once; its completion wakes the frame, which reads the kind
  (`net_request_take`, `net::CompletionKind`) and text (`net_request_text`).
  The waiting frame roots the callback, the promise and everything the caller
  captured while the request is in flight. After the microtask checkpoint the
  event loop fires a due timer first (Node's timers phase precedes its poll
  phase), otherwise drives the transfers until a completion arrives, the next
  timer is due or the deadline passes, and delivers one completion per turn
  (in completion order) before draining microtasks again. Loop time follows the
  wall clock after a completion, so later timers are due relative to it; an
  execution whose only timers are unreferenced stays alive while a request is
  in flight. A request in flight at the deadline ends the execution with
  `execution deadline exceeded`, like a timer.
- **Imports.** Six host ABI 33 imports: `net_request_start (ptr:i32,
  len:i32) -> f64` (the request descriptor below; capability `net.request`,
  charged to the host-call budget), and the bookkeeping operations
  `net_request_watch (serial:f64, id:f64) -> ()`, `net_request_take (id:f64)
  -> i32`, `net_request_text (id:f64) -> i32` (a string in the host result
  buffer), `net_body_read (id:f64) -> ()` and `net_request_abort (id:f64) ->
  ()`, which take no capability and are not charged to the host-call budget but
  observe the deadline. The descriptor is UTF-16 text of at most
  `net::MAX_REQUEST_BYTES`: the line `net::DESCRIPTOR_VERSION`, the method, the
  URL, the header count, each header name and value, the body flag, then the
  body as the rest (lone surrogates become U+FFFD; the body is sent as UTF-8).
  A malformed descriptor, an operation on an unknown or released id, a second
  waiter, a read while an operation is armed, and starting more than
  `net::MAX_PENDING_REQUESTS` requests are fatal `NetError`s, because only a
  tampered module can cause them.
- **Transport.** HTTP/1.1 through hyper's client on a current-thread Tokio
  runtime created by the first request; HTTPS through rustls (ring provider,
  TLS 1.2 and 1.3, ALPN `http/1.1`) verifying the server against the
  compiled-in Mozilla roots (`webpki-roots`), never an ambient trust store. One
  connection per request, no proxies, no cookies, no ambient credentials, no
  automatic redirects and no decompression. The runtime sets `host` and the
  framing headers; a request that sets one of `net::RUNTIME_HEADERS` fails.
  Name resolution, connect, TLS, the request and every body read share the
  remaining execution deadline. Response heads are bounded by
  `net::MAX_RESPONSE_HEADERS` fields and `net::MAX_RESPONSE_HEADER_BYTES`;
  request headers by `net::MAX_REQUEST_HEADERS` and
  `net::MAX_REQUEST_HEADER_BYTES`. A body chunk carries every frame already
  received up to `net::MAX_CHUNK_BYTES` without waiting for more (so SSE events
  arrive as soon as they are sent) and is decoded as UTF-8 across chunk
  boundaries (U+FFFD for invalid bytes, as a non-fatal `TextDecoder`). A body
  beyond `net::MAX_RESPONSE_BODY_BYTES` fails the read. Each body read costs
  guest fuel proportional to its text (about 2.6 fuel per byte measured), and
  a request head about 65 000 fuel, so under the production budget an
  execution makes roughly ten requests or reads a few hundred kilobytes;
  budgets are unchanged.
- **Guest API.** Engine preludes (`<varta>/web/fetch.ts`, `url.ts`,
  `permission.ts`), loaded only when a module names one of their exports, so
  other programs keep their bytes: `fetch(input: string | URL | Request, init?)`;
  `Request` (`method` normalized and validated like Node, `url`, `headers`,
  `signal`, `redirect`); `Response` (`status`, `statusText`, `ok`, `headers`,
  `url`, `redirected`, `type`, `body`, `bodyUsed`, `text()`, `json()` →
  `unknown`, and `new Response(body?, init?)` with a text or stream body);
  `ReadableStream` (`getReader()`, `cancel()`, `locked`, `values()` and
  `[Symbol.asyncIterator]`), `ReadableStreamDefaultReader` (`read()` in call
  order, `cancel()`, `releaseLock()`), a stream constructed from an
  `UnderlyingSource` with `start`, `pull` (returning `Promise<void>`) and
  `cancel` function fields and a controller with `enqueue`, `close` and
  `error`; `URL` and `URLSearchParams` (below). `RequestInit` has `method`,
  `headers` (`HeadersInit`), `body` (a string or `URLSearchParams`, which sets
  `text/plain;charset=UTF-8` or `application/x-www-form-urlencoded;
  charset=UTF-8` unless a content type is given), `signal` and `redirect`.
  Under `net_request` a class may declare `async *[Symbol.asyncIterator]()`
  (an async generator method without parameters, registered under a name no
  string key can spell), and `for await` over an instance of such a class
  calls it; other iterator objects are not modelled.
- **Cancellation.** A request follows its `signal` with an abort listener: an
  already aborted signal rejects with its reason before anything starts;
  aborting while the head is pending cancels the transfer (its connection
  closes at once) and rejects with the signal's reason; aborting during the
  body rejects the pending or next `read()` with the reason; `cancel()` on
  the body releases the request. `AbortSignal.timeout(ms)` therefore bounds a
  request, as in Node.
- **Redirects** default to `"error"`, as the accepted platform design
  requires (WHATWG and Node default to `"follow"`): a 3xx response with a
  `Location` rejects with `TypeError: fetch failed` (cause `unexpected
  redirect`). `"manual"` returns the 3xx response itself (as Node does, not an
  opaque response); `"follow"` issues a new request for the resolved location,
  checked against the grant again, changing `303` (and `301`/`302` after a
  `POST`) to a bodiless `GET`, at most 20 times; `redirected` and `url` report
  it.
- **Errors.** Name resolution, connection, TLS and protocol failures reject
  with `TypeError: fetch failed` whose `cause` is an `Error` with the
  runtime's reason; a failure while reading the body rejects the read with
  `TypeError: terminated` (Node's messages). An unparseable or relative URL
  rejects with `TypeError: fetch failed` (cause `Invalid URL: …`).
- **Deviations from Node and WHATWG.** Bodies are text: response chunks are
  decoded strings rather than `Uint8Array` bytes, `ReadableStream` is not
  generic (`ReadableStream<Uint8Array>` is rejected until generic classes and
  `Uint8Array` land in the class-generics and collections streams; the body
  type is the seam), and `read()` returns `{ done, value }` with `value` the
  empty string once `done`. `fetch` and `new Request` take a string, a `URL`
  (its `href`) or a `Request`, and `new Response` takes a string, a stream or
  `null`. Prelude instances serialize as `{}` and `JSON.stringify` of
  `unknown` is admitted ([batch integration](#batch-integration-v37)). There are no
  `clone()`, `blob()`, `arrayBuffer()`, `formData()`, `Response.json()`,
  `Response.error()`, `Response.redirect()`, `keepalive`, `mode`,
  `credentials`, `cache`, `referrer` or `integrity`; request headers are sent
  as given, without Node's default `accept`, `accept-language`, `user-agent`
  and `accept-encoding`.
- **URL subset** (pure guest code; `varta.web.unitAt` reads one code unit in
  place): the special schemes `http`, `https`, `ws` and `wss` (other schemes
  throw `TypeError: Invalid URL`), lowercased ASCII hosts (IPv6 literals kept
  as written; no IDNA, no IPv4 number normalization, forbidden host code
  points rejected), ports with defaults dropped, dot segments, backslashes as
  slashes, relative references (scheme-relative, path-absolute, path-relative,
  query-only, fragment-only) and the WHATWG path, query, fragment and userinfo
  percent-encode sets, with non-ASCII text encoded as UTF-8. `URL` exposes
  `href`, `origin`, `protocol`, `username`, `password`, `host`, `hostname`,
  `port`, `pathname`, `search`, `hash`, `searchParams` (live: its mutations
  rewrite the query), `toString()` and `URL.canParse`; setters are not
  admitted (class setters are not in the subset). `URLSearchParams` takes a
  query string, a `Record<string, string>` or `string[][]`, and offers
  `append`, `delete`, `get`, `getAll`, `has`, `set`, `forEach`, `size`,
  snapshot `entries`/`keys`/`values` and `toString()` with
  `application/x-www-form-urlencoded` serialization; decoding turns `+` into a
  space and decodes percent escapes of printable ASCII, keeping other escapes
  as written until a code-unit constructor exists.
- **Tests** never use the network beyond 127.0.0.1: a loopback server with an
  ephemeral port (HTTPS with a test-generated certificate and a test-only
  trust anchor compiled only into the runtime tests), with expectations
  produced by Node.js v24.20.0 against a mirror server.

#### Permission broker (v37)

V37 routes the capability decisions of the operations it adds or rebinds
(`net.request`, `time.timer` and the `time.now` clock) through one broker in
the runtime. Today it answers synchronously from the package's requests
intersected with the host ceiling, before the operation has any effect, and a
denial is recoverable rather than fatal:

- a denied request rejects `fetch` with a `NotAllowedError` `DOMException`
  (above);
- a denied timer start (`timer_start_v2`, the v37 form of `timer_start` with
  the same signature and minimum host ABI 33) arms nothing, releases the timer
  frame as if it were cleared, logs `varta log: <capability denied:
  time.timer> (timer)` and returns 0, and the guest throws the
  `NotAllowedError` of the permission prelude synchronously from `setTimeout`,
  `setInterval` or `AbortSignal.timeout`, so the sleep pattern
  `new Promise<void>((resolve) => setTimeout(resolve, ms))` rejects. V36 keeps
  its fatal `TimerDenied` (`timer_start` is frozen); v37 imports
  `timer_start_v2` only, so a v37 module relabelled as v36 fails at its host
  ABI section;
- a denied clock read (`date_now`, a v37-only import) logs `varta log:
  <capability denied: time.now> (Date.now)` and returns NaN, and the `Date`
  prelude throws a `NotAllowedError` from `Date.now()` and `new Date()`;
  `varta.time.now` keeps the failed host status (`denied`, a failed
  `VartaResult` when captured) that every profile reports.

Every other capability check a v37 program can reach already reports a
recoverable failed status: `env.*`, `fs.*`, `net.fetch`/`net.get`,
`process.exit` and the HTTP `stop` call set the host status, which
`varta.result.capture` reads. Two checks stay outside the broker by design:
`net.listen` is decided when the host starts an HTTP service, before any
guest code runs, and v36's `timer_start` keeps its frozen fatal denial (v37
never imports it).

The broker is the seam for a later consent prompt: a third answer, "pending
until the user decides", would leave the operation in its slot without an
effect (a request with no transfer, a timer not yet armed) while its frame
waits on its engine-internal promise through the scheduler; the user's answer
would then complete it as granted or denied. A user grant is checked against
the host ceiling like any grant, so a prompt can never exceed the ceiling.
No prompt or user interface exists yet.

### `checked-f64-utf16-v38` — Batch: program and data limits, turn budgets, linked intrinsics, view protocol and TSX, structural subtyping and codegen v2

V38 is registered and inherits every v37 feature and selector, including value
layout 17 (`function_fields()`) with the value17 host readers, exception
channel v8, JSON schema 17 (`json_stringify_v17`, `json_parse_v18`), the
microtask scheduler, frame catalog version 3 (`frame_liveness()`), async
generators, the timer operations with the recoverable `timer_start_v2`, the
`net.request` operations and the [permission broker](#permission-broker-v37)
with its recoverable denials. Wherever a v37 subsection above admits a
feature, layout, binding or diagnostic for v37 only, it applies to v37 and
v38; v36 and earlier profiles stay frozen. V38 declares one version identity
of its own (see the [version matrix](#version-matrix)):

- **Host ABI** `abi::PROGRAM_LIMITS_VERSION` (34), for v38 alone. Every
  other v37 operation, including the value17 readers and the timer and
  `net.request` operations, keeps its signature, binding and minimum ABI; the
  data-limits stream binds `json_stringify_owned` (minimum host ABI 34) in
  place of `json_stringify_v17` ([data limits](#data-limits-data_limits)).
  Host imports added in this batch use host ABI 34 as their minimum.
- **Value ABI and layout** `value::FUNCTION_FIELD_VALUE_VERSION` (value17),
  shared with v36 and v37. V38 adds no value tag or layout yet;
  `LanguageProfile::value_layout` equals `value_abi`, so descriptors,
  catalogs, frames and exception cells are stamped and read exactly as under
  v37.

An older runtime that does not list v38 rejects the unknown profile name in
`varta.language` validation, before compilation or any guest effect. A v38
module relabelled as v37 declares host ABI 34 against v37's 33 and fails at its
host ABI section before instantiation; as for every profile, a v38 declaration
with a downgraded or missing host ABI section, a value ABI other than value17
or a missing value section fails validation before any guest effect.

Each ABI is bumped at most once per batch. The host ABI bump is already made at
registration and every stream adds its imports under it. The program-limits
stream owns the batch's memory-layout and value-layout foundation: any value 18
(a new `value` constant, `value_abi`, `value_layout`, value18 variants of every
layout-bound reader with their minimum host ABI 34, and the launcher maximum)
or any change to the static-region ABI lands as an early, separate foundation
commit, and the other streams rebase onto it. V37 and earlier profiles keep
their host ABI, value ABI, static region and layouts.

Eight selectors gate the batch's work streams, and all admit nothing yet:

- program-limits stream: `program_limits()` (profile-dependent structural
  limits up to the Wasm limits, a shared descriptor-driven checked decoder,
  multi-page static data, table-driven resume dispatch and dead-code
  elimination);
- data-limits stream: `data_limits()` (arrays, dictionaries, `Map`, `Set`,
  `Uint8Array`, strings and JSON bounded only by the memory budget, and more
  than 64 live frames without a trap; [implemented](#data-limits-data_limits));
- turn-budgets stream: `turn_budgets()` (per-event-turn fuel and deadline
  configurable by the manifest within the host ceiling, a fuel profiler and
  logs before failure in `varta dev`);
- linked-intrinsics stream: `linked_intrinsics()` (Rust pure intrinsics linked
  into the guest Wasm so third-party hosts stay thin);
- view-protocol stream: `view_protocol()` (the L0 `view_commit` protocol,
  event delivery and a headless renderer);
- TSX stream: `tsx()` (TSX parsing and template compilation);
- structural-subtyping stream: `structural_subtyping()` (width and structural
  subtyping of records, interfaces and classes, plus the remaining pilot
  language items);
- codegen-v2 stream: `codegen_v2()` (cheaper code generation for loops, string
  comparison and concatenation, and dictionaries).

Until a stream adds its subsection here, a v38 module compiles and runs exactly
like a v37 module apart from its declared profile and host ABI, and production
budgets of every profile are unchanged.

#### Data limits (`data_limits()`)

Selector `data_limits()` lifts the fixed per-value ceilings of v37 and
earlier profiles, so the memory budget, fuel and the deadline bound data in
v38, as in a JavaScript engine. Every bound is read from one registry,
`varta_common::limits::DataLimits`, through `LanguageProfile::data()`:
`DataLimits::LEGACY` for every profile before v38 (its values are exactly the
frozen constants) and `DataLimits::V38`. Production budgets are unchanged:
8 MiB memory, 1M fuel, 1024 host calls, 5 s deadline. Modules of v37 and
earlier compile byte-identically (pinned by FNV digests of 14 v37 modules in
`varta-runtime` `data_limit_tests`, and by the earlier frozen-digest tests).

| Bound | v37 and earlier | v38 | What bounds v38 in practice |
| --- | --- | --- | --- |
| Elements of one array (dense, resizable, sparse), entries of one dictionary, `Map`, `Set`, `Uint8Array`, flattened spread/rest arguments | 1024 | `V38_MAX_ARRAY_ELEMENTS` = 2^27 | Memory budget |
| Bytes of one string value | 1 MiB | `V38_MAX_STRING_BYTES` = 2^30 (2^29 UTF-16 units) | Memory budget |
| One guest heap allocation | 1 MiB | `V38_MAX_ALLOCATION_BYTES` = 2^31 − 64 KiB | Memory budget |
| JSON nesting depth | 64 | `V38_MAX_JSON_DEPTH` = 10000 (host ceiling) | Host JSON stack |
| JSON values visited | 4096 | unbounded | Encoded size, so the memory budget |
| JSON parse input | 64 KiB | `json_input_bytes` = 2^30 | The guest string, so the memory budget |
| JSON output | 1 MiB result buffer | Memory budget (`json_stringify_owned`) | Memory budget |
| Live continuation frames | 64 (fixed root table) | Hashed table up to 2^27 entries | Memory budget |
| Pending resume tokens / timers (host) | 64 / 32 | 131072 / 65536 | Memory budget per 64-byte frame block |

The v38 element and string limits are Wasm-derived: the largest sparse
backing (`8 + 8·2^27 + 2^27/8` bytes) and string, with their heap header,
fit a signed i32, so every size the guest computes stays in i32 arithmetic.
Sizes are computed with checked or bounded arithmetic before allocation; an
index at or beyond the element limit traps as before (a deliberate deviation
from ECMAScript, where such a read is `undefined`).

**Sparse backings.** V38 keeps value ABI/layout 17. A sparse backing of up to
1024 slots keeps the value12 layout exactly (fixed 128-byte presence bitmap);
a backing above 1024 slots, which only a v38 module can build, carries one
presence bit per slot (`DataLimits::sparse_bitmap_bytes`), and
`DataLimits::sparse_capacity` inverts its packed byte length. The value
readers take the module's limits (`Vector::read_limited`,
`Dictionary::read_limited`, `ReferenceBox::read_limited`,
`PrimitiveBox::read_limited`, `ErrorObject::read_limited`,
`validate_aggregate_limited`); the existing readers keep the frozen limits,
so a v37 module that crafts a larger backing is still rejected. Runtimes
before v38 reject the v38 profile name, so they never see such a backing. A
future value18 (owned by the program-limits stream) must keep this bitmap
formula.

**Typed resource failures.** A v38 module with a managed heap exports two
mutable i32 globals, `resource_failure` and `resource_request`
(`limits::RESOURCE_FAILURE`, `RESOURCE_REQUEST`), validated as mutable i32
exports. Immediately before an `unreachable` trap the allocator (an oversized
request, the memory budget or failed `memory.grow`), the root stack (helper
pushes and function frames) and the live-frame table store the exhausted
`limits::Resource` (`Memory`, `ValueSize`, `RootStack`, `LiveFrames`) and the
requested amount. The runtime reports such a trap, and only such a trap, as
`RuntimeError::Resource(ResourceExhausted)`, for example `resource limit:
memory budget exhausted (requested 34078744)`. The globals select a
diagnostic and carry no authority. Resource failures are fatal: they end the
execution like any trap and are never delivered to the guest. Recoverable
v38 errors keep their v37 forms: `RangeError` for `Map maximum size
exceeded`, an invalid array or typed-array length, and the dictionary entry
limit, which v38 raises at the element limit. The host scheduler's
`ContinuationLimit` and `TimerLimit`, and the JSON depth and deadline errors,
are fatal, typed and reported.

**Heap growth and marking.** The v38 allocator grows linear memory by at
least half its committed pages when the budget allows, and by the exact need
otherwise, so collections run once per geometric growth step instead of on
every 64 KiB page. Marking uses a worklist of block headers in the unused
tail of the root region, above the live roots: rooted blocks are pushed, and
popping a block marks and pushes its unmarked children. Marking work is then
linear in the live edges instead of one heap scan per level of depth. A full
worklist records the overflow, and the collector finishes with the exact
fixed-point heap scans, draining the worklist after every scanned block, so
no reachable value is ever lost or truncated. The collector still allocates
nothing and stays nonmoving. Near the memory budget, where geometric growth
is no longer possible, collections again run on every page.

**Live frames and pending continuations.** A v38 module retains live
continuation frames in a growable hashed table instead of the fixed 64-slot
permanent table: one permanent root slot holds a private array backing (heap
flag 8, reference elements, no value tag; no value reader reaches it) used as
an open-addressing set keyed by the frame pointer (Fibonacci hashing, linear
probing, backward-shift deletion), doubling at half load through the
allocator, through one helper (`FrameTable`: register, find, release).
Registration roots the new frame on the root stack and keeps the old table
rooted while the table grows; the dispatcher finds a frame by hashing, and a
completed frame is looked up again before it is removed, because its body may
have grown the table. The collector traces the table like any reference
array. A table beyond the element limit is reported as `Resource::LiveFrames`
and failed growth as `Resource::Memory`. The host scheduler bounds pending
resume tokens by the memory budget per minimal frame block
(`limits::FRAME_BLOCK_BYTES`, 64 bytes): `V38_MAX_PENDING_CONTINUATIONS`
(131072) and `V38_MAX_PENDING_TIMERS` (65536) under the production budget.
Exceeding them is fatal (`SchedulerError::ContinuationLimit`, `TimerLimit`); a
correct module exhausts its memory budget first. Earlier profiles keep the
fixed 64-slot table, 64 tokens and 32 timers with `QueueFull` and
`TimerBudget`. Scheduler operations are not host calls.

**JSON.** V38 binds `json_stringify_owned` (host ABI 34, `(i32 kind, i32 low,
i32 high) -> (i64 handle, i32 error class)`, capability-free) in place of
`json_stringify_v17`. The host serializes under the module's data limits into
a buffer bounded by the memory budget, allocates the owned UTF-16 result
through the module's host-free `varta.value_abi.allocate`, writes it in 64 KiB
chunks, retains it through `varta.value_abi.retain` and returns its packed
handle (zero for `undefined`; the error class word is as for
`json_stringify_v17`); the 1 MiB result buffer is not used. Visited values are
bounded by the encoded size. Nesting is bounded by the host JSON depth ceiling
`limits::V38_MAX_JSON_DEPTH` (10000; Node 24 overflows its stack at 6182
nested arrays): values nested deeper than 64 are traversed on a dedicated
256 MiB JSON host stack (committed as touched), and deeper values fail with a
reported JSON depth error (stringify: "nesting exceeds the host depth
ceiling"; parse: `JsonInputLimit::Depth`). Checked parsing accepts input up to
`DataLimits::json_input_bytes`, arrays and dictionaries up to the element
limit, and stops at the execution deadline (`JsonInputLimit::Deadline`,
fatal); serde_json's `unbounded_depth` feature is enabled only for depths
above 64. V38 parse results are built top-down and iteratively: each container
is published into its reachable parent slot before its children, so one
retained root covers the whole value, the root stack never overflows and no
host recursion occurs. V37 and earlier keep `json_stringify_v17`, the 1 MiB
output, 64 KiB input, depth 64, 4096 values, the child-first builder and every
message.

**Collections and text built-ins.** The `Map`/`Set` and `Uint8Array`
preludes compile under v38 with the v38 element limit
(`Prelude::lifted`; v37 compiles the unchanged sources), and the v38
`Array.prototype.join` concatenates balanced halves, so joining n elements
copies O(n log n) code units instead of O(n²). `Map`/`Set` lookup stays
hashed; dictionaries and mixed records keep their linear key search and
ordered insertion (see the pending items below).

**Bounded host copies.** Value readers (logging, `typeof`, equality, unions,
JSON) read guest memory in place and allocate host memory bounded by the
module's memory. `json_stringify_owned` buffers at most the memory budget of
UTF-8 output and writes it into the guest in 64 KiB chunks (see the JSON
subsection); checked parsing copies its input, which is a guest string and
so bounded by the memory budget. The host scheduler keeps at most one entry
per 64-byte frame block of the memory budget. No v38 host path allocates
more than a constant multiple of the memory budget.

**Measured costs.** Guest fuel and final memory pages (64 KiB each; the
budget is 128) under the production 8 MiB memory budget and test-only fuel
and host-call budgets (`varta-runtime` `data_limit_tests`,
`frame_limit_tests`, `json_limit_tests`; deterministic, not wall-clock):

| Workload | 10k elements | 100k elements |
| --- | --- | --- |
| `number[]` built with `push` | 8.2M fuel, 27 pages | 103.6M fuel, 60 pages |
| `number[]` built by indexed writes | 2.5M fuel, 27 pages | 25.0M fuel, 40 pages |
| `string[]` joined (one host call per element for `String`) | 21.4M fuel, 27 pages | 374.9M fuel, 60 pages |
| `Map<number, number>` with `set` | 63.4M fuel, 40 pages | memory budget exhausted (typed `Memory`) at 415.5M fuel |
| `Set<number>` with `add` | 59.2M fuel, 40 pages | 578.4M fuel, 110 pages |
| String of n code units by doubling | 0.11M fuel, 27 pages | 0.75M fuel, 27 pages |
| JSON round trip of a `number[]` (guest work only; building the input) | 494 fuel (0.26M), 27 pages | 1474 fuel (2.4M), 69 pages |

| Frames workload | Fuel | Host calls | Pages |
| --- | --- | --- | --- |
| 100 concurrent awaits | 0.64M | 2 | 27 |
| 1000 concurrent awaits | 7.7M | 2 | 27 |
| 1000 pending `then` reactions | 2.2M | 1 | 27 |
| 1000 async frames suspended in a generator loop | 25.0M | 2 | 40 |
| 1000 live frames with forced collections | 43.3M | 1002 | 60 |
| Live frames until the memory budget (typed `Memory`) | 44.5M | 0 | 128 |

The geometric growth and the worklist cut the earlier costs several times:
1000 concurrent awaits took 83.7M fuel with the v37 collector, a 5000-node
linked list now marks in 5.9M fuel where 3000 nodes did not finish in 5000
production budgets, and a 4000-element `join` fell from 177.9M to 9.8M fuel.
Production fuel (1M) and the 1024 host calls still bound one execution to
roughly a thousand elements of such work; the turn-budgets stream makes the
budgets configurable.

**Pending.** Dictionary and mixed-record lookup is a linear key search and
insertion or deletion shifts the ordered vectors (a 1000-entry dictionary
with 500 deletions costs 251M fuel). Hashing them needs an index next to
the key vector, a value-layout change for the program-limits stream's
value18 (a third storage slot, for example an open-addressing `number[]` of
entry positions with tombstones and compaction, as in the `Map` prelude).
`String(x)`, `x % y` and number templates are host calls, so loops over more
than about 1000 elements exhaust the production host-call budget. `console.log`
diagnostics and the primitive `String`/`Number`/BigInt conversions still read
with the frozen string and depth limits, and the logs of a failed execution
are not shown.

## Local modules

The compiler accepts named function imports (`import { f as g } from "./lib.ts"`),
direct function exports (`export function f(...)`) and local export lists
(`export { f as g }`). Local export lists may forward an imported function.
Each module has a separate function scope; private helpers with the same name in
different files do not collide. Imports must name exported functions, and their
parameter/result types are checked at each call. Duplicate or reserved bindings,
duplicate exports and unresolved exports fail before emission. Before v31, type
aliases and interfaces retain their existing erased behavior and are not
importable.
Profile v31 adds [named types](#checked-f64-utf16-v31--named-types), including
type-only imports/exports and `./x.js` specifiers for `.ts` sources. Profile v32
adds [re-exports](#module-re-exports-module_reexports): `export { … } from` and
`export * from` forward functions, module constants, named types and classes
without binding them locally; their sources are ordinary graph dependencies.

The entry module must declare its own `main(): void` with no parameters. An
imported binding or an export alias cannot supply the entry point. A dependency
may declare an ordinary function named `main`, with its own signature. Recursion
within a file is supported; calls to the entry function remain rejected.

The dependency graph is a DAG: cycles, including self-imports, fail with an import
site diagnostic. There are no top-level execution or initialization semantics,
except that v32 initializes [module constants](#module-constants-module_constants)
in dependency order before `main`.
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

Profile v32 also adds [default exports](#default-exports-default_exports):
`export default` declarations and bindings, `import x from` and `default` in
export and re-export lists, linked as the name `default`.

Limits: 256 unique modules including the entry, 2048 import declarations, 32 modules
on the active dependency path, 1 MiB per source, 4 MiB aggregate source and 256
user functions across the graph. Re-export declarations (v32) count as import
declarations. Existing per-file lexical/semantic complexity
limits still apply. Namespace, side-effect and attributed imports, type-only
imports before v31, default imports/exports and export-from/export-star before
v32, namespace re-exports and dynamic imports fail explicitly. Unused imports and re-exports still
resolve and validate their dependency.

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

The [permission model](development-plan.md#permission-model) (accepted direction,
details pending) makes every denial in new profiles recoverable (`NotAllowedError`
or a failed `VartaResult`, never a trap) and adds permission states, a host
permission broker and user consent as a grant source bounded by the host ceiling.
Nothing below changes until a profile adopts it.

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

`time.timer` (v36) is a separate boolean operation: it authorizes arming
timers (`setTimeout`, `setInterval`, `AbortSignal.timeout`) and neither implies
nor is implied by `time.now`; see [Timers](#timers-timers).

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

`net.get` and `net.fetch` implement HTTP only: HTTPS grants and unsupported
schemes are rejected for them. V37's `net.request` admits `http` and `https`
under the same URL rules ([network requests](#network-requests-net_request)).
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
capability identifiers and scalar Wasm signatures; the
[embedding contract](embedding.md#host-operation-table) lists every operation
with its signature, minimum host ABI, capability and admitting profiles. Pointer-taking imports require
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

<!-- generated:limits:begin (checked by crates/varta-common/tests/documentation.rs) -->
| Resource | Limit | Source constant |
| --- | --- | --- |
| Guest linear memory | 8 MiB | `abi::MAX_MEMORY_BYTES` |
| Guest instructions (fuel) | 1000000 | `abi::FUEL` |
| Execution/HTTP deadline | 5 s | `abi::DEADLINE_SECONDS` |
| Host calls per execution | 1024 | `abi::MAX_HOST_CALLS` |
| Captured output | 1 MiB | `abi::MAX_OUTPUT_BYTES` |
| Host input string | 64 KiB | `abi::MAX_INPUT_BYTES` |
| One host result / one string value | 1 MiB | `abi::MAX_RESULT_BYTES` |
| Wasm file | 4 MiB | `abi::MAX_WASM_BYTES` |
| Wasm function body | 65536 bytes | `abi::MAX_WASM_FUNCTION_BODY_BYTES` |
| Distinct Wasm signatures | 256 | `abi::MAX_WASM_SIGNATURES` |
| Physical function parameters | 32 | `abi::MAX_WASM_PARAMETERS` |
| User functions | 256 | `abi::MAX_USER_FUNCTIONS` |
| HTTP adapters | 1 | `abi::MAX_HTTP_HANDLERS` |
| Compiler helpers (legacy / sparse / maximum) | 10 / 12 / 13 | `abi::*_INTERNAL_FUNCTIONS` |
| Record fields | 32 | `value::MAX_RECORD_FIELDS` |
| Record shapes per graph | 128 | `value::MAX_RECORD_SHAPES` |
| Record key bytes | 256 | `value::MAX_RECORD_KEY_BYTES` |
| Array elements / spread arguments | 1024 | `value::MAX_ARRAY_ELEMENTS` |
| Array element types per graph | 128 | `value::MAX_ARRAY_TYPES` |
| Union types | 128 | `value::MAX_UNION_TYPES` |
| JSON depth / visited values | 64 / 4096 | `value::MAX_JSON_*` |
| JSON schema types (legacy / current) | 260 / 389 | `json::*MAX_SCHEMA_TYPES` |
| Language metadata | 1024 bytes | `language::MAX_METADATA_BYTES` |
| Live continuation frames | 64 | `frame::MAX_LIVE_FRAMES` |
| Pending microtask continuations (v34) | 64 | `scheduler::MAX_PENDING_CONTINUATIONS` |
| Microtasks per execution (v34) | 65536 | `scheduler::MAX_EXECUTION_JOBS` |
| Promise serials / rejection ids per execution (v34) | 1048576 | `scheduler::MAX_PROMISE_SERIALS` |
| Unhandled-rejection reasons kept / units per field / bytes each | 16 / 256 / 2048 | `scheduler::MAX_*REJECTION_REASON*` |
| Pending timers (v36) | 32 | `scheduler::MAX_PENDING_TIMERS` |
<!-- generated:limits:end -->

Limits that are derived from several constants or enforced outside `abi`:

| Resource | Default limit |
| --- | --- |
| Managed string/record/array storage | Within the 8 MiB memory budget, after static page, 1 MiB host buffer and 64 KiB root stack; includes block metadata |
| Reference roots | 8192 handle slots across active frames and temporaries |
| JSON serialization | 1 MiB encoded, depth 64, 4096 visited values per call |
| Checked JSON parsing | 64 KiB encoded input, depth 64, 4096 values; schemas at most 64 KiB in the shared static pool (type counts above) |
| HTTP response headers | 64 KiB |
| Manifest/config | 256 KiB |
| Function parameters | Each captured `VartaResult` uses two physical parameters |

The per-value rows above (array elements and spread arguments, string
values, JSON depth, visited values, input and output, live frames, pending
continuations and timers) are the frozen limits of every profile before v38.
Profile v38 takes them from `limits::DataLimits::V38`
([data limits](#data-limits-data_limits)); its memory, fuel, deadline,
host-call and output budgets are the production budgets above.

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

## Developer compatibility inventory

`varta-conformance` is an outer development-tool consumer of public common,
compiler and runtime APIs. It has no dependency on project packaging or the CLI
and adds no dependency, import, source profile or budget to the execution-only
runtime. Workspace dependency-boundary tests classify it as tooling. SHA-256
uses the already locked sha2 0.10.9 dependency in this tool alone.

The [runner contract](../conformance/README.md) pins edition 2025, the profile named in its manifest and
checked-ts-main-v1 programs. Original corpus files are mandatory manifest entries
with exact SHA-256; duplicate/unknown metadata, escapes, symlinks, omissions and
byte/count limits fail before launch. Source bytes are snapshotted before workers
and passed on stdin without compiler filesystem loading or upstream rewriting.
The report includes inventory/source/dependency-lock fingerprints and host target
identity. ECMAScript-derived feature rows and Varta-specific contracts have
separate origins and map to CORE IDs; feature case counts do not assert complete
feature semantics or produce a global completion percentage.

A child process compiles each source, validates it through the runtime API and
runs with empty grants/host ceiling and ordinary fuel/memory/deadline limits,
unless the case's manifest row lists reviewed `grants`; those are then both the
requested grants and the host ceiling, and only `time.timer` is admitted (any
other grant fails inventory loading).
The parent clears the worker environment, writes and drains bounded protocol
pipes on independent threads, polls a bounded wall deadline, and kills/reaps failed
workers. Internal compiler, transport, output, process and timeout faults are
infrastructure failures. Runtime faults and changed observations are regressions;
a documented exact compile diagnostic may match a known-unsupported baseline but
never increments the passed count. Unexpected support requires baseline review.
Strict mode also rejects known gaps and untested feature rows. CI stores the
baseline report in logs while preserving these categories.

This is partial CORE-15 verification of the checked execution model. Script/global
realms, full module/strict/non-strict modes, upstream harness bindings, complete
edition/profile matrices and independent platform/readiness evidence remain open.
Current feature and case counts are reported by the runner and recorded in
[conformance/README.md](../conformance/README.md). An explicit union-typed JSON case and separate
uncontextual inference/peer/reference cases retain independent observations;
v27 closes the formerly unsupported inference case. Existing
regression tests, public APIs, artifacts, fuzzing and readiness acceptance work
remain part of the full core objective.

## Runtime internals (non-normative)

These sections describe runtime and common-crate mechanisms that no source profile
admits yet. They are implementation notes, not language contracts.

### Managed closure envelope views (transport preparation)

Common function views independently validate the existing v12+ closure cell,
its optional environment and each captured binding envelope without allocating
or invoking guest code. Count/mask validation is bounded by MAX_CAPTURES; payload
bits stay opaque, preserving legitimate recursive captures and scalar bits that
resemble pointers. Binding type IDs are private per-module metadata. Native
result pairs retain their two-slot physical shape and a root bit only for their
first slot. Legacy v11 cells and public structured value tags remain distinct.

This is envelope validation, not proof of module code membership or typed captured
payload validity. A future public function-value transport must supply those
proofs and version its tags, schemas and native operations before source admission.
Current v30 value13 cannot represent function-valued properties, arrays or unions.

### Typed private capture catalog

Optional v30 `varta.functions` metadata describes private binding IDs, result
widths, value schema DAGs and code/signature capture layouts. The compiler builds
it from its binding and checked dispatch tables. Preflight validates catalog
version/profile, bounded references/counts, acyclic type depths and uniqueness;
missing optional metadata remains compatible with earlier v30 artifacts.

A typed inspector consumes immutable catalog data plus a borrowed guest memory
snapshot. It validates exact binding masks/types/widths/status, primitive payloads,
canonical BigInt/Error objects and typed aggregate/union children. Record fields
must match their compiler slot identities as well as names/kinds/optionality;
serialized descriptor entry order may vary. A v32 `class` node is a record-tagged
leaf validated as a record aggregate; it counts as the record member of a
reference union and cannot appear in a primitive union. Sparse arrays preserve holes and
validate their exact element type. Code/signature catalog membership is checked
before function memoization so aliases cannot bypass a type mismatch. Recursive
closures are legal and work/node/depth allowances remain bounded. Actual callable
invocation still uses the guest's checked dispatch; this API supplies no call
permission and does not invoke code. A public function tag and versioned native
transport remain a separate required step.

### Function tag12 in value14

Value14 reuses the existing physical closure but makes Function tag12 an explicit
reference kind. Legacy tag support stays exact. Reference-cell function reads
require a module-owned catalog; header-only envelope inspection is separate from
payload validation. V14 catalog schemas can describe function-valued record fields,
array elements and a single signature in a reference union. Data validation keeps
recursive private closure graphs bounded and checks signature membership before
memoization. A Rust codec entry point supplies immutable catalog data and a shared
function work allowance; it cannot invoke guest code.

JSON resolution validates each function before mapping it to Undefined, allowing
existing object omission, array-null and top-level-Undefined rules to apply. This
mapping does not expose private capture cycles as JSON cycles. Callable toJSON
properties fail explicitly until invocation can happen in guest code. Schema14
inherits JSON input types/layout generation without a callable JSON constructor.
Host-operation registration and source admission remain future versioned work;
current v30 modules still use value13/ABI28.

### Prepared module capture ownership

`VartaRuntime::prepare_module` and `prepare_module_and_tools` retain the validated
Wasm module together with an optional immutable `VerifiedFunctionCatalog`.
Metadata is verified once during portable-module preflight. The sealed common
catalog exposes shared schema reads and bounded capture proofs, without mutable
schema access or repeated metadata-DAG validation. The raw `FunctionCatalog`
API still verifies metadata on each proof because its fields remain mutable.

Normal startup, tool invocation and fresh HTTP request instances use the prepared
module path. Each Store receives its module's catalog before instantiation and
owns an Arc independently of the prepared module's lifetime. Execution allows
one instantiation attempt, matching its existing one-instance resource ceiling.
A second attempt cannot replace a live instance's schema; a failed first attempt
clears the schema and cannot be retried in that Store. No process-wide catalog
registry or guest-memory metadata lookup is involved.

The existing `validate_module`/`validate_module_and_tools` APIs continue returning
backend modules for compatibility and inspection. A backend module alone carries
no capture owner. Missing optional catalogs remain accepted for older v30 code.
This ownership layer does not admit value14 source operations or grant indirect
call authority; versioned host handlers and source admission are still pending.
Continuation-frame layouts extend this module-owned metadata in the frame catalog
described next; the design is in the
[development plan](development-plan.md#k44--suspension-scheduling-and-async-planned).

### Suspension foundation (K4.4 block 1)

This is the implemented foundation of the agreed K4.4 design. No profile admits
suspension syntax: the only producer of a suspension point is a test-only
lowering hook in the compiler. No host import, capability or value tag changes;
runtime preflight admits the resulting frame surface only under v33
([runtime admission](#runtime-admission-resumable_frames)). Modules without
suspension points compile exactly as before (the passes below find nothing to
do).

**IR and normalization.** `Expression::Suspend { value, state }` yields `value`
and evaluates to the resume input, whose type is the node's type. After lowering
and literal erasure, `lower::suspension` normalizes every function that contains
one. Operands evaluated before a suspension point (callees, ignored receivers,
arguments, binary operands, record/array elements, member bases and indices) move
into hidden locals in their original order; later operands stay in place. `&&`,
`||`, `??` and `?:` become explicit branches assigning one result local, so the
suspending operand runs only on its branch. Loop tests that suspend move to the
top of the body as `if (!test) break`. Lowering-produced value blocks split into
ordinary statements only for the constructs whose preparation has been reviewed:
simple, compound and logical assignments, updates and `&&`/`||`/`??`/`?:`. Then
the prepared receiver, key and old value are evaluated before the suspension.
Every other value block and the remaining positions are rejected with
`unsupported suspension point inside <construct>`. The construct is named from
the source node at the block's span, for example `call to Math.hypot`,
`` `repeat` method call ``, `switch case test`, `do-while condition`,
`a closure` or `an entry function`. The canonical form is
`Init(t, Suspend { value })` with a suspension-free `value` and a fresh `t`.

**State machine.** Suspension points are numbered 1..n in statement order, so
each structured statement covers one contiguous state range. State 0 means
"created, not started". Every list member is guarded: suspension-free runs execute
only while running (state 0), and compound statements are entered while running
or when the target state lies in their range. An `if` records its branch choice in
a hidden local; a loop re-checks its test only while running. Resumption restores
all saved locals, then re-enters the structured code, so the emitter rebuilds the
`try` handler, label and root bookkeeping of the target state itself. The handler
context is therefore static per state, and the frame's handler field stays zero.
At the target state the delivery clears the state, raises a `Throw` input as a
Number exception at that point, performs a `Return` input as `return` (running
enclosing `finally` blocks) or binds a `Normal` input. Suspension inside `catch`
and `finally` bodies and `switch` case bodies, first rejected in this block, is
admitted with the v33 rules in
[completions in handlers](#synchronous-generators-generators).

**Frames.** A frame is a separate managed kind, heap flag **4096**, with a
48-byte body: `[layout-id:u32, fields=5:u32, control:i64, status:i64,
environment:i64, completion:i64, handler:i64]`. Control is `resume-code:u32 |
state:u32`, status is `status:u32 | generation:u32` (suspended 1, running 2,
completed 3), and the exact edge mask is **28** (environment, completion and
handler). The environment is an environment block (flag 256) of kind **1** whose
entries are segments of kind **2**. Each segment holds at most 32 binding handles
(the 32-bit edge mask), and a frame holds at most 8 segments. Kinds 1 and 2 are
rejected by closure validators, which expect kind 0. Saved bindings reuse binding
cells (flag 512) with their private type ids, widths and reference bits. A
captured local stores its closure cell, so closures and the continuation share
one binding; that entry is zero until the cell exists. Every other local owns a
frame cell that the creator allocates. The state, completion and input locals are
re-initialized on each entry and are not saved. Budgets: 256 saved bindings per
frame (independent of the 32-capture closure limit), 256 suspension states per
function, and 64 live frames. Live frames are permanent root-table slots placed
after module-constant roots in the reserved root region.

**Creator, resume body and dispatcher.** A suspendable function's own index
becomes its creator: it allocates and registers the frame (rooted before the next
allocation), binds the arguments into cells and returns a resume token `frame:u32
| generation:u32 << 32` without running the body. Generations come from one
instance-wide counter; exhausting it is fatal. The resume body takes
`(frame:i64, completion:i32, value:i64)`. The exported dispatcher
`varta_resume(token:i64, completion:i32, value:i64) -> i32` performs closed direct
calls by layout id; it uses no table or indirect call. Before any effect it traps
when:

- the exception channel is not empty;
- the frame is not live in this instance's root table (ownership);
- the envelope is not exact (length, flags, mask, field count, zero handler);
- the frame is running (reentrancy) or completed, or the generation is stale;
- the completion kind is not Normal 0, Throw 1 or Return 2;
- the layout, resume code and state do not agree;
- the input is not admissible.

Admissible inputs are scalars only. Float64 accepts any bits, Boolean only 0 or
1, and Undefined only 0. A Throw input is a Number, and a Return input must match
a Float64, Boolean, undefined or void result; other results admit no Return.
Host-supplied references need a validated transport and are outside this block.
The dispatcher reports Suspended 0, Returned 1 or Threw 2. It moves an uncaught
exception from the channel into an exception-union completion cell and clears the
channel. The completion type is the profile's catch union; in a v32 program
that declares an Error class it holds the Error root class, so an uncaught
Error-class instance is a Record member of the completion with its descriptor,
`message` and `name` intact, and saved class instances are traced through the
frame like any other reference binding. Suspension points inside methods,
constructors (including Error-root `super(...)`), `instanceof` tests and the
other value blocks of the new v32 forms are rejected by construct name, and a
generic instance can contain one only behind a rejected reference. A completed
frame leaves the root table. Its completion stays readable
until the next guest call that may collect it. The dispatcher has effects and is
never a pure helper; JSON helper purity validation is unchanged.

**Exception and GC invariants.** The exception channel is empty whenever the
scheduler holds control. A suspension point asserts this fatally before
returning, and the dispatcher requires it on entry. Fatal traps and resource
exhaustion are never converted into completions. In frame-bearing modules the
collector additionally traces the frame flag through the exact mask; segment and
cell tracing reuses the environment/binding path. Restoration checks every saved
cell before use: allocation bounds, flags, reference mask, private type id, width
and canonical Boolean/null/undefined payloads.

**Metadata and versioning.** Frame-bearing modules emit function catalog version
**2** with `frames` rows: resume code, saved binding ids and sharing, per-state
input class and yielded binding, result and Return-input class, and exception
binding. `FunctionCatalog::from_bytes` still accepts only version 1 and rejects
version 2; `from_frame_bytes` reads both. Runtime preflight selects the reader
by profile, so only v33 and v34 read version 2; v35 frames use liveness
layouts and catalog version 3 instead
([frame liveness](#frame-liveness-frame_liveness)).
Modules without frames keep emitting version 1 unchanged. Test builds also export
frame creators (`varta.test.frame.<layout>`) and the collector
(`varta.test.collect`); release builds export only `varta_resume`.

Profile v33 and its host ABI declare the `varta_resume` export, the frame flag
and layout and frame catalog version 2 validation in preflight
([runtime admission](#runtime-admission-resumable_frames)). Profile v34 adds the
scheduler-owned token, root and generation policy that lets suspended frames
outlive the guest call that created them
([microtask scheduler](#microtask-scheduler-promises)).
