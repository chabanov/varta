# Varta development plan

## Status

Updated 2026-10-29. Varta is an experimental research project: a compiler and
capability-checked runtime core with K1–K3 implemented in their initial checked
profiles and K4 language/value work in progress. It is not a production release
and does not claim full ECMAScript, Node or npm compatibility.

| Item | Current state |
| --- | --- |
| Profiles | Default `checked-i32-utf8-v1`; latest released opt-in `checked-f64-utf16-v31`; `checked-f64-utf16-v32` is the [pilot batch](#pilot-batch-v32-in-development), merged on the `v32-integration` branch and not released. Versions of every profile: [version matrix](architecture.md#version-matrix) |
| Last full gate | `main`: commit `bf8c6c2` (2026-10-28), 1750 tests. `v32-integration` with both pilot waves: commit `45a6af1` (2026-10-29), 1908 tests. `v34-integration-merge` with the records, unknown and async streams: commit `02d1a75` (2026-10-29), 2018 tests, 0 failures. `v36-merge` with the declared-fields, function-fields, timers and async-generator streams: commit `765743e` (2026-10-29), 2196 tests, 0 failures, conformance 101 passed and 9 known gaps. All are the seven-command gate in [verification.md](verification.md#full-gate) on a local host, not cross-platform CI results |
| Compatibility inventory | Current counts and gaps: [conformance/README.md](../conformance/README.md) |
| Completion | Measured by the [core completion ledger](#core-completion-ledger); no work stream is closed by this snapshot |
| Pilot | [uarp-sdk 0.12.0](#pilot-acceptance-package-uarp-sdk); M1 in progress: on the `v36-merge` branch `generated/meta.ts`, `generated/models.ts`, `core/json.ts`, `core/pagination.ts` and `core/errors.ts` compile and validate; the other files stop at the Web streams of `core/sse.ts` or a closure capture before initialization in `core/util.ts` ([measurement](#pilot-acceptance-package-uarp-sdk)) |
| Platform targets | Third-party embedding contract published in [embedding.md](embedding.md); browser, desktop and Raspberry Pi 5 targets, application lifecycle and view layer follow the [accepted design](#platform-targets-and-application-model) (2026-10-29), not implemented yet |

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
   step 5's timers and step 6's async generators and `for await` are
   implemented under v36 and integrated on `v36-merge`
   ([timers](architecture.md#timers-timers),
   [async generators](architecture.md#async-generators-async_generators));
   I/O and async HTTP are next.

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
| CORE-17 | Application lifecycle | Planned ([accepted design](#application-lifecycle-and-event-turns)) | Long-lived instances, event turns with per-turn budgets, host→guest task delivery, timers and I/O completions, no automatic restart after a trap |
| CORE-18 | View output | Planned ([accepted design](#view-layer-l0l3)) | L0 protocol over the specified HTML/CSS subset, headless, browser and native renderers, cross-renderer layout and accessibility conformance |

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

Apart from the product example `examples/uarp-console/` (the
[cross-target acceptance application](#uarp-console-cross-target-acceptance-application),
decided on 2026-10-29), no separate application project is built in this
repository; the example is not a workspace member and no crate depends on it. The scenarios
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

**Program limits under v38 (2026-10-30).** The program-limits stream
(`program_limits()`) on branch `v38-program-limits`, production budgets
unchanged, debug `varta` build. *Real pilot:* the per-file probe on the
scratch copy carrying the v37 walk's stubs (streams, binary bodies, RegExp,
`globalThis`): under v37 32 of 65 files stop at the record-shape limit (25
resource modules, `models.ts` and the collections prelude); under v38 no file
stops at a program-size limit — 6 compile, 49 stop at `core/sse.ts:111` (item
12 above, property-path facts across a call) and 10 at `Blob`. *Synthetic
mimic* (the generator of the v36 limits measurement: N models, M resource
methods each returning `request<Model>()`, a `Client` constructing every
resource, plus N·356/1900 enum-like constants; "full" makes every method
reachable through a never-taken branch, "minimal" awaits one call):

| N / M | Shape | v37 production | v38 functions | Wasm | Static | Largest body | Wasmtime compile | Fuel (start + 1 call) | Memory |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 128 / 64 | minimal | union limit | 39 | 34 KB | 2.4 KB | 3.5 KB | 10 ms | 15 K | 1.2 MiB |
| 128 / 128 | full | union limit | 1314 | 2.8 MB | 136 KB | 66 KB | 1.0 s | 17 K | 1.3 MiB |
| 1900 / 900 | minimal | union limit | 79 | 76 KB | 4 KB | 12.7 KB | 23 ms | 42 K | 1.2 MiB |
| 1900 / 900 | full | union limit | 10107 | 30.7 MB | 1.07 MB | 69 KB | 8.2 s | 43 K | 2.2 MiB |
| 4096 / 2048 | minimal | union limit | 122 | 73 KB | 6 KB | 24 KB | 28 ms | 54 K | 1.2 MiB |
| 4096 / 2048 | full (awaiting drivers) | union limit | 22903 | 75.7 MB | 2.6 MB | 145 KB | 40 s | 58 K | 3.7 MiB |

Every v38 row compiles and runs under production budgets; `varta run` of the
1900/900 full graph completes. Against the v36 measurement with a temporarily
raised build (1900/900: 8165 functions, 9.7 MB, 766 KB static, a 221 KB
`varta_resume`, ≥816 K fuel and a static-metadata failure at run time),
startup plus one call drops from ≥816 K to 43 K fuel (no data-segment or
unread-constant cost, constant-fuel dispatch), the largest body is no longer
the dispatcher, static data spills into the extension, and a minimal use
keeps only reachable code (79 functions instead of every resource method).
Still open: each distinct `request<T>` response type costs about 11 Wasm
functions (async frame, resume body and per-`Promise<T>` helpers; the
checked `JSON.parse(text) as T` already decodes through the host's
schema-driven parser, so its cost per type is a schema in static data) and,
in the full graph, about 3 KB of code per function, three times the v36
figure — most of it resume and promise code, for the code-generation stream.
Wasmtime compile time (8 s for 31 MB) now dominates loading a full graph.
Scripts and raw results stay outside the repository.

**Verified behavior with v37 (2026-10-29).** The same per-file probe on the
`v37-merge` branch with all five v37 streams integrated (language core, class
generics, collections, RegExp, `net.request`) and the integration fixes,
under `checked-f64-utf16-v37` with production budgets unchanged; the v36
column comes from the same binary:

| First diagnostic under v37 | Files | Under v36 |
| --- | --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts`, `core/pagination.ts`, `core/errors.ts` | 5 | the same 5 |
| `core/sse.ts:227` `unsupported generic class type reference` (`ReadableStream<Uint8Array>`, the byte body of `parseEventStream`; reached by `core/sse.ts`, `core/transport.ts`, `core/resource.ts`, `index.ts` and the 55 resource modules) | 59 | `core/sse.ts:227` `unsupported type reference ReadableStream` |
| `core/util.ts:41` `unknown identifier: globalThis` (`randomId`'s `globalThis.crypto` feature test) | 1 | `core/util.ts:28` capture before binding initialization |

Both v36 blockers are gone: `ReadableStream` exists (text chunks) and the
`sleep` timer callback captures `onAbort` before its declaration. A
whole-program walk on a scratch copy (the pilot sources are never edited),
stubbing each blocker to reach the next, finds in order:

1. `ReadableStream<Uint8Array>` and `ReadableStreamReadResult<Uint8Array>`
   (`core/sse.ts:227`, `core/transport.ts:300`): byte bodies; v37 streams are
   text. Deferred to v38.
2. `core/sse.ts:6` the generic method signature `json<T = JsonValue>(): T`
   in `UarpEvent`.
3. `core/sse.ts:80` `implements AsyncIterable<UarpEvent>`: fixed in the
   integration.
4. `core/transport.ts:60` the `object` type annotation (`form?: object`).
5. `core/json.ts:10` `Blob`, `ArrayBuffer`, `ArrayBufferView` in
   `BinaryInput`; then `core/transport.ts:358` `FormData` and
   `response.blob()` (multipart and binary bodies).
6. `generated/models.ts:16453` `object shape count exceeds limit`: the whole
   program needs more than 128 record shapes (the 1203rd interface).
7. With a core-only entry (`new Transport(...).request<Item>(...)`):
   `core/util.ts:41` `globalThis.crypto`, `core/transport.ts:115`
   `globalThis.fetch as FetchLike | undefined` and `:425`
   `(globalThis as {...}).process?.env`. Deferred to v38.
8. `core/util.ts:79` `for (const item of value)` after `Array.isArray(value)`
   on `unknown` (no array narrowing of `unknown`). `JSON.stringify(value)` of
   `unknown` on line 85 now compiles.
9. `core/sse.ts:276` `Promise.race([reader.read(), timeout])`: two promise
   types in one union.
10. `core/sse.ts:353` `reader.releaseLock?.()`: an optional call of a class
    method value.
11. `core/sse.ts:367` object spread of a parameter with optional fields.
12. `core/sse.ts:177` and `:111` property-path facts dropped by an
    intervening call (`if (!response.body) throw …; this.#onState?.(…);
    parseEventStream(response.body, …)`), where TypeScript keeps the
    narrowing: the receiver is a union again.
13. `core/transport.ts:332` `JSON.stringify(value)` typed `string |
    undefined` where TypeScript declares `string`.

With the stream and binary stubs applied, the per-file probe checks 6 of 65
files (`core/util.ts` joins); 27 stop at the call of item 12, 25 at the
record-shape limit and 7 at `Blob` in their resource modules.

*Limits.* Unreachable functions no longer take slots under v37 (only
functions reachable from the entry points count against 256): a fetch hello
world went from 228 linked to 154 reachable functions (156 with `URL`
inputs). Each distinct `request<T>` response type costs 10 functions, so 24
distinct response types fit one program
(`class_generic_tests::pilot_request_pattern_function_counts`); the whole SDK
declares 670. The 128 record-shape limit is now the first program-wide limit:
interfaces used by a single resource module already exceed it in 25 files.

**Verified behavior with v36 (2026-10-29).** The same probe on the
`v36-merge` branch with all four v36 streams integrated (checked declared
fields, function fields with value layout 17, timers and async generators,
host ABI 32), under `checked-f64-utf16-v36` with production budgets unchanged.
The first diagnostic per file, with the v35 result from the same binary:

| First diagnostic under v36 | Files | Under v35 |
| --- | --- | --- |
| Compiles and validates: `generated/meta.ts`, `generated/models.ts`, `core/json.ts`, `core/pagination.ts`, `core/errors.ts` | 5 | 4 (all but `core/errors.ts`) |
| `core/sse.ts:227` `unsupported type reference ReadableStream` (the parameter type of `parseEventStream`; reached by `core/sse.ts`, `core/transport.ts`, `core/resource.ts`, `index.ts` and the 55 resource modules) | 59 | `core/errors.ts:4` declared property `errors` not checkable from `unknown` |
| `core/util.ts:28` `unsupported capture before binding initialization` (`sleep`'s timer callback names `onAbort`, a `const` declared after it) | 1 | `core/util.ts:125` defaulted function-typed parameter |

Both v35 blockers are gone: `ProblemDocument.errors` is a checked declared
field and `streamBackoffDelay`'s `random: () => number = Math.random`
compiles. In a scratch copy (sources outside this repository, each step
patched minimally, then the module checked again), the next blockers in order
are:

1. `core/util.ts`: the TDZ capture above (with `onAbort` first and
   `let timer = 0`, the pilot's abortable `sleep` with `setTimeout`, abort
   listeners and optional chains compiles); `globalThis.crypto` and
   `Uint8Array` (`randomId`); `TextEncoder`, then `String.fromCharCode` and
   regular expressions (`encodeQueryComponent`); `for…of` over an
   `Array.isArray`-narrowed `unknown` (an `unknown[]` view); `Object.entries`
   pairs typing `[key, value]`'s `key` as `unknown` (no tuple types);
   `JSON.stringify` of `unknown` in a program that loads a Web prelude;
   `Array.prototype.join`; `RegExp` and `String.replace` (`joinUrl`);
   `Date.now()`/`Date.parse` (`parseRetryAfter`). With these stubbed,
   `core/util.ts` compiles.
2. `core/sse.ts`: Web streams (`ReadableStream`, `getReader`,
   `ReadableStreamReadResult`, `TextDecoder`) and `Response`; method
   signatures in interfaces (the generic `json<T>()` of `UarpEvent`);
   `implements AsyncIterable`; `#private` fields; a class field typed by its
   `new` initializer; `Set<string>`; a union of several record shapes
   (`StreamState`); `[Symbol.asyncIterator]`; `Array.prototype.join`; a
   `() => void reader.cancel()` arrow (result `undefined`) where a
   `() => void` listener is expected (reported as `expected function, found
   function`); `let x: T;` assigned later; `ReturnType<typeof setTimeout>`;
   `Promise.race` over differently typed promises; regular expressions; a
   `switch` on strings; an object spread with an extra method; property-path
   narrowing (`response.body`, `event.retry`); a constructor closure capturing
   `this`; an uninitialized `string | undefined` class field; `Date.now` as a
   function value; and finally `parseEventStream`'s body exceeds the resume
   compilation budget (production budgets unchanged). With these patched and
   that body stubbed, `core/sse.ts` compiles: the `EventStream` class with its
   reconnecting async generator method, abortable backoff sleeps and abort
   listeners.
3. `core/transport.ts:300`: the Fetch surface (`ReadableStream`, `Response`,
   `Request`, `URL`, `URLSearchParams`, `Blob`, `FormData`) and `#private`
   fields.

**Verified behavior with v36, async stream (2026-10-29).** The same probe on
the `v36-async-generators` branch (selector `async_generators` only) under
`checked-f64-utf16-v36` gives the v35 table unchanged: the records and
functions streams own its two diagnostics. In a scratch copy with both patched
as above (`errors?: unknown`, no default for `random`), 59 files stop at
`core/sse.ts:227` `unsupported type reference ReadableStream` — the parameter
type of `parseEventStream`, whose `async function*` declaration is now
admitted — and two files stop outside the async area (`core/errors.ts:136`, a
`switch` returning on every path that the return check does not accept, and
`core/util.ts:23`, `sleep`'s `new Promise` without a type argument around
`setTimeout`). `parseEventStream` itself also needs Web streams
(`ReadableStream`, `getReader`), `TextDecoder` and `Uint8Array`, `RegExp`
literals, `Set<string>`, a `switch` on strings, timers (`setTimeout`,
`ReturnType<typeof setTimeout>`), `Promise.race` over differently typed
promises and object methods in `makeEvent`; `EventStream` needs
`[Symbol.asyncIterator]` and `implements AsyncIterable`. `core/pagination.ts`
compiles with `autoPaginate<T>` instantiated: `collect`'s
`limit = Number.POSITIVE_INFINITY` default, `(await fetchPage(cursor)) as …`
and `AsyncIterable<T>` inference were fixed in this stream, and the remaining
blockers are `new Set<string>()` and `for (const item of items)` over an
`Array.isArray`-narrowed `unknown` (an `unknown[]` view, records stream); with
both rewritten in a scratch copy it runs and pages through typed pages as
Node does.

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
  `getRandomValues` have no host operation yet; decided on 2026-10-29: entropy
  stays ungated, like `Math.random`.
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
a v36 module relabelled as v35 fails validation before any guest effect, and
runtimes that do not list v36 reject its declared profile name
([contract](architecture.md#checked-f64-utf16-v36--pilot-batch-checked-declared-fields-function-fields-async-generators-and-timers)).
All four streams are implemented and integrated on `v36-merge`, with
cross-stream tests (`varta-runtime` `v36_cross_stream_tests`), `AbortSignal.any`
in the v36 abort prelude and the conformance pin at v36 (the timer case runs
with a reviewed per-case `time.timer` grant). Both v35 pilot blockers are gone;
the [v36 measurement](#pilot-acceptance-package-uarp-sdk) lists the next ones.

| Selector | Work stream | Pilot step | State |
| --- | --- | --- | --- |
| `checked_declared_fields()` | Records: mixed-record declared fields checkable from `unknown`, such as arrays of optional records | M1, M2 | Implemented ([contract](architecture.md#checked-declared-fields-checked_declared_fields)): convertible declared fields with exact-or-built reads and write-back, checked parsing and assertions converting JSON graphs, any-layout record descriptors, dictionary destructuring, `typeof` property-path conditionals, `??`/`\|\|` literal fallbacks, `let x: unknown;`, exhaustive `switch` returns |
| `function_fields()` | Functions: function-valued record and class fields; optional and defaulted function-typed parameters (owns the value17 bump) | M2, M4 | Implemented ([contract](architecture.md#function-fields-function_fields)): value layout 17, function fields/elements/unions/`unknown`, optional and defaulted function parameters, optional chains, `Math.random` as a value, abort listeners, `Promise.withResolvers`; `resolve(promise)` and checked `as` to function types pending |
| `async_generators()` | Async: `async function*` generators and `for await…of` | M5 | Implemented ([contract](architecture.md#async-generators-async_generators)): declarations, generic instantiations and class methods, the queued `next`/`return`/`throw` protocol, `yield*` over async generators, `for await` over async generators, arrays and generators; async generator expressions and `Symbol.asyncIterator` open |
| `timers()` | Timers: `setTimeout`/`clearTimeout` and timer-driven promises (host ABI 32 imports) | M4, M5 | Implemented ([contract](architecture.md#timers-timers)): `setTimeout`/`setInterval`/clears, sleep pattern, `AbortSignal.timeout` (with v36 abort listeners), exact `time.timer` grant |

### Pilot batch v37 (in development)

The next pilot batch targets the blockers of the
[v36 measurement](#pilot-acceptance-package-uarp-sdk) in profile
`checked-f64-utf16-v37`, developed in five parallel streams: language-core
gaps, class generics, collections and binary text, RegExp and the asynchronous
network request capability. V37 inherits every v36 feature with value
ABI/layout 17 and declares its own host ABI 33 (`abi::NET_REQUEST_VERSION`),
under which every host import of the batch is added; a v37 module relabelled
as v36 fails at its host ABI section, and runtimes that do not list v37 reject
its declared profile name. The collections stream owned any value-layout bump
and needed none, so v37 keeps value layout 17
([contract](architecture.md#checked-f64-utf16-v37--pilot-batch-language-core-class-generics-collections-and-binary-text-regexp-and-netrequest)).
All five streams are implemented and integrated on `v37-merge`; the
[batch integration](architecture.md#batch-integration-v37) contract covers
how they combine (unreachable-function removal, private state in JSON,
`undefined`-aware index reads, recoverable clock denials, one `split` path,
`URL` fetch inputs). Pending for v38: byte bodies
(`ReadableStream<Uint8Array>`, `Blob`, `FormData`, `ArrayBuffer`),
`globalThis` member access, generic method signatures in interfaces,
`Array.isArray` narrowing of `unknown`, unions of collections with other
objects and `instanceof Map`/`Set`, and the record-shape limit
(see the [v37 measurement](#pilot-acceptance-package-uarp-sdk)).

| Selector | Work stream | Pilot step | State |
| --- | --- | --- | --- |
| `language_core()` | Language core: TDZ-safe closure capture before declaration, deferred `let x: T;` initialization, dense `for…of` typing, property-path narrowing, `switch` on strings, function assignability fixes | M3, M4 | Implemented ([contract](architecture.md#language-core-language_core)): captures before initialization with run-time TDZ (`ReferenceError`-named Error instances), hoisted nested function declarations, definite assignment of `let x: T;`, element-typed `for…of`/`xs[i]`/`s[i]` with checked holes (TypeError deviation), property-path narrowing with call/write invalidation, `switch` by strict equality with clause narrowing, value functions as `void` callbacks with signature diagnostics, endless-loop return analysis, undefined-capable and `new C()` class fields, constructor closures over `this.field`, unions of several record shapes, classes and arrays (descriptor and element-kind member tests, discriminant/`in` narrowing, field reads and method calls on the union, discriminated JSON conversion), `never` exhaustiveness, module `let`/`var` variables with live exports, function-valued module constants, self-referencing timer handles. Open: `instanceof ReferenceError`, captures of destructured or loop-head bindings before initialization, `Array.isArray` narrowing of `unknown` to an array, `A[] \| B[]` of record element types, class expressions in module initializers |
| `class_generics()` | Classes: generic methods and classes, interface method signatures, `implements`, `#private` | M4, M5 | Implemented ([contract](architecture.md#class-generics-class_generics)): monomorphized generic instance/static/async methods with inference from arguments and the contextual result, per-instantiation override dispatch, generic classes with generic bases and declared subclasses of instances, method signatures as function fields and object-literal methods without `this`, checked `implements`, `#private` fields/methods/getters and `#x in v`. Pending: generic method signatures (`json<T>(): T`), overrides inside generic classes, `instanceof` of a generic class on `unknown`, abstract classes. Measured: about 10 Wasm functions per distinct `request<T>` response type, 24 distinct types per program against the pilot's 670. Pilot probe (unmodified pilot): first blocker unchanged (`ReadableStream`, net-request stream); with the transport/SSE modules stubbed in a scratch copy 29 of 65 files check (5 before), the rest blocked by other streams (object spread of optional parameters, `Blob`, unions, the shape limit, `encodeURIComponent`) and by checked `as` to `void`/`JsonValue` in `request<T>` instances |
| `collections()` | Collections and binary text: `Map`/`Set`, tuples, `Array.join`, `String.fromCharCode`, `Date` subset, `Uint8Array`, `TextEncoder`/`TextDecoder`, `crypto.getRandomValues`, base64 (owned the value-layout bump; needed none) | M4, M6 | Implemented ([contract](architecture.md#collections-and-binary-text-collections)): `Map`/`Set` with hashed lookup and live iteration, tuples (and `Object.entries` pairs), `padStart`/`padEnd`/`split`/case mapping/`join`/`fromCharCode`/`fromCodePoint`, `Uint8Array`, UTF-8 `TextEncoder`/`TextDecoder` with streaming, `btoa`/`atob`, `crypto.getRandomValues`/`randomUUID`, UTC `Date` with ISO/IMF parsing and the `time.now`-gated clock (host ABI 33 imports `string_to_upper_case16`, `string_to_lower_case16`, `date_now`). Private storage (no value tag): logging, JSON and `unknown` reject these values; `subarray`, `ArrayBuffer`, union keys and `globalThis.crypto` open |
| `regexp()` | RegExp: `RegExp` and the regex string methods | M4, M6 | Implemented ([contract](architecture.md#regexp-regexp)): literals and constant `new RegExp` compiled ahead of time to a linear-time guest Pike VM (no host import), `exec`/`test`/`lastIndex`, match arrays, `search`/`match`/`matchAll`/`replace`/`replaceAll`/`split` with templates, replacer functions and string patterns; backreferences, lookaround, `\p{…}`, run-time patterns and the `d`/`v` flags open |
| `net_request()` | Network: async `fetch`/`Request`/`Response`/`Headers`/`URL`/`ReadableStream` over `net.request`, `Symbol.asyncIterator` for async iterables (host ABI 33 imports) | M5, M6 | Implemented ([contract](architecture.md#network-requests-net_request)): `net.request` with URL scopes and methods, hyper + rustls transport, streamed text bodies, abort and timeouts, redirects `error`/`manual`/`follow`, `URL`/`URLSearchParams` subset, `[Symbol.asyncIterator]` async generator methods, a recoverable denial through the [permission broker](architecture.md#permission-broker-v37) for requests and v37 timers; bytes bodies wait for `Uint8Array`, generic `ReadableStream<T>` for class generics |

### Batch v38 (in development)

The next batch addresses the scale limits and platform gaps that remain after
the v37 batch and the [v37 measurement](#pilot-acceptance-package-uarp-sdk) in
profile `checked-f64-utf16-v38`, developed in eight parallel streams. V38
inherits every v37 feature, with value ABI/layout 18 since wave 2 (hashed
dictionaries, below; value17 before), and declares its own host
ABI 34 (`abi::PROGRAM_LIMITS_VERSION`), under which every host import of the
batch is added; a v38 module relabelled as v37 fails at its host ABI section,
and runtimes that do not list v38 reject its declared profile name. The
program-limits stream owns the batch's memory-layout and value-layout
foundation (any value 18 or static-region ABI change) and lands it as an early
separate foundation commit
([contract](architecture.md#checked-f64-utf16-v38--batch-program-and-data-limits-turn-budgets-linked-intrinsics-view-protocol-and-tsx-structural-subtyping-and-codegen-v2)).
The selectors are registered; the table records what each stream admits.

| Selector | Work stream | State |
| --- | --- | --- |
| `program_limits()` | Program limits: profile-dependent structural limits up to the Wasm limits, a shared descriptor-driven checked decoder, multi-page static data, table-driven resume dispatch, dead-code elimination (owns any value18 or static-region ABI change) | Implemented ([contract](architecture.md#program-limits-program_limits)): multi-page static region (host ABI 34, no value 18; the foundation), profile limits registry with the Wasm limits, table-driven `varta_resume`, unreachable async frames and unread constants removed, startup fuel independent of static size. Pending: per-type `request<T>` code (async frame and promise helpers, about 11 functions per response type) |
| `data_limits()` | Data limits: arrays, dictionaries, `Map`/`Set`, `Uint8Array`, strings and JSON bounded only by the memory budget; more than 64 live frames without a trap | Implemented ([contract](architecture.md#data-limits-data_limits)): `DataLimits` registry (2^27 elements, 2^30 string bytes, JSON depth host ceiling 10000), value17 sparse backings above 1024 slots with one bit per slot, typed `RuntimeError::Resource` failures through the `resource_failure`/`resource_request` globals, geometric heap growth and a worklist mark, a hashed growable live-frame table and memory-derived scheduler bounds, `json_stringify_owned` (host ABI 34) into owned guest strings, iterative JSON parsing, lifted `Map`/`Set`/`Uint8Array` preludes and an O(n log n) `join`. Dictionary and mixed-record hashing landed in wave 2 (b) under value layout 18. Pending: per-element host calls of `String(x)` against the 1024 host-call budget (`%` no longer calls the host), frozen limits in `console.log` diagnostics and primitive conversions |
| `turn_budgets()` | Turn budgets: per-event-turn fuel and deadline configurable by the manifest within the host ceiling; fuel profiler and logs before failure in `varta dev` | Admits event turns with per-turn budgets, manifest budgets capped by the ceiling, long-lived application instances, host event sources and permission broker v1; logs before failure and `--budget-report` in `dev`/`run`/companion ([contract](architecture.md#turn-budgets-and-application-instances-turn_budgets)). Pending: per-function fuel attribution (codegen stream), per-turn UI values from measurements on the targets |
| `linked_intrinsics()` | Linked intrinsics: Rust pure intrinsics linked into the guest Wasm so third-party hosts stay thin | Implemented in part ([contract](architecture.md#linked-intrinsics-linked_intrinsics)): 47 pure operations (Math, `**`/`%`, Number text and parsing, primitive-cell conversions, UTF-16 access and case mapping; since wave 2 every BigInt operation) are guest code from the shared `varta-intrinsics` crate, linked function by function into the module's intrinsic region of memory 0 (one memory); a v38 host implements 54 imports. Measured below ([linked intrinsics measurement](#linked-intrinsics-measurement-v38)). Pending: the value18 conversions and comparisons (shared code, fuel-bound on hot paths), JSON and the value18 readers, which need the reference, aggregate and closure-envelope readers in the `no_std` core |
| `view_protocol()` | View protocol: L0 `view_commit`, event delivery, headless renderer | Implemented (P7): `view_commit` (host ABI 34), synthesized event export, `varta-view` headless renderer (tree, cascade, accessibility; no layout), `ui.open_url`/`clipboard.write` decided by permission broker v1 with `command_result`; every view record is a host task of an application instance and runs as its own turn |
| `tsx()` | TSX: TSX parsing and template compilation | Implemented (start of P8): `.tsx` parsing, AOT templates with getter holes, typed `JSX` namespace; L1 stays a package outside the engine |
| `structural_subtyping()` | Structural subtyping: width and structural subtyping of records, interfaces and classes, remaining pilot language items | Implemented in part ([contract](architecture.md#structural-subtyping-structural_subtyping)): width subtyping of records, interfaces and class instances through views (one accessor call plus one descriptor test per source layout; direct slots for shapes without wider sources), identity-preserving comparisons and `instanceof`, diagnostics naming both types and the missing/mismatched/extra members, erased generic method signatures with checked results, `object` annotations, `Array.isArray` narrowing of `unknown`, record and optional spreads, `obj.m?.()` on class methods, property narrowing kept across calls and getters with checked reads, `JSON.stringify` typed `string` and of `never`, literal `??` fallbacks, record-to-dictionary assertions and records held by `unknown` read as JSON objects, `Promise<unknown>` arrow contexts, `as void`, string `for...of`, exhaustive literal `switch`, constructor parameter properties, `encodeURIComponent`/`encodeURI`. Pending: Web byte bodies (`ReadableStream<Uint8Array>`, `arrayBuffer()`, `Blob`, `FormData`), typed `globalThis`/`process.env`, `Promise.race` over differently typed promises, `A[] \| B[]` of records, `instanceof Map/Set/Uint8Array`, unions mixing collections with other objects, JavaScript key order for spreads through views. Pilot probe below |
| `codegen_v2()` | Code generation v2: cheaper loops, string comparison and concatenation, dictionary call sites; fuel profiler | Implemented for codegen ([contract](architecture.md#code-generation-v2-codegen_v2)): quiet expressions without root traffic, call and frame fast paths, operand-cache collapse, plain updates, content-equality and inline literal comparisons, amortized string builders, guest `charAt`/`charCodeAt`/`%`, bounded first allocation scans and a collection threshold; `varta dev --profile-fuel` ([profile builds](architecture.md#fuel-profile-builds-varta-dev---profile-fuel)). Admits no syntax; v37 bytes frozen |

Structural-subtyping pilot probe (per-file `varta check` of every pilot module
under v38, scratch copies only, no live API call): the unmodified pilot checks
5 of 65 files, all others still stop at `ReadableStream<Uint8Array>` in
`core/sse.ts` (byte bodies, pending). With the byte-body, `Blob`/`FormData`,
`globalThis` and `Promise.race` uses replaced by text equivalents in a scratch
copy, every structural-subtyping and pilot-language blocker of the v37 walk is
gone (`object`, spreads of optional parameters, `Array.isArray`, optional
method calls, property narrowing in `sse.ts`, `JSON.stringify` typing,
`encodeURIComponent`, literal fallbacks, `pick`'s record-to-dictionary
assertion, `Promise<unknown>` page fetchers): 6 files check, and the rest stop
at limits owned by the program-limits stream (25 at the 128-shape limit, 12 at
the 256-function limit, 2 shape limits inside the fetch prelude) and 6 at
`Blob`. With those two limits raised in a scratch build only, the remaining
first blockers are the resume-body compilation budget (35), the union type
count (12), the promise type count (3), the function body budget (2), `Blob`
(7) and one checked conversion to a union of record shapes without a literal
discriminant.

**Integration (`v38-merge`).** The eight streams are merged into one v38
with these reconciliations: one limits registry (`LanguageProfile::limits()`
with the data limits as its `data` field) read through one reader parameter
(`value::ModuleBounds`); test budgets only through `budget::Budgets`; the v38
instantiation allowance as part of turn 1; one allocator with both the
data-limits growth and the codegen collection threshold (a full free-list scan
before a v38 collection); the fuel profiler on the one run path across all
turns, next to `--budget-report`; linked intrinsics within v38's Wasm limits,
with one v38 path per operation (codegen fast path, linked fallback, no host
import); view records as host tasks of application instances, one turn each,
and the view capabilities decided by permission broker v1. Integrated pilot
probe (per-file `varta check` under v38 of a scratch copy of the pinned
0.12.0 sources, no live API call): the unmodified pilot checks 5 of 65 files
(`core/errors.ts`, `core/json.ts`, `core/pagination.ts`, `generated/meta.ts`,
`generated/models.ts`); the first blockers are `ReadableStream<Uint8Array>`
in `core/sse.ts` (59) and `globalThis` in `core/util.ts` (1). With the
structural-subtyping stream's text equivalents for byte bodies, Web types and
`Promise.race` applied to the scratch copy, 51 of 65 files check (6 before
integration); the rest stop at `Blob` (10) and at checked conversions to a
union of record shapes without a literal discriminant (4).

**Wave 2 (open, owned after integration).**

| Stream | Scope | State |
| --- | --- | --- |
| (a) Intrinsics in memory 0 and a `no_std` value layout | Move the linked library's private memory into a reserved region of memory 0 (owner decision: no multi-memory); make `varta-common`'s value layout a `no_std` core (`std` feature on by default) that `varta-intrinsics` depends on with `default-features = false`, removing the restated cell readers of `varta_intrinsics::layout`; link the remaining 28 layout-reading pure operations. Also lifts the frozen 1 MiB string and result-buffer limits of the linked string intrinsics under v38 | Implemented in part (branch `v38-intrinsics-region`): one memory with the relocated intrinsic region (`varta.intrinsics`: stack, packed data, reserved arena), the `no_std` core (`abi`, `memory`, `value` cells and error classes, `bigint` transport, `limits`, `text`) as the one owner of the cell layout, `varta-bigint` without `std`, and the 13 BigInt operations linked; the value conversions and comparisons run the same shared code as host imports. Linked code reads strings within the v38 data limit; results above the 1 MiB result buffer still trap. Open: a cheap number/string conversion path, JSON and the value18 readers ([next increment](#linked-intrinsics-measurement-v38)), owned string results |
| (b) Dictionary and mixed-record hashing | Hashed dictionaries and mixed records; needs value layout 18 (a third storage slot) | Implemented on `v38-dictionary-hashing` (selector `dictionary_hashing()`, [contract](architecture.md#dictionary-hashing-dictionary_hashing)): value ABI/layout 18 for v38 with a four-slot dictionary storage record (`holes`, `keys`, `table`, `values`), an open-addressing `number[]` table probed from a seedless FNV-1a hash of the UTF-16 key units by allocation-free guest helpers, tombstones and holes for deletions, compaction before enumeration and amortized rebuilds at half load; twelve value18 readers (host ABI 34), JSON schema 18 and exception channel v9; every table validated against its entries at every reader. Measured below ([dictionary hashing measurement](#dictionary-hashing-measurement-v38)). Pending: array-index keys inserted below an existing key stay linear (an ordered rotation), and allocation-heavy loops at 20k+ entries are bound by the first-fit allocator (stream d) |
| (c) Web bytes | `ReadableStream<Uint8Array>`, `ArrayBuffer`, `Blob`, `FormData`, `response.blob()`, typed `globalThis` and `process.env`, and the structural-subtyping stream's pending items (`Promise.race` over differently typed promises, `A[] \| B[]` of records, `instanceof Map/Set/Uint8Array`, unions mixing collections with other objects, JavaScript key order for spreads through views, checked conversions to unions of record shapes without a literal discriminant) | Open; first pilot blockers |
| (d) Codegen hotspots | From the profiler: keyed-list diff, per-push argument arrays, `number[]` index boxing, closure re-validation at call sites | Implemented on `v38-codegen-hotspots` ([contract](architecture.md#code-generation-v2-codegen_v2), [measurements](#code-generation-v2-measurements)): appended `push`, checked element reads without union cells, word-stored literal and grown backings, concatenation chains, quiet function-value calls, exact-size free lists. `charAt` of non-ASCII units in constant time; `--profile-fuel` of programs with checked JSON fixed. Pending: the 100-row keyed diff under 1M fuel needs (b); closure entry checks stay as decided; incremental collection deferred (open) |

**Wave 3 (recorded, owner decisions of the wave-2 linked-intrinsics review).**

| Item | Scope | State |
| --- | --- | --- |
| (a) Inline conversions and comparisons | `primitive_to_string_v5`, `primitive_to_number_v5`, `primitive_compare` and `primitive_equal` get an inline codegen fast path for integers, ASCII strings and same-tag comparisons, with the linked general path (`varta_intrinsics::bigint`, already shared with the host imports) behind it, removing those host imports without a fuel regression. Measured when they were linked without it: the codegen v2 dictionary workload (50 inserts + 50 reads) rose from 456 K to 1.1 M fuel, and the v37 case `generator_cases/lifetime.ts` exhausted the production fuel budget under v38 | Open |
| (b) Linked JSON | `json_parse_v18` and `json_stringify_owned` link with a guest-allocated heap block, rooted for the duration of the call, as their scratch arena, under the normal collector, budget and root rules (no separately reserved gap); the decoder validates the whole input before it calls guest helpers and roots children until their parent is published | Open |
| (c) Owned string results | Results larger than the 1 MiB result buffer (case mapping, `String(x)` of a longer string) are returned as owned guest strings | Open |

#### Code generation v2 measurements

Fuel of the measured web workloads (`codegen_v2_fuel_ceilings_hold`,
`--nocapture`): totals at the checked size under test-only budgets of 40
production fuel budgets and unbounded host calls, the v37/v38 ratio, the
marginal fuel per iteration between half and full size, and the largest size
one production budget (1,000,000 fuel, 1024 host calls) runs, found by
bisection (`codegen_v2_largest_sizes`, ignored by default). "v38 before" is the
integrated v38 tip (cb6795d) before the wave-2 hotspot stream; "v38" is after
it. The keyed view is a generic fine-grained reactive keyed list modelled on a
UI library's signal/effect/mapKeyed layer
(`codegen_v2_fixtures/keyed_view.ts`): mount mounts the rows, update mounts
them, reverses their order and changes every row's text.

| Workload | Size | v37 fuel | v38 before | v38 fuel | Ratio v37/v38 | v37 / iteration | v38 / iteration | Max size v37 → before → v38 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `x = x + i` loop | 10,000 | 950,477 | 140,552 | 140,669 | 6.76× | 95.0 | 14.0 | 10,521 → 71,391 → 71,382 |
| 3-char string `===` | 4,000 | 1,129,277 | 304,913 | 305,002 | 3.70× | 278.0 | 76.0 | 3,535 → 13,146 → 13,145 |
| string `===` literal | 4,000 | 1,517,333 | 132,875 | 132,964 | 11.41× | 375.0 | 33.0 | 2,620 → 30,277 → 30,274 |
| `s = s + "x"` | 800 | 999,604 | 50,471 | 50,392 | 19.84× | 1,638.7 | 61.4 | 800 → 16,501 → 16,504 |
| dictionary 50 inserts + 50 reads | 50 | 628,459 | 446,490 | 374,901 | 1.68× | 15,712.7 | 10,071.3 | 67 → 81 → 87 |
| record field reads | 1,000 | 180,012 | 22,698 | 22,787 | 7.90× | 163.0 | 22.0 | 6,030 → 45,424 → 45,420 |
| direct function calls | 1,000 | 129,469 | 27,544 | 27,661 | 4.68× | 129.0 | 27.0 | 7,748 → 37,017 → 37,013 |
| closure calls | 1,000 | 426,855 | 330,982 | 305,015 | 1.40× | 426.0 | 304.0 | 2,345 → 3,027 → 3,286 |
| array `push` + index sum | 500 | 649,595 | 464,755 | 112,049 | 5.80× | 1,413.9 | 220.3 | 738 → 1,045 → 4,096 |
| Markdown-like scan (lines) | 10 | 335,140 | 68,458 | 66,002 | 5.08× | 31,852.6 | 6,410.0 | 27 → 145 → 150 |
| `JSON.parse` of ~2 KB (records) | 40 | 327,996 | 98,913 | 64,371 | 5.10× | 10,034.5 | 1,549.6 | 80 → 415 → 510 |
| list render (rows) | 100 | 2,928,606 | 292,803 | 160,137 | 18.29× | 50,624.7 | 1,582.6 | 74 → 341 → 511 |
| keyed-list diff (rows) | 100 | 4,969,154 | 1,785,586 | 1,445,572 | 3.44× | 73,483.8 | 19,708.1 | 45 → 68 → 80 |
| keyed-list diff (rows) | 500 | > 40,000,000 | 30,048,681 | 27,131,549 | — | — | 80,568.3 | as above |
| Markdown blocks (chat replies) | 1 | 218,887 | 104,367 | 59,276 | 3.69× | 195,850.0 | 54,727.0 | 3 → — → 18 |
| keyed view mount (rows) | 10 | 179,358 | 146,005 | 82,809 | 2.17× | 15,170.0 | 7,760.4 | 19 → 74 → 129 |
| keyed view update (rows) | 10 | 891,462 | 278,593 | 159,116 | 5.60× | 141,673.8 | 15,774.6 | 11 → 35 → 51 |

The hotspot stream removed the costs the profiler attributed at the
integrated tip: `push` with a static argument list appends through a
per-arity helper without an argument array (array push + sum 4.1× cheaper);
checked element reads load the slot without a union cell; small literal and
grown backings zero their 128-byte presence bitmap word by word, since
Wasmtime charges `memory.fill` and `memory.copy` one fuel per byte, and
literals reserve four slots; string `+` chains concatenate up to eight
operands per allocation; calls of function values with quiet operands take no
roots; and small blocks come from exact-size free lists (about 42 instead of
70 fuel per allocation in the list workloads, constant time after
collections). List render now stops at the 1024 host calls of a turn
(`String(i)` twice per row), not at fuel. `charAt` and `s[i]` of a non-ASCII
unit copy the unit in the guest: the linked `string_char_at16` they called
before copied the whole string per call (about 670 fuel on a 4-unit string,
23,600 on a 1002-unit one, more than v37's host call), so a Markdown-like
block parser (`codegen_v2_fixtures/markdown.ts`: heading, list, fence, table
and paragraph with a few non-ASCII characters) spent 30,376 fuel in 215
`CharAt` calls (141 each) and now 9,876 (46 each, about 100 for a non-ASCII
unit). The per-call copy-in of whole strings is a cost of the linked
library's private memory, for wave-2 stream (a). Fuel profiles of programs
with checked JSON, tools or HTTP work again: the host-free ABI helpers stay
uninstrumented and their fuel counts toward the caller.

What remains: the 100-row keyed diff spends 1,052,379 of its 1,445,572 fuel
(73%) in the dictionary's linear key search (`ArraySearch` and `StringEquals`,
10,200 comparisons), which wave-2 stream (b) replaces with hashing; the rest
of the workload is about 0.39M fuel, so the 1M target needs that stream. A
closure call still validates the function cell at the call site and its
environment and each captured binding cell at entry (about 70 fuel per object,
the v12 entry-check contract and its malformed-environment tests). Decision
(2026-10-30): the full v12 entry checks stay, so a binding cell corrupted after
creation is still detected; closure calls are not made cheaper by trusting
guest-built environments.

**Open: incremental or generational collection (deferred, 2026-10-30).** A
v38 collection marks the live heap once through the worklist and then sweeps
the whole heap: about 300k fuel per collection at 6000 live and 9000 dead
blocks (a 3000-node linked list built while dropping small arrays; two
collections took 619k fuel), so one turn with a large live heap can still
exhaust its fuel in a single collection. Bounding that per turn needs an
incremental (or generational, remembered-set) collector, which needs a write
barrier on every reference store (record and class fields, array elements,
binding cells, environments, frames, promise and timer state) with exact
rooting kept provable. Not scheduled.

#### Dictionary hashing measurement (v38)

Fuel per operation on `n` distinct keys (`"k" + i`, built into an array
before the measured loop) under v38 with test-only fuel and the production
8 MiB memory: before is the integrated v38 at cb6795d (linear key search,
shifting insertion and deletion), after is value layout 18
(`varta-runtime` `dictionary_fuel_measurements`, `--ignored --nocapture`, and
the baseline copy of the same workload run at cb6795d). Deterministic fuel,
not wall-clock.

| Entries | Insert before | Insert after | Lookup before | Lookup after | Delete before | Delete after |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 54,385 | 1,874 | 54,114 | 1,250 | 201,200 | 1,161 |
| 10,000 | 628,116 | 2,499 | 700,275 | 1,949 | 2,095,900 | 1,385 |
| 20,000 | 1,187,358 | 17,714 | 950,645 | 113,018 | 4,190,900 | 112,421 |
| 30,000 | 2,126,057 | 2,600 | 1,953,995 | 260,779 | 6,417,544 | 1,404 |
| 50,000 | 3,298,734 | memory budget | 2,859,370 | — | 10,535,787 | — |
| 100,000 | memory budget | memory budget | — | — | — | — |

- **Constant cost.** Up to 10,000 entries an operation costs 1,200–2,500 fuel
  (the per-type helper call with its roots, one probe through an
  allocation-free helper, two vector appends, amortized rebuilds), 30–1,500
  times less than the linear search; a lookup also allocates its `T |
  undefined` cell. The 251M-fuel shape the data-limits stream recorded
  (1,000 insertions with 500 keys deleted again) takes 2.7M.
- **Allocator bound beyond.** At 20,000 and 30,000 entries the fuel profile attributes
  most of the hashed run to the first-fit allocator (about 4,100 fuel per
  allocation over a free list fragmented by the key strings' garbage); the
  dictionary helpers themselves cost about 1,800 fuel per insertion there.
  Rebuilding tables of growing power-of-two sizes adds large garbage blocks
  the linear layout does not have. The allocator belongs to wave-2 stream (d).
- **Memory.** The table adds two to four `number[]` slots (16–32 bytes) per
  entry, so 50,000 hashed entries with their keys exhaust the 8 MiB memory
  budget (typed `Memory`), where the linear layout still fits; the 100,000-key
  array alone exhausts it in both. Production budgets are unchanged.
- **Ordered insertion.** An array-index key below an existing key (for example
  ids inserted in descending order) is rotated into place with one
  `memory.copy` per vector and one pass over the table: linear, as before, but
  2,000 descending ids with 1,000 deletions and a 6,000-entry string-key
  workload together run in 185M fuel (`dictionary_hashing_cases/large.ts`).

#### Linked intrinsics measurement (v38)

Measured on the author's macOS arm64 host with production budgets
(`cargo test -p varta-runtime --lib linked_intrinsic_measurements -- --ignored
--nocapture`; debug test build). Growth is the v38 module minus the same
program compiled for v37; fuel is one whole run, including instantiation (v38
instantiation has its own allowance, so the fixed data-segment charge of v37
disappears). "Before" is the two-memory library at the v38 integration
(`cb6795d`), "after" the memory-0 region (measured 16 KiB stack, packed data,
16 KiB reserved arena with free lists, arguments read in place) with the
BigInt operations linked.

| Program uses | v37 bytes | v38 bytes before → after | Fuel v37 | Fuel v38 before → after | Region pages after |
| --- | --- | --- | --- | --- | --- |
| none | 1917 | 3186 → 3186 | 34 | 19 → 19 | 0 |
| Math.sin | 1951 | 7917 → 7883 | 39 | 106 → 120 | 1 |
| every Math | 3080 | 26344 → 26217 | 377 | 7303 → 7513 | 1 |
| toFixed | 3016 | 96066 → 90081 | 16798 | 4272 → 4623 | 1 |
| toString(radix) | 2911 | 78152 → 72336 | 16775 | 90768 → 101257 | 1 |
| Number text (4) | 6531 | 105777 → 98530 | 18774 | 57009 → 65512 | 1 |
| parseInt/parseFloat | 2433 | 53389 → 54724 | 17014 | 4922 → 6291 | 1 |
| isNaN/isFinite(union) | 2481 | 26576 → 28684 | 17104 | 2683 → 3671 | 1 |
| charAt/charCodeAt | 2519 | 5634 → 6568 | 17197 | 858 → 858 | 1 |
| BigInt `*`, `String(bigint)` | 4475 | host imports → 83009 | 17652 | host → 18759 | 1 |
| `"n=" + n` | 2365 | 3625 → 3625 (host import) | 17015 | 737 → 737 | 0 |
| toUpperCase/toLowerCase | 3079 | 23635 → 23901 | 17620 | 7027 → 8697 | 1 |

- **Region.** One page for every measured closure; with the first memory-0
  layout (64 KiB stack, 32 KiB arena) it was 2 pages for Math and 3 for the
  Number-text and BigInt closures. The stack and reserved arena are the
  measured maxima with a margin of at least 2×
  (`linked_footprint_tests`): stack 5,016 bytes at most (BigInt widths and
  parsing of 2,000,000-bit values), bounded scratch 6,984 bytes at most
  (`toString(radix)` of `Number.MAX_VALUE`); the test fails when a rebuilt
  library needs more than half of either constant. Before the arena reused
  freed blocks, radix text of extreme Numbers took 83 KiB and decimal text
  3.4 KiB of a bump arena; size-class free lists bring them to 7 KiB and
  2 KiB.
- **Arguments in place.** Per call on 1,024 non-ASCII units, before → after
  reading in place: `charAt` 24,533 → 738, `parseInt` of a non-numeric
  string 6,572 → 2,327, `parseFloat` 5,621 → 1,372 (the same on 2,048
  units), `toUpperCase` 870,326 → 846,813, `toLowerCase` 634,909 → 611,177
  (case mapping itself costs about 820 and 595 fuel per unit through the
  core Unicode tables). `charCodeAt` is codegen v2's inline path (52).
  `linked_string_operations_read_their_input_in_place` pins these.
- **Exhausted arena.** Scratch that outgrows the reserved arena near the
  memory budget is the typed `RuntimeError::Resource` (memory, with the
  request), as `an_exhausted_linked_arena_is_a_typed_memory_failure` drives
  page by page. Linked BigInt multiplication and decimal text of operands
  near a million bits exhaust the 8 MiB budget (the host import, with the
  native allocator, does not): the arena's free lists do not coalesce.
- **Size.** Linked BigInt arithmetic pulls `num-bigint` (about 80 KiB once);
  the whole library is `crates/varta-compiler/src/link/intrinsics.json`
  `bytes` (about 290 KiB) and a program links only what it reaches.
- **Fuel.** A linked call costs its instructions instead of one of
  `abi::MAX_HOST_CALLS`. Linking `primitive_to_string_v5`,
  `primitive_to_number_v5`, `primitive_compare` and `primitive_equal` was
  measured and reverted (see wave 3 (a)).
- **Cold start.** Node 24 runs a linked single-memory module (Math, Number
  text, parsing, case mapping and BigInt) with only `log_string16` and
  `log_f64_v2` implemented, first run about 4 ms (V8 lazy compilation). The
  browser and Raspberry Pi 5 were not measured: no browser automation and no
  device were used.

**Next increment: the value-layout readers.** The remaining pure imports read
value layouts beyond primitive and BigInt cells (wave 3 (a)–(c) cover the hot
conversions, JSON and owned results):

1. The readers of `varta_common::value` (`ReferenceBox`, `Vector`,
   `Dictionary`, `ErrorObject`, `validate_aggregate`) and the closure-envelope
   reader of `varta_common::function` in the `no_std` core, generic over
   `memory::Memory` like `PrimitiveBox`. They name `ModuleBounds`, the limits
   tables and `function` (which names `language`, `serde` and the catalog),
   so the profile-independent parts move into the core or the readers take
   their bounds as plain values. Value layout 18 landed in wave 2 (b): the
   readers must validate its dictionary table exactly as
   `value::Dictionary::read_in` does. Then `value_typeof_v11`,
   `value_truthy_v11`, `value_nullish_v11`, `value_is_array_v11`,
   `value_strict_equal_v11`, `value_nullish_equal_v11` and
   `value_is_error_class_v7` link as plain readers.
2. `value_log_v12` as linked rendering followed by the `log_string16` effect,
   so logging stays the only host output.
3. A coalescing arena (or arena blocks from the guest allocator) so linked
   BigInt work near the bit capacity fits the memory budget as the host
   import does.

### Platform targets and application model

**Status: design accepted by the owner on 2026-10-29
([accepted decisions](#accepted-decisions-2026-10-29)). Implemented so far in
profile v38: the runtime half of P5 (event turns with per-turn budgets,
long-lived application instances, host event sources, permission broker v1 with
`query`/`request`, stored decisions and a prompt seam;
[contract](architecture.md#turn-budgets-and-application-instances-turn_budgets)),
P7 (the L0 view protocol and the headless renderer, its records as host tasks
of application instances and its capabilities decided by broker v1) and the
start of P8 (TSX templates;
[contract](architecture.md#v38-view-protocol-and-tsx-view_protocol-tsx)); the
rest of this section is not implemented yet.** The contract that exists today for third-party hosts is
[embedding.md](embedding.md). This section is the agreed design for the owner's
stated goal:

> A developer writes an application that uses uarp-sdk (HTTP through `fetch` with
> headers and body, `AbortSignal`, timers and backoff, SSE async generators,
> pagination) and builds the **same** application for desktop, for the browser and
> for Raspberry Pi 5, with full security and everything in Wasm.

A web-developer peer wrote a ~25-line JavaScript host for Tool invocation ABI v1
that matched native `varta invoke` in Node and a real browser, but had to read Rust
code to do it. Their requests, by priority:

| Priority | Request | Where this section answers it |
| --- | --- | --- |
| P0 | Published embedding contract | [embedding.md](embedding.md) (published with this design, documentation only) |
| P0 | Official browser target: same imports, manifest and deny-by-default, generated from `HostOperation`, same conformance, honest budgets | [Targets](#targets-and-artifacts), [browser security](#browser-security-model-and-honest-guarantees), [generated hosts](#generated-hosts-and-cross-host-conformance) |
| P0 | Lifecycle for interactive applications and host→guest events | [Lifecycle](#application-lifecycle-and-event-turns) |
| P0 | UI output, preferably a declarative view tree | [View layer](#view-layer-l0l3) |
| P0 | Modern profiles on every entry point with structured values | [Structured entry points](#structured-entry-points) |
| P1 | One async `fetch` capability (native and `window.fetch`), KV storage, binary/encoding/crypto | [Async fetch](#async-host-io-fetch-first), [later increments](#storage-binary-data-and-crypto) |
| P2 | Source-mapped diagnostics, `varta dev --target browser`, size/startup budgets | [Increments](#increments-and-acceptance) |

"No JS engine on the device" is met completely on desktop and Raspberry Pi 5 by
the [native renderer](#native-renderer-for-desktop-and-raspberry-pi-5):
application logic, UI framework and rendering are compiled Wasm plus memory-safe
Rust, with no JavaScript engine anywhere in the process. In the browser the
engine is the platform itself; there, the generated applier is the only script
besides the guest, and it never runs application code.

#### Feasibility probes (2026-10-29)

All probes ran on the author's macOS arm64 host at commit `8ccdb12`, with
production budgets, in a scratch directory outside the repository. No toolchain or
image was installed or downloaded, and no build-system change was committed.

| Probe | Method | Result |
| --- | --- | --- |
| Raspberry Pi 5 launcher | Installed Rust targets: `aarch64-apple-darwin`, `aarch64-unknown-linux-musl`, `wasm32-wasip2`, `x86_64-unknown-linux-musl`; `cargo-zigbuild` and `zig` present; no `aarch64-unknown-linux-gnu` target and no GNU cross linker. `cargo zigbuild --offline --locked -p varta-launcher --release --target aarch64-unknown-linux-musl` | **Builds** in 1 min 28 s: a statically linked, stripped aarch64 ELF of 11.0 MB. Run in an already present arm64 `debian:12-slim` container (the Raspberry Pi OS Bookworm base) with no network: `--runtime-info` reports `linux`/`aarch64` and the full profile list; a v36 async program and three tool invocations print exactly the macOS output; one run takes 18 ms wall time |
| Cross-target packaging | `varta build --single --launcher <aarch64 runtime>` on macOS | **Fails by design**: packaging executes the companion's `--runtime-info`/`--validate` probes, which cannot run a Linux binary on macOS ("cannot execute binary file") |
| Third-party tool host | A Node host (`varta-tool-host.mjs`, ~500 lines with full validation) written **only** from [embedding.md](embedding.md): manifest/ceiling parsing, pattern matching, module validation, catalog validation, argument parsing, string transfer, budgets | **21 of 21** invocations identical to native `varta invoke --json` (UTF-8/Cyrillic/emoji strings, i32 wrap-around, booleans, void with logs, `env.get` granted and denied by a deny-all host ceiling, eight malformed-argument cases, unknown tool). Writing it exposed one contract gap, fixed in the document: the JS API reports no import function types, so a JS host must parse the type/import sections |
| Modern profiles in a JS host | A 92-line Node probe that implements only the operations the contract specifies (logging, `string_char_code_at16`, the five scheduler imports, the exception channel and the microtask checkpoint) | A v36 module with a class, `async`/`await`, `for…of`, `try`/`catch` and binary64 logging (20 KB, 8 imports) **instantiates and runs to completion with output identical to the native runtime**. A v36 module using `JSON.stringify`, checked JSON parsing and `Headers` (60 KB, 12 imports) instantiates but stops at its first `json_stringify_v15` call: the layout-bound intrinsics are unspecified |

What a browser host needs beyond the probe, for the v35/v36 surface:

- **Already specifiable:** logging, UTF-16 character access, clock, environment as an
  embedder-supplied map, the scheduler and checkpoint, the exception channel, and
  Tool ABI v1. `log_f64_v2` is exactly JavaScript's `String(number)`.
- **Layout-bound pure intrinsics** (`json_*`, `value_*`, `union_*`,
  `primitive_*`, `bigint_*`, `number_to_*`, `number_parse_*` and the exception
  renderers): about 12k lines of runtime code plus `varta-common` readers, bound
  to value layouts that change per profile. Reimplementing them in JavaScript would
  create a second, drifting implementation. The accepted
  [thin host](#thin-host-guest-intrinsics) moves them into the guest instead.
- **Budgets:** memory is enforceable through the module's declared maximum; fuel
  only through the accepted metering transform; the deadline only through a Worker (see
  [browser security](#browser-security-model-and-honest-guarantees)).
- **Not available:** `fs.*` (no file system), `net.listen`, `process.exit` beyond
  ending the execution, and HTTPS `fetch` (the native runtime is HTTP-only today).

#### Targets and artifacts

| Target | Host | Artifact from `varta build` | State today |
| --- | --- | --- | --- |
| macOS arm64/x86-64, Linux x86-64 | Wasmtime launcher | `--single`: native launcher + portable Wasm + manifest (VARTA3) | Implemented for the build host's own OS/CPU |
| Windows x86-64/arm64 | Wasmtime launcher | Same container | Unverified |
| Linux aarch64 (Raspberry Pi 5) | Static musl Wasmtime launcher | Same container with a prebuilt aarch64 launcher | Launcher cross-builds and runs (probe); packaging needs offline launcher metadata |
| Browser, Worker (recommended) | Generated JS host in a dedicated Worker; generated applier on the main thread | `dist/web/`: `app.wasm`, `app.manifest.json`, `varta-host.js`, `varta-applier.js`, `index.html` loader with CSP and SRI | Not implemented; T1/R2 subsets proven by probes |
| Browser, main thread | Generated JS host | Same bundle with `mode: "main"` | Development only: no deadline enforcement |
| Desktop/Pi5 with UI | `varta-desktop` shell: Wasmtime + [native renderer](#native-renderer-for-desktop-and-raspberry-pi-5) in one process (no webview) | Prebuilt shell + payload in an OS bundle ([desktop packaging](#desktop-packaging)) | Not implemented |

- **Native cross-targets.** `varta build --target server-<os>-<arch>` (headless) or
  `desktop-<os>-<arch>` ([desktop packaging](#desktop-packaging)) selects a
  prebuilt companion or shell for that target. Because a foreign binary cannot be executed,
  each companion ships a `runtime-info.json` produced when it was built, and the
  builder validates the payload with its own runtime of the same package version.
  This is a *declared* compatibility claim, not a probe, and is labelled
  unverified in build output until package authenticity (C3) can sign companions
  (accepted). musl static builds are used for Linux (no glibc version coupling on Raspberry Pi OS); a GNU
  build needs `aarch64-unknown-linux-gnu` and a cross linker, neither of which is
  installed. Real Pi 5 hardware is unverified: its default kernel uses 16 KiB pages
  (the container used 4 KiB), so a device run is the first acceptance check.
- **Browser.** One artifact, not a separate compilation: the browser bundle reuses
  the same validated portable Wasm and manifest. The generated host and applier are
  versioned with the host ABI and value ABI; a mismatch fails at load.

#### Application lifecycle and event turns

Today every entry point is one bounded execution: `main` with its microtask
checkpoint, one tool call, or one HTTP request, each in a fresh store. Interactive
applications need state that survives between events. Design:

- **Application mode.** An entry module declares `varta.app({ ... })` (compile-time
  form like `varta.tool`) with an `init` function and typed event handlers. The host
  keeps **one long-lived instance** for the application's lifetime. It is the
  long-lived isolate already anticipated by
  [K4.4](#k44--suspension-scheduling-and-async-planned); the one-instantiation rule
  is unchanged.
- **Event turn.** One turn = deliver one host task (UI event, timer, I/O
  completion), run the microtask checkpoint, then flush the view patch buffer once.
  Turns never interleave; a checkpoint is never interrupted for fairness.
- **Budgets.** Per-turn fuel and per-turn deadline (renewed for each turn, never by
  `await`), per-instance memory and live-frame/scheduler bounds (not renewed), and
  lifetime limits on total turns or wall time when the embedder sets them. New
  named constants whose values are set after measurement (accepted); existing
  production budgets of existing entry points stay unchanged.
- **Failure.** A trap or exhausted budget terminates the whole instance (no
  reuse after a trap); the host shows a fatal state and **does not restart the
  instance automatically** (accepted); only an explicit user or embedder action
  starts a new instance from `init`.
  Ordinary `throw` and rejections remain language completions; an unhandled
  rejection after a turn follows the existing policy but ends only that instance.
- **Coexistence.** Tool and HTTP entry points stay stateless, fresh-store
  invocations. An application may also publish tools; they run in their own fresh
  instances and do not see application state (shared state remains C2 work).

#### Host→guest event delivery

Events reuse the scheduler: the host owns a bounded task queue next to the
microtask queue. A task is either a resume token (timer or I/O completion of a
suspended frame) or a UI event record. Ordering policy (to fix before
implementation, as K4.4 requires): FIFO across sources, timers ordered by due time
and then registration, I/O completions in completion order, at most one task per
turn. High-frequency sources (`input`, `scroll`, `pointermove`, `resize`) are
coalesced in the host: the queue keeps only the newest pending record per
listener and kind, so one burst costs one turn. The queue is bounded; overflow
drops coalescible records and fails the instance for others. Every completion
carries the operation id and generation, so a late completion of a cancelled
operation is discarded before it reaches the guest.

#### Async host I/O, `fetch` first

uarp-sdk needs `fetch(url, { method, headers, body, signal })`, `Response.status`,
`headers`, `text()`/`json()` and a streamed body for SSE. Accepted: **one
capability**, `net.request`, which replaces `net.get`/`net.fetch` in new profiles
(earlier profiles keep them), with URL patterns (the existing URL matching rules)
and optional method lists, mapped natively and to `window.fetch`:

| Aspect | Native (Wasmtime) | Browser |
| --- | --- | --- |
| Authorization | Exact `net.request` + URL (+ method) against package and host ceiling, before connecting | Same check in the generated host, before calling `fetch`; CORS can only narrow further |
| Transport | Async HTTP client on the existing Tokio runtime; HTTPS through `rustls`, justified and pinned exactly at adoption | `window.fetch` with `credentials: "omit"`, `redirect: "error"`, `referrerPolicy: "no-referrer"`, `cache: "no-store"`: no ambient cookies or redirects |
| Request | Method, header list, UTF-8 or byte body, all bounded | Same |
| Response | Status, header list, body as a stream of bounded chunks | `Response.body` reader, same chunk bounds |
| Cancellation | `AbortSignal` from the prelude; abort drops the transfer and resolves the pending operation with an `AbortError` rejection | `AbortController` passed to `fetch` |

ABI shape: `net_request_start(request_record) -> op id`, `net_body_read(op id) -> op
id` and `net_abort(op id)`, each completing later as a scheduler task that resumes
the awaiting frame with a result record. Guest-side `fetch`, `Response`, `Headers`
and a `ReadableStream` subset for SSE are engine preludes (like `Headers` today),
so the SDK's own code is unchanged. Bounds: request and response header counts and
bytes, chunk size, total body bytes, concurrent operations per instance and an
end-to-end deadline per operation, all named constants.

#### Thin host: guest intrinsics

The owner's goal ("everything in Wasm") and cross-host conformance both favour a
host that contains only **effects**: capability-checked I/O, the scheduler, view
output and the clock. Today about half of the import surface is pure computation
bound to value layouts. Options:

| Option | Semantics across hosts | Cost | Assessment |
| --- | --- | --- | --- |
| Reimplement every pure intrinsic in each host (JS, later others) | Two implementations of JSON, Number formatting/parsing and BigInt drift; JavaScript's own `JSON.parse` differs from checked parsing | Every host ×12k lines | Rejected |
| Compile the existing Rust intrinsics to `wasm32` and **link them into the guest module** at build time | Identical everywhere; fuel-metered as guest code; no host change | A linking step in the compiler, fuel and size increase (to measure) | **Accepted**, as a new profile with a new host ABI that drops those imports; never reimplemented per host |
| Rewrite intrinsics as TS preludes | Identical; typed | Very large; slower than Rust-compiled code | Later, case by case |

Earlier profiles keep their imports and meaning; only a new profile moves
intrinsics into the guest. Requirements: link only the intrinsics the application
actually uses, and measure the growth of `app.wasm` together with cold start in the
browser and on the Pi 5.

#### View layer (L0–L3)

Goals from the owner: a consistent model, convenient application building with
typed npm packages, the browser touched **exclusively** through Wasm (application
logic and the UI framework run in the guest; the host is a thin, generated,
capability-checked renderer that applies patches and delivers events by handler
id), and the same application on every target.

**Accepted baseline:** four layers. Only L0 and L2 are engine work; L1
and L3 are typed TS packages compiled into the application, so the engine stays
application- and framework-agnostic.

| Layer | Owner | Content |
| --- | --- | --- |
| **L0 — view protocol** | Engine (`HostOperation` registry, generated appliers) | One batched patch stream per turn and one typed event stream back |
| **L1 — reactive core** | Typed TS package (not engine code) | Signals, computed values, effects, batching, ownership/cleanup, keyed list reconciliation, context; fine-grained, no virtual DOM |
| **L2 — compiler support** | Engine | `tsx` parsing; AOT compilation of JSX into static templates with dynamic holes bound to L1, emitting L0 patches |
| **L3 — authoring APIs** | Packages over L1 | Native signals + TSX first; a specified hooks subset later; Svelte syntax last or never |

**L0 — a DOM-like protocol over a specified HTML/CSS subset.** L0 names elements,
attributes and CSS properties from a **specified, versioned subset** ("view subset
v1"), not all of HTML and CSS. The browser maps the subset one to one onto the real
DOM; the native renderer implements exactly the same subset; the headless renderer
serializes it. This keeps the web developer's mental model and avoids a lossy
abstract widget tree. Cross-renderer conformance compares layout rectangles within
stated tolerances, not pixels.

The v1 subset draft and a protocol draft that a browser applier can be written
against follow. Beyond v1: SVG and MathML namespaces, portals into other roots,
declarative animations (`@keyframes`, WAAPI keyframe records) with
`transitionend`/`animationend`, measurement queries (`measure`,
`observe_resize`, `observe_intersection`, answered one turn later so
`@floating-ui/core`-style positioning works deterministically), History API
routing (`nav.push`/`nav.replace`, `popstate`), file input through a user-gesture
`ui.pick_file` capability, clipboard capabilities, drag and drop, and server-side
rendering with hydration.

##### L0 v1 subset (draft)

Criterion: the native renderer implements as little as possible while the first
uarp-sdk application still has enough. **First application, confirmed by the
owner on 2026-10-29:** the [UARP console](#uarp-console-cross-target-acceptance-application)
with a settings screen (base URL, API key), side
navigation between resources, paginated lists with "load more", a detail card,
create/edit forms (enums as `select`), streamed SSE text, loading and error
states, a confirm dialog and toasts. The subset below was proposed by the
web-developer peer.

| Area | In v1 | Not in v1 |
| --- | --- | --- |
| Structure | `div`, `span`, `p`, `h1`–`h3`, `ul`/`ol`/`li`, `header`/`nav`/`main`/`section`/`aside`/`footer` (landmarks for AccessKit; `aside` added in [round 4](embedding.md#view-draft-v0-round-4-subset-fixes)) | Table elements (Taffy has no table layout: use grid with `role=table/row/cell/columnheader`) |
| Text | `strong`, `em`, `code`, `pre`, `br` | Raw HTML, `contenteditable` |
| Controls | `button`; `a` for internal routing, and external links opened by the renderer under the accepted `ui.open_url` capability; `form` only as a submit boundary (no `action`/`method`); `label` (`for` focuses the field); `input` types text, password, number, search, checkbox, radio; `textarea`; `select`/`option` with a native popup; `progress` | `input` types date, color, range, file; `details`/`summary` |
| Overlays | `dialog` with the `showModal` command, top layer, focus trap, Esc = cancel | — |
| Media | `img` from artifact resources only (PNG; SVG through resvg) | Remote images (v1.1, need the network capability: exfiltration channel); `iframe`, `video`/`audio`, `canvas`, `script`/`style` elements |
| Selectors, cascade | `.a`, `.a.b`, `:hover`, `:active`, `:focus-visible`, `:disabled`, `:checked`, inline style; specificity by class count, then source order; stylesheets sent through the protocol; custom properties and `var()`; `@media` with `prefers-color-scheme` and `min-width`/`max-width` | Combinators, `::before`/`::after`, `:nth-child` |
| Layout | `display` block, flex, grid, none, inline (text level); `box-sizing` always border-box; width/height/min-*/max-*, margin, padding, gap; flex direction, wrap, `justify-content`, `align-items`, `align-self`, grow/shrink/basis; grid templates in px, %, fr, auto, `minmax()`, `repeat()`, `grid-column`/`grid-row` with `span` | Named grid areas, `position: sticky` (v1.1), `calc()` (v1.1) |
| Positioning, scrolling | `position` static, relative, absolute, fixed; top/right/bottom/left; `z-index`; `overflow` visible, hidden, auto | — |
| Units | px, %, rem, fr | em, vh/vw, `calc()` |
| Paint | border (width, solid, colour), `border-radius`, `background-color`, `color`, `opacity`, `outline` (width, solid, colour, offset) | Background images and gradients; `box-shadow` (v1.1) |
| Text styling | Generic families plus artifact fonts; `font-size`; `font-weight` 400/500/600/700; italic; `line-height`; `text-align` left/center/right; underline; `white-space` normal/nowrap/pre-wrap; one-line `text-overflow: ellipsis`; `overflow-wrap: anywhere` | — |
| Cursor | default, pointer, text, not-allowed | — |
| Motion | Transitions on opacity, color, background-color and transform (translate/scale only) | `@keyframes`, rotate (v1.1) |

- **Authoring guidance.** Grid and flex containers holding form fields use
  shrinkable tracks (`grid-template-columns: minmax(0, 1fr)`, `min-width: 0` on
  flex items); otherwise a `textarea`'s intrinsic width overflows narrow
  viewports such as 375 px ([round 4](embedding.md#view-draft-v0-round-4-subset-fixes)).
- **Fonts.** The application ships its fonts in the artifact (by default one sans
  and one mono). The browser loads the same fonts through `@font-face` from
  artifact resources, so layout is comparable across renderers.
- **Events.** click, input, change, submit, keydown, focusin/focusout, scroll
  (coalesced to at most once per frame), cancel/close. Events bubble from the target
  to the root; every node on the path with a listener for that kind gets a record,
  until a `stopPropagation` listener ([draft v0](embedding.md#view-protocol-draft-v0-unstable));
  there is no capture phase.
  `preventDefault` and `stopPropagation` are listen-time options the host executes
  synchronously; `submit` always prevents default because there is never navigation.
  No pointermove, drag, wheel or touch gestures in v1; the renderer scrolls itself.
- **Input fields (most important).** The **host owns editing**: buffer, caret,
  selection, IME composition, undo/redo, in-field copy/paste, key navigation and
  password masking. Attributes: `value`, `placeholder`, `disabled`, `readonly`,
  `maxlength`, `autofocus`, `name`, `checked`, `selected`; `type=number` returns a
  string; no built-in validation (validation lives in the application, for example
  with valibot). Enter in a form submits, Esc in a dialog cancels, Tab follows
  document order, `tabindex` is 0 or -1 only. Guest commands: focus, blur, select,
  scrollIntoView, scrollTo(id, top), showModal, close. In v1 (accepted in round 3):
  `clipboard.write` as a user-gesture capability with `copy_text`, key filters on
  `keydown` listeners and auto-growing textareas
  ([draft v0](embedding.md#view-draft-v0-round-3-chat-requirements)). v1.1:
  selecting arbitrary text outside fields.
- **Accessibility (AccessKit).** Roles table, row, cell, columnheader, dialog,
  alert, status, navigation, list, listitem, tab, tablist, tabpanel; attributes
  `aria-label`, `aria-labelledby`, `aria-describedby`, `aria-live`,
  `aria-expanded`, `aria-selected`, `aria-current`, `aria-busy`, `aria-invalid`,
  `aria-hidden`. A visible focus outline is mandatory. The headless renderer outputs
  the accessibility tree.
- **Window (native targets).** Title, initial and minimum size, DPI handled by the
  renderer, the OS colour scheme, system copy/paste shortcuts.
- **Freeze criterion for L0 v1.** A set of conformance pages where the same patch
  stream with the same fonts produces headless (Taffy + Parley) layout rectangles
  matching the browser's `getBoundingClientRect` within ±1 px; an accessibility
  tree matching AccessKit's; and input tests for IME, fast typing under delivery
  latency (Worker messages, the native thread hop) and
  stale-patch rejection.

##### L0 protocol draft v0

A draft for the peer's browser applier and headless renderer; every budget is a
placeholder to be replaced by measured named limits.

- **Transport.** Patches: the guest encodes one **batch** in its own memory and
  calls `view_commit(ptr: i32, len: i32) → (i32)` once per turn, a new
  capability-free `HostOperation` with its own minimum host ABI. The host validates
  the **whole batch** before applying any operation (atomic): an invalid batch is a
  fatal error of the instance, never a partial render or a recoverable status.
  Events: the host writes one event record at `abi::RESULT_BASE` and calls the guest
  export `varta.view_abi.event(len: i32) → ()`, which copies it before any other
  host operation; that call is the task of one turn.
- **Encoding.** Little-endian. A batch starts with `version: u16`, `flags: u16`,
  `op_count: u32`. Each operation is `opcode: u8`, three reserved zero bytes, then
  its operands. Ids are `u32`; strings are `len: u32` (bytes) followed by UTF-16LE
  code units, padded to 4 bytes; tag, attribute, CSS property, event and command
  names are `u16` codes from a generated subset table, so anything outside the subset
  cannot even be encoded.
- **Ids.** Node ids are guest-chosen nonzero `u32`; id 1 is the root container, which
  the host creates. An id is live from its `create_*` until the `remove` of it or an
  ancestor; reusing it requires that `remove` earlier in the same or a previous batch.
  Handler ids are nonzero `u32` chosen by the guest. Any unknown, dead or duplicate id
  invalidates the batch. Removing a node releases its subtree and listeners.

The opcodes, operand wire order, element/attribute/CSS/event/command codes, bit
sets, event record offsets and placeholder budgets are data-only registry tables in
`varta_common::view` (unstable draft v0, no host operation registered), generated
into the [embedding contract](embedding.md#view-protocol-draft-v0-unstable) and
checked by a drift test.

- **Event record.** `version: u16`, `kind: u16` (click, input, change, submit,
  keydown, focusin, focusout, scroll, cancel, close), `handler: u32`, `target: u32`
  (the node the event originated on), `seq: u32`, `flags: u32` (shift, ctrl, alt,
  meta, isComposing, checked), `scroll_top`, `scroll_height`, `client_height`
  (f64, scroll only), then the `key` and `value` strings (empty when absent).
- **Seq semantics.** The host keeps one edit counter per field and increments it
  for every local user edit, including each IME composition step; it starts at 0,
  `input` records carry the new counter, `change` records the current one, and
  coalescing may leave gaps ([draft v0](embedding.md#view-protocol-draft-v0-unstable)). `set_value`/`set_checked` carry the counter the
  guest last saw for that field. The host applies the patch only when that base
  equals its current counter; otherwise newer local edits exist and the patch is
  dropped as stale. An equal value is a no-op that does not move the caret. During a
  composition, input records carry `isComposing`; the final value follows
  composition end.
- **Turns and frames.** One task (event, timer, I/O completion) is one turn: the
  guest handles it, runs the microtask checkpoint and commits at most one batch. The
  renderer applies batches in order and may coalesce several turns into one frame;
  scroll records are coalesced to one per frame and keep only the latest position.
- **Budgets (placeholders).** Batch bytes and operations, live nodes and listeners,
  one text or attribute value, total stylesheet text and pending event records, as
  the `view::DRAFT_*` constants; coalescible records are replaced, others are fatal
  on overflow.
- **Headless output.** The headless renderer returns the node tree (tags,
  attributes, text), the layout rectangle of every element and the accessibility
  tree as deterministic JSON, the basis of the freeze criterion and of conformance
  snapshots.

**The applier is the trust boundary.** It is generated (or, natively, it is the
renderer's patch decoder), contains no application logic and enforces:

- text nodes and attributes only, never raw HTML (`innerHTML`,
  `insertAdjacentHTML`, `outerHTML`, `srcdoc` are not reachable);
- a tag allowlist without `script`, `iframe`, `object`, `embed`, `base`, `meta`,
  `link`, `form action` targets outside the origin, and no `on*` attributes;
- URL-valued attributes (`href`, `src`, `srcset`, `poster`, `action`, `xlink:href`,
  CSS `url()`) restricted to bundled assets and to origins the manifest grants:
  **an image or media URL to a foreign host is a network effect and an
  exfiltration channel**, so it requires the same `net.request` grant as `fetch`,
  whose URL patterns act as the origin allowlist; there is no separate `ui.load`
  capability (accepted); `javascript:`, `data:` (except bundled images) and `blob:`
  from outside the applier are rejected.

The native renderer has no script engine and no URL loader of its own: every image
or font load goes through the capability-checked host, so the same URL rule holds.

**Renderers for the same protocol.**

| Renderer | Where the guest runs | Guarantees |
| --- | --- | --- |
| Browser DOM applier | Dedicated Worker (recommended) | See [browser security](#browser-security-model-and-honest-guarantees): fuel through the metering transform, deadline via `Worker.terminate` |
| **Native renderer** (desktop, Pi 5; primary) | Native Wasmtime in the same process | Full fuel, deadline, memory and capability enforcement; no JavaScript engine on the device; the strongest target |
| Headless renderer | Native or Node | Serialized tree, layout boxes and accessibility tree; used for conformance, snapshots and server-side rendering |
| System webview applier | — | Not built (accepted): it would bring a JavaScript engine back onto the device |

**Server-side rendering and hydration.** The headless renderer emits HTML with
stable node ids (`data-v` attributes) derived from the template structure; the
browser applier adopts existing nodes by id instead of creating them, and the guest
replays `init` to rebuild its handler table. Hydration mismatch fails closed to a
full client render.

**Performance and patch volume (estimates, to measure).**

| Scenario | Patches | Crossings | Guest work |
| --- | --- | --- | --- |
| Initial render of 50 resource rows (row, 4 cells, 4 texts, 2 attributes, 1 listener) | ~600 operations, ~7 KB at ~12 bytes each | 1 `view_commit` | Proportional to the tree once |
| Update of one field (fine-grained L1) | 1 `set_text` | 1 | Proportional to dependents of one signal |
| Same update through a virtual-DOM diff | 1 | 1 | Proportional to the component subtree (~500 vnodes compared); at the current measured cost of compiled code (for example, the `Headers` workload uses a fifth of the per-execution fuel), a whole-list diff per keystroke is a material share of a per-turn budget |
| Keystroke in a search box | 1 event → 1 turn | 2 (event in, commit out) | Debounced fetch through timers |

Startup: the probes' modules are 4–60 KB; a reactive core and template runtime
are estimated at a few tens of KB of Wasm; V8 compiles such modules lazily in well
under a millisecond. Measured artifact size and startup become CORE-16 rows.

**Typed npm reach — what "using npm UI libraries" can honestly mean.** Under the
[research thesis](#research-thesis) only packages that ship TS/TSX sources (and
compile within the admitted subset) can be used; packages that ship compiled JS
with `.d.ts` are rejected. Most npm UI component libraries (React, Preact, MUI,
Radix, shadcn) are compiled JS plus declarations, or need React DOM, so they are
out of reach except by porting through the hooks subset later. Preact and Svelte
are themselves JavaScript with JSDoc types; Solid's reactive core is TypeScript
(its JSX compiler is a Babel plugin in JavaScript). Realistic first targets are
**framework-agnostic, DOM-free typed cores**:

| Package | Why | Needs |
| --- | --- | --- |
| `clsx` | Trivial first test | — |
| `valibot` | Validation; simpler than `zod` | Generics, unions |
| `@tanstack/table-core`, `virtual-core`, `query-core` | Ideal tests of L1 adapters | Classes, generics, closures; `query-core` needs timers and promises |
| `@floating-ui/core` | Exercises the asynchronous measurement queries | `measure` queries |
| `nanoid` | Small | Crypto random |
| `@zag-js/*` | Accessible state machines | Check its DOM queries first |

Caution: `date-fns` and all date/number/plural formatting need `Date` and `Intl`
(a large gap, possibly a capability-free host provider); `immer`, MobX and Vue
reactivity rely on `Proxy` and are out of scope; CSS-only design systems (plain
CSS, Tailwind output) are usable because CSS is data shipped as a stylesheet asset.

**Front-end options compared.** Facts: SWC is configured with
`Syntax::Typescript(Default::default())` in `crates/varta-compiler/src/parse.rs`, so
`tsx` is off today; JSX typing in TypeScript uses `JSX.IntrinsicElements` and
`jsxImportSource`, which the existing checker can consume once `tsx` is enabled.

| Option | Thesis fit | Engine work | Runtime cost | Library reach | Verdict |
| --- | --- | --- | --- | --- | --- |
| **(A)** TSX + Preact-compatible API (function components, hooks, `h`/`jsx-runtime`), virtual-DOM diff in the guest | Typed TSX; framework is a TS package; but Preact itself is JS + JSDoc, so a typed port is needed | `tsx` parsing and JSX lowering to calls | Whole-subtree rerender and diff per state change; fuel-heavy per turn | Familiar API; React libraries still compiled JS | Alternative; later as a **specified hooks subset over L1**, never claimed "React compatible" (rerender, dependency arrays and stale closures do not match fine-grained signals) |
| **(B)** Solid-style fine-grained signals, TSX compiled to templates + reactive holes | Typed TSX; L1 is a TS package (Solid's core is TS but would need tightening to the typed subset) | `tsx` parsing, template compilation in the compiler (estimate 3–6k lines) | Work proportional to what changed; minimal patches | Same typed-source limit; ports of typed DOM-free cores | **Accepted primary path** |
| **(C)** Full Svelte component compiler/file format, Svelte ecosystem compatibility | Svelte compiler and output are JS; libraries often untyped | Reimplement Svelte | — | Svelte JS libraries | Rejected |
| **(D)** `.svelte` front-end restricted to `<script lang="ts">` with typed templates, compiled to the same IR and L1 | Good: AOT, typed script | High (see below) | Like B | `.svelte` libraries that are fully typed | Last (accepted); only after B is stable |

*Steel-manning Svelte (D).* Svelte's compile-time model (components compiled to
imperative updates, no virtual DOM, a tiny runtime) is philosophically the closest
to Varta's AOT thesis, and Svelte libraries usually ship `.svelte` **source** that
the consumer compiles, which suits "typed source only" better than React's compiled
packages. The obstacles, with estimates:

- a second front-end for the SFC format (template, `<script>`, `<style>`):
  4–8k lines of Rust;
- our own template type checking at `svelte-check` level, because `svelte-check`
  and `svelte2tsx` are JavaScript tools; without it untyped expressions enter
  templates: 3–5k lines, generating typed IR from templates so the existing checker
  runs;
- the Svelte compiler is JavaScript and emits JavaScript that drives the DOM
  directly, so it would be reimplemented in Rust, emitting Varta IR plus L0
  patches: 5–10k lines;
- Svelte 5 runes: deep `$state` relies on `Proxy`; mapping `$state`/`$derived`/
  `$effect` to L1 at compile time works for closed typed records and arrays by
  instrumenting field writes, not for dictionaries or `unknown`;
- many published components use untyped or JSDoc-typed `<script>` blocks, which
  remain rejected.

B can be read as Svelte's compile-time reactivity in TSX syntax: it reuses SWC's
`tsx` parser and the existing type checker. Because L2 lowers templates to one IR
("static template + holes bound to L1"), D can later be added as another front-end
over the same L1 runtime and L0 protocol. Mixing components across front-ends is
deferred: three front-ends × three renderers would multiply the conformance surface.

**Trying to break the baseline against the criteria.**

| Criterion | Holds? | Weak point and mitigation |
| --- | --- | --- |
| 1. Typed only | Yes | L1/L3 are typed TS packages; JSX typing through `JSX.IntrinsicElements`; no `.d.ts`-only dependency |
| 2. All app/UI logic in Wasm | Yes | The applier is generated, logic-free and capability-checked; `preventDefault` etc. are declarative options, not callbacks |
| 3. Security parity | Partly | Native targets are stronger than the browser (logic in Wasmtime, no JS engine); the browser lacks fuel and fine-grained deadlines; URL-loading attributes are network effects and need grants |
| 4. Same app everywhere | Yes, within the L0 subset | The native renderer implements the same subset the browser maps to the DOM; fidelity is proven by the freeze criterion, and anything outside the subset is rejected |
| 5. Familiarity | Medium | TSX and signals (Solid) are familiar; hooks subset later; Svelte syntax not soon |
| 6. npm reach | Narrow, honestly | Typed DOM-free cores first; component libraries need ports |
| 7. Performance | Good | One crossing per turn; fine-grained patches; VDOM option costs more fuel |
| 8. Cost and order | Moderate | L0, headless renderer and L1 are small; template compilation is the main compiler work |
| 9. Conformance | Yes | The headless renderer gives deterministic tree and accessibility snapshots on every host |
| 10. Budgets and event loop | Yes, with coalescing | Per-turn budgets; coalesced high-frequency events; queries answer in later turns |

Breaking points found: synchronous DOM reads (layout, selection) are impossible
by design and libraries that require them need the asynchronous queries;
controlled-input races need the seq rule above; hooks over signals need a
per-component rerender adapter, so the subset must be specified rather than implied.

**Decision (accepted 2026-10-29).** L0 is a DOM-like protocol over a specified,
versioned HTML/CSS subset with the headless renderer; L1 is a typed package
maintained outside the engine crates; B (TSX + compiled signals) is the first and
only path for now; then the browser applier and the native desktop/Pi 5 renderer.
A hooks subset comes later, only as a specified subset and never called "React
compatible"; `.svelte` comes last. No webview is built now.

#### Native renderer for desktop and Raspberry Pi 5

**Primary desktop and Pi 5 design, accepted 2026-10-29.** A system
webview brings a JavaScript engine back onto the device, even if only as a display.
A native renderer keeps the whole device free of a JavaScript engine, which fully
matches the [research thesis](#research-thesis). The renderer is memory-safe Rust
in our crates (no `unsafe`); its dependencies may contain `unsafe`, each with a
justification and an exact `=` pin. It implements exactly the
[L0 v1 subset](#l0-v1-subset-draft),
the same subset the browser maps one to one onto the DOM.

| Concern | Candidate | License as published (verify at adoption) | Notes |
| --- | --- | --- | --- |
| Windowing, input, IME events | `winit` | Apache-2.0 | IME preedit/commit on macOS, Windows, X11 and Wayland (`text-input-v3`; support in the Pi OS compositor is unverified) |
| Layout | `taffy` | MIT | Block, flexbox and grid; no table layout (hence grid + roles in v1) |
| Text shaping, bidi, line breaking, editing | `parley` (with `fontique` for font enumeration and fallback) | Apache-2.0 OR MIT | Includes a plain-text editor model with IME preedit; `cosmic-text` (MIT OR Apache-2.0) is the alternative |
| GPU rendering | `vello` on `wgpu` | Apache-2.0 OR MIT; `wgpu` MIT OR Apache-2.0 | Compute-shader 2D renderer; Pi 5 VideoCore VII exposes Vulkan 1.2 through Mesa `v3dv` (performance unverified) |
| CPU fallback | `vello_cpu`, or `tiny-skia` with `softbuffer` | Apache-2.0 OR MIT; `tiny-skia` BSD-3-Clause; `softbuffer` MIT OR Apache-2.0 | For weak or missing GPUs |
| Accessibility | `accesskit` (with its `winit` adapter) | MIT OR Apache-2.0 | Maps to the native accessibility APIs on all three desktop OSes |
| SVG images | `resvg` | Apache-2.0 OR MIT | Artifact SVG only |
| Clipboard | `arboard` | MIT OR Apache-2.0 | v1: in-field copy/paste only |
| Prior art | Dioxus Blitz (HTML/CSS renderer on Stylo, Taffy, Parley, Vello) | MIT OR Apache-2.0; Stylo is MPL-2.0 | A reference and possible source of parts; Stylo implements far more CSS than the subset needs, and the subset's selector language is small enough for our own cascade |

Accepted stack: winit, taffy, parley/fontique, vello with a CPU fallback,
AccessKit and resvg; each dependency is justified and pinned exactly (`=`) when it
is adopted. Feasibility note: none of these crates is in `Cargo.lock` or in the
local Cargo registry cache today, and none has been added; licenses above are as
published by the projects and must be re-checked for the pinned versions.

Process model: one native process. The renderer owns the main thread (required by
`winit` on macOS) and the window; Wasmtime runs the guest on a worker thread with
full fuel, deadline, memory and capability enforcement; batches and event records
cross a bounded in-process channel, so there is no IPC serialization beyond the
batch bytes themselves. Every effect (fetch, storage, timers, files, clipboard)
goes through the same capability-checked host as headless execution.

**Alternatives.**

| Option | For | Against | Verdict |
| --- | --- | --- | --- |
| **(a) Own subset renderer** (winit + taffy + parley + vello/CPU + AccessKit) | No JS engine; one implementation for every desktop OS and Pi 5; exactly the browser's subset; headless renderer shares layout code | We own text input, scrolling, accessibility mapping and CSS fidelity | **Accepted** |
| (b) Abstract widget tree mapped to native OS widgets (AppKit, Win32, GTK) | Native look and native text input | Three backends; lossy mapping from the browser model; no shared layout with the headless renderer | Rejected for v1 |
| (c) Existing toolkit behind the protocol: Iced (MIT), Slint (GPLv3, royalty-free or commercial licences: **licence risk**), Xilem/Masonry (Apache-2.0, experimental) | Mature widgets and text input (Iced, Slint) | Their layout and styling models are not the HTML/CSS subset, so the browser mapping becomes lossy again; licence constraints for Slint | Alternative; Masonry worth re-evaluating as it matures |
| (d) System webview (WKWebView, WebView2, WebKitGTK) with the generated applier | Full CSS fidelity, mature text input and accessibility | A JavaScript engine on the device; WebKitGTK on Pi 5 is heavy (typically 100 MB or more, slow start, GPU issues); large platform dependency graph (`wry`/`tao`) | Not built now (accepted) |

**Costs and risks.** Text input and IME (Cyrillic, CJK) across winit backends,
including Wayland on Pi OS; accessibility completeness through AccessKit; smooth
scrolling with nested overflow; fonts and i18n (fallback, bidi); CSS fidelity
against the browser within the freeze tolerance; binary size (estimated +10–20 MB
for the renderer stack, to measure); GPU availability and driver quality (vello's
compute path on `v3dv`); startup and memory on Pi 5 (target well below the 100 MB
or more of WebKitGTK, to measure); the effort of owning a renderer at all, which is
contained only by keeping the subset small.

**First renderer increment.** Box, text, flex, button, text input with IME and
overflow scrolling, measured on macOS and on a Pi 5: startup time, resident
memory, frame time for the 50-row list and keystroke-to-frame latency.

#### Desktop packaging

**Shell crate.** A new distribution-layer crate, `varta-desktop` (binary
`varta-shell`), next to the headless `varta-launcher`, under the same boundary
rules: production dependencies are common, artifact and runtime plus the renderer
stack, never compiler, SWC, project, CLI or clap. The headless launcher stays the
companion for CLI and server applications. The shell runs `app.wasm` in Wasmtime
and the [native renderer](#native-renderer-for-desktop-and-raspberry-pi-5) in one
native process, with the same capability-checked host for every effect.

**No webview fallback for now (accepted).** The shell has no system webview
path; a later reconsideration would need its own decision, dependency review and
CSP/IPC contract. A system browser on `localhost` is rejected: it is not an
application window and shares the browser's realm.

**Build model: compile once, choose a shell per target.** The portable Wasm and
manifest are compiled once and attached to a **prebuilt shell** for each target
with the existing VARTA3 layout (native prefix | Wasm | manifest | fixed versioned
EOF footer), so the developer needs no cross toolchain. CLI:

```sh
varta build --target desktop-macos-arm64     # also desktop-macos-x64
varta build --target desktop-windows-x64     # also desktop-windows-arm64
varta build --target desktop-linux-x64       # also desktop-linux-aarch64 (Pi 5)
varta build --target browser                 # dist/web bundle
varta build --target server-linux-aarch64    # headless launcher, like --single today
```

Prebuilt shells are versioned with the package version, host ABI, value ABI and
profile list; each ships the `runtime-info.json` it was built with, because a
foreign-OS shell cannot be executed for today's `--runtime-info` probe. Shells are
published with each Varta release (fetching them is a tooling step outside the
engine; accepted: distributed through releases, signing later). Until C3
provenance exists, this is a declared compatibility claim plus a checksum the
developer pins, labelled unverified in build output and documentation.

**OS bundles.**

| OS | Bundle | Varta can automate | Needs the developer's own identity |
| --- | --- | --- | --- |
| macOS | `.app` (Info.plist, icon, the shell with its attached payload), optional `.dmg` | Bundle layout, Info.plist from manifest metadata, ad-hoc signing for local runs | Developer ID signing and notarization (Apple account). The payload is inside the signed executable, so it is attached before signing |
| Windows | `.exe`, optionally MSIX | Executable, icon, MSIX manifest | Authenticode or MSIX signing certificate. No WebView2 dependency |
| Linux, Pi 5 | `.deb` or AppImage | `.deb` depending on the Vulkan/Mesa and Wayland/X11 client libraries; no WebKitGTK | Repository or AppImage signing keys, if wanted |

**Startup.** Native launchers compile the embedded portable Wasm at every start.
The 20 KB probe module runs end to end in 18 ms on aarch64, but a pilot-sized
module is expected to take much longer (to measure; the file limit is
`abi::MAX_WASM_BYTES`). Options:

| Option | Security argument | Assessment |
| --- | --- | --- |
| Compile at every start (today) | No native code is ever read from disk | Until the cache lands; the shell shows its window and a splash while compiling |
| Shell-generated local code cache, keyed by SHA-256 of the Wasm plus the Wasmtime version, engine configuration and host target, written only by the shell from validated portable Wasm into a user-private directory | A cache entry is native code. Whoever can write it runs code with the user's authority, which that user can already do by replacing the shell; it never comes from package input. A per-install MAC key (OS keychain) rejects swapped or stale entries | **Accepted** through Wasmtime's own cache feature (deserializing compiled modules is `unsafe` in Wasmtime's API and our workspace forbids `unsafe`, so never through our own code). The ban on native payloads in package input stays |
| Winch baseline compiler or the Pulley interpreter | No cached native code | **Accepted: Winch for fast development starts**; Pulley (no JIT, relevant to kernels without JIT in the research thesis) remains research; to measure |

**Window, metadata and permissions.** Application metadata joins the manifest as
a strictly validated optional `app` object: title, initial and minimum size, icon
asset, single-instance flag; menus and tray later. Capability grants: the package
manifest is the request; the shell shows it at first run (or the installer at
install time) as a plain-language list ("connect to https://api.example.com",
"store data on this computer", "read the clipboard") and stores the user's decision
as the host ceiling in the user's configuration directory. Denied requests stay
denied at runtime with the usual status. Changing grants is an explicit settings
action, never a prompt triggered by guest code.

**Updates.** Later, and only with C3 provenance: a signed update manifest names the
new payload hash and shell version; the shell verifies the signature and runs the
normal artifact validation before replacing itself.

#### Browser security model and honest guarantees

| Guarantee | Browser (guest in a Worker) | Notes |
| --- | --- | --- |
| Memory isolation | **Strong** | Wasm linear memory; module-declared maximum checked before instantiation |
| Capabilities and host calls | **Strong only while the generated host is intact** | Any other script in the page's realm (XSS, extensions, third-party scripts) shares trust and could call imports or read guest memory. Mitigate: the guest runs in a dedicated Worker created from the bundle, a strict CSP, Subresource Integrity on the loader, Trusted Types, no other scripts. The origin's own permissions are an outer ceiling the host cannot widen |
| Deadline | **Coarse** | `Worker.terminate()` kills the whole instance; no per-turn graceful interruption. On the main thread there is none (development only) |
| Fuel | **Through a deterministic metering transform** (accepted) | The browser packaging step applies a deterministic metering transform to the same validated Wasm: a fuel counter decremented at function entries and loop headers, trapping at zero. Native builds keep Wasmtime fuel, so browser fuel units are defined by the transform, not equal to Wasmtime's. Published gas-metering instrumentations report overheads on the order of 5–15 % time and a few percent size; to measure for Varta |
| Host calls, output, patch budgets | Strong | Counted by the host |
| Stack | Engine-defined | Recursion depth differs from Wasmtime; conformance must not depend on exact overflow depth |

Capability mapping in the browser: `net.request`/`net.fetch`/`net.get` →
`window.fetch` after the URL check (credentials omitted, redirects errors);
`time.now` → `Date.now`; random → `crypto.getRandomValues`; `env.*` → an
embedder-supplied configuration map, never page state; `fs.*` → denied (a separate
origin-private storage capability could map to OPFS); `net.listen` → denied;
`process.exit` → ends the instance.

#### Generated hosts and cross-host conformance

- **Generator.** A tool in `varta-common`'s registry ecosystem emits, from
  `HostOperation`, `LanguageProfile` and the layout constants: the import table
  (names, exact signatures, minimum ABI, capability ids), validation tables and the
  binary patch/event encodings, as a small JS module. Hand-written JS supplies only
  the effects (DOM application, `fetch`, timers). A drift test compares the
  checked-in generated JS with the registry, like the documentation blocks.
- **Conformance under Node and browsers.** `varta-conformance` gains a host
  selector: compile with the Rust compiler as today, then execute each case either
  in the native worker or in a Node worker running the generated host, and compare
  observations. A browser run uses the same protocol in a headless browser in CI.
  Cases whose imports are not supported by a host are reported as host gaps, never
  as passes.
- **Embedding conformance.** The tool/run/scheduler cases of the probes become
  checked fixtures: positive, denied and malformed cases with expected envelopes.

#### Structured entry points

Tool and HTTP contracts are rejected in every binary64 profile today. Design: a
**v2 adapter ABI** shared by tools, HTTP, application events and view events: the
host passes one bounded JSON (or binary record) argument and receives one result,
through a compiler-generated guest adapter that runs the profile's own checked JSON
parsing, calls the typed handler and serializes the result, exactly as the HTTP
adapter does in the i32 profile. Schemas come from the checked TS types (records,
optional fields, unions, arrays, binary64, UTF-16 strings). Async handlers return a
promise; the adapter's execution runs the request-scoped checkpoint. Tool ABI v1 and
HTTP v1 keep their meaning; v2 is a new contract version under a new profile.

#### Storage, binary data and crypto

Later increments, each its own capability or capability-free contract:

- `kv.get`/`kv.set`/`kv.delete`/`kv.list` with namespace patterns; native backend a
  file under a pinned root, browser backend IndexedDB; bounded keys, values and
  total bytes.
- `Uint8Array`/`ArrayBuffer`/`DataView` (CORE-05), `TextEncoder`/`TextDecoder`,
  base64: guest code, no host imports.
- Crypto random: `crypto.getRandomValues`/`randomUUID` as an ungated,
  capability-free entropy source like `Math.random` (accepted);
  `crypto.subtle.digest` as guest code or a capability-free host provider.
- `Date` and `Intl`: a capability-free host provider with explicitly versioned data,
  or a guest library; a large gap for formatting libraries. **Concrete consumer:**
  the UARP console's Single Chat feed needs relative times ("2 minutes ago") and
  date separators. Required subsets: `Intl.RelativeTimeFormat` (`format` for the
  units second to year, `numeric` `always`/`auto`, `style` `long`/`short`) and
  `Intl.DateTimeFormat` (`dateStyle`/`timeStyle` and the `weekday`, `day`,
  `month`, `year`, `hour`, `minute` components, plus `formatToParts` for
  separators), `Date` construction from epoch milliseconds and its UTC/local
  getters. Proposed delivery, capability-free: CLDR data for exactly the locales a
  manifest lists (for example `"locales": ["en", "uk"]`) compiled into the
  artifact's static data, so formatting is guest code and identical on every host;
  the current locale and time-zone offset are read-only embedder configuration
  (not a capability, no page or environment access); reading the clock stays
  behind `time.now`. Time-zone rules beyond one current offset (historic or
  future transitions) would need a versioned tz data subset and are out of the
  first increment.

#### Permission model

**Status: accepted direction 2026-10-29; details pending.** Normative once each
part lands in its profile and in [architecture.md](architecture.md#capabilities-and-trust);
until then this is the agreed target. Part 1 has landed in v37, and parts 2 to
6 for headless and embedder hosts in v38
([broker v1](architecture.md#permission-broker-v1-turn_budgets): states, `query`, user-gesture `request`, `required`/`purpose`, decisions keyed by
the artifact hash, the prompt seam, audit log); prompt UI per target
(P9, P10b) and "allow for this session" remain. V1 narrows two points: a
permission in `prompt` state is not asked at its first use (only through
`request()`), and the headless decision file is the only persistent store.
Requirement from the owner: permissions are
handled professionally. A denial is never a fatal error, because a user will
later be able to confirm a permission at the moment it is needed (a consent
prompt). The security invariants of the capability model do not change; the model
adds a decision step and a user as a bounded grant source.

**1. Recoverable denials (new profiles, from v37).**

- Every capability-checked operation fails **recoverably** when it is denied or the
  platform refuses it. Asynchronous APIs (`fetch`/`net.request`, timers,
  `clipboard.write`, `ui.open_url`, future storage) **reject** their promise with a
  `DOMException` named `NotAllowedError`; synchronous APIs **throw** that
  `DOMException`, or keep returning a failed `VartaResult` with status 1 where the
  `VartaResult` ABI applies. A denial **never traps** and never ends the execution;
  only budgets, traps of guest code and authorized exits stay fatal.
- **Revision of round 3.** `copy_text` and the `ui.open_url` activation report
  every outcome to the guest: `copy_text` carries a request id and each request,
  and each external-link activation, gets one `command_result` record with the
  status `ok`, `not_allowed`, `refused_by_platform` or `invalid_context`, which the
  guest API maps to `NotAllowedError` (decided 2026-10-29,
  [round 5](embedding.md#view-draft-v0-round-5-command-results)).
- **Profiles.** The v37 net-request stream is implementing recoverable
  `net.request` denials and recoverable v37 timer denials now. **v36 is frozen**: a
  denied `timer_start` stays fatal there, exactly as its contract states. Earlier
  synchronous operations keep their recoverable `VartaResult` statuses, which
  already never trap.

**2. Permission states.** Each capability (with its concrete resource where the
grant has patterns) is in one of three states, following the
[Permissions API](https://www.w3.org/TR/permissions/):

| State | Meaning |
| --- | --- |
| `granted` | Package request ∩ host ceiling allow it, and the user has allowed it or it needs no consent |
| `denied` | Outside the request or the ceiling, or the user refused it; operations fail recoverably at once |
| `prompt` | Requested and within the ceiling, but no decision yet; the next use (or `request()`) asks the user |

- **Query (accepted 2026-10-29).** A guest API
  `varta.permissions.query({ name, resource? })` resolves to `{ state }` and never
  prompts; it **distinguishes `denied` from `prompt`**. Fingerprinting
  consideration: the answer reveals only this application's own state (the
  package request, the ceiling and decisions stored for this application
  identity), never another application's, and the broker answers within the
  current user's profile only. In the browser the host combines the broker's
  state with `navigator.permissions.query` where the browser exposes that
  permission: the stricter wins (`denied` over `prompt` over `granted`), and where
  the browser does not expose it the broker's own state is the answer.
- **Request (accepted 2026-10-29).** `varta.permissions.request({ name, resource? })`
  exists and asks explicitly, but it is valid only during a user-gesture turn: there
  it prompts if the state is `prompt` and resolves to the new state; in every other
  turn it does not prompt and resolves to the current state. Outside `request()`, a
  permission in `prompt` state is asked at its first use (subject to the
  user-gesture rule for UI-sensitive permissions).

**3. Manifest.** A requested grant gains, besides its exact resource patterns:

- `"required": true | false`. Required permissions are decided at install or first
  run (prompted once, or granted by policy); if the user refuses one, the shell
  says that the application cannot run instead of starting it half-authorized.
  Optional permissions start in `prompt` and are requested at the point of use.
- `"purpose"`: a short human-readable text shown in every prompt, for example
  "Connect to your UARP workspace". It is untrusted package text: the prompt shows
  it as a quote next to the broker's own description of the operation and the
  exact resource, never instead of them.
- The patterns stay exact and are the only resources a prompt can grant: a user can
  allow `https://api.uarp.example/v1/**` because the package asked for it, never a
  broader one.

**4. Permission broker (host).** One host component decides every concrete
operation, asynchronously:

1. Package request ∩ host ceiling, exactly as today (deny by default, exact
   operation and resource matching). Outside it: `denied`, no prompt.
2. Stored user decisions for this application identity: allow, deny, or allow for
   this session.
3. Otherwise the prompt UI of the target, or `denied` where no prompt exists.

- **Ceiling.** A user decision can only narrow or confirm within the ceiling; it can
  never exceed it. The ceiling stays embedder-owned and is never written by a
  prompt.
- **Identity and persistence.** Decisions persist per application identity: the
  artifact hash until C3 provenance exists, then the signer and application id (so
  an update by the same signer keeps decisions, unless its requests grew). Users
  can revoke decisions at runtime; revocation affects **new** operations, never one
  already authorized and running.
- **Audit log.** Every decision (source, operation, resource, outcome, time) is
  appended to a bounded host audit log the user and embedder can read; the guest
  cannot read or write it.
- **Prompt UI per target.**
  - Browser: the host page's own dialog outside the guest's view (the guest renders
    only inside its closed shadow root), or the browser's own permission prompt where
    the browser has one for that permission.
  - Native shell: a dialog drawn by the renderer **outside the guest's control**
    (its own window layer, not an L0 node), so a guest cannot spoof or overlay it.
  - Headless and server: a policy file only, never a prompt; anything unresolved is
    `denied`.
- **Anti-abuse.** No prompt from a background turn (a timer, I/O completion or
  any turn that is not a user-gesture turn); UI-sensitive permissions prompt only on
  a user gesture; prompts are rate-limited per application (for example at most one
  open prompt and a cool-down after repeated refusals), and repeated refusals
  become a stored `denied`.

**5. Budgets while waiting.** A decision that needs the user suspends the operation
through the scheduler: the guest's promise stays pending and **no fuel** is used
while waiting. The prompt does not count against a turn's deadline, because the
turn ends when the guest returns to the scheduler; the decision arrives later as an
ordinary task (a completion record) in a new turn. Synchronous APIs cannot wait: a
synchronous operation in `prompt` state fails at once with `NotAllowedError` (or
status 1), and the application asks through `request()` first. Within one
execution (the current model) the wait still counts against the execution
deadline; long-lived application instances bound only their turns.

**6. Invariants unchanged.** Deny by default; check before effect; exact
operation matching; grants are the only authority. A user decision is one more
**grant source**, bounded by the package request and the host ceiling, and its
result is a concrete grant for that application identity. No guest value, prompt
text or purpose string is ever authority.

**7. Increment placement.** Recoverable denials: v37 now (net-request stream).
Broker, permission states and `query`: with the application lifecycle increment
(P5), since they need tasks across turns. Prompt UI: with the browser bundle (P9)
and the OS bundles and first-run approval of the shells (P10b). The headless
policy-file path needs no new UI.

**Decided on 2026-10-29.** `request()` exists, user-gesture only; `query`
distinguishes `denied` from `prompt`; the result records of `copy_text` and
`ui.open_url` are the engine's choice and are specified in
[round 5](embedding.md#view-draft-v0-round-5-command-results).

Also accepted on 2026-10-29: `ui.open_url` admits the grant form `https://*`
for arbitrary external links (never for other capabilities); every such activation
prompts with the full URL and no decision is stored
([details](embedding.md#view-draft-v0-round-6-gestures-and-platform-refusals)).

**Open questions.** Whether "allow for this session" is offered; the exact identity
key before C3; how a required permission refused at first run is presented on each
target; the rate-limit values.

#### Evolving JSON: unknown fields and variants (strict only)

**Status: accepted 2026-10-29 (owner's final decision: strict only).** Checked JSON
stays strict in every profile, including future ones:

- **Unknown fields are rejected.** Parsing into a closed record
  (`varta.json.parse<T>`, `JSON.parse(text) as T`, `unknown as T`) fails when the
  input has a field the type does not declare.
- **Extra fields only by declaration.** A type that must accept additions says so
  explicitly with an index signature, `[key: string]: unknown`; such types keep
  preserving the unknown fields as today.
- **Unmatched discriminants raise an error.** A discriminant value that matches no
  literal member of a discriminated union is rejected with a catchable error in
  every profile with recoverable checked JSON or checked assertions (earlier
  profiles keep their existing checked-JSON failure). Applications that need an
  open set parse the discriminant as `string` and narrow by hand.
- **No tolerant mode and no fallback branch.** There is no option to ignore unknown
  fields and no implicit fallback member for unknown variants.

Considered and rejected: a tolerant mode that ignored unknown fields with a bounded
diagnostic and routed unmatched discriminants to a fallback member.

Context: the UARP API evolves additively and uarp-sdk keeps unknown fields in its own
types; applications on Varta model that with explicit index signatures and `string`
discriminants. The existing strict contract in [architecture.md](architecture.md)
already describes this behaviour; no new profile is needed for it.

#### Increments and acceptance

Each increment is a separate versioned contract with positive, negative and
resource tests, Wasmtime validation and, where a JS host exists, the same cases
under Node. Order (P1–P12):

| # | Increment | Acceptance | CORE | Pilot |
| --- | --- | --- | --- | --- |
| P0 | Embedding contract ([embedding.md](embedding.md)) with drift tests | Doc-only Node T1 host matches native (done in the probe) | 13, 14 | — |
| P1 | Generated JS host (T1, R1, R2 subsets) + conformance host selector for Node | Every conformance case whose imports the host supports matches native; probes become fixtures | 13, 15 | — |
| P2 | Thin host: Rust intrinsics linked into the guest under a new profile | Only the intrinsics an application uses are linked; JSON/Headers/value programs run identically in Node and Wasmtime; `app.wasm` growth, fuel deltas and cold start in the browser and on the Pi 5 recorded | 01, 10, 13, 16 | M1–M4 programs |
| P3 | Cross-target native packaging (`--target`, offline launcher metadata, musl aarch64) | Same artifact runs on a real Pi 5 (16 KiB pages) and on macOS/Linux; Windows CI | 14 | — |
| P4 | Structured entry points v2 (binary64/UTF-16 tools and HTTP) | A uarp-sdk-shaped typed tool with records/unions runs in native and Node hosts | 10, 13 | M2–M3 |
| P5 | Application lifecycle: long-lived instance, event turns, per-turn budgets, timers delivered as tasks; [permission broker](#permission-model), states and `query` | Timer and event ordering tests against Node logs; trap kills the instance without automatic restart; per-turn budget values set from measurements. **Runtime half implemented under v38 (`turn_budgets()`)**; pending: Node-log ordering cases for host events, per-turn UI values from target measurements, per-function fuel profiling | 07, 08, 09, 17 | M4–M5 (timers, backoff) |
| P6 | Async `fetch` (`net.request`, replacing `net.get`/`net.fetch` in the new profile), streaming bodies, `AbortSignal`, HTTPS through `rustls` | uarp-sdk `transport.ts`/`sse.ts` paths against a local test server, native and browser | 09, 10, 11 | M5–M6 |
| P7 | L0 view protocol + headless renderer (tree + accessibility snapshot) | Patch/event conformance cases; malformed patches fail closed | 10, 15, 18 | — |
| P8 | L1 reactive core package + `tsx` + template compilation (B) | TSX counter/list/form examples produce identical headless snapshots on every host | 02, 03, 18 | — |
| P9 | Browser bundle (Worker host + applier, CSP/SRI loader, deterministic metering transform), `varta dev --target browser` | The UARP console's `app.wasm` runs in Chromium, Firefox and WebKit with the headless snapshots; metering overhead measured; permission prompts in the host page or the browser's own prompts | 14, 16, 18 | M7 |
| P10 | Native renderer, first increment: box, text, flex, button, text input with IME, scrolling; `varta-desktop` shell (each dependency justified and pinned at adoption) | The UARP console's settings and list screens render on macOS and a Pi 5 with layout matching the headless renderer; startup, memory, frame time and keystroke latency measured | 14, 16, 18 | M7 |
| P10a | Prebuilt shells per target distributed through releases, `varta build --target desktop-…`, offline `runtime-info.json` labelled unverified | A macOS host builds Windows and Pi 5 bundles without a cross toolchain; the payload is validated by the builder's runtime | 14 | — |
| P10b | OS bundles (`.app`, `.exe`/MSIX, `.deb`/AppImage), manifest `app` metadata, first-run capability approval | Bundles install and run; denied grants stay denied; required permissions decided at first run and optional ones prompted at use in a dialog outside the guest's control | 14 | — |
| P10c | Startup: measure a pilot-sized compile; shell-generated cache through Wasmtime's cache feature; Winch for development starts | Startup budget met; tampered cache entries rejected | 14, 16 | — |
| P10d | Rest of the L0 v1 subset natively | Freeze criterion passes on native, headless and browser renderers; the **same UARP console `app.wasm`** passes the scripted acceptance session in the browser, the macOS native renderer, the Pi 5 (`desktop-linux-aarch64`) native renderer and the headless conformance run | 14, 15, 18 | M7 |
| P11 | Hooks subset over L1 (specified; never called "React compatible") | Subset spec plus differential cases | 03, 18 | — |
| P12 | KV storage, binary/encoding, crypto, Date/Intl provider (KV before P10d: the console keeps its API key there) | Per-capability denial and budget tests | 05, 06, 10, 11 | M4 |
| — | `.svelte` front-end (D) | Last; only after P8 is stable | 18 | — |

Interleaving with the pilot ladder: M1–M4 language work (the v36 streams) proceeds
unchanged, P0–P4 have no language dependency and can run in parallel; P5–P6 are
the runtime half of K4.4 step 5 and close M5–M6 together with async generators;
P7–P10d turn the pilot into the cross-target acceptance scenario at M7: the UARP
console below.

##### UARP console (cross-target acceptance application)

Confirmed by the owner on 2026-10-29 as the first application. It lives in this
repository at `examples/uarp-console/`, a product example kept apart from the
engine: a TS/TSX project that is not a Cargo workspace member, that no crate
depends on, and that takes the pinned uarp-sdk as an npm dependency installed under
`node_modules/`, ignored by version control and never vendored. It is built once into one
`app.wasm` with one manifest:

- **Screens:** settings (base URL, API key); side navigation between resources;
  paginated lists with "load more" (cursor pagination through `CursorPage<T>`); a
  detail card; create/edit forms with enums as `select`; streamed SSE text
  (`parseEventStream`, async generator); loading and error states (including
  `ProblemDocument` errors); a confirm dialog; toasts.
- **Behaviour:** search or filter input debounced by `setTimeout`; a new request
  aborts the in-flight one through `AbortController`; retry with backoff on
  transient errors.
- **Manifest:** `net.request` for the configured API base URL only; the API key is
  entered on the settings screen and kept in KV storage, never in page state or in
  the artifact.
- **Acceptance:** a scripted session against a local mock server (record/replay of
  fetch completions) yields identical headless snapshots (tree, layout rectangles
  within the freeze tolerance, accessibility tree) and event logs for the **same
  `app.wasm`** in four places: the browser (Worker host + applier), the macOS
  desktop native renderer, the Raspberry Pi 5 native renderer
  (`desktop-linux-aarch64`) and the headless conformance run. The console then runs
  visibly on all three display targets.

**Risks.** The layout-bound intrinsics (P2) are the largest hidden dependency of
every non-Wasmtime host; HTTPS adds a TLS dependency (`rustls`); owning a native
renderer (text input,
IME, accessibility, scrolling, CSS fidelity) is the largest new engineering risk;
browser fuel needs instrumentation with unmeasured overhead; the typed npm UI
ecosystem is small, so most value comes from DOM-free cores and our own L1;
controlled inputs and IME need careful applier work; a second front-end (D) would
double template typing work.

##### Accepted decisions (2026-10-29)

The owner accepted all thirteen recommendations of this design:

1. **UI order.** TSX with compiled signals first; a hooks subset later, never
   called "React compatible"; `.svelte` last.
2. **Renderer.** L0 is a DOM-like, specified, versioned HTML/CSS subset, rendered
   natively on desktop and Raspberry Pi 5; no webview is built now (decision 13).
3. **Resource-loading URLs** need the same `net.request` grant, whose URL patterns
   are the origin allowlist; there is no separate `ui.load` capability.
4. **Thin host.** The existing Rust pure intrinsics are linked into the guest Wasm
   under a new profile and never reimplemented per host. Only the intrinsics an
   application uses are linked, and the growth of `app.wasm` and cold start in the
   browser and on the Pi 5 are measured.
5. **Browser fuel.** The browser packaging step applies a deterministic metering
   transform to the same validated Wasm; native builds keep Wasmtime fuel.
6. **Network.** `net.request` replaces `net.get`/`net.fetch` in new profiles; HTTPS
   uses `rustls`.
7. **Foreign-target shells.** A declared `runtime-info.json` is acceptable,
   explicitly labelled unverified, until C3 signing exists.
8. **Per-turn budgets.** Values are set after measurement; a trapped instance is not
   restarted automatically.
9. **Entropy** is ungated, like `Math.random`.
10. **CORE ledger.** Rows CORE-17 (application lifecycle) and CORE-18 (view output)
    are added.
11. **Renderer stack.** winit, taffy, parley/fontique, vello with a CPU fallback,
    AccessKit and resvg; each dependency is justified and pinned exactly at
    adoption. Prebuilt shells are distributed through releases, with signing later.
12. **Startup.** A shell-generated cache through Wasmtime's own cache feature;
    Winch for fast development starts.
13. **No webview fallback** for now.

Later the same day the owner chose the console's **Single Chat** UX and accepted
two L0 v1 capabilities, user-gesture only with exact grants: `ui.open_url`
(external links opened by the renderer) and `clipboard.write` (`copy_text`). Their
contract is the [round 3 draft](embedding.md#view-draft-v0-round-3-chat-requirements);
v38 implements both in the headless renderer.

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
dispatcher's first caller. Step 5's timers are implemented under v36
(`timers`); its asynchronous I/O and step 6 are not implemented.
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
   **Timers implemented** in v36 (selector `timers`,
   [contract](architecture.md#timers-timers)): `setTimeout`, `setInterval`,
   their clears, the sleep pattern and `AbortSignal.timeout` on a runtime
   event loop after the microtask checkpoint. A timer is a scheduled job
   frame awaiting an engine-internal promise, so the existing frames root its
   callback and arguments and the runtime keeps only serials and ids; the
   queue is ordered by (due time, sequence) on a virtual loop clock that the
   runtime advances to each due time while it waits for the wall clock.
   Decided in this increment: timer ids are numbers (browser model, not
   Node's `Timeout`); arming a timer requires the exact `time.timer` grant
   (deny by default, fatal at the call); a timer due at or after the
   execution deadline ends the execution with a budget error instead of
   waiting; at most `MAX_PENDING_TIMERS` are pending; the single execution
   deadline still covers the whole loop (no separate wait budget yet). Open:
   asynchronous I/O, cancellation of host operations through `AbortSignal`,
   abort listeners (function fields), request-scoped loops for async HTTP and
   a per-case grant in the conformance runner.
6. Async generators, `for await`, top-level await and compatibility expansion.
   Exit: the uarp-sdk pilot steps M5–M6 compile and pass their tests.
   **Async generators and `for await` implemented** in v36 (selector
   `async_generators`, branch `v36-async-generators`,
   [contract](architecture.md#async-generators-async_generators)): an async
   generator object is the frame of an internal body generator whose
   completion cell holds a guest state record (request queue and
   `[[AsyncGeneratorState]]`); guest helpers implement the ECMAScript
   Enqueue/Resume/CompleteStep/DrainQueue/AwaitReturn operations, and each
   `await` in a body is a scheduled relay job that steps the body, so suspended
   generators hold no live-frame slot and are collected. `for await` covers
   async generators, arrays and synchronous generators, and `yield*` delegates
   to async generators, with Node's job counts
   (`varta-runtime` `async_generator_tests`, Node 24 logs, a collection forced
   before every job). No host import, value tag or layout is added. Measured
   with production budgets: about 10.0k fuel per `for await` step, 3.3k per
   `await` in a body (1.7k in an async function). Open: `yield*` over arrays
   and synchronous generators, async generator expressions and object methods, `Symbol.asyncIterator` and other
   async iterables, top-level `await`, `await` inside intrinsic value blocks
   other than checked assertions, and a cheaper relay (the per-step cost limits
   one execution to about 100 `for await` values).

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
