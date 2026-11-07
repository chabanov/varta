# Varta functional overview

Checkpoint: 2026-11-06, implementation on `main` (audit fixes of that day).

Varta compiles strictly typed TypeScript ahead of time into portable WebAssembly.
Native applications execute in Wasmtime through a capability-checked host;
browser applications execute the Wasm through the official metered JavaScript
host, normally in a dedicated Worker. Native UI uses Varta's renderer without a
WebView; browser UI uses the View protocol's DOM applier.

The current stage is **v38 stabilization, unreleased**. This document collects
the implemented surface and its remaining boundaries. It describes the combined
v38 surface; earlier profiles retain their own admission rules and behavior.
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
| Types and programs | Interfaces/aliases, unions and narrowing, optional fields, checked `unknown`/assertions, classes/inheritance/generics/private fields, functions/closures, supported control flow, modules and re-exports; v38 iteration protocol: `[Symbol.iterator]()` iterable classes, generator methods, array literal spreads, `Array.from` of iterables and synchronous `yield*` ([contract](architecture.md#iteration-protocol-iteration_protocol)); v38 K9 type level: conditional types with `infer`, mapped and template literal types, every utility type, `keyof`/`T[K]`/`typeof` queries, record intersections, call/construct signatures and constructor types, variadic/optional/rest tuples, type predicates, assertion functions, `satisfies`, polymorphic `this`, `never` exhaustiveness and tsc's literal inference ([contract](architecture.md#type-level-type_level)); v38 declarations and classes: enums with reverse mapping, namespaces and declaration merging, abstract classes, function/method/constructor overloads, setters and auto-accessors, static fields and `this` in static methods, class expressions and local classes ([contract](architecture.md#declarations-and-classes-declaration_forms)) | General dynamic properties/prototypes/descriptors, hand-written iterator objects and iterable destructuring, wider receiver behavior and remaining type/module semantics |
| Values and built-ins | Number/BigInt/UTF-16 strings, objects, sparse/resizable arrays, hashed dictionaries, tuples, Map/Set, JSON, admitted Math/String/Array operations, ahead-of-time RegExp; v38 standard library (K9 D): V8-order `sort`/`toSorted`, the ES2023 copying methods, `splice`/`flat`/`flatMap`, iterator helpers, Set methods, `Object.groupBy`/`Map.groupBy`/`Object.fromEntries`, `JSON.stringify` property lists and indentation, `AggregateError` and the complete `Date` in the UTC zone ([contract](architecture.md#standard-library-standard_library)); v38 `Symbol` values with their own value tag ([contract](architecture.md#symbol-values-symbol_values)) | Symbol-keyed properties, weak references, Proxy/Reflect, integrity levels, `structuredClone`, object coercion, wrappers, Intl, host time zones, wider RegExp and full conversion inventory; static blocks, effectful static initializers and decorators (code at class definition), enum/namespace objects as values, class index signatures |
| Async language | Promise chains/combinators, async/await, sync/async generators, for-await, timers, microtasks and abort support | Structural thenables, top-level await and wider async iterable forms |
| Runtime | Bounded executions, isolated HTTP request instances and long-lived application instances with event turns, host tasks, permission answers and captured state | Broader sustained cancellation/shutdown, retained-resource and target evidence |
| Collection | Exact roots/tracing across values, closures and suspension, incremental collection and idle maintenance; large-heap bitmap clearing before the root snapshot | Sustained leak/retained-root checks, collector tuning and peak-memory/RSS measurements |
| Authority | Deny-by-default grants, exact operation/resource checks, package request intersected with a host ceiling, permission broker and validated artifact/ABI boundaries | Broader policy/permission UX, independent review and complete alternate-host acceptance |
| System access | Text file reads/printing under virtual roots, environment access, clock/timers, logging and guest exit | General filesystem writes, arbitrary subprocess spawning and a broader standard system API |
| Client networking | Capability-gated async HTTP(S) fetch, headers, streamed response reads, cancellation/timeout and admitted Web request/response values | Complete Web compatibility and all cross-host network behavior |
| Server networking | Sync/async HTTP v2 handlers, JSON and streamed Web Response bodies, Web Request ingress with streamed request bodies, HTTP/1.1 keep-alive, async Wasmtime with pooled instances and admission, TLS 1.3 with HTTP/2, fuzz/flood tests and local benchmarks | Per-stream HTTP/2 write idle, h2c, HTTP/3, certificate reload, Linux and sustained-load acceptance |
| Persistent data | Native Deno-KV-compatible files; browser OPFS backend with the shared Rust value codec; typed reads, lists and atomic updates | Async storage completion, watch/queues and complete Deno KV compatibility |
| UI | TSX, checked tree/style/event batches, native layout/paint/input and official browser view sessions | Complete native L0/HTML/CSS/IME/accessibility and cross-renderer acceptance |
| GPU UI | Optional wgpu/Vello backend, shared bounded display scenes, CPU recovery and direct mixed-border color fields | Direct GPU opacity groups, other devices and full presentation/performance acceptance |
| Distribution | Portable Wasm/manifest, source-free native launcher, packaged native shell and browser bundles with checked resources | Complete OS/CPU packaging matrix, artifact authenticity and release acceptance |
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

Language-level async/await is implemented. The planned HTTP execution migration
to Wasmtime `async_support`/`call_async`, yielding and pooling is an independent
server scalability increment; see [K7](development-plan.md#k7--modern-http-server-agreed-design-2026-11-05).

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
  admitted counters. The API returns promises, but current storage operations
  complete within their host call; nonblocking completion, watch and queues
  remain future work. Native file compatibility and browser OPFS are specific
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

Styling can be generated: a project's CSS-first Tailwind entry
(`"tailwind": {"entry": …}`) is compiled at build time in Rust (a port of
Tailwind CSS v4.3.5, oracle-checked) from the class tokens of the module
graph, and the canonical stylesheet replaces the entry's text resource;
`className` is an alias of `class`. Utilities over static class tokens
apply; a full `@import "tailwindcss"` build still needs the selectors of T2
stage S5 (Preflight and the theme's `:host`), so full Tailwind applications
do not apply yet ([contract](architecture.md#build-time-tailwind-stylesheets-k8-t1)).

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
| macOS arm64 | Main local full gate; native shell, font, renderer and scoped Metal/browser observations | Complete real-window/system-input, sustained memory/latency and release acceptance |
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
cross-target parity remain open. The newest full gate does not rerun every
pilot/browser/hardware observation.

## Verification and completion boundary

The full gate recorded in
`target/verification/20261105-225401-b9bonnxr/report.json` passes all ten commands
on unchanged inputs: **3861 Rust tests, zero failures, 17 ignored, and 218 browser
Node tests**. It includes the large-heap idle-work test and the corrected
codegen-v2 fuel comparison. Its recorded HEAD is `6dfb06e` with the GC changes
present; `10ff9ae` commits that verified code. See
[current verification evidence](verification.md#current-verification-evidence--2026-11-06).

The checked-v38 ECMAScript 2025 inventory has 175 passing and five reviewed
unsupported cases out of 180 after the K9 A, B and D cases of 2026-11-06;
eight feature rows have no executable case.
The baseline matches. This is a bounded compatibility inventory rather than
Test262, full language compatibility or a global completion percentage; see
[the inventory](../conformance/README.md).

Remaining delivery work is grouped into four broad acceptance boundaries:

1. Wider language/value/module/API semantics and package-own-test execution.
2. K7 H1–H4 server work and its security/resource/performance evidence.
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
