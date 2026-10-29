# Varta development plan

## Status

Updated 2026-10-29. Varta is an experimental research project: a compiler and
capability-checked runtime core with K1–K3 implemented in their initial checked
profiles and K4 language/value work in progress. It is not a production release
and does not claim full ECMAScript, Node or npm compatibility.

| Item | Current state |
| --- | --- |
| Profiles | Default `checked-i32-utf8-v1`; latest released opt-in `checked-f64-utf16-v31`; `checked-f64-utf16-v32` is the [pilot batch](#pilot-batch-v32-in-development), merged on the `v32-integration` branch and not released. Versions of every profile: [version matrix](architecture.md#version-matrix) |
| Last full gate | `main`: commit `bf8c6c2` (2026-10-28), 1750 tests. `v32-integration` with both pilot waves: commit `45a6af1` (2026-10-29), 1908 tests. `v34-integration-merge` with the records, unknown and async streams: commit `02d1a75` (2026-10-29), 2018 tests, 0 failures. All are the seven-command gate in [verification.md](verification.md#full-gate) on a local host, not cross-platform CI results |
| Compatibility inventory | Current counts and gaps: [conformance/README.md](../conformance/README.md) |
| Completion | Measured by the [core completion ledger](#core-completion-ledger); no work stream is closed by this snapshot |
| Pilot | [uarp-sdk 0.12.0](#pilot-acceptance-package-uarp-sdk); M1 in progress: on the `v35-merge` branch `generated/meta.ts`, `generated/models.ts`, `core/json.ts` and `core/pagination.ts` compile and validate; `Headers` and `AbortSignal` resolve, and the other files stop at `ProblemDocument.errors` or a defaulted function-typed parameter ([measurement](#pilot-acceptance-package-uarp-sdk)) |

Next work, in order:

1. **Suspension foundation** ([K4.4](#k44--suspension-scheduling-and-async-planned)
   block 1): expression normalization, continuation frames over closure cells,
   exception and GC invariants and the closed resume dispatcher are implemented
   compiler-internally and tested in Wasmtime
   ([contract](architecture.md#suspension-foundation-k44-block-1)) and merged
   on the v32 integration branch. Runtime admission under profile v33
   (`resumable_frames`) is implemented; the runtime does not schedule frames
   yet. Synchronous generators (K4.4 step 2, `generators`) are implemented in
   v33 on the `v33-generators` branch
   ([contract](architecture.md#synchronous-generators-generators)).
2. **Pilot M1–M3**, alongside block 1: review and release the v32 pilot batch
   (both waves are merged on the integration branch), then index signatures and
   wider records, Web types (`Headers`, `AbortSignal`) and the 65-file module
   graph budgets.
3. Promise jobs, then async/await (K4.4 steps 3–4): implemented under v34
   and merged with the records and unknown streams on `v34-integration-merge`
   ([scheduler](architecture.md#microtask-scheduler-promises),
   [async functions](architecture.md#async-functions-async_functions));
   timers, I/O and async HTTP (step 5) are next.

Completed increment logs, earlier checkpoints and verification records are in
[history.md](history.md).

## Product direction

Varta targets general-purpose, typed TS/JS applications compiled to portable
Wasm and shipped with a controlled execution environment. A developer should be
able to implement a JSON HTTP server in TS/JS using Varta's own runtime APIs,
without writing a separate Rust/Node/Python server wrapper for each application.
Network I/O belongs in the runtime; routes, payload schemas and business rules
belong in the TS/JS application. Explicit AI tools and MCP are optional interfaces
on top of a capable application core, not prerequisites for ordinary applications.

This is the target direction, not a claim that these platform features already
exist. The current implementation is the compiler/runtime foundation described
in [architecture.md](architecture.md): a checked TypeScript subset, local modules,
strings and reclamation, captured host results, capability policies, native
packaging, primitive Tool Contract v1, offline discovery and checked JSON tool
invocation. Closed records, bounded resizable/sparse typed arrays in opt-in profiles,
`null`/`undefined`, graph tracing and bounded
`JSON.stringify`, `varta.json.parse<T>` and the first HTTP/1 service profile are
implemented. A shared-state application service, broader Application Contracts,
MCP, package signatures,
GUI/database/hardware adapters and full JS/npm compatibility are not implemented
yet. HTTP listeners are independent of the existing outbound client.

### Research thesis

Varta is an experimental research project; its subject is the architecture
itself. Thesis: **strictly typed TypeScript, including typed npm packages, can be
compiled ahead of time into portable WebAssembly and run safely on any OS kernel
through a thin capability-checked host, without a JavaScript engine on the device.**
Language semantics are resolved by the compiler; the device needs only a Wasm
host and explicitly granted host operations.

| Pillar | Commitment | Open research questions |
| --- | --- | --- |
| Write typed TS | Everything typed, without exceptions. `any`, implicit `any`, untyped JS and packages that ship only `.d.ts` are rejected with a clear diagnostic. `unknown` is admitted and must be narrowed before use. | Structural interfaces, generics and classes under exact layouts; whether `as`/`!` become runtime-checked assertions |
| Compile ahead of time | Types are the mandatory source of semantics and layout. No JS interpreter or JIT is embedded. | How far typed dynamic features (prototypes, `this`, property access) compile statically; async/generators without Wasm stack switching ([K4.4](#k44--suspension-scheduling-and-async-planned)) |
| Run on any OS kernel | Portable Wasm plus a per-target host; one artifact runs wherever a compatible host exists. | A replaceable or interpreted host for kernels without JIT; an OS abstraction layer (WASI 0.2/0.3 as a reference model) |
| Run safely | Deny by default; exact operation and resource grants; bounded execution. | Per-package capabilities as a defence against supply-chain attacks |

Evidence comes from real typed npm packages that are supported: TypeScript
sources available, no native addons and no `eval`. The measurements are the share
of files compiled, the share of the package's own tests passing, artifact size,
startup time and memory. Node, Deno and Bun are comparison baselines, not the
product category. The [uarp-sdk pilot](#pilot-acceptance-package-uarp-sdk) is the
first such package.

### Runtime target and compatibility

The compatibility target is ECMAScript 2025 semantics for strictly typed programs
(see the [research thesis](#research-thesis)), retaining Varta's TS → Wasm →
controlled-runtime path. The current checked subset is an implementation stage,
not the permanent boundary. A JSON server is the first vertical acceptance
scenario, not the definition of a complete runtime.

The complete core roadmap must cover:

| Area | Target | Current gap |
| --- | --- | --- |
| Language semantics | ECMAScript values, numbers, strings, objects/arrays, functions/closures, errors and control flow | Default `number` is i32; opt-in binary64 supports closed aggregates and JSON. Opt-in UTF-16 supports lone units and character methods; closed primitive unions, typed functions and initialized lexical captures are implemented; dynamic objects/coercion and broader function semantics remain pending |
| Memory | Bounded, correct ownership and tracing for full application value graphs | Collector traces closed records, resizable/sparse arrays, reference unions, Error/closure graphs and strings; general dynamic object graphs remain pending |
| Scheduling | Event loop, promises, microtasks, async/await, timers, cancellation and shutdown | Synchronous bounded invocations only |
| Standard APIs | JSON, URL, encodings/binary data, Request/Response/Headers, streams and abort signals | Most standard APIs are absent |
| System APIs | Capability-checked filesystem, environment, processes and inbound/outbound networking | Existing bounded operations are a small partial API |
| Modules/packages | ES module semantics, a documented resolution/loading policy and tested package compatibility | Explicit local named-function modules only |
| Distribution | Independent runtime, portable guest artifacts and target-specific native packaging | Runtime-only companion implemented; cross-target packaging, signing and platform release distribution remain pending |
| Verification | A pinned conformance target, public-API compatibility cases, negative/security tests and resource/performance measurements | Current regression tests prove the MVP contract, not Deno/Bun or ECMAScript completeness |

Do not treat unsupported JS as solved by providing an application-specific host
wrapper, and do not claim full compatibility because a few server examples run.
Track each missing semantic/API explicitly. Package/API compatibility and broader
language semantics continue after the first HTTP milestone.

Before extending the value representation, specify the intended JS semantics and
version/profile migration: binary64 Number values (including -0/NaN/infinities),
UTF-16 string behavior including lone surrogates, null versus undefined,
coercion/equality, property ordering and aliasing. Existing ABI/contract v1 i32
artifacts must retain their documented interpretation or be rejected by version;
never reinterpret them silently as the future JS-compatible profile. Preserve
clear diagnostics for features not yet implemented.

Primary references: [ECMAScript language values](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html),
[Deno HTTP server](https://docs.deno.com/runtime/fundamentals/http_server/) and
[Bun HTTP server](https://bun.sh/docs/runtime/http/server). These define semantic
and developer-experience references, not a claim of current Varta support.
### Core completion ledger

Completion is measured against the full core target above, not the number of
commits, regression tests or implemented checked profiles. The entries below are
work streams, not equally weighted percentage points. A work stream can close only
when its explicit feature inventory, executable positive/negative cases and
documented compatibility bounds pass. A global 100% claim additionally requires
the conformance, platform and independent security acceptance work; it cannot be
inferred from the current regression suite.

| ID | Work stream | Current state | Remaining acceptance work |
| --- | --- | --- | --- |
| CORE-01 | Number, String and primitive semantics | Partial | Remaining BigInt/object ToPrimitive and inferred storage generality (v28 scalars; v29 graphs, primitive-union arithmetic, throws and JSON integration), Symbol, Symbol/object comparison and remaining conversion operations, locale formatting and general object coercion (canonical logging v18; primitive conversions v19/v20; checked toString/radices v21; precision/valueOf v22; prefix parsers, Boolean/global predicates and guest truth/control flow v23; scoped truth guards v24), remaining operators and primitive built-ins against the pinned ECMAScript target |
| CORE-02 | Objects, arrays and value graphs | Partial | Dynamic properties, property descriptors, prototypes, constructors/classes and their tracing/identity semantics |
| CORE-03 | Functions and control flow | Partial | Complete call/receiver/binding behavior, remaining iteration and generator semantics; retain existing closure/exception guarantees |
| CORE-04 | Exceptions | Partial | General thrown values and complete error families/propagation, with explicit roots and unwinding rules |
| CORE-05 | Standard collections and binary values | Planned | Map/Set, typed arrays, ArrayBuffer/DataView and other target built-ins with ownership/resource contracts |
| CORE-06 | Math and remaining language built-ins | Partial | Finish the explicit Math inventory; cover Date, RegExp and remaining selected standard objects |
| CORE-07 | Garbage collection and resource lifecycle | Partial | Extend exact tracing to every admitted value kind; verify retained roots, cancellation and leak-free long-lived lifetimes |
| CORE-08 | Event loop and promise jobs | Planned | Specify and implement bounded task/microtask queues, Promise resolution/rejection and ordering; agreed [suspension design](#k44--suspension-scheduling-and-async-planned) |
| CORE-09 | Async language and cancellation | Planned | Async/await, timers, cancellation/abort and owned shutdown, integrated with scheduling and roots; agreed [suspension design](#k44--suspension-scheduling-and-async-planned) |
| CORE-10 | JSON and Web APIs | Partial | General JSON parsing plus the target URL/encoding/Request/Response/Headers/stream APIs |
| CORE-11 | System APIs | Partial | Complete the declared filesystem/process/network surface with exact permissions, portable errors and bounded I/O |
| CORE-12 | ES modules and packages | Partial | Module initialization/linking/loading semantics, explicit package-resolution policy and tested package targets |
| CORE-13 | ABI and capability enforcement | Implemented for current profiles | Retain fail-closed version/signature/resource checks for every newly admitted boundary |
| CORE-14 | Distribution | Partial | Supported target matrix, Windows/cross-target validation, release delivery and package authenticity |
| CORE-15 | Conformance | Partial | Pinned checked-case inventory and isolated runner are implemented; expand edition-wide cases, script/module/harness execution for upstream Test262, historical profiles and independent compatibility verification |
| CORE-16 | Performance and production readiness | Partial | Reproducible startup/memory/throughput/leak measurements, sustained workloads, fuzzing and independent security review |

The immediate implementation order remains CORE-01/02/03/06 foundation work,
followed by CORE-07/08/09 scheduling and lifetime integration and CORE-10/11/12 API
and package expansion. CORE-13/15/16 verification accompanies every increment.
Application state, MCP and domain adapters remain separate deferred work streams;
they do not substitute for missing engine semantics.

CORE-06's Math inventory is pinned to the
[ECMAScript 2025 Math object](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-math-object):

| Math surface | Current state |
| --- | --- |
| abs, ceil, clz32, floor, fround, f16round, hypot, imul, max, min, pow, round, sign, sqrt, trunc | Checked direct Number calls implemented in v15 |
| acos, acosh, asin, asinh, atan, atanh, atan2, cbrt, cos, cosh, exp, expm1, log, log1p, log10, log2, sin, sinh, tan, tanh | Checked direct Number calls implemented in v16/ABI19 |
| random | Fresh OS entropy and exact [0,1) samples implemented in v17/ABI20; ignored arguments preserve effects |
| E, LN10, LN2, LOG10E, LOG2E, PI, SQRT1_2, SQRT2 | Exact v15 numeric reads; descriptors/general namespace objects remain planned |
| General argument conversion/arity, spread, first-class methods and ordinary Math object properties | Planned beyond the current direct-call admission rules |

Counting method names does not establish complete Math semantics. Every remaining
entry needs its edge cases, argument/exception behavior, resource contract and
source-free runtime tests before CORE-06 can close.

CORE-01's Number surface is pinned to ECMAScript 2025:

| Number surface | Current state |
| --- | --- |
| Number(...) | Primitive/union conversion in v20; wrappers and object ToPrimitive pending |
| Boolean(...) and global isFinite/isNaN | V23 ordinary-value ToBoolean and primitive numeric coercion; wrappers/object ToPrimitive pending |
| Eight numeric constants; isFinite/isNaN/isInteger/isSafeInteger | Checked v15 reads/predicates |
| Number.prototype.toString | Narrowed direct calls, bases2–36 and primitive radix semantics in v21 |
| toFixed, toExponential, toPrecision, valueOf | Direct narrowed Number calls in v22, primitive precision conversion, exact rounding and owned results; valueOf guest-only |
| parseInt/parseFloat and global aliases | Direct primitive/union calls in v23, prefix grammar, Int32 radices and bounded exact integer rounding |
| toLocaleString | Planned with an explicit locale/Intl contract |
| Wrappers, ordinary prototype objects/descriptors, first-class methods | Planned under object/function foundation work |

These checked methods do not close CORE-01 or establish general Number-object
compatibility; all remaining rows and full conversion/equality/value targets still apply.
### Architecture commitments

- Keep the engine application-agnostic. Application entities, state schemas,
  commands, domain validation and domain permissions are defined by TS/JS
  applications. The engine supplies generic execution, state ownership,
  transactions, versions, limits and authorization mechanisms. Reference examples
  must not become built-in document/editor/robot profiles in engine crates.
- Define the supported TS/JS language profile explicitly. Grow it through concrete
  application requirements; unsupported dynamic behavior continues to fail clearly.
- Keep source compilation separate from interface publication. An ordinary source
  export does not automatically expose an AI tool. Developers explicitly choose
  which operations form the public tool surface.
- Distinguish **Application Contract** (commands, queries, data, errors, state,
  revisions and lifecycle) from **Tool Contract** (the AI-exposed subset, names,
  descriptions, schemas, required effects and limits). Both share one definition
  of the operation rather than duplicating business logic per transport.
- Preserve runtime boundary checks. External/AI arguments and results require
  bounded validation even when their TS definitions were checked at build time.
  Schemas describe data; host policy separately authorizes concrete effects.
- Treat artifacts, callers and AI-generated arguments as separate trust inputs.
  Effective authority must satisfy package requests, host ceilings and caller
  permissions. Discovery and tool annotations never grant execution rights.
- Ship public contract metadata together with executable code. Validate its link
  to real handlers. Signing must bind code, contracts and requested permissions;
  a recomputable checksum alone does not establish a trusted publisher.
- Keep the portability promise precise: portable Wasm plus compatible host
  interfaces, with native launchers built per supported OS/CPU. Do not promise
  one native executable for every platform or hard real-time behavior.
- Avoid including source implementations in release tool catalogs. Names, schemas
  and descriptions are intentionally public. Wasm remains analyzable/editable;
  compilation does not guarantee secrecy of business logic, and signature checks
  require a trusted verifier. Credentials belong to trusted host-side providers.
### Current repository scope

Core development comes first. The engine consists of `varta-common`,
`varta-compiler` and `varta-runtime`. Project loading/configuration, scaffolding,
packaging and CLI presentation are supporting development tools. They consume
engine APIs and must not become engine dependencies.

No separate application project is being built in this repository. The scenarios
below are deferred consumers of the core, not work items for built-in domain
models. First design and verify the generic mechanisms; only then build a separate
TS/JS project using those real APIs. Test source fixtures remain necessary for
compiler/runtime regression coverage.
### Active priority: a usable application core

Engine capability comes first, not an editor example or MCP. K1–K3 are implemented
in their initial checked profiles and K4 language/value work continues. The current
order is the [status](#status) list: the suspension foundation and pilot steps
M1–M3. A native TS/JS program serving a real JSON object remains the acceptance
scenario for the generic server capabilities. An external wrapper around
`varta invoke`, or a guest returning a string that a separate host parses as JSON,
does not satisfy it.

### Pilot acceptance package: uarp-sdk

The order of language and API work is chosen by one real typed npm package: our
own [`uarp-sdk`](https://www.npmjs.com/package/uarp-sdk), the TypeScript client for
our UARP platform ([source](https://github.com/Snaga-AI/uarp-sdks), directory
`packages/typescript`). For each increment, the question is: what prevents this
package from compiling and running? Completion is still measured by the
[core completion ledger](#core-completion-ledger); the pilot orders work within
it, and each ladder step below names the CORE work streams it advances.

The package is an external acceptance target, not an engine feature. No UARP
types, routes or client logic enter engine crates, built-in profiles or the core
CLI, and its sources are not vendored into this repository.

Pinned baseline: version **0.12.0**, tarball SHA-256
`2a0f2ce97132c90be61ca45c5ea65b77ba25d1706e50894543c5737a6d8bb56e`
(npm integrity `sha512-zoKVCcSTIJXiLvyUKmXyNBH1xPDo7XRcikch+YUasnQPbvTZ8atvdgCci9e8FD+klG4/VyXcMpYGMlXngEPbEg==`).
Later versions are adopted explicitly, with this section updated.

| Part | Size | Content |
| --- | --- | --- |
| `src/generated/models.ts` | ~23k lines | 1762 interfaces and 357 string-literal union aliases; almost entirely types |
| `src/generated/resources/*` | ~20k lines | 55 `XResource extends APIResource` classes, 841 methods returning `Promise<T>` |
| `src/core/*`, `src/index.ts` | ~1.3k lines | Transport, retries, SSE, pagination, error hierarchy |

The package is ESM (`"type": "module"`), has 65 source files and no runtime
dependencies, and contains no explicit `any`, so it meets the
[typed-only rule](#research-thesis). Its only library-level `any` entry point is
`JSON.parse`. Counts below are regex-based over comment-free 0.12.0 sources and
approximate.

**Verified behavior with v35 (2026-10-29).** The same probe on the
`v35-merge` branch with all seven v35 selectors integrated (Web types, mixed
index records, JSON to `unknown`, async methods, promise values and liveness
frames on value layout 16), under `checked-f64-utf16-v35` with production
budgets unchanged. The first diagnostic per file:

| First diagnostic | Files |
| --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts`, `core/pagination.ts` | 4 |
| `core/errors.ts:4` `unsupported declared property errors of a mixed object type` (`ProblemDocument.errors?: Array<{ field?: string; message?: string }>` next to `[key: string]: unknown`: the declared array type is not checkable from the index type; reached through the same 60 files as `Headers` before) | 60 |
| `core/util.ts:125` `unsupported union members other than number, boolean, string, null or undefined` (the defaulted function-typed parameter `random: () => number = Math.random` of `streamBackoffDelay`) | 1 |

The file count is unchanged, but both v34 blockers are gone: `Headers` and
`AbortSignal` resolve to the engine preludes and the probe now stops later in
the same two files. In a scratch copy with `errors?: unknown`, the `index.ts`
probe reaches `core/util.ts:125` too; with that parameter's default removed as
well, its next diagnostic is `core/sse.ts:226` `unsupported async generator
function` (`parseEventStream`, K4.4 step 6). So the next pilot batch needs
declared fields checkable from `unknown` (arrays of mixed/optional records),
optional function-typed parameters and async generators.

**Verified behavior with v34 (2026-10-29).** The same probe on the
`v34-integration-merge` branch with all eight v34 selectors (value layout 15,
index signatures, recursive types, wide records, `unknown`, checked
assertions, promises and async functions): each of the 65 source files of the
pinned tarball was compiled with `varta check` under `checked-f64-utf16-v34`
as the only `export *` source of a probe entry (`index.ts` through its default
import), so the file and all its dependencies form the graph; production
budgets were unchanged. The first diagnostic per file:

| First diagnostic | Files |
| --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts`, `core/pagination.ts` | 4 |
| `core/errors.ts:121` `unsupported type reference Headers` (reached through `core/resource.ts`, `core/transport.ts`, `core/sse.ts`, every resource, `generated/resources/index.ts` and `index.ts`) | 60 |
| `core/util.ts:21` `unsupported type reference AbortSignal` (`sleep`) | 1 |

Compared with the second v32 wave, `core/pagination.ts` passes: `CursorPage<T>`
is admitted and the uninstantiated `async function* autoPaginate<T>` is not
lowered until a caller instantiates it (async generators remain rejected, K4.4
step 6). `generated/resources/index.ts` and `index.ts` now report `Headers`
first instead of the module graph limits, because the walk reaches
`core/errors.ts` before the graph is complete; the graph limits are still
expected next. With a `Headers` class stub, `core/errors.ts` stops at
`ProblemDocument`, whose declared fields are combined with
`[key: string]: unknown` — a mixed object type that v34 index signatures do
not admit, so the `class … extends Error` hierarchy with its `unknown` causes
is still not reached in the file itself.

**Verified behavior with v32, first wave (2026-10-28).** Each of
the 65 source files of the pinned tarball was compiled under
`checked-f64-utf16-v32` as the only `export *` source of a probe entry, so the
file and all its dependencies form the graph; production budgets were unchanged.
The first diagnostic per file:

| First diagnostic | Files |
| --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts` | 3 |
| `core/errors.ts:121` `unsupported type reference Headers` (reached through `core/resource.ts`, `core/transport.ts`, `core/sse.ts` and most resources) | 36 |
| `core/pagination.ts:3` `unsupported generic type declaration` (`CursorPage<T>`, reached by paginated resources) | 23 |
| `core/util.ts:8` generic function `pick<T, K extends keyof T>` | 1 |
| `generated/resources/index.ts`: module graph exceeds the import-declaration limit | 1 |
| `index.ts:54` `export default` | 1 |

With `Headers` stubbed, `core/errors.ts` next stops at an index signature in
`ProblemDocument`; its `class … extends Error` hierarchy is the M2 blocker after
that. The measurement first exposed a false `source complexity` rejection: the
complexity pre-pass lexed template text after a substitution, and the doc comments
after it, as code. The integration branch fixes the pre-pass to rescan template
text like the parser.

**Verified behavior with v31 (2026-10-28).** Named interface/alias types,
type-only imports/exports and `./x.js` specifiers are implemented, so
`core/json.ts` links through `import type { JsonValue } from './core/json.js'`.
After the 2026-10-28 source-limit change (no per-file token count; flat alias
unions and list commas no longer count as chains), `generated/models.ts` parses
and its local `interface Error` model shadows the built-in type name. It now stops
at the first top-level `export const … as const`; literal union types (up to
hundreds of members) are the following blocker.

**Earlier verified behavior (2026-10-28).** `varta check` with profile
`checked-f64-utf16-v30` rejects `import type` (`core/resource.ts`), top-level
`export const` (`generated/meta.ts`), `class` (`core/errors.ts`) and any *use* of an
interface whose field has a string-literal union type. A file containing only type
aliases passes when nothing uses them. No file of the package compiles as part of
its module graph.

| Requirement | Occurrences | Current Varta |
| --- | --- | --- |
| Modules: `import type`, `export *`, `export default`, top-level `export const`, `.js` specifiers resolving to `.ts` | 65 files | v31: type-only imports/exports and `.js` specifiers; v32 (unreleased): `export const`, `export { … } from`, `export *` and `export default`; the 65-file graph exceeds the module and import-declaration limits |
| Named interfaces/aliases; structural assignability; index signatures and `Record<string, …>` | 1917 interfaces / 38 | v31: named closed shapes; v32 (unreleased): `interface … extends` and merging, literal-keyed `Record`; no width subtyping, index signatures or `Record<string, …>` |
| String-literal types and `as const` | 357 aliases / 357 | v32 (unreleased): literal types and their unions, `as const` on whole initializers |
| Records wider than 32 fields (`Agent` 55, `UpdateRuntimeConfigRequest` 60) | 5 | 32-field limit |
| Classes: `extends`, `protected`, `#private`, getters, `Error` subclasses, `new.target` | 74 classes, 82 `#` members | v32 (unreleased): top-level classes with `extends`, `protected`/`private`, getters and static methods, `Error` subclasses and `new.target.name`; v34 (stream branch): `cause?: unknown`; `#private` not supported |
| Generics: `request<T>`, `pick<T, K extends keyof T>` | 13 in core | v32 (unreleased): generic interfaces, aliases and top-level functions by monomorphization with `keyof`, `T[K]` types and inference; generic methods/classes, async generic functions and computed access with a `keyof` key not supported |
| `await` / `for await`; async generators, `Symbol.asyncIterator`, `new Promise` | 44 / 22 | Not supported ([suspension design](#k44--suspension-scheduling-and-async-planned)) |
| Template literals, optional chaining `?.`, object spread | 537 / 27 / 48 | v32 (unreleased): untagged template literals; `?.` and object spread not supported |
| Already supported: `??` (52), default parameters, `Math.random` (6) | — | v12+, v12+ and v17+ respectively |
| Type assertions `as T`, non-null `x!`, `unknown` | ~18 / ~4 / 149 | v34 (stream branch): `unknown` cells with guard narrowing, `unknown` catch bindings and Error causes, runtime-checked `as`/`!`; `JSON.parse(text) as T`, `x is T` predicates and `in` narrowing not supported |
| fetch, Headers, Response, AbortController, setTimeout, ReadableStream, TextDecoder, FormData, Blob, URLSearchParams, crypto, Date, Set | core | Only `net.get(url): string` |

About 52 KB of unique string literals (API paths, messages) appear in the source;
the share that survives type erasure must be measured against the 64 KiB static
data limit before M3.

**Research points exercised by the pilot.**

- *Exact capabilities.* The SDK needs network access to one base URL and reads of
  `UARP_API_KEY`, `UARP_BASE_URL` and `SNAGA_API_KEY`; its manifest should contain
  exactly these grants. `Math.random` needs no grant today. `crypto.randomUUID`/
  `getRandomValues` have no host operation yet; whether entropy stays ungated or
  gets an explicit capability is an open decision.
- *Checked assertions over JSON (proposal).* `decodeBody` returns
  `JSON.parse(text) as T`, an unchecked assertion in Node. Varta owns its lib
  declarations: `JSON.parse` could return `unknown`, removing the library-level
  `any`, and `as T` could compile to a validator generated from the target type,
  so the unchanged SDK validates server responses. This would deliberately
  deviate from Node (an assertion could throw) and needs its own profile contract
  before adoption. For a generic `T` it needs the type at each call site, which
  favours monomorphization.
- *Declaration merging.* `index.ts` combines `interface UarpClient extends
  Resources {}` with `class UarpClient` and `Object.assign(this, …)`. The shape is
  fully described by types, but this is the package's least conventional pattern.

**Ladder.**

| Step | Compiles | Requires | CORE |
| --- | --- | --- | --- |
| M1 | `models.ts`, `meta.ts`, `json.ts` | Type-only imports/exports, top-level `export const`, string-literal types, `as const`, index signatures, recursive `JsonValue`, records wider than 32 fields | 01, 02, 12 |
| M2 | `errors.ts` | Classes, `extends Error`, `super(message, { cause })`, `new.target`, class fields | 02, 03, 04 |
| M3 | `resource.ts` and all `generated/resources` | Inheritance, `protected`, template literals, `encodeURIComponent`, module graph at 65-file scale | 02, 12 |
| M4 | `util.ts` | Generics with `keyof`, Date, crypto, Set | 03, 05, 06 |
| M5 | `transport.ts`, `pagination.ts` | async/await, async generators, `for await`, `#private`, fetch/Headers/Response/AbortController/setTimeout | 08, 09, 10, 11 |
| M6 | `sse.ts` | ReadableStream, TextDecoder, `Symbol.asyncIterator`, `new Promise` | 08, 09, 10 |
| M7 | `index.ts` and the SDK's own tests | Declaration merging, `Object.assign(this, …)`, running the package tests under Varta | 02, 15 |

M1–M3 do not depend on asynchronous execution and can proceed alongside the
suspension foundation. A step closes only when its files compile from the pinned
sources, emitted modules validate in Wasmtime and the behavior is covered by tests;
unsupported constructs keep failing with clear diagnostics. Measurements for the
research record: share of files and SDK tests passing, artifact size, startup time
and memory compared with Node, Deno and Bun.

### Pilot batch v32 (in development)

Profile `checked-f64-utf16-v32` collects pilot M1–M3 features developed in
parallel, one work stream per selector. Each stream landed on its own branch with
its own tests, contract subsection and profile rules. All nine streams are merged
on the `v32-integration` branch in two waves together with their cross-stream
integration (constants, classes, generic functions and Error classes through
re-exports and default exports; literal-typed class fields and template
substitutions; class bodies reading constants; generic interfaces extending
generic bases; Error-class instances through generic functions and suspension
frames) and one conformance case per stream that a single-file case can test.
The second wave also merges the compiler-internal K4.4 suspension foundation.
V32 is not released: the integration branch awaits review and its merge into
`main`.

| Selector | Work stream | Pilot step | State |
| --- | --- | --- | --- |
| `module_constants()` | Top-level `const`/`export const` with checked initializers and `as const` | M1 | Merged on the integration branch |
| `literal_types()` | String/number/boolean literal types and wide literal unions | M1 | Merged on the integration branch |
| `classes()` | Classes, fields, constructors, methods, `extends` | M2 | Merged on the integration branch |
| `module_reexports()` | `export { x } from`, `export * from`, `export type { T } from` | M3, M7 | Merged on the integration branch |
| `template_literals()` | Template literal expressions | M3 | Merged on the integration branch |
| `generics()` | Generic type declarations and functions by monomorphization, `keyof` | M1, M4 | Merged on the integration branch (second wave) |
| `default_exports()` | `export default` and default imports | M7 | Merged on the integration branch (second wave) |
| `interface_extends()` | `interface … extends` and declaration merging of interfaces | M1, M7 | Merged on the integration branch (second wave); interface/class merging stays rejected |
| `error_subclasses()` | `class X extends Error`; exception channel v5 | M2 | Merged on the integration branch (second wave) |

### Pilot batch v34 (in development)

Decided with the project author on 2026-10-29: the next pilot batch collects
uarp-sdk M1–M2 remainders together with K4.4 steps 3–4 in profile
`checked-f64-utf16-v34`, developed in three parallel streams. V34 inherits every
v33 feature and declares its own host ABI, because the async stream's runtime
scheduler calls `varta_resume`, and its own value ABI for the record and
promise layouts of the records and async streams
([contract](architecture.md#checked-f64-utf16-v34--pilot-batch-dynamic-records-unknown-and-async)).
The records stream switched v34 to value layout 15 first
([contract](architecture.md#value-layout-15-dynamic_values)): value15 keeps
every value13 layout behind its own value15 readers, JSON schema 15 and
exception channel v6, and v34 programs behave exactly as under v33. The three
streams are merged on the `v34-integration-merge` branch, where `unknown` reads
value15 cells (including dictionaries and wide records), `as Record<string, T>`
is a checked target for primitive `T`, promise records and async frames run
under value15 descriptors and recursive values may be held in frames
(`varta-runtime` `v34_cross_stream_tests`). Pending: `JSON.parse` and
`varta.json.parse<unknown>` producing `unknown` (they need an internal
recursive JSON value type), narrowing `unknown` to dictionaries by
`typeof`/`in` and `await` inside dictionary reads and writes (an intrinsic value
block). The host memory retained by unhandled rejection reports is now bounded
by named constants for every scheduler profile (decided on the `v35-frames`
branch: it changes only the diagnostic text, see
[bounded reports](architecture.md#microtask-scheduler-promises)).

| Selector | Work stream | Pilot step | State |
| --- | --- | --- | --- |
| `dynamic_values()` | Records: value layout 15 foundation, value15 readers | M1 | Merged on `v34-integration-merge` |
| `index_signatures()` | Records: `{[k: string]: T}` and `Record<string, T>` as a dynamic-key record kind (value15 tag 13) | M1 | Merged on `v34-integration-merge` |
| `recursive_types()` | Records: recursive aliases and interfaces such as `JsonValue` and trees | M1 | Merged on `v34-integration-merge` |
| `wide_records()` | Records: records wider than 32 fields, up to `MAX_WIDE_RECORD_FIELDS` (multiword presence, descriptor edge bitmap) | M1 | Merged on `v34-integration-merge` |
| `unknown_type()` | Unknown: `unknown` cells, `typeof`/`===`/`instanceof` narrowing, caught values and `cause?: unknown` | M1, M2 | Merged on `v34-integration-merge` |
| `checked_assertions()` | Unknown: `x as T` and `x!` as runtime-checked conversions | M1, M2 | Merged on `v34-integration-merge`; arrays of non-primitive elements are not checkable assertion targets |
| `promises()` | Async (K4.4 step 3): microtask queue and scheduler, base `Promise<T>` semantics | M5 | Merged on `v34-integration-merge` |
| `async_functions()` | Async (K4.4 step 4): `async`/`await` on continuation frames | M5 | Merged on `v34-integration-merge` |

### Pilot batch v35 (in development)

Decided with the project author on 2026-10-29: the next pilot batch targets the
first diagnostics of the [v34 measurement](#pilot-acceptance-package-uarp-sdk)
(`Headers` in 60 files, `AbortSignal` in `core/util.ts`, then the mixed
`ProblemDocument` object type) together with the async and frame follow-ups, in
profile `checked-f64-utf16-v35`, developed in four parallel streams. V35
inherits every v34 feature and, at registration, shares host ABI 30 and value
ABI/layout 15 with v34; the declared profile name alone makes runtimes that do
not list v35 reject its modules. A stream that adds a host import or a value tag
bumps the corresponding ABI for v35 itself
([contract](architecture.md#checked-f64-utf16-v35--pilot-batch-web-types-mixed-records-async-methods-and-frame-liveness)).
The async stream switched v35 to value layout 16 (value15 plus the promise
tag) with host ABI 31 for its value16 readers
([contract](architecture.md#value-layout-16-promise_values)); the records and
Web streams run on that layout and JSON schema 16 without a further tag,
layout, host import or schema version, and every v35 frame uses the frames
stream's liveness layout, including async closure bodies, whose captured cells
are shared state-0 slots. The four streams are integrated on `v35-merge`.

Pending in the records stream: spreads without a dictionary context and into
closed records, numeric index signatures, class index signatures, declared
fields that are not checkable from the index type (for example `items: Item[]`
next to `[key: string]: unknown`), mixed records nested inside checked-parse
targets, `x.key` on `object` without an `in` guard, a null test in an earlier
statement than the `typeof` test (`if (x === null) return;` keeps
`object | null`), and `JSON.stringify` of `unknown`-holding values in programs
that declare Error classes (the v32 rule). Pending in the Web stream: abort
event listeners, because a signal would hold listener functions, which needs a
function-valued object field in the value layout.

| Selector | Work stream | Pilot step | State |
| --- | --- | --- | --- |
| `web_headers()` | Web: engine-provided typed `Headers` class, guest-implemented, no capability | M2, M5 | Implemented (prelude; snapshot iteration, no tuple types) |
| `abort_signals()` | Web: `AbortController`/`AbortSignal` data model, no timers | M4, M5 | Implemented without listeners (pending a value-layout change), `timeout` or `any` |
| `mixed_index_records()` | Records: declared fields combined with a string index signature; `for…in`, compound assignment and spread on dictionaries | M1, M2 | Implemented: mixed records with checked declared reads, `for…in`, compound/logical/update assignments, spreads, number keys, `Record<string, T>` inference, shared dictionary operations, optional and redeclared class fields |
| `unknown_json()` | Records: `JSON.parse`/checked parse producing `unknown`; narrowing `unknown` to dictionaries | M1, M2 | Implemented: `JSON.parse` and `parse<unknown>`, `JSON.parse(text) as T` as checked parse, descriptor-checked dictionary assertions, `object` narrowing and `in` guards |
| `async_methods()` | Async: async class methods, async arrow functions and async function expressions | M5 | Implemented; generic methods stay rejected with all generic methods |
| `promise_values()` | Async: promises as tagged values (arrays, unions, `unknown`) and `Promise.all/allSettled/race/any` | M5 | Implemented (value ABI 16, host ABI 31); no `AggregateError` class, `Promise.withResolvers` and executor `resolve(promise)` not admitted |
| `frame_liveness()` | Frames: liveness-based continuation frame slots (v35 frame layout only) | M5 | Implemented ([contract](architecture.md#frame-liveness-frame_liveness)): per-state saved sets, direct liveness segments with exact per-state GC maps, frame catalog version 3; about half the fuel per `await`/`yield` of v34 |

### Pilot batch v36 (in development)

The next pilot batch targets the remaining diagnostics of the
[v35 measurement](#pilot-acceptance-package-uarp-sdk) (declared fields of the
mixed `ProblemDocument` that are not checkable from its index type, and the
defaulted function-typed parameter of `streamBackoffDelay`), together with the
v35 follow-ups that need function-valued fields (abort listeners), async
generators and timers (`AbortSignal.timeout`, backoff delays), in profile
`checked-f64-utf16-v36`, developed in four parallel streams. V36 inherits every
v35 feature and declares its own host ABI 32 (`abi::TIMERS_VERSION`), under
which every host import of the batch is added, and its own value ABI/layout 17
(function values as tagged values, introduced by the function-fields stream);
a v36 module relabelled as v35 fails at its host ABI section, and runtimes that
do not list v36 reject its declared profile name
([contract](architecture.md#checked-f64-utf16-v36--pilot-batch-checked-declared-fields-function-fields-async-generators-and-timers)).
The functions stream is implemented; the other selectors admit nothing yet.
With it, `core/util.ts` passes its v35 blocker (the defaulted
`random: () => number = Math.random` of `streamBackoffDelay`, line 125) and the
abort-listener and optional-chain code of `sleep`; its next blockers are
`setTimeout`/`clearTimeout` (timers stream), a closure capturing a `const`
declared after it (`onAbort` in `sleep`, a TDZ capture), `globalThis.crypto`
with `Uint8Array`, `TextEncoder` and regular expressions.

| Selector | Work stream | Pilot step | State |
| --- | --- | --- | --- |
| `checked_declared_fields()` | Records: mixed-record declared fields checkable from `unknown`, such as arrays of optional records | M1, M2 | Registered, admits nothing |
| `function_fields()` | Functions: function-valued record and class fields; optional and defaulted function-typed parameters (owns the value17 bump) | M2, M4 | Implemented ([contract](architecture.md#function-fields-function_fields)): value layout 17, function fields/elements/unions/`unknown`, optional and defaulted function parameters, optional chains, `Math.random` as a value, abort listeners, `Promise.withResolvers`; `resolve(promise)` and checked `as` to function types pending |
| `async_generators()` | Async: `async function*` generators and `for await…of` | M5 | Registered, admits nothing |
| `timers()` | Timers: `setTimeout`/`clearTimeout` and timer-driven promises (host ABI 32 imports) | M4, M5 | Registered, admits nothing |

### Deferred external validation scenarios

| Profile | Example contract operations | Required behavior |
| --- | --- | --- |
| Desktop editor | Inspect a document, rename or patch it, update selection | Connect to the existing application instance; preserve revisions, history/undo and UI consistency |
| Data service | Find records, change an allowed business status | Enforce identity/data scopes, transactions, bounded queries and stable errors |
| Robot software | Read telemetry, start a mission, request a stop | Validate state/command limits; define duplicate, timeout and disconnect behavior; keep the local safety controller independent of AI transport |

A **headless document application** was selected as a possible later shared-state
demonstration: a title, revision and small command/query interface, controlled by
two test clients and then an external MCP client. It is deferred behind the active
core priorities and is not the next implementation task. It exercises shared state and conflicting changes
without requiring a GUI toolkit, database service or physical hardware. This is
a future external validation scenario, not a commitment to a GUI framework.
It must be built as a separate TS/JS application on public Varta APIs, with its
own source, configuration and domain rules. Choosing this demonstration does not
authorize adding document models, title rules, rename commands or application
roles to `varta-common`, `varta-runtime`, `varta-project` or the core CLI.
## Delivery sequence

Core language/runtime capabilities now take precedence over the remaining
contract-platform work. Initial K1–K3 and K4.1 profiles below are implemented;
K4.2's number/aggregate/JSON increments are implemented; remaining value work is active.
C1 and C2.1 are completed
foundations; the unimplemented portions of C2–C5 are deferred. Work-package names
are not implemented commands or a frozen public API.
### K1 — Structured guest values and JSON (checked profile implemented)

- Specify a checked data profile before emission: typed record literals, named
  field access, nesting, optional/null semantics and collection limits. Begin
  with the closed record shapes needed to construct ordinary JSON responses;
  broader arrays and dynamic JSON are separate incremental extensions.
- Define representation, aliasing and ownership for records and contained strings.
  Extend roots/tracing or choose a validated bounded representation explicitly;
  tracing must retain nested objects/strings through aliases and field writes.
- Implement guest-visible JSON serialization with correct escaping, UTF-8 and
  encoded-byte/depth limits. A record must serialize as a JSON object, not as a
  quoted JSON string or the tool protocol's `{value: ...}` envelope.
- Add schema-checked JSON parsing incrementally before accepting structured
  request bodies. Define duplicate/unknown/missing fields and errors explicitly.
  JSON number handling must agree with the supported numeric profile; never
  silently round or truncate unsupported external values into i32.
- Preserve identical check/build/dev acceptance and test actual Wasm behavior,
  malformed input, escaping, retained nested values and resource exhaustion.

Implemented first increment: closed structurally typed records containing i32,
boolean, UTF-8 string, null and nested record fields can be constructed, passed,
returned, aliased, mutated and serialized in compiled Wasm. Object identity and
source evaluation/key enumeration order are preserved. The collector traces
record edges, keeps live strings/objects in place and reuses dead storage.
`JSON.stringify` uses a versioned descriptor ABI with checked memory, escaping,
cycle/depth/node/output limits and no added system authority. Compiler negatives,
hostile ABI data, repeated collection, effect catalogs and CLI dev/Wasm/native/
named invocation exercise the same path. No application model was added.

Implemented second increment: homogeneous dense arrays support `T[]`/`Array<T>`,
contextually typed empty literals, numeric indexing/replacement, `.length`,
nesting, identity and parameter/result transfer. The collector traces every array
reference, including beyond 32 elements. JSON checks array headers, kinds, ranges,
cycles and shared traversal/output budgets. The profile is capped at 1024 elements
per fixed-length array and 128 distinct array element types. Out-of-range indices
trap; sparse/resizable arrays remain pending. Undefined values are available in
the separate K4 v2 binary64 profile. Structured-value ABI
v2 adds arrays while the runtime continues to accept v1 records/scalars without
reinterpretation. Host ABI, artifact and primitive tool versions are unchanged.

Implemented third increment: `varta.json.parse<T>(text)` compiles a bounded closed
schema, checks complete JSON input and creates ordinary owned values using the
same guest allocator/root stack. It rejects duplicate/unknown/missing fields,
wrong types, unsupported number forms and budget violations. Parsed record order
is normalized by schema; this is a checked Varta extension, not the dynamic
ECMAScript JSON.parse API. Invalid input traps and named dispatch reports
`execution_failed`; system status is preserved. Runtime validation checks helper
signatures, transitive host-free call graphs and schema/descriptor consistency,
including modules without a tool catalog. Source-free Wasm/native artifacts use
the same parser and existing permissions. Host/value/container versions remain
unchanged; parsing has its own schema format version 1 and new registered import.

K1's initial checked data path is implemented. K2 below now provides its first
HTTP request/response, handler and lifetime profile.
`undefined`, optional/dynamic properties, general JSON.parse/revivers, recoverable
parse-result APIs, prototypes/hooks and full JS Number/UTF-16 semantics remain
broader language/runtime work. Tool Contract v1 signatures remain primitive;
structured wire contracts need their own explicit version.
### K2 — Generic server lifecycle and inbound HTTP (first profile implemented)

- Specify bootstrap/handler registration, application lifetime, request lifetime,
  shutdown and ownership. A long-lived listener must not be treated as one guest
  call that consumes the current five-second execution deadline forever.
- Start with checked named synchronous handlers if that is sufficient. A host-driven
  event loop can dispatch bounded guest calls without first promising general
  JS promises, closures or arbitrary async semantics. Freeze the callback/request/
  response ABI and error behavior before implementation.
- Provide generic guest-facing server primitives and typed request/response data.
  The developer writes routes and payload construction in TS/JS. Core code does
  not contain application endpoints or example response schemas.
- HTTP handler registration is independent of `varta.tool` and MCP. Ordinary
  server handlers need no AI-tool declaration and are not published to agents
  automatically. Reuse checked marshalling/execution mechanisms without making
  the tool catalog the mandatory entry point for all applications.
- Introduce an actual listener capability with validated address/port scopes and
  package/host conjunction before binding a socket. Outbound grants must never
  authorize listening. Do not revive an unused port-list placeholder as authority.
- Use a maintained HTTP implementation rather than hand-written framing. Bound
  connections, headers, bodies, responses, pending work, per-request fuel/memory/
  deadlines and aggregate work. Define disconnect, malformed request and trap
  isolation, backpressure and descriptor cleanup.
- Separate normal HTTP response bodies from tool invocation/MCP envelopes. Test
  real loopback requests with hermetic listeners and verify denied binds before
  effect. Keep HTTPS support explicit and separate from an initial HTTP profile.

Exit: a compiled TS fixture starts a permitted listener through Varta's API,
returns a real JSON object with a JSON content type, handles repeated requests,
isolates failed handlers and shuts down cleanly. No application-specific host
wrapper or MCP connection is required.

Implemented: `varta.http.serve({hostname,port}, handler)` is an entry-module
compile-time declaration, with typed `VartaHttpRequest` / `VartaHttpResponse` and
no required tool publication. The compiler generates an isolated string adapter
using real checked JSON/record mechanisms. Versioned HTTP metadata advertises the
handler, exact bind endpoint, effects and fixed limits. Its feature import makes
older runtimes fail explicitly. Runtime validation recomputes effects and checks
host-free transfer helpers before execution.

A permitted listener starts after bounded bootstrap. It outlives bootstrap's
five-second deadline; each request receives a fresh bounded store. Hyper/Tokio
own framing/timers. The first profile serializes connections, closes after one
reply, caps headers/body/envelopes, rejects unsupported framing/response headers,
isolates handler failures and supports owned graceful shutdown. Readiness/logs
stream through runtime events and the thin CLI. Loopback tests cover JSON bodies,
headers, repeated requests, slow input, malformed framing, fuel traps, scope
conjunction before bind, shutdown, and source-free Wasm/native artifacts.

Remaining extensions: concurrent/streaming requests, persistent guest state,
standard Request/Response classes, async JS, keep-alive and TLS/HTTP2. Authentication
and domain routes are application responsibilities. The checked HTTP acceptance
path and K3 runtime-only packaging are implemented. No separate application
project has been added.
### K3 — Runtime-only packaging and independent core validation (implemented)

- `varta-artifact` owns portable Wasm/manifest and fixed-footer container I/O,
  independent of VM and source tooling. The project re-export remains available;
  I/O errors now use shared `VartaError`. Existing VARTA3 layouts are preserved.
- `varta-launcher` builds `varta-runtime`, with no production dependency on the
  TS compiler, SWC, project tooling or development CLI/parser. It executes artifacts,
  invokes checked tools, inspects metadata and enforces optional host ceilings.
- `build --single` requires a sibling companion or explicit trusted `--launcher`.
  A private snapshot negotiates strict version/ABI/native-target compatibility
  and validates the payload without executing guest code before outputs are written.
  Probe output/time budgets, rejection of packaged candidates and preservation
  of existing output on preflight failure have regression coverage.
- Core dependency/public-API tests remain; CI builds the launcher independently
  and runs packaging regressions on Linux/macOS. Local validation is on macOS;
  Windows, cross-target packaging, release installers and signatures remain pending.
- K2 regression compiles/packages the HTTP fixture, removes source/configuration,
  deploys only the native executable in another directory and serves JSON with
  owned shutdown. Both standalone runtime and native self-execution are covered.
  Tool invocation/inspection also work source-free through the independent launcher.

This completes the first core HTTP/distribution acceptance path, not general
JS/TS compatibility. The user's current priority remains the core; no separate
application project has been added.
### K4 — ECMAScript value profile and language expansion
#### K4.1 — Profile negotiation and migration contract (implemented)

New compiler output declares `checked-i32-utf8-v1` in strict bounded
`varta.language` metadata. Runtime validation rejects unknown/malformed/duplicate
profiles before native compilation or execution; inspection distinguishes declared,
legacy ABI v1 and unspecified raw Wasm. Older ABI v1 artifacts preserve current
semantics. The runtime companion's protocol v2 reports supported profiles, which
packaging checks on its private snapshot. The physical ABIs/container do not change.
Regression tests cover both profile origins, existing arithmetic/string/identity
behavior, traps, unsupported source values, profile corruption and offline discovery
through runtime, CLI and native artifacts. This is a compatibility foundation;
K4.2 below adds opt-in binary64 with checked aggregates/JSON and a further
undefined-capable profile. A further UTF-16 profile is implemented; dynamic object behavior remains pending.
#### Target value semantics (compatibility target)

The pinned reference is **ECMAScript 2025, ECMA-262 16th edition**. The proposed
`ecmascript-2025-v1` name is reserved for the future value-semantics profile; it is
deliberately absent from the supported profile registry. It does not mean all of
ECMAScript 2025 or web/Node/npm APIs are available. Publish a feature/conformance
matrix as each part becomes executable; do not infer support from the profile name.

| Value area | Required future behavior |
| --- | --- |
| Number | IEEE-754 binary64, preserving signed zero, NaN and infinities; floating-point arithmetic, remainder and division; no i32 wrapping for ordinary Number operations |
| String | UTF-16 code-unit values, including lone surrogates; code-unit length/indexing and lexical comparison; no implicit Unicode normalization |
| Null/undefined | Distinct values/tags; void calls and missing properties can produce undefined; neither is integer zero |
| Identity | Object/array aliases preserve identity through mutation and tracing; primitives compare by their defined value rules |

These are target requirements, derived from the pinned
[ECMAScript 2025 value types](https://tc39.es/ecma262/2025/multipage/ecmascript-data-types-and-values.html).
Internal storage may use f64 or an explicitly specified tagged representation, but
must not lose signed zero or conflate NaN payload bits with heap references. The
collector traces actual reference tags, never arbitrary number/string bits. Object
property slots and array elements must preserve the same value distinctions.

Strict equality must return false for different value types and NaN, and treat
the signed zeroes as equal. Object equality uses identity. `Object.is`/SameValue
(v30) equates NaNs and distinguishes signed zeros; SameValueZero keeps NaNs
equal while joining signed zeros. Loose equality, relational operations,
truthiness and string/number conversion must follow the specified conversion order,
including short-circuit operand values and observable left-to-right effects.
Accepted mixed-type expressions must not reuse today's same-type comparisons.
See the pinned [abstract operations](https://tc39.es/ecma262/2025/multipage/abstract-operations.html).
Until an operation exists, the compiler must reject it explicitly. TypeScript
annotations may restrict admission but must not change the meaning of an accepted
JavaScript value/operation.

Standard JSON must preserve the specified distinctions: non-finite numbers become
JSON null, negative zero serializes as zero, undefined object fields are omitted
and undefined array elements become null; top-level stringify may return undefined.
Surrogates need well-formed escaping. Generic parsing needs binary64 numbers and
ordinary JSON object behavior. See the pinned
[JSON algorithms](https://tc39.es/ecma262/2025/multipage/structured-data.html#sec-json-object).
The existing `varta.json.parse<T>` is a separate strict schema API: duplicate/unknown
field rejection, integer-only schemas and bounds remain versioned contracts. Do
not silently turn it into unrestricted `JSON.parse` or weaken AI input validation.
Numeric-index property ordering, insertion order of other string keys, symbols,
holes and out-of-range array reads need their standard semantics when admitted;
the current closed-record/fixed-array restrictions are not the final JS model.
The property-order reference is
[OrdinaryOwnPropertyKeys](https://tc39.es/ecma262/2025/multipage/ordinary-and-exotic-objects-behaviours.html#sec-ordinaryownpropertykeys).
#### K4.2 — Value/ABI design and implementation (in progress)

The first scalar increment is implemented as **`checked-f64-utf8-v1` / host ABI v2**.
Project `languageProfile` and explicit compiler profile APIs select it for a whole
source graph; existing APIs/configurations default to the old profile. Number
parameters/locals/results use separate f64 slots, captured Number results use
`(f64,i32)`, and UTF-8 string handles retain exact roots. There is no NaN boxing or
dynamic Number heap layout. Host ABI v2 adds typed numeric logging, remainder and
exact checked process-exit imports; old pointer/i32 imports retain their meanings.
Artifact readers now validate manifest/code ABI agreement using the existing pinned
wasmparser dependency, without adding a VM/compiler dependency to distribution I/O.

Tests cover fractional/large/subnormal values, signed zero, NaN/infinity,
division/remainder, comparisons/truthiness, short-circuit effects, switch/loops,
function/module slots, captured status and collection with live strings. Public
engine APIs, dev, portable Wasm, independent runtime and native packages execute
the new path. Downgraded/missing metadata, incorrect signatures, legacy aggregate
metadata and manifest mismatches fail before execution. Permission denials and
host ceilings remain enforced. This is not full ECMAScript compatibility.

The aggregate/JSON increment is also implemented: **value ABI v3 / schema v2**,
with Float64 tag 6, raw numeric bits in eight-byte slots, reference-only tracing
and exact checked array indices. Separate JSON v3 imports bind the layout and
return full i64 parse payloads; allocator/retainer signatures and host-free effect
checks remain mandatory. Strict finite decimal/exponent parsing uses the existing
serde_json parser with round-trip conversion enabled. ECMAScript numeric text
uses exact-pinned ryu-js 1.0.3; -0 serializes to 0 and non-finite values to null.
Mixed metadata, descriptor/schema disagreement and unimplemented tool/HTTP
publication fail explicitly. Legacy i32 schemas/layouts retain their semantics.

Runtime/public-API/dev/native tests cover mutation/aliasing, signed zero, IEEE
rounding, formatting thresholds, partial-input rejection before allocation,
collection of nested reference graphs, incorrect indices, helper effects and
permission/host-ceiling preservation. Inspection reports the actual value ABI;
companion negotiation advertises ABI maxima and probes the concrete payload.

The undefined increment is implemented in **`checked-f64-utf8-v2` / host ABI v3 /
value ABI v4 / schema v3**. Undefined tag 7 has a canonical zero payload and is
separate from null. Void expressions/calls, explicit undefined annotations,
parameters/returns, typed typeof and nullish comparisons preserve effect order.
JSON v4 omits undefined object fields, emits null for array entries and returns
actual undefined at the root. Canonicality and traversal limits also apply to
omitted fields. Typed JSON parsing cannot target undefined or missing fields.
New imports/metadata prevent old runtimes from reinterpreting the representation;
all previous profiles remain available. Tests cover lexical shadowing/TDZ, void
captures/status, GC, malformed payloads/versions, public APIs and source-free native
packages under package/host authority.

The UTF-16 increment is implemented in **`checked-f64-utf16-v1` / host ABI v4 /
value ABI v5 / schema v4**. Guest strings store UTF-16LE byte slices, preserving
lone surrogates through literals, concatenation, calls, aggregates and tracing.
Length/charAt/charCodeAt and unsigned code-unit comparisons use the pinned String
semantics for admitted numeric arguments. Separate string/JSON imports prevent
legacy UTF-8 reinterpretation. JSON escapes lone units and strict parsing retains
them before guest allocation. Resource names reject lone units and NUL before OS
effects; well-formed UTF-8 host results convert exactly to owned UTF-16. Physical
and converted byte limits remain bounded. Record keys/schema metadata retain
UTF-8; surrogate keys and general string bracket access are not yet admitted.
Tests round-trip every u16 and cover effect order, GC pressure, real system I/O,
permissions/host ceilings, malformed encodings/versions, public engine APIs and
source-free native execution. No application project is introduced.

The primitive-union increment is implemented in **`checked-f64-utf16-v2` / host
ABI v5 / value ABI v6 / schema v5**. Closed primitive annotations use immutable,
canonical tagged cells with exact string-only child tracing. Initialization,
reassignment, function boundaries, record fields and explicitly typed array
elements preserve actual types and full Number bits. Direct-local typeof/null/
undefined guards narrow branches; writes under refinement are rejected to keep
loop assumptions sound. Mixed primitive ternaries, truthiness, strict equality,
nullish-only loose equality and short-circuit union operators execute in source
order. JSON handles dynamic undefined fields/entries/roots; typed parsing remains
explicitly non-union. Version/import/cell validation, GC pressure, malformed
payloads, module/public APIs, source-free execution and unchanged system authority
are tested. Older profiles retain their contracts.

The missing-value increment is implemented in **`checked-f64-utf16-v3` / host
ABI v6 / value ABI v7 / schema v6**. Closed optional primitive fields retain
separate presence and insertion-order metadata; explicit undefined stays an own
property. Object.hasOwn, statically absent record reads, nullable numeric reads of
primitive arrays/UTF-16 strings and undefined-capable let/return initialization
use real values and exact roots. Inherited optional names and reference-element
reads are rejected until their semantics are supported. Assignment appends newly
present declared fields only after RHS success. JSON validates required/absent
slots and full trailers, preserves enumeration and counts omitted slots against
budgets. Strict parsing remains non-optional/non-union and creates fully present
required records in the new layout. Tests cover 32-field masks, malformed metadata,
source order, declaration/return rules, collection pressure, permissions, public
module APIs and native source-free artifacts. Earlier profiles remain frozen.

The reference-union increment is implemented in **`checked-f64-utf16-v4` / host
ABI v7 / value ABI v8 / schema v7**. One record shape and one array element type
may join primitives in a checked union. Tag 9/typed v2 value imports validate
canonical reference cells and shallow managed layouts without reinterpreting
primitive tag 8 or old imports. Identity, aliases and exact tracing survive
boxing, optional fields and nullable object/array-element reads. typeof object
retains Null/Record/Array, while Array.isArray and null/undefined guards recover
known types in branches. Contextual literals/function returns remain supported.
JSON recursively serializes references, rejects cycles and keeps omission/budget
rules; strict parsing remains non-union/non-optional. Tests cover malformed cells,
source order, guards, GC pressure, independent/public/native execution and system
authority. Existing profiles retain their contracts.

The checked-JSON increment is implemented in **`checked-f64-utf16-v5` / host
ABI v8 / value ABI v8 / schema v8**. `json_parse_v9` supports optional fields and
closed primitive/reference unions while retaining `json_stringify_v8`. A union
selects its unique JSON-kind member without coercion; missing optional fields
retain clear presence bits and canonical undefined slots. Required fields still
must be present even when admitting undefined. Strict backward schemas, unique
member kinds and descriptor optional flags are checked before guest allocation.
Parsing preserves finite binary64 bits and lone UTF-16 units, exact cell/child
roots and alias identity. Duplicate/unknown keys, invalid members and resource
budgets remain enforced. Tests cover malformed metadata/helpers, no partial
allocation, collection, module APIs, source-free artifacts and host ceilings.
Earlier profiles remain frozen; no application logic or new capability is added.

The resizable-array increment is implemented in **`checked-f64-utf16-v6` / host
ABI v9 / value ABI v9 / schema v9**. Stable array identity owns replaceable,
geometrically grown dense storage; push evaluates all arguments before appending,
and pop returns the actual removed value or undefined while clearing its slot.
Aliases and parsed arrays share changes across records/functions/modules. GC
traces the identity's sole backing edge and every live reference element, including
arrays beyond 32 slots; returned children survive collection after removal.
Versioned v3 value operations/JSON imports validate cell/storage agreement and
canonical spare slots. The current limit remains 1024; indexed writes are bounded
replacements, with length assignment/holes/indexed extension still deferred.
Tests cover effects, union/reference values, malformed layouts/versions, resource
bounds, GC, public APIs, source-free packages and host ceilings. No application
logic or new capability is added; older profiles keep inline fixed arrays.

The same v6 profile now also implements `at` and `slice` as additive compiler
operations over ABI/layout v9. Number/Undefined bounds retain optional union
values and source-order effects; fractions truncate, NaN maps to zero and negative/
infinite/outside bounds follow the checked dense-array algorithms without integer
conversion traps. At returns the original element or Undefined. Every slice has
fresh storage/identity and shares nested object/array references. Selected slots
and actual reference masks are copied, spare slots are zero, and the new backing
is retained before identity allocation can collect. Tests cover edge cases,
mutating arguments, GC, the complete helper dependency set, pure status, public module
APIs, source-free execution and host ceilings. No import, capability or versioned
representation changed; earlier profiles retain their admitted behavior.

The same v6 profile now implements `includes`/`indexOf` over ABI/layout v9.
A shared allocation-free search helper compares actual kinds/payloads, UTF-16
contents and underlying reference identity. Includes uses SameValueZero; indexOf
uses strict equality. Needle/Number-or-Undefined starting bounds preserve source
order and exact roots through later allocations, including detached pop results.
Tests cover numeric edge cases, mixed values, optional bounds, GC, all ten helper
dependencies, public APIs, source-free artifacts and permission/status preservation.
No capability, import or representation changed; earlier profiles are unchanged.

Next specify and implement wider dynamic values and external adapters. Broader
shape unions, sparse collections, further array operations, prototype values and
general coercion still need explicit semantics and layouts. Tool/HTTP publication
in the newer profiles remains pending;
no legacy field/schema/handler is reinterpreted.
The remaining rollout must satisfy these rules:

1. Existing host ABI v1, structured-value v1/v2, Tool Contract v1 and HTTP v1 stay
   frozen. Old artifacts either execute through their existing paths or fail by
   version; never infer binary64/UTF-16 from the newest compiler version.
2. A further semantics-breaking profile requires a new host ABI (ABI v2 belongs
   to binary64/UTF-8 v1, ABI v3 to its undefined increment, ABI v4 to UTF-16, ABI v5 to primitive unions, ABI v6 to missing values, ABI v7 to reference unions, ABI v8 to checked optional/union JSON, ABI v9 to resizable arrays, ABI v10 to primitive exceptions, ABI v11 to Error objects, ABI v12 to native error families and ABI v13 to recoverable checked JSON; further breaking layouts need a new version)
   and explicit language metadata. A future runtime must validate the profile/ABI
   combination, reject missing metadata, and reject incompatible value layouts.
   Earlier runtimes must reject that version even if they ignore new custom-section names.
3. Keep tool v1's exact i32/boolean/UTF-8 JSON contract. A new-profile handler needs
   an explicitly checked adapter or a new tool/invocation contract; never round,
   truncate or coerce external arguments/results to fit the old slots. HTTP v1's
   transfer layout also needs an adapter or a new HTTP contract. Strict integer
   port/status/process parameters must be checked before any OS operation.
4. Host filesystem/URL/environment strings need an explicit conversion policy for
   lone surrogates and NUL. Reject unrepresentable/security-invalid inputs before
   effect; do not silently replace characters that change resource identity.
5. Add executable conformance cases for fractional/large numbers, signed zero,
   NaN/infinity, division/remainder, UTF-16/lone surrogates, undefined/null, equality,
   coercion, property order and JSON. Exercise compiler/runtime/public API/native
   execution, collection pressure and permission denials in both old/new profiles.

After that increment, expand closures/errors, scheduling, standard APIs and
module/package compatibility from the roadmap above. Application-state/MCP work
remains deferred behind core capabilities; no application project is required.
#### K4.3 — Wider language and asynchronous runtime (in progress)

The first explicit guest-exception increment is implemented in
**`checked-f64-utf16-v7` / host ABI v10 / value ABI v9 / schema v9**. Primitive
throw and try/catch work across named calls/modules, with typed guardable catch
bindings, rethrow, lexical scopes and ordinary return/break/continue. The reserved
root/global channel preserves thrown cells across frame unwinds; pending checks
stop unfinished expressions before later effects. Runtime initialization and
uncaught boundaries validate channel state and return owned typed values with
bounded diagnostics. Resource traps, checked JSON/index failures and authorized
exit remain fatal; system statuses retain their existing meaning. Tests cover
primitive bits/UTF-16, nested control flow, GC/repeated catches, malformed channels,
public APIs, source-free artifacts and host ceilings.

Finally/abrupt-completion handling is implemented additively in the same profile:
one finalizer per region, exact saved return/exception roots, nested completion
ordering and override semantics for return/throw/break/continue. Fatal traps and
authorized exit bypass finalization. Completion analysis checks overridden returns;
tests include collection, captured-result pairs and source-free execution.

The first generic Error value model is implemented in
**`checked-f64-utf16-v8` / host ABI v11 / value ABI v10 / schema v10**: nominal
identity, checked string/undefined constructors, immutable name/message reads,
toString, instanceof narrowing and exact ownership through calls/aggregates/catches/
finalizers. Message presence/non-enumerability follow the admitted standard rules.
Uncaught errors own full UTF-16 messages; malformed objects/channel versions and
old observers fail closed. This remains a deliberate subset: writable properties,
cause/options, error families, subclasses, prototype behavior and stack traces
need further implementation. JSON parsing cannot synthesize Error objects.

The next error increment is implemented in **`checked-f64-utf16-v9` / host ABI v12 /
value ABI v11 / schema v11**: TypeError, RangeError and SyntaxError constructors,
nominal family annotations/unions, the Error base type, family-specific instanceof
narrowing and correct names/toString/owned diagnostics. A versioned header class
and immutable exception selector keep earlier readers/profiles frozen. Identity,
roots, presence and status survive calls, aggregates, rethrow, finally and GC.
These are explicit guest exceptions; checked JSON/index failures remain fatal.

The first recoverable API error increment is implemented in
**`checked-f64-utf16-v10` / host ABI v13 / value/schema v11**. Checked JSON syntax
failures become SyntaxError and closed-schema mismatches become TypeError through
the versioned json_parse_v13 import. Whole-input grammar/resource validation and
typed limit tracking separate input mistakes from fatal budgets without matching
error strings. Error construction is owned/rooted, publication checks the empty
channel and untrusted helpers, and the compiler checks pending before observing
results or later effects. Tests cover Unicode, schema/type/numeric failures,
limits/corruption, GC/repeated recovery, host ceilings and source-free execution.
Older profiles keep fatal parsing; system status and authority remain unchanged.

String.repeat is implemented additively in the native-family profile (v9) and
successors: Number/Undefined count normalization, exact UTF-16 repetition and
recoverable RangeError for negative normalized counts/infinities. One bounded
result allocation, bulk-copy doubling, rooted operands and activation-local
scratch preserve effects/ownership without new imports, helpers or ABI versions.
String/memory/fuel/deadline limits remain fatal. Tests include GC, nested calls,
module/public APIs, status/host ceilings and source-free artifacts.

Recoverable JSON.stringify cycle failures depend on admitting cyclic structures
in the source value model. Current closed type DAGs cannot create them from TS;
raw malformed/cyclic Wasm fixtures continue to fail fatally. Specify recursive/
dynamic types and their validation before treating this as a source capability.

Next expand standard APIs and recoverable language/API errors without
weakening fatal budgets or capability checks. General JSON.parse, serializer
exceptions and remaining native families, writable
properties, causes/options, custom subclasses and stack traces need further work.
Typed function values and capture-free arrows are implemented in
**`checked-f64-utf16-v11` / host ABI v14 / unchanged value/schema v11**. Exact
function types support locals/reassignment, arguments, returns and calls; expected
types supply arrow parameter context. Named references have stable identity across
module aliases; each arrow evaluation creates a fresh managed object. Closed direct
Wasm dispatch checks semantic signatures, preserves evaluation order, roots and
pending exceptions, and keeps JSON helper host-effect analysis unchanged. Explicit
function/type/body bounds prevent unbounded dispatcher expansion. Tests cover GC,
malformed cells, hidden host effects, public APIs, grants and source-free execution.
Captures, function-valued aggregates/unions/JSON and wider function syntax remain
unsupported; no application logic or asynchronous machinery enters this increment.

Lexical closures over initialized bindings are implemented in
**`checked-f64-utf16-v12` / host ABI v15 / unchanged value/schema v11**. Captured
parameters/locals/catch bindings use shared managed cells; nested closures forward
cells and escaping closures retain their complete graph. Mutations remain visible
across sibling closures while separate activations stay independent. For-loop let
bindings are copied before update, including continue/finally paths; body bindings
are allocated anew on entry. Exact edge masks preserve non-reference bits, host
result pairs and GC of unreachable cycles. Entry guards validate function,
environment and binding metadata before user effects. JSON helper call-graph
purity, capability checks and fatal resource limits remain unchanged.

Captures before binding initialization, mutable captured union refinement and
captures under an active refinement are rejected. Later TDZ/ReferenceError and
alias-aware refinement work must specify those semantics before admission.
Function expressions and private self bindings are now implemented below.
Outer-binding recursive initialization, function-valued aggregates/JSON and
broader function syntax remain pending. Tests exercise nested/escaped mutable
closures, per-iteration bindings, roots/cycles, exceptions/finally, malformed memory,
limits, helper effects and source-free/public APIs.

The first Array callback increment is implemented additively in v12: map/filter/
forEach/find use ordinary typed function dispatch and capturing closures. Receiver
and callback evaluate once; traversal snapshots length and reads the current
backing at each turn. Filter/forEach skip removed slots; find admits Undefined in
its callback value and visits deleted indices. Predicates preserve pre-call values
and reference identity. Map supports checked value results/void-to-undefined;
filter/find apply checked truthiness; forEach discards results. Typed wrappers share
existing frame roots, exception routing, effect validation and resource bounds.
Tests cover mutations/regrowth, empty/1024-element arrays, errors/limits, malformed
callbacks, collection, permissions and source-free/public APIs. No ABI/import or
permission changes are required.

The sparse-array increment is implemented in **checked-f64-utf16-v13 / host ABI
v16 / value ABI v12 / schema v12**. A bounded presence bitmap follows typed backing
slots; holes and spare data are canonical zero, and bitmap bytes are never traced
as child references. Sparse literals, hasOwn, reads/at, filling existing slots,
slice, pop/push, includes/indexOf and callback traversal preserve presence. Map
allocates its final-length result once and fills only visited indices, retaining
holes after callback shrinkage. Filter/forEach skip holes; find visits Undefined.
Checked JSON renders holes as null and parses fully present arrays. Versioned v6
value imports, json_stringify_v12/json_parse_v14 and value_is_error_class_v2 bind
the new representation; old profiles retain their contracts and v12 map trap.
Tests cover bitmap boundaries/growth, 1024-length maps, GC, malformed data, version
substitution, helper purity, public/source-free APIs and capability ceilings.

Bounded delete, indexed extension and writable length are implemented in
**checked-f64-utf16-v14 / host ABI v17 / unchanged value/schema v12**. Indexed
writes retain the original receiver/key and evaluate RHS before numeric write
checks/growth; extension creates holes. Length shrink clears references and bits,
while growth allocates/copies before publishing or reuses zero spare storage.
Invalid numeric uint32 lengths raise owned RangeError, valid lengths above the
1024 resource ceiling remain fatal, and deleting nonconfigurable length raises
TypeError. Array-only numeric/literal delete preserves length and returns true
for absence. The thirteenth bounded host-free helper preserves canonical sparse
layout, exact roots, helper purity, status and authority. Tests include independent
mutation models, operand order/errors, callbacks, GC, boundary lengths/indices,
profile compatibility and source-free/public APIs.

Reduce/some/every are implemented additively in v12 and successors with unchanged
ABI/imports/helpers. Typed wrappers snapshot length after receiver/callback/seed
effects, skip live missing entries and preserve prior element/accumulator roots.
Some/every short-circuit through checked truthiness and return false/true for
empty/all-hole arrays. Reduce supports exact typed accumulators, an optional seed
(including explicit Undefined), first-present initialization without a seed, and
an owned TypeError for an empty/all-hole array without a seed. Inline reducers
contextually type parameters and returns; checked unions, aggregate/function and
physical-result accumulators retain their ownership. Tests cover sparse mutation,
operand order, short circuits, errors/fatal limits, malformed callbacks, GC,
1024-element traversals at default fuel, helper purity, permissions/host ceilings
and public/source-free execution.

Contextual callback return and reduce seed inference are implemented additively
in v12 and successors. Unannotated inline Array callback blocks infer actual typed
return sites and supported closed unions, including bare/fallthrough Undefined;
ordinary arrows retain prior defaults. Bodies lower once and nested arrow returns
remain separate. Finally saved returns receive exact inferred slot/root types
before emission, preserving aggregate/union/multi-slot ownership through GC.
Reduce context comes from explicit callback annotations/function signatures, then
surrounding aggregate result positions, enabling empty/nested arrays and optional
record seeds. Wider outer scalar results never widen a numeric seed. Exact
signatures, ambiguous-type rejection, operand order, authority and all budgets
remain intact. Tests cover these rules, errors/diagnostics, GC/finalizers, helper
purity, modules, host ceilings and source-free/public engine use.

Anonymous function expressions, typed this/thisArg and checked function.call are
implemented additively in v12 and successors. Function expressions share capture,
return-inference and direct-dispatch mechanisms. Explicit this is separate from
positional parameters, exact in function types and rooted through normal physical
parameters/cells; receiver mode discriminates even identical Wasm signatures.
Six Array callback methods accept optional thisArg with once-only source order
before snapshot length; reduce keeps its seed/Undefined receiver semantics.
Arrows preserve lexical this through escaped/transitive captures. Calls preserve
strict primitive/null/undefined values and ignored receiver effects. Tests cover
GC/finalizers, sparse mutation, errors/fatal limits, malformed targets, slot/fuel
bounds, authority/helper purity and source-free/public execution. No ABI/import/
layout/helper change is needed.

Named-expression self bindings and declaration receivers are implemented
additively in v12+. Named expressions require an annotated/contextual result and
own a private immutable name, behind the parameter/body scope. A hidden rooted
self parameter carries the actual callee identity into each activation; entry
checks validate signature/code/environment before effects. Escaped closures retain
it through existing cells, while alias reassignment and shadowing stay separate.
Declaration this is included in exact types/physical parameters and canonical
imported references; direct bare calls supply checked Undefined, call/Array paths
supply explicit receivers. Entry main and legacy tool/HTTP contracts stay
receiver-free. Tests cover recursion/scopes, GC, pairs/slot limits, errors/fatal
recursion, forged self inputs, helper purity/permissions and source-free/public
APIs. No heap/import/helper/ABI change is introduced.

Optional/default/rest parameters are implemented additively in v12+. One call
normalizer checks arity/types, supplies missing Undefined and builds fresh bounded
rest arrays. Exact function modes participate in dispatch even for equal physical
signatures. Extended callees separate rooted incoming ABI slots from sequential
source-binding initialization: defaults run after supplied argument effects and
only for Undefined, see prior initialized parameters/this/private self, and cannot
read later parameters or body declarations. Incoming roots clear after transfer;
normal capture cells/GC/finalizer/result-pair ownership remain intact. Array
callbacks receive their real implicit arguments, optional tails and checked rest;
reduce preserves the exact accumulator type. Tests cover these semantics, scopes,
GC, errors/fatal limits, mode substitution, helper purity, permissions and source-
free/module/public execution. No layout/helper/import/ABI change is required.

Bounded spread-call arguments are implemented additively in v12+. Narrowed array
segments and explicit values work with declarations, closures and explicit
Function.call receivers. Shallow snapshots preserve source evaluation order and
child identity; sparse holes become Undefined. A rooted wrapper validates the
1024 flattened-argument cap (fatal), dynamic arity/types (owned TypeError), pads
optional slots and builds a fresh dense rest array before defaults/body code.
Explicit Function/physical-result pairs retain their widths outside aggregates.
Pure guest Undefined checks avoid per-element host-call budget use. Existing
function/physical-slot/body/resource limits, helper purity, actual grants/status
and profile/ABI boundaries remain intact. Tests cover v12/v13/v14 default-fuel
limits, ordering, holes/defaults, exceptions, GC/pairs, forged dispatch, diagnostics,
virtual modules and source-free artifacts with host ceilings/authorized exit.
No layout/helper/import/ABI change is required.

Parameter destructuring is implemented additively in v12+: closed record/array
patterns, nesting/renaming/static literal keys, top/nested defaults, elisions,
object/array rest and destructured array rest parameters. Incoming aggregate
signatures stay exact; all BoundNames start in TDZ and leaves initialize in source
order with ordinary shared capture cells. Nullable nested aggregates throw owned
TypeError before later defaults/body code. Array reads observe live mutation and
retain iterator exhaustion; rest is fresh/dense and maps sparse holes to Undefined.
Object rest preserves source presence/enumeration order and shallow child identity.
Pure guest tests/metadata copies retain status, exact roots, GC, physical pairs,
helper purity and actual permissions. Shared pattern budgets (64 depth/1024 nodes)
join existing resource limits. Tests cover full-width presence, default-fuel rest/
callbacks, errors/fatal limits, scopes/closures/this/self, virtual modules and
source-free native/Wasm execution. No ABI/import/helper/layout change is required.

Local const/let destructuring is implemented additively in v12+ through the shared
binding mechanism. Blocks and ordinary for initializers predeclare all leaves in
TDZ, infer or contextually check one RHS evaluation, then initialize in source
order. Const/let keep mutability and managed closure cells; nested scopes restore
shadowed bindings. For let now separates the initializer from the first iteration,
as well as rebinding before later updates; const retains its loop environment.
Pattern semantics, exact roots, GC/physical pairs, recoverable/fatal paths, status,
helper purity and actual grants remain intact. Shared 64/1024 pattern budgets also
bound each lexical declaration. Tests cover scopes/mutability/captures, iteration/
finally, bounds/default-fuel copies, dependency diagnostics, virtual modules and
source-free artifacts with real grants/host ceilings. No ABI/helper/layout change.

Checked destructuring assignment statements are implemented in v12+. Existing
initialized mutable local/parameter/captured targets are validated without new
bindings; RHS evaluates once and nested source-order writes retain existing cells.
Defaults see earlier writes/prior values, duplicate targets are allowed and runtime
errors preserve successful writes before skipping later work. Exact/union types
and checked Undefined narrowing protect invariant target types; defaults use target
context. Shared traversal/root/rest/exception rules, bounds, actual grants/status
and helper purity remain intact. Tests cover closures, partial failures, GC/pairs,
1024-value default budgets, diagnostics, virtual modules and source-free artifacts.
No ABI/import/helper/layout change is introduced.

Expression-valued destructuring assignment is implemented additively in v12+.
Save one rooted RHS and return the original value/type only after traversal;
identity, unselected fields and sparse holes remain intact. Results work in nested
chains, arguments, returns, callbacks and supported expression positions. Scoped
root floors protect earlier operands across embedded traversal statements, while
normal handlers/finalizers retain cleanup and partial-write semantics. Collection/
effect visitors see both RHS and body. Tests cover result context, earlier/later
operand GC, physical pairs, fatal limits/default budgets, helper purity, virtual
modules and source-free artifacts with real grants/status. No ABI/helper change.

Checked record-field/array-element destructuring targets are implemented in v12+,
including nested/default/rest targets. Prepared receivers/indices evaluate once
before corresponding source reads/defaults or rest copying; hidden roots preserve
identity through allocation, reassignment and array resizing. Existing setters
retain types/presence/order and profile-specific bounds; length targets require
v14. Tests cover order/throws, GC/pairs, current backing, fatal limits, legacy gates,
helper purity, virtual modules and source-free artifacts with grants/ceilings and
dependency diagnostics. No ABI/helper/layout change.

Intrinsic array for-of is implemented in v12+. One rooted source is traversed
through live length/backing; v13+ holes yield Undefined. Lexical const/let heads
receive fresh capture cells per iteration, while assignment identifiers/members/
patterns retain checked writes. Iterator values precede reference/default effects;
TDZ, patterns, GC/pairs, branch/finalizer cleanup and status/grants retain their
contracts. Nullable array absence is recoverable TypeError; bounds/fuel/exit stay
fatal. Tests cover 1024-element default budgets, virtual modules and source-free
artifacts with grant denial, host ceilings and dependency diagnostics. No ABI,
helper, import or layout change. General iterables and async iteration remain open.

Checked do-while is implemented in v12+. Dedicated typed IR runs the body first,
routes continue through finalizers to the trailing condition and skips that test
for break/return/throw. Body scopes/captures, condition-root cleanup and actual
effect visitors retain ordinary semantics. Completion/callback inference recognizes
guaranteed first-body returns/throws without inventing normal exits; possible
break/continue paths still permit fallthrough. Tests cover ordering, overrides,
truthiness, GC/pairs, bounded condition roots, profile gates, virtual modules and
source-free artifacts with grants/ceilings and dependency diagnostics. No ABI,
helper, import or layout change; existing resource/exit failures remain fatal.

Checked statement labels and named break/continue are implemented in v12+. Unique
function-local IDs resolve active targets; iteration aliases bind to actual loop
continues before generated initializer/rest loops. Non-loop statements allow
named break. Existing finalizer routing and exact scope cleanup preserve captures,
GC, saved returns/pairs and fatal limits. Completion analysis propagates named
paths through inner loops until their own label, keeping callback/function return
inference sound. Tests cover loops/blocks/switches/aliases, invalid targets/depth,
legacy gates, virtual modules and source-free artifacts with grants/ceilings and
dependency diagnostics. No ABI/helper/import/layout change.

Object.keys is implemented in v12+ for closed records and nullable single-record
unions. One rooted argument yields a fresh dense String array of present own keys:
index names sort numerically, other names follow insertion order, and explicit
Undefined differs from absence. Pure metadata enumeration avoids payload reads;
compaction zeros tail payloads/bits before publishing both lengths. Internal value
blocks preserve caller roots/floors and exception/return lifetimes, with full
layout/effect traversal. Tests cover 32-field/default limits, Unicode/order/JSON/
rest, GC/pairs/callbacks, profile gates, virtual modules and source-free artifacts
with grants/ceilings and dependency diagnostics. No ABI/import/helper/layout change.

Object.values is implemented in v12+ for closed records and nullable single-record
unions. Shared key/value enumeration derives exact or admitted union array elements
from field types, preserving presence/JS order, numeric bits and shallow identity.
Typed cached payloads are rooted before output allocation; compaction reads only
literals/locals and clears spare values/bits before publishing lengths. Empty
records yield Undefined[]; unrepresentable unions fail without casts. Tests cover
32-field default limits, JSON/rest/callbacks, GC/pairs/cached child graphs, profiles,
virtual modules and source-free artifacts with grants/ceilings and diagnostics.
No ABI/import/helper-export/layout change; keys retain their existing behavior.

Object.keys/Object.values now also accept bounded arrays and nullable single-array
unions in v12+. Private guest helpers produce ascending decimal keys and exact
invariant values while skipping holes and retaining explicit Undefined. Fresh
zeroed backing and rooted incremental prefixes preserve GC/child graphs, numeric
bits, shallow identity and later growth. Helper roots are bounded/restored; no
host/import/helper-export/layout/ABI change. Tests cover all element kinds, 1024
indices, repeated GC/roots, virtual modules and source-free artifacts with grants,
host ceilings, status and fatal exit.

Checked catch object patterns are implemented in v12+ for static name/message
identifier bindings, aliases/defaults and empty patterns. The full primitive/Error
channel remains unchanged: non-null primitives supply Undefined, nullish inputs
throw TypeError before binding effects, and Errors retain existing fields. All
names are predeclared under TDZ, initialized left to right and own fresh capture
cells; hidden original-value roots and partial leaves clear on every exit. Binding
failures reach outer handlers/finalizers. Tests cover defaults/effects, GC/pairs,
returns/branches/callback inference, modules, source-free artifacts and diagnostics.
Array/rest/other-key patterns and aggregate throws remain explicit future work;
no ABI/import/helper-export/layout change.

String.slice/substring are implemented in v12+ for narrowed strings with optional
Number/Undefined bounds. They preserve code-unit UTF-16/NULs/lone surrogates,
source-order effects and owned result storage; floating-point normalization/clamp
precedes bounded integer conversion. Pure private helpers retain ordinary GC,
physical pairs/saved returns/finalizers/status/grants and fatal byte/memory/fuel
limits. Shared slice dispatch lowers receivers once and keeps array/legacy rules.
Tests cover numeric edges, nested/throwing bounds, repeated GC, maximum byte
limits with test fuel, virtual modules and source-free artifacts with host ceilings
and diagnostics. No ABI/import/helper-export/layout change.

String.includes/startsWith/endsWith are implemented in v12+ with required String
needles and optional Number/Undefined positions. Exact UTF-16 unit comparisons
preserve lone surrogates/NULs/case; normalized clamping precedes integer conversion.
Private guest helpers allocate/call nothing and return canonical booleans. Source
order, exception/GC/pair/finalizer lifetimes and shared Array.includes dispatch
retain existing contracts. Candidate scanning is bounded by existing fuel/deadline
limits, including adversarial-input regression tests. Maximum byte, module and
source-free artifact tests preserve grants/host ceilings/status/diagnostics.
No ABI/import/helper-export/layout change.

String.indexOf/lastIndexOf and array reverse search/mutation increments are
implemented as recorded in the checkpoint follow-up above.

V12+ dynamic String own keys for closed records are implemented. A rooted value
block evaluates source/key once, then a UTF-16-ordered balanced decision tree reads
existing presence bits without payload/prototype access. Literal String keys keep
a direct path, including false for unrepresentable names/__proto__. Existing numeric
literal and older-profile rules remain unchanged. Tests cover exact
Unicode ordering, all 32 bits/default limits, argument mutation/throws, GC/pairs/
finalizers/callbacks, effects/grants/status, modules and source-free artifacts.
No ABI/import/helper-export/layout change.

Dynamic String own keys now cover arrays in v13+ and Error families/only-Error
unions in v12+. Allocation-free bounded array parsing reuses current-length/bitmap
checks; length remains own, holes differ from explicit Undefined. Error lookup
masks only own-message after exact key matching. Tests cover bitmap edges/max index,
canonical/unknown/max-byte names, argument mutation/order/throws/fatal limits,
GC/pairs/finalizers, older profiles, modules, source-free artifacts and grants/status.
No ABI/import/helper-export/layout change.

Typed computed-key reads on closed records are implemented in v12+ as specified in
the [computed read contract](architecture.md#computed-record-read-contract). Tests
cover admitted result unions/optional presence, exact UTF-16 ordering, all slots,
numeric bits/reference identity, source/key mutation and throws, GC/pairs/finalizers,
helper purity, bounds/profiles, virtual modules and source-free artifacts.
No ABI/import/helper-export/layout change.

Typed computed-key assignment statements are implemented in v12+ as specified in
the [computed write contract](architecture.md#computed-record-write-contract).
Prepared source/key/RHS roots, full field/member checks and pure Error-family guards
preserve exact payloads, optional presence/order and existing limits. Regressions
cover modules/source-free artifacts, diagnostics, effects/grants/status, failure
order, invariant types, all slots, GC/saved pairs/finalizers and fatal budgets.
No ABI/import/helper-export/layout change.

Computed record assignment values/chains are implemented in v12+ under the
[assignment value contract](architecture.md#computed-record-assignment-value-contract).
Both static computed literals and dynamic String keys preserve field rules and
original RHS types/identity, with value-block floors/roots across nested effects,
GC and finalizers. Tests cover all slots, callbacks/destructuring composition,
partial chains, source-free artifacts, diagnostics and permissions/fatal exits.
No ABI/import/helper-export/layout change.

Ordinary simple assignment values are implemented in v12+ under the
[simple value contract](architecture.md#simple-assignment-value-contract). Shared
prepared references/setters retain original RHS types/results, local functions and
physical pairs, current array storage/sparse rules and v14 checked length values.
Regressions cover effects/partial chains, invariance, fatal indices/length bounds,
capture/GC/finalizer cleanup, modules/artifacts, source ranges and grants/status.
No ABI/import/helper-export/layout change.

Checked arithmetic compound values/statements are implemented under the
[compound contract](architecture.md#arithmetic-compound-assignment-contract).
Regressions cover old-value ordering, matching Number/String union cases, absence/
RHS exceptions, current sparse storage/length bounds, original result bits, GC/
saved pairs/returns, artifacts/modules, source diagnostics and permission/status
boundaries. No ABI/import/helper-export/layout change.

Dynamic record arithmetic compounds are implemented under the
[dynamic compound contract](architecture.md#dynamic-record-compound-assignment-contract).
Regressions cover retained receiver/key/old-value identity, inherited versus missing
read timing, strict union cases, complete field types, exact UTF-16 names/all field
bits, partial chains, callbacks/GC/pairs/returns/finalizers, source diagnostics,
virtual modules and source-free artifacts with operation/host-policy boundaries.
Computed reads retain their existing union restrictions and all compilation/value/
fatal budgets; no ABI/import/helper-export/layout change.

Logical assignments are implemented under the
[logical contract](architecture.md#logical-assignment-contract). Regressions cover
truth/nullish edges, branch-only RHS/store effects, precise skipped-member result
types, optional presence/order, prepared identity, dynamic field failures, sparse/
current storage/length rules, function context/identity, GC/pairs/returns/finalizers,
fatal skips and exhaustion, module/artifact execution and permission boundaries.
No ABI/import/helper-export/layout change; existing limits remain active.

Number updates are implemented under the
[update contract](architecture.md#number-update-contract). Regressions cover
prefix/postfix Number results, -0/NaN/infinity/rounding, source/key order, optional/
sparse/missing/inherited failures, length checks, partial chains, captured/fresh
loop cells, GC/pairs/returns/finalizers, helper purity, precise source ranges,
module/artifact execution, status/grants/host ceilings and fatal exhaustion/exit.
No ABI/import/helper-export/layout or resource-limit change.

Number bitwise/shift operations are implemented under the
[bitwise contract](architecture.md#number-bitwise-contract). Regressions cover
all binary64 exponent ranges and seeded payloads against an independent numeric
oracle, all operators/shift masks, source order, narrowed unions/callbacks, roots/
pairs/GC/finalizers, helper purity, source ranges, module/source-free execution and
operation/host-policy/fatal boundaries. No ABI/import/helper-export/layout change.

Bitwise compound assignments are implemented under the
[bitwise compound contract](architecture.md#bitwise-compound-assignment-contract).
Regressions cover all six operators, Number guards/results, original receiver/key/
old snapshots, RHS effects/exceptions, exact UTF-16 names/all field bits, partial
chains, sparse/current storage/length rules, GC/pairs/saved returns/finalizers,
helper purity, dependency ranges, modules/source-free artifacts and permission/
status/fatal boundaries. No new IR/ABI/import/helper-export/layout or limit change.

Number exponentiation is implemented in **checked-f64-utf16-v15 / host ABI18 /
value and schema12** under the [power contract](architecture.md#checked-f64-utf16-v15--number-exponentiation).
The pure typed import preserves system status and uses one host-operation budget
unit with deadline checks before/after native math. Explicit Number edge cases
precede platform-approximated finite pow; BigInt/coercion remain deferred.
Tests cover exact special-case bits/parity, independent sqrt accuracy, budgets,
prepared compounds/GC/pairs/finalizers, inherited sparse/JSON/closure contracts,
old-profile and hostile metadata/signature rejection, virtual modules, source-free
artifacts, source ranges and operation/host-policy/fatal boundaries. Native
negotiation advertises v15/18; value/schema/container formats and limits persist.

Direct Math.abs/floor/ceil/trunc/round/sqrt are implemented in v15 under the
[checked Math contract](architecture.md#checked-math-intrinsic-subset). One narrowed
Number input, exact zero/nonfinite/rounding rules and guest-only execution preserve
all argument effects/roots/pairs/finalizers, host status and budgets. All-exponent
numeric checks and source-free artifacts cover this subset; shadowed/computed/
borrowed/coercive calls and older profiles are rejected. No ABI/import/permission/
helper-export/layout/limit change.

Math.min/max, sign, fround, clz32 and imul are also implemented in v15, including
empty/variadic extrema, exact Uint32 conversion, signed low-bit products and
binary32 ties-to-even rounding. Tests cover all-exponent conversions, binary32
halfway/overflow/subnormal boundaries and all-argument NaN/exception order.

Number.isFinite/isNaN/isInteger/isSafeInteger are implemented in v15 under the
[predicate contract](architecture.md#number-predicate-intrinsics): all admitted
types/unnarrowed unions, non-coercing false for non-Numbers/missing input, and
effectful ignored arguments. Guest-only Boolean results preserve ABI/budgets,
roots/pairs/finalizers and artifact/capability boundaries.

All sixteen Math/Number numeric constants are implemented in v15 under the
[constant contract](architecture.md#numeric-constant-properties), with exact bits,
dot/literal-key reads, lexical precedence and complete read-only write-path checks.
Namespace objects/aliases and descriptors remain future work; existing Number
logging text is retained while JSON uses the pinned ECMAScript formatter.

Math.pow is implemented in the same profile through the existing FloatPower
operation, preserving exact special-case behavior, finite approximation, source
order, host budgets/status and helper-purity rejection.

Math.f16round is implemented under the [direct binary16 contract](architecture.md#direct-binary16-rounding).
Exact power-of-two rescaling avoids f32 double rounding, preserves special values
and covers all half encodings/midpoint neighborhoods with an independent bit oracle.

Math.hypot is implemented under the [scaled norm contract](architecture.md#scaled-variadic-mathhypot).
Power-of-two normalization and compensated products/sums/root refinement avoid
premature magnitude loss without new host calls or raised limits. Sampled native
comparisons and independent decimal overflow goldens cover numerical behavior;
all-argument effects, roots and source-free/public APIs retain their contracts.

The twenty deterministic native Math operations are implemented under the
[native Math contract](architecture.md#checked-f64-utf16-v16--native-math), with
explicit edge cases, platform approximation, typed ABI19 and inherited value/schema12.
Registry/metadata, budgets/status, helper purity, all exponents/domain invariants,
argument lifetimes, public modules and source-free packaging have regression coverage.

Math.random is implemented under the [random contract](architecture.md#checked-f64-utf16-v17--mathrandom),
with fresh OS entropy, exact 53-bit sampling, fatal failure/limit behavior and
ignored-argument/root/status guarantees. No seed or shared PRNG is persisted.
Profile/registry/signature checks, injected fault/edge tests, independent stores
and source-free portable/native/public engine paths cover this boundary.

ECMAScript decimal Number output is implemented under the
[Number output contract](architecture.md#checked-f64-utf16-v18--number-output),
with versioned scalar/union logs, shortest canonical decimal/scientific formatting,
old-profile compatibility and unchanged value/schema12. Edge/exponent/minimality,
hostile-import/helper checks, budgets/status/GC and source-free/public APIs are covered.

Primitive String conversion is implemented under the
[primitive String contract](architecture.md#checked-f64-utf16-v19--primitive-string-conversion),
with cached first values, ignored argument effects, exact UTF-16 and owned results
through existing value12 cells/result buffers. Profile/registry/hostile-cell/helper
checks, maximum-string/GC/status tests and source-free/public paths cover it.

Primitive Number conversion and numeric String parsing are implemented under the
[primitive Number contract](architecture.md#checked-f64-utf16-v20--primitive-number-conversion).
Checked cells, exact grammar/whitespace/rounding and whole-string NaN rules preserve
effects, signed zero, roots/pairs, status/buffers and fatal limits. Independent
integer goldens, exponent-wide checks, maximum strings and source-free/public API
paths cover the foundation needed for radix argument conversion.

Checked Number.toString is implemented under the
[radix contract](architecture.md#checked-f64-utf16-v21--number-radix-text), with all
bases2–36, cached/coerced primitive radix arguments, correct RangeError order and
owned UTF-16. Exact bounded intervals choose shortest text; independent rational
round-trip/minimality tests, runtime budgets/roots/status and source-free/public
paths cover this method. Wrappers and ordinary prototype objects are still pending.

Number precision and valueOf are implemented under the
[precision contract](architecture.md#checked-f64-utf16-v22--number-precision),
with bounded exact rounding, conversion/default/range ordering, owned results and
guest-only identity. Independent Decimal goldens, exponent-wide bounds, raw ABI/
helper checks, lifetimes/status/budgets and source-free/public paths cover them.

Numeric prefix parsers are implemented under the
[parsing contract](architecture.md#checked-f64-utf16-v23--numeric-prefix-parsing),
with primitive inputs/radices, prefix grammar, exact integer rounding, Int32 wrapping
and ordinary NaN failures. Independent integer goldens, exponent/Unicode/bounds,
malformed cells, helpers and source-free/public paths cover the boundary.

Boolean and global numeric predicates are implemented under the
[conversion contract](architecture.md#boolean-and-global-numeric-predicates-v23).
Ordinary-value ToBoolean is guest-only, numeric coercion reuses existing imports;
root/status/budget, exponent, helper and source-free/public checks cover it.

Guest truth/control flow and logical operators are implemented under the
[control-flow contract](architecture.md#guest-truth-conversion-and-logical-operators-v23).
Exact tests, operand-valued short circuiting, selected-branch type narrowing,
roots/status/budgets and source-free/public paths have regression coverage.

Truth-based local/RHS guards are implemented under the
[v24 contract](architecture.md#checked-f64-utf16-v24--truth-guards),
sharing ABI26 and existing values while versioning stricter source refinement rules.
Tests cover scoped branches/loops, false primitive classes, mutation/capture safety,
metadata, numeric bits/roots/status and source-free/public API execution.

Write-aware invalidation/captures are implemented under the
[write-flow contract](architecture.md#write-aware-refinements-and-captures-v24).
Per-binding revisions, bounded AST loop/test effects and typed captured cells retain
root/status/limit/legacy boundaries; conservative inference remains explicit.

Post-store inference and normal/targeted-exit joins are implemented under the
[completion-flow contract](architecture.md#completion-targeted-local-flow-v24).
Optional effects, repeated-body invalidation and exceptional entry remain conservative.
Known call effects are implemented under the
[call-flow contract](architecture.md#proven-call-effects-for-local-cells-v24).
Stable callback inputs are implemented under the
[parameter-flow contract](architecture.md#stable-callback-parameter-effects-v24).
Next implement higher-order callback-argument binding, property/exception effects, initializer inference,
loop fixed points and the
remaining inventory. Generalized
coercion, aggregate exception design and constructors/prototypes
remain language work. Scheduler/promise jobs follow retained-root, cancellation,
queue and handle lifetime design.

- Grow functions/closures, dynamic object/array behavior, exceptions and the
  corresponding memory model under the explicit compatibility profile.
- Implement event-loop and promise-job semantics, timers, async/await, cancellation
  and handle lifetime with bounded queues and correct retained roots.
- Expand standard JSON/Web/binary APIs on that foundation. The initial synchronous
  HTTP handler profile is a delivery increment, not the final language/API ceiling.
#### K4.4 — Suspension, scheduling and async (planned)

Status: design agreed with the project author on 2026-10-28. Block 1 (step 1
below) is implemented as a compiler-internal foundation and merged on the v32
integration branch, where frames also carry v32 class and Error-class instances;
its contract is in
[architecture.md](architecture.md#suspension-foundation-k44-block-1). Runtime
preflight and artifact validation admit its frame surface (frame heap kind,
frame catalog version 2 and the `varta_resume` export) only under profile v33
([runtime admission](architecture.md#runtime-admission-resumable_frames)); no
host import or value tag changes. Step 2 (synchronous generators) is the first
source producer of frames; generator frames are resumed only by guest code, and
the v33 runtime still does not call the dispatcher. Steps 3–4 (promises and
async functions) are implemented under v34, where the runtime scheduler is the
dispatcher's first caller; steps 5–6 are not implemented.
Each increment still needs its own versioned contract in
[architecture.md](architecture.md), positive/negative and resource tests and the
normal gates before source admission.

**Profile v33 (in development).** Runtime admission of block 1 and step 2
share profile `checked-f64-utf16-v33`, registered with its own host ABI so
older runtimes reject the `varta_resume` export. Generators add no value tag,
so the value ABI stays at the v32 version. Its selectors `resumable_frames()`
(step 1 runtime admission) and `generators()` (step 2) are both implemented
([contract](architecture.md#checked-f64-utf16-v33--suspension-and-generators)).

**Profile v34 (in development).** Steps 3 (microtask scheduler and base
`Promise<T>`) and 4 (`async`/`await`) belong to the
[v34 pilot batch](#pilot-batch-v34-in-development), decided with the project
author on 2026-10-29. Its own host ABI makes runtimes without the scheduler
reject v34 modules. Its selectors `promises()` and `async_functions()` are
implemented on the `v34-async` branch
([scheduler](architecture.md#microtask-scheduler-promises),
[promises](architecture.md#promise-objects-promises),
[async functions](architecture.md#async-functions-async_functions)): the
runtime owns the microtask queue and is the first caller of `varta_resume`.

One suspension/resumption mechanism serves generators, async functions and async
generators. The event loop orders work, Promise holds a future result, and
async/await and generators compile to resumable state machines. Suspension never
retains a native or Wasm stack: every `await`/`yield` returns from an ordinary
Wasm call, so no Wasm stack-switching proposal and no embedded JS engine is needed.

| Part | Responsibility |
| --- | --- |
| Guest Wasm memory | Promises, closures, suspended frames and their values |
| Rust scheduler | Queues, timers, operation accounting, dispatching the next step |
| Tokio | Waiting for network/timer/external-operation completions only |
| Compiler | Resume functions, frame layouts and exact GC maps |

**Isolate model.** An isolate is one application execution context: Wasm instance,
Store, memory, globals and queues. Exactly one executor accesses its Store.
Background operations send completion messages; they never modify guest memory or
call guest code themselves. Scheduling is one mechanism; isolate lifetime is a
separate policy:

- The first async HTTP profile uses **one isolate per request**, preserving the
  current trap isolation. Promises may live for the whole asynchronous handling of
  that request. When its lifecycle ends, pending operations are cancelled, external
  roots released and the Store dropped; detached tasks cannot outlive the request.
  The contract must specify the bounded number of in-flight requests, the per-Store
  memory budget and the exact end point (response Promise settled and body delivered).
  Async inside one request does not by itself make requests concurrent.
- A **long-lived isolate** is a later, explicit mode with shared globals. A fatal
  trap terminates the whole isolate; ordinary `throw` and rejections remain
  recoverable language completions.
- The existing one-instantiation-per-execution rule applies to both modes. It
  protects the module/metadata pairing; it does not define execution duration.

**Turn order.** (1) run initial code or one ready task; (2) run microtasks,
including those queued while draining; (3) only after the microtask checkpoint
completes, take the next ready task; (4) with no work, wait for I/O or the nearest
timer. Promise jobs keep FIFO enqueue order per the
[ECMAScript Jobs model](https://tc39.es/ecma262/2025/multipage/executable-code-and-execution-contexts.html#sec-jobs).
Ordering between timers and different I/O sources is a Varta policy that must be
fixed explicitly before implementation. A microtask checkpoint is never interrupted
to run timers for fairness; runaway microtask chains are stopped by budgets and
runtime cancellation, never by reordering observable work.

**Compiler pipeline.** Suspendable functions follow:

```text
typed IR
→ evaluation-order normalization
→ explicit temporaries and branches
→ values live across suspension
→ frame construction
→ state machine
```

Normalization preserves already-evaluated callees, receivers and arguments
(`f(a(), await b())`, `obj.method(a(), await b())`), conditional execution of
`&&`, `||`, `??` and `?:`, and the prepared receiver/key/old value of compound
assignments in evaluation order. It must run on IR after lowering, because
intrinsic evaluation order is encoded in lowering-produced `ValueBlock`s. The
first increment emits an explicit suspension node, splits ordinary expressions
and branches, and rejects `await`/`yield` inside an intrinsic `ValueBlock` with
a clear `unsupported await inside <construct>` diagnostic until each construct is
supported. After normalization, `ValueBlock` and temporary floors serve only a
contiguous execution segment; everything crossing a suspension point receives
an explicit frame slot.

**Continuation frames.** A separate managed kind, not disguised as the existing
Environment, whose validators expect their exact format:

```text
ContinuationFrame {
    layoutId
    resumeCode
    state
    status
    environment
    savedCompletion
    handlerContext
}
```

Frames reuse binding cells for values shared by closures and continuations, the
Environment retention mechanism, type/physical-slot/GC-mask checks and immutable
metadata owned by `PreparedModule`. The first increment conservatively moves all
potentially needed bindings and temporaries into the frame; liveness analysis is
a later size optimization. Frames have their own explicit budget: the closure
capture limit must not silently become an async-function complexity limit. The
function catalog gains versioned frame-layout descriptions and keeps reading
older catalogs. New continuation/generator/Promise value tags should be
coordinated with the next profile/host-ABI/value-ABI cycle, which also introduces
function tag 12, rather than added through a separate ABI bump.

**Exception invariant.** Whenever control belongs to the scheduler, the global
language-exception channel is canonically empty. Before suspension, a pending
`return` or `throw` (including deferred `finally` completions and active handler
context) moves into the frame with its own GC retention. Resumption first
validates the frame and the input value, then delivers the completion to the
matching handler state before the next user effect. Fatal traps and resource
exhaustion are never converted into rejections.

**Closed dispatcher.** The runtime resumes guest code only through an exported
`varta_resume(frame, completionKind, value)` that performs closed direct calls by
code index; no table or indirect call is introduced. Before dispatch it checks that
the frame belongs to this Store and matches the catalog, that `resumeCode`, layout
and state agree, that the frame is neither completed nor running (reentrancy) and
that the input has an admissible type. Asynchronous waits carry a generation
number so a late completion cannot resume a frame twice. The dispatcher is
executable code with effects and is never classified as a pure helper; JSON helper
purity validation stays unchanged.

**Promise.** A full guest-heap object:

```text
Promise {
    state: pending | fulfilled | rejected
    result: Value
    fulfillReactions
    rejectReactions
    handled
}
```

`Promise<T>` is a built-in parametric type like `Array<T>`; user generics are not
a prerequisite. `then` creates a new Promise and registers reactions, handlers run
through the microtask queue, `new Promise(executor)` calls the executor
synchronously, and the handler result determines the next Promise state. The
resolution algorithm is the compatibility-critical part: one-time resolution,
adoption of another Promise or thenable, self-resolution TypeError and correct
error propagation. A static type selects a specialization but does not remove
runtime semantics: a mutable structural thenable reads its current `then`,
checks callability, keeps the receiver and adopts through a job
([NewPromiseResolveThenableJob](https://tc39.es/ecma262/2025/multipage/control-abstraction-objects.html#sec-newpromiseresolvethenablejob));
unions dispatch on the actual member. The internal-Promise fast path is allowed
only when the checked runtime type and the profile contract guarantee it. The first
increment may restrict admitted types; arbitrary thenables, getters and prototypes
remain explicit compatibility items until the object model exists.

**Generators.** `function*` suspends at `yield`; the consumer resumes it through
`next(value)`, `throw(error)` and `return(value)`. The frame holds execution state,
saved values and handler context, and re-entry into a running generator is rejected.
Synchronous generators need no event loop, so they are implemented and verified
first as the base of the suspension mechanism. `async function*` later combines
the frame with Promise and a queue of iterator requests; `yield*`, iterator closing
and `finally` belong to its complete contract.

**GC and lifetime.** Suspended functions outlive the Wasm call that created them.
GC roots therefore also include scheduled callbacks and microtasks, suspended
frames, Promise reactions and active timers/I/O operations, held in an explicit
external root table. Each operation has an identifier with a generation; completion
or cancellation releases its retained values, and a late message from a cancelled
operation cannot reach a reused slot. A pending Promise without active I/O or a
timer does not keep the process alive.

**Limits and cancellation.** Separate budgets: compute per run step, total budget
per request/operation, I/O wait time and service lifetime. The current per-run
deadline (5 s), fuel (1M) and host-call (1024) ceilings cannot simply be applied to
a long-lived application, and `await` must not renew the total budget. Epoch
interruption is already enabled in the engine configuration and is the basis for
per-step limits. Cancellation attaches to operations through AbortSignal; Promise
itself is not a cancellation mechanism. Ordinary cancellation may be delivered as
a rejection; termination for resource exhaustion remains a separate fatal path.

**Unhandled rejections.** After the microtask queue drains, unhandled rejections
are checked. By default they terminate the corresponding execution (the request or
the long-lived isolate) with a diagnostic; a handler attached in time removes the
candidate. Tracking is separate from reaction policy, following
[HostPromiseRejectionTracker](https://tc39.es/ecma262/2025/multipage/control-abstraction-objects.html#sec-host-promise-rejection-tracker).

**Record/replay (research).** A fixed queue order is not sufficient for
reproducibility. Record all external inputs: I/O results, time, random values,
cancellations and completion delivery order. Replay also verifies the sequence of
requested operations and reports any divergence.

**Verification.** Differential event-order logs against Node for identical
programs (like the existing comparison oracle), conformance-runner support for
asynchronous harnesses (`$DONE`/asyncHelpers) before using Test262 Promise/async/
generator cases, and explicit tests for roots across suspension, late/cancelled
completions, reentrancy, fatal-versus-rejection separation and request isolation.

**Implementation order and exit criteria.** Each step closes only with positive,
negative and resource tests, Wasmtime validation of emitted modules, source-free
artifact execution and a documented contract in [architecture.md](architecture.md).

1. Foundation block: expression normalization → frames over closure cells →
   exception and GC invariants → closed dispatcher. Exit: a compiler-internal
   suspension point round-trips through a frame with exact roots, a forced
   collection and a pending exception, and every unsupported position fails with a
   construct-specific diagnostic. **Implemented compiler-internally:**
   `lower::suspension`, `emit::frames` and `function::frame`; the Wasmtime tests
   are in `varta-compiler` `suspension_tests`. Still open for this block:
   - runtime admission: **implemented** under profile v33 and its host ABI
     (selector `resumable_frames`). Runtime preflight and artifact validation
     share `varta_common::function::admission`: catalog version 2 only for v33,
     the exact closed `varta_resume` signature and body, one resume body per
     layout, creators and binding classes, typed refusals for every earlier
     profile. Tests: runtime `frame_admission_tests` (including a forced
     collection in a runtime-owned store), common `admission_tests` and the
     launcher's source-free artifact run. Decided with the project author on
     2026-10-29: admit it in its own profile v33 together with step 2 (the
     first source syntax), rather than adding a runtime surface to the v32
     pilot batch that no source can reach.
   - suspension inside `catch`/`finally` bodies and `switch` case bodies:
     **implemented** with step 2; `switch` case tests, `do-while` conditions
     and the remaining intrinsic value blocks stay rejected by name;
   - host-supplied reference inputs;
   - liveness-based frame slots: **implemented** under v35 (selector
     `frame_liveness`,
     [contract](architecture.md#frame-liveness-frame_liveness)): each point
     saves only the locals live across it, directly in liveness segments
     whose edge masks are the exact GC map of the suspended state, described
     by frame catalog version 3 and admitted for v35 only; v33/v34 keep their
     byte-identical cell layout. Measured with production budgets: about 1.6k
     instead of 3.0k fuel per `await`, 1.1k instead of 1.8k per `yield`, and
     141 instead of 57 suspended generators per execution (`varta-runtime`
     `frame_liveness_tests`);
   - a scheduler-owned retention policy for completed frames: partly
     addressed by liveness frames, which clear their GC maps on resumption so
     a completed frame retains only its completion cell; an explicit policy
     (for example releasing that cell once the scheduler consumed it) is
     still open.
2. Synchronous generators: `yield`, resumption and `finally`. Exit: `next`/`throw`/
   `return`, re-entry rejection and `finally` ordering match ECMAScript in
   differential tests against Node. **Implemented** in v33 (selector
   `generators`, [contract](architecture.md#synchronous-generators-generators)):
   annotated top-level and generic `function*`, `yield` in normalized
   positions and in `try`/`catch`/`finally`/`switch` bodies, the full
   `next`/`return`/`throw` state protocol with the re-entry `TypeError`, and
   `for...of` with iterator closing. Generators add no value tag (value ABI
   unchanged); generator frames are owned by their objects, resumed by a closed
   per-type step function and unreachable through `varta_resume`. Six runtime
   programs match pinned Node 24 logs (`varta-runtime` `generator_tests`), plus
   compiler validation/diagnostic tests, a `varta dev` and source-free artifact
   CLI test and the `generators` conformance case. Open: `yield*`, spread and
   `Array.from` over generators, generator methods and expressions,
   `IteratorResult` narrowing by `done`, generators inside aggregates and
   unions (needs a value tag) and the iterator protocol of other objects.
3. Microtask queue and base Promise semantics. Exit: `then` chains, thenable
   adoption, self-resolution TypeError and FIFO job order match Node event logs.
   **Implemented** in v34 (selector `promises`, branch `v34-async`): a
   runtime-owned bounded FIFO queue per execution of generation-checked resume
   tokens, drained after `main` only through `varta_resume`; promises are
   guest-heap records without a value tag; every reaction, adoption and
   `finally` step is a scheduled job frame of a synthesized function, so job
   counts and order match Node (`varta-runtime` `promise_tests` pin Node 24
   logs). Five scheduler imports join the v34 host ABI; unhandled rejections
   terminate the execution with their reason. Decided in this increment:
   resolve functions take `T` only (adoption goes through `then` and async
   returns), and scheduler bookkeeping is bounded by its own limits instead of
   the host-call budget. Open: structural thenables, the combinators and
   promises in aggregates (they need a value tag), `Promise.withResolvers`.
4. async/await over the same frames. Exit: `await` in normalized expression
   positions, try/catch/finally across `await` and unhandled-rejection policy.
   **Implemented** in v34 (selector `async_functions`): annotated top-level and
   generic `async function` declarations including async `main`, `await` in
   normalized positions, `try`/`catch`/`finally` across `await` and
   returned-promise adoption (`async_tests`, Node 24 logs). Open: `await`
   inside intrinsic value blocks (string concatenation, template literals,
   intrinsic calls), async arrows/methods/expressions, optional parameters,
   per-await fuel cost (frames save every local; liveness-based slots are the
   planned fix) and request-scoped isolates for async HTTP (step 5).
5. Timers, asynchronous I/O, cancellation and shutdown. Exit: separate budgets,
   generation-checked late completions and request-scoped cleanup.
6. Async generators, `for await`, top-level await and compatibility expansion.
   Exit: the uarp-sdk pilot steps M5–M6 compile and pass their tests.

The prepared function-catalog ownership increment, which owns the immutable
metadata that frame layouts extend, is committed (`bf8c6c2`).
### C1 — Tool Contract v1 and offline discovery (completed)

Entry-module `varta.tool({ name, description }, handler)` declarations now
publish explicitly exported functions. Checked primitive signatures supply named
parameters and results; metadata does not duplicate handwritten schemas.
Ordinary exports and dependency modules cannot silently publish tools.

- Shared strict types, limits and generated JSON schemas live in `varta-common`.
- Compiler lowering resolves the actual handler (including imported/re-exported
  bindings), derives transitive system requirements and emits a sorted catalog
  plus typed Wasm exports.
- Runtime validation binds metadata to actual Wasm export signatures and recomputes
  transitive effects. Malformed, duplicate, oversized, incompatible and inconsistent
  catalogs fail before execution. Indirect calls and Wasm start functions are
  outside tool invocation ABI v1.
- Project inspection returns public schemas, effects, limits and existing package
  permissions. It compiles/validates portable code without instantiating or running
  it. The CLI only presents that metadata.
- Contract v1, invocation ABI v1, host ABI v1, manifest schema v1 and native
  container format v1 are distinct. Existing no-tool applications keep working;
  tool metadata is inside the Wasm payload, so packaging requires no new footer.
- String copy/retain exports establish fresh-instance invocation ownership and are
  tested across input-buffer reuse and collection. C2.1 below now supplies the JSON
  dispatcher. `domainErrors: []` leaves typed application errors for later C2 work.

Exit verified: compile a primitive string tool, inspect the same catalog in Wasm
and native packages after deleting sources, reject invalid catalogs/signatures/
effects, preserve permission denials and demonstrate that inspection does not run
privileged application code. No LLM connection or MCP server is implemented.

The complete implemented syntax, limits and ownership rules are in the
[tool contract](architecture.md#tool-contract-v1-and-offline-discovery).
### C2 — Validated invocation and generic application state (partly complete, deferred)

The typed invocation path is implemented. Generic state ownership, transactional
updates and shared application lifetime remain deferred until the active language,
JSON, server-lifecycle and packaging work is established.
#### C2.1 — Checked named JSON invocation (completed)

- Shared runtime/project APIs and `varta invoke <artifact> <name> --params '<JSON>'`
  dispatch only published tools, without calling `main` or executing native launchers.
- A bounded closed-object parser rejects duplicate/unknown/missing parameters,
  wrong types, invalid UTF-8 and non-i32 input before instantiation. Integers use
  exact plain integer tokens; decimal/exponent and negative-zero forms are rejected.
- All argument helpers and the selected handler share a fresh store, fuel,
  memory, deadline and host-call budgets. String copy/retain results are checked;
  strings returned to the host are bounds/UTF-8 checked and copied immediately.
- Canonical booleans and the encoded result envelope are validated. JSON output
  counting avoids oversized temporary serialization buffers. Logs remain separate.
- Stable invocation codes distinguish request, catalog/code, policy, result,
  execution and guest-exit failures. Recoverable host statuses and package/host
  resource authorization keep their previous semantics.
- Tests cover real Wasm/native invocation without sources, no startup/private
  dispatch, strict JSON, primitive boundaries, buffer reuse/collection, hostile
  result values, escaped-output limits, traps, recovery and permission denials.

This completes the dispatcher slice, not the live application milestone. The
[implemented invocation contract](architecture.md#checked-json-invocation) describes
its bounds. It has no persistent state, caller identities, remote listener,
rollback, automatic retry or application-wide queue/accounting yet.
#### Remaining C2 work (after K1–K3)

- Design the generic application-state contract and ownership/transaction ABI
  before implementation: application-defined state types, bounded host storage,
  guest access, validated changes and atomic commit. The engine must not know
  document fields, command names or domain rules. Revision preconditions belong
  to generic request metadata rather than reserved application parameter names.
- Add typed application/domain errors on top of the stable invocation codes.
  Keep them separate from host statuses, protocol errors and traps; revision
  conflicts must not become host I/O errors.
- Define application identity, startup/shutdown, state ownership, request identity,
  ordering, revision checks, cancellation and retry/idempotency semantics.
- Validate a proposed initial model: a long-lived trusted application service owns
  state and serializes mutations; fresh bounded Wasm invocations implement commands.
  The application's lifetime is independent of any agent or MCP connection.
  Decide this model explicitly before adding persistent guest memory or concurrency.
- Preserve the checked string-transfer boundary when introducing persistent
  host-owned state. State must never retain guest pointers after a call ends.
- Keep one command/query path for UI/API/MCP callers. Apply authorization and state
  preconditions at that path, then commit a mutation atomically. A stale revision
  yields a typed conflict; transport failure must not trigger an automatic write retry.
- Enforce per-call budgets and bounded application-level queues/accounting so that
  reconnecting or making repeated calls cannot reset all resource controls.

Exit: generic core tests demonstrate shared state across client connections,
revision conflicts, authorization and atomic rejection of malformed/trapping
commands. A separately built TS/JS document application then demonstrates reading
and renaming through those public APIs with two clients. Its title schema,
commands and validation live in the application. State survives client disconnects;
disk persistence and a GUI are separate requirements, not implied by this milestone.
### C3 — Artifact provenance and trusted loading

Deliver package verification before third-party distribution or treating tool
metadata as publisher-authenticated.

- Define trust roots, signed bytes, contract/code/permission binding and signature
  verification before invocation. Cover versions and payload lengths as well as
  code; changing the catalog or manifest must invalidate authenticity.
- Define publisher identity, key rotation/revocation, update and rollback rules.
  Keep signing credentials outside shipped artifacts and guest memory.
- Separate package authenticity from caller authorization and native OS signing.
  A valid signature cannot expand host grants, and an unsigned developer build
  must not silently acquire trusted-package status.
- Use explicitly trusted local development builds for earlier prototypes. Preserve
  safe portable Wasm validation and do not restore arbitrary native deserialization.

Exit: code, contract and permission tampering is rejected by a trusted loader;
trusted/unknown/revoked publishers are distinguishable, and host ceilings still
restrict a correctly signed application. AOT remains a later, separate design.
### C4 — Local MCP bridge to the running application

Deliver external-agent discovery and control of the C2 application through MCP.

- Pin the supported MCP revision(s) and transcript compatibility tests. Implement
  deterministic, bounded `tools/list` and validated `tools/call` from the existing
  catalog/dispatcher, with protocol metadata and errors appropriate to that revision.
- Start with stdio. Separate protocol output from application logs and enforce
  request size, queue, response size, timeout and cancellation limits.
- For an already-running desktop application, the stdio bridge connects through
  an authenticated local channel to a specific existing instance. Launching another
  bridge must not silently create a second document or divergent application state.
- Bind callers to the intended application/contract identity, expose the permitted
  catalog, and authorize every concrete invocation. Tool descriptions/annotations
  remain untrusted data and do not override host or caller policy.
- Test disconnect/reconnect, multiple agents, duplicate requests and changing
  document revisions. State belongs to the application, not the MCP session.

Exit: an external MCP client lists and invokes published tools from a separately
compiled TS/JS application through the common command path, including generic
state/revision behavior when supported. No application command names or state
fields are built into the bridge. No source code, embedded model or AI-provider SDK is needed
inside the Wasm module. No remote listener is enabled by this local milestone.
### C5 — Generic adapters and remote access (deferred)

- Define versioned host-adapter boundaries, permissions, ownership, event handling
  and resource budgets before adding optional platform integrations.
- Add authenticated remote MCP with HTTPS, explicit listener configuration and
  network deployment tests after local generic dispatch works.
- Establish a supported OS/CPU/runtime matrix and packaging tests. Evaluate small
  devices against actual runtime/memory constraints rather than assuming support.
- Keep GUI behavior, database business operations, document history and robot
  missions/safety workflows in separate applications or integrations. They are not
  built-in engine models. Future external projects validate generic adapter APIs
  only after the required core mechanisms have been implemented.

Desktop/server/robot support must not be claimed merely because Wasm and MCP run
on a development host. Selecting or implementing a reference application is not
part of current core development.
### K5 — Modules, standard system APIs and package compatibility

- Grow ES module resolution/loading under explicit host authority and resource
  budgets; track implemented import/export and initialization semantics.
- Expand filesystem/process/network APIs as generic runtime facilities with
  operation/resource-specific permission checks and portable error behavior.
- Evaluate real package compatibility against an explicit target. Separate loader,
  language and Node/Web API gaps; do not promise npm compatibility from syntax alone.
### K6 — Conformance and runtime readiness

- Pin a supported ECMAScript edition/profile and build a compatibility matrix
  with a maintained conformance-suite strategy plus Varta-specific runtime tests.
- Exercise ordinary server/CLI/library programs through actual public APIs,
  independent of MCP and domain-specific host implementations.
- Track correctness, malformed-input/security behavior, leak-free lifecycle,
  startup/memory/throughput and supported platforms before calling the runtime ready.
### Supporting language and runtime work

- K1 provides the initial checked values/JSON path. Continue resizable/dynamic
  collections, enums and numeric-profile expansion through explicit semantics/ownership
  contracts. Full JS/npm compatibility remains a separate compatibility effort.
- Preserve tracing for strings, records and arrays as richer value graphs are
  introduced; new value kinds need explicit collector and ownership contracts.
- Async/events require cancellation, task/handle lifetime, retained roots, scheduling
  and queue budgets before exposing promises or long-running guest operations.
- Continue model/differential tests, fuzzing and performance measurements of the
  compiler, collector, boundary schemas and artifact parser throughout these phases.

Every implementation milestone retains identical shared check/build/dev acceptance,
versioned documentation, positive and denial tests, bounded malformed-input handling,
Wasmtime validation/behavior coverage and the existing quality gates. Planned
features are described as planned until their exit criteria actually pass.

Protocol references for planning: [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
and [MCP transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports).
Choose and test exact compatibility targets when implementing C4; the catalog
and application core remain independent of transport/protocol revisions.
## Later milestones

- HTTPS deployment and asynchronous execution follow the C5 and supporting-work
  contracts above; neither is supplied by the initial synchronous HTTP handler profile.
- Authenticated AOT: define who trusts the signer, which bytes are authenticated,
  and how engine version, target and CPU requirements are pinned. Never restore
  arbitrary native deserialization behind a filename or format marker.
- Package compatibility: only after module/type/memory semantics are stable;
  evaluate packages against a documented subset rather than promising Node parity.
- Production: fuzz schemas/containers/Wasm boundaries, expand platform coverage,
  isolate compilation and blocking host operations where hard OS-level resource
  isolation is required, and obtain an independent security review.
