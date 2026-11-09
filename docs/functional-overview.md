# Varta functional overview

Current accepted local package: [fresh startup readiness and private entry paths](application-readiness.md), with Cloud generation-fenced promotion. Pending candidates can initialize with reads, while writes require the active generation. Actual Studio authentication/opening, two real app builds, six Chrome phases, four distinct native executions and retained data45/revision5 after full service restart pass. One completed-package full gate passes 14/14 commands, 4735 Rust tests (26 ignored), 196 Node tests and all three compatibility baselines (core 246/248 plus two known unsupported cases; v39 3/3; v40 4/4). See the [package receipt](evidence/2026-11-08-application-readiness-verification.json) and [full report](evidence/2026-11-08-application-readiness-full-report.json). Deployment commands are API-only; Studio supplies the authenticated stable entry. Production/public TLS and editor deployment controls remain separate work.

Previous coordinated application functionality: [durable run recovery](evidence/2026-11-08-application-recovery-verification.json)
in Cloud, over the unchanged accepted engine. KV retains desired state and
fenced attempts; workers stage/reconcile/cancel without holding native parent
credentials. After API/vault/host restart, a new owner session creates a new
scope/attempt while logical run/application/resource/release IDs and data remain.
Normal Chrome 41→42→restart→fresh session→43 and a fresh read-only 43/revision 3
pass, together with old-session rejection and same-owner/different-parent ticket
denial. All 357 source files and 38 logs match; 15 targets/62 groups pass. This is a
Cloud targeted package, not a new engine/full test run or restored browser heap.

Previous accepted local package: [app-scoped browser host](application-host.md)
with explicit source requests, canonical artifacts, per-run authority, one-use
activation, applied-frame readiness and native parent cancellation. Normal
Chrome accepts unrelated localhost cookies without treating them as authority;
duplicate own cookies and cross-run/path/method expansion fail. Native and
coordinated Cloud Chrome read 41/click 42, confirmed stop and fresh read-only
reopen 42/revision 2 pass. At that preceding checkpoint all 333 Cloud source files and 58 logs matched;
13 offline targets pass 50 groups.

One full gate passes 14/14 commands, 4731 Rust tests (26 ignored), 195 Node and all
three compatibility inventories on unchanged code. See the [package receipt](evidence/2026-11-08-application-host-verification.json).
Legacy preview remains deny-all. First hosting is HTTP localhost. Cloud durable
run reconciliation is accepted above; stable private addresses, active release
promotion and Studio launch/update remain separate. Production admission stays off.

Previous accepted package: [native protected-session delegation](native-delegation.md)
and the Cloud async lookup adapter, accepted as a host-only building block.
Explicit host policy enables native issue/lookup/revoke and method/path-scoped
relay, with immutable request provenance, bounded proof/scope lifetime and
cancellation on parent loss. No guest import, ABI or language profile changes.
Actual one-worker buffered/streaming native HTTP and coordinated Cloud synthetic
SDK plus real HTTP/KV/CAS/revoke/logout/restart acceptance pass. The checkpoint's
299 source files and 19 logs were independently matched.

One final full gate passes 14/14 commands, 4725 Rust tests (26 ignored), 195 Node
tests and all three compatibility inventories, with unchanged sources. See the
[package receipt](evidence/2026-11-08-native-delegation-verification.json).
That native-only checkpoint did not establish browser launch. The current local
application-host package above supplies its explicit build/transport boundary;
public stable launch and active promotion remain separate.

Previous package: [bounded native HTTP execution](execution-host.md). A separate
artifact-only host stages and independently validates a checked Wasm/manifest
pair, then runs a loopback-only service in a bounded child with explicit
authority. Listening readiness follows startup; stop/expiry revoke the endpoint
and become terminal only after the child is reaped. Replay, cancellation
tombstones and parent-loss termination are covered. Five new unit and two native
CLI cases pass; one full gate passes 14 commands, 4716 Rust tests (26 ignored)
and 195 Node tests. This is not native GUI, persistent private deployment or a
browser resource session.

Previous implementation: [local preview/browser launch](preview-host.md), with
Cloud durable artifact custody and preview orchestration kept in application
code. The separate native host validates immutable official browser bytes
without executing them, then grants one isolated browser run with deny-all
permissions, one-use activation, HttpOnly per-run custody and a short client
lease. Stop, expiry and lost leases revoke server authority; the trusted browser
bridge closes the Worker when its callbacks run. Real Chrome TSX
first-frame/click, protected cookie isolation, denied network access, stop and
guest-trap acceptance pass. One full attempt failed on four outer test-host
readiness waits. Test-only corrections and all remaining checks pass: 4709
unique Rust tests, 26 ignored, 195 Node, core 246/248 with two known
unsupported, v39 3/3 and v40 4/4. The original full remains failed; this is
targeted completion. See the [current
receipt](evidence/2026-11-08-preview-host-verification.json). Studio exposes prepare/open/confirmed stop; detailed guest health, durable
history quotas/indexing and archive-download UX remain application follow-up.
No new live OTP,
production HTTPS, native OS launch or native service deployment is claimed.
Earlier dated verification remains historical below.

Previous verified package: the [local trusted toolchain host](toolchain-host.md)
checks/builds immutable supplied-file bundles through the existing compiler and
browser artifact validator. It pins the compiler executable, authenticates the
server client, bounds workers/data/time, recovers lost acceptance replies and
cancels/reaps compiler processes without running submitted code. Runtime and
launcher remain execution-only. Cloud headless native TSX Studio through real
HTTP/API/KV/separate worker and six direct native scenarios pass on the pinned
final CLI. This is no new Chrome/HTTPS/live-account acceptance or durable artifact
byte ingest/preview.

Native KV identity now checks the actual SQLite connection, including cached
descriptor reuse, through paired small read-only pinned vendor patches. Forty
KV/HTTP tests pass; 400 parallel requests have zero identity errors. One final
full gate passes **14 commands, 4700 Rust tests (26 ignored), 195 Node tests**;
core 246/248 has two known unsupported cases, v39 passes 3/3 and v40 4/4.
Source fingerprints match; only documentary result annotations follow. See the
[package evidence](evidence/2026-11-07-toolchain-host-verification.json).

Previous accepted implementation package: Studio stabilization with official local HTTPS
delivery, autonomous session expiry during suspended I/O, and repeated
reset/cancellation/resource checks. Independent Cloud acceptance exercises the
actual Studio UI over trusted local HTTPS, including real OTP, save/reload,
logout and provider rejection of the old key. An initial unconfirmed request-code
response is recorded with its existing-code recovery. One full run passes all
14 commands with unchanged sources: 4690 Rust tests (26 ignored), 195 Node tests,
core 246 passed / 2 known unsupported out of 248, v39 3/3 and v40 4/4. Temporary certificate trust and key were removed. Evidence is in the
[stabilization receipt](evidence/2026-11-07-studio-stabilization.json).

Previous accepted package: protected browser-to-native BFF sessions, opt-in and
unreleased. A bounded memory-only server vault holds credentials; Studio uses a
separately granted channel with exact host origins. Real HTTPS engine acceptance
and synthetic Cloud login/project/reload/restart/logout/second-account acceptance
pass. One full snapshot passed, followed by targeted verification of two final
cancellation/stream-error fixes; this is not a repeated full pass of the final
code. See the [session evidence](evidence/2026-11-07-protected-session-verification.json).
Restart ends sessions; encrypted persistence and a native protected client remain
future work. See the [contract](architecture.md#protected-sessions-protected_sessions).

Previous verified checkpoint: 2026-11-07, checked snapshots, Tailwind/CSS shadow motion and the
subsequent async HTTP/KV correction on base `1407f8eb`. The combined snapshot
passes [full verification](evidence/2026-11-07-precommit-verification.json):
4671 Rust tests (26 ignored), 190 Node tests and both compatibility inventories.
The KV fix also has separate [scoped evidence](evidence/2026-11-07-http-kv-async-verification.json).

Varta compiles strictly typed TypeScript ahead of time into portable WebAssembly.
Native applications execute in Wasmtime through a capability-checked host;
browser applications execute the Wasm through the official metered JavaScript
host, normally in a dedicated Worker. Native UI uses Varta's renderer without a
WebView; browser UI uses the View protocol's DOM applier.

The verified working-tree increment permits complete constructor receivers in
classes without non-template descendants, checked after generic materialization.
It covers publication, methods/getters/setters, closures, abrupt completion and
async retention; partial receiver observation remains outside this increment.

The current stage is **v38 stabilization with opt-in SDK/session increments,
unreleased**. V39 adds [checked snapshots](architecture.md#checked-structural-snapshots-v39):
recursive declared-field checks, retained extras, isolation, enumeration and JSON
round trips. Native, standalone and Chrome synthetic SDK acceptance passes;
the initial verification used one full attempt followed by scoped corrections
and completion. That original full report remains failed; see the
[snapshot evidence](evidence/2026-11-07-checked-snapshot-verification.json). Earlier
profiles retain their frozen module hashes and admission rules.

The subsequent CSS motion package adds native shadow/ring transitions and
keyframes, including unequal lists, currentcolor, reversal and retained damage.
Stylesheet `transition` shorthand uses the existing checked longhands. The
[full gate](evidence/2026-11-07-shadow-motion-verification.json) passes on unchanged
inputs for that CSS snapshot. Direct
GPU shadow rendering and broader native table/CSS support remain separate work.

The project remains in stabilization. This document collects
the implemented surface and its remaining boundaries. It describes the combined
v38 surface; earlier profiles retain their own admission rules and behavior.
The preceding follow-up adds synchronous generator signature inference,
union-to-record views and three module compatibility cases. The current increment
adds initializer identities, checked unbound method values and tuple-rest calls,
inference and finite signature adapters. The current source
snapshot is recorded in the verification evidence.
An API name below means its checked implementation, not its entire JavaScript,
TypeScript, Web, Node or Deno counterpart.

Use the [status and delivery plan](development-plan.md#status) for current work,
the [language guide](language-guide.md) for examples and the
[architecture contract](architecture.md) for normative semantics. Registry facts
come from the generated [version matrix](architecture.md#version-matrix),
[budget limits](embedding.md#manifest-limits) and
[entry-point limits](embedding.md#entry-point-limits), rather than independent
defaults maintained in this overview.

## Functional inventory

| Area | Implemented surface | Remaining boundary |
| --- | --- | --- |
| Compiler | Shared TS parsing, resolved typed IR, Wasm emission and validation for check/dev/build; explicit profile negotiation and source-aware diagnostics | Deliberate checked subset; unsupported syntax/types produce diagnostics |
| Types and programs | Interfaces/aliases, unions and narrowing, compatible union-to-record views, optional fields, checked `unknown`/assertions, classes/inheritance/generics/private fields, functions/closures, v38 synchronous generator signature inference, initializer identity fields, complete constructor receivers in classes without descendants, checked unbound method values, tuple-rest calls and inference, supported control flow, modules and re-exports; v38 iteration protocol: `[Symbol.iterator]()` iterable classes, generator methods, array literal spreads, `Array.from` of iterables and synchronous `yield*` ([contract](architecture.md#iteration-protocol-iteration_protocol)); v38 K9 type level: conditional types with `infer`, mapped and template literal types, every utility type, `keyof`/`T[K]`/`typeof` queries, record intersections, call/construct signatures and constructor types, variadic/optional/rest tuples, type predicates, assertion functions, `satisfies`, polymorphic `this`, `never` exhaustiveness, tsc's literal inference, checked pattern template types and class arrays shared under ancestor element types ([contract](architecture.md#type-level-type_level)); v38 declarations and classes: enums with reverse mapping, namespaces and declaration merging, abstract classes, function/method/constructor overloads, setters and auto-accessors, static fields and `this` in static methods, class expressions and local classes, class values with static dispatch, enum/namespace objects, class index signatures, static blocks, effectful module initializers and temporal-dead-zone checks ([contract](architecture.md#declarations-and-classes-declaration_forms)); v38 expression forms: `await` in every operand, object-literal methods with `this`, constant computed keys, tagged templates, destructuring of iterables, hand-written iterators, `using`, structural thenables, object-literal getters and setters ([contract](architecture.md#expression-forms-expression_forms)) | General dynamic properties/prototypes/descriptors, wider receiver behavior and remaining type/module semantics |
| Values and built-ins | Number/BigInt/UTF-16 strings, objects, sparse/resizable arrays, hashed dictionaries, tuples, Map/Set, JSON, admitted Math/String/Array operations, RegExp (v38 K9 D3: the full ES2025 pattern language with backreferences, lookaround, modifiers, property escapes, `d` and `v`, matched by a backtracking matcher bounded by fuel; patterns built at run time compiled in the guest, `RegExp.escape`, match results as arrays, lazy `matchAll`, symbol protocols and custom matchers, [contract](architecture.md#full-regexp-full_regexp)); v38 standard library (K9 D): V8-order `sort`/`toSorted`, the ES2023 copying methods, `splice`/`flat`/`flatMap`, iterator helpers, Set methods, `Object.groupBy`/`Map.groupBy`/`Object.fromEntries`, `JSON.stringify` property lists and indentation, `AggregateError` and the complete `Date` in the UTC zone ([contract](architecture.md#standard-library-standard_library)); v38 console notation (K9 D1): multi-argument `console.log`/`info`/`debug` printed exactly as Node 24 prints them, class names, Errors, Map/Set/Date ([contract](architecture.md#console-notation-node_console)), standard error and the rest of Node's console (`error`, `warn`, `trace`, `assert`, `group`, `count`, `time`, `dir`, `table`; [contract](architecture.md#console-methods-console_methods)); v38 `Symbol` values with their own value tag ([contract](architecture.md#symbol-values-symbol_values)), object integrity levels ([contract](architecture.md#integrity-levels-integrity_levels)), local-time `Date` and linked locale data ([contract](architecture.md#local-time-local_time)); v38 `WeakMap`/`WeakSet`/`WeakRef`/`FinalizationRegistry` over ephemeron tables in the collector ([contract](architecture.md#weak-references-weak_references)) | String-key enumeration of symbol-keyed records, Proxy/Reflect, `structuredClone`, object coercion, wrappers, the remaining Intl API surface, and full conversion inventory; standard decorators, tagged class values in unions/arrays/records/`unknown`, and remaining static-property ordering cases |
| Async language | Promise chains/combinators, async/await (v38: in every operand position), sync/async generators, for-await, timers, microtasks and abort support; v38 structural thenables of statically known types, `await using` and top-level `await` in module initializers | Thenables through unions or `unknown` and wider async iterable forms |
| Runtime | Bounded executions, isolated HTTP request instances and long-lived application instances with event turns, host tasks, permission answers and captured state | Broader sustained cancellation/shutdown, retained-resource and target evidence |
| Collection | Exact roots/tracing across values, closures and suspension, incremental collection and idle maintenance; large-heap bitmap clearing before the root snapshot | Sustained leak/retained-root checks, collector tuning and peak-memory/RSS measurements |
| Authority | Deny-by-default grants, exact operation/resource checks, package request intersected with a host ceiling, permission broker and validated artifact/ABI boundaries | Broader policy/permission UX, independent review and complete alternate-host acceptance |
| System access | Text file reads/printing under virtual roots, environment access, clock/timers, logging and guest exit | General filesystem writes, arbitrary subprocess spawning and a broader standard system API |
| Client networking | Capability-gated async HTTP(S) fetch, headers, streamed response reads, cancellation/timeout and admitted Web request/response values | Complete Web compatibility and all cross-host network behavior |
| Server networking | Sync/async HTTP v2 handlers, JSON and streamed Web Response bodies, Web Request ingress with streamed request bodies, HTTP/1.1 keep-alive, async Wasmtime with pooled instances and admission, TLS 1.3 with HTTP/2, connection and per-stream write-idle bounds, fuzz/flood tests and local benchmarks | h2c, HTTP/3, certificate reload, Linux and sustained-load acceptance |
| Persistent data | Native Deno-KV-compatible files; browser OPFS backend with the shared Rust value codec; typed reads, lists and atomic updates | Async storage completion, watch/queues and complete Deno KV compatibility |
| UI | TSX with reactive `data-*` attributes, shared Rust stylesheet/cascade engine and CSS error recovery, checked tree/style/event batches, native layout/paint/input and official browser view sessions | Complete native L0/HTML/CSS/IME/accessibility and cross-renderer acceptance |
| GPU UI | Optional wgpu/Vello backend, shared bounded display scenes, CPU recovery and direct mixed-border color fields | Direct GPU opacity groups, other devices and full presentation/performance acceptance |
| Distribution | Portable Wasm/manifest, source-free native launcher, packaged native shell, browser bundles and isolated local preview lifecycle | Complete OS/CPU packaging matrix, artifact authenticity and release acceptance |
| Tool contracts | Explicit publication, offline catalog/schema inspection and checked JSON invocation of primitive tool signatures | Shared transactional application/tool state, identities, general Application Contracts, MCP and LLM integration |

## Execution, async and memory

A long-lived application keeps its state between event turns. The initial turn
runs startup and its microtasks; later turns deliver a timer, network completion,
host event, permission answer or view event and then drain the queued microtasks.
Promise and generator continuations use managed frames and the checked resume
dispatcher. The host scheduler holds continuation identities rather than guest
references.

HTTP request isolation is a separate contract: each request gets its own bounded
instance. Request-owned async/streaming work stays with that request until its
response ends or is cancelled. Shared persistence is provided through admitted
host APIs such as KV, not an implicitly shared request heap.

Language-level async/await is implemented. HTTP execution also uses Wasmtime
`async_support`/`call_async`, yielding and pooled instances; broader server
acceptance remains open; see [K7](development-plan.md#k7--modern-http-server-agreed-design-2026-11-05).

| Policy | Authoritative value | Meaning |
| --- | --- | --- |
| V38 turn fuel | `budget::TURN_FUEL` in the generated [budget table](embedding.md#manifest-limits) | Work available to a turn; fuel includes metered guest/charged host work and is not elapsed time |
| V38 application memory | `limits::V38_MAX_MEMORY_BYTES` in that table | Linear-memory policy, not a measured RSS or a promise that every instance commits that amount |
| Idle collection | `budget::MAINTENANCE_FUEL`, `MAINTENANCE_STEP_FUEL` and the [collector contract](architecture.md#string-memory-reclamation) | Separate maintenance budget; bounded bitmap initialization prevents a large heap from making one initialization step proportional to the entire heap |
| Network read inactivity | `net::READ_INACTIVITY_SECONDS`, [network request contract](embedding.md#network-requests-v37) | A separate wait-for-bytes window, still bounded by lifetime/cancellation policy |
| HTTP request/connection/worker limits | Generated [entry-point limits](embedding.md#entry-point-limits) | Request memory and concurrent service resources have their own bounds |

The v38 budget/serial changes follow recorded owner decisions; earlier profiles
retain their declared behavior. Promise/timer identities are not reused, while
live state remains bounded. Neither larger limits nor the passing regression
suite establish sustained throughput, latency or leak freedom.

## System, data and package APIs

- **Files and environment:** file access uses pinned, host-mapped virtual roots
  under v38, with traversal/symlink refusal and checks before I/O. The current
  filesystem surface reads text; KV is the implemented persistent write path.
  Environment access and guest exit require their exact grants. Logging is a
  separate operation from authority to read a resource.
- **Web values:** admitted URL/URLSearchParams, Headers, Request/Response,
  AbortController/AbortSignal, Blob/File/FormData and readable response streams.
  Client Web Request values do not imply Web Request input support in the server.
  Existing network bodies can be streamed/forwarded; this is not a claim of a
  complete arbitrary Web Streams producer/controller API.
- **Binary and crypto:** admitted Uint8Array/ArrayBuffer and Web byte paths,
  UTF-8 TextEncoder/TextDecoder, base64, random values and UUID generation.
  General typed-array/DataView ownership/interoperability and a complete Web
  Crypto API remain outside the implemented surface.
- **KV:** `varta.kv.open`, typed `get`/`getMany`, `set`/`delete`, `list` with
  selectors/cursors, expiration, version checks and atomic updates, including
  admitted counters. Async HTTP request Stores can read managed values and keys
  through suspendable guest allocator callbacks, including under a one-worker
  host policy. Storage work still settles within the initiating turn;
  nonblocking database I/O, watch and queues remain future work. Native file compatibility and browser OPFS are specific
  contracts, not general Deno API compatibility.
- **Packages:** v38 resolves supported bare/scoped package specifiers and exact
  export subpaths through project `node_modules`. The package must ship usable
  TypeScript sources located through its `types` metadata and declaration map.
  JavaScript/declaration-only packages, linked/symlinked packages and wildcard
  exports are not generally admitted. Namespace/side-effect/dynamic imports and
  wider module initialization behavior remain explicit gaps.

See [local modules](architecture.md#local-modules),
[KV storage](embedding.md#kv-storage-v38) and the
[source declarations](../crates/varta-common/src/varta.d.ts). Broader ordinary
JavaScript semantics and the unsupported inventory remain in the
[CORE ledger](development-plan.md#core-completion-ledger).

## HTTP and network ownership

The client implements async HTTP(S) fetch through exact `net.request` grants,
with checked URLs/methods/headers, response-body reads, aborts, timeouts and
owned completion cleanup. Legacy `net.get`/`net.fetch` retain their separate,
narrower contracts. Browser networking retains the browser's CORS and
header/platform restrictions.

The v38 server implements sync/async handlers receiving `VartaHttpRequest`,
ordinary responses and streamed Web Response bodies. It keeps request instances
isolated and bounds connections separately from active workers, including
keep-alive, worker waiting, stream idle/lifetime and disconnect/shutdown behavior.
The UARP proxy uses the streaming response path for upstream session events.

The agreed [modern HTTP server stages](development-plan.md#k7--modern-http-server-agreed-design-2026-11-05)
are merged ([record](development-plan.md#k7-merged-values-approved-2026-11-06)):

| Stage | Implemented | Still open |
| --- | --- | --- |
| H1 | HTTP/1.1 keep-alive, separate bounded connection/worker pools, Web `Request` ingress with streamed incoming bodies and host-lowered limits | — |
| H2 | Async Wasmtime calls yielding on epochs, pooled `InstancePre`, task/memory admission | — |
| H3 | Native TLS 1.3 with ALPN `h2`/`http/1.1`, HTTP/2 stream/header/reset bounds; a connection stays open while a body streams | Per-stream write idle, h2c, certificate reload |
| H4 | Fuzzing, flood and slow-client tests, local throughput/p50/p99/memory benchmarks (macOS arm64) | Linux and sustained-load benchmarks |

HTTP/3, WebSocket and response compression remain later work. The implemented
pool limits are safety bounds, not measured production capacity.

## UI and native interaction

TSX compiles to Varta's checked View operations and event dispatch. The examples
use ordinary TypeScript components, external CSS and reactive bindings. This is
a View protocol and an admitted styling surface, not general React/Svelte or an
unrestricted browser DOM in guest code.

| Part | Implemented native surface |
| --- | --- |
| Structure/layout | Block/flex/grid, dimensions and constraints, margins/padding/gaps, admitted positioning/z-index, inline shaping/line fragments, headings/lists/pre/labels |
| Paint | Background colors, solid borders and radii, outlines, clipping, isolated opacity, underline, packaged images (`img`), retained drawing/damage and mixed border colors |
| CSS engine (K8 T2, T3) | CSS Syntax 3 tokens (comments, escapes), HTML class names with escaped class selectors, `@layer` with `!important` and conditional layers, `@supports`, `@property`, CSS-wide keywords, Media Queries 4/5 features as recorded host inputs (macOS reduced motion and increased contrast on the desktop), `calc()`/`min()`/`max()`/`clamp()`, every length unit, Color 4/5 colours with named colours, gamut mapping and multi-colour `color-mix()`, numeric font weights, `cubic-bezier()`/`steps()`; one Rust engine, linked natively and run by the browser host as `view-engine.wasm` |
| Text | Unicode shaping, ligatures, combining marks, admitted bidi flows, wrapping and preserved whitespace, explicit font data, macOS system-family mapping, optical size and supported face/synthesis paths |
| Controls | Buttons, text/search/password/number inputs, textarea, checkboxes/radios, select, forms and modal dialogs with top-layer/focus behavior |
| Input | Pointer/keyboard, hover/active/focus/cursors, editing, selection/caret, Undo/Redo, scrolling and IME preedit |
| Animation | Color/background/opacity transitions and translations on a host-set animation clock; translated boxes update painting, hits and geometry; macOS overlay scrollbar fades on the same clock |
| Platform effects | Host-owned clipboard and external links under gesture/grant rules, with explicit denial/platform failure |
| Accessibility | Roles/names/states/descriptions, semantic snapshots, geometry, live updates and assistive actions; macOS adapter and scrollbar-action routes |

The native scope still has concrete gaps:

- Controls need admitted block/flex/grid placement; arbitrary atomic inline
  control layout and mixed block/inline formatting are not complete.
- The CSS engine refuses `ex`, `ch`, `cap`, `ic`, `lh` with
  `line-height: normal` and `rlh`, which need the first available font's
  metrics, and sums of lengths and percentages outside sizes, margins,
  padding, gaps, flex basis, insets and `vertical-align`.
  Increased-contrast and reduced-motion are read from the OS on macOS only;
  other desktops report no preference.
- Native `<img>` draws packaged images (the configuration's `images`, the
  same files the browser bundle carries) to the HTML and CSS standards:
  PNG/APNG, JPEG, GIF, WebP, BMP and ICO converted to sRGB with EXIF
  orientation, animations on the view clock, `srcset`/`sizes`,
  `object-fit`/`object-position`, `image-rendering`, `aspect-ratio`, and
  inline images in lines of text with `vertical-align`. AVIF, SVG, HDR
  images and alternative-text rendering are refused, and the optional GPU
  backend presents frames with images through its software fallback.
- General transforms/scaling and scrollable overflow from translated boxes
  remain unfinished. macOS scrollbars paint natively in both system styles
  (classic gutters, and overlay bars shown while scrolling and faded on the
  animation clock); dragging a thumb, clicking a track and Linux scrollbars
  do not.
- OS glyph fallback and wider real-font typography remain open. Some ordinary
  bidi shaping is implemented while RTL edge ordering and bidi on preserved
  whitespace paths remain separate gaps.
- Complete IME, VoiceOver/AT-SPI/UIA and actual window/clipboard/link/consent
  sessions require their own acceptance. Headless pixels and semantic metadata
  do not establish those outcomes.

Native CSS now paints outer/inset box shadows and Tailwind rings, including
rounded corners, wrapped inline decorations and opacity. Shadows preserve layout
and pointer bounds. Intrinsic width/height keywords include min/max-content,
fit-content and stretch; fit-content measures and clamps the intrinsic bounds.
Shadow scenes use software presentation on the optional GPU backend.

The remaining application package adds numeric font features, checkbox/radio
accents, inset clipping (`sr-only`), sticky positioning, `inline-block`/`inline-flex`,
legacy line clamping, best-effort pretty wrapping, linear gradients, backdrop
blur and clock-driven opacity/rotation keyframes. New rendering checks cover
actual ink, geometry, hits and retained frames. `table-layout` is admitted as
inert on existing non-table boxes; native table rendering is still absent.
Known combinations and broader effects remain explicitly limited in the
[CSS contract](architecture.md#view-css-engine-k8-t2-and-t3).
[Consolidated verification](evidence/2026-11-07-tailwind-css-remainder-verification.json)
records one failed full attempt, corrected scopes and completion of unfinished
suites: 4632 Rust/190 Node tests passed, 26 ignored. The final snapshot has no
new uninterrupted full pass. Real generated browser delivery and Cloud Studio
input/save are verified; the earlier shadow checkpoint is historical.

Styling can be generated: a project's CSS-first Tailwind entry
(`"tailwind": {"entry": …}`) is compiled at build time in Rust (a port of
Tailwind CSS v4.3.5, oracle-checked) from the class tokens of the module
graph, and the canonical stylesheet replaces the entry's text resource;
`className` is an alias of `class`. Utilities over static class tokens
apply, and the full `@import "tailwindcss"` entry with Preflight now passes
check/dev through the active `full_tailwind_application_applies` test. Native
rendering still has CSS gaps. The updated native/Wasm corpus admits all 312
recorded application candidates; broader CSS/Tailwind remains a separate scope
([contract](architecture.md#build-time-tailwind-stylesheets-k8-t1)).

Contracts: [native controls](architecture.md#native-control-defaults),
[preserved whitespace](architecture.md#native-preserved-white-space),
[dialogs](architecture.md#native-modal-dialogs-top-layer),
[transitions](architecture.md#native-css-transitions),
[transforms](architecture.md#native-css-transforms),
[the CSS engine](architecture.md#view-css-engine-k8-t2-and-t3),
[images](architecture.md#native-images-img) and
[the native shell](architecture.md#native-shell-varta-shell).

## GPU and application graphics

The optional wgpu/Vello UI backend consumes the shared bounded scene, keeps
input revisions/cache limits and can recover through CPU rendering. Distinct
border color fields can stay on GPU. Recorded Metal Debug/Release probes cover
that implementation; opacity groups still have explicit CPU fallback.

P13b's typed guest graphics API, buffers/textures/shaders/pipelines, frame callbacks
and game input are planned. P13c's typed 3D library/Three.js-port work depends on
that contract and a pinned source/admission inventory. UI GPU support supplies
neither a general guest GPU API nor unmodified Three.js compatibility. Device-loss,
other-device, real-window and whole-application performance acceptance remain open.
See [the GPU/graphics plan](development-plan.md#gpu-and-game-graphics-future-direction).

## Development, delivery and tools

The CLI supplies project scaffolding, shared checking/dev/build, run/validate,
inspect and explicit tool invocation. Diagnostics preserve source identity and
typed categories. Fuel/resource reporting and supported network record/replay
help reproduce measured application behavior; network replay is not a guarantee
of complete replay of every external effect.

`build --single` combines a runtime-only native launcher, portable Wasm and a
manifest for the build host. Native execution needs no compiler or JS engine.
`varta-shell` loads packaged view applications under grants/host policy.
Browser builds ship Wasm/manifest, the generated host/Worker and DOM applier,
checked PNG/font resources and loader integrity/CSP rules; browser dev supports
rebuild/reload. Main-thread browser mode is development-only and does not provide
the Worker's deadline boundary.

Tool Contract v1 explicitly publishes typed primitive handlers. Offline
inspection derives catalog/schemas/effects without running the application;
`varta invoke` checks JSON parameters/results and uses an isolated invocation.
Ordinary exports do not become tools automatically. Shared transactional tool
state, authenticated callers, general Application Contracts, MCP and LLM
integration remain future work.

## Platforms and demonstrated applications

| Target | Evidence at this checkpoint | Remaining acceptance |
| --- | --- | --- |
| macOS arm64 | Consolidated local verification (one full attempt plus scoped corrections and completion); native shell, font, renderer and scoped Metal/browser observations | Complete real-window/system-input, sustained memory/latency and release acceptance |
| Linux native | Implementation/CI paths and earlier probes | Current full target run, desktop integration and target-specific acceptance |
| Windows | Architecture/implementation paths | Real target builds, packaging and platform acceptance |
| Raspberry Pi 5 | Earlier aarch64 launcher cross-build/run probe | Actual Pi hardware, kernel/page-size, UI/GPU/input and package metadata acceptance |
| Browser Worker | Metered applications/views, OPFS, effects and recorded SDK/settings checks in Chrome; selected other-engine evidence | Updated streaming probe, current complete Chromium/Firefox/WebKit and pilot matrices |

The TSX Showcase and the UARP console pilot application were removed from the
repository on 2026-11-06 (owner decision); a structured developer examples package
replaces them. Their recorded evidence is historical and cannot be reproduced from
the current tree. It included:

- Pinned uarp-sdk 0.13.2: 65/65 source files checked and exported generic sites
  instantiated under the admitted profile.
- Real native SDK calls against a Varta loopback mock: 43 cases in six groups.
- Console: 249 tests in 36 groups and ten headless view checks.
- SDK groups recorded/replayed natively and replayed in Chrome with matching
  output/accounting; settings stored through browser OPFS survive reload.
- Native shell launch path with system fonts renders the console's first frame
  and each button's hover/pressed state headlessly.

These are separately recorded scoped checkpoints. SDK-own-test execution needs
the M7 harness; sustained live-service sessions, complete native windows and
cross-target parity remain open. The current consolidated verification does not rerun every
pilot/browser/hardware observation.

## Verification and completion boundary

The startup-readiness package passes one uninterrupted full run: **4735 Rust
tests and 196 browser Node tests**, with 26 Rust tests ignored and byte-identical
sources before and after; no production change followed the gate. See
[current verification evidence](verification.md#current-verification-evidence--2026-11-08).

The recovery batch combines the K9 A type-level steps, B declarations and
classes, C expressions and accessors, D1 console and standard-library additions,
D2 symbols and integrity levels, D3 RegExp follow-ups, D4 local time and locale
data, and D5 weak references and collections. Its combined conformance baseline
is superseded by the class/function inventory: 246 passed and 2 known unsupported
cases out of 248; 4 of 131 feature rows still have no executable cases. The bounded compatibility inventory is not
Test262, full language compatibility or a global completion percentage; see
[the inventory](../conformance/README.md).

The next compiler package addresses safe partial-initialization observation and
shared method identity across polymorphic/generic implementations, then the
remaining decorator contexts. The checked increment keeps these cases diagnosed;
standard decorators and property descriptors remain the two known unsupported
cases. See [next steps](development-plan.md#next-implementation-package). Async
generator inference and consumed-yield inference remain outside the preceding
inference package's bounds.

Remaining delivery work is grouped into four broad acceptance boundaries:

1. Wider language/value/module/API semantics and package-own-test execution.
2. Remaining K7 server features and broader security/resource/performance evidence.
3. Remaining native UI plus real-window, system-input and platform matrices.
4. Sustained resource/latency measurements, fuzzing, independent security review,
   artifact/release policy and production acceptance.

The updated Chrome streaming probe after the network inactivity change remains
unrun at this checkpoint. Broader GPU/application graphics are separate planned
increments. None of those requirements closes merely because a package merges
or the local regression gate passes.

## Keeping this overview current

- Update the checkpoint commit/date and evidence scope when implementation moves.
- Read registry-generated tables for versions and numeric limits; keep this
  overview's explanations linked to them.
- Distinguish implemented code, a recorded scoped observation, a full gate and
  outstanding acceptance. Retain superseded failures as history.
- Keep [the status/queue](development-plan.md#status), README and this inventory
  consistent. Normative semantics belong in architecture/embedding/profile rules,
  examples in the language guide and completed logs in history.
- Documentation-only updates need link/diff checks, not another engine test run.
