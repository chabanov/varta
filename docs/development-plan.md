# Varta development plan

## Status

Updated 2026-11-05 from `main` at `10ff9ae`. The native inline painting and
horizontal-edge packages, waves 1–3, the browser console leg and the native
console batch are integrated. Their earlier branch names and scoped measurements
below describe the original checkpoints, not outstanding merges. Current work
includes the v38 HTTP streaming/keep-alive server and the recorded changes to
application budgets, lifetime serials and network inactivity handling.

The large-heap incremental-GC bitmap-clearing fix is committed in `10ff9ae`.
Its full gate passes **3861 Rust tests and 218 browser Node tests**, with
17 ignored Rust tests and unchanged inputs across all ten commands. The earlier
codegen-v2 fuel comparison failure is superseded by this successful run; the
existing cost measurements were updated for the collector change. See
[current verification evidence](verification.md#current-verification-evidence--2026-11-06).

Varta remains an experimental research project: a working compiler for an
explicit checked TypeScript subset, a capability-checked Wasmtime runtime,
official browser applications and an experimental native shell. K1–K3 are
implemented for their declared profiles; K4 and platform work remain in
stabilization. Regression counts do not establish production readiness, full
ECMAScript/Node/npm compatibility or completion of the accepted plan.

The [functional overview](functional-overview.md) collects the implemented
language, runtime, system, network, storage, UI, GPU, packaging and tool surfaces,
their current limitations and the evidence behind this checkpoint.

| Item | Current state |
| --- | --- |
| Profiles | Default `checked-i32-utf8-v1`; newest implemented opt-in `checked-f64-utf16-v38`. V32–v38 are integrated in `main` and remain development profiles; their earlier branch names below identify historical measurements, not outstanding merges. Registry-derived versions: [version matrix](architecture.md#version-matrix) |
| Language and scheduling | Classes/generics, module exports, collections, checked JSON, suspension frames, generators, Promise jobs, async/await, async generators, timers and async HTTP(S) client requests are implemented within explicit admission rules |
| Application runtime | V38 long-lived instances, host event turns, per-turn budgets, permission broker, virtual file roots, KV storage, incremental and idle collection, L0 headless view output and TSX are implemented. Wave 1 adds the v38 long-lived network contract (inactivity window, per-read bound and delivery cap, host-owned `User-Agent`), bare package specifiers, the official browser host's clock/entropy/environment/exit effects with idle collection, and the native `varta-shell` for packaged artifacts with manifest grants and a host ceiling ([wave 1 integration](#wave-1-integration--2026-11-04)). Native display targets and full browser acceptance remain open; generated browser view sessions now work within their checked host surface |
| Last recorded successful full gate | Merged `main` with the K9 C expression and function forms (integrated with K9 A steps 1–2 and K9 B), K9 B declarations and classes, the K8 CSS engine's font-relative units, the first K9 D standard library stream, the Tailwind compiler, native `img`, the iteration protocol, overlay scrollbars and the audit fixes, macOS arm64, 2026-11-06, gated on the K9 C integration commit 1414396 whose sources equal merged `main` except the gate records: **4379 Rust tests passed, 0 failures, 25 ignored; 185 browser Node tests passed**; conformance baseline matched; sources unchanged during the run. Report: `target/verification/20261106-213007-y3wpjbwk/report.json` in that worktree. Linux CI and other platforms not run |
| Current GC / verification state | Incremental clearing bounds bitmap initialization before the root snapshot; the new large-heap maintenance test and the codegen-v2 fuel comparison pass in the consolidated gate. Current code/resource inputs match those checked during the run. The previous failed attempt is retained as history. The updated Chrome streaming acceptance has not been run. |
| Compatibility inventory | ECMAScript 2025, checked profile v38: **191 passed, 5 known unsupported out of 196** after the K9 A, B, C, D and D4 (`date-local-time`) cases, module evaluation, enum and namespace objects, enum assignability and class index signatures (2026-11-06), baseline matched ([conformance README](../conformance/README.md)). This is a partial inventory, not a core-completion percentage |
| HTTP server | V38 HTTP v2 streams Web `Response` text/bytes/`ReadableStream` bodies; the modern-server stages H1–H4 are merged ([record](#k7-merged-values-approved-2026-11-06)): Web `Request` ingress with streamed request bodies, async Wasmtime with pooled `InstancePre` and task/memory admission, native TLS 1.3 with ALPN `h2`/`http/1.1` and HTTP/2 with stream/header/reset bounds, fuzz/flood/slow-client tests and local benchmarks (~44k req/s, p99 ~1.4 ms on macOS arm64). An HTTP/2 connection stays open while a response body still streams ([fix](#review-fixes--2026-11-06)). The separate v38 network-read inactivity window is implemented (`6dfb06e`); its updated Chrome streaming acceptance has not been run. Open: a per-stream HTTP/2 write-idle bound, h2c, HTTP/3, certificate reload, Linux and sustained-load benchmarks. |
| Pilot | Pinned uarp-sdk **0.13.2**: recorded checks cover 65/65 source files and exported generics; real native `UarpClient` loopback acceptance covers 43 cases in 6 groups. The console has 249 tests in 36 groups and 10 headless view checks. Its six SDK groups replay in Chrome with native-equal results/accounting, and browser settings persist through OPFS and reload. These are separate pilot-specific checkpoints; the latest full gate does not rerun all live-browser/SDK acceptance. SDK-own-test execution, sustained live-service sessions and full cross-target parity remain open. |
| JS host | Official metered browser application/view bundles include scheduler/network/effects, packaged resources, permission prompts, OPFS KV storage, network replay and dev/reload. Native-recorded SDK replay and settings persistence have local Chrome evidence. The browser oracle driver and CI definitions are integrated; their existence does not prove current remote CI or a complete Chromium/Firefox/WebKit matrix. The older unmetered prototype remains a separate compatibility surface. |
| Native renderer | Integrated on `main`: retained CPU rendering/damage, inline edges/borders/decorations, positioned painting/z-index, preserved/mixed white space, user-agent control defaults and number stepping, modal dialogs, native consent, accessibility geometry and macOS adapters. `varta-shell --system-fonts` adds macOS font mappings, optical size/bold synthesis, color/opacity transitions and translated/animated boxes. The console first frame and every button's hover/pressed states render headlessly through the shell path with system fonts. Real-window console sessions, complete IME/accessibility/clipboard/link gestures remain open; see [native controls](#native-control-defaults--wave-3-package-w3-native-controls-scoped-2026-11-05) and [transitions](architecture.md#native-css-transitions). Native backlog batch (2026-11-06, merged): platform fallback faces with `--system-fonts` (⚙ draws in Menlo as in Chrome; CoreText on macOS, fontconfig on Linux, which also maps the generic families there; [record](#native-fallback-faces-and-linux-system-fonts--native-backlog-batch-2026-11-06)), `transform: scale()` with transitions ([record](#native-css-scale--native-backlog-batch-2026-11-06)) and classic macOS scrollbars ([record](#native-classic-scrollbars--native-backlog-batch-2026-11-06)); colour emoji on macOS as Chrome draws them ([record](#native-colour-emoji--2026-11-06)). Text and colour emoji under a uniform `scale()` now draw through CoreText at the scaled size ([record](#native-text-under-scale--2026-11-06)). macOS overlay scrollbars are shown while scrolling, on the animation clock ([record](#native-overlay-scrollbars--2026-11-06)). Colour glyphs on Linux, scrollbar dragging and track clicks, and every Linux comparison with Chrome remain open. Packaged images (`img`) are laid out and drawn natively to the HTML, CSS and image-format standards, checked against Chrome ([record](#native-images--2026-11-06)). |
| Native white space | `w2-native-whitespace` is merged (`316fc62`): preserved `pre-wrap`/`pre` tab stops, hanging spaces and mixed white-space modes in one flow have scoped Chrome evidence at three DPIs. Bidi on that path, ellipsis combined with it and a configurable `tab-size` remain open ([record](#native-preserved-white-space--wave-2-package-w2-native-whitespace-scoped-2026-11-04)). |
| Showcase and browser comparison | Typed `main.tsx`, reactive bindings, external `styles.css` and explicit `.ts` imports; local macOS Debug/Release bundles plus a newer Dev bundle. Twelve trusted-input assertions pass for the CSS/TSX dev artifact. Chrome interaction and native result comparison pass for the sampled scenarios; standalone root sizing/scrolling and admitted control box sizing are normalized. Native appearance and accessibility gaps remain. The Showcase was removed on 2026-11-06 (owner decision); its measurements are kept in [`docs/evidence/`](evidence/). |
| GPU and games | Optional wgpu/Vello UI backend and shared bounded scenes are implemented; `w3-gpu-color-fields` is merged (`ff45daa`), so distinct border colors can stay on GPU. Recorded Metal Debug/Release probes cover that implementation; direct GPU opacity groups, real-window/performance acceptance and other devices remain open. P13b guest graphics and P13c typed 3D ports remain planned; no guest GPU API or unmodified Three.js support is claimed. [Scope](#gpu-and-game-graphics-future-direction) |
| Review batch | Four reproduced defects fixed in `abea3ac`: compiler stack exhaustion, JS table limits, explicit thenable rejection and fractional timer ordering |
| Completion | [CORE ledger](#core-completion-ledger) records implemented scope and remaining acceptance separately. Whole work streams are not closed by passing regression tests |

### K9 B — declarations and classes — 2026-11-06

Owner decisions for the first release (2026-11-06): the whole TypeScript
language except untyped code, to the published standards, with Node 24 as the
runtime oracle and tsc 5.x as the type-system reference. Track B covers
TypeScript's declaration forms and the remaining class forms, under the v38
selector `declaration_forms()` ([contract](architecture.md#declarations-and-classes-declaration_forms),
[rules](profile-rules.md#declarations-and-classes-declaration_forms)). No host
ABI, value layout, frame catalog or scheduler change; v37 keeps rejecting
every form.

Admitted: numeric, string, heterogeneous and `const` enums (constant member
expressions, literal-union enum types, reverse mapping, widening of mutable
bindings to the enum type); namespaces (nested, dotted, exported across
modules and import aliases) with declaration merging into namespaces,
functions, classes and enums; abstract classes, methods, getters and fields;
function, method, static and constructor overloads; setters, getter/setter
pairs and `accessor` fields; static fields as module variables with
`private`/`protected`/`readonly` checks, `this` in static methods resolved
through the called subclass; class expressions bound by `const` and classes
declared inside functions; parameter-property defaults; `declare`,
`override` and multiple `implements` (already admitted, now pinned).
Enums, namespaces, statics, class expressions and overload signatures are
lowered before scanning (`crate::desugar`); class forms are a pre-layout
rewrite (`classes::forms`), setter assignments go through
`classes::accessors` and overload resolution through `lower::overloads`.

Probe cases of the K9 brief (`k9/cases`, `cases2`): `a19_enum`,
`a20_const_enum`, `a21_string_enum`, `a22_namespace`, `a23_abstract`,
`a26_getset`, `a27_static`, `a28_overloads` and `a69_class_expression` now
compile and print Node's output (with `a24_implements`, `a25_private`,
`b8_static_method` and `b9_getter_only`, which already did); `a42_decorator`
stays refused (below). The whole brief corpus goes from 43 to 53 compiling
programs of 89.

Evidence: `varta-runtime` `declaration_form_tests` (nine Node-pinned
programs, local classes, the multi-module test, collector stress builds, 32 refusals with
TypeScript's error codes where tsc rejects too, and the v37 rejections), the
CLI smoke `v38_declaration_forms_run_in_dev_and_artifacts` (dev, both
artifacts, both binaries) and the conformance inventory: eight feature rows
under the `typescript_handbook` origin and ECMAScript class sections, nine
passing cases and two known gaps, and the `class-instance` gap promoted
(145 passed, 6 known unsupported of 151). Fuel goldens unchanged (no
existing program changes bytes). Full gate on macOS arm64, 2026-11-06, on
the commit that adds the stream: **4023 Rust tests passed, 0 failures, 21
ignored; 218 browser Node tests passed**; conformance baseline matched
(`target/verification/20261106-163846-0mggblrz/report.json`). Linux CI and
other platforms not run.

Owner decisions of 2026-11-06 (whole language; module evaluation per
ECMAScript; match tsc for enum assignability; close the smaller gaps)
were followed by a second increment: module evaluation with effects
(selector `module_evaluation()`, [contract](architecture.md#module-evaluation-module_evaluation)):
initializers that call code, static blocks and effectful static
initializers at the class's position, with a compile-time
temporal-dead-zone check over the module's reachable code; `super` in
static code; re-exports of enums, namespaces and classes with statics
(generated names tagged by module path); static fields of classes in
namespaces; writes of imported static fields through exported setters. The
conformance `static-block` gap passes. Then: non-const enums and namespaces
as values through generated objects (`Object.keys(E)`, `typeof E`, `keyof
typeof E`), tsc 5 enum assignability (`number` to numeric enums, nominal
string enums, `E[k]` kept `string | undefined` by owner decision), and
class index signatures (a hidden dictionary field, TS2411 checks).

Still open, with the reason:

- **Standard decorators.** A decorator receives the class (a class value)
  or a member's function value with a context object
  (`ClassMethodDecoratorContext` and its family, `addInitializer`,
  `metadata`) and may replace it. Classes are not values yet, and a
  replaced method must dispatch through a per-class function slot whose
  initial value reaches the class's private members from module code.
  Proposed design: class values as a compile-time-known class identity
  (an `i32` descriptor) with `new`, static calls and `name` dispatched over
  the closed set of subclasses; decorated methods as private originals plus
  a module-variable function slot initialized by the decorator call during
  module evaluation, with access checks admitting code lexically inside the
  class; the context objects as a typed prelude. No legacy
  `experimentalDecorators`.
- **Generic overload signatures**: selecting one needs speculative type
  argument inference per candidate signature.
- **A `this`-using static method inherited across modules**: the copy into
  the subclass needs the declaring module's private names; with class
  values it becomes a dispatch over the receiver class.

Earlier state, superseded by the increment above where it says so:

- **Static blocks, effectful static initializers and decorators.** All three
  run code when a class is defined. Module initializers are a pure,
  call-free subset by rule ("admitting calls … needs a new evaluation-order
  design and initialization on tool/HTTP entries first"); class-definition
  effects need that design (ordered initializer functions at each class's
  position, their budget and their re-run per HTTP request store or tool
  invocation). Decorators additionally need class objects as values
  (class decorators receive and may replace the class) and the
  `ClassMethodDecoratorContext` family of prelude types. Refused by name.
  Legacy `experimentalDecorators` are not proposed.
- **Enum and namespace objects as values** (`Object.keys(E)`, passing `E`,
  `keyof typeof E`): need a frozen record value per enum and track A's
  `typeof`/`keyof` type queries.
- **Class index signatures** (`[key: string]: T` in a class): class
  instances would need dictionary storage beside their slots.
- **Smaller gaps**: a re-export (`export { E } from`) forwards an enum's
  type but not its generated declarations; static fields of a class inside a
  namespace; writes to imported static fields (imported module variables are
  read-only); `super` in static methods; a `this`-using static method
  inherited across modules; generic overload signatures; `E[k]` is typed
  `string | undefined` where tsc says `string` (kept by owner decision as the
  `noUncheckedIndexedAccess` reading); enum assignability (closed: tsc 5
  rules since the second increment).

### Native images — 2026-11-06

Applications that show images now draw them natively, not only in the
browser bundle, to the published standards with Chrome as the oracle
([contract and conformance table](architecture.md#native-images-img)). The
first increment admitted static sRGB PNG only; the owner directive of the
same day asked for the standards instead of a PNG subset, which this record
covers. A configuration declares its image files in a top-level `images`
map (view name to project path); packaging decodes them with the native
decoder and puts them into the artifact's resource section, version 2 when
there are images (a fonts-only section keeps its version 1 bytes;
[format](architecture.md#packaged-images-section-version-2)), and the browser
bundle carries the same files under the same names with their media types,
so both renderers admit the same images. `images` and `browser.resources`
cannot be declared together.

- **Formats and colour.** PNG/APNG, JPEG, GIF, WebP, BMP and ICO, told apart
  by signature (MIME Sniffing §6.1), decoded by the image-rs decoders with
  limits; converted to sRGB with `moxcms` from `cICP`, `iCCP`, `sRGB`,
  `cHRM`/`gAMA` (PNG 3rd edition priority) or an embedded ICC profile; EXIF
  orientation with `image-orientation`. JPEG converts after sampling, as
  Skia draws it (converting at decode differed by about 5/255 when
  upscaled). AVIF (no safe-Rust decoder), SVG (no SVG renderer) and HDR or
  narrow-range `cICP` are refused with their own reasons.
- **HTML.** `srcset`/`sizes` with Chromium's density selection and the
  density-corrected natural size; `width`/`height` attributes as
  presentational hints and the `aspect-ratio` hint; an `img` without an
  image represents nothing, and one that would show alternative text or a
  sized broken-image box is refused (user agent rendering); `decoding` and
  `loading` change nothing for local images. Animations play on the view's
  animation clock with Blink's 100 ms short-frame rule and each format's
  loop count.
- **CSS.** `object-fit`, `object-position`, `image-rendering` (`pixelated`
  and `crisp-edges` as CSS Images 3 §5.2 defines them), `aspect-ratio`,
  replaced sizing (CSS 2.1 §10.3.2, §10.4, §10.6.2).
- **Images in lines of text.** An inline image beside text is an atomic
  inline of the inline formatting context: a Parley `InFlow` box with a
  soft wrap opportunity on both sides (CSS Text 3 §5.1) that keeps the
  decoration edges next to it (a recorded Parley patch; `VARTA-PATCH.diff`
  regenerated and checked to reproduce the vendored tree from the release),
  and every `vertical-align` value with Chrome's shifts in layout units,
  `top`/`bottom` growing the line box as aligned subtrees. Images alone on a
  line keep their line box. Percentage sizes of images in text, positioned
  or transformed images there, images under an inline box with opacity and
  images in ellipsis or native white-space flows are refused.
- **Paint.** A software-only scene primitive (`Primitive::Image`) samples as
  Skia does: bilinear, and below half size linear between two `SkMipmap`
  levels at level of detail `log2(1/s) - 0.5`; the destination snaps in
  Blink's 1/64 layout units to whole paint pixels before DPI, a rounded
  image takes its edge coverage from its content-box clip alone, and a clip
  that cuts an image masks only where it cuts (a viewport clip had doubled
  the image's own edge coverage). Frames with images take the explicit
  software path (`PreparedFrame::Raster`) on the GPU backend.
- **Accessibility.** `alt` names the image; an empty `alt` without a label
  is presentational, as Chrome exposes it.
- **Dependencies.** `png` 0.18.1, `image` 0.25.10 (PNG, JPEG, GIF, WebP,
  BMP, ICO features only) and `moxcms` 0.8.1 (no default features), all safe
  Rust with exact pins; `flate2` is held at 1.1.10 in the lockfile.

Verification on macOS 27.0.1 with Chrome 156.0.8078.80: `native_image_probe`
with `scripts/test-native-images.mjs`, 22 cases and 22482 checks per scale
(geometry, hits, roles and names, frame durations and loop counts with
Chrome's `ImageDecoder`, pixels of the viewport and of every image's box),
pass at 1x, 1.5x, 2x and 3x; viewport RGB means at most 0.821, 0.904, 0.847
and 0.742/255 and per-image means at most 1.095, 1.404, 1.472 and 1.000/255.
The oracle shows a later animation frame by giving the same `img` a lossless
PNG of the frame Chrome's decoder produced (a canvas in its place had been
composited differently at 1.5x). Unit tests cover sniffing, the section
encoding, selection, sizes, colour sources and refusals, orientation
mapping, `pixelated` and nearest sampling, animation timing and loop counts,
object sizes, the constraint table, atomic-inline breaking with decoration
edges, every `vertical-align` and the refusals; `varta-desktop` tests decode,
lay out, draw, hit and name a packaged image through the shell core and
refuse a corrupt image in `varta-shell` before storage; a CLI smoke test
packages an image into both distributions.

**Owner decision (2026-11-06).** The image limits in
`varta_common::view::images` are the extended set: 256 images, 32 MiB per
file and 128 MiB in all, 16384 px per dimension (a common GPU texture
maximum), 4096 frames per image, 256 MiB of decoded RGBA in all (every frame
counted) and 512 MiB of retained pixels with their levels; a host may only
lower them. The older `browser.resources` inventory uses the same constants.
New dependencies, all safe Rust with exact pins, are accepted: `png` 0.18.1,
`image` 0.25.10 (PNG, JPEG, GIF, WebP, BMP and ICO only) and `moxcms` 0.8.1.
Known before this batch and unrelated to it: `native_inline_probe`'s
`paint-overlap-hits` case fails its Chrome pixel bound (RGB mean 5.39) on
`81e26b9` and later; open.

**Open.** AVIF and SVG images; alternative-text and broken-image rendering;
HDR images; Chrome draws a downscaled `pixelated` image by nearest neighbour
and drops `crisp-edges`, so the oracle compares those cases with Chrome's
values; Chromium's cached-candidate `srcset` heuristic; `varta dev` shows no
packaged images; images on Linux are not compared with Chrome; the GPU
backend presents image frames through the software path.

### Language gap ranking and the v38 iteration protocol — 2026-11-06

Ranking of the remaining language gaps of the [functional
overview](functional-overview.md) for typed npm packages, measured against
the pilot and the inventory:

- **Pilot.** The pinned uarp-sdk 0.13.2 sources (65 files) already check; of
  the candidates they use only `[Symbol.asyncIterator]` (`core/sse.ts`,
  admitted since v37), `Date.now`/`Date.parse` (UTC subset) and four
  linear-time regular expressions; no top-level `await`, `Symbol()`, `then`
  members, local-time `Date` methods or wrapper objects. The three pinned SDK
  own tests (`pins/uarp-sdk-tests.json`, fetched at the pinned commit with
  their recorded sha256, scratch only) use none of these either: they stop at
  `node:` modules and `import.meta.url` (M7), with one `Date.parse` and
  regex literals in assertions.
- **Inventory.** Of the five known gaps one is a candidate (`symbol-type`,
  `Symbol()` values); the others are local classes, property descriptors,
  generator return inference and union-to-record arguments.
- **Typed npm code.** The iteration protocol is the most used: custom
  collections, trees, token streams and paginators implement
  `[Symbol.iterator]()` with `yield`/`yield*`, and array spreads
  (`[...a, ...b]`, `[...set]`, `[...iterable]`) are among the most common
  expressions of TypeScript code, yet v38 refused every array literal spread
  except `[...collection]`, synchronous generator methods and `yield*`.
  `Symbol()` values follow (brands, private keys), but they need a value tag
  and symbol-keyed properties. Library code avoids top-level `await` (it
  blocks every importer); structural thenables appear mostly as
  `PromiseLike<T>` parameters; local-time `Date`, wider RegExp and wrapper
  objects (`new String`, `Object(x)`) are rarer in libraries and each needs a
  policy decision first.

Implemented in order: the [iteration protocol](architecture.md#iteration-protocol-iteration_protocol)
(selector `iteration_protocol()`, no host ABI, value layout or scheduler
change, so the browser host runs it unchanged). Behaviour is pinned to Node
24.20.0 (`varta-runtime` `iteration_protocol_tests`, collector stress builds
included), the CLI smoke runs it through `varta dev`, both artifacts and both
binaries, and the inventory gains the `iteration-protocol` row with three
passing cases (135 passed, 5 known unsupported of 140). Existing fuel goldens
are unchanged. Full gate on macOS arm64, 2026-11-06, before the commit that
records it: **3951 Rust tests passed, 0 failures, 21 ignored; 218 browser Node
tests passed**; conformance baseline matched; sources unchanged during the
run (`target/verification/20261106-103548-wia1snhj/report.json`). Linux CI and
other platforms not run.

Not done, with the reason:

- **`Symbol()` values and `description`** (inventory `symbol-type`): a symbol
  needs a value tag in value layout 18 (`typeof`, unions, `unknown`, logging,
  equality, JSON skipping) and readers in every host; symbol-keyed
  properties, their main use, need dynamic property keys. A value-layout
  change needs an owner decision.
- **Top-level `await`**: module initializers run synchronously before
  `main`; awaiting in them makes module evaluation asynchronous (async
  initializer frames, initialization order across the graph, scheduler turns
  before `main` in the native and browser hosts). Rare in library code.
- **Structural thenables**: adopting an object with a typed `then` needs
  the NewPromiseResolveThenableJob job in both schedulers (browser parity)
  and checked callback typing; explicit rejection (`abea3ac`) stays.
- **Local-time `Date`**: the time zone is ambient host state; reading it
  needs a host policy (a grant or a fixed zone) decided by the owner.
- **Wider RegExp** (backreferences, lookaround, `\p{…}`): the matcher is
  linear-time by contract; these need a new matcher contract.
- **Object wrappers and object-to-primitive coercion**: rare in typed
  library code; they need `valueOf`/`toString` dispatch on objects.

### K9 D standard library — 2026-11-06

Owner decision (K9, 2026-11-06): the first release covers the whole
TypeScript language except `any`, to the published standards with Node 24 as
the runtime oracle; track D is the ECMAScript 2025 standard library,
prioritized by use in typed npm code. This record covers what the track
implemented in the selector `standard_library()`
([contract](architecture.md#standard-library-standard_library),
[rules](profile-rules.md#standard-library-standard_library)); no host ABI,
value layout, scheduler or browser-host change, so the official browser host
runs the same Wasm.

Implemented (priority 1 and the UTC part of priority 4):

- `sort`/`toSorted` with V8's TimSort, so comparator call sequences, counts
  (pinned for 0 to 1000 elements) and the order an inconsistent comparator
  leaves equal Node's; `toReversed`, `with`, `toSpliced`, `splice`,
  `copyWithin`, `concat`, `shift`, `unshift`, `flat` (literal depths),
  `flatMap`, `keys`/`values`/`entries`, `Array.of`, mapped `Array.from`
  and `Array.from({ length })`.
- ES2025 iterator helpers on generators and built-in iterators and
  `Iterator.from`; ES2025 `Set` methods; `Map.groupBy`, `Object.groupBy`,
  `Object.fromEntries`, `Object.getOwnPropertyNames` of records and
  dictionaries, `hasOwnProperty`.
- `JSON.stringify` property lists and `space`; `AggregateError`; combinator
  arrays mixing `Promise.reject`; `Math.max`/`min` spreads; `String`
  `at`/`substr`/`isWellFormed`/`toWellFormed`/locale-free case mapping.
- The complete `Date` with local time in the UTC zone (`Date.UTC`,
  component constructors, local/UTC getters and setters, `toString` and the
  other string forms, `toJSON`, `Date.parse` of the forms they produce).

Evidence: `varta-runtime` `standard_library_tests` (eleven Node 24.20.0
programs with `TZ=UTC`, collector stress builds, refusals, v37 denials), the
CLI smoke `v38_standard_library_runs_in_dev_and_artifacts`, and eleven
conformance cases in five new feature rows and four existing ones (146
passed, 5 known unsupported of 151). Existing fuel goldens are unchanged.
Full gate on macOS arm64, 2026-11-06, before the commit that records it:
**4024 Rust tests passed, 0 failures, 21 ignored; 218 browser Node tests
passed**; conformance baseline matched; sources unchanged during the run
(`target/verification/20261106-165316-gtq53y0k/report.json`). Linux CI and
other platforms not run.

Not done, each refused by name with its reason:

- **`Symbol()` values, `Symbol.for`/`keyFor`, symbol-keyed properties and
  the remaining well-known symbols**: a new value tag in value layout 18
  (extended in place, v38 is unreleased) read by every value reader in both
  hosts, plus symbol-keyed members. Needs the owner's value-layout decision.
- **`WeakMap`, `WeakSet`, `WeakRef`, `FinalizationRegistry`**: weak edges
  (ephemerons) in the exact-root collector and deterministic cleanup turns
  in both schedulers. A strong-reference stand-in would leak and is not
  admitted.
- **Local time zones**: done by K9 D4 (below): the owner decided that the
  host zone needs no grant; it is a recorded host input resolved by TZif
  and ICU4X data in guest code.
- **Full RegExp** (backreferences, lookaround, `\p{…}`, `d`/`v`, modifiers):
  a backtracking matcher bounded by fuel would replace the linear-time
  contract of v37; needs a new matcher contract.
- **Proxy, Reflect, descriptors and prototypes**: closed checked values have
  no exotic objects or mutable prototypes.
- **`Object.freeze`/`seal`/`preventExtensions`**: a per-object flag checked
  by every write (value layout 18) — owner decision.
- **`structuredClone`, `JSON.parse` revivers, `JSON.stringify` replacer
  functions**: need a type-directed deep copy and guest-driven
  (de)serialization walks.
- **Intl** (NumberFormat, DateTimeFormat, PluralRules, Collator,
  RelativeTimeFormat, ListFormat, Segmenter) and `localeCompare`/`normalize`:
  need ICU4X locale data shipped deterministically (size and crate pins to
  be justified).
- `Promise.any` still rejects with an `Error`, not an `AggregateError`;
  object-to-primitive conversion (`String(date)`, `` `${array}` ``) stays
  refused.

### K9 D4 local time — 2026-11-06

Track K9 D4 (local-time `Date`, then `Intl`), first batch: `Date` in the
host's time zone, to ECMA-262 §21.4 with Node 24.20.0 as the oracle, under
the selector `local_time()`
([contract](architecture.md#local-time-local_time),
[rules](profile-rules.md#local-time-local_time)). Owner decisions applied:
local time always uses the host's zone without a grant, recorded as a host
input for replay; locale data through ICU4X with deterministic data.

Implemented:

- Two capability-free host inputs, `intl_time_zone` and `intl_locale`
  (registry, lowering, runtime handler, `varta.d.ts`, generated browser
  registry, Worker forwarding of the browser's resolved zone and locale,
  documentation tables). Embedders pass `RunOptions::host_inputs` (default
  UTC/`en-US`); `RunReport::host_inputs` records them; the native hosts read
  `TZ`//etc/localtime and `LC_ALL`/`LC_MESSAGES`/`LANG` as ICU does, and
  `varta run`/`dev` take `--time-zone`/`--locale`.
- The engine crate `varta-intl` (`no_std`, no `unsafe`): TZif v2–4 and POSIX
  TZ footer evaluation (`zone`, `posix`), ECMA-262 LocalTime/UTC with the
  earliest-instant and offset-before-the-gap rules, and the localized zone
  name of `toString` (`names`) over ICU4X, with V8's equivalent-year rule for
  the daylight flag outside 1970–2038. Linked into guest Wasm as
  `date_zone_offset` and `date_zone_name`; the date prelude calls them.
- The intrinsic linker links indirect calls: address-taken library
  functions get slots appended to the guest's table, every table index
  relocation (including position-independent `__table_base`-relative ones)
  is rewritten, and the runtime's effect analysis counts an indirect call as
  a call of every table function. ICU4X (through `core::fmt`) needs it; every
  later `Intl` service does too.
- Local `Date.parse` of date-time forms without an offset (§21.4.3.2).

New dependencies (exact pins, `[workspace.dependencies]`): `jiff-tzdb`
=0.1.9 (the IANA tzdata 2026e as TZif, data only, `no_std`, no
dependencies; the only maintained bundled TZif crate, and in ICU's rearguard
form, which matches V8's daylight flags), `icu_datetime` =2.4.0 and
`icu_time` =2.4.0 with compiled data (zone names, metazones; same ICU4X 2.4
line as `icu_properties`), `icu_locale_core` =2.4.0, `icu_provider` =2.4.0
and `writeable` =0.6.4 (already locked through `icu_properties`/`parley`).
ICU4X's other data crates (`icu_datetime_data`, `icu_time_data`,
`icu_calendar` and its data, `icu_decimal`, `icu_plurals`, `icu_pattern`,
`fixed_decimal`, `calendrical_calculations`) come with them. They are built
only into the wasm32 intrinsic library and `varta-intl`'s own tests.

Evidence: `varta-intl` tests (offsets of 30 zones at 4872 instants and
18144 local times, and of all 436 zones Node lists at 228 instants, against
Node; POSIX rule forms; 2349 zone names in nine locales against Node with
every difference pinned by cause), `varta-runtime` `local_time_tests` (one
program, ten zone/locale pairs, Node 24.20.0 output; defaults and recorded
inputs; unknown zones; fractional offsets; malformed inputs; imports; one
read per instance; v37 unchanged), the linker test
`indirect_calls_of_linked_code_get_appended_table_slots`, the conformance
case `date-local-time` (run with `hostInputs`).

Sizes (macOS arm64): the checked-in intrinsic library grows from 519 KB to
3.6 MB. A v38 program that uses `Date` links the tzdata, about 225 KB of
data (333 KB module for a one-line program); one that calls
`toString`/`toTimeString` also links ICU4X's zone names of every CLDR
locale, about 2.5 MB (2.85 MB module).

Differences from Node 24.20.0 that need the owner (not documented as
accepted deviations yet):

1. **CLDR version.** ICU4X 2.4 ships CLDR 49; Node 24.20.0 ships CLDR 48
   (ICU 78.3). Zone names differ where CLDR 49 changed them (17% of the
   2349 pinned name samples, mostly outside English; in `en-US` Honolulu,
   Apia, Casablanca and two historic summer names). ICU4X 2.3 has CLDR
   48.2.1 but cannot share a lockfile with `icu_segmenter` 2.4 that the
   vendored Parley uses. Options: accept CLDR 49 (newer than Node) and pin
   its text, or generate Varta's own ICU4X data from CLDR 48 JSON
   (`icu_provider_export`, checked-in baked data).
2. **`getTimezoneOffset` with seconds.** The specification gives a fraction
   for local mean time offsets (296.0333… for New York before 1883); V8
   truncates to 296. Varta follows the specification.
3. **tzdata 2026e vs Node's 2026c**: Manitoba and Inuvik differ from 2027.

Data selection for `Intl` (proposal; nothing shipped as a subset). Measured
wasm32 data of ICU4X 2.4's compiled data for every locale, each reached by
runtime-chosen options: `DateTimeFormat` with all calendars and zone names
3.8 MB, `DisplayNames` (languages, regions, scripts, locales) 3.3 MB,
`Segmenter` with LSTM models 2.4 MB (the dictionary models ICU4C uses are
larger), zone names alone 2.4 MB, `Collator` 1.1 MB, `RelativeTimeFormat`
0.37 MB, currency names about 2.5 MB and unit names about 0.6 MB (estimated
from the data sources), `ListFormat` 47 KB, compact decimals 43 KB,
`PluralRules` 13 KB, decimal symbols 10 KB, locale canonicalization 87 KB,
tzdata 225 KB: about 17 MB for the full surface. Linked data sits in each
instance's linear memory and counts toward its memory budget (an HTTP
request keeps 8 MiB), and the checked-in intrinsic library would carry it
all. Proposal:

- **A (no subset).** Keep every locale, but move locale data out of
  baked statics into one versioned data blob per service that the linker
  appends as *passive* segments, one per (data marker, locale); the guest
  copies only the locales it uses into its arena on first use (`memory.init`
  through a synthesized accessor). Artifact size grows with the services a
  program uses (up to about 17 MB), memory only with the locales it touches.
- **B (selected locales).** ECMA-402 lets an implementation choose its
  available locales (`[[AvailableLocales]]`). A project lists them
  (`"locales": ["en", "uk"]` in `varta.config.json`; default: a small set the
  owner names, or all); `varta build` generates the data blob for exactly
  those (`icu_provider_export`), and the host's default locale resolves
  through CLDR fallback to the best available one. `supportedLocalesOf` and
  `resolvedOptions().locale` report the selection, which is conformant but
  differs from Node for locales outside it.
- Both keep the formatting code in the linked library and the data
  deterministic and versioned (CLDR, ICU4X and tzdata versions in the
  artifact). B with A's lazy segments gives the smallest artifacts and
  memory.

Not done in this batch: `Intl` (DateTimeFormat, NumberFormat, PluralRules,
Collator, RelativeTimeFormat, ListFormat, DisplayNames, Segmenter, Locale,
`getCanonicalLocales`, `supportedValuesOf`) and the `toLocale*` family,
pending the data-selection decision; the full gate of this batch.

### Native CSS `scale()` — native backlog batch, 2026-11-06

`transform` lists that scale are admitted
([contract](architecture.md#native-css-transforms)): the stacking context
paints into the recorded scene and its draws and hit regions are mapped
through the transform in device pixels (nested scales compose); the
ancestors' clip goes back through the inverse transform; text under a scale
used the portable outlines (now host glyphs under uniform scales, see
[text under scale](#native-text-under-scale--2026-11-06)); the public geometry maps the subtree as
`getBoundingClientRect` does. Transitions interpolate pairing transform lists
function by function (scale included); non-pairing lists jump (Chrome
interpolates their matrices; recorded in the contract). Three new
`native_positioned_probe` cases (`scale-button`, `scale-order`,
`scale-nested`) match Chrome at 1x, 1.25x and 2x (11801 checks per scale;
pixel means 0.00 to 0.95/255), and the transition oracle now also compares
`getBoundingClientRect` boxes on every sample, including a new `scale` case
(867 checks). Refused inside a scaled subtree: scroll containers and clips,
text fields, selects, a curved outer clip and inline captures. Found on the
way, not specific to scale: Blink paints an element's outline before its
level-zero stacking children, native after them.

### Native classic scrollbars — native backlog batch, 2026-11-06

Scrollbars are painted natively when the system shows them always
([contract](architecture.md#native-scrollbars)). `Environment::scrollbars`
carries the style; `varta-shell` reads `NSScroller.preferredScrollerStyle`
(through the already locked `objc2-app-kit`, a safe class method). With
classic scrollbars an overflowing `overflow: auto` axis reserves a 15px
gutter (Taffy's `scrollbar_width`, per axis, in up to three layout passes, so
a vertical gutter that makes content overflow horizontally adds the
horizontal bar as in Chrome), and the track, borders, corner and thumb are
painted from Chromium's `NativeThemeMac` constants and Blink's thumb geometry
in device pixels. The oracle runs Chrome with `-AppleShowScrollBars Always` in
its own argument domain (no system setting is touched; `browser-probe.mjs`
gains `macDefaults`), checks that Chrome's gutter is 15px, and compares seven
cases (vertical, horizontal, both with the corner, gutter-induced overflow,
borders and padding, the minimum thumb, content that fits): 10124 geometry and
hit checks per scale and pixel means 0.00 to 0.02/255 (1x), 0.06 to 0.48 and
1.13 for the bordered case (1.25x), 0.06 to 0.09 (2x). Open: thumb hover
color, thumb dragging and track clicks, a dark `color-scheme` (outside the
subset), rounded scroll containers with classic bars (refused), live changes
of the system setting (read at window start) and Chrome's Linux Aura
scrollbars (Linux keeps overlay behaviour). The overlay thumb shown while
scrolling followed the same day
([record](#native-overlay-scrollbars--2026-11-06)).

### Native overlay scrollbars — 2026-11-06

With macOS's default "Show scroll bars" setting (automatic, or when
scrolling) native scroll containers now show Chrome's overlay scrollbar while
they scroll ([contract](architecture.md#native-overlay-scrollbars)); classic
bars are unchanged.

- **Constants from Chromium.** `ui::OverlayScrollbarAnimatorMac`
  (`kFadeOutDelay` 500 ms, `kAnimationDuration` 250 ms, ease-in-out, integer
  thumb width), `MacScrollbarImplV2` (thumb box 12, expanded 16),
  `ScrollbarThemeMac` (overlay frame 16, minimum thumb 26, track 28 for a
  thumb), `NativeThemeMac` with `is_overlay` (thumb `0x80`, inset 2, track
  gradient and border colors for both schemes) and Blink's
  `RecalculateOverlayScrollbarColorScheme` (dark when the scroller's
  background lightness is at most one half).
- **Measured in Chrome 156 on this Mac** (CDP wheel, `scrollTop` and
  `mouseMoved`, screenshots every 30 to 60 ms): a programmatic scroll before
  the first paint also shows the thumb, so the positioned oracle's scrolled
  cases now match with the bar on both sides; the fade starts 500 to 520 ms
  after the scroll and ends by 750 to 800 ms; the pointer on the bar widens
  the thumb 7 to 11 pixels and fades the track in over about 250 ms, and the
  bar stays until 500 ms after the pointer leaves; a wheel under a resting
  pointer first shows the narrow thumb and widens it a frame later; the bar
  is not clipped by the scroller's rounded corners; with a horizontal
  overflow the vertical track stops 16 pixels short. Two places where Chrome
  differs from Blink's classic geometry: the thumb is computed in CSS pixels
  and drawn scaled (fractional edges at 1.25x), and an offset under one pixel
  leaves it at the start (the compositor's truncation).
- **Model.** The view keeps each shown bar's event times (`overlay_scrollbar`):
  every visual state is a function of them and the animation clock, so
  headless renders are deterministic. Every write of a scroll offset goes
  through `Tree::scroll_to`, which shows the bar when the offset changes. The
  bar paints in its container's outline phase and, while shown, adds a hit
  region for the container. `FrameMetadata::scrollbar_at` and
  `View::set_scrollbar_hover` carry the pointer; `varta-shell` feeds them from
  each pointer sample. `View::animating` stays true during the fade-out delay
  but `set_animation_time` reports no change then, so the shell advances its
  clock on its idle wake without presenting frames until the fade runs.
- **Evidence.** `native_overlay_scrollbar_probe` with
  `scripts/test-native-overlay-scrollbars.mjs` runs seven cases' steps (wheel,
  wheel under the resting pointer, programmatic scroll, bordered, dark,
  two-axis, rounded, minimum thumb) natively on the clock and in Chrome in
  real time, and compares content width, hits and pixels around the bar at
  each capture (shown, hovered, faded): 55 checks per scale pass at 1x, 1.25x
  and 2x; pixel means 0.00 to 0.34/255 (1x), 0.03 to 1.17 (1.25x, where the
  bordered and dark cases also carry the existing fractional box-edge
  differences outside the bar), 0.00 to 0.58 (2x). The classic and positioned
  probes still pass at 1x and 2x.
- **Open.** Dragging a thumb and clicking a track (neither style has them
  natively), horizontal overlay bars (native layout scrolls only
  vertically), textarea bars, Chrome's `kFadeInScrollbarWhenMouseWheelMayBegin`
  (a trackpad touch that has not scrolled yet shows the bar; not modelled),
  live changes of the system setting, and a real-window session (only the
  headless view and the shell's code path are exercised).

### Review fixes — 2026-11-06

An independent review of `54d318a` reproduced two defects; both are fixed
with regression tests.

- **HTTP/2 idle with a streaming body.** A request stopped counting as
  running once its handler returned the response head, while the body was
  still streaming; past the idle bound the connection sent `GOAWAY` and the
  next request on it failed (`connection was not ready`). The streamed body
  now keeps its request running until it ends (`connection::Streaming`); a
  client leaving mid-body is not counted as abandoned. The new test streams
  for twice the lowered idle bound over HTTP/2 and serves a second request
  on the connection; it fails without the fix. Under full-gate load that
  test also exposed a race: the idle timer only compared request counts
  across a sleep, so a body ending just before a check made the connection
  idle at once. The bound now measures quiet since the last request or body
  start or end (24 parallel runs: 4 failures before, none after).
- **Emoji fallback on long text.** Finding uncoloured emoji searched every
  emoji grapheme for each cluster, cutting emoji out of missing ranges
  searched every emoji for each range, each range's current family stack was
  searched among all overrides, fixed line heights shaped a primary-font
  sample per override, and each run searched all primary ranges. All are now
  one walk over sorted ranges or a logarithmic range map
  (`fallback::RangeMap`), with one sample per style. Parley also counted the
  characters from its item start for every font segment; it now counts from
  the previous segment ([patch record](../vendor/parley/VARTA-PATCH.md)).
  8000 emoji/letter pairs with a fallback face: 2.75 s to 0.59 s (Debug),
  204 ms to 32 ms (release), against 0.10 s and 3 ms without fallback, the
  rest being the shaping of 16000 font runs, which such text needs.
- **Status.** The HTTP status row above and the functional overview's
  server rows described the merged K7 stages as planned; they are current
  now.

### Native colour emoji — 2026-11-06

Owner decision 2026-11-06: native draws colour emoji as Chrome does, checked
against Chrome; packaged and `--font` sets stay deterministic (no system
fallback). With `varta-shell --system-fonts` on macOS emoji now draw in
colour instead of refusing the text
([contract](architecture.md#native-fallback-faces)).

- **Face and bytes.** Apple Color Emoji's `sbix` strikes are 183 MiB, past
  the accepted 32 MiB fallback face limit. The shaper gets the face's own
  tables with an empty `sbix` (about 1 MiB, `morx` ligatures intact), and the
  font set's CoreText rasterizer draws its glyphs with the platform face
  registered for exactly those bytes; without that face (another rasterizer)
  the strikeless face is declined and the portable path refuses it.
- **Drawing.** Colour glyphs are premultiplied RGBA images
  (`GlyphRasterMask::color`), painted as the existing bounded rectangles with
  their own colours and the text colour's alpha, placed at the whole pixel
  nearest the quarter-pixel position (Chrome's placement, measured over
  sixteenth-pixel offsets at 1x and 2x). Synthetic bold paints them as they
  are (Chrome's bold emoji are pixel-identical); synthetic italic stays
  refused. The portable outline path now refuses colour glyphs instead of
  painting them in the text colour.
- **Metrics.** CoreText enlarges small emoji (a quarter up to 16px, none from
  24px). Chrome's advance is the `hmtx` advance scaled by CoreText's ascent
  over the face's own, rounded (checked at 33 sizes from 5 to 30px), and its
  line metrics are CoreText's rounded ascent and descent. New Parley hook
  `Layout::adjust_glyph_advances` ([patch
  record](../vendor/parley/VARTA-PATCH.md)) applies the advances before line
  breaking; `FallbackFace::metrics` carries them.
- **Presentation.** Blink's emoji segmentation: emoji-presentation graphemes
  (Unicode TR51) drawn by a face without colour glyphs ask the host for its
  emoji face, which then comes before the CSS families (`©\u{FE0F}` in
  Helvetica draws Apple Color Emoji, `❤` without VS16 Zapf Dingbats, as in
  Chrome). New dependency edge: `icu_properties` 2.4.0 for `varta-view`
  (already compiled in through Parley; no new package).

Verification on macOS 27, Chrome 156: `native_fallback_probe` with
`scripts/test-native-inline.mjs` passes at 1x, 1.5x, 2x and 3x (121 checks
each); the five emoji lines match at most 0.13/255 mean, the ZWJ/flag line
exactly. Unit tests cover the mask format, colour spans, placement, text
alpha, portable refusal, emoji requests, platform metrics, Chrome's advances
at nine sizes and line heights at four, and colour drawing only with the
registered face.

**Not done.** Linux still refuses colour faces (Noto Color Emoji is `CBDT` or
`COLR`; a portable colour renderer and Chrome on Linux are needed). Colour
glyphs under a non-uniform scale are refused (closed for uniform scales by
[text under scale](#native-text-under-scale--2026-11-06)). `COLR` and `SVG `
faces on macOS stay declined.

### Native text under scale — 2026-11-06

Text under a uniform `transform: scale()` drew the portable outlines, and
colour emoji there refused the frame. The host glyph rasterizer (CoreText on
macOS) now draws such text at the scaled size where the transforms put it,
as Chrome rasterizes it, and the recorded glyph goes through the inverse
transform, which mapping undoes; non-uniform scales keep the outlines
([contract](architecture.md#native-css-transforms)). Three
`native_fallback_probe` cases (`sans-scaled`, `sans-emoji-scaled`,
`serif-emoji-bold-scaled`, under `scale(1.5)`) match Chrome's transformed
boxes and pixels at 1x, 1.5x, 2x and 3x (152 checks per scale; pixel means
0.04 to 0.52/255); a view test checks the scaled request size and that a
non-uniform scale keeps outlines.

### Native fallback faces and Linux system fonts — native backlog batch, 2026-11-06

Closes the console's `⚙` (U+2699) `.notdef`: with `varta-shell
--system-fonts`, a character none of an element's families covers takes the
face the platform's browser draws it with
([contract](architecture.md#native-fallback-faces)).

- **macOS.** CoreText's `CTFontCreateForStringWithLanguage` for the primary
  font, exactly Chrome's `PlatformFallbackFontForCharacter`. The default
  cascade list alone is not Chrome: for U+2699 it names the hidden
  `.Apple Symbols Fallback`, while CoreText and Chrome draw Menlo; for U+2192
  under Times it misses Times New Roman. The call needs one `unsafe` FFI
  declaration, so `core-text` 21.0.0 is vendored with a single safe wrapper
  ([patch record](../vendor/core-text/VARTA-PATCH.md)); the face is the
  platform font's own tables assembled as a standalone face (no file read).
  The unit test checks nine character/primary/weight cases against Chrome
  156's `CSS.getPlatformFontsForNode` (Menlo, Menlo-Bold, Lucida Grande and
  its bold, Times New Roman, Zapf Dingbats, Hiragino Sans W3/W6), and Apple
  Color Emoji is refused as a colour face.
- **Blink's segment rule.** Chrome falls back over all `.notdef` ranges of one
  shaping segment with the face of the first range's first character, so a
  star after a check mark under Times draws in Zapf Dingbats, not Songti SC;
  native does the same per style, in passes. Blink also splits segments at
  script runs, which native does not (recorded in the contract).
- **Line metrics.** Two Chrome rules the oracle exposed, both through one
  recorded Parley hook (`Layout::adjust_run_metrics`, [patch
  record](../vendor/parley/VARTA-PATCH.md)): a fallback face's run under a
  fixed `line-height` takes the primary font's ascent and descent (the line
  box stayed 24px in Chrome, 25 to 26 natively before), and Chrome on macOS
  enlarges the ascent of Times, Helvetica and Courier by 15% of their height
  (the `serif` and `sans-serif` spans were 2px too short and 3px too low
  before). Plain text with `line-height: normal` and mixed faces still uses
  Parley's largest line height (20 where Chrome has 22 for the fixtures);
  an inline formatting context matches.
- **Linux.** `--system-fonts` maps the generic families through fontconfig as
  Chrome on Linux does (Times New Roman, Arial and Monospace through Skia's
  acceptance rules, Blink's last resort otherwise, `system-ui` as the
  fontconfig match for `sans`) and falls back through `FcFontSort` for the
  character and locale. New dependency: `fontconfig` 0.11.0 (MIT, safe
  wrapper, `dlopen`, so no link-time fontconfig); its one new package is
  `yeslogic-fontconfig-sys` 6.0.1, the rest was already locked.

Verification on macOS 27, Chrome 156: `native_fallback_probe` with
`scripts/test-native-inline.mjs` passes at 1x (72 checks) and 2x (72 checks);
pixel means 0.17 to 0.35/255 (1x) and 0.10 to 0.18/255 (2x) for the
Helvetica, Times and Menlo lines. The three UI-font lines compare geometry
only: the UI font's `opsz` axis makes the CoreText rasterizer decline it
(pre-existing), and its glyphs differ from Chrome by about 2.7/255 mean with
or without fallback. View unit tests cover ranges, the segment rule, refusal,
limits, the primary-metrics rule and the enlarged ascent.

**Not verified on Linux.** No Linux machine with fontconfig and Chrome was
available. The fontconfig code was compiled and its tests run on macOS against
Homebrew fontconfig 2.x (`--features fontconfig`, with the library aliased to
the Linux soname through `DYLD_LIBRARY_PATH`; serif mapped to Times New Roman,
sans-serif to Arial, monospace to Andale Mono, system-ui to Verdana there),
and the crate type-checks for Linux in a Debian container without
fontconfig. Chrome on Linux was not compared: the generic mapping, the
acceptance rules and the fallback order follow Chromium's sources
(`SkFontConfigInterfaceDirect::matchFamilyName`,
`gfx::GetFallbackFontForChar`) and remain to be checked with a Linux Chrome
oracle. Owner decisions: the fallback limits (`MAX_FALLBACK_FACES`,
`MAX_FALLBACK_FACE_BYTES`, `MAX_FALLBACK_BYTES`) are new native-host limits,
and packaged and `--font` runs still keep `.notdef` (deterministic, but unlike
Chrome, which falls back to the system there too).

### Native border CPU follow-up — scoped 2026-11-04

Implemented after `54585e3f`, committed in `ed77739e`: opposing-front ownership stops
when exactly one contour has arrived; uniform blocks use certified arrival
intervals, and the existing contour mask restricts work to whole covered pixels.
The remaining Debug hotspot was identified with a repeated CPU prepare fixture:
owning array iterators dominated the per-pixel side scan. Indexed side traversal,
borrowed seam offsets/candidate references and explicit premultiplied channel
accumulation avoid that bookkeeping. The tiny ellipse predicate is inlined in
Debug too. Bounds checks, assertions, arithmetic, sample order, source/cache/GPU
ceilings and build profiles stay unchanged; no unsafe path is added.

Three alternating before/current runs on the same M1 Max compare retained
pre-commit binaries with current Debug/Release builds. Median CPU preparation for
the cold 2× scenes is:

| Scene | Debug before → current | Release before → current |
| --- | --- | --- |
| Mixed border | 29.602 → 10.432 ms | 2.336 → 2.359 ms |
| Oversized mixed corner | 18.755 → 7.212 ms | 1.372 → 1.340 ms |
| Opposing mixed corners | 66.357 → 12.866 ms | 7.517 → 2.798 ms |

Across the three mixed scenes at both scales and cold/changed states, current
median preparation spans 2.199–12.866 ms Debug and 0.365–2.798 ms Release. The
36 individual samples per profile span 2.193–13.021 ms and 0.354–2.976 ms,
respectively. Short ordinary Release cases are roughly unchanged in these three
samples; no uniform speedup is claimed. The measured Debug cases now fit the
16.7 ms frame interval, but these are host-owned offscreen fixtures, not
whole-application latency or general smoothness acceptance.

Final scoped checks pass 83 View library +265 integration cases, strict View
all-target Clippy and optional GPU example builds in both profiles. New geometry
checks cover close opposing arrivals, whole-block separation and translated
elliptical fronts against independent inset membership. All 80 images match the
previous binary byte-for-byte in each of three pairs per profile (480 total image
comparisons). Each probe run still has 60 actual GPU observations and 20 explicit
CPU fallbacks; groups and mixed fields have not acquired direct GPU support.
Evidence, exact source inputs, binaries and samples:
`target/paint-next-stage/border-speed-profiled/checkpoint.json`.

The earlier `border-speed/` and `border-speed-final/` checkpoints verify their
earlier inputs. A circular-root proposal in the latter showed no additional
measured gain and was removed. This follow-up has scoped acceptance only; the
3393-test gate verified the earlier `54585e3f` inputs; the 3473-test
[commit gate](#native-inline-painting-commit-gate--2026-11-04) covers this follow-up.
Continue rounded inline decoration/own opacity, direct GPU fields/groups and
application/platform/pilot/release acceptance. Do not close CORE-16/18 or the whole
plan from this fixture improvement.

### Native inline slice backgrounds — scoped 2026-11-04

Implemented after the CPU follow-up: native inline backgrounds now admit the
existing radius grammar with default slice decoration across wrapped and bidi
fragments. Radius normalization uses each fragment's dimensions, while its
background is clipped from a continuous visual-order strip. Shared f64 radius/arc
generation avoids losing a narrow tail after a very wide composite. Own font-box
hits follow the curve; independent descendant text hits and glyph ink remain
unclipped. Retained, stateless and neutral-vector consumers agree. Transparent
backgrounds and zero-opacity ancestors avoid contour work while retaining input
metadata. New contour reservations share the unchanged text paint allowance;
overlapping nested fragment backgrounds cannot renew that allowance per node.

Final scoped acceptance passes **85 View library +272 integration cases**, strict
View/Desktop all-target Clippy with GPU, Debug/Release GPU probes and **4344
Native/Chrome geometry, hit and pixel checks** over21 cases. Eight new browser
scenes cover wrapping, percentages/rem/asymmetric radii, bidi discontinuities,
smaller descendants and translucent nested backgrounds. Their foreground is
transparent to isolate decoration pixels; existing exact glyph samples remain.
ROI RGB means are0.3625–1.50625/255 under the unchanged2/255 bound; largest channel
delta40. Three deliberate bad-image/bad-size/legacy-ink inputs are correctly
rejected by the comparison harness.

The GPU probe now has21 scenes: each build records64 actual GPU observations and
20 explicit CPU fallbacks. The new rounded-inline scene supplies four actual GPU
observations per build; opacity/mixed fields remain CPU fallbacks. All80 older
images per build remain byte-identical to the CPU-optimization checkpoint. Exact
source/binary/image hashes and logs are under
`target/paint-next-stage/inline-next/final/checkpoint.json`. This is scoped evidence,
not a new full-workspace gate or whole-application/platform acceptance.

Chrome156 and WebKit26.5 independently agree on fragment-relative percentage
radii. WebKit differs on the tested own-font-box corner hit with a smaller text
descendant; native follows CSS curved-border hits and the sampled Chrome result.
Firefox could not be admitted to the diagnostic: the cached build times out in
its compositor, and a fresh matched build cannot access either temporary or
owned test profiles. No Firefox or full three-engine parity is claimed.

The next static-box context-placement follow-up addresses the reproduced
source-order overlap gap below. Own inline opacity remains open.
The Chrome diagnostic also confirms unchanged cross-span ligature geometry when
opacity changes. Preserve shared shaping/first-source glyph ownership and group
the whole inline subtree, rather than scaling each fragment or glyph. Inline
edges/borders/outlines and the wider platform/pilot/release plan remain open.

### Native static opacity context placement — scoped 2026-11-04

The reproduced red-opacity/blue-sibling discrepancy is fixed (committed in `ed77739e`).
An iterative source scheduler delays each nonunit-opacity box subtree until
ordinary content in its nearest root/opacity context has painted, then paints
peer contexts in source order. This also orders pointer hits: opacity0 keeps
its context priority. Contexts lift through nonisolating block/flex/grid ancestors
with their original clipping and remain atomic inside an opacity ancestor.
Layout and shaping are not repeated; source, scene-cache and compositor ceilings
remain unchanged. Context creation/removal may conservatively invalidate input
metadata reuse, even for disjoint boxes.

The package adds independent overlap, nested curved-clip and retained mutation/
vector-recovery regressions. `native_opacity_probe` feeds11 overlapping scenes
to the existing Chrome geometry/hit/pixel oracle, with the same2/255 RGB mean
bound and boundary-hit convention. Scoped checks pass **85 View library +275
integration cases**, strict View all-target Clippy and **1971 Native/Chrome
checks**. Overlap ROI means span0–0.4325/255; the largest channel delta is16
at the curve. Evidence is under
`target/paint-next-stage/opacity-context/`; this is scoped follow-up work, not a
new consolidated full gate or whole-application/platform acceptance.

Next: whole inline-subtree opacity with shared glyph shaping and first-source
ligature ownership; independently investigate ordinary block-background versus
inline-content paint phases before claiming general CSS stacking parity.
Inline edges/borders/outlines, direct GPU fields/groups and the full accepted
platform/pilot/core/release plan remain open.

### Native inline opacity — committed in `ed77739e`, 2026-11-04

Varta now admits inline opacity through whole-subtree isolation,
not per-glyph/fragment alpha. A flow with nonunit inline opacity captures source
draws once after shared shaping, retains first-source ligature ownership and
joins the nearest root/opacity context's deferred queue. Fragment line identities
and source ordinals preserve overlapping-line paint/hit order. Original clip
geometry is retained until replay; text work is charged once and the existing
source-derived temporary-program and retained-cache ceilings are unchanged.

Public regressions cover shared ligatures, wrapped backgrounds/ink, nested
contexts and pointer priority, retained mutation/vector recovery, inherited
fractional curve coverage applied once, shared contour refusal and a65,536-glyph
capture crossing the retained command ceiling followed by cache recovery. The
last fixture splits text across admitted source nodes rather than enlarging the
wire string limit.

Frozen-source scoped checks pass85 View library +283 integration cases and strict
View all-target Clippy. The all-scene diagnostic records31/33 Chrome cases passing
and two failures; the full strict command still exits1. The preceding static-box
opacity oracle is rerun and passes1971 checks. No current-source actual GPU or
full-workspace pass is claimed for this increment.

That checkpoint retained two fractional curved-background/ink failures in the
expanded33-scene strict browser probe under the unchanged2/255 mean bound.
Its overlapping-line failure was fixed by ordering line fragments before
applying group opacity. Independent Chrome pixels at3px,3.25px and3.49px offsets
are identical;3.51px changes them. Blink uses a pixel-snapped rounded paint clip
separately from layout geometry. The following paint-rounding increment fixes
those original1× failures while preserving logical geometry. Earlier evidence
and unhidden failures remain under `target/paint-next-stage/inline-own-opacity/`.

The completed delivery batch remains open: paint rounding and the normal
block/inline phase fix below have scoped evidence; finish remaining text-raster
acceptance and consolidate the full gate at the delivery boundary.
Direct GPU groups/fields, native window/platform, pilot, core and release
requirements remain part of the same accepted plan.

### Decoration paint rounding — scoped 2026-11-04

Box and inline backgrounds, border/outline boxes and curved clipping now share
paint-edge rounding before DPI scaling, with separately retained logical input
geometry. Paint radii retain logical percentage references with snapped extent
constraints; broken-inline paint radii use floored fragment dimensions. Wide
slice construction stays f64 until local clipping. A positive logical clip can
retain pointer/text visibility even when its painted path collapses. Glyph
positions and raw styles stay fractional, and the implicit canvas preserves its
actual physical far edges at nonintegral scales. No source/cache/GPU limits grow.

Frozen-source native checks pass86 View library +287 integration +78 Desktop
GPU-feature tests, plus strict affected all-target Clippy. Both Debug and Release
probes cover22 scenes with64 actual GPU observations and24 explicit CPU fallbacks
per build; inline opacity remains CPU composition. Four corrupted oracle inputs
(image, length, legacy ink and scale) are rejected with their required diagnostics.
An initial GPU-fixture stateless comparison omitted its font; the fixed probe
retains that comparison with the correct explicit font, and the failed log remains.

New public regressions compare3px/3.25px/3.49px decoration pixels at1×/1.25×/2×,
while checking distinct logical bounds/targets, subpixel glyph positions, empty
paint versus nonempty logical clips, and full-canvas coverage at1.3×/1.7×/2.3×.
The original33-scene Chrome oracle now passes at1×; the20 decoration scenes pass
at2× (6767 and4043 checks, respectively). At1.25×,19/20 complete and the ink/curve
case still fails at mean2.248197/255, maximum98, against the original2/255 mean
bound. It remains an explicit acceptance gap. This does not establish
full typography, app-window or platform acceptance.

### Ordinary block and inline paint phases — scoped 2026-11-04

The reproduced negative-margin overlap is fixed in both pixels and input targets.
A bounded iterative scheduler separates ordinary descendant block backgrounds,
content, zero-level contexts and outlines. Flex/grid items and replaced controls
are atomic pseudo contexts; opacity descendants lift to the nearest real context
with their original ancestor clips. Anonymous text contributes actual occupied
font-box line fragments, so blank block width no longer intercepts later targets.
Background-decorated inline flows use one source capture even at opacity1;
undecorated glyphs keep the direct path while hits use line/source order.

Scoped checks pass86 library +291 integration +78 Desktop GPU-feature tests and
strict affected all-target Clippy. A cache-overflow regression now checks all
14,400 viewport hit positions and unchanged node bounds instead of requiring
identical internal metadata order across a real context change. Source/cache/GPU
budgets are unchanged. Debug and Release Metal probes each pass88 observations:
64 actual GPU draws and24 explicit CPU fallbacks, maximum RGB mean0.1472.
Inline opacity remains CPU composition. The old full gate remains historical.

The browser corpus grows from33 to59 cases. All24 new block/flex/grid cases
(plain text, inline opacity1/0.5/0 and nested ancestors) pass at1×/1.25×/2×,
including geometry, targets and the original pixel bound. The all-case diagnostic
passes58/59 at1× and2×,57/59 at1.25×; the strict commands still exit1. Dense
undecorated overlapping text reveals a new raster mismatch at all three scales
(mean5.388889/3.940351/5.369444, respectively), while the previous125% ink/curve
failure stays mean2.248197. No assertion or2/255 threshold is weakened.
Evidence: `target/paint-next-stage/paint-phases/`.

An independent axis-only Chrome experiment retains horizontal subpixel changes
but finds vertical paint-origin rounding before DPI. Blink's
[physical text geometry code](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/core/paint/text_fragment_painter.cc)
rounds the containing line origin while retaining the unrounded selection offset.
This corrects the earlier overly broad inference from diagonal-margin probes:
x/y must be tested separately. The next text increment must preserve logical
input/selection geometry and horizontal subpixels while aligning the vertical
paint origin; dense-text raster differences still need their own explanation.
Remaining inline styles, direct GPU groups/fields and the full accepted
core/pilot/platform/release plan remain active. General positioned/z-index
painting and native-window/platform acceptance are not established here.

### Paragraph vertical paint origin — scoped 2026-11-04

Plain and rich paragraph text now round only the containing vertical paint
origin before DPI. Horizontal glyph positioning and intrinsic line/font offsets
remain fractional; logical layout, hit rectangles, selection geometry and raw
styles do not round. The existing outline cache remains independent of position
and DPI, and native editable fields retain their direct shaped-control path.
A new regression isolates the axes for plain/rich paragraphs at1×/1.25×/2×,
checks fractional logical bounds and snapshots, and compares retained/vector
output. The first test attempt's exact-float assertion was corrected to allow
normal f32 layout arithmetic; no raster acceptance threshold changed.

Current scoped checks pass86 View library +292 integration +78 Desktop
GPU-feature tests (456 total), with strict affected all-target Clippy. Current
Debug/Release Metal probes each retain64 actual GPU +24 CPU-fallback observations
and maximum RGB mean0.1472; inline opacity remains CPU composition.
The original125% opacity/curve scene now passes with RGB mean1.925481/255,
maximum66 (previously2.248197,max98). All59-scene browser diagnostics now complete
58/59 cases at every scale, with10104 successful assertions each. The remaining
dense undecorated overlapping-text raster mismatch is still explicit and all
strict commands still exit1. Evidence: `target/paint-next-stage/text-origin/`.
The wider accepted plan and remaining typography/GPU/platform/pilot/release
acceptance remain active; this is not full CSS or platform readiness.

### Native glyph coverage — 2026-11-04

The dense-text mismatch is now traced to macOS font smoothing, independently
isolating standard/antialiased modes and raw CoreText output. The host rasterizer
interface preserves explicit font selection, shared Parley shaping and logical
geometry while providing platform coverage. The macOS preview installs CoreText;
coverage becomes disjoint rectangles for both CPU and actual GPU, sharing the
old glyph cache and paint/scene budgets. The portable reference remains available.
The pinned desktop-only CoreText wrapper adds one locked package, reusing the
existing CoreGraphics/CoreFoundation versions; no engine dependency or Wasm ABI
changes. Provider refusal, invalid masks, replacement, phases, byte ceilings,
hidden groups and hit/selection invariants have targeted coverage.

The same59-scene strict Chrome corpus passes10105 assertions at each of
1×/1.25×/2× using native coverage. Dense-text mean errors improve from
5.388889/3.940351/5.369444 to0.644444/0.221053/0.597222, respectively, under the
unchanged2/255 bound. The portable outline command retains its platform-smoothing
failure; no case or original assertion is removed. Native statistics record
actual coverage preparation so a silently substituted portable path cannot
count as native acceptance. The GPU corpus adds clipped dense text and a native
editable field, requiring actual coverage and ordinary vector output.
Evidence: `target/paint-next-stage/glyph-raster/`.

The completed painting package passes the one consolidated full gate:3434 tests,
0 failures,16 ignored, all eight commands with unchanged inputs. Baseline
conformance remains123 passed/4 known unsupported of127, no regressions or
infrastructure failures. Scoped native checks pass90 View library +296 integration
+78 Desktop tests; strict affected GPU Clippy passes. Each Debug/Release Metal
probe covers24 scenes with72 actual GPU and24 explicit CPU-fallback observations,
including required native coverage in clipped dense text and a native field;
maximum mean GPU/CPU RGB delta0.1472. Only result annotations follow the gate.

Collection members beyond index0 and variable font instances now use native
coverage (wave 3, below); color/bitmap fonts, `opsz` axes and unsupported
native cases retain portable outlines. Other platform glyph rasterizers, full
font/IME/window/accessibility acceptance, remaining inline borders,
direct GPU groups/color fields and the full core/pilot/platform/release plan
remain open. Current local success is not whole-plan or cross-target acceptance.

Solid underline now propagates decorating-box color/thickness through nested
text, supports nested lines and own field decoration, and uses bounded unhinted
quadratic/cubic ink interception with CJK exclusions. Per-line/source ordering
preserves backgrounds, overhangs and opacity. The independent original-font
Chrome corpus passes26 scenes/222 checks at each of1×/1.25×/2× under the unchanged
2/255 bound (maximum mean0.635532 across the three runs). It includes fractional
font boxes/baselines, wrapping, bidi, CJK, curves, clips, overhangs and overlapping
lines. Paragraph ascent/descent rounding is corrected without rounding horizontal
advances or containing line heights. The completed underline delivery gate is
`target/verification/20261104-142517-oo4etf6x/report.json`: all8 commands pass,
3448 tests/0 failures/16 ignored, with unchanged source SHA-256
`fbd84faf854f41f0802aec9f2dc6525c82be4fe87fc6ad843e54fb96b10af02d`.
View has93 library +307 integration tests; Desktop GPU-feature tests pass78.
Fresh post-gate native probes retain all26+59 scenes at each DPI, with actual
coverage/no outline substitution. Each25-scene Debug/Release GPU run passes76
actual GPU +24 explicit CPU fallbacks (maximum mean0.1472). The portable and
native decoration examples have distinct workspace output names. Only result
annotations follow this gate; outline work continues separately and is not covered
by that frozen source. These fixtures do not replace real-font, native-window,
control/system-input, other-platform or full-plan acceptance.

The open inline family batch now implements square and rounded fragmented
outlines, outer-union/inset geometry, folded inner contours and descendant-first
outline ordering. Fractional width/offset normalization preserves authored CSS.
Scoped checks pass104 View library +315 integration tests and strict affected
GPU Clippy. The independent Chrome corpus passes33 scenes at1×/1.25×/2×
(304/300/300 assertions). Debug and Release Metal each pass84 actual GPU +28
explicit CPU-fallback observations across28 scenes; maximum mean delta0.1472.
Evidence: `target/paint-next-stage/outline/checkpoint.json`. The3448-test full
gate above verifies the earlier underline inputs, not these newer changes.
Continue adjacent inline spacing/border work in the same open batch; real fonts,
native windows/input/accessibility, other targets and the whole accepted plan
retain their separate acceptance requirements.

The same open batch now admits non-replaced inline vertical padding in px/rem/%
and ignored vertical margins, including horizontal `auto` margins. Own background,
hit and outline fragments expand while content boxes, glyph/underline placement
and line metrics remain independent. Cascade side overrides/removal reuse the
existing layout store. All425 View tests pass (104 library +321 integration).
The new21-scene Chrome probe passes10131 assertions at each of1×/1.25×/2×;
maximum mean RGB delta0.579711 under the unchanged2/255 bound. Browser hit checks
exclude one-CSS-pixel boundary neighbourhoods, including curved edges, because
Chrome samples a pixel footprint while the native API tests an exact point;
all geometry and pixel comparisons remain enabled. Nonzero horizontal spacing,
inline borders and the broader acceptance requirements above remain open.

Final affected all-target View/Desktop GPU Clippy passes after replacing retained
opaque-pointer-capable layout lengths with owned pixel/fraction values. The
425-test View rerun passes; fresh native outputs are byte-identical to all21
Chrome-checked fixtures at every DPI. Final Debug/Release Metal runs each pass
30 scenes/92 actual GPU +28 explicit CPU fallbacks, maximum mean delta0.1472.
Both new padding scenes require actual vector GPU rendering. The172 pinned
source/build/fixture/tool inputs remain unchanged through these checks;
`target/paint-next-stage/spacing/checkpoint.json` records scope and hashes.
This scoped checkpoint precedes the final commit gate below; horizontal inline
spacing/borders remain open.

Next within this family: implement owned opening/closing edge records for
horizontal margin/padding/borders, then slice border painting over the resulting
fragments. Preserve whitespace collapse, source byte offsets, intrinsic widths,
natural bidi ordering and existing shaping/fragment/work budgets. The current
Parley `InFlow` box is not a drop-in edge: its line breaker always permits a break
after the box. A decoration boundary must retain the text's actual break policy,
including inside words, plus correct joining/ligature behavior at nonzero inline
edges ([CSS Text boundary shaping](https://www.w3.org/TR/css-text-3/#boundary-shaping)).
Verify those cases against the browser before extending native admission; keep
the family open until the shared boundary/border implementation is ready for its
single consolidated full gate. Edges and solid borders are implemented in the
working tree; see the next section.

### Native horizontal inline edges and borders — scoped 2026-11-04

Implemented in `cc3a94d` after `219db06`: non-replaced inline left/right padding,
solid borders and signed px/rem margins (`auto` is zero) are admitted. The rebased
`wip/inline-edge-shaping` prototype supplies Parley `InlineStart`/`InlineEnd`
edges that take advance without a break opportunity; native collection records
margin+border+padding per nonzero side, resolves edges next to a pending
collapsible space only when it is emitted, and projects them in visual order.
The box's first/last fragment receives its opening/closing border and padding,
enclosing fragments receive margins, negative margins shorten enclosing
fragments without negative widths, and empty decorated boxes keep their border
box. Vertical borders extend fragments like vertical padding. Each fragment
paints its slice of the continuous strip's border ring after its background,
in f64 until local clipping. Underlines stay under text; glyph loops skip edges.
Horizontal percentages, spacing or borders on `<br>`, edges in flows with
right-to-left runs, non-solid styles and distinct visible side colors are
refused explicitly. No source, fragment, shaping or cache limit grows.

Two corrections came from independent Chrome comparisons. Broken-inline radii now
zero the corners of a fragment's absent edges before side-overlap normalization
(Blink's per-fragment rounded border), so a narrow last fragment keeps its full
right radius and middle fragments are square. Browsers also hit-test line boxes
last to first: a point in a later line's margin gap hits the block even where an
earlier line's decorated fragment overlaps it. Rich flows now add those gaps as
block hit regions between line orders. A first model that captured the whole
line union was wrong and was replaced after a rounded-corner counterexample.
Chrome additionally differs in a one-pixel band just above a line box, depending
on fragment geometry; the shared verifier now treats that band like its existing
one-pixel rectangle edges.

`portable_edge_probe` has 39 Chrome scenes: 22 for spacing (padding, margins,
negative/collapsing margins, rem/fractional values, radii, outlines, nested and
empty boxes, boundary spaces, unbreakable words, opacity, underline, mixed and
`auto`), 15 for borders (sides, widths, radii, spacing, nesting, empty,
opacity, alpha, `currentcolor`, rem, vertical-only, outline, unbroken 50%) and
2 for overlapping lines. Each of 1×/1.25×/2× passes **68837 checks** with maximum
ROI mean 0.198/255 under the unchanged 2/255 bound. Earlier oracles pass at all
three scales: native text 10105, native decoration 222, outline 304/300/300 and
opacity 1971 (after declaring its 40×40 viewport, which the verifier began to
require in `ed77739e`). Spacing passes 9807 checks: the new band excludes 323
interior hit samples that also pass under the committed verifier (10130). View
passes 111 library + 336 integration tests, Desktop GPU-feature tests pass 78,
and strict all-target Clippy passes for View and Desktop with GPU. Debug/Release
Metal probes each cover 34 scenes with 108 actual GPU and 28 explicit
CPU-fallback observations (maximum mean 0.1472); the edge and border scenes
render on GPU. Evidence: `target/edge-probe/checkpoint.json`.

The consolidated full gate then passes all eight commands with unchanged inputs:
**3495 tests, 0 failures, 16 ignored**; conformance 123 passed/4 known
unsupported of 127, baseline matched, no regressions or infrastructure failures
(`target/verification/20261104-175437-p30mj15f/report.json`, source SHA-256
`ec01855a55530d285d047ade86c1154265f86853c97c4b037ae74c93d352d465`). Only
result/status documentation annotations follow it. Distinct border
side colors and other border styles, device-pixel border-width snapping (open for
all boxes), bidi edge reordering, horizontal percentages and
real-font/window/platform acceptance remain open.

### Direct GPU color fields — wave-3 package `w3-gpu-color-fields`, scoped 2026-11-05

Borders with distinct side colors no longer switch a GPU frame to CPU. Vector
preparation lowers the private color field to a public `DrawPrimitive::ColorField`:
the even-odd border contour plus disjoint whole-pixel cells whose colors come
from the unchanged CPU partition. The GPU clips to the contour once and fills the
cells, with no layer textures; the software display-list consumer replays the
same cells. Budgets, caches and GPU limits are unchanged
([contract](architecture.md#native-structure-and-user-agent-defaults)).

Scoped evidence on `wave3/w3-gpu-color-fields` (M1 Max, Metal):
`gpu_clip_probe` in Debug and in Release each covers 34 scenes with **120 actual
GPU and 16 explicit CPU-fallback observations** (previously 108 and 28). The
three mixed-border scenes report `gpu_rendered:true` at both scales, before and
after mutation, with mean RGB delta 0.0249–0.0780 under the unchanged 2/255
bound. Their maximum channel deltas (45–71) do not exceed those of the existing
one-color `border` (64–66) and `huge-inset` (75–77) scenes in the same runs.
Opacity scenes remain CPU fallback. Unit
tests check that every covered pixel gets the software color exactly once,
the lowering's peak byte budget, the over-budget display-list refusal and the
viewport-edge clamp. Border-color integration tests check vector frames, field
reuse, CPU retained damage after a mutation and the software consumer within one
channel unit, with and without an ancestor clip.
After the review follow-up, `verify.py local varta-view` (153 library + 351
integration), `cargo test -p varta-desktop --features gpu` (89 + 7 + 18), strict
all-target Clippy for View and Desktop with GPU and both probes (unchanged counts,
deltas and cells) pass. Probe timings were taken on a shared machine and
support no speed claim. Direct GPU opacity groups, window/platform acceptance and
other GPU devices remain open.

### Native control defaults — wave-3 package `w3-native-controls`, scoped 2026-11-05

Branch `wave3/w3-native-controls` (on `49186cf`) replaces the native
explicit-style requirement of buttons, text/search/password/number inputs,
textareas, checkboxes and radios with Chrome's user-agent defaults under the
official host's normalization, in a new version of the native user-agent table
([contract](architecture.md#native-control-defaults)): the control font
(13.333px, normal weight and line height, inherited family), per-kind padding
and borders, intrinsic sizes (content, the macOS `x`-advance text-field width,
textarea columns/rows, 13px checkables), Chrome's native theme for controls
without author background, border or radius, default disabled colors and
placeholder gray, and the default focus ring. Author-styled controls need the
`border` shorthand and take user-agent values for the rest. `input
type=number` edits and undoes its string value as text, and Up/Down step it as
Chrome does with the default step (owner decision, 2026-11-05). Closed from the
[wave-2 integration](#wave-2-integration--2026-11-05) list: unsized controls
now use Chrome's control size, a CSS-wide keyword in `font-family` is refused by
the shared L0 validator and by the browser host's font namespace, and a
paint-time reshaping refusal names its flow node.

Evidence, macOS arm64, headless Chrome 156. `native_controls_probe` with the new
original `controls.ttf` fixture passes `scripts/test-native-controls.mjs` at
each of 1×, 1.25× and 2× with **141801 checks** after the second review fixes
(boxes within 0.02px, hit targets, whole-viewport RGB means, number stepping
values and events, and number validity) under the unchanged 2/255 bound; the
largest means are 0.98, 1.08 and 1.07/255 in the author-styled select case
(select painter differences, below) and otherwise 0.29, 0.68 and 0.20/255.
Its 32 rendering cases are themed, disabled, seven default focus rings, five
author outline longhands (color, offset with a width, checkbox, author radius,
shorthand without `solid`), author-styled, control font, block placement, flex
buttons, placeholders, author-styled selects, rich button labels, checkables
stretched in a flex row and a grid, focused stretched checkables, padded
checkables (flex-start, stretch, grid) and four white-ring cases on a gray row;
43 stepping cases replay Up/Down with Shift, Ctrl, Meta and Alt on guest-set
values in real Chrome and compare the value and the `input`/`change` events,
and the validity of those and 29 more values is compared with Chrome's
sanitization.
`verify.py local varta-view` passes; `test-native-fields.mjs` passes 28 checks;
the structure and white-space oracles pass at the three scales; the inline
oracle still stops only at `paint-overlap-hits` with the recorded means (5.39,
3.94, 5.37). `headless_frame` rendered the showcase in the first round (not
rerun after the review fixes). It cannot load the console
view: the view module built from this tree is 4273111 bytes, above the frozen 4
MiB Wasm limit, which `headless_frame` enforces. An uncommitted scratch replay
of the `chat` and `stream` view-script trees found no control refused by a
control rule (the send button stops at its `transition-*` properties, and the
weights 500/600 have no face), but no reviewer can reproduce it, so the
console's control rendering is **unverified** until the module fits or the
replay becomes a test.

Review fixes (same branch). A checkable square is an intrinsic size, so the
default flex stretch and grid stretch grow it (13×62 and 150×34, as Chrome),
with the theme square centered; before, a definite 13px size kept it 13×13. A
focused control with only author `outline-color`/`outline-offset`/
`outline-width` now draws Chrome's auto ring in the author color and offset
(width ignored, as Chrome) instead of no focus indicator, and a radio's ring
follows Chrome's stadium when stretched. Rich control labels (text plus inline
elements) use the whole-pixel control line spacing; a rich flow now measures
each run with its own span's line height, because the shaper's run metrics
take the next span's style when a font-size change splits the run. CSS-wide
`font-family` keywords are named by native (`Violation::FontFamilyKeyword`),
the stylesheet checker, the compiler and the browser DOM boundary (an indexed
`ViewProtocolError`), and the three keyword lists share
`css::CSS_WIDE_KEYWORDS`.

Second review fixes (same branch). A themed checkable drops author padding and
borders as Blink's theme does, so its 13px square stays the border box (with
`padding: 6px`: 13×13 at `flex-start` and 13×40 stretched, as Chrome; it was
25×25). The auto focus ring now has Chrome's white outer ring, one pixel
outside the colored ring, visible on non-white backgrounds and through a
transparent `outline-color`. Number stepping now admits integers below 10¹⁸
(Chrome steps `1000000000000000` to `1000000000000001` exactly), treats a value
that rounds beyond `f64::MAX` as invalid, and counts each remaining refusal as
`stepsRefused` in the snapshot instead of dropping it silently. The oracle adds
padded checkables (flex-start, stretch, grid) and white-ring cases on a gray
row, checks the `input`/`change` order and Shift/Ctrl/Meta/Alt presses, and
compares native validity with Chrome's sanitization of every stepping and
grammar value; unit tests cover the owning span's line height in rich flows
and the control line spacing of rich labels.

Open: `select` keeps its explicit-style requirement (its arrow and popup belong
to another package); the select case shows Chrome's chevron arrow, wider text
inset and faded disabled select against the native triangle and full author
colors (mean about 1/255). Chrome's hover/active theme colors, the number spin
button, author-styled checkable themes (native paints the author box), unsized
text fields whose primary font has no `x`, and inline-block controls in a line
box are not implemented. Number stepping has no `step`/`min`/`max` (not L0
attributes; an open L0 item), refuses (and counts) values Chrome steps with
18-digit precision loss or prints in exponent form, and non-integral values
from 10¹⁵ that Chrome may still step exactly, reads locale forms typed in
Chrome (`5.`, `+5`, `1,000`) as invalid, and makes each step an Undo entry where Chrome 156 does not; the
browser host keeps Chrome's stepping. The auto focus ring's corner radius for
author-styled controls without a radius follows the author radius rather than
Chrome's default ring radius. Theme constants and the text-field width
formula follow Chrome on macOS; other platforms' Chrome may differ. Desktop
accessibility should expose a number field as a spin button with its value
(`crates/varta-desktop`, another package).

### Native accessibility geometry, macOS scroll routes and CoreText face variants — wave-3 package `w3-native-macos-adapters`, scoped 2026-11-05

Branch `wave3/w3-native-macos-adapters` (on `49186cf`).

- **Accessibility geometry.** Native bounds now cover offscreen (scrolled-out
  or clipped) elements, text nodes and the presented rows of an open select
  list, all from the presented frame; see the native accessibility text on
  `FrameMetadata::accessibility_geometry` in
  [the architecture contract](architecture.md). A hidden box is published only
  where clipping containers (`clips_children`) and the frame keep the
  clip-aware hit test of the vendored `accesskit_consumer` (shared by the
  macOS, AT-SPI and UIA adapters) away from it; other hidden nodes (under a scroll border,
  rows scrolled out of the open list) and closed-select options stay
  unbounded.
- **macOS assistive scrolling.** The pinned adapter routes AppKit's scroll-area
  page actions, and the increment/decrement and settable value of a published
  vertical scroll-bar child, to the existing page/item/absolute typed actions
  ([patch record](../vendor/accesskit_macos/VARTA-PATCH.md)). Native names and
  semantics come from an in-process observation of AppKit's `NSScrollView` on
  macOS 27.0.1. Not done: no assistive client drove Varta's own window (the
  probing process is not trusted for the accessibility API here).
- **CoreText face variants.** Collection members and variation instances are
  instantiated from the artifact bytes; color/bitmap tables, `opsz` and `avar`
  v2 keep declining with explicit counters. Each variation key is
  CoreGraphics's (unlocalised) axis name at the `fvar` index, whose tag
  (from CoreText) and ranges are checked, and every instance is read back;
  duplicate axis names or a mismatch decline (`declined_variation`). The
  native text probe's faces mode reports zero outline glyphs for the variable,
  differing-axis-name and collection fixtures and the Chrome oracle stays under
  2/255 at 1×/1.25×/2× (maximum mean 0.446), with the process language pinned
  to English, Ukrainian and German.

Scoped results: `python3 scripts/verify.py local varta-desktop` (99 library,
7 binary, 18 shell tests) and `local varta-view` (149 library, 349
integration) pass; strict all-target Clippy for both crates (and the desktop
`gpu` feature) and `cargo fmt --all -- --check` pass. The full gate is the
wave lead's. After review (fix round): hit tests over visible content no
longer return scrolled-out or clipped-away nodes, and a face whose Mac and
Windows axis names differ draws its requested instance; `local varta-desktop`
(104 library, 7 binary, 18 shell) and `local varta-view` (151 library, 349
integration) pass again. Second fix round: the clip-aware hit test moved into a
vendored `accesskit_consumer` so the AT-SPI and UIA adapters get it too
([patch record](../vendor/accesskit_consumer/VARTA-PATCH.md)), and variation
keys are matched by `fvar` tag. Third fix round: that match compared
CoreText's axis names, which are localised, so under Ukrainian or German every
variable instance declined; keys are now CoreGraphics's names at the `fvar`
index with CoreText's tag checked there, and a desktop test reruns the
instances under pinned English, Ukrainian and German. The Linux and Windows
targets were not compiled here (their dependency sources are not in the
offline cache and no Windows target is installed); the cross-platform reach of
the consumer patch rests on the shared lock entry until CI or a Pi 5 builds
it.

### Console browser leg — 2026-11-05

The backlog package `w4-console-browser-leg`, done in this session without
subagents. The real uarp-sdk client and the console's settings run in the
official browser host with native-equal results:

- `varta dev --app` now takes `--network-record` and `--network-replay`
  (before: only with `--view-script`), so an application instance without a
  scripted session records and replays its network log; refused without
  `--app` or a script and for the browser target (CLI smoke test). The
  native budget report prints the maintenance total whenever maintenance
  fuel was spent, also when no idle period ran a step.
- `examples/uarp-console/scripts/test-browser-sdk.mjs`: every SDK group is
  recorded natively against the loopback mock, replayed natively with the
  mock stopped and replayed in local Chrome from the program's own bundle;
  all six groups (43 cases) print the native replay's lines with exactly
  its fuel, host calls, microtasks, output, turns and idle maintenance.
- `examples/uarp-console/scripts/test-browser-settings.mjs`: the product with
  the network closed by the host ceiling; the key saved in the settings is
  stored in KV (OPFS in Chrome) and read back after a page reload, in the
  native sessions' three states and with their log lines; the key appears
  in no snapshot or page. Whole accessibility trees are not compared (Chrome
  hides content under a modal dialog; `w4-native-dialog`).
- Pilot finding F10: the SDK retries a request the host denied as a
  connection error, delaying the console's `unavailable` state on both
  hosts.

Not run: Firefox and WebKit (no Playwright module set up here), Linux, CI.

### Post-wave-3 debt batch — 2026-11-05

Done in one session without subagents (owner decision, 2026-11-05):

- KV read host work: the classifier pays as it goes, so what a read builds
  on the host (absent optional fields included) is bounded by the turn's
  fuel plus one stride; pinned by `host_work_of_a_read_is_bounded_by_its_fuel`
  and documented under KV decoding cost.
- Browser fatal texts: the Worker reports an exhausted turn fuel (guest
  metering or a host charge) and a v38 resource failure with the native
  runtime's texts from `registry.effects.fatal`; the turn-kind names live in
  `varta_common::budget::turn_kind`. A project test compares the templates
  with `BudgetExhausted`/`ResourceExhausted`; the Chrome KV acceptance and
  the effect acceptance (new `fuel-main`, `fuel-timer`, `memory-budget`
  cases) require the exact native failure text.
- `font-family` CSS-wide keywords and unsized control font size were already
  closed in wave 3; a CSS-wide keyword held by a custom property takes its
  inherited or initial value on both renderers.
- Load-sensitive tests: fuel-comparison runs take a test-only wall clock so
  only fuel ends them; the view network-wait tests prove their bound by the
  failure kind, the server's EOF and fuel instead of a 2 s wall bound; the
  HTTP server harness reports a service's own failure before it listens.

Verification: the full gate above, `node scripts/test-browser-kv.mjs` (18
cases) and `node scripts/test-browser-effects.mjs` (12 cases) in local
Chrome 156.

### Wave 3 integration — 2026-11-05

Eleven wave-3 packages are merged on `main` after review and up to four fix
rounds with independent re-checks: `w3-v38-bytes-unknown`,
`w3-browser-kv-opfs`, `w3-native-controls`, `w3-network-replay`,
`w3-console-stitching-screens`, `w3-native-consent`,
`w3-native-macos-adapters`, `w3-gpu-color-fields`, `w3-conformance-v38`,
`w3-ci-browser-gate` and `w3-sdk-test-sources` (`w3-hw-linux-desktop` needs a
Linux machine and was not run). On the merged tree the pilot passes
`check-sdk.sh` (65/65 files, 6/6 generic bodies), the console's `test.sh` (249
tests in 36 groups and 10 view checks) and `sdk-loopback.sh` (43 cases in 6
groups); the Chrome KV acceptance (18 cases) and the browser replay
acceptance match native. Integration resolved code conflicts between the KV
codec extraction, byte-value staging and network replay in the browser
Worker, rebuilt the KV codec after the merge, and updated the CLI smoke test
for browser builds that now ship KV storage.

Owner decisions (2026-11-05): the value ABI stays 18 with the additive
`Bytes` tag; the whole-artifact OPFS lock in the browser is an accepted,
documented deviation; the native prompt input delay is 500 ms; number
fields step with ArrowUp/ArrowDown as Chrome does. Lead decisions: an
application's first-turn deadline starts after first-run approval; the KV
value extent bound follows from the value size (shared parts count at every
reach) so values Deno stores unshared still store and read, while
sharing-driven expansion is refused by name; the browser refuses nested KV
database paths.

Open after integration:

- (Closed after integration: the host work of a KV read for absent optional
  fields is bounded by the turn's fuel and pinned by a codec test; the
  browser host reports exhausted turn fuel and v38 resource failures with
  the native texts, compared in the KV and effect acceptances.)
- Native consent: a request whose patterns do not fit the prompt is denied
  without a prompt natively but shown in the browser; 'allow for this
  session', a refusal cool-down and refused-required presentation remain
  owner decisions.
- Accessibility hit tests, the scroll-bar child and the consumer patch are
  built and observed on macOS only; Linux (AT-SPI) and Windows (UIA) were not
  compiled here.
- Number steps beyond exact integers below 10^18 are refused (Chrome prints
  `1e+18`); a spin button and `step`/`min`/`max` beyond the admitted L0
  attributes remain open.
- Conformance: KV and fetch cases with real grants need file roots and a
  loopback server in the runner (owner decision); a union-to-record argument
  conversion is recorded as a reviewed gap.
- The uarp-sdk own tests are pinned (3 files) but refused by `varta check`
  (`node:assert/strict`, `import.meta.url`) until the M7 harness exists.
- Not run: Linux/Windows/Pi 5, CI on the merged tree (nothing pushed),
  Firefox for KV, manual live-window checks (consent prompt, clipboard and
  link gestures), live `api.snaga.ai` sessions.

### Wave 2 integration — 2026-11-05

Nine wave-2 packages are merged on `main` after review, two fix rounds and
independent re-checks: `w2-v38-sdk-path-gaps`, `w2-net-abort`,
`w2-native-positioned-zindex`, `w2-native-whitespace`,
`w2-shell-fonts-effects`, `w2-headless-geometry`, `w2-browser-kv`,
`w2-console-real-client` and `w2-browser-docs-drift`. Pilot findings F1–F3
are closed. On the merged tree the pilot passes `check-sdk.sh` (65/65 files,
6/6 generic bodies), the console's `test.sh` (179 tests in 32 groups and both
view checks) and `sdk-loopback.sh` (43 cases in 6 groups). Integration fixed
one semantic merge conflict (a font-size test written against the old
`resolve_text` signature) and made every white-space refusal of a plain text
flow name the parent element.

Owner decisions (2026-11-05):

- The console manifest requests `time.timer` and `time.now` besides
  `net.request` and KV; the unmodified SDK needs both.
- Browser KV storage uses OPFS `FileSystemSyncAccessHandle` in the dedicated
  Worker, with the value layer shared as Rust compiled to wasm32
  ([agreed design](#browser-kv-storage-agreed-design-wave-3)); browser builds
  admit KV grants (wave-3 package `w3-browser-kv-opfs`).
- v38 opens bytes (`Uint8Array`/`ArrayBuffer`) held by `unknown`, replacing
  the earlier compile-error decision; this needs a value-layout tag and is a
  wave-3 package (implemented on `wave3/w3-v38-bytes-unknown`, wave 3 (k)).
- The value ABI number stays 18: `ValueKind::Bytes` (tag 15) is an additive
  tag of the in-development value layout 18, not a step to a new value ABI
  ([contract](architecture.md#bytes-held-by-unknown-byte_values)).
- The uarp-sdk 0.13.2 own tests may be fetched from GitHub into an ignored
  directory with the commit pinned (wave-3 package).
- HTTP contract v1 stays frozen with its immediate 503 while busy; only v2
  waits for the worker permit.

Open after integration:

- (Closed in wave 3 by `w3-native-controls`: a CSS-wide keyword in
  `font-family` is refused by the shared validator and the browser's font
  namespace, and a custom property holding one takes its inherited or
  initial value on both renderers, so it never reaches `font-family`;
  unsized controls use Chrome's control font size natively.)
- The manual trusted-gesture run (copy target and link in a live
  `varta-shell` window) needs a human.
- The Taffy-measurement mapping of a refused text node to its flow has no
  input that exercises it; paint-time reshaping refusals still carry no node.
- The console view itself renders no fixed or z-index node yet, so its
  `headless_frame` acceptance for positioned content stays open; the browser
  applier's view root must become a stacking context.
- Not run: Linux/Windows, CI with the merged tree, Firefox for the KV and
  font cases, live `api.snaga.ai` sessions.

### Wave 1 integration — 2026-11-04

Six parallel packages, each reviewed independently and corrected after review,
merged into `main`: `w1-package-specifiers`, `w1-sdk-network-contract`,
`w1-browser-pilot-effects`, `w1-native-shell-core`,
`w1-native-structure-text` and `w1-pilot-sdk-probe`. Their own subsections
record scope and scoped results. Owner decisions taken at review: the v38 read
delivery cap (instead of documenting a fatal turn-fuel exhaustion), the native
fail-closed content-coding rule and differing browser-controlled headers as
documented, Chrome's ellipsis hit targets (hidden truncated text still hits its
element), and a test-time `varta build` of the shell probe instead of a
checked-in Wasm fixture.

Open after integration:

- Pilot findings F1–F3 (`examples/uarp-console/README.md`), closed in wave 2
  (F1 and F3 by `w2-v38-sdk-path-gaps`, F2 by `w2-net-abort`): an explicit generic
  call on an element-access receiver, an aborted fetch's connection held until
  the client's next wake, and `cause` through `instanceof Error` /
  `new Error(msg, { cause })`.
- Native text: Chrome 156 on macOS rounds an exact half-pixel font ascent or
  descent down, Varta rounds it up (the `headings` structure case keeps
  rgbMaxDelta 255 with a passing mean); needs an owner decision because it
  changes every text path. Chrome's minimum logical font size (non-absolute
  sizes below 9px compute to 9px) is modelled on the wave-2 branch
  `wave2/w2-native-positioned-zindex`.
- The portable `native_inline_probe` fails `paint-overlap-hits` against Chrome
  (portable rasterizer smoothing; the host CoreText probe passes); pre-existing.
- `varta-shell` creates per-artifact KV directories before the runtime validates
  code and profile (documented; moving it needs a runtime API change).
- The prototype web host records the v38 network rules as deviations; its
  differential v38 `net.request` large-body case fails for that reason.
- Not run: real-window shell sessions, Linux/Windows, CI with the new Node
  step, WebKit/Firefox after the fix round.

### Aborted fetch closes its connection — wave-2 package `w2-net-abort`, scoped 2026-11-04

Fixes pilot finding F2 (`examples/uarp-console/README.md`); both sides were at
fault. Client: `net_request_abort` dropped the transfer's task handle, which
only marks a Tokio task cancelled. The current-thread request runtime dropped
the task, and the separately spawned hyper connection task that owns the
socket, only when the event loop next drove it, which a guest waiting only on
timers never does. Now the transfer shares its connection task with its owner
(`ConnectionSlot`), and an abort cancels both tasks and runs the runtime until
both are dropped before the call returns. Server: a client disconnect cancels
the v2 worker, but it holds the single worker permit until it stops, and the
transport accepted the next connection at once and refused it with 503. The v2
accept loop now waits for the permit first; HTTP contract v1 stays frozen
with its immediate 503 while busy (owner decision). Contract text:
[net_request cancellation](architecture.md#network-requests-net_request) and
the HTTP v2 native server section.

Regressions in `net_tests`, each shown failing before its half of the fix:
`aborted_requests_close_their_connection_before_the_guest_wakes` (v37 and
v38 against the loopback server's new `/held` route, which records the
client's end of stream; before: the close came 602 ms after the abort, at the
guest's wake) and
`aborted_requests_leave_the_server_worker_free_for_the_next_request` (a v38
client and a Varta v2 service; the test observer holds the cancelled worker
for 300 ms; before: `hello: 503`). Scoped verification: `verify.py local
varta-runtime --filter net` (55) and `--filter server` (32), the runtime
`abort`, `web_bytes`, `v37_integration`, `frozen_profile` and `cancellation`
groups, strict Clippy of `varta-runtime` and formatting. The README
reproducer, run with a debug `varta` against a scratch copy of the mock's
`/slow/health` and `/unary/health` routes, now reports the cancelled request
after about 200 ms of wall time and `200` for the next request, with no wait
and with a 1 s wait. The full gate is the wave lead's.

Review follow-up (same branch): a transfer that ends on its own (end of body,
body limit, inactivity timeout) now closes its connection before its terminal
completion is delivered, the same way as an abort. The new regression
`transfers_that_fail_on_their_own_close_their_connection_before_the_guest_wakes`
(v38 head and body timeouts) passes with or without that change on macOS:
Tokio's current-thread scheduler already dropped the cancelled connection
task in the same tick, so the change makes the guarantee explicit rather than
fixing an observed leak. The abort regressions no longer abort on a timer:
the loopback `/gate` route answers once the server holds `/held`, and the
checks read the order of the server's trace, keeping only the close-to-wake
margin as wall time. The `server_tests::cancellation` disconnect checks
(all v2) now require the next request to be served, with no 503.

### Native preserved white space — wave-2 package `w2-native-whitespace`, scoped 2026-11-04

Branch `wave2/w2-native-whitespace` (on `4fce278`) replaces the refusals of
`pre-wrap`/`pre` tabs and leading/trailing line spaces, and of text nodes whose
`white-space` mode differs from their flow root, with the
[native preserved white space](architecture.md#native-preserved-white-space)
model in `varta-view` (`text::whitespace`; inline flow changes in `inline.rs`).
Flows without such preserved text keep Parley's breaking unchanged.

Scoped results: `python3 scripts/verify.py local varta-view` passes (lib 124,
integration 346); `cargo clippy -p varta-view --all-targets -- -D warnings` and
`cargo fmt --all -- --check` pass. The new `native_whitespace_probe` (12 cases:
tab stops and a skipped near stop, preserved, hanging and conditionally hanging
spaces and tabs, wrapping at the right edge, mixed modes in both directions,
`pre` and `pre-wrap` shrink-to-fit and min-content, center alignment) passes
`scripts/test-native-inline.mjs` with **10636 checks** at each of 1×, 1.25× and
2×, maximum ROI RGB mean 1.052/1.627/1.071 under the 2/255 bound.
`native_structure_probe` (its `pre` case now takes the new path) passes 51181
checks per scale; `native_paragraph_probe` 81 and `native_fields_probe` 28 pass.
`native_inline_probe` still stops at the documented pre-existing
`paint-overlap-hits` case with unchanged means 5.39/3.94/5.37; without that case
it passes 9927 (1×) and 4227 (1.25×, 2× decorations) checks. Chrome 156.0.8078.80
on macOS arm64; the probe's JSON goes to the existing inline oracle:

```sh
cargo run -p varta-view --example native_whitespace_probe --locked -- 1.25 > target/native-whitespace-125.json
node scripts/test-native-inline.mjs target/native-whitespace-125.json
```

Open: right-to-left runs on this path are refused; ellipsis roots whose text
needs it are refused; the L0 grammar has no `tab-size` (tabs always use 8);
min-content keeps the final lines' tab advances for a tab that cannot end a
line. Chrome breaks before a closing bracket after spaces in every mode, which
Parley's line breaking does not (pre-existing, observed in Chrome 156 only).
Two integration tests that asserted the old refusals (`paragraphs.rs`,
`structure.rs`) now assert the laid-out lines.

Review fixes (scoped, 2026-11-05): tab stops use the flow root's space advance
whatever the tab's own font size; a wrapping node inside a `pre`/`nowrap` root
wraps at min-content on Parley's path too; underlines and hit fragments keep
trailing `pre` spaces on the plain-text paint path; the model's refusals become
`LayoutError::Unsupported` with the flow's node. Five new probe cases
(`tab-small-span`, `tab-mixed-spans`, `pre-normal-span-min-content`,
`nowrap-normal-span-min-content`, `pre-underline-trailing`) fail against the
unfixed code and pass now: the probe's 17 cases give **15150 checks** at each of
1×, 1.25× and 2×, maximum ROI RGB mean unchanged at 1.052/1.627/1.071. Structure
(51181 per scale) and paragraph (81) oracles pass; the inline oracle again stops
only at `paint-overlap-hits` (mean 5.39) and passes 9927 checks without it. A
ligature that joins text from two elements (the fixture's `AA`) gives the later
element a zero-width line fragment, which the native element box includes and
Chrome's `getBoundingClientRect` does not. The difference is independent of
`white-space`: it also occurs with a `normal` root and a `normal` span, on
Parley's own path; the fixtures avoid it.

Second review fixes (scoped, 2026-11-05): underlines skip preserved tabs,
which Chrome never decorates, on the plain and rich paths, and keep trailing
preserved spaces; every refusal of a plain text flow, including Taffy
measurement, names its parent element. Native tests cover both underline
rules and the plain `pre`/`pre-wrap` trailing-space underline and hit
fragment. Four probe cases (`pre-underline-tab`,
`pre-underline-trailing-tab`, `pre-wrap-underline-tab`,
`pre-underline-tab-rich`) each fail against the unfixed code at 1× (RGB
means 4.096, 5.363, 5.495, 9.065) and pass now: 21 cases, **18753 checks** at
each of 1×, 1.25× and 2×, maximum ROI RGB mean unchanged at 1.052/1.627/1.071.
Structure (51181 per scale) and paragraph (81) oracles pass; the inline oracle
stops only at `paint-overlap-hits` (mean 5.39) and passes 9927 checks without
it. A paint-time reshaping failure of a plain text node still reports a text
error without a node.

### Native structure, alignment and ellipsis — wave-1 package `w1-native-structure-text`, scoped 2026-11-04

Branch `wave1/w1-native-structure-text` (on `cc3a94d`) admits `h1`–`h3`, `ul`,
`ol`, `li` with outside disc/circle/square/decimal markers, `pre` and `label`,
inherited `text-align: left | center | right` and one-line `text-overflow:
ellipsis` in native layout and paint. Defaults come from the new versioned
native user-agent table in `varta-view` (`NATIVE_UA_DEFAULTS`); see the
[architecture contract](architecture.md#native-structure-and-user-agent-defaults).
Labels forward clicks through the existing headless `for` activation; headings,
lists and items publish their levels/roles with frame bounds in the headless and
AccessKit trees. A second Chrome comparison showed that truncated text keeps its
pointer targets; by owner decision native ellipsis keeps Chrome's behaviour, so
hidden truncated text still hits its element (there are no visible-glyph-only
targets).

Console refusal triage, unchanged artifact (SHA-256 `ac764bd1…3043164`), Arial
(regular/bold) for `system-ui`/`sans-serif` and Courier New for `monospace`,
1024×768. `headless_frame` still stops at node 16 (chat) and node 12 (stream)
before and after; a scratch triage that neutralizes each refusal and continues
lists them in order. Before: chat 16 `button` control box rule plus
`text-align: left`, 17 `text-align: center`, 19 `text-overflow: ellipsis`, 39
`textarea` control rule, 43 `button` control rule plus transition properties,
then paint refusals of synthesized weight 500 (19, 43); stream 12 `textarea`, 16
`button` and transitions, weight 500. The final reviewed trees additionally
refuse `h2` (190, 208, 178; stream 69) and `pre` (202; stream 81). After: only the
`button`/`textarea` control box rules, the transition properties and weight 500
remain; replacing 500 with an available face admits every remaining text,
heading, `pre`, alignment and ellipsis node. Control defaults belong to the
controls package; weight 500 is font coverage, not engine policy.

`native_structure_probe` covers 9 cases (headings, overrides, lists, nested
lists, list spans, `pre`, `label`, aligned wrapped/bidi/decorated paragraphs,
ellipsis) against Chrome through `scripts/test-native-structure.mjs` with two
original weighted faces: **30246 checks** pass at each of 1×, 1.25× and 2×,
maximum whole-viewport RGB mean 0.577/255 under the unchanged 2/255 bound.
`portable_edge_probe` (68837) and `native_paragraph_probe` (81) pass, and the
native outputs of the inline, edge, opacity, spacing, outline, decoration,
fields and lengths probes are byte-identical to `cc3a94d`.

Review fixes on the same branch. The monospace size rule now follows Blink's
generic-family switch (`style::FontSize`): `px`/`rem` sizes are absolute, heading
scales and percentages keep the parent's state, and 13/16 applies at each
element whose sole-`monospace` family state changes, before a percentage line
height resolves. Six new probe cases reproduce Chrome 156's `h1 > code` 26px,
`h3 > pre` 15.21px, `p { font-size: 150% } > code` 19.5px, `pre > span {
font-size: 200% }` 26px, `h2 { font-family: monospace }` 19.5px and
`pre { line-height: 150% }` 19.5px; Chrome draws generic monospace with a system
face, so each monospace element is a flex container whose `line-height: 100%`
(or 150%) records its size and whose fixture-font item also checks the switch
back. List markers resolve styles and ordinals in one walk (each `ol` counts
its children once) and charge the walk, path reads and baseline search to the
inline work allowance. With the 15 cases the structure oracle passes **51181
checks** at each of 1×, 1.25× and 2× (maximum whole-viewport RGB mean
0.577/255); the monospace cases' means are at most 0.141. `portable_edge_probe`
(68837), the desktop CoreText `native_text_probe` through the inline oracle
(10105) and `native_paragraph_probe` (81) pass.

The structure probe's portable outlines leave two Chrome differences in the
`headings` case, which passes only on its whole-viewport mean (rgbMaxDelta 255
at 1×, 1.25× and 2×); neither is margin collapse or snapping (every box matches
within 0.02 and the h1–h3 baselines match). First, Chrome 156 on macOS rounds an
exact half-pixel primary ascent or descent down, while Varta rounds half away
from zero: with the fixture's 800/200 units, the 12.5px paragraph descent of
2.5px becomes 2 in Chrome and 3 natively, so its half leading is 2 instead of 1
and Chrome paints its glyph one CSS pixel lower (rows 115–123 versus 114–122 at
1×, two device rows at 2×, about 1.25 at 1.25×). Chrome baseline offsets in a 100px
line confirm it for ten sizes: descents 2.5, 3.5, 4.5, 5.5, 6.5 and 12.5px
(12.5–62.5px) and ascents 10.5, 15.5, 17.5 and 4.5px (13.125, 19.375, 21.875,
5.625px) all round down, 12.505px (descent 2.501) still rounds down and 12.51px
rounds up. Blink rounds Skia metrics half up, so the platform metric input is
below the half on CoreText; matching it needs a platform metric decision for
every text path, not a structure fix, and stays open. Second, Chrome's CoreText
glyphs are dilated (about 0.5 device pixel upward and 0.2 sideways for the bold
h1 at 2×, none below the baseline) and their baselines are whole device pixels
at 1.25×, whereas the portable outlines are undilated at a fractional device
baseline; this is the documented portable smoothing difference that the
desktop host coverage path removes.

The `native_inline_probe` oracle's `paint-overlap-hits` failure (RGB mean
5.39/3.94/5.37 at 1×/1.25×/2×) is that same portable smoothing difference, not
a Chrome or Varta regression: today's Chrome screenshot of the case is
byte-identical to the failing run of 12:20 on 2026-11-04 (Chrome
156.0.8078.80), and the passing evidence of 14:56 used the host CoreText
rasterizer (`text_rasterizer: host`, mean 0.64), as the desktop
`native_text_probe` does. Four overlapping 20px lines stack Chrome's sideways
dilation into the 24×30 region's glyph edge columns (RGB delta up to 49 on
every row) where the portable outlines leave white.

Still open: Chrome's half-pixel ascent/descent rounding above, normal line
height with fractional ascent/descent (Chrome rounds each), ordinal attributes,
`list-style`, marker `tabular-nums`, multi-line ellipsis, decorated or bidi
ellipsis, author `text-align` on fields and the full gate after merge.

### Native positioned painting and z-index — wave-2 package `w2-native-positioned-zindex`, scoped 2026-11-04

Branch `wave2/w2-native-positioned-zindex` (on `4fce278`) removes the native
paint refusal of non-static `position` and non-`auto` `z-index`. Paint order,
clip scopes, hit order, wheel chaining and scroll-into-view follow the
[architecture contract](architecture.md#native-positioned-painting-and-stacking):
CSS2 Appendix E layers per real stacking context, containing-block clip and
scroll chains, fixed boxes anchored to the viewport, and a refusal for absolute
or fixed boxes that escape an opacity group's clip. Zero-level contexts now
paint strictly in tree order; before, an opacity context inside a flex/grid item
painted after a later block's opacity context. Chrome's minimum logical font
size is modelled for non-absolute sizes (the wave-1 open item).

`native_positioned_probe` covers 12 cases (relative boxes over later in-flow
blocks and text, z-index levels, negative layers in a context and under an
auto-positioned box, fixed and initial-containing-block absolute boxes in a
scrolled box, an absolute box with a scrolled containing block and a scrolled
intermediate box, an opacity context with an inner z-index 5 below a later
z-index 1 sibling, rounded positioned clips, flex-item z-index, fixed toasts
with text, minimum font size, static opacity blocks whose z-index -1 and 5 have
no effect, and `p { font-size: 50% } > code`, whose monospace re-derivation to
6.5px computes to 9px) against Chrome through
`scripts/test-native-positioned.mjs`: **7347 checks pass at each of 1×, 1.25×
and 2×**, maximum RGB mean 1.349/255 (fixed toasts with text at 1.25×) under
the unchanged 2/255 bound, and both nearest-edge `scrollIntoView` comparisons
change exactly the native plan. With the minimum font size disabled the probe
fails on the first affected box height. Unit tests cover layer order, hit
order, negative layers, late-found pseudo-context layers, tree-ordered opacity
contexts across flex items, static opacity contexts ignoring z-index, flex
items, fixed and absolute scrolling and wheel chaining, the escape refusal,
retained/vector parity and the console mockup's toast stack and
narrow-viewport panel rules.

The existing oracles still pass: `native_inline_probe` 58 of 59 cases (9927
checks per scale) with `paint-overlap-hits` failing exactly as documented
before (portable smoothing, means 5.388889/3.940351/5.369444),
`native_paragraph_probe` 81, `native_opacity_probe` 1971,
`native_structure_probe` 51181 per scale and `native_lengths_probe` 168.
`python3 scripts/verify.py local varta-view` passes (128 library + 346
integration tests), as do strict all-target Clippy for `varta-view` and the
104 default-feature `varta-desktop` tests; the GPU feature was not run.

Outside the package's files: two refusal triggers in `crates/varta-view/tests/`
(`headless.rs`, `renderer.rs`) now use still-unsupported content, the two
assistive scroll call sites use the containing block chain, and the
`View::scroll_into_view_plan` doc comment in `lib.rs` (owned by another wave-2
package) now states that positioned targets scroll their containing-block chain.

Review fixes on the same branch: a static block that is a stacking context only
through opacity takes level zero, since `z-index` does not apply to it (before,
it was ordered by its `z-index`, against Chrome); the minimum font size reads
the absolute state from `style::FontSize` instead of a second flag. After
these fixes the positioned probe, `native_structure_probe` and
`native_inline_probe` (58 cases; `paint-overlap-hits` with the same means) were
re-run at all three scales with the results above; the paragraph, opacity and
lengths oracles were not re-run.

Open, not met:

- Acceptance "`headless_frame` of the console view no longer refuses fixed or
  z-index nodes" is **not shown**. The console source renders no toast or panel
  node yet and its initial frame still stops at the control-box rules, so
  `headless_frame` was not run; a unit test with the console mockup's toast and
  panel CSS stands in for it and does not close the item.
- For the web-host owner (`examples/varta-web-host`, removed on 2026-11-06): the browser applier's
  view root must be a stacking context (`isolation: isolate` on the root
  container) to match native root-level negative `z-index` layers, which paint
  above the root's background. Until then those layers differ between native
  and web.
- Scrolled paint is not snapped to device pixels as in Chrome; positioned
  inline boxes, `position: sticky` and Metal GPU runs of positioned scenes
  remain open.

### Native inline painting commit gate — 2026-11-04

The requested commit boundary passes all eight consolidated checks: **3473 tests,
0 failures,16 ignored**, with unchanged inputs. Report: `target/verification/20261104-161029-qyx08oka/report.json`;
source SHA-256 `22ba2807b47b0ab666df1b89bd0bc57a6b1d96ed5a55b2f9650d474b7478ad71`.
The compatibility inventory remains123 passed/4 known unsupported of127, baseline
matched, with no regressions or infrastructure failures. Current scoped GPU
checks retain30 scenes/92 actual GPU +28 explicit CPU fallbacks per Debug/Release
build. The172 pinned engine/build/fixture/tool inputs match those scoped checks.
Only result/status annotations follow this gate; the gated code was committed
unchanged as `ed77739e`. Incomplete horizontal edge shaping and native
border/fragment integration are excluded from that commit; the prototype is
kept only on the local branch `wip/inline-edge-shaping`. The full accepted core/pilot/platform/
release plan and the remaining real-font/window/input acceptance stay open.

### Current UI checkpoint and proposed continuation

This section retains the earlier UI development records. The current merged
surface is summarized in [Status](#status); current blockers are in the
[integration backlog](#current-integration-backlog). UI-1/UI-2 and the optional
GPU foundation are already implemented, rather than the next unstarted packages.

**Recorded UI-2 package — native accessibility foundation, after `be85c981`.**
Shared flat semantic snapshots, native AccessKit roles/names/states, stable
nonrecycled platform IDs and presented-frame physical bounds are implemented
(committed in `5a521250`). Focus, Click, string SetValue and ReplaceSelectedText route
through bounded host queues and ordinary input/broker/event paths, with source
identity and frame guards. Inactive rendering avoids semantic projection.
The existing headless JSON contract, Wasm ABI and production guest budgets stay
unchanged. Scoped checks pass: 6 semantic/action tests, one runtime event-turn
test, 22 desktop tests in each default/GPU build and strict affected-package
all-target Clippy with GPU. Native macOS API checks pass in CPU Debug and Metal
GPU Release: controls/names, activation, Ukrainian SetValue and concrete option
selection; Debug also covers radio/checkbox, Undo and window zoom/restore with
preserved state and subsequent input. Release guest async output is observed.
[Pinned inputs and observations](evidence/accessibility-checkpoint-2026-11-03.json).
Consolidated verification ran all eight commands successfully: **3215 passed,
0 failures, 16 ignored**; the conformance baseline matches (123/127, four known
unsupported). The wrapper's source guard failed because an additional native
password-fixture build created one untracked generated manifest during the run.
Moving that artifact to `target` restores the exact initial fingerprint;
reintroducing only that artifact reproduces the exact reported final fingerprint.
The eight completed command results are reused for those identical source inputs.
The original report remains failed; this is an audited reconciliation, not a new
clean full pass. Evidence: `target/verification/20261103-192156-v_2wjssj/report.json`
and `source-reconciliation.json`; restored source SHA-256
`6871cd21c8520f2f7e48e677e937c233f50dcbcea33524e8fbaf2659a7a9bf44`.
Only documentation/observation records change after reconciliation.
The additional native password fixture exposes a secure field with masked values
before and after assistive editing. GPU Release is 28,976,058 bytes, +401,878
against the previous GPU UI bundle; this is package size, not memory use.
The subsequent description increment below implements bounded `aria-describedby`.
Full text ranges/line data, assistive scrolling/announcements and
screen-reader/cross-target acceptance remain separate open work. UI-2 does not
close P10/P13 or claim complete accessibility.

**UI-2.1 — native descriptions and bounded observer work.** `aria-describedby`
now contributes a separate typed/AccessKit description using shared finite name
traversal. The native observer preflights names, actionable generic-container
names and descriptions before allocating strings. One fixed 8 MiB potential-work
allowance returns a typed observer error; it does not alter v38 guest admission,
headless JSON or browser budgets. Ordinary trusted action validation no longer
evaluates a name. On refusal, desktop publishing clears old actions and exposes
a host-owned static-text explanation; later valid state restores fresh native IDs.
Semantic FIFO rebinding also preserves a queued focus-then-value pair across its
focus acknowledgement while rejecting newly readonly/replaced/hidden targets.

The typed Showcase links name, textarea and select help text through explicit
ARIA references. Scoped verification covers exact work allowance and one-byte
overflow, hidden references/order/cycles, password masks, replacement nodes,
generic-name expansion, runtime survival/recovery and stale native-action cleanup.
Current scoped checks pass **237 view cases, 2 runtime assistive cases and
25 desktop cases**, affected GPU all-target Clippy and the final desktop GPU
all-target check; the 12-input Showcase scenario passes. CPU Debug and Metal
Release expose separate reactive `Help` descriptions. A CPU Debug fixture
exposes the explicit refusal text and recovers by ordinary Space in the same
process. Those native observations precede the final semantic FIFO refinement;
its native retry is blocked by UI-tool timeouts. The attempted final Release
textarea edit has no confirmed outcome. A short thread sample shows worker waits
and AppKit window-raise work, without proving the timeout cause.
[Pinned observations, hosts, code digest and limits](evidence/accessibility-descriptions-2026-11-03.json).
The older 3215-test reconciliation above does not verify these newer code inputs.
The full workspace gate remains a pre-commit
batch-boundary check. Text ranges/line data, assistive scrolling/announcements and
VoiceOver/other-target acceptance remain open.

**UI-2.2 — logical field ranges and checked selection.** Committed work adds
scalar/CRLF text units, Unicode word starts, hard lines and directional selection
to the native observer. AccessKit receives chunks of at most 255 units with
same-line links and exact UTF-8 source mapping held in the host. Password runs
expose masks only. An observer-only ceiling of 4096 auxiliary runs shares the
recoverable refusal path; metadata also consumes the existing work allowance.
Absolute selections carry opaque content revisions, so actual edits, Undo/Redo
and input-type changes invalidate old endpoints even when bytes later match.
Equal echoes and selection-only frames preserve run IDs. Readonly selection
works; disabled/composing, foreign-owner and stale requests are ignored.
Selection and replacement retain the ordinary editor/event-turn boundary.

Visual soft-wrap/bidi/character geometry, assistive scrolling/announcements,
VoiceOver and non-macOS acceptance remain open. Scoped verification passes
**244 view cases, 3 runtime assistive cases and 29 desktop GPU cases**, strict
affected-package GPU all-target Clippy and a GPU-capable Debug build. Seven
strengthened text cases pass again separately and are included in the view count.
The prepared CPU Debug bundle's native query timed out in the UI tool, so platform
range behavior remains unconfirmed.
[Pinned inputs, checks and limits](evidence/accessibility-text-ranges-2026-11-03.json).
These scoped results are separate from the preceding full-gate checkpoints.

**UI-2.3 — visual text geometry implemented; platform acceptance open.** The renderer retains shaped
cluster rectangles, source ranges, direction, visual lines and the content clip
for the same text used by pointer navigation. The borrowed frame API requires
matching source/content identities and refuses IME display. Native password
units now match the renderer's grapheme masks; legacy headless JSON remains
unchanged. CRLF textarea geometry and keyboard End keep the source pair intact.
The native projection now publishes visual/directional spans, clipped character
positions/widths and matching line links. Exact range keys prevent ID reuse for
changed portions after wrapping. Painted endpoint carets preserve affinity;
collapsed selections stay collapsed. The visual span cap has independent exact
limit/refusal/recovery cases. Scoped checks pass 90 field/editor cases before the
final caret refinement, then 16 final core cases and 35 desktop GPU cases;
overlapping cases are not additional unique tests. Strict affected-package GPU
all-target Clippy, formatting and a GPU-capable Debug build pass. The prepared
`Varta Showcase GEOMETRY Debug.app` query timed out after 20 seconds in the UI
tool; launch and OS range behavior remain unconfirmed.
[Source digest, artifact hashes and limits](evidence/accessibility-text-geometry-2026-11-03.json).
UI-2.2 results above do not verify this newer code. Assistive scrolling,
announcements and actual platform acceptance remain next.

**UI-2.4 — assistive scrolling and announcements, in progress.** Vertical
page/item/absolute scroll controls and nearest-edge element reveal are implemented
on acknowledged-frame geometry. Native physical offsets convert to CSS using
that frame's scale. The core validates source paths, field revisions and scroll
state; the worker keeps the existing full frame guards. Values/selection/focus
are preserved, scroll records run in later guest turns with no gesture grant,
and the established coalescible queue policy remains bounded at capacity.
Roleless overflow/live containers remain in native semantics while legacy JSON
is unchanged. Scope: 19 view cases, four runtime assistive cases and four final
desktop scroll cases pass; the 38-case desktop regression run precedes the final
DPI refinement. Logs: `target/a11y5-final-view-tests.log`,
`target/a11y5-runtime-tests.log`, `target/a11y5-desktop-tests.log` and
`target/a11y5-final-scroll-tests.log`. Strict/final broad verification remains at
the coherent batch boundary. This vertical checkpoint precedes the text-range
implementation below. The previous UI-2.3 report applies to its recorded earlier
inputs. Wave 3 (`w3-native-macos-adapters`) adds the macOS route for these page,
item and absolute actions through the pinned adapter; acceptance through a real
assistive client remains open.

Text-range reveal and single-line horizontal display panning now use acknowledged
shaped source ranges, opaque content revisions and per-axis edge hints. Own field
content scrolling precedes ancestor scrolling; logical selection/value/focus and
readonly behavior are preserved. Ordinary navigation/IME/real edits restore
caret following; equal guest echoes retain the reading position. Horizontal
panning adds no guest event or ABI field. Tests cover long fields, nested panes,
password grapheme masks, CRLF/trailing empty lines, stale/composing ranges and 2×
frames. Native text-run callbacks decode to semantic ranges before the FIFO and
survive matching content ACKs; removed IDs and changed content are rejected.
The final range checks pass 27 view cases and 25 desktop accessibility cases
(with GPU enabled); five runtime assistive cases cover later guest turns and
horizontal reveal with no guest event. Evidence:
`target/a11y6-final-view-tests.log`, `target/a11y6-final-desktop-tests.log`,
`target/a11y6-final-runtime-tests.log`, `target/a11y6-progress.json`.
The source snapshot is separate from the preceding vertical checkpoint; earlier
intermediate logs include failing fixtures and are not final verification.

Live-update implementation now preserves root/roleless busy semantics and
alert/status defaults without changing frozen JSON. A pinned macOS adapter patch
replaces per-node label announcements with one safe message plan per complete
publication. Atomic messages include the complete visible subtree, while separate
trigger channels determine when and at which priority they announce. Busy periods retain
fixed-size SHA-256 delivered-content fingerprints instead of old text. Ready content
is announced once, while cancelled, empty, removed and unchanged updates stay silent.
Nested live/off text remains in complete atomic messages but off changes alone
are silent; nested atomic owners handle their own triggers. Only changed channels
set a coalesced message's priority. Password values are masks
before projection and text-run children are not copied again. Geometry/selection
changes skip planning. The Showcase marks counter and async output as status
regions. Patch source, checksum, licenses and reviewable diff:
[`vendor/accesskit_macos/VARTA-PATCH.md`](../vendor/accesskit_macos/VARTA-PATCH.md).

The earlier 11 semantic/31 desktop checks and 59-case report precede the atomic
message correction. Review against ARIA 1.2 found that the first planner omitted
nested live/off text from complete messages. The initial broad run was interrupted
before completion (`target/a11y7-verification-aborted.json`); it is not a passing
full gate. The corrected planner also avoids persistent historical-text retention,
and preflight charges complete-message work within the unchanged observer
allowance. Native text folding and name-origin metadata also prevent lost text
under role overrides and duplicate derived names across semantic children;
legacy JSON folding is preserved. Native name/description references use the
same grapheme masks as painted passwords, while legacy names keep scalar masks.
Corrected scoped checks pass **14 semantic cases and 53 desktop GPU cases**;
affected view/desktop GPU all-target strict Clippy and formatting pass. Evidence:
`target/a11y8-final-semantic.log`, `target/a11y8-final-desktop.log`,
`target/a11y8-final-clippy.log`, `target/a11y8-progress.json`, and
`target/a11y8-vendor-provenance-final.json`. These supersede the earlier scoped
results for the changed live/naming inputs. The full batch gate follows these
corrections. A second read-only probe then exposed a partial-removal error:
removing B from an atomic A/B region announced A. The second broad run was
interrupted (`target/a11y8-verification-aborted.json`). Source-part fingerprints
now separate removals from relevant text changes, with owner/channel fingerprints
for net cancellation and ID-only order tracking. Fourteen live cases pass,
including partial removal, busy cancellation and mixed-priority reorder; the
standalone probe now passes. Logs: `target/a11y9-live-complete.log`,
`target/a11y9-removal-probe.log`, `target/a11y9-clippy.log`.
The 67-case checkpoint above precedes this final removal/reorder refinement;
the final complete desktop selection passes **55 cases with GPU**, and strict
all-target desktop GPU Clippy passes. The 12-input Showcase scenario passes on
the freshly built artifact. CPU Debug bundle:
`dist/Varta Showcase A11Y9 Debug.app` (106,556,627 bytes; not Release size).
The initial headless frame was inspected at 1024×768; it is not an OS screenshot.
Native app selection and even the UI-tool inventory both timed out after 30 s
with kernel reset. No matching app process was observed after selection; launch
and actual UI/VoiceOver behavior remain unconfirmed. No shell/AX automation was
used as a substitute. Evidence: `target/a11y9-progress.json`,
`target/a11y9-debug-bundle.json`, `target/a11y9-vendor-provenance.json`,
`target/a11y9-desktop-final.log`, `target/a11y9-showcase-tests.log` and
`target/a11y9-initial.png`. Consolidated verification kept sources unchanged but
failed the dependency-boundary test: it rejected the desktop's exact
`accesskit_consumer` test dependency. The rule now permits that consumer only in
desktop tests; AccessKit remains excluded from engine crates. Report:
`target/verification/20261104-005142-wa0_orw4/report.json` (failed, not a full pass).
The corrected boundary passes both dependency tests; the consolidated result follows below.
End-to-end native/VoiceOver acceptance remains open. At the `5a52125` checkpoint,
the pinned macOS range adapter only emitted node ScrollIntoView without a range
or edge hint; unit-tested text-run requests alone did not close platform range
acceptance. The final consolidated workspace gate passes **3281 tests**, with 16
ignored and all eight commands successful on unchanged inputs:
`target/verification/20261104-011619-8_yzsx6c/report.json`. This verifies the code
committed as `5a52125`; it does not close native range/VoiceOver acceptance.
A separate read-only probe also reproduced unchecked range-end addition in the
unchanged upstream macOS `from_ns_range` utility. Exact UTF-16 validation and
native visible-range routing were prepared separately under ignored
`target/a11y10-stage`, outside that gate and commit. Their application follows.

**Native range package after `5a52125` — implementation verified; platform acceptance open.**
Checked range addition and exact UTF-16 round trips reject
overflow and surrogate/CRLF interior endpoints before effects. The standard macOS
visible-range setter and ordinary ScrollToVisible route through typed source
ranges, preserving field selection, value, focus and acknowledged-frame guards.
Field owners advertise their own content reveal without requiring a scrollable
ancestor. The visible-range getter covers whole vertically visible lines,
including horizontally clipped text, per Apple's contract; ScrollView maps to
the public scroll-area role. The exact prepared source passed 66 CPU desktop
tests (including 11 new cases) and macOS SDK metadata compilation before application.
After application, normal Cargo verification passes **66 desktop GPU tests**,
strict all-target desktop GPU Clippy, formatting and the exact reverse-patch
check. Evidence:
`target/a11y10-stage/staged-inputs.json`,
`target/a11y10-stage/desktop-regressions.log`, and
`target/a11y10-stage/stage-check-exact-graph.log`,
`target/a11y10-desktop-gpu-tests.log`, `target/a11y10-clippy.log`, and
`target/a11y10-vendor-provenance.json`. The original overflow probe
fails on the extracted unchanged utility, while the new checks pass. This is an
implementation package verified by the final **3292-test full gate**:
`target/verification/20261104-015702-6g3mx2fb/report.json`, with all eight commands passing and unchanged inputs. This is separate
from the preceding 3281-test checkpoint. Actual native range/VoiceOver,
page/item/absolute native routes and other-target acceptance remain open.

**Native clipboard package after `2400dbf` — implementation gate complete; platform acceptance open.**
Host-only View/Application APIs validate source/content/selection and static
keydown prevention before returning an opaque copy/cut/paste task. Completion
runs between turns and rejects stale/replayed/foreign results; failed Cut leaves
the field intact. Readonly copy is allowed, password default copy/cut refused,
and ordinary editor history/UTF-16 maxlength rules remain. Paste normalizes
single-line and textarea line breaks. Physical shortcut mapping keeps non-Latin
layouts, with non-macOS Insert/Delete alternatives. Non-field keydown still
bubbles without I/O or select typeahead.

A text-only pinned arboard bridge runs lazily on one bounded desktop I/O worker.
The UI/application loops remain active and following keys retain their FIFO.
Existing field/input limits and guest capability/ABI/script meanings stay
unchanged. Dependency boundaries forbid clipboard SDKs in engine/runtime code.
A source binding is captured on a validated shortcut, without another per-frame
style walk. Deferred timing preserves initial acceptance and reports host wait
separately; completion-frame CPU samples are not whole-operation CPU totals.

Scoped checks so far: 15 core clipboard cases, six real Wasm/Application cases,
and 76 desktop GPU cases pass, including existing accessibility/geometry/input
regressions. The first Clippy run found helper items below a test module; they
were moved. Final strict all-target Clippy and both dependency-boundary tests
pass. The fresh CPU Debug bundle builds, and its 12-input Showcase script passes.
A new Cua app-selection attempt times out after 60 s with kernel reset; launch
and system clipboard behavior remain unconfirmed. Evidence:
`target/clipboard-core-final.log`, `target/clipboard-runtime-tests.log`,
`target/clipboard-desktop-gpu-updated.log`, `target/clipboard-clippy-final.log`,
`target/clipboard-boundaries.log`, `target/clipboard-showcase.log`, and
`docs/evidence/clipboard-checkpoint-2026-11-04.json`. SDK/OS transient allocation is outside
retained-response bounds. Current system clipboard/IME/VoiceOver and target
acceptance remain open. Final full gate: **3323 passed, 0 failures, 16 ignored**, all eight commands with unchanged inputs. Report: `target/verification/20261104-032530-xtwjknm6/report.json`; source SHA-256 `011b1246d98baaba320d4a7e85bf0b676c75779b3e03099e3cf279d2659d7ced`. Only documentation annotations follow the gate.

**Next native painting batch — open after clipboard commit `22b02d0`.**
Rounded corners and isolated group opacity remain required by the unchanged UARP
console, including 50% avatars, 999px pills and 8–16px controls. An outer-corner
resolver and contour are now connected to borderless static-box backgrounds
and their own hit regions. Px/rem/percentages normalize before large finite
magnitudes can overflow. Visible descendant boxes remain independently hittable;
rectangular ancestor clipping keeps the original contour. Four integration cases
cover 1×/2× pixels/hits, huge finite percentages, descendants, clipping and dynamic
radius changes with reference/retained/vector software equality. Six internal
corner cases cover geometry/path boundaries. At that initial checkpoint,
rounded borders/outlines, descendant clips, inline fragments/controls, opacity,
retained fallback and actual GPU/native acceptance remained open; the following
contour step implements the common-color box borders/outlines. The initial scoped
view run passes **50 internal + 238 integration**
cases: `target/paint-next-stage/applied-view-tests-fixed.log`. The first compile
attempt used a nonexistent convenience method in the new test; the corrected
public `rasterize_boxes` call passes. No full gate for these newer inputs is claimed.

The next contour step adds common-color rounded borders, including asymmetric
padding-edge curves clipped to their own rectangle, transparent holes and rounded
outlines. Very negative outline offsets keep the per-axis minimum visible shape.
A 100-configuration ellipse oracle checks over one million full-coverage pixel
locations; edge pixels use a safety square rather than treating AA coverage as a
binary center test. A diagnostic confirms the original failing edge has the same
raw/clipped result (aliased alpha 0; AA alpha 128), so the oracle was corrected
without changing production rasterization (`target/paint-next-stage/corner-alias-diagnostic.log`).
Before the final large-arc refinement, the affected view run passes **52 internal
+ 242 integration** (`target/paint-next-stage/border-outline-view-tests-coverage.log`).
Large radii then receive bounded adaptive tangent cubics: the exact final corner
family passes **nine unit + eight integration** cases, including a radius-500000
precision probe and 1×/2× reference/retained/vector software equality. Logs:
`target/paint-next-stage/adaptive-corner-unit.log`,
`target/paint-next-stage/adaptive-rounded-integration.log`. An independent
circle-cubic coefficient audit checks the exact radial-error extrema for all
four allowed subdivision counts (`target/paint-next-stage/arc-error-bound.json`).
Those overlap the earlier broad cases; they are not additional independent full
suite counts. Rounded descendant clips, controls/inline, different border-side
colors, isolated opacity, retained fallback and real GPU/native acceptance remain
open within the same batch. The initial contour compile attempts used an absent
helper method and a test-only unadmitted `box-sizing`; direct bounds comparison
and the existing content-box dimensions correct them without registry changes.

The next clip-chain step now carries rounded ancestor padding clips through
reference/retained/fallback painting, neutral `DrawClip` data and the desktop GPU
consumer, with per-region pointer clipping and proper outline/popup scope. The
mask cache reuses three viewport buffers independent of depth. Interior primitive
and dirty rectangles skip complex masks conservatively; unrounded trees allocate
no inherited-clip map entries. Tests cover SDK mask parity/cache resets, maximum
tree depth, empty masks, capacity fallback, nested clips, borders, siblings and
dynamic `overflow`. The final scoped view run passes **57 internal + 246
integration** cases: `target/paint-next-stage/clip-pipeline-view-tests-final.log`.
An earlier assertion incorrectly equated auto and hidden scroll metadata; it now
checks restored hidden state separately while preserving actual scroll policy.

The dedicated `gpu_clip_probe` passes **28 Debug Metal frames** on Apple M1 Max:
seven box scenes × two scales × cold/visible interior mutation. It preserves the
existing 2/255 mean threshold and disabled layer textures; observed mean RGB
delta is 0–0.1472/255, with maximum channel delta 68 at AA edges. CPU preparation
of these small fixtures is 0.561–4.169 ms cold and 0.280–1.070 ms changed; partial
damage is 64/144 physical pixels. These are one-sample offscreen preparation
observations, not window/guest/input-latency or browser comparisons. The first
probe had an invisible nested mutation; the fresh fixture corrects its layout
and asserts both changed pixels and nonzero partial damage. Evidence, source/
binary/image hashes and all samples:
`target/paint-next-stage/clip-pipeline-checkpoint.json` and
`target/paint-next-stage/gpu-clip-probe-visible.log`. The earlier GPU all-target
check passes (`target/paint-next-stage/clip-pipeline-check.log`); it precedes later
performance/probe refinements. Native field/assistive text bounds under curved
clips, inline/control content, isolated group opacity, side colors, actual window/
VoiceOver, Release and other-target acceptance remain in the open package. No
new full gate or code commit closes this intermediate step.

The subsequent text/control step now preserves full shaped advances and reveal
ranges while `FieldTextGeometry::visible_bounds` computes visibility under the
same curved ancestor/content clips. A bounded analytic convex-intersection
solver handles empty intersections and zero-width carets without viewport-mask
allocation. Native runs split when clipped vertical extents differ; source ranges,
password masks, selection affinity and revision guards remain intact. Preflight's
remaining observer allowance is carried through the whole desktop publication,
including all fields; work refusal clears old actions and recovers with fresh IDs.
Only the focused editable caret needs an IME visibility query. Rounded fields,
selects, buttons, checkboxes and radios now paint through the shared pipeline;
controls inset their content corners, while button visible overflow and popup
scope retain their CSS behavior. Immediate identical clips reuse one scope.

The stress fixture exposed a real stack abort inside the former recursive native
layout construction at an admitted native depth. Construction now uses bounded
enter/exit scheduling without increasing thread stacks or the existing native
128-depth/4096-box limits. A subprocess checks depth128 at the normal2MiB stack,
reference/retained/vector equality and depth129 refusal. Both older grouped
subprocess tests also needed qualified case names; they now assert that a child
actually executed one test, so the earlier grouped outer-case counts alone do
not establish that child coverage.

Current affected checks pass **64 View internal + 249 integration + 78 desktop
GPU-enabled unit tests**. The new control probe passes **52 actual Debug Metal
frames** (13 scenes × 1×/2× × cold/visible change) on M1 Max, retaining the old
2/255 mean threshold and GPU caps. Mean RGB delta is0–0.1472/255, maximum channel
delta68. Current one-sample CPU preparation spans0.623–6.284ms cold and
0.354–8.998ms changed; these remain fixture preparation, not GUI input latency.
Logs, source/binary/image hashes, samples and failed-fixture/stack evidence are
in `target/paint-next-stage/text-control-checkpoint.json`; authoritative logs are
`text-control-layout-view-tests.log`, `text-control-desktop-gpu-tests.log` and
`gpu-controls-layout.log` in that directory. Rounded inline decorations, distinct
border-side colors, isolated subtree opacity, complete capacity/platform
acceptance and real window/VoiceOver/Release observations remain open. This is
still the same painting batch; the last full3323 gate verifies committed
clipboard inputs, and no new full gate/commit or full-plan completion is claimed.

The next geometry increment shares generated closed line/cubic contour clipping
between the existing padding path and arbitrary diagonal half-planes. Projected
derivative roots split each arc before the bounded crossing search, covering
both two-crossing orientations without flattening. Hole components keep their
fill rule. Coefficients normalize before products and explicitly reject NaN/Inf;
monotone controls keep a fast path. The affected run passes **69 View internal +
249 integration + 78 desktop GPU-enabled** tests. A subsequent final five-case
helper run covers the strengthened dual-case assertions and explicit nonfinite
normal guard; it overlaps those counts. Logs and exact source/binary/image
fingerprints: `target/paint-next-stage/diagonal-clip-checkpoint.json`.

The expanded probe passes **60 actual Debug Metal frames**, adding oversized
asymmetric inner arcs and fractional edges. Mean RGB delta remains0–0.1472/255
at the unchanged2/255 threshold; maximum channel difference77 is retained in
the report. Current one-sample preparation ranges are0.580–6.175ms cold and
0.344–4.989ms changed. These are still common-color box/control fixtures and do
not prove mixed-color corner commands, group opacity, application/native input
or Release/cross-platform acceptance. The full painting family remains open.

A numeric corner regression rules out whole-ring nearest-normalized-edge color
assignment: Top/Bottom30, Left/Right0, TL radius90 on a100x100 border box wrongly
assigns Bottom to a fully covered pixel inside the TL transition. The staged
counterexample and next notes are in `target/paint-next-stage/mixed-border-counterexample.json`
and `mixed-border-next.md`. Colored AA pieces also need correct isolated coverage:
ordinary repeated SrcOver loses opacity at shared seams. The next implementation
must finish the side partition; the geometry helper alone does not complete the
mixed-color painter. The following isolated compositor supplies its coverage
foundation without claiming that the side painter is implemented.

**Isolated native box opacity — CPU implementation and affected checks; family
still open (2026-11-04).** Static boxes now composite their own background,
border, children, content and outline before applying opacity once. Nested groups,
root alpha and zero-opacity input geometry are covered. The flat program uses an
explicit stack and reusable128×128 pixel tiles, under9MiB of layer payload at the
unchanged maximum native depth including the root. That excludes source programs,
retained images and shared viewport masks; it is not a total-memory claim.
Global path coverage avoids tile-edge curve seams. Inherited rectangular/rounded
clips apply once to the completed group while raw clips retain text/hit/IME
geometry. The implementation includes isolated additive coverage for complementary
AA pieces; actual mixed border-side painting remains open.

The retained65536-command/16MiB limits stay fixed. An overflow uses a fallibly
allocated nonretained program derived from existing text-work/box/fragment ceilings,
preserves an already streamed prefix and following draws, and drops it after
rendering. No guest/layout/shaping replay or larger cache is introduced. Temporary
control-scope memo entries now retain their source allocations to prevent pointer
reuse from returning a different clip. A weak-reference regression verifies this
lifetime. Grouped `Renderer::prepare` preserves CPU scene/damage reuse: unchanged
frames rasterize zero dirty pixels, local mutations repaint only damage, and
switching between ordinary vector and group raster frames preserves pixels.
This remains explicit CPU fallback; the desktop disables GPU presentation on it
under the old resource contract. No GPU layer texture allowance is added.

The final affected selection passes **74 View library +260 integration +78 desktop
GPU-enabled** cases, including11
opacity cases, full-size fields and both cache-overflow paths, fractional coplanar
clips, rounded selection/IME geometry and maximum-depth opacity on a normal2MiB
thread stack. Log: `target/paint-next-stage/group-final-affected-tests.log`.
The preceding View-only pass is in `group-retained-prepare-view.log`.
The resumed earlier affected run first found two obsolete tests that still required
opacity refusal; those now assert the new alpha behavior and preserve genuine
unsupported-frame recovery via positioned painting. Its failures remain recorded
in `group-resumed-affected-tests.log`. Scoped results do not replace the full3323
gate for the older clipboard inputs. Mixed colors, rounded inline decoration/own
inline opacity, direct GPU groups, final family gate and real window/platform/
Release application acceptance remain open. Whole-plan completion is not claimed.

Final actual offscreen probes pass in Debug and Release on M1Max/Metal, each with
**60 GPU +8 explicit CPU-fallback** observations. Both keep the existing2/255 mean
threshold (observed max mean0.1472, max channel77). Unchanged group preparation
rasterizes zero pixels; observed single-sample times are0.098–0.204ms Debug and
0.015–0.050ms Release. Local mutations repaint64/144 pixels in those fixtures.
These are bounded host scenes, not application/window/input latency. Strict
all-target affected GPU Clippy passes. Final logs, code/binary/image fingerprints,
failure history and exact scope are recorded in
`target/paint-next-stage/group-opacity-checkpoint.json`.

**Mixed border colors — native CPU implementation (2026-11-04).** Distinct side
colors now paint through one premultiplied color field over the shared closed
border ring. The ordinary common-color vector path remains available. Point
ownership uses the inward-moving padding edges/arcs, including oversized inner
corners and exact zero-width donation; ordinary corner comparisons are analytic,
with a bounded solve only for opposing curved fronts. Independent inset-membership
and rotation checks validate the partition. Interval bounds prove uniform color
blocks; uncertain pixels retain a rotated16-sample pattern. This avoids repeated
source-over seams and an opaque under-paint, including unequal side alpha.

A real fractional-edge regression found that origin-floor/size-ceil rounding
omitted the last partial row. Pixel bounds now enclose all four edges separately;
the regression and a full common-contour alpha comparison at three scales retain
the original pixel checks. Retained mutation, raster/vector recovery, maximum
native depth with opacity plus a colored border, input metadata and source snapshots
remain covered. The private command adds one reusable128×128 RGBA tile and no new
guest operation, public drawing primitive, source allowance or cache/GPU ceiling.

The expanded offscreen probe has20 scenes:15 actually render on GPU (60 observations
per build), while opacity and three mixed-color scenes explicitly return CPU
fallback (20 observations). The three mixed-color scenes now render on GPU; see
[direct GPU color fields](#direct-gpu-color-fields--wave-3-package-w3-gpu-color-fields-scoped-2026-11-05)
for the current count. Source/fixture failures and the slow initial probe are
retained under `target/paint-next-stage/`. At that committed checkpoint, Debug mixed-border preparation
is still slower than the common-color path, especially opposing large corners;
no general smoothness or whole-application latency acceptance is claimed. Optimize
that path further with measured evidence; direct GPU fields/groups, rounded inline
decoration/own opacity and platform/application acceptance remain open. This extends
the painting package for the requested commit, not full-plan completion.

The final complete gate passes **3393 tests, 0 failures, 16 ignored**, all eight
commands on unchanged inputs (`target/verification/20261104-083445-nlyqugwi/report.json`;
source SHA-256 `4bfd91cb3b2fe8c9b96ed662c3bf443fb38476af01c9b4f0ac465a6f21edfe92`).
Final View coverage is80 library +265 integration cases; final optional GPU Clippy
and both20-scene probes pass. `native-painting-precommit-checkpoint.json` in
`target/paint-next-stage/` retains exact inputs, binaries, images and scope.
Mixed-field preparation at that checkpoint spans7.084–66.547ms Debug and
0.353–7.616ms Release in those single-sample scenes. The scoped CPU follow-up
above records the subsequent performance work, committed in `ed77739e`. No general UI smoothness or
whole-plan completion is inferred. Only documentation annotations followed that
gate before commit `54585e3f`; the later optimization has its own scoped evidence.

Commit `54585e3f` groups the 25 view integration case files into one declared
Cargo target, retaining per-file modules and adding a missing-file registration
guard. Standalone comparison proves the same 233 original cases and passes all
of them in one process (9.7 s including startup on this host). This reduces repeated
loader starts without removing cases. The applied Cargo grouping passes 234
cases before painting integration, then 238 with the four new painting cases.
Ordinary module declarations preserve recursive formatting; the registration
check reads those declarations and catches omitted case files. The
preceding 3323-test full gate verifies the clipboard package, not these newer
build/painting inputs. No additional full gate is run for this intermediate step.

**Renderer batch based on `1259eb8`; 2026-11-03.** This batch includes renderer
and pointer changes, new Rust modules/tests, the Showcase and documentation.
The consolidated gate passes **3202 tests, 0 failures, 16 ignored**, all eight
commands with unchanged source inputs. Evidence: `target/verification/20261103-182413-ec8q70t8/report.json`.
The optional GPU graph has a separate passing all-target Clippy run and desktop
test run; 52 real Metal offscreen cases and 162 browser Node cases pass. Only
documentation and measurement annotations changed after verification. These
checks close this implementation batch, not the broader P10/P13 target matrix.

- Retained CPU rendering caches style reads, shaped text, glyph outlines and
  bounded draw commands, replays damage and reuses packed RGB pixels. Pointer
  hover/active and cursor feedback preserve stale-input protections; motion is
  coalesced separately from activation and keyboard input. Immediate typing waits
  for the focus frame. Debug assertions remain enabled; graphics dependencies
  and the small desktop host have targeted dev optimization.
- Recorded scoped checks: **43 tests passed** across matcher/cascade/expansion,
  pointer, renderer/input, desktop and runtime selections; strict affected-package
  all-target Clippy passed. Evidence: `target/hover-{view,matcher,desktop,runtime}-tests.log`
  and `target/hover-clippy.log` (local, ignored). These logs predate subsequent
  example-only edits and are not a workspace-wide result for all current files.
- Latest external-CSS Showcase build and **12 trusted-input assertions** passed:
  `target/showcase-css-dev-{build,verify,package}.log`. The Dev app contains that
  artifact; earlier Debug/Release bundles are not automatically the latest build.
- The pinned TSX pointer benchmark has **192 reference-equal frames**. At 2×,
  historical Debug/Release median frame preparation is 8.58/1.66 ms for pointer
  feedback, 6.68/1.38 ms for the counter and 16.85/10.23 ms for scrolling.
  [Raw data](evidence/tsx-pointer-performance-2026-11-03.json)
  identifies the measured Wasm and hosts. It predates the CSS extraction/browser
  font configuration and excludes input turns, startup, presentation and pointer
  sampling wait; it is not input latency or a current Chrome/native speed ratio.
- The official browser dev host runs the same typed example with an explicitly
  packaged Arial. Chrome checks cover counter/step/checkbox, Ukrainian editing,
  Undo/Redo, Tab, select and JSON/generator/async output. The native output agrees.
  The initial browser body margins/indefinite root height and control
  content-box mismatch are corrected by UI-1 below; at that checkpoint native
  controls lacked OS accessibility exposure. UI-2 above records the later foundation. This is a
  scoped macOS/Chrome observation, not cross-platform acceptance.

**UI-1 — committed in `be85c981`; broader target acceptance remains open.** Normalize
root viewport and admitted control box sizing between the official browser host
and native preview; add comparable input-to-present instrumentation and exercise
counter, hover, editing and scrolling in Debug/Release. Preserve packaged fonts,
CSP/resource checks, budgets and stale-frame rejection. Exit: regression cases
for geometry at matched widths and the responsive breakpoint, the existing
12-action scenario, and a report separating guest execution, frame preparation
and presentation latency (with measurement limits stated). Do not assume that a
submission timestamp proves actual screen presentation. Working changes now
normalize standalone browser root geometry and admitted control border boxes,
authorize the fixed page stylesheet with an exact CSP hash, add bounded opt-in
native/browser timing and wake the native worker on input/presentation rather than
waiting for its idle timer. Focused acceptance passes 162 browser Node tests, the 12-action Showcase
scenario, vector/reference control tests and the optional GPU Clippy selection.
Real browser root bounds match the viewport at 800/720/700 CSS px. Native and
browser timing boundaries are explicitly different; physical scanout is unmeasured.
The final consolidated gate passes 3202 tests. Matched CPU/GPU Debug/Release
window timing, resize/visibility and native accessibility acceptance remain open;
native UI automation timed out while attempting window activation.

**P13a GPU prototype — implemented; local acceptance in progress.** The optional
desktop feature pins wgpu 30.0.0/Vello GPU 0.3.0/pollster 0.4.0. Shared scene
preparation supplies the same layout, shaping and input metadata without CPU
rasterization; bounded neutral path reuse avoids repeated conversion. Windows
present directly with one outstanding submission. Device/validation/surface
failure recovers to CPU without recreating Wasm; a forced Metal device loss was
observed preserving a changed counter and subsequent Ukrainian input. Explicit
macOS Display P3 tagging and sRGB-primary conversion correct wide-gamut colors.
Default builds/backend remain CPU. Debug and Release each pass 26 real Metal offscreen frames at 1×/2×, with
exact input geometry and mean RGB delta 0.187–0.477/255. Release window samples
(n=56) have preparation median/p95 1.65/4.46 ms and queue-to-present-return
6.13/19.80 ms. One host-entry-to-first-present-return sample is 1.89 s. These
small local observations exclude physical scanout and do not establish a Chrome
speed ratio. Stripped Release bundles are CPU 24,065,252 bytes and GPU 28,574,180
bytes. [Pinned measurements](evidence/gpu-ui-checkpoint-2026-11-03.json)
identify inputs and binaries. Total GPU-memory accounting, broader target
acceptance remain open. The consolidated final gate passes 3202 tests. P13b/P13c follow
only after their contracts and source admission work; they do not delay closing
ordinary UI gaps. Accessibility, richer painting/overlays/images, clipboard/IME,
the unchanged UARP session and target packaging remain explicit P10 work.

Development continues in coherent batches with focused checks during coding and
one consolidated full gate at the completed code-batch boundary before a code
commit, following [verification](verification.md). Documentation-only changes do
not trigger another engine suite. Do not replace the historical reports or
claim a passing full gate until it actually runs on the final inputs.

### Stabilization batch 1 — verified 2026-11-01

Implemented on the working tree after `abea3ac`: JSON record property order in
shared and compiled builders, and required-boolean property truth narrowing.
The final full gate passed **2815 tests, 0 failures, 16 ignored**; all eight
commands passed and the source fingerprint stayed unchanged. Conformance remains
123 passed and 4 known unsupported, with no regressions. Node host: 117 passed;
normal-stack UARP console: 107 passed in 28 groups and two headless view checks.
The six new tests cover decoding/fallback/GC, recursive/wide records, safe
boolean narrowing and source-free Wasm/native packaging. Deterministic fuel
baselines remain unchanged. Two historical comparisons now acknowledge the
explicit recursive-JSON ordering difference between v37 and v38; older profile
bytes and their own expectations are preserved.

Evidence: `target/verification/20261101-174930-muu4a2lf/report.json` (local,
ignored). Only documentation result records were updated after that gate; code
and build inputs were unchanged at that record. That batch was verified before
direct-union implementation began and is included in `f38413f`. No whole CORE
work stream or broader platform acceptance is closed here.

### Stabilization batch 2 — verified 2026-11-01

Direct literal-constrained `JSON.parse(text) as T` decoding is implemented for
eligible records, discriminated unions and arrays. It preserves literal constraints
before erasure, probes required tags, validates the chosen record once, and builds
without an intermediate guest unknown graph. Conservative misses run the original
parse/assertion; the input is evaluated once. Public literal-bearing
`varta.json.parse<T>` admission and host/schema/value ABIs are unchanged.

Nine focused runtime tests and one CLI artifact test cover errors,
recursive/nested values, async input, GC/tape growth, seeded mutations, imported
types, non-schema target admission and source-free Wasm/native execution. Existing decoder, historical-profile,
codegen and exact fuel baselines pass. Measured last-tag cases consume 69,707 vs
102,176 fuel (2 variants/128 numbers), 71,660 vs 105,810 (32/128), 297,330 vs
407,976 (32/512) and 81,307 vs 125,167 (200/128), compared with only direct
assertion decoding disabled. Shared tag scanning meets the existing dispatch
ceilings unchanged: 12 vs 7 fuel per preceding candidate at 24 vs 200 members.
These measurements do not establish general throughput.

Final local gate: **2825 passed, 0 failed, 16 ignored**; all eight commands passed
and the source fingerprint stayed unchanged. Evidence:
`target/verification/20261101-192403-a66359j5/report.json` (local, ignored).
Conformance remains 123 passed and four known unsupported, without regressions.
Fresh Node host: 117 passed; normal-stack UARP console: 107 passed in 28 groups
and two view checks. The unchanged dispatch-cost regression now passes at both
24 and 200 alternatives. Only result documentation was edited after the gate;
code/build inputs were unchanged at that record. This batch is included in `f38413f`.

This batch also corrects the reproduced literal/unknown discriminant overlap:
valid later record alternatives use strict matching, while multiple successful
matches report ambiguity. Older profiles keep their behavior. Further narrowing
inventory and the server/platform/release queue stay open.

The server/asynchronous lifetime batch follows below. The wider compatibility
inventory and platform/release queue continue; no whole CORE stream is closed
by these two stabilization batches.

### Server batch — native acceptance verified 2026-11-01

HTTP contract v2 now admits synchronous and async UTF-16/binary64 adapters in v38. The
wire envelope remains bounded UTF-8 JSON; guest strings are transcoded explicitly,
response status still requires a valid integer, and metadata/profile/value-layout
checks reject v1/v2 substitution before execution. Each request still creates a
fresh bounded Store; listener ownership, grants and shutdown remain independent.
The launcher advertises the supported HTTP maximum while old contracts remain v1.

Focused checks cover Unicode requests, thrown/fuel-failed handlers, invalid
fractional statuses and later successful requests, version/profile mismatch and
legacy HTTP paths. Async handlers reuse normal async lowering and the existing
scheduler until their response Promise settles, with pure state/value exports
checked before execution. Module values initialize once in each request Store;
startup's main is not rerun and state is not shared between requests. Sync v2
handlers also run their microtask checkpoint before replying. Required effects
come from the emitted direct-call graph, including the closed resume dispatcher.

Focused async cases pass with timers, a forced-collection decoder build, real
outbound fetch/body completion, and denial without a socket. Rejections and idle
pending responses fail only that request; background intervals are discarded at
response completion. Tampered completion helpers and effect catalogs are rejected.
Effective startup budgets/ceilings and headless permission inputs now carry into
every request. Fresh counters do not renew across async tasks; the fixed HTTP
handler deadline caps a larger application allowance. Listener consent is checked
before bind as well as at guest operations. Optional/stored/artifact-scoped
permissions and ceilings are tested before socket effects. Workers retain bounded
v2 output before failure and deliver one diagnostic for each failed invocation.

Focused server regressions and the sustained sample: 29 passed, including independent fuel, memory,
host-call and output limits from package requests and host ceilings; aggregate
async fuel/deadlines; the fixed contract deadline; optional listener consent;
headless decisions; v2 failure output and unchanged legacy budget semantics;
disconnect cleanup, active-response shutdown, idle-connection shutdown and real
hostname resolution. V2 request owners now cancel guest loops and native
timer/async-network waits. Graceful shutdown drains an active reply under its
original deadlines, closes idle connections and never invokes a later handler.
Process-owned bounded system DNS workers retain no Store and cannot delay request
runtime destruction. Queue expiry/cancellation, late answers and oversized
answers have focused verification. A running OS lookup may finish later within
that fixed pool. Synchronous `net.get`/`net.fetch` inside v2 requests now use the
same owned transport without running guest jobs or renewing allowances. Disconnect
interrupts response-head and body waits, closes sockets and releases the worker;
neither guest try/catch nor result capture can recover cancellation/deadline
failure. Status-only fetch, raw bounded get bodies, redirects not followed,
hostname resolution and denial before socket effects have focused verification.
The previous synchronous transport remains in contexts without an owned token;
frozen legacy/v36/v37 bytes/diagnostics and execution compatibility checks pass.
Scheduler (9), timer (18), network (18), DNS (5) and execution cancellation (1)
regressions also pass. V2 workers now close a usage record on success, failure and
cancellation, then publish owned usage/audit evidence after resource release.
`RunReport::http` keeps worker totals and a bounded prefix separate from startup;
the observer receives every worker report independently of aggregate truncation.
A real repeated-request case exceeds both retained-detail and aggregate-audit
bounds while preserving totals. A source-free async HTTP case passes in dev,
Wasm through CLI/companion, and native self-execution, including Unicode, failed
request isolation and streaming `--budget-report` output. An isolated native
resource sample passes: after eight warmup cancellations and forty measured
cancellations, open descriptors remain 11 -> 11 and every cancelled request has
the same 1,966,080-byte guest memory peak. Each holds a live string/frame across
outbound I/O, closes the real upstream socket and permits a later healthy request.
This verifies the sampled descriptor/guest-memory lifecycle, not native RSS or
all-target leak behavior.

Explicit `wait_network` view-script steps now wait under a validated wall-clock
bound without advancing virtual timers, changing instance lifetime or shortening
surviving response deadlines. Timeout also caps guest continuations and idle
maintenance, terminates before later inputs and releases I/O. Focused runtime and
CLI checks cover streamed Unicode bodies, policy denial, timeout, an infinite
guest turn, later body reads and invalid direct/script inputs. Real completion
order/content remain external; recorded or simulated network replay stays open.
Hostile source-free artifacts also pass focused acceptance: wrong HTTP version,
omitted effects, duplicate metadata, a missing completion export, substituted
state signature and an unknown helper are rejected across CLI/companion/native
validation, inspection and execution before startup effects or bind. The intrinsic
library has been rebuilt and its source fingerprint is current. The final full
gate passed **2871 tests, 0 failures, 16 ignored**, with all eight commands and an
unchanged source fingerprint. Generated embedding/architecture checks pass for
the new server registries; conformance remains 123/127 with four known gaps and
no regressions. Evidence: `target/verification/20261101-215315-99yy5gon/report.json`
(local, ignored). This closes the native server batch's stated acceptance, not
the broader CORE, replay, native RSS or cross-target release work.

Browser delivery now has two separate strands. The documentation-built prototype
keeps its 117 passing Node cases and remains unmetered. The official registry
foundation is generated directly from Rust by `varta_common::browser`: operation
names/signatures, profile admission and entry-point versions, value/frame acceptance,
fair host fuel rates and private meter names. Its checked-in frozen ES module has
a regeneration drift test; the iterable operation inventory comes from the enum
itself and is checked against every wire-name arm and the embedding tables.

The compiler's private fuel transform now runs with the shared JS `FuelTank` in an
explicit fixed-fixture probe. **25 native/Node and 25 native/Chrome observations
match**, including both sides of call/byte-work payment boundaries, exact payment,
remaining fuel, logging positions and Unicode output. Local engines: Node
24.20.0 and headless Google Chrome 156.0.8078.74, macOS arm64. The one fixture's
private image grows from 178,264 to 286,321 bytes; this is not a size/startup survey.
The probe is verification tooling for trusted compiler output, not an application
loader or permission validator.

Focused evidence: common library 167 passing tests, 11 embedding table checks,
4 Node tank/registry checks and 12 meter differential/rejection/resource tests; compiler
all-target Clippy and the no-std common core check pass. The earlier compiler
library checkpoint passed 161 tests with 2 ignored. New browser Rust work remains
outside the historical native-server full gate.

Trusted preparation is now implemented in
`varta_project::browser::BrowserArtifact`: original manifest/code ABI and complete
native module/catalog/helper-effect validation precede instrumentation. The owned
result preserves original bytes, metadata, requested/ceiling policies and resolved
budgets. Its private image applies memory/table maxima equivalent to native Store
limits; smaller declared maxima remain smaller, and an initial allocation over
the ceiling fails. Empty ceilings stay explicit. Preparation does not invoke guest
code or mount native filesystem roots. Focused malformed-import/reserved-marker,
policy ownership, legacy-budget and capped-resource cases pass. Project checks pass 19 library tests
(including six browser-preparation cases) and two dependency-boundary checks;
compiler/project all-target Clippy passes. An additional malformed-memory64 case
pins rejection before byte-size arithmetic, without changing supported memory32
execution. The bounded-image fixture still matches native under Node and Chrome.

The console/microtask bundle now has a source-free library delivery
path (`BrowserArtifact::bundle`/`BrowserBundle::write_new`), with original/private
Wasm, manifest, self-contained Worker/loader and an SRI/CSP/Trusted Types page.
Pinned SHA-256/length checks precede Worker creation; the parent watchdog and
AbortSignal stop a running instance and retain prior logs. Host policies are
never embedded: bundling a preparation with a host ceiling is rejected, while
trusted runtime budget ceilings are separate execution inputs. Memory ceiling
changes at JS loading currently reject rather than pretending to enforce them.
Promise/async jobs now run through the bounded checkpoint; other unsupported
effects fail at bundle construction. Timer support is added below.
The shared workspace adds a pinned base64 declaration and project adds direct
SHA-256/base64 dependencies for loader SRI; both versions were already locked,
and no engine dependency direction changes.

Local real Chrome acceptance now passes **34 bundle cases**: native output (including
BOM/lone units and chunk-boundary surrogate pairs), guest exceptions, fuel/call/
output limits, a deadline inside running Wasm, pre-start/active cancellation,
four invalid budget ceilings, four same-length file substitutions and two
length violations. Async/rejection cases additionally compare exact remaining fuel, host-call,
microtask and output-byte counts with native. Evidence: `/tmp/varta-browser-scheduler-chrome-v3.log`; the
reproducible commands are in [verification](verification.md#browser-registry-and-fuel-probe-development).
These cases do not establish a complete official browser host or all-target pass.
Project tests now pass 24 library cases and two dependency-boundary checks;
Clippy is checked at this focused boundary. New browser/dependency work remains
outside the historical native-server full gate.

The console bundle is now exposed through `varta build --target browser`, using
the checked source pipeline without mounting native execution roots. Output is
`web` beside the configured `out` prefix; existing bundles are never overwritten.
Explicit `--target wasm` and `--target native` retain the portable/verified-launcher
paths. Six focused CLI tests pass, covering async checkpoint inclusion, original byte/manifest equivalence,
source-free validation, refusal to overwrite, gap/type failures before output,
unmounted roots, flag/policy rejection and native companion execution. The
companion build and CLI all-target Clippy pass. This extends the focused browser
checkpoint, not the historical native-server full gate.

Browser Promise/async support is implemented through a closed resume dispatcher,
FIFO reactions and a single shared allowance across `main` and the checkpoint.
The bounded rejection reader snapshots primitive, Error, subclass and BigInt
reasons; late handling, retained/not-retained reasons, decimal numeric diagnostics
and large fields match native in the real-browser fixtures. A related native fix
uses profile string limits for Error and subclass fields instead of reapplying the
legacy limit in v38; field-prefix copying remains bounded. The older profiles'
limits remain unchanged.

Focused evidence for this increment: 34 real Chrome cases, eight Node scheduler/
reader boundary tests, five registry/layout drift tests, 11 browser project tests,
13 native Promise tests, two native exception-reader tests and six CLI cases.
At that focused checkpoint the broader full gate was pending; the consolidated
browser-core gate recorded at the end of this plan now covers the Rust batch. A separate local numeric
probe matched 4096 seeded binary64 diagnostics against native Rust formatting;
this was exploratory evidence, not a CI suite or exhaustive numeric proof.

General browser artifact admission and permission/effect integration, layout/view code generation,
other effect-specific fair charges, start-function charges, long-lived Worker
turns, browser dev integration, Firefox/WebKit and full corpus/UARP acceptance
remain open.

The explicit text shaper now accepts a bounded collection of named fonts, rejects
ambiguous aliases and resolves CSS family lists to that collection. Parley can
fall back between supplied fonts for missing glyphs, without system discovery.
The mixed Cyrillic/Latin advance and flex position agree with Firefox, WebKit and
Chrome (`/tmp/varta-glyph-fallback-layout-matrix.log`). Broader script, real font,
weight/style and platform fallback acceptance remain open.

The explicit font collection now permits multiple verified faces of one family.
Supported CSS font weights and normal/italic style inherit into shaping. Synthetic
normal, bold and italic faces with different advances match Firefox, WebKit and
Chrome (`/tmp/varta-font-faces-matrix.log`). The constructor rejects a face whose
declared family does not match its bytes. Real-font matching, synthesis, variable
fonts and broad platform typography acceptance remain open.
The browser package is still in development; the focused Chrome result does not
establish general browser budget conformance.

Server acceptance ledger, in order:

1. Implemented with focused verification: `Execution::request_setup` snapshots
   effective budgets/caps and headless broker inputs; fresh request executions
   inherit them without widening grants or renewing allowances. Bind authorization
   also applies permission states. V1 keeps its legacy budget/consent semantics.
2. Implemented with focused verification: v2 disconnect cancellation reaches guest
   loops and native timer/async-network waits; shutdown drains the active response
   under existing deadlines. Guest/network resources are released before output
   callbacks; process-owned DNS cannot retain a Store or block its destruction.
   Bounded request usage/audit now survives success, failure and disconnect in
   `RunReport::http` and `RuntimeEvent::HttpRequest`; full observer reports and
   bounded aggregate details have independent retention, with explicit omissions
   and complete worker totals. Owned v2 requests also cancel synchronous legacy
   network imports under the same deadline, without weakening their grants or
   making cancellation recoverable. A warmed, isolated repeated-cancellation sample
   verifies closed sockets, stable descriptor count and fixed per-request guest
   memory; broader native RSS/target/lifetime acceptance stays open.
   Bounded v2 output/diagnostics also survive
   failed calls through worker-owned delivery, independent of transport delivery.
3. Implemented with focused runtime/CLI verification: explicit bounded
   `wait_network`, frozen virtual timers, preserved lower execution bounds and
   surviving transfer deadlines, fatal timeout/late-input prevention, and owned
   cleanup. Real I/O is explicitly outside deterministic replay guarantees.
4. Source-free async CLI/native, hostile-contract artifacts and generated
   embedding/architecture checks pass; the rebuilt intrinsic fingerprint is
   current. Completed: all eight full-gate commands pass for the native batch,
   with an unchanged source fingerprint. Next delivery focus: browser batch 3.

### Next delivery batches

This table preserves the broader accepted queue. The GC/fuel comparison
follow-up now passes its consolidated gate. Next, run the Chrome streaming
acceptance with the new inactivity contract. Continue the agreed
[K7 server stages](#k7--modern-http-server-agreed-design-2026-11-05) and real-window
console acceptance from their implemented foundations. The former UI-1/P13a-first
proposal is historical. Each batch still requires its own exit evidence; local
benchmarks and compatibility checks establish only their measured scope.

| Order | Batch | Exit criterion |
| --- | --- | --- |
| 1 | V38 semantic stabilization (CORE-01/02/10/15): JSON property insertion order and boolean narrowing verified in batch 1; direct discriminated-union decoding and literal/unknown overlap verified in batch 2; wider narrowing inventory remains | Differential cases preserve observable ordering and errors across compiled/linked decoders, serialization and source-free artifacts; existing profile and fuel exceptions remain explicit |
| 2 | V38 server and asynchronous lifetime integration (CORE-08/09/11/17): streaming responses and HTTP/1.1 keep-alive implemented; K7 H1 partial, H2–H4 open | Complete Web Request/body and policy admission, then async execution/pooling, server TLS 1.3/HTTP/2 and hardening/load evidence while retaining request isolation, permissions, budgets and cancellation contracts |
| 3 | Official browser delivery (P1/P9): metered bundles/effects, OPFS KV, replay, dev/reload and the browser oracle driver integrated; remaining streaming/pilot/target acceptance | Same supported artifacts and UARP session agree in Node, Chromium, Firefox and WebKit; fuel and host work are enforced, permission denials tested, bundle/startup costs measured |
| 4 | Native targets (P3/P10–P10d): renderer, shells, bundles and headless layout comparison | Same console artifact completes the accepted session visibly on macOS and an actual Pi 5, with layout/accessibility agreement and measured startup, memory and input latency; Windows and target metadata have independent checks |
| 5 | Native GPU and graphics (P13a–P13c): UI prototype/color fields implemented; opacity groups, device acceptance and guest graphics/3D contracts open | Measured CPU/GPU Showcase prototype, then bounded game graphics and separately proven typed library/port; no Three.js or WebView compatibility inferred from GPU support |
| 6 | Release acceptance (CORE-14/15/16): explicit v38 subset, package compatibility and sustained/security validation | Reproducible target matrix, documented unsupported inventory, package-own-test results, long-lived resource measurements and release/trust policy; no production or full-compatibility claim before these pass |

Work on performance fast paths, wider standard APIs and KV async/watch/queues
continues against measured blockers alongside these batches; it does not close
unrelated acceptance. MCP, hooks, Svelte and general application-state/tool
contracts retain their deferred scope. See the detailed [platform increments](#increments-and-acceptance)
and [wave-3 open items](#wave-3b-integration-open-items).

## Review correction batch — 2026-11-01

Four reproduced review defects are addressed together: native compiler stack
exhaustion on generated conditional spines, unbounded table growth in the JS host,
unsupported thenables silently fulfilled as ordinary values, and fractional timer
delays diverging from the stated Node normalization. Regressions cover normal
CLI check/dev and source-free artifacts, static and erased resolution paths,
record presence and class members, typed/unknown dictionaries, timer ordering and
JS validation boundaries. Six v37 programs (five network cases and the SSE case)
intentionally gain resolution guards. A per-file comparison with `b2a36e0`
confirmed the other 127 programs unchanged; v36 and legacy aggregate baselines
and measured fuel numbers retain their values. These are corrections, not new completed CORE
work streams or a general thenable compatibility claim.

Verification follows the scoped/full workflow: focused regression and historical
profile/fuel suites at the completed boundary, the actual UARP console on the
normal stack, and the shared Node/browser-host differential corpus. The final
full workspace gate and source fingerprints are recorded under
`target/verification/`; no broader platform result is inferred from local tests.

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
production native GUI shells, hardware adapters and full JS/npm compatibility
remain open. An experimental CPU desktop preview is implemented. KV storage is
implemented; broader database APIs remain open. HTTP listeners are independent of
the existing outbound client.

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

| Area | Target | Implemented scope and remaining gap |
| --- | --- | --- |
| Language semantics | ECMAScript values, numbers, strings, objects/arrays, functions/closures, errors and control flow | Opt-in binary64/UTF-16, typed graphs, classes/generics, collections and structural views work. General prototypes/descriptors, coercion, symbols and full call semantics remain open |
| Memory | Bounded ownership and exact tracing for application graphs | Exact roots and v38 incremental/idle GC cover admitted graphs and suspended frames. Sustained lifetime, cancellation, peak-memory and leak measurements remain acceptance work |
| Scheduling | Event loop, Promise jobs, async/await, timers, cancellation and shutdown | Bounded microtasks, timers, network completions, application turns and native v38 async HTTP adapters work. Unsupported structural thenables are rejected; generalized cancellation/shutdown, replay and cross-target lifecycle acceptance remain open |
| Standard APIs | JSON, URL, encoding/binary, Request/Response/Headers, streams and abort | Checked/unknown JSON, compiled decoders, linked serialization, fetch/Web bytes, URL, abort, selected collections/Date/RegExp work. Full standard-library and binary/crypto/Intl surfaces remain partial |
| System APIs | Capability-checked files, environment, processes and networking | Files with v38 virtual roots, outbound async HTTP(S), native v38 sync/async HTTP adapters and KV work. Wider process/filesystem APIs and complete cross-target effect coverage remain open |
| Modules/packages | ES modules, explicit resolution/loading policy and tested packages | Local typed graphs, constants/live bindings, re-exports/defaults, named types and text resources work; pinned pilot checks. General module initialization/loading and package-own-test acceptance remain open |
| Distribution | Independent runtime, portable Wasm and target-specific packaging | Source-free Wasm and single native executable work on the build target; official browser bundles and experimental desktop preview work within their checked surfaces. Production native GUI shells, cross-target releases and authenticity remain open |
| Verification | Conformance, boundary/security tests and resource/performance measurements | Workspace and partial checked conformance pass at the checkpoint above. Upstream Test262 execution, complete feature inventory, sustained target workloads and independent security review remain open |

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

| ID | Work stream | Current state | Implemented through v38 | Remaining acceptance work |
| --- | --- | --- | --- | --- |
| CORE-01 | Number, String and primitive semantics | Partial | Binary64/UTF-16, BigInt graphs, primitive coercions and checked numeric/string methods | Symbol, object ToPrimitive, wrappers/prototypes/descriptors, locale/Intl and full operator/conversion inventory |
| CORE-02 | Objects, arrays and value graphs | Partial | Sparse/resizable arrays, dynamic-key dictionaries, classes/generics/private fields, structural views and checked graph conversion | General dynamic properties/descriptors/prototypes and remaining identity/key-order semantics |
| CORE-03 | Functions and control flow | Partial | Typed functions, closures, control flow, generators, async functions and async generators | Complete receiver/binding/call behavior, general iterables and remaining generator forms |
| CORE-04 | Exceptions | Partial | Typed exceptions, Error subclasses, unknown catches and rooted propagation across suspension/finally | General thrown-value admission and complete error-family/propagation inventory |
| CORE-05 | Standard collections and binary values | Partial | Map/Set, tuples, Uint8Array, encoding/base64 and v38 Web byte APIs | Full typed-array/ArrayBuffer/DataView contracts and ownership/interop inventory; general tagged collection values |
| CORE-06 | Math and remaining language built-ins | Partial | Checked Math inventory, the v38 complete Date in the UTC zone, ES2025 Array/Set/iterator/JSON library methods (K9 D) and ahead-of-time RegExp | General Math object/call semantics, host time zones, Intl, Symbol, weak references and remaining RegExp features |
| CORE-07 | Garbage collection and resource lifecycle | Partial; large-heap idle-work fix verified | Exact tracing, liveness frames, incremental collection/idle maintenance and bounded bitmap clearing before the root snapshot under v38 | Sustained retained-root/leak/cancellation validation, collector tuning and target peak-memory evidence remain open |
| CORE-08 | Event loop and promise jobs | Partial; implemented checked scheduler | Bounded FIFO Promise jobs, adoption, microtask checkpoints, timer/network tasks and generation-checked tokens | Structural thenable support (currently rejected), remaining scheduling semantics and cross-host ordering beyond supported network replay |
| CORE-09 | Async language and cancellation | Partial | Async/await, timers, abort listeners/timeout/any, async generators, for-await, cancellable fetch and focused v38 HTTP lifecycle checks including synchronous network waits | Top-level await, wider async iterable forms, broader host-call lifecycle and sustained owned shutdown acceptance |
| CORE-10 | JSON and Web APIs | Partial | Typed/unknown JSON with verified record insertion order, compiled decoders and direct literal-constrained assertion attempts, linked serialization, URL/Headers/Request/Response, streamed bytes | Wider Web compatibility and remaining checked conversion/narrowing semantics |
| CORE-11 | System APIs | Partial | Capability-gated files/env/time/process operations, virtual roots, verified native v38 sync/async HTTP v2 and artifacts, async HTTP(S) client and KV | Broader filesystem/process APIs, KV async/watch/queues, portable errors and target parity |
| CORE-12 | ES modules and packages | Partial | Typed graphs, constants/live bindings, re-exports/defaults, class/generic linking and text imports; v38 bare package specifiers resolved through `<config dir>/node_modules`, the `types` export and the `.d.ts.map` to shipped TypeScript sources ([contract](architecture.md#local-modules)); the pinned SDK checks when imported as `uarp-sdk` | Full module initialization/loading semantics, `*` export patterns and other resolution policy expansion (owner decision), SDK-own-test/package acceptance |
| CORE-13 | ABI and capability enforcement | Implemented for admitted native profiles | Version/signature/layout/resource checks, host ceilings and permission broker | Retain fail-closed guarantees for new boundaries; official non-Wasmtime host parity remains unclosed |
| CORE-14 | Distribution | Partial | Independent launcher, portable Wasm, source-free native executable, official browser bundles and packaged varta-shell | Complete distribution/target acceptance, Windows/Pi/cross-target delivery, release policy and artifact authenticity |
| CORE-15 | Conformance | Partial | Pinned ECMAScript 2025 checked-v38 inventory, isolated runner, reviewed timer grants and negative/differential/artifact tests | Five known cases, eight untested feature rows, broader granted KV/fetch cases, wider edition/history coverage, upstream harness and independent platform verification |
| CORE-16 | Performance and production readiness | Partial | Fuel profiler, deterministic golden fuel, codegen/JSON/GC measurements and regression checks | Reproducible startup/throughput/leak/peak-memory benchmarks on supported targets, fuzzing and independent security review |
| CORE-17 | Application lifecycle | Partial; native foundation implemented | Long-lived instances, host tasks, per-turn budgets, permission decisions, bounded scripted network waits, request cancellation, a sampled native resource lifecycle and recorded network replay (native record/replay and the browser replay transport, wave 3 `w3-network-replay`) | Cross-host ordering beyond replayed network completions, Firefox/WebKit replay runs, complete owned shutdown and broader sustained lifetime/native RSS acceptance |
| CORE-18 | View output | Partial; L0/TSX and official browser sessions implemented | L0 batch/event protocol, headless tree/cascade/accessibility, TSX, official browser model/DOM/Worker, packaged resources, local dev/reload and experimental CPU native controls/layout | Remaining native L0 styling/layout, real-window console sessions, complete cross-renderer geometry/accessibility, actual Pi/other-target acceptance and measurements |

“Partial” means that useful checked functionality exists while the full target's
exit criteria remain open. In particular CORE-05/08/09/17/18 are no longer
“Planned”. These rows have unequal scope and are not percentage points. The
[next delivery batches](#next-delivery-batches) replace the earlier foundation-first
queue; CORE-13/15/16 verification accompanies every batch. MCP and domain adapters
do not substitute for missing engine semantics.

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

Core development comes first. The engine consists of `varta-common`, `varta-bigint`, `varta-intrinsics`,
`varta-view`, `varta-compiler` and `varta-runtime`; `varta-intrinsics-wasm` is the
wasm32 shim of the shared intrinsic implementation. Project loading/configuration, scaffolding,
packaging and CLI presentation are supporting development tools. They consume
engine APIs and must not become engine dependencies.

Apart from the developer examples under `examples/` (self-contained projects that
show the public project protocol; not workspace members, and no crate depends on
them), no separate application project is built in this repository. The product
example `examples/uarp-console/` (the
[cross-target acceptance application](#uarp-console-cross-target-acceptance-application),
decided on 2026-10-29) was removed on 2026-11-06 by owner decision. The scenarios
below are deferred consumers of the core, not work items for built-in domain
models. First design and verify the generic mechanisms; only then build a separate
TS/JS project using those real APIs. Test source fixtures remain necessary for
compiler/runtime regression coverage.
### Active priority: a usable application core

Engine capability comes first, not an editor example or MCP. K1–K3 are implemented
in their initial checked profiles and K4 language/value work continues. The current
order is the [next delivery batches](#next-delivery-batches): v38 stabilization,
async server/lifetime integration and measured cross-target acceptance. The
suspension foundation and M1–M3 source features are already integrated. A native TS/JS program serving a real JSON object remains the acceptance
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

Pinned baseline: version **0.13.2** (owner decision, adopted at the v38
wave-3a integration, 2026-10-30), tarball SHA-256
`7cfda733abddadf6f08f1e3c86c53e7ebb00ee927b3afb5f6d7f3ba69e4dcc75`
(npm integrity `sha512-Zitwbwt6K0JGwn44YK1sZZprWgPNfiwXJRgIn4mNbhwYzEN2nnSdbskn0isRx9pbLAyHY8Ft3ofsQ9UB4gJ8sA==`,
shasum `0c7a9b036a762bb0bac319762815c16d7ff78eec`, verified against the
registry and recomputed from the tarball), 65 source files. 0.13.2 changes
only `generated/meta.ts` (`SDK_VERSION`) and `generated/models.ts`: the
tool-call event gains a required `arguments: JsonObject` (with
`arguments_preview?` and a deprecated alias) and three run events type
`error_code?` as the `ErrorCode` literal union instead of `string`. Later
versions are adopted explicitly, with this section updated. History: the
wave-2 integration and the wave-3a stream probes used **0.13.1** (tarball
SHA-256 `2310e037455f3da99c672e7b70cb340c476be81921b76a2f7e8b2e65901043b5`, npm
integrity `sha512-ccaWE8iqbR+NLnZqgcB2jNsoH3OXP1qJXVFFGAxGZ1UsgBOIhD7JCf/OAImYeBrhSG2WoBHyOBM65ZR0+hAsAQ==`,
65 source files; it fixed `uploadPublicSessionImage`, which sends its
`BinaryInput` as `binary: body` with an image content type through the
`binaryContentType` helper of `core/transport.ts`); the wave-2 stream probes used
**0.13.0** (owner decision of 2026-10-30, tarball SHA-256
`69d986d3002247fc347d5a39742f5a46246163d782dcb910e118c8bb4790395d`, npm
integrity `sha512-frtICedMsIcJialN4oBVzsqn4sI6O7vWcAw++Ioc7ZuXu7dGM8lwXG+soKCnvtN5zsZmIDr7ztSVH25Wgm+Izg==`,
65 source files); every measurement up to and including the v38 integration
probe used **0.12.0** (tarball SHA-256
`2a0f2ce97132c90be61ca45c5ea65b77ba25d1706e50894543c5737a6d8bb56e`, npm
integrity `sha512-zoKVCcSTIJXiLvyUKmXyNBH1xPDo7XRcikch+YUasnQPbvTZ8atvdgCci9e8FD+klG4/VyXcMpYGMlXngEPbEg==`,
65 source files); 0.13.0 changes the transport's retryable statuses (no
409), `generated/meta.ts`, the generated models and 15 resource modules.

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

The dated probes below retain their original first diagnostics and measurements.
They are historical snapshots, including uses of “today”, “not supported” and
“pending”; the current source probe and acceptance boundary are in [Status](#status).

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

| Requirement | Occurrences in the earlier pilot | Historical v30–v34 checkpoint |
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

**Ladder (scope retained; source admission integrated through v38).**

| Step | Compiles | Requires | CORE |
| --- | --- | --- | --- |
| M1 | `models.ts`, `meta.ts`, `json.ts` | Type-only imports/exports, top-level `export const`, string-literal types, `as const`, index signatures, recursive `JsonValue`, records wider than 32 fields | 01, 02, 12 |
| M2 | `errors.ts` | Classes, `extends Error`, `super(message, { cause })`, `new.target`, class fields | 02, 03, 04 |
| M3 | `resource.ts` and all `generated/resources` | Inheritance, `protected`, template literals, `encodeURIComponent`, module graph at 65-file scale | 02, 12 |
| M4 | `util.ts` | Generics with `keyof`, Date, crypto, Set | 03, 05, 06 |
| M5 | `transport.ts`, `pagination.ts` | async/await, async generators, `for await`, `#private`, fetch/Headers/Response/AbortController/setTimeout | 08, 09, 10, 11 |
| M6 | `sse.ts` | ReadableStream, TextDecoder, `Symbol.asyncIterator`, `new Promise` | 08, 09, 10 |
| M7 | `index.ts` and the SDK's own tests | Declaration merging, `Object.assign(this, …)`, running the package tests under Varta | 02, 15 |

At the 2026-11-01 checkpoint, the recorded unmodified 65/65-file probe covers
source admission for M1–M6 and `index.ts`; the console exercises selected runtime
paths. M7 package-own-test and four-target acceptance remain open. A step closes
only when its files compile from the pinned
sources, emitted modules validate in Wasmtime and the behavior is covered by tests;
unsupported constructs keep failing with clear diagnostics. Measurements for the
research record: share of files and SDK tests passing, artifact size, startup time
and memory compared with Node, Deno and Bun.

#### Program-size limits measurement (2026-10-29)

Measurement only; no limit changed. Before deciding whether v36 raises the
structural [limits](architecture.md#resource-limits), the pinned SDK was counted
statically with the TypeScript 6.0 checker (structural interning approximating
Varta's) and synthetic v36 programs were compiled at commit `8ccdb12` with a
temporary, uncommitted build that raised the count limits and relaxed the size
checks. Production runtime budgets were used except where a row states a
test-only fuel budget.

Census of the full `UarpClient` graph (what Varta lowers today: every declaration
of every reached module, no dead-code elimination) and of a minimal use
(construct the client and await one resource call, method-granular reachability):

| Entity | Full graph | Minimal use |
| --- | --- | --- |
| Function-likes (functions / methods / constructors+accessors / closures) | 1008 (30 / 907 / 15 / 56) | 40 |
| Async functions / async generators | 10 / 3 | 5 / 0 |
| Generic instances (`request<T>` / `pick` / `autoPaginate`) | 857 (671 / 155 / 31) | 1 |
| Declared interfaces / reachable structural record shapes / classes | 1917 / 1885 / 64 | 1917 / 25 / 62 |
| Array element types / unions (literal-aware) / `Promise<T>` types | 337 / 420 / 620 | 5 / 8 / 6 |
| Physical Wasm signatures | 36 | 19 |
| String literals (UTF-8 bytes; 356 enum-value constants in `models.ts`) | 29.9 KB | 0.8 KB |

Calibration against the compiler's own counts: sync functions, methods and
generic instances cost exactly one Wasm function; a closure costs one function
and about 3.9 KB; an async function costs 3 functions, 1 frame and about 4 KB, and
a new `Promise<T>` adds 5 functions, a frame, 2 record shapes and about 5.4 KB.
Over the 2090 model types that compile alone, compiler/census ratios are 1.01
for record shapes, 1.09 for array types and 1.38 for unions. Static strings take
2 bytes per UTF-16 unit; a checked `as T` schema takes about 85 bytes per field
plus a 16-byte-per-field layout. The 356 enum constants alone cost 30 KB of static
data and 390 K fuel at startup, and startup fuel grows by about 1 unit per static
byte even when nothing is used.

Synthetic pilot mimic (classes of resource methods, each returning
`request<Model>()`, a generic async function doing `JSON.parse(text) as T`; one
method per model), production walls in the order they are reached:

| Limit | Per async method | Reached at |
| --- | --- | --- |
| `abi::MAX_USER_FUNCTIONS` (Wasm function section) | 9.0 functions | 27 methods |
| `value::MAX_RECORD_SHAPES` | ~2 shapes plus models | ~55 |
| `promises::MAX_PROMISE_TYPES` (compiler-local, not in the limits table) | 1 type | 62 |
| 64 KiB static page (compiler and runtime JSON metadata check) | ~0.96 KB | ~70–89 |
| `value::MAX_UNION_TYPES` (each optional reference field is a union) | ~1.2 per model | ~100 |
| Frame catalog (bounded by `abi::MAX_USER_FUNCTIONS`) | 2 frames | 126 |
| `abi::MAX_WASM_FUNCTION_BODY_BYTES` (`varta_resume` is a linear chain, ~125 B/frame) | 252 B | ~260 |
| `abi::MAX_WASM_BYTES` | 10.9 KB | ~385 |
| `abi::FUEL` (startup ≈ static bytes, plus ~10 K per call) | ~0.9–1.3 K | ~1000 |

Scaling with the raised build (N models, M methods; test-only fuel budget):

| N / M | Compile | Wasm functions | Shapes / unions / promises | Wasm | Static | Largest body | Wasmtime compile | Fuel (start + 1 call) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 128 / 128 | 0.6 s | 1177 | 266 / 160 / 129 | 1.3 MiB | 114 KiB | 32 KiB | 0.7 s | 150 K |
| 1024 / 512 | 4.4 s | 4657 | 1563 / 1231 / 514 | 5.3 MiB | 463 KiB | 126 KiB | 2.8 s | ≥506 K |
| 1900 / 900 | 7.5 s | 8165 | 2844 / 2277 / 902 | 9.3 MiB | 748 KiB | 221 KiB | 5.5 s | ≥816 K |
| 4096 / 2048 | 20.7 s | 18545 | 6236 / 4903 / 2050 | 21.4 MiB | 1.9 MiB | 505 KiB | 10.2 s | traps |

Rows past the static page run only until checked JSON rejects metadata outside
static memory. Shape-only programs (4096 shapes, one function) and sync-only
programs (4096 functions or methods) stay small: at most 0.75 MiB Wasm, 0.4 s
Wasmtime compile and 18–24 K fuel.

Estimated full-graph requirements of the SDK: about 6.5–7 K Wasm functions,
2.5–2.7 K record shapes (including promise records), 0.5–0.7 K unions, ~370 array
types, 620–700 promise types, ~35 generator types, ~1.3 K frames, 0.45–0.8 MB
static data, 7–20 MB Wasm, a `varta_resume` body of 0.17–0.4 MB and 1.0–1.6 M fuel
for startup plus one call (error bars: ±10–20 % on counts, ×1–3 on code size
because the real `request` body has more suspension states than the mimic).
Signatures (≤ 40), function types (≤ 30), memory (1.2 MiB used) and the deadline
(< 0.2 s per run) are not limiting. With dead-code elimination the minimal use
would need about 80 functions and 95 shapes, but the 356 constants remain unless
module constants are pruned too.

Candidate v36 ceilings for the owner's decision (≥ 2× headroom): user functions
16384; record shapes and classes 8192; unions 4096; array types 1024; promise
types 2048 (moved into the `varta-common` registry); generator types 256; Wasm
file 32 MiB. Moving together: JSON schema and capture type counts (derived),
runtime function-section, catalog and frame checks, artifact/launcher Wasm
readers. Not a constant change: the static page needs a multi-page static region
(a new memory layout: `RESULT_BASE`, which also serves as the page divisor of
`MAX_MEMORY_BYTES`, every runtime metadata bound and possibly a larger memory
budget), `varta_resume` needs a table dispatch instead of a larger body limit, and
startup fuel needs either a larger budget or startup cost independent of static
size and constants. All ceilings are global constants checked by the runtime for
every profile today, so raising them must make them profile-dependent to keep
v1–v35 artifacts frozen. Scripts and raw results are kept outside the repository.

### Reading the profile and measurement history

All v32–v38 implementation streams described below are integrated in `main`.
The versioned tables describe what each profile admits: an earlier profile's
limitations remain intentional even when a later profile removes them. Dated
probes and named branches are historical evidence, not the current next-task
queue. For current state use [Status](#status), the [CORE ledger](#core-completion-ledger),
the [platform checkpoint](#platform-implementation-checkpoint) and the consolidated
[integration backlog](#current-integration-backlog). No release status is inferred
from a historical integration branch.

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
V32 is integrated in `main` and remains an unreleased development profile.
The branch names in the table record where the streams first integrated.

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
| `data_limits()` | Data limits: arrays, dictionaries, `Map`/`Set`, `Uint8Array`, strings and JSON bounded only by the memory budget; more than 64 live frames without a trap | Implemented ([contract](architecture.md#data-limits-data_limits)): `DataLimits` registry (2^27 elements, 2^30 string bytes, JSON depth host ceiling 10000), value17 sparse backings above 1024 slots with one bit per slot, typed `RuntimeError::Resource` failures through the `resource_failure`/`resource_request` globals, geometric heap growth and a worklist mark, a hashed growable live-frame table and memory-derived scheduler bounds, `json_stringify_owned` (host ABI 34) into owned guest strings, iterative JSON parsing, lifted `Map`/`Set`/`Uint8Array` preludes and an O(n log n) `join`. Dictionary and mixed-record hashing landed in wave 2 (b) under value layout 18. Pending: per-element host calls of `String(x)` against the 1024 host-call budget (`%` no longer calls the host) and frozen limits in primitive conversions; `console.log` logs within the module's string limit since `v38-p2-finish` (`log_string16`, linked text) |
| `turn_budgets()` | Turn budgets: per-event-turn fuel and deadline configurable by the manifest within the host ceiling; fuel profiler and logs before failure in `varta dev` | Admits event turns with per-turn budgets, manifest budgets capped by the ceiling, long-lived application instances, host event sources and permission broker v1; logs before failure and `--budget-report` in `dev`/`run`/companion ([contract](architecture.md#turn-budgets-and-application-instances-turn_budgets)). Pending: per-function fuel attribution (codegen stream), per-turn UI values from measurements on the targets |
| `linked_intrinsics()` | Linked intrinsics: Rust pure intrinsics linked into the guest Wasm so third-party hosts stay thin | Implemented in part ([contract](architecture.md#linked-intrinsics-linked_intrinsics)): the pure operations (Math, `**`/`%`, Number text and parsing, primitive-cell conversions, UTF-16 access and case mapping; since wave 2 every BigInt operation; since wave 3 the value18 conversions, the primitive comparisons and the value18 readers behind codegen-v2 fast paths) are guest code from the shared `varta-intrinsics` crate, linked function by function into the module's intrinsic region of memory 0 (one memory); the generated [embedding tables](embedding.md#linked-intrinsics-v38) list them and what a v38 host still implements. Measured below ([linked intrinsics measurement](#linked-intrinsics-measurement-v38), [wave 3](#wave-3-conversions-and-readers-measurement-v38)). Since `v38-p2-finish` checked JSON serialization and the logged text of a value are linked too, every linked text result is an owned guest string, and a v38 module imports effects only ([linked serialization measurement](#linked-serialization-measurement-v38)). `JSON_STRINGIFY_VALUE` recalibrated to the linked encoder at the wave-3b integration ([fair host fuel](#fair-host-fuel-v38)) |
| `view_protocol()` | View protocol: L0 `view_commit`, event delivery, headless renderer | Implemented (P7): `view_commit` (host ABI 34), synthesized event export, `varta-view` headless renderer (tree, cascade, accessibility; opt-in geometry section with explicit fonts), `ui.open_url`/`clipboard.write` decided by permission broker v1 with `command_result`; every view record is a host task of an application instance and runs as its own turn |
| `tsx()` | TSX: TSX parsing and template compilation | Implemented (start of P8): `.tsx` parsing, AOT templates with getter holes, typed `JSX` namespace; L1 stays a package outside the engine |
| `closure_narrowing()` | Narrowing in closures (TypeScript 5.4) | Implemented ([contract](architecture.md#v38-closure-narrowing-closure_narrowing)): parameters and `let` bindings keep their narrowing in arrows, function expressions and TSX holes created after their last assignment; raised by the UARP console chat screen |
| `text_imports()` | Text resources: `import x from "./x.css" with { type: "text" }` | Implemented ([contract](architecture.md#local-modules)): verbatim UTF-8 default imports resolved and loaded like modules, build-time stylesheet check for direct `View.stylesheet` arguments; owner-approved 2026-10-30 |
| `package_specifiers()` | Package specifiers (wave 1): `import { C } from "pkg"` for typed npm packages | Implemented ([contract](architecture.md#local-modules)): `<config dir>/node_modules/<name>` or `@scope/<name>` only, pinned no-follow reads, bounded `package.json`, exact `exports` keys with the `types` condition only and the top-level `types` fallback for `.`, `.d.ts` mapped to its one shipped `.ts`/`.tsx` source through the adjacent `.d.ts.map`; untyped, declarations-only, symlinked and `*`-pattern packages refused at the import; v1–v37 name the admitting profile. The pinned `uarp-sdk` 0.13.2 checks and runs when imported by name in a scratch project. Pending: the console's imports still use relative `node_modules` paths (owned by the console package) |
| `structural_subtyping()` | Structural subtyping: width and structural subtyping of records, interfaces and classes, remaining pilot language items | Implemented in part ([contract](architecture.md#structural-subtyping-structural_subtyping)): width subtyping of records, interfaces and class instances through views (one accessor call plus one descriptor test per source layout; direct slots for shapes without wider sources), identity-preserving comparisons and `instanceof`, diagnostics naming both types and the missing/mismatched/extra members, erased generic method signatures with checked results, `object` annotations, `Array.isArray` narrowing of `unknown`, record and optional spreads, `obj.m?.()` on class methods, property narrowing kept across calls and getters with checked reads, `JSON.stringify` typed `string` and of `never`, literal `??` fallbacks, record-to-dictionary assertions and records held by `unknown` read as JSON objects, `Promise<unknown>` arrow contexts, `as void`, string `for...of`, exhaustive literal `switch`, constructor parameter properties, `encodeURIComponent`/`encodeURI`. Pending: Web byte bodies (`ReadableStream<Uint8Array>`, `arrayBuffer()`, `Blob`, `FormData`), typed `globalThis`/`process.env`, `Promise.race` over differently typed promises, `A[] \| B[]` of records, unions mixing collections with other objects. Wave 2 (`sdk_paths`, [contract](architecture.md#sdk-paths-sdk_paths)): `instanceof Map/Set/Uint8Array` of closed unions (refused for `unknown`/`object`; `instanceof Uint8Array` of them since wave 3 (k)); spreads and own-key enumerations (`Object.keys`/`values`/`entries`, `Object.assign` sources, `for...in`) through views keep JavaScript key order where the view's sources declare exactly its fields in its order and are refused otherwise. Pilot probe below |
| `codegen_v2()` | Code generation v2: cheaper loops, string comparison and concatenation, dictionary call sites; fuel profiler | Implemented for codegen ([contract](architecture.md#code-generation-v2-codegen_v2)): quiet expressions without root traffic, call and frame fast paths, operand-cache collapse, plain updates, content-equality and inline literal comparisons, amortized string builders, guest `charAt`/`charCodeAt`/`%`, bounded first allocation scans and a collection threshold; `varta dev --profile-fuel` ([profile builds](architecture.md#fuel-profile-builds-varta-dev---profile-fuel)). Admits no syntax; v37 bytes frozen |
| `strict_members()` | Strict members: undeclared member access rejected, per-union member limits from the registry, discriminant dispatch, exhaustive discriminant switches | Implemented ([contract](architecture.md#strict-members-and-wide-unions-strict_members)): `property X is not declared by Owner` for every static access form; `union_members`/`json_union_members` in `ProgramLimits` (frozen 16/7, v38 the entity bound); one discriminant lookup and a flat literal chain (about 55 fuel per member before the match, 9 after); flat or balanced lowering above 16 members; exhaustive `switch` over `x.kind` and nested paths with a diagnostic naming unhandled members. Pending: `varta dev --profile-fuel` of programs with checked conversions fails (`value ABI helpers cannot call host imports`), pre-existing |
| `file_roots()` | Virtual file roots (wave 2): `fs.*` paths and grants are virtual paths `/<root>/...`; the configuration names roots inside the project, the host policy maps them for packaged runs | Implemented ([contract](architecture.md#virtual-file-roots-file_roots)): `fs.roots` in the configuration pinned no-follow at load, manifest `fsRoots` names (host ABI 34), host-policy `fsRoots` mapping pinned at startup, canonical virtual paths with whole-component matching against request and ceiling, no-follow reads below the root descriptor; run, application, view, HTTP request stores and tool invocation. V1–v37 keep host paths |
| `iteration_protocol()` | Iteration protocol (2026-11-06): iterable classes, generator methods, array literal spreads, `Array.from` of iterables, synchronous `yield*` | Implemented ([contract](architecture.md#iteration-protocol-iteration_protocol), [record](#language-gap-ranking-and-the-v38-iteration-protocol--2026-11-06)): `[Symbol.iterator]()` methods (inherited, overridden, generic classes), `x[Symbol.iterator]()`/`x[Symbol.asyncIterator]()` calls, `Iterator<T>`/`IterableIterator<T>` annotations and `implements Iterable<T>`, synchronous generator methods (split into a wrapper and a receiver-free body), spreads of arrays/tuples/strings (code points)/generators/collections/iterable instances in source order, one-argument `Array.from`, `yield*` with observed resumption (`Suspend::observe`) forwarding `next`/`throw`/`return`. No ABI, value-layout or host change. Pending: iterable destructuring, hand-written iterator objects, `Iterable<T>` value annotations (`Array.from` with a map function: `standard_library()`) |
| `standard_library()` | K9 D standard library (2026-11-06): ES2025 Array/Set/Object/JSON/String/Math methods, iterator helpers, `AggregateError`, complete UTC `Date` | Implemented ([contract](architecture.md#standard-library-standard_library), [record](#k9-d-standard-library--2026-11-06)): typed guest code of the standard, aggregate, v38 collections and v38 date preludes; V8's TimSort comparator order. No ABI, value-layout or host change. Pending owner decisions: `Symbol` value tag, integrity flag; pending work: weak references, full RegExp, Proxy/Reflect, `structuredClone`, Intl |
| `local_time()` | K9 D4 local time (2026-11-06): `Date` in the host's time zone, zone and locale as recorded host inputs | Implemented ([contract](architecture.md#local-time-local_time), [record](#k9-d4-local-time--2026-11-06)): `intl_time_zone`/`intl_locale` (host ABI 34, no capability), linked `date_zone_offset`/`date_zone_name` over `varta-intl` (tzdata 2026e, ICU4X 2.4 / CLDR 49), linked indirect calls. Pending owner decisions: CLDR 49 vs Node's 48, Intl data selection; pending work: Intl and `toLocale*` |
| `expression_forms()` | K9 C — expressions and functions (2026-11-06): `await` anywhere, comma, `new.target` comparisons, tagged templates, spreads into fixed parameters, `this` parameters and field arrows, object-literal methods with `this`, computed keys, destructuring everywhere, hand-written iterators, `using`, structural thenables | Implemented in part ([contract](architecture.md#expression-forms-expression_forms), [record](#k9-c--expressions-and-functions-2026-11-06)): no ABI, value-layout, scheduler or host change. Pending: top-level `await` (needs module-level evaluation), object-literal getters/setters, `TemplateStringsArray.raw`, `SuppressedError` for double throws in `using`, `await` in `switch` case tests |
| `declaration_forms()` | Declarations and classes (K9 B, 2026-11-06): enums, namespaces and merging, abstract classes, overloads, setters and accessors, static fields, class expressions | Implemented ([contract](architecture.md#declarations-and-classes-declaration_forms), [record](#k9-b--declarations-and-classes--2026-11-06)): pre-scan lowering of enums (constant evaluation, reverse mapping), namespaces (hoisting with scope-aware renaming, cross-module), static fields (module variables), class expressions and local classes, overload signatures; pre-layout class rewrite for abstract members, setters and auto-accessors; overload resolution with checked results. No ABI, value-layout or host change. Pending (owner decision): static blocks, effectful static initializers and decorators (code at class definition), enum/namespace objects as values, class index signatures |

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

**Web-bytes pilot probe (`v38-web-bytes`, 2026-10-30).** Per-file `varta
check` under v38 of scratch copies of the pinned **0.13.0** sources
(`npm pack uarp-sdk@0.13.0`, integrity verified), production budgets, no
live API call. Unmodified: **62 of 65** files check (the integration probe of
0.12.0: 5); the 3 others (`generated/resources/public.ts`,
`generated/resources/index.ts` and `index.ts`, which import it) stop at one
SDK generator bug: `uploadPublicSessionImage` passes its `BinaryInput` as the
JSON `body: unknown`, a `Uint8Array` held by `unknown` (a known subset gap;
the SDK owner fixes the generator). With that one line changed to `binary:
body` in a scratch copy, **65 of 65** files check, including the merged
`UarpClient` class. No file stops at a union member limit.

**Wave-2 integration pilot probe (`v38-wave2-merge`, uarp-sdk 0.13.1,
2026-10-30).** Per-file `varta check` under v38 of a scratch copy of the
pinned **0.13.1** sources (`npm pack uarp-sdk@0.13.1`, integrity verified),
each file the only `export *` source of a probe entry (default-imported when
it has a default export), production budgets, debug build, no live API call.
Unmodified: **7 of 65** files check (`core/errors.ts`, `core/json.ts`,
`core/pagination.ts`, `core/sse.ts`, `core/util.ts`, `generated/meta.ts`,
`generated/models.ts`); the other 58 stop at one new line of 0.13.1,
`core/transport.ts:476:22: unknown identifier: Blob`: the feature detection
`typeof Blob !== 'undefined' && body instanceof Blob` in `binaryContentType`
reads the prelude class `Blob` as a value, which v38 does not admit. With
that guard reduced to `body instanceof Blob` in a scratch copy, **65 of 65**
files check: the 0.13.0 generator bug (a `Uint8Array` held by `unknown`) is
gone. Owner decision (2026-10-30): admit `typeof` of engine globals under
v38.

**`typeof` engine-globals pilot probe (`v38-typeof-globals`, uarp-sdk
0.13.1, 2026-10-30).** Same per-file probe (`varta check` under v38, each
file the only `export *` source of a probe entry, default-imported when it
has a default export) of a scratch copy of the pinned **0.13.1** sources
(`npm pack uarp-sdk@0.13.1`, integrity verified), production budgets, debug
build, no live API call, sources unmodified: **65 of 65** files check (7
before). `typeof Blob` in `binaryContentType` folds to `"function"` and
`body instanceof Blob` narrows as before
([contract](architecture.md#web-bytes-web_bytes)).

**Wave-3a integration pilot probe (`v38-wave3a-merge`, uarp-sdk 0.13.2,
2026-10-30).** Same per-file probe (`varta check` under v38, each file the
only `export *` source of a probe entry, default-imported when it has a
default export) of a scratch copy of the pinned **0.13.2** sources (`npm pack
uarp-sdk@0.13.2`, integrity verified), production budgets, debug build, no
live API call, sources unmodified: **65 of 65** files check, with the
method-subtyping, linked conversions, linked JSON, fair fuel and KV streams
integrated.

**Wave 2 (integrated on `v38-wave2-merge`, 2026-10-30).**

| Stream | Scope | State |
| --- | --- | --- |
| (a) Intrinsics in memory 0 and a `no_std` value layout | Move the linked library's private memory into a reserved region of memory 0 (owner decision: no multi-memory); make `varta-common`'s value layout a `no_std` core (`std` feature on by default) that `varta-intrinsics` depends on with `default-features = false`, removing the restated cell readers of `varta_intrinsics::layout`; link the remaining 28 layout-reading pure operations. Also lifts the frozen 1 MiB string and result-buffer limits of the linked string intrinsics under v38 | Implemented in part (branch `v38-intrinsics-region`): one memory with the relocated intrinsic region (`varta.intrinsics`: stack, packed data, reserved arena), the `no_std` core (`abi`, `memory`, `value` cells and error classes, `bigint` transport, `limits`, `text`) as the one owner of the cell layout, `varta-bigint` without `std`, and the 13 BigInt operations linked (under value layout 18 the linked conversion is `bigint_from_value_v5`, since primitive and BigInt cells keep their value17 layout); the value conversions and comparisons run the same shared code as host imports. Linked code reads strings within the v38 data limit. The cheap conversion path and the value18 readers followed in wave 3 (a) and (h); JSON, the logged text of values and owned string results in wave 3 (b), (c) and (h) (`v38-p2-finish`) |
| (b) Dictionary and mixed-record hashing | Hashed dictionaries and mixed records; needs value layout 18 (a third storage slot) | Implemented on `v38-dictionary-hashing` (selector `dictionary_hashing()`, [contract](architecture.md#dictionary-hashing-dictionary_hashing)): value ABI/layout 18 for v38 with a four-slot dictionary storage record (`holes`, `keys`, `table`, `values`), an open-addressing `number[]` table probed from a seedless FNV-1a hash of the UTF-16 key units by allocation-free guest helpers, tombstones and holes for deletions, compaction before enumeration and amortized rebuilds at half load; twelve value18 readers (host ABI 34), JSON schema 18 and exception channel v9; every table validated against its entries at every reader. Measured below ([dictionary hashing measurement](#dictionary-hashing-measurement-v38)). Pending: array-index keys inserted below an existing key stay linear (an ordered rotation). The 20k+ allocator bound of the stream alone is gone after integration with (d)'s exact-size free lists ([wave-2 integration measurement](#wave-2-integration-measurement-v38)) |
| (c) Web bytes | `ReadableStream<Uint8Array>`, `ArrayBuffer`, `Blob`, `FormData`, `response.blob()`, typed `globalThis` and `process.env`, and the structural-subtyping stream's pending items (`Promise.race` over differently typed promises, `A[] \| B[]` of records, `instanceof Map/Set/Uint8Array`, unions mixing collections with other objects, JavaScript key order for spreads through views, checked conversions to unions of record shapes without a literal discriminant) | Implemented on `v38-web-bytes` under `web_bytes()` ([contract](architecture.md#web-bytes-web_bytes)): byte bodies (host ABI 34 `net_request_start_v2`, `net_body_read_bytes`), `ArrayBuffer`/`Blob`/`FormData` preludes with a deterministic multipart boundary, typed `globalThis` with `process.env` as an `env.get`-granted snapshot (`env_snapshot`), mixed `Promise.race`, strict record-union conversions (exactly one match, ambiguity a `TypeError`), union-to-union checked assertions, function adapters for `f as F`, `instanceof` narrowing of property paths in conditionals, records reaching a `Uint8Array` held by `unknown`, `process.env` by exact names with static-key reads, classes merged with an interface through `Object.assign(this, …)`. Wave 2 (`sdk_paths`, [contract](architecture.md#sdk-paths-sdk_paths)): `File` with WHATWG `FormData` entries (every `Blob` entry a `File`), `for...of` over `FormData`, `instanceof Map/Set/Uint8Array` of closed unions, spreads and own-key enumerations through views refused unless key order is kept, `"cause" in` an Error instance refused, `error.cause` reads and `new Error(message, options)`, `xs[i].m<T>()`; multipart and query wire bytes equal Node's. A `Uint8Array` held by `unknown` and the SDK's byte parts: wave 3 (k). Pending: the record view of a class instance held by `unknown` (beyond the literal-key `in` of wave 3 (k)), `A[] \| B[]` of records, `Blob.stream()`, `formData()`, `FormData` iterator objects |
| (d) Codegen hotspots | From the profiler: keyed-list diff, per-push argument arrays, `number[]` index boxing, closure re-validation at call sites | Implemented on `v38-codegen-hotspots` ([contract](architecture.md#code-generation-v2-codegen_v2), [measurements](#code-generation-v2-measurements)): appended `push`, checked element reads without union cells, word-stored literal and grown backings, concatenation chains, quiet function-value calls, exact-size free lists. `charAt` of non-ASCII units in constant time; `--profile-fuel` of programs with checked JSON fixed. With (b) integrated the 100-row keyed diff takes 492,170 fuel (under the 1M target; [wave-2 integration measurement](#wave-2-integration-measurement-v38)). Pending: closure entry checks stay as decided; collection work per turn is wave 3 (f) |
| (e) Strict members and wide unions | Undeclared member access as a compile error (owner decision, TS2339), per-union member limits in the limits registry, discriminant dispatch for wide record unions (uarp-sdk 0.13.0 `RunEventV1`, 24 members), exhaustive discriminant switches | Implemented on `v38-strict-members` ([contract](architecture.md#strict-members-and-wide-unions-strict_members)); `if`/`else if` chains stay non-exhaustive, as in TypeScript |
| (f) Virtual file roots | Portable `fs.*` paths: roots declared by the configuration, mapped by the host policy, grants on virtual paths, absolute host paths denied under v38 (`file_roots()`) | Implemented on `v38-virtual-roots` ([contract](architecture.md#virtual-file-roots-file_roots)); the integration adds a CLI smoke test of a long-lived application instance reading through a root. Native HTTP v2 was added later; see the [current native server contract](architecture.md#http-contract-v2--native-server). Browser filesystem effects remain unsupported |
| (g) TSX fixes | Accessible names, `.js`/`.jsx` resolution, deterministic view scripts, text resources, closure narrowing | Implemented on `v38-tsx-fixes`: accname 1.2 names of the L0 subset, `./x.js` → `x.ts`/`x.tsx` (both is an error) and `./x.jsx` → `x.tsx`, `--view-script` on virtual time with `{"input":"wait","ms":N}`, text resources `with { type: "text" }` with the build-time stylesheet check (`varta_common::view::stylesheet`; ambient `*.css`/`*.txt`/`*.md` modules in `varta.d.ts` for editor typing only), narrowing kept inside closures created after the last assignment |

**Wave 3 (recorded, owner decisions of the wave-2 linked-intrinsics review).**

| Item | Scope | State |
| --- | --- | --- |
| (a) Inline conversions and comparisons | `primitive_to_string_v5`, `primitive_to_number_v5` (under value 18: `primitive_to_string_v6`, `primitive_to_number_v6`), `primitive_compare` and `primitive_equal` get an inline codegen fast path for integers, ASCII strings and same-tag comparisons, with the linked general path (`varta_intrinsics::bigint`, already shared with the host imports) behind it, removing those host imports without a fuel regression. Measured when they were linked without it: the codegen v2 dictionary workload (50 inserts + 50 reads) rose from 456 K to 1.1 M fuel, and the v37 case `generator_cases/lifetime.ts` exhausted the production fuel budget under v38 | Implemented on `v38-p2-conversions` ([contract](architecture.md#linked-intrinsics-linked_intrinsics), [measurement](#wave-3-conversions-and-readers-measurement-v38)): fast paths for safe integers (two digits at a time), unboxed Numbers and strings, digit strings, booleans, `null`, `undefined`, same-kind and Number-like comparisons and BigInts below 2^32; the BigInt-free `primitive_to_string`/`primitive_to_number` behind Numbers and strings, the value18 operations only for BigInts. Dictionary workload 157,733 fuel (162,075 with the host imports), `generator_cases/lifetime.ts` 341,359 (v37 628,080). Pending: fractional Number text, non-digit string parsing and comparisons with large BigInts cost the linked code's instructions (thousands of fuel per call) |
| (b) Linked JSON | `json_parse_v18` (under value 18: `json_parse_v19`) and `json_stringify_owned` link with a guest-allocated heap block, rooted for the duration of the call, as their scratch arena, under the normal collector, budget and root rules (no separately reserved gap); the decoder validates the whole input before it calls guest helpers and roots children until their parent is published | `json_parse_v19` linked on `v38-p2-json` ([contract](architecture.md#shared-checked-json-decoder-and-linked-json-wave-3)): one `no_std` UTF-16 decoder (`json::scan`) and top-down builder (`json::build`) shared with the host imports of v10–v38, a serde-equivalent `no_std` schema reader, rooted guest scratch blocks, the host's exceptions and typed limits; fair host fuel for every v38 import ([rates](#fair-host-fuel-v38), [measurement](#linked-json-measurement-v38)). `json_stringify_owned` linked on `v38-p2-finish` ([contract](architecture.md#linked-serialization-and-logging-v38)): one value18 encoder (`varta_intrinsics::json`) with the host serializer's output, checks and failures, iterative, in rooted scratch, its result allocated once at its exact length ([measurement](#linked-serialization-measurement-v38)). Schema-specialized decoders: implemented on `v38-json-decoders` (`json_decoders()`, the linked decoder their fallback; [measurement](#compiled-json-decoders-measurement-v38)). Open: simd128 scanning |
| (c) Owned string results | Results larger than the 1 MiB result buffer (case mapping) are returned as owned guest strings | Implemented on `v38-p2-finish` ([contract](architecture.md#owned-text-results-v38)): every linked text operation (Number text, `charAt`, case mapping, `typeof`, BigInt text, serialization, the logged text of a value) returns a fresh guest string its call site packs without a snapshot; case mapping streams over its input in place; an exhausted budget is the typed memory resource |
| (d) ASCII case mapping inline | An inline ASCII fast path for `toUpperCase`/`toLowerCase` in front of the linked case mapping, which costs about 820 (upper) and 595 (lower) fuel per unit through the core Unicode tables | Open |
| (e) Coalescing intrinsic arena | A merging (coalescing) arena for linked scratch, so linked BigInt work near a million bits fits the memory budget as the host import does (today its size-class free lists do not coalesce and such operands exhaust the 8 MiB budget) | Open |
| (f) Collection work per turn | Owner decision: first idle collection between turns with a deterministic per-instance maintenance budget reported by `--budget-report`; then an incremental collector with bounded work per allocation (write barriers on every reference store, exact rooting kept provable). Motivated by a console turn costing about 910k fuel instead of about 430k when a collection fires inside it | Implemented on `v38-gc-turns` under `incremental_collection()` ([contract](architecture.md#idle-and-incremental-collection-incremental_collection), [measurement](#collection-between-and-within-turns-measurement-v38)): the host-free `varta.collector.idle` export run at idle points under `maintenanceFuel`/`maintenanceDeadlineMs`, a snapshot-at-the-beginning incremental collector with a deletion barrier on every reference store, side bitmaps and a lazy sweep, collector stress builds over every store kind and the case corpus. Open: below |
| (g) Network wait in scripted sessions | Explicit bounded real-I/O waits with a frozen virtual timer clock and a declared replay boundary | Native `wait_network` implemented with focused runtime/CLI checks; recorded network replay implemented in wave 3 ([contract](architecture.md#recorded-network-replay-v38)); simulated (synthesized) networks remain open |
| (h) Linked value18 readers | Link the value18 readers (`value_*_v11`, `value_is_error_class_v7`, `value_log_v12` as linked rendering plus the `log_string16` effect, `json_parse_v19`, `primitive_to_string_v6`/`primitive_to_number_v6` with (a)); `bigint_from_value_v5` is already linked at the integration, since it reads only primitive and BigInt cells | Implemented in part on `v38-p2-conversions`: the reference, vector, dictionary (with its full table validation), Error and closure-envelope readers are generic over `Memory` in the `no_std` core; `value_typeof_v11`, `value_truthy_v11`, `value_nullish_v11`, `value_is_array_v11`, `value_strict_equal_v11`, `value_nullish_equal_v11` and `value_is_error_class_v7` are `varta_intrinsics::value` (also the value8–value17 host readers), linked behind tag fast paths. `json_parse_v19` is linked by (b); since `v38-p2-finish` logging is the linked text `value_format_v12` (the JSON diagnostic renderer's notation) passed to the `log_string16` effect, and no profile admits `value_log_v12` |
| (i) L0 open choices | The L0 view protocol's open choices, including `role="separator"` and `role="group"` | Open |
| (k) Bytes held by `unknown` | Owner decision (2026-11-05): open bytes (`Uint8Array`/`ArrayBuffer`) held by `unknown` under v38, replacing the earlier compile error, so the SDK's own `appendPart(form, key, value: unknown)` sends `Uint8Array`, `ArrayBuffer` and `{ data, filename, contentType }` parts | Implemented on `wave3/w3-v38-bytes-unknown` under `byte_values()` ([contract](architecture.md#bytes-held-by-unknown-byte_values)): the byte-array tag `ValueKind::Bytes`, an additive tag of value layout 18 (no ABI number change) that every `Uint8Array` carries in cells, fields, elements and dictionary values; `unknown` and `object` hold one; `instanceof Uint8Array`, `ArrayBuffer.isView`, `typeof`, `String` and `JSON.stringify` as in Node (also inside records); the JSON-object view of a record held by `unknown` in own-key order with its bytes fields; `"key" in` a `Uint8Array`, `ArrayBuffer`, `Blob` or `File` held by `unknown` from Node 24.20.0's prototype members; `kv_stage` refuses a tagged `Uint8Array` by name; identity comparisons of two values that may both hold a `Uint8Array` (views of one `ArrayBuffer` share their storage) and checked JSON decode targets reaching one are compile errors. The SDK's multipart byte parts produce Node's wire bytes. Open: own-key enumeration and dynamic-key `in` of a `Uint8Array`, array or engine instance held by `unknown` (named `TypeError`), `'data' in` an array part (`TypeError`, Node: false), logging a typed `Uint8Array` |
| (j) Deno KV storage | Owner decision: a native KV host capability file-compatible with Deno KV, so the UARP API's Deno KV file (SQLite, `denokv_sqlite` schema 3) opens in Varta without a migration and Deno reads what Varta writes | Implemented on `v38-kv` under `kv()` ([contract](architecture.md#kv-storage-kv)): `kv.read`/`kv.write` path grants over virtual roots, nine host ABI 34 imports, pinned no-follow open verified against SQLite's descriptor, exact schema check, lock probe, a strict V8 serializer codec, Deno-shaped `Kv`/`AtomicOperation`/`KvListIterator`/`KvU64` with strict checked-JSON decoding; fair host fuel at the calibrated KV rates since the wave-3a integration ([KV fair fuel](#kv-fair-fuel-v38)). Open: below |

**KV storage dependencies (v38 `kv`, 2026-10-30).** Evidence: Deno v2.9.4's
`Cargo.lock` (tag `v2.9.4`, `github.com/denoland/deno`) locks `denokv_sqlite`
0.14.0 (checksum `9cedc50f…5c8c31`), `denokv_proto` 0.14.0 (`540b7d96…1243f6`),
`rusqlite` 0.40.1 (`11438310…a43323`) and `libsqlite3-sys` 0.38.1
(`f6c19a05…a61db`), and its `Cargo.toml` enables rusqlite's `bundled`
feature; v2.9.7 locks the same four with the same checksums (only `v8` moves,
150.2.0 to 150.4.0, both V8 15.0, serializer version 16). Varta pins them
exactly (`=`), with `bundled` and denokv's required `fallible_uint`, plus the
two crates its connection factory names: `deno_error` =0.7.1 (Deno's lock) and
`chrono` =0.4.42 (Deno's lock, `std` only, no clock feature); `rand` stays at
the already locked 0.8.8 for the versionstamp generator. The new transitive
crates (async-stream, async-trait, futures, hex, log, prost, tokio-stream,
uuid, v8_valueserializer, wtf8, parking_lot, hashlink and their small
dependencies) come from denokv's own manifests; denokv requires Tokio's
`full` feature set, which the existing Tokio 1.54.1 pin satisfies (its worker
threads run their own current-thread runtimes; the runtime's waiting runtime
is separate from the `net.request` one). No existing lock entry changed
version. All of these are dependencies of `varta-runtime` only; the boundary
test checks that no engine contract, compiler, artifact or intrinsic crate
reaches SQLite. Rejected: a hand-written SQLite schema or V8 engine, and
`v8_valueserializer` as the codec (owner requirement: Varta's own strict
codec; denokv uses that crate only for its V8 `sum`, which Varta does not
call).

**KV storage decisions (owner, 2026-10-30).** Writes fail fast: a busy
write lock or a spent turn fails before anything is submitted, a commit is
reported `ok` only once durable, and a commit not decided by its cutoff is
refused at its commit point and never applied
([contract](architecture.md#kv-storage-kv)). KV operations stay synchronous
within the turn, with Deno's promise-returning API.

**Production census (uarp.kv, 2026-10-30).** A read-only aggregate scan by the
owner's API developer (counts only; no keys or values left the host), with a
streaming serializer parser checked against the 2.9.4 fixture (45 of 45):
1,201,564 rows with `v_encoding` 1, every one with header version 16 (`FF
10`), none over 64 KiB, none unparsable (479,105 at most 256 B, 651,992 at most
4 KiB, 70,467 at most 64 KiB). Top-level values: object 1,071,399, int32
73,415, one-byte string 46,635, double 8,921, `true` 906, dense array 193,
two-byte string 88, sparse array 7. Tag totals: one-byte string 23.9M, double
16.45M, int32 2.88M, object 2.59M, `undefined` 1.06M, two-byte string 402,734,
dense array 371,406, padding 188,599, `true` 163,463, sparse array 142,455,
`false` 88,299, `null` 2,388, object reference 240. Never present: Date, Map,
Set, BigInt, ArrayBuffer and views, RegExp, Error, primitive wrappers, UTF-8
strings. Consequences: complete sparse-form arrays read as dense arrays,
back-references read as copies, `undefined` properties fill `T | undefined`
fields, and Date, Map, Set, BigInt and bytes inside values stay read-side
`TypeError`s without a guest mapping (low priority). Besides these rows the file
holds 139 KvU64 values and 1 bytes value (the earlier count).

**KV storage open items.** `watch` (three uses in the UARP API) and queues
(not used); event-loop completions like `net.request` instead of the
synchronous wait within the turn; `Date`, `Map`, `Set`, BigInt and nested
`Uint8Array` values (not in production; the codec reads and writes them, the
guest mappings are missing); key parts other than strings and numbers;
keeping a stored record's property order and aliasing on read; reading a
property holding `undefined` as present for optional fields; the browser
KV storage's recorded differences (a per-artifact lock, expiry collection
at a writable open only; [agreed design](#browser-kv-storage-agreed-design-wave-3)
below); Deno in CI to read Varta-written files automatically. The
`if (res.ok)` narrowing and the other API-shape deviations are wave-3
language items. The fixtures are synthetic: Deno
2.9.4 (x86_64-linux, the owner's API developer) and Deno 2.9.7
(aarch64-darwin) write byte-identical `v` and `v_encoding` for all 48 rows;
versionstamps differ by design (random steps).

#### Browser KV storage (agreed design, wave 3)

Owner decision, 2026-11-05. Built by the wave-3 package `w3-browser-kv-opfs`
(the open questions below are answered there, as lead decisions for owner
review); `varta build`/`varta dev --target browser` admit KV grants again, and
a Worker without sync access handles refuses a granted program by name
(`browser::kv_storage_unavailable`).

- **Storage bridge: OPFS `FileSystemSyncAccessHandle` in the dedicated
  Worker.** The guest already runs in a dedicated module Worker. A sync access
  handle's `read`, `write`, `truncate`, `getSize` and `flush` are synchronous,
  so the `kv_*` imports stay synchronous host calls, every guest entry stays
  synchronous, and a commit can be reported only after `flush` returns, as
  [KV storage point 4](embedding.md#kv-storage-v38) requires. It needs no
  COOP/COEP and no cross-origin isolation. Only acquiring the handle
  (`navigator.storage.getDirectory`, the file handle and
  `createSyncAccessHandle`) is asynchronous, so it happens before
  instantiation. Rejected: `SharedArrayBuffer` and `Atomics.wait` against a
  page-side IndexedDB (every deployment and `varta dev` would need
  cross-origin isolation); JSPI (every guest entry becomes asynchronous); a
  preloaded mirror with write-behind (reports `COMMITTED` before the write is
  durable).
- **Value layer: shared Rust compiled to wasm32.** Staging (the value18
  layout walk of `kv/stage.rs`), the V8 serializer subset (`kv/v8.rs`), the
  Deno KV key codec, the decode classifier (`kv/decode.rs`) and the checked
  construction of guest values are the same Rust code the native runtime
  runs, built for wasm32 in the style of `varta-intrinsics`
  (`varta-intrinsics-wasm`, a checked-in build with a fingerprint), not a
  JavaScript port. Fair-fuel parity with native depends on byte-identical V8
  encoding lengths and the same guest allocation order, which shared code
  gives by construction.

Questions of the design, answered by the package (lead decisions recorded
for owner review):

1. **Per-artifact directory.** Answered: `varta-kv/sha256-<digest>/` in OPFS,
   named from the artifact identity as the native `varta-shell` names
   `<data-dir>/sha256-<digest>/`; a new artifact version starts with empty
   storage. A virtual path (`/<root>/<name>.kv`) names one database inside
   the artifact's container; the guest never chooses an OPFS name.
2. **Which handles to acquire.** Answered: one container file per artifact
   (`kv.varta`), so wildcard grants work; acquired before instantiation only
   when the program imports KV and the ceiling covers the package's `kv.read`
   grant.
3. **Atomic commit and durability.** Answered: a write-ahead record, a flush,
   then the other of two checksummed header slots and a second flush;
   `COMMITTED` only after the second. A crash or a closed tab leaves the old
   or the new state (node tests crash every write, flush and truncate,
   compaction included). A second tab or Worker maps to the native "locked by
   another writer" text; the lock is per artifact, so even a read-only open
   fails there.
4. **Failure mapping.** Answered: an exhausted quota, a corrupt container,
   unavailable origin storage and an unknown header-write outcome are
   recoverable `TypeError`s with named texts (`browser::registry().kv.texts`),
   never `COMMITTED` and never a trap. A Worker with no sync access handles
   at all refuses a granted program by name at mount instead.
5. **Ceiling validation.** Answered: `resolvePermissions` accepts
   `{"kv.read"|"kv.write": {"patterns": [...]}}` and validates each pattern
   as a native host ceiling's grant; matching is the native virtual-root
   matching.
6. **On-disk format.** Answered: a Varta-defined container, format version 1
   (no SQLite on wasm32, no new dependency), with native observable
   semantics, including `denokv_sqlite` 0.14.0's partial application of a
   commit whose `sum`/`min`/`max` meets a non-`KvU64` value.
7. **Goldens.** As agreed: versionstamps keep Deno's format and order with
   their own random steps, so the Chrome acceptance compares logs that never
   print a versionstamp value.

Not answered by the design and left as recorded differences: expired entries
are collected at a writable open only (the native worker also collects later
in the instance), and a commit never waits for a lock.

Review fixes (2026-11-05, lead and owner decisions):

- **Whole-artifact lock: accepted** (owner). A second tab cannot open even a
  read-only database while another holds the container; documented as a
  deviation ([embedding](embedding.md#browser-effect-mapping-official-host),
  [architecture](architecture.md#kv-storage-kv)). Coordination between tabs
  may come later. The handle is acquired once per Worker, so a locked-out
  tab stays locked out until it reloads.
- **Nested database paths: refused** (lead). The container has no
  directories, so `/data/sub/app.kv` is a recoverable `TypeError` naming
  `registry.kv.noDirectories` (fail closed), where native opens it only
  inside a host-created directory; the Chrome/WebKit acceptance and a Node
  test cover it.
- **Value depth: one shared limit** (lead). `varta_kv_codec::MAX_VALUE_DEPTH`
  (the legacy checked-JSON depth) bounds the stage walk, the V8 encoder
  and decoder and the classifier identically on both hosts with a named
  recoverable `TypeError`; the construction was already iterative. Measured
  before the limit: Chrome trapped the guest with "Maximum call stack size
  exceeded" from about 750 levels, WebKit from about 170, and a native debug
  build aborted its process with a host stack overflow at 1500. Native,
  Chrome and WebKit now agree just below, at and one above the limit and at
  2000 levels. A part shared within a value counts at its deepest reach
  (second re-check): the stage walk, the V8 writer and the V8 reader keep
  each aggregate's height and check it again at every later reach, so a
  value stores, in either property order, exactly when its expansion (what
  a `get` classifies) is within the limit; native, the wasm32 codec and
  Chrome agree on the shared-chain cases.
- **Value extent: shared bounds on what a read builds** (lead, third
  re-check). A read expands shared parts, so a diamond `x = { a: x, b: x }`
  of a few dozen stored bytes exhausted the turn's fuel on `get`, and a
  slightly larger one ran a native decode past the deadline at about 1 GB
  of host memory. `kv::MAX_VALUE_NODES` and `kv::MAX_VALUE_UNITS` now bound
  the values and text a value expands to, a shared part counted at every
  reach, in the V8 writer and reader on both hosts with one named
  recoverable `TypeError`; the classifier pays fuel as it goes. Lead
  decision (2026-11-05): the bounds only stop sharing-driven blow-up. They
  are derived from `kv::MAX_VALUE_BYTES` (one value per encoded byte at
  least; text at the densest rate, a nine-digit index key), so every value
  Deno stores without sharing sets and gets, as in Deno and the earlier v38
  build; the earlier fuel-derived value bound refused arrays of 2048
  numbers. A large value whose decoding is past its turn's fuel (alone, or
  with the others of one `getMany` or `list` batch) or whose result is past
  the guest's memory ends the run with the ordinary fatal exhaustion, as
  any too expensive work does, on both hosts. Keys and range selectors
  never meet a value's bounds and keep Deno's key-size messages.
- **Writability as native**: a database opens writable when the request and
  the ceiling cover `kv.write`, whatever the broker's state; an uncertain
  container refuses reads too.
- **Atomicity of a failing `sum`/`min`/`max`**: still `denokv_sqlite`
  0.14.0's partial commit, now pinned by a native regression test
  (`a_failing_sum_keeps_the_earlier_mutations_of_its_commit`); whether
  Varta should keep it is an open owner question.

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
of the workload is about 0.39M fuel, so the 1M target needs that stream
(integrated: 492,170 fuel, [wave-2 integration measurement](#wave-2-integration-measurement-v38)). A
closure call still validates the function cell at the call site and its
environment and each captured binding cell at entry (about 70 fuel per object,
the v12 entry-check contract and its malformed-environment tests). Decision
(2026-10-30): the full v12 entry checks stay, so a binding cell corrupted after
creation is still detected; closure calls are not made cheaper by trusting
guest-built environments.

**Incremental collection (wave 3 (f), implemented on `v38-gc-turns`).** A
v38 collection used to mark the live heap once through the worklist and then
sweep the whole heap inside whichever turn crossed the threshold: about 300k
fuel per collection at 6000 live and 9000 dead blocks. Both owner steps are
in place ([contract](architecture.md#idle-and-incremental-collection-incremental_collection),
[measurement](#collection-between-and-within-turns-measurement-v38)). Open
items: an incremental cycle costs more fuel in total than an atomic
collection of the same heap (side bitmaps, per-block bit lookups and the
quantum bookkeeping; see the measurement), so the per-cycle cost and the
pacing constants want a second pass; scripted sessions whose timer chains
leave no idle point still collect inside the chain; and a cycle's floating
garbage raises peak memory, which the thresholds bound by collecting
atomically near the budget and for large requests.

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
  the linear layout does not have. The allocator belongs to wave-2 stream (d);
  integrated with its exact-size free lists, 20,000 and 30,000 entries cost
  about 1,000–2,300 fuel per operation
  ([wave-2 integration measurement](#wave-2-integration-measurement-v38)).
- **Memory.** The table adds two to four `number[]` slots (16–32 bytes) per
  entry, so 50,000 hashed entries with their keys exhaust the 8 MiB memory
  budget (typed `Memory`), where the linear layout still fits; the 100,000-key
  array alone exhausts it in both. Production budgets are unchanged.
- **Ordered insertion.** An array-index key below an existing key (for example
  ids inserted in descending order) is rotated into place with one
  `memory.copy` per vector and one pass over the table: linear, as before, but
  2,000 descending ids with 1,000 deletions and a 6,000-entry string-key
  workload together run in 185M fuel (`dictionary_hashing_cases/large.ts`).

#### Wave-2 integration measurement (v38)

The wave-2 streams together on `v38-wave2-merge` against the integrated v38
tip (`cb6795d`), both under v38 with the same harness (the 13 codegen
patterns, the keyed diffs and the Markdown fixture at their checked sizes
under test-only budgets of 40 production fuel budgets and unbounded host
calls; the keyed view row counts by bisection under one production budget,
1,000,000 fuel and 1024 host calls; dictionaries as in the dictionary
hashing measurement: marginal fuel of `n` inserts, lookups and deletes of
distinct keys under test-only fuel and the production 8 MiB memory). Fuel
is deterministic; the debug test build on the author's macOS arm64 host.

| Workload | Size | Before (cb6795d) | After (wave 2) | Change |
| --- | ---: | ---: | ---: | ---: |
| `x = x + i` loop | 10,000 | 140,552 | 140,669 | +0.1% |
| 3-char string `===` | 4,000 | 304,913 | 305,002 | +0.0% |
| string `===` literal | 4,000 | 132,875 | 132,964 | +0.1% |
| `s = s + "x"` | 800 | 50,471 | 50,392 | −0.2% |
| dictionary 50 inserts + 50 reads | 50 | 446,490 | 162,075 | 2.75× |
| record field reads | 1,000 | 22,698 | 22,787 | +0.4% |
| direct function calls | 1,000 | 27,544 | 27,661 | +0.4% |
| closure calls | 1,000 | 330,982 | 305,015 | 1.09× |
| array `push` + index sum | 500 | 464,755 | 112,049 | 4.15× |
| Markdown-like scan (lines) | 10 | 68,458 | 66,002 | 1.04× |
| `JSON.parse` of ~2 KB (records) | 40 | 98,913 | 64,371 | 1.54× |
| list render (rows) | 100 | 292,803 | 160,137 | 1.83× |
| keyed-list diff (rows) | 100 | 1,785,586 | 492,170 | 3.63× |
| keyed-list diff (rows) | 500 | 30,048,681 | 2,413,659 | 12.45× |
| Markdown blocks fixture (chat replies) | 1 | 104,367 | 59,276 | 1.76× |
| keyed view mount, rows in 1M fuel | — | 74 | 128 | 1.73× |
| keyed view update, rows in 1M fuel | — | 35 | 65 | 1.86× |

| Entries | Insert before → after (per op) | Lookup before → after (per op) | Delete before → after (per op) |
| ---: | ---: | ---: | ---: |
| 10,000 | 628,116 → 2,142 | 700,275 → 997 | 2,095,900 → 1,013 |
| 20,000 | 1,187,358 → 1,907 | 950,645 → 1,677 | 4,190,900 → 1,022 |
| 30,000 | 2,126,057 → 2,228 | 1,953,995 → 1,173 | 6,417,544 → 1,031 |

- The keyed diff meets its 1M target with hashing and the hotspot work
  together: the 100-row diff is 1.45M with the hotspots alone and 0.49M
  integrated; the 500-row diff falls 12×.
- Dictionaries cost about the same per operation from 10,000 to 30,000
  entries once both streams are in: the dictionary stream alone measured
  17,714/113,018/112,421 (20k) and 2,600/260,779/1,404 (30k) fuel per insert,
  lookup and delete, bound by the first-fit allocator over fragmented key
  garbage; the exact-size free lists take those small blocks off the first-fit
  path. Tables are larger blocks and stay on the first-fit path.
- The keyed view mounts 128 rows (the hotspot stream alone: 129) and updates
  65 (alone: 51) in one production budget: hashing adds a little per mounted
  row and saves on the keyed update.
- The scalar loops change by at most 0.4% (value-layout and helper
  differences in the static prologue); none regresses beyond that.

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
  measured and reverted (see wave 3 (a)); wave 3 links them behind fast
  paths ([measurement](#wave-3-conversions-and-readers-measurement-v38)).
- **Cold start.** Node 24 runs a linked single-memory module (Math, Number
  text, parsing, case mapping and BigInt) with only `log_string16` and
  `log_f64_v2` implemented, first run about 4 ms (V8 lazy compilation). The
  browser and Raspberry Pi 5 were not measured: no browser automation and no
  device were used.

**Next increment: the value-layout readers.** The remaining pure imports read
value layouts beyond primitive and BigInt cells (wave 3 (a)–(c) cover the hot
conversions, JSON and owned results):

1. Done in wave 3 (h): the readers in the `no_std` core and the seven
   value18 readers linked behind fast paths (plain linked readers cost
   thousands of fuel per call: about 17,000 to validate a three-entry
   dictionary, 850 for a Number cell before words were read in place).
2. Done on `v38-p2-finish`: `value_log_v12` split into the linked text
   `value_format_v12` and the `log_string16` effect
   ([measurement](#linked-serialization-measurement-v38)).
3. A coalescing arena (or arena blocks from the guest allocator) so linked
   BigInt work near the bit capacity fits the memory budget as the host
   import does.

#### Wave-3 conversions and readers measurement (v38)

Wave 3 (a) and (h) on `v38-p2-conversions` against the integrated tip
(`d436311`), both under v38 with production budgets unless stated; fuel is
deterministic (debug test build on the author's macOS arm64 host). The
workloads are `codegen_v2_fuel_ceilings_hold` (test-only budgets of 40
production fuel budgets and unbounded host calls).

| Workload | Size | Before (host imports) | After (linked, fast paths) | Change |
| --- | ---: | ---: | ---: | ---: |
| `x = x + i` loop | 10,000 | 140,669 | 140,689 | +0.01% |
| 3-char string `===` | 4,000 | 305,002 | 304,970 | −0.01% |
| string `===` literal | 4,000 | 132,964 | 132,932 | −0.02% |
| `s = s + "x"` | 800 | 50,392 | 50,357 | −0.1% |
| dictionary 50 inserts + 50 reads | 50 | 162,075 | 157,733 | −2.7% |
| record field reads | 1,000 | 22,787 | 22,755 | −0.1% |
| direct function calls | 1,000 | 27,661 | 27,655 | −0.0% |
| closure calls | 1,000 | 305,015 | 304,983 | −0.0% |
| array `push` + index sum | 500 | 112,049 | 112,043 | −0.0% |
| Markdown-like scan (lines) | 10 | 66,002 | 65,276 | −1.1% |
| `JSON.parse` of ~2 KB (records) | 40 | 64,371 | 59,604 | −7.4% |
| list render (rows) | 100 | 160,137 | 148,445 | −7.3% |
| keyed-list diff (rows) | 100 | 492,170 | 471,773 | −4.1% |
| keyed-list diff (rows) | 500 | 2,413,659 | 2,349,708 | −2.6% |
| Markdown blocks fixture (chat replies) | 1 | 59,276 | 59,038 | −0.4% |
| keyed view mount (rows) | 10 | 83,618 | 82,534 | −1.3% |
| keyed view update (rows) | 10 | 158,639 | 156,668 | −1.2% |

Fuel per operation (`linked_value_measurements`, marginal cost per loop
iteration; v37 is the host import, which charges a host call and no fuel
for its work):

| Operation | v37 | v38 |
| --- | ---: | ---: |
| `String(i)` (Number) | 381 | 217 |
| `"k" + i` | 559 | 364 |
| `String(u)` (`number \| string` cell) | 319 | 232 |
| `u < 3`, `u == 2`, `u === 2` | 1,156 | 736 |
| `Number(u)` (Number cell) | 174 | 104 |
| `Number(e)` (digit string) | 271 | 172 |
| `Number(d)` (cell holding a digit string) | 182 | 198 |
| `d == 1024`, `d < u` (digit strings) | 443, 341 | 398, 324 |
| `b < 3`, `b == 5` (`5n`) | 757 | 575 |
| `typeof v`, `=== null`, `Array.isArray`, `instanceof Error` | 819 | 408 |
| `String(i / 8)` (linked shortest text) | 399 | 4,265 |
| `Number(s)` (`"12.5"`, linked parser) | 271 | 1,995 |
| `s < u` (linked parser) | 430 | 2,160 |
| `Number(w)` (`"abc"`, linked parser) | 206 | 1,371 |

- **Corpus.** Every runtime case program runs under v38 with no more fuel
  than under v37 (`case_corpus_behaves_identically_under_v38`); among the
  cases the naive linking regressed, `generator_cases/lifetime.ts` takes
  341,359 fuel (v37 628,080), `v35_cases/narrowing.ts` 29,038 (60,417),
  `value15_cases/recursive.ts` 30,313 (61,207). Plain linked readers had
  taken 742,696 fuel in five `describe` calls of the narrowing case.
- **Remaining costs.** The general linked paths cost their instructions: a
  fractional Number's shortest text about 4,300 fuel, parsing a non-digit
  string about 1,400–2,000, a comparison with a BigInt of more than one
  digit word or a BigInt with a string several thousand. A fixture comparing
  every pair of twelve mixed primitives (strings, a BigInt, fractions) takes
  425,395 fuel under v38 and 348,471 under v37 (identical output). Building
  the library at `opt-level = 3` would cut these by 15–20% for 20% more
  library bytes; it stays size-optimized.
- **Size.** The library grew from 293,907 to 335,259 bytes (the readers in the
  core and the shim). What a module links (`linked_closure_sizes`):
  `primitive_to_string` 9.3 KiB of code (ryu-js), `primitive_to_number`
  12 KiB, `primitive_to_string_v6` 55 KiB (BigInt text), the comparisons
  28–29 KiB, each value18 reader about 25 KiB (shared). Modules: the
  dictionary workload 20,184 → 41,357 bytes, the keyed view 36,538 → 57,560,
  the Markdown fixture 17,396 → 38,212, `"v" + x` alone 6,827 → 27,867
  (fractional Number text links ryu-js); without the BigInt-free variants
  and narrowed cells the keyed view had 96,972 bytes.
- **Region.** One page for every measured closure; the stack (5,016 bytes)
  and bounded scratch (6,984 bytes) maxima are unchanged, the new families
  use at most 1,904 stack bytes and 228 arena bytes
  (`linked_operations_fit_the_region_stack_and_reserved_arena_with_margin`).
- **Imports.** The generic fixtures import only effects: the keyed view,
  the Markdown fixture and the dictionary workload `log_string16`, the TSX
  application `view_commit` (`v38_fixtures_import_only_effects_and_pending_json`;
  before: `primitive_to_string_v6` and `primitive_equal`). The v38
  host-operation set ([generated table](embedding.md#host-operation-table))
  is effects only, except `json_parse_v19`, `json_stringify_owned` and
  `value_log_v12` (all three linked or split since `v38-p2-finish`).
- **Contract-only host.** Node 24.20 ran the v38 build of a mixed fixture
  (conversions and comparisons of twelve primitive kinds, `"k" + n`, the
  readers over `unknown` values of every kind including a hashed
  dictionary, a TypeError and a function; no JSON) with a 20-line host that
  implements only `log_string16` and refuses any other import: one memory,
  one import, output identical to `varta dev` and to v37, first run 2–14 ms.

#### Linked JSON measurement (v38)

Owner decisions (2026-10-30): one shared `no_std` decoder for the host path
of `json_parse_v13`…`v19` (implementation unified, output unchanged);
fair host fuel for every v38 host import; then link checked JSON under v38
and re-baseline like with like.

Fuel of one `JSON.parse(text) as T` beyond the same program without the
parse (`linked_json_fuel_measurement`, `--nocapture`), test-only budgets,
debug build. "Fair" charges the host import's work at the published rates
([fair host fuel](#fair-host-fuel-v38)); v37 "at fair rates" is the
test-only like-with-like accounting of a v37 module.

| Workload | Source chars | v37 | v37 at fair rates | v38 import, free | v38 import, fair | v38 linked (production) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 record `{id, name, tags}` | 54 | 737 | 36,547 | 433 | 36,243 | 30,012 |
| 40 records (codegen v2 "JSON.parse ~2 KB") | 2,181 | 18,755 | 455,335 | 10,495 | 447,075 | 366,979 |
| 1,000 numbers `1.5` | 4,001 | 275 | 1,627,855 | 175 | 1,627,755 | 886,545 |
| one RunEventV1-like member, as `unknown` | 76 | 1,995 | 66,967 | 1,192 | 66,164 | 43,762 |

The linked decoder before wave-3 tuning took 380,454 and 936,657 fuel on the
middle rows; kept optimizations: a plain-unit string scan, ASCII field-name
comparison, two-word slot sets (wasm32 has no 128-bit shifts), an exact
number path (at most 15 digits, exponent within 22), a canonical schema
fast path, a small `Copy` stop type, a fast path for arrays of numbers and
strings, cached record order words and word-wise presence bitmaps. What
remains: about 30 fuel per text unit of a long string, 400 to 600 per
scalar value (bounds-checked unit reads, the tape, per-value budgets and
dispatch), 200 to 300 per built block beyond the guest allocator, and 2,000
to 4,000 per schema node read and checked on every call (the schema is not
cached across calls: nothing survives a linked call). A Wasmtime fuel
calibration loop that only loads and compares each unit costs 18 fuel per
unit. A schema-specialized decoder emitted by the compiler was not built:
the per-call schema work it would remove is the largest remaining term on
small documents (the schema read alone measured 9,923 fuel for the
five-type record schema in a standalone wasm32 build) and is the next
measured step; a per-module schema cache in the library would remove it
too but breaks the "nothing survives a call" rule.

Memory: the linked scratch (tape and frames) is guest heap, 16 bytes per
tape node, rooted for the call and unrooted per block once free; a growing
tape holds its old and new capacity. A 100,000-number round trip runs in
`round_trip.ts`; `v38_json_round_trip_costs` pins 10,000 and 50,000 numbers
(25.1M and 125.0M fuel with fair serialization).

**SIMD (`simd128`).** Validates in Wasmtime with the production engine
configuration (`simd128_validates_in_the_production_engine`) and in Node 24
(`WebAssembly.validate`); browser support (not tested here, no browser
automation): Chrome 91+, Firefox 89+, Safari 16.4+. The linked decoder
does not use it (the measured cost there is per value and per schema node,
not per scanned unit); the compiled decoders scan strings and whitespace
with it since the SIMD stream
([measurement](#simd-json-scanning-measurement-v38)).

#### Linked serialization measurement (v38)

Branch `v38-p2-finish` (from `c033125`): `json_stringify_owned` linked,
`value_log_v12` split into the linked `value_format_v12` text and the
`log_string16` effect, and every linked text result an owned guest string
([contract](architecture.md#linked-serialization-and-logging-v38),
[owned text](architecture.md#owned-text-results-v38)). Deterministic fuel,
debug test build, test-only budgets (`linked_serialization_fuel_measurement`,
`--nocapture`): one serialization or logged value beyond the same program
without it; "fair" is the host import's charge at the published rates
(`CALL` 50, `JSON_STRINGIFY_VALUE` 1,500 per value, `JSON_STRINGIFY_UNIT`
30 per output unit; the host renderer of a logged value is charged the same
rates since this branch, and the linked path adds `log_string16`'s `CALL`
and bytes).

| Workload | v38 import, uncharged | v38 import, fair | v38 linked | linked / fair |
| --- | ---: | ---: | ---: | ---: |
| number `2.5` | 315 | 1,945 | 2,694 | 1.39 |
| short string | 319 | 2,069 | 2,681 | 1.30 |
| 1 record (3 fields) | 334 | 10,456 | 18,213 | 1.74 |
| 40 records (~1.8 KB) | 368 | 412,944 | 431,616 | 1.05 |
| 1000 numbers | 368 | 1,621,944 | 945,800 | 0.58 |
| 40 strings | 361 | 72,735 | 48,080 | 0.66 |
| 40 integers | 361 | 65,535 | 36,873 | 0.56 |
| 40 arrays `["a","b"]` | 361 | 193,935 | 147,116 | 0.76 |
| 40 empty arrays | 361 | 65,535 | 90,490 | 1.38 |
| 40 empty records | 361 | 65,535 | 84,501 | 1.29 |
| 40 records `{a,b,c}` | 361 | 265,935 | 285,092 | 1.07 |
| dictionary, 5 entries | 334 | 10,456 | 25,650 | 2.45 |
| dictionary, 20 entries (hashed) | 361 | 36,735 | 95,402 | 2.60 |
| log: Number cell | −305 | 1,273 | 2,448 | 1.92 |
| log: string cell | −301 | 1,337 | 429 | 0.32 |
| log: 1 record | −293 | 9,775 | 18,230 | 1.86 |
| log: 40 records | −293 | 412,225 | 439,658 | 1.07 |
| log: fulfilled promise | −293 | 4,585 | 15,147 | 3.30 |

- **Optimization.** The first linked encoder cost 3 to 12 times the fair
  charge (two full passes; `ryu` in 32-bit Wasm about 2,500 fuel per
  Number, twice; per-record validation repeated; 45 fuel per guest word read
  through out-of-line helpers; `memory.copy` of large structs, charged per
  byte). Now: an exact Number fast path (`number::Shortest`; ryu for the
  rest), one pass for outputs up to 16,384 units, a fused record reader
  (`read_record_in`, shared with `validate_aggregate_in`) whose descriptor
  is validated once per call (`RecordShape`), header-only reads of union
  cells (their member is validated by its own reader), inline word readers
  in the shared value core, 64-bit slot shifts, in-place name comparison
  for small records, a one-pass canonical-slot check in `Dictionary::read_in`,
  and no per-record allocations. The `opt-level = 3` library would cut a
  further 18% for 70% more library bytes; it stays size-optimized.
- **Recalibrated.** At the wave-3b integration `JSON_STRINGIFY_VALUE` rose
  from 1,500 to 5,500 by the published method, so the linked encoder costs at
  most the fair charge on every shape below
  ([wave-3b measurement](#wave-3b-integration-measurement-v38)); the table
  and the gap described next are the state before.
- **Remaining gap.** Validation dominates where the fair rate counts one
  value: a dictionary's storage record and its key, value and table vectors,
  a promise's private record, a record's descriptor on first use, and the
  fixed cost of a call (scratch entry, result allocation, roots; about
  1,200). Pinned (`linked_serialization_stays_within_its_measured_cost`) at
  1.1 to 1.2 times the measured ratios. The v37/v38 corpus check compares a
  program's guest instructions through its v38 build that imports
  serialization (charged fair); with the linked build, seven case programs
  cost more than v37 at fair rates (`value15_cases/values.ts` 136,680
  against 89,756, `recursive.ts` 193,880 against 130,055,
  `v37_integration_cases/private_json.ts` 181,457 against 127,028,
  `promise_value_cases/logging.ts` 103,047 against 68,311,
  `value15_cases/dictionary_json.ts`, `v35_cases/mixed.ts`,
  `v35_cases/dictionary_ops.ts`, 2% to 21% more).
- **Output.** Identical to the host imports on every runtime case program
  that serializes or logs a value (`linked_serialization_matches_the_host_imports_on_the_case_corpus`,
  the JSON, data-limit and view case directories included), on 12 generated
  programs of 24 random value graphs each (records, arrays, hashed
  dictionaries with holes, sparse arrays, unions, functions, errors,
  promises, BigInts, lone surrogates, integer-like keys), on records with
  optional fields and private state, and on the failures (cycles, the depth
  ceiling and the logged depth limit, a BigInt `TypeError` after
  validation, the logged output limit); collections during serialization
  keep every value (`linked_serialization_survives_collections`). Case
  mapping of 600,000 units, a 240,001-unit JSON text and a 600,007-unit
  logged line succeed (they trapped or needed the host before); 2,000,000
  units to upper case is the typed memory failure.
- **Logging near the budget.** The logged text of a value now lives in
  guest memory until `log_string16` reads it: logging a value whose text
  needs more memory than is left fails with the typed memory resource where
  the host renderer succeeded (measured: a 1.2 MB text with about 6 MB
  live). An Error message beyond the frozen 1 MiB, which the host renderer
  rejected, is logged.
- **Size.** The library grew from 443,815 to 515,798 bytes (the encoder,
  the owned-text paths and the streaming case mapping). The JSON round-trip
  fixture (`codegen_v2_fixtures/json_log.ts`) is a 259,557-byte module
  (`varta build`). Stack: 5,372 bytes at most (serialization at 5,000
  levels), within half the region's 16 KiB.
- **Imports.** The generic fixtures import only effects
  (`v38_fixtures_import_only_effects`): the keyed view, the Markdown
  fixture, the dictionary workload and the JSON round trip with logged
  records `log_string16`, the TSX application `view_commit`. The v38
  host-operation set is effects only (51 imports).
- **Contract-only host.** Node 24.20 ran the v38 build of the JSON round
  trip (tool-call arguments serialized and logged, the screen parsed back,
  records, a dictionary, an array of mixed values and a union cell logged)
  with a 25-line host that implements only `log_string16` and refuses any
  other import: one import, output identical to `varta dev` and to the
  `build --single` launcher, first run 7 ms.
- **Full gate.** `python3 scripts/verify.py full` passed on `87ac3c9`
  (local macOS arm64 only; Linux not run).

#### Fair host fuel (v38)

Rates (`varta_common::fuel`, published in the
[embedding contract](embedding.md#fair-host-fuel-v38)):

| Work | Fuel | Calibration |
| --- | ---: | --- |
| host import call (`CALL`) | 50 | the cheapest linked call with an argument (`charAt`, 46) |
| string argument or result byte (`BYTE`) | 1 | Wasmtime's `memory.copy` rate |
| JSON input unit (`JSON_PARSE_UNIT`) | 30 | linked scan of long strings (28 per unit) |
| JSON value decoded (`JSON_PARSE_VALUE`) | 1,500 | fitted upper bound of the linked decoder and builder per value (400 to 1,200 measured) |
| schema type (`JSON_SCHEMA_TYPE`) | 3,000 | linked schema read, validation and descriptor check per type |
| schema field or union member (`JSON_SCHEMA_FIELD`) | 3,000 | the same per field (the 40-field `wide_json.ts` case) |
| JSON value serialized or logged (`JSON_STRINGIFY_VALUE`) | 5,500 | the largest per-value need of the linked encoder on the measured shapes, a logged fulfilled promise (5,026: 1,500 plus its 10,578 excess over 3 values), rounded up to 500; 1,500 by symmetry with the parse rate until the wave-3b integration, when the linked encoder cost 0.56 to 3.31 times it ([measurement](#linked-serialization-measurement-v38)). Since v38 links serialization the rate only documents its cost (rated v37 runs and test builds that import it) |
| JSON output unit (`JSON_STRINGIFY_UNIT`) | 30 | by symmetry with the parse rate |

Method: run each workload linked and through the host import without
charges; the difference is the work the host import does not meter; fit
`a·units + b·values + c·schema nodes` and round up to the bound the
measured shapes need, so a host import never costs less than its linked
equivalent (the parse rows above: fair import 1.2 to 1.8 times linked). The
v37/v38 corpus check (`case_corpus_behaves_identically_under_v38`) now
states "v38 never charges more than v37, with the host work of both at the
published rates" (v37 rated by a test-only switch); with a lower value rate
(800, 450) the 40-field `wide_json.ts` and `v35_cases/mixed.ts` cases broke
it, which fixed the final rates. Re-pinned: the codegen v2
"JSON.parse ~2 KB" ceiling is 485,000 (measured 421,398; v37 at fair rates
769,036, ratio at least 1.6 instead of 4.5); every other ceiling and ratio
holds unchanged against v37 at fair rates (table:
`codegen_v2_fuel_ceilings_hold`).

**Per-turn fuel (owner decision, applied at the wave-3a integration).** The
v38 production per-turn fuel is `budget::TURN_FUEL` = 8,000,000
(`Budgets::TURN`; a manifest may lower it, only a host ceiling raises it);
v1–v37 keep `abi::FUEL` = 1,000,000 per execution. The measurements behind
the decision, v38 turns (`console_turn_fuel`) before the integration:

| Turn | v38 fuel |
| --- | ---: |
| keyed view mount, 50 rows | 401,356 |
| keyed view update, 50 rows | 764,901 |
| Markdown blocks, 3 replies | 170,543 |
| 24-member union events, 25 parses | 1,196,812 |
| response of 2 KB parsed | 469,103 |
| response of 10 KB parsed | 2,274,081 |
| response of 20 KB parsed | 4,832,673 |

A console turn that receives a 20 KB response and updates a 50-row view
needs about 5.6M; the 24-member event fixture exceeded 1M in one turn (its
tests declared 4M test-only per-turn fuel; with the 8M default they run on
production budgets). Decision: a v38 per-turn default of 8,000,000 fuel
(about 40% headroom over the 20 KB turn), with the host-call budget
unchanged; the [wave-3a measurement](#wave-3a-integration-measurement-v38)
re-measures these turns.

#### KV fair fuel (v38)

KV storage pays fair fuel like every v38 import
([contract](architecture.md#fair-host-fuel-fair_fuel)): the call, its
descriptor and result bytes, and three KV rates (`varta_common::fuel`):

| Work | Fuel | Calibration (release build, reference macOS arm64 host) |
| --- | ---: | --- |
| KV open, read or commit (`KV_OPERATION`) | 100,000 | the fixed host time of a point read (about 95,000 fuel-equivalent) and of a commit whose check fails (no write: 15,000 to 90,000 over runs), rounded up |
| KV key or value byte staged, committed or returned (`KV_BYTE`) | 10 | a set's host time per value byte, 17.2 fuel-equivalent, charged twice (staged and committed); a get's 2.4 per byte is charged once |
| KV entry returned (`KV_ENTRY`) | 2,000 | a listed entry's host time, 13,070 fuel-equivalent, is covered together with its two decodes at the checked JSON rates (29,700 charged per string entry) |

Method (`kv_fuel_calibration`, `cargo test --release -p varta-runtime --lib
kv_fuel_calibration -- --ignored --nocapture`): KV has no linked equivalent,
so the guest's own speed is the yardstick. A plain guest loop measures the
guest fuel rate (3.17 to 3.23 fuel per ns). Each workload (get of 16, 4,096
and 32,768-unit strings; set of the same; a commit whose check fails; a list
of 10, 100 and 400 entries; reopening a database) runs 40 and 80 times on a
fresh database, fastest of five runs; the marginal wall time minus the
marginal guest fuel at that rate is the operation's host time, which the
guest rate converts to fuel. Fitting `a + b·bytes` and `a + c·entries` and
rounding up gives the rates; with them the charges cover the measured host
time 1.2 to 2.6 times on gets, 2.0 to 2.2 times on lists and 1.1 times per
byte on sets. Two fixed host costs stay outside fuel, bounded by the turn
deadline and the commit cutoff instead (the commit reserve, `kv::COMMIT_RESERVE_MS`):
a commit's durable sync (a set's fixed host time is about 430,000
fuel-equivalent, about 135 µs, of which `KV_OPERATION` covers 100,000) and
opening a database file (about 3,000,000 fuel-equivalent, about 0.95 ms:
SQLite open, schema and descriptor checks, the lock-probe connection and the
worker thread), which a program does once. Charging them as fuel would price
I/O waits as guest work; whether to add a per-open or per-durable-commit rate
is an [open item](#wave-3a-integration-open-items). An unaffordable charge is
the typed fuel error before the effect: an unaffordable commit is never
submitted (`an_unaffordable_commit_is_the_typed_fuel_error_and_writes_nothing`).

#### Collection between and within turns measurement (v38)

Branch `v38-gc-turns` (release CLI on the reference macOS arm64 host;
fuel is deterministic, wall times are not). Main is c033125.

UARP console view entry (`examples/uarp-console`, `TEST_VIEW=chat`,
uarp-sdk 0.13.2), **wall-clock application run** (`varta dev --app
--budget-report`, the timers have real delays, so idle points exist; which
turns are followed by one varies between runs with the host's speed):

| | Main | Branch |
| --- | ---: | ---: |
| Turn 9 (the turn a collection fired in) | 863,896 | 434,215 |
| Largest turn after the first | 863,896 | 435,709 |
| Fuel of the 14 turns | 3,994,329 | 3,590,507 |
| Idle maintenance (6 periods, outside turns) | – | 3,140,454 |
| Peak memory (bytes) | 1,966,080 | 2,162,688 |

The same entry **scripted** (`--view-script src/tests/view/chat.script.json`,
virtual time): the streaming timer chain (turns 2 to 14) leaves no idle
point, and the request that crosses the threshold in turn 9 is large (at
least the managed heap shifted right by `collector::LARGE_REQUEST_SHIFT`), so
it still collects atomically there: turn 9 863,896 → 857,590, turn 10
283,193 → 566,290 (the next cycle runs incrementally), the 19 turns
4,487,916 → 4,686,279 fuel, two idle periods after the chain 1,529,017
maintenance fuel, peak memory 1,966,080 → 3,014,656 bytes. The output equals
`chat.expected.json` and `stream.expected.json` (stream: 2,800,682 →
2,849,722 turn fuel, one idle period of 531,103).

Collector work, a 1500-node linked list built while dropping two small
arrays per node (`main` then one timer, `--app`): main 3,010,132 → 3,313,773
fuel in-turn; the idle period after it runs one complete cycle of the
2.8 MB heap, 1,983,000 maintenance fuel (with 4096-unit steps; the
reference step is now `collector::IDLE_STEP_UNITS`) (two atomic collections
of the same program on main cost 722,685 together). An incremental cycle
therefore costs about 2 to 5 times an atomic collection in total; the
in-turn share is bounded per allocation (`collector::STEP_MIN_UNITS` plus the
request's bytes shifted right by `collector::STEP_BYTE_SHIFT` units), and the
idle period takes the rest outside turns.

Barrier and allocation cost while no cycle marks: two instructions per
reference store and six per allocation (the allocation counter and the phase
test). Keyed view (`codegen_v2_fixtures/keyed_view.ts`, bounded run): mount
of 50 rows 388,892 → 404,694, update of 50 rows 740,267 → 764,121, 120 rows
mounted 961,525 (still within one `abi::FUEL`, the pinned
`keyed_views_fit_one_production_budget` holds unchanged). The case-corpus
parity test (`case_corpus_behaves_identically_under_v38`, v38 never more fuel
than v37) holds unchanged.

#### Wave-3a integration measurement (v38)

Integrated tip `v38-wave3a-merge` (method subtyping, linked conversions and
readers, linked JSON with fair host fuel, KV storage with its fair rates,
the 8M per-turn fuel), debug test build on the reference macOS arm64 host;
fuel is deterministic.

Code generation v2 (`codegen_v2_fuel_ceilings_hold --nocapture`, v37 at
production rules, host work uncharged). Instantiation is not charged in
either profile ([fuel determinism](architecture.md#fuel-determinism)), so
the numbers are the same on every host; the first measurement charged v37
its data-segment copy over 16 KiB macOS pages, which Linux skipped:

| Workload | Size | v37 fuel | v38 fuel | Ratio | v37 / iteration | v38 / iteration |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| number loop | 10,000 | 950,609 | 140,755 | 6.75x | 95.0 | 14.0 |
| 3-char string `===` | 4,000 | 1,113,008 | 305,028 | 3.65x | 278.0 | 76.0 |
| string `===` literal | 4,000 | 1,501,064 | 132,990 | 11.29x | 375.0 | 33.0 |
| `s = s + "x"` | 800 | 983,331 | 50,413 | 19.51x | 1,638.7 | 61.4 |
| dictionary 50+50 | 50 | 620,050 | 157,791 | 3.93x | 15,870.9 | 3,064.0 |
| record field reads | 1,000 | 163,743 | 22,813 | 7.18x | 163.0 | 22.0 |
| function calls | 1,000 | 129,593 | 27,717 | 4.68x | 129.0 | 27.0 |
| closure calls | 1,000 | 426,971 | 305,041 | 1.40x | 426.0 | 304.0 |
| array push + index sum | 500 | 633,334 | 112,105 | 5.65x | 1,413.9 | 220.3 |
| Markdown-like scan | 10 | 527,763 | 65,336 | 8.08x | 62,347.4 | 6,350.0 |
| `JSON.parse` ~2 KB | 40 | 752,651 | 412,273 | 1.83x | 20,432.8 | 9,797.0 |
| list render | 100 rows | 2,923,097 | 148,503 | 19.68x | 50,732.7 | 1,466.6 |
| keyed diff | 100 rows | 4,989,513 | 471,837 | 10.57x | 73,849.8 | 4,695.6 |
| keyed diff | 500 rows | fails (fuel) | 2,349,776 | – | – | 4,734.6 |
| Markdown blocks fixture (chat replies) | 1 | 235,968 | 59,102 | 3.99x | 229,000.0 | 54,676.0 |
| keyed view mount | 10 rows | 164,741 | 82,600 | 1.99x | 15,324.8 | 7,748.4 |
| keyed view update | 10 rows | 878,389 | 156,736 | 5.60x | 141,983.4 | 15,143.4 |

Keyed view at the v38 turn budget (`keyed_view_rows_per_v38_turn`,
8,000,000 fuel and 1,024 host calls, bisection): **621 rows mounted** and
**404 rows updated** in one turn, both bounded by fuel (one production
`abi::FUEL` budget: 120 and 50, `keyed_views_fit_one_production_budget`).

Console turns (`console_turn_fuel`):

| Turn | v38 fuel | Share of the 8M turn |
| --- | ---: | ---: |
| keyed view mount, 50 rows | 388,892 | 4.9% |
| keyed view update, 50 rows | 740,267 | 9.3% |
| Markdown blocks, 3 replies | 169,910 | 2.1% |
| 24-member union events, 25 parses | 1,196,587 | 15.0% |
| response of 2 KB parsed | 458,859 | 5.7% |
| response of 10 KB parsed | 2,228,788 | 27.9% |
| response of 20 KB parsed | 4,761,240 | 59.5% |

Checked JSON per parse (`linked_json_fuel_measurement`, the fuel of one
`JSON.parse(text) as T` beyond the program without it):

| Workload | Text units | v37 | v37 at fair rates | v38 import, free | v38 import, fair | v38 linked (production) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 record | 54 | 737 | 36,547 | 433 | 36,243 | 30,086 |
| 40 records (~2 KB) | 2,181 | 18,755 | 455,335 | 10,495 | 447,075 | 367,042 |
| 1,000 numbers | 4,001 | 275 | 1,627,855 | 175 | 1,627,755 | 886,614 |
| RunEventV1-like member (`unknown`) | 76 | 1,995 | 66,967 | 1,135 | 66,045 | 43,282 |

KV per operation (`kv_operation_fuel`; a `Run` record of the language-guide
shape per entry, marginal over 40 operations on a fresh database):

| Operation | v38 fuel (fair rates) | Guest fuel alone |
| --- | ---: | ---: |
| `get` of 1 record | 185,767 | 7,129 |
| `set` of 1 record | 110,463 | 8,043 |
| `list` of 10 records | 987,076 | 102,864 |

A listed record costs about 98,000 fuel, most of it the two decodes'
schema charges (`JSON_SCHEMA_TYPE`/`JSON_SCHEMA_FIELD` per call: the schema
is read and checked on every `kv_key`/`kv_value`, as on every checked
parse): listing 50 records takes about 4.9M of the 8M turn.

Mixed-kind conversions (`mixed_kind_conversion_fuel`: every pair of twelve
mixed primitives of a `string | number | bigint` array, with fractions,
`1e21`, digit and non-digit strings, `5n` and `-70000000000n`, compared with
`<`, `<=`, `==` and `===`, and each converted with `String`): v37 234,585
fuel with its host work uncharged, **264,295 at the fair rates**, v38
**785,203** (identical output). The accepted regression is 3.0 times the
fair-rated v37 on this fixture (the earlier fixture measured 425,395 against
348,471 unrated): the BigInt beyond 2^32 and the non-digit strings reach the
linked comparison and parser, which cost their instructions (thousands per
call) where the host import charges `CALL` and its bytes.

Module size: the linked library is 443,815 bytes (wave-2 293,907; the
conversions and readers +41,352, linked JSON +109,486, shared without
duplicates: the sum of the two branch deltas is 150,838, the integrated
delta 149,908). Linked JSON's scratch path in the library's allocator adds
about 600 bytes of code to every linked closure (`primitive_to_string` 10,123
code bytes, 9.3 KiB before), so a v38 program whose only linked code is
Number text grew past its v37 build (32,822 against 31,335 bytes;
`unread_module_constants_cost_nothing_at_startup` now avoids Number text, as
`unreachable_async_functions_and_their_frames_are_removed` already did).

Imports of a v38 module that parses checked JSON, stringifies, and gets,
sets and lists KV records (a scratch project, 231,296 bytes): the nine
`kv_*` imports, `json_stringify_owned`, `promise_react`, `promise_settle`,
`scheduler_enqueue`, `promise_unhandled`, `promise_handled` and
`log_string16`; `json_parse_v19` is linked. Its one turn takes 662,988 fuel
and 14 host calls.

#### Compiled JSON decoders measurement (v38)

Owner decision (2026-10-30): compiled per-schema decoders, no cross-call
schema cache. Implemented on `v38-json-decoders` (selector
`json_decoders()`, [contract](architecture.md#compiled-json-decoders-json_decoders)):
every checked parse of a static target runs a checker and builder emitted
for its schema nodes, with the linked `json_parse_v19` as the fallback for
every failure and every input the decoders leave alone. Debug test build on
the reference macOS arm64 host; fuel is deterministic.

Per parse (`json_decoder_fuel_measurement`, the fuel of one
`JSON.parse(text) as T` beyond the same program without it; the linked
decoder alone is the compiler's `compile_generic` test build, the v38
production path before this stream):

| Workload | Text units | v37 at fair rates | v38 linked decoder (before) | v38 decoders | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 record `{id, name, tags}` | 54 | 36,547 | 30,086 | 3,297 | 9.1x lower |
| 40 records (~2 KB) | 2,181 | 455,335 | 367,042 | 105,473 | 3.5x lower |
| 1,000 numbers `1.5` | 4,001 | 1,627,855 | 886,614 | 276,246 | 3.2x lower |
| one RunEventV1-like member, as `unknown` | 76 | 66,967 | 43,282 | 6,515 | 6.6x lower |
| 100 numbers `0.30000000000000004` | 2,001 | 217,855 | 373,975 | 485,872 | 1.3x higher |

Re-pinned: the codegen v2 "JSON.parse ~2 KB" workload measures 154,662
(412,273 with the linked decoder alone; v37 at fair rates 769,036); its
ceiling is 175,000 and its v37 ratio at least 4.5 (was 485,000 and 1.6).
Every other ceiling and ratio of `codegen_v2_fuel_ceilings_hold` holds
unchanged.

Console turns (`console_turn_fuel`, whole turns: building the text,
parsing, using the result):

| Turn | v38 before | v38 decoders | Share of the 8M turn |
| --- | ---: | ---: | ---: |
| 24-member union events, 25 parses | 1,196,587 | 367,543 | 4.6% |
| response of 2 KB parsed | 458,859 | 173,043 | 2.2% |
| response of 10 KB parsed | 2,228,788 | 896,010 | 11.2% |
| response of 20 KB parsed | 4,761,240 | 1,804,828 | 22.6% |

KV per operation (`kv_operation_fuel`) is unchanged: `get` of 1 record
185,767, `set` 110,463, `list` of 10 records 987,076 (about 98,000 per
listed record). KV decodes are host imports (`kv_key`, `kv_value`) that
decode the stored V8 serialization on the host and pay the per-call schema
charges; compiling them needs the entry's text in the guest (a new host
operation returning it, then the same decoders) or a compiled V8 decoder,
an owner decision recorded in the open items.

Where the fuel goes (`varta dev --profile-fuel`): the checker of a short
number literal (`1.5`) costs about 190 fuel (its strict grammar with a
bound check per unit, the exact conversion and its tape entry), an array
element about 30 more, its build about 25; a short string or key about 100
to scan. The build of a record costs about 1,000 fuel, most of it the guest
allocator (about 45 per block) and zeroing (a vector's storage carries the
128-byte minimum sparse bitmap of value layout 18). The schema work the
linked decoder repeated on every call (2,000 to 4,000 fuel per schema
node) is gone: it happens at compile time. Long literals (more than 15
significant digits or decimal exponents beyond 22) cost about 4,800 fuel
each through the linked `primitive_to_number` and its string cell (the
linked decoder's `core` conversion about 3,700): documents dominated by
them cost more than before; a correctly rounded fast path for 16 to 19
digits (Eisel–Lemire, about 10 KiB of power tables) is the measured next
step.

Code size (`decoder_code_bytes_per_target`, code-section bytes the
decoders add to the same program with the linked decoder alone): about
6.0–6.5 KiB shared by every target of a module (scanner, tape, block, cell,
vector and string builders, entries, the linked `primitive_to_number`
closure beyond what `json_parse_v19` already links), plus per target type:
`number[]` 6,524 bytes in all, `Record<string, number>` 6,890, `unknown` (a
seven-member union, its array and dictionary) 6,087, a record with a nested
record 7,872, `Item[]` (five nodes) 7,876, a 40-field record 9,981. Second
call sites of a target add no decoder function.

#### SIMD JSON scanning measurement (v38)

Branch `worktree-agent-ae6712a3a00a2a71c` (from `10ff9ae`): the compiled
decoders scan string bodies and whitespace and copy escaped strings sixteen
UTF-16 units per step with fixed-width SIMD (`simd128`,
[contract](architecture.md#compiled-json-decoders-json_decoders); v38 hosts
must support it, [embedding step 10](embedding.md#module-validation)). Fuel
from `json_simd_tests::json_scan_fuel_per_string_byte` (`--ignored
--nocapture`), debug test build on the reference macOS arm64 host, fuel
deterministic. "Scalar" is the same decoders with the scalar scanner (the
test switch `json_decoder_test_hook::scalar`), whose fuel equals the decoders
before this stream exactly; "scan" is the self fuel of the string scanner
(`<json Text>`) over the bytes between the quotes of every string and key;
"parse" is the whole parse (check and build). The transcript is ~276 KB of
UTF-16 text (200 session events: nested records, long text fields with
prose, code, Cyrillic, emoji and about 3,000 escapes, mostly `\n`).

| Document | Scanner | Text bytes | String bytes | Scan fuel | Scan per string byte | Whitespace fuel | Escaped-string build fuel | Parse fuel | Parse per byte |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| transcript, compact | linked decoder alone | 283,128 | 245,598 | – | – | – | – | 13,270,110 | 46.87 |
| transcript, compact | scalar (before) | 283,128 | 245,598 | 3,046,577 | 12.41 | 5 | 1,652,807 | 5,946,329 | 21.00 |
| transcript, compact | `simd128` | 283,128 | 245,598 | 643,032 | 2.62 | 5 | 531,623 | 2,421,600 | 8.55 |
| transcript, indented | scalar (before) | 343,130 | 245,598 | 3,046,577 | 12.41 | 799,240 | 1,652,807 | 6,824,775 | 19.89 |
| transcript, indented | `simd128` | 343,130 | 245,598 | 642,752 | 2.62 | 375,284 | 531,623 | 2,875,810 | 8.38 |
| one plain ASCII string | scalar (before) | 278,536 | 278,528 | 3,203,105 | 11.50 | 5 | 278,563 | 3,483,297 | 12.51 |
| one plain ASCII string | `simd128` | 278,536 | 278,528 | 208,940 | 0.75 | 5 | 278,563 | 489,132 | 1.76 |

Per scanned string byte the scanner costs **0.75 fuel instead of 11.5
(15.3x fewer)**: a step of sixteen units is 24 instructions against 23 per
unit. On the transcript the scan pass costs 4.7x less (12.41 to 2.62 per
string byte) and the whole parse 2.5x less (21.0 to 8.55 per byte, 2.4M fuel
instead of 5.9M); the 10x target holds for the per-byte term but not for the
transcript average, which fixed costs now dominate: about 55 fuel per string
(entry, the first step, the packed result: 4,600 strings, most of them short
keys) and about 80 per escape (the scalar escape check and a new step after
it), against 0.75 per byte. Building a plain string is one `memory.copy`,
charged one fuel per byte, so a plain long string's parse floor is the copy
(1.0) plus the scan (0.75). Whitespace runs of an indented document cost
2.1x less (a single space after a colon costs the same as before). Escaped
strings build 3.1x cheaper (sixteen units per step up to each backslash, a
letter escape mapped through a nibble table). The decoders' code grows by
374 bytes per module (`decoder_code_bytes_per_target`: 9,048 instead of
8,674 bytes for `Item[]`). The v38 golden `json` entry of
`fuel_determinism_tests` moves 33,789 to 33,439; v37 is unchanged.

Open: the per-string and per-escape terms (a key scanner that returns at the
first quote of its step without the general scanner's prologue, escapes
handled inside a step without restarting it), and the linked decoder, the
decoders' fallback, which stays scalar (46.9 fuel per byte on the
transcript): `varta-intrinsics` is built for plain wasm32, and enabling
`simd128` for it changes the code of every linked intrinsic.

#### Wave-3a integration open items

Historical checkpoint: some items below were completed by wave 3b. Current
remaining work is consolidated in the [integration backlog](#current-integration-backlog).
Owner decisions are retained where stated:

- Compiled per-schema JSON decoders (owner decision: ahead-of-time decoders
  emitted by the compiler, no cross-call schema cache): implemented for
  every checked parse on `v38-json-decoders`
  ([measurement](#compiled-json-decoders-measurement-v38)); KV decodes keep
  their host imports and per-call schema charges (see there).
- Idle collection between turns, then an incremental collector (owner
  decision; wave 3 (f)): implemented on `v38-gc-turns`
  ([measurement](#collection-between-and-within-turns-measurement-v38)).
- HTTP adapters for v38 (owner decision).
- Recorded or simulated network replay for scripted sessions; the bounded native
  `wait_network` step of wave 3 (g) has focused verification.
- KV: the event-loop (async) completion model instead of the synchronous
  wait, `watch` and queues, and the guest mapping of `Date`, `Map`, `Set` and
  BigInt values; whether a per-open or per-durable-commit fuel rate should
  price SQLite's fixed I/O ([KV fair fuel](#kv-fair-fuel-v38)).
- `if (res.ok)` narrowing on required boolean-literal discriminants: implemented
  and verified in stabilization batch 1 (2026-11-01).
- The L0 view protocol's open choices (wave 3 (i)).

#### Wave-3b integration measurement (v38)

Integrated tip `v38-wave3b-merge` (exact fuel determinism, compiled JSON
decoders, linked serialization and logged text with owned text results,
idle and incremental collection, declared response headers), debug test
build on the reference macOS arm64 host; fuel is deterministic and equal on
Linux ([fuel determinism](architecture.md#fuel-determinism)).

Code generation v2 (`codegen_v2_fuel_ceilings_hold --nocapture`, v37 at
production rules, host work uncharged):

| Workload | Size | v37 fuel | v38 fuel | Ratio | v37 / iteration | v38 / iteration |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| number loop | 10,000 | 950,609 | 140,835 | 6.75x | 95.0 | 14.0 |
| 3-char string `===` | 4,000 | 1,113,008 | 305,116 | 3.65x | 278.0 | 76.0 |
| string `===` literal | 4,000 | 1,501,064 | 133,078 | 11.28x | 375.0 | 33.0 |
| `s = s + "x"` | 800 | 983,331 | 50,549 | 19.45x | 1,638.7 | 61.5 |
| dictionary 50+50 | 50 | 620,050 | 161,145 | 3.85x | 15,870.9 | 3,127.6 |
| record field reads | 1,000 | 163,743 | 22,901 | 7.15x | 163.0 | 22.0 |
| function calls | 1,000 | 129,593 | 27,797 | 4.66x | 129.0 | 27.0 |
| closure calls | 1,000 | 426,971 | 305,145 | 1.40x | 426.0 | 304.0 |
| array push + index sum | 500 | 633,334 | 112,271 | 5.64x | 1,413.9 | 220.4 |
| Markdown-like scan | 10 | 527,763 | 65,856 | 8.01x | 62,347.4 | 6,391.6 |
| `JSON.parse` ~2 KB | 40 | 752,651 | 157,702 | 4.77x | 20,432.8 | 3,879.1 |
| list render | 100 rows | 2,923,097 | 153,497 | 19.04x | 50,732.7 | 1,515.0 |
| keyed diff | 100 rows | 4,989,513 | 483,285 | 10.32x | 73,849.8 | 4,806.8 |
| keyed diff | 500 rows | fails (fuel) | 2,405,340 | – | – | 4,844.9 |
| Markdown blocks fixture (chat replies) | 1 | 235,968 | 60,268 | 3.92x | 229,000.0 | 55,706.0 |
| keyed view mount | 10 rows | 164,741 | 86,022 | 1.92x | 15,324.8 | 8,062.4 |
| keyed view update | 10 rows | 878,389 | 161,914 | 5.43x | 141,983.4 | 15,625.0 |

The rows that allocate rise 1% to 4% over the wave-3a integration with the
collector's per-allocation bookkeeping (six instructions while no cycle
marks); the keyed view mount's speedup pin is 1.9 (was 1.99). `JSON.parse`
~2 KB is the compiled decoder's.

Keyed view at the v38 turn budget (`keyed_view_rows_per_v38_turn`,
8,000,000 fuel and 1,024 host calls, bisection): **630 rows mounted** (621
at wave 3a) and **381 rows updated** (404), both bounded by fuel.

Console turns (`console_turn_fuel`):

| Turn | Wave 3a | Wave 3b | Share of the 8M turn |
| --- | ---: | ---: | ---: |
| keyed view mount, 50 rows | 388,892 | 404,694 | 5.1% |
| keyed view update, 50 rows | 740,267 | 764,121 | 9.6% |
| Markdown blocks, 3 replies | 169,910 | 173,098 | 2.2% |
| 24-member union events, 25 parses | 1,196,587 | 386,462 | 4.8% |
| response of 2 KB parsed | 458,859 | 176,443 | 2.2% |
| response of 10 KB parsed | 2,228,788 | 912,410 | 11.4% |
| response of 20 KB parsed | 4,761,240 | 1,837,444 | 23.0% |

Checked JSON per parse (`json_decoder_fuel_measurement`,
`linked_json_fuel_measurement`; the fuel of one `JSON.parse(text) as T`
beyond the program without it):

| Workload | Text units | v37 | v37 at fair rates | v38 import, fair | v38 linked decoder | v38 compiled decoder (production) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 record | 54 | 737 | 36,547 | 36,307 | 30,158 | 3,369 |
| 40 records (~2 KB) | 2,181 | 18,755 | 455,335 | 449,011 | 369,002 | 107,417 |
| 1,000 numbers | 4,001 | 275 | 1,627,855 | 1,627,771 | 886,662 | 276,278 |
| RunEventV1-like member (`unknown`) | 76 | 1,995 | 66,967 | 66,229 | 43,474 | 6,707 |
| 100 numbers `0.30000000000000004` | 2,001 | – | 217,855 | – | 374,007 | 488,196 |

Serialization per value (`linked_serialization_fuel_measurement`; "fair"
is the host import at the recalibrated rates, `JSON_STRINGIFY_VALUE`
5,500):

| Workload | Values | v38 import, uncharged | v38 import, fair | v38 linked (production) | linked / fair | Rate needed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| number `2.5` | 1 | 331 | 5,961 | 2,710 | 0.45 | 2,249 |
| short string | 1 | 335 | 6,085 | 2,697 | 0.44 | 2,112 |
| 1 record (3 fields) | 6 | 350 | 34,472 | 18,237 | 0.53 | 2,795 |
| 40 records (~1.8 KB) | 241 | 384 | 1,376,960 | 431,656 | 0.31 | 1,578 |
| 1000 numbers | 1,001 | 384 | 5,625,960 | 945,848 | 0.17 | 1,500 |
| 40 strings | 41 | 377 | 236,751 | 48,112 | 0.20 | 1,500 |
| 40 integers | 41 | 377 | 229,551 | 36,897 | 0.16 | 1,500 |
| 40 arrays `["a","b"]` | 121 | 377 | 677,951 | 147,148 | 0.22 | 1,500 |
| 40 empty arrays | 41 | 377 | 229,551 | 90,514 | 0.39 | 2,109 |
| 40 empty records | 41 | 377 | 229,551 | 84,525 | 0.37 | 1,963 |
| 40 records `{a,b,c}` | 161 | 377 | 909,951 | 285,124 | 0.31 | 1,620 |
| dictionary, 5 entries | 6 | 350 | 34,472 | 25,674 | 0.74 | 4,034 |
| dictionary, 20 entries (hashed) | 21 | 377 | 120,751 | 95,426 | 0.79 | 4,295 |
| log: Number cell | 1 | −313 | 5,265 | 2,448 | 0.46 | 2,683 |
| log: string cell | 1 | −309 | 5,329 | 421 | 0.08 | 1,500 |
| log: 1 record | 6 | −301 | 33,767 | 18,238 | 0.54 | 2,912 |
| log: 40 records | 241 | −301 | 1,376,217 | 439,690 | 0.32 | 1,614 |
| log: fulfilled promise | 3 | −301 | 16,577 | 15,155 | 0.91 | 5,026 |

"Rate needed" is the value rate at which the import's charge equals the
linked encoder (the old rate, 1,500, plus the linked excess per value);
the largest, 5,026, rounded up to 500 is the published rate. The linked
encoder's fuel is unchanged by the recalibration; only rated v37 runs and
test builds that import serialization pay it (the golden v37 `json rated`
entry 57,680 → 97,680; `value15_cases/values.ts` rated 73,371 → 173,371).

Idle collection (`idle_collection_measurement`, an application instance on
production budgets, 12 events, an idle period after each turn):

| Session | Turns | Turn fuel | Largest turn | Maintenance per idle period | Maintenance total | Peak memory (bytes) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| churn, 400 rows of strings and arrays per event | 13 | 24,426,072 | 2,230,826 | 856,161 to 1,143,620 | 12,880,376 | 4,718,592 |
| JSON responses, 60 records (~5 KB) parsed, serialized and logged per event | 13 | 15,722,625 | 1,327,675 | 108,898 to 148,840 | 1,685,715 | 2,424,832 |

The UARP console entry (`examples/uarp-console`) was not re-measured at this
integration: its pinned uarp-sdk dependency is not installed in the
integration worktree ([collection measurement](#collection-between-and-within-turns-measurement-v38)
holds its branch numbers).

Collector reconciliation: the compiled decoders and every linked intrinsic
allocate through the module's `Allocate` helper, so they run collection
steps and may start a cycle; their stores go into zero slots of blocks
published (or retained) before the next allocation, so they need no barrier
([profile rules](profile-rules.md#idle-and-incremental-collection-incremental_collection)).
`collector_stress_tests::STORE_KINDS` adds the decoders (tape growth and the
linked fallback), linked serialization of a graph being overwritten and the
owned text results, and the stress builds run over the linked-JSON hand
corpus, 24 random decoder schemas, the 12 generated serialization and
logging programs and the JSON limit cases
(`stress_builds_match_normal_builds_over_the_json_and_serialization_corpora`).

Imports: a v38 module imports effects only (51 host operations, none
reading value layouts); `json_stringify_owned`, `value_log_v12` and
`json_parse_v19` are linked or not admitted under v38, KV decodes
(`kv_key`, `kv_value`) remain host imports with their per-call schema
charges.

#### Wave-3b integration open items

Recorded gaps carried into the 2026-11-01 checkpoint (owner decisions where
stated); the review correction batch does not close these optimization items:

- Checked JSON record key order: implemented and verified in stabilization
  batch 1 (2026-11-01): named keys in document order, array-index keys numeric.
  Shared/compiled/host paths and the completed full gate pass.
- The v1–v37 host serializers onto the shared `no_std` encoder.
- A collector tuning pass (the per-allocation bookkeeping above, the
  scripted-session turn-9 atomic collection, the 2 to 5 times total cost of an
  incremental cycle moved to idle periods, peak memory; owner decision: the
  stream's defaults are accepted for now).
- KV: a compiled V8-serialization decoder, or the entry's text in the guest
  for the compiled JSON decoders.
- An Eisel–Lemire fast path for 16 to 19 significant digits (the
  `0.30000000000000004` row above).
- Direct decoding into discriminated-union members.
- `String(1.5)` and BigInt text as linked code cost more than v37's rated
  host calls (`COSTLIER_UNDER_V38`: `unknown_cases/assertions.ts` 24,938
  against 26,683, `value15_cases/values.ts` 173,371 against 178,582).
- Logging a value whose text needs more guest memory than is left fails with
  the typed memory resource (owner decision: a consequence of the
  effects-only host).

#### SDK network contract (v38, wave 1 `w1-sdk-network-contract`)

Inventory items `long-lived-stream-transfer-limits` and
`browser-transport-sdk-headers`: the unmodified uarp-sdk's session stream
(server-sent events for minutes, beyond 512 KiB in total) and its ordinary
requests (which always set `User-Agent`) must work natively and in the
browser host. Implemented on the `net_request_start_v2` path only
([contract](architecture.md#long-lived-transfers-and-host-owned-headers-v38),
[wire rules](embedding.md#transfer-timing-v38)); v37 and earlier keep their
end-to-end deadline, whole-body bound and headers as given.

- Inactivity instead of an end-to-end deadline: the head within one
  per-turn deadline of the start, each body wait within one window, renewed
  by every received chunk, never past the instance lifetime (bounded runs:
  their deadline). No production budget changed.
- `net::MAX_RESPONSE_BODY_BYTES` counts what one read receives; there is no
  total byte bound (time, one host chunk and the guest memory budget bound a
  stream).
- `user-agent` and `accept-encoding` are host-owned (checked, ignored, never
  sent) on both hosts; the native host sends `accept-encoding: identity` and
  fails another coding; the browser admits Fetch-decoded bodies and hides
  `content-encoding`/`content-length` for them.
- One read delivers at most `net::MAX_READ_DELIVERY_BYTES` on both hosts
  (owner decision: cap the delivery rather than document the fatal
  outcome). The v38 byte read path costs about `net::READ_BYTE_FUEL` fuel
  per byte (1 KiB about 0.23 M, 8 KiB about 1.7 M, 32 KiB about 6.7 M fuel
  per read turn), so a read coalesced up to `net::MAX_CHUNK_BYTES` exhausted
  `budget::TURN_FUEL` during a burst; larger received data is now split
  across reads (`/events?count=12&size=60000&pause=0` completes under the
  production budgets natively and in Chrome).
- Owner decision: the header and coding rules stay as documented (native
  fail-closed on a non-identity coding; the other browser-controlled request
  headers, `cookie`, `origin`, `referer`, `sec-*`, …, differ between hosts).
- Verification: native loopback tests (`net_tests`, `net::tests`,
  `net::client::tests`), the transport's Node tests and
  `scripts/test-browser-streaming.mjs` (local Chrome against native, same
  lines, the long stream and the burst). The prototype web host
  (`examples/varta-web-host`) records the new rules as stated deviations
  and does not implement them.

### Current integration backlog

Current outstanding items, grouped by acceptance boundary rather than old branches:

- **GC/lifecycle follow-up:** bounded bitmap clearing on large heaps is committed
  and passes the full gate in `10ff9ae`; the earlier fuel comparison failure is
  superseded. Sustained retained-root/leak/cancellation, peak-memory measurements
  and target lifetime evidence remain open; one large-heap test does not close
  CORE-07 or production readiness.
- **Server:** streaming responses, keep-alive and separate connection/worker
  pools are implemented. K7 H1 still needs Web Request ingress/streamed request
  bodies and remaining host policy; H2 async Wasmtime/pooling, H3 server TLS
  1.3/HTTP/2 and H4 flood/slow-client/performance acceptance remain open.
- **Browser/pilot:** OPFS KV, host effects and native-equal SDK network replay
  are integrated. Run the updated Chrome streaming acceptance after the read
  inactivity change; prove sustained live sessions and the remaining browser
  matrix. The pinned SDK own tests still need the M7 harness.
- **Native UI:** integrated control defaults, mixed whitespace, dialogs,
  positioned painting, transitions and translated boxes have scoped/headless
  evidence. Real-window console/consent/clipboard/link/assistive sessions,
  remaining glyph fallback, transforms and scrollbar painting need acceptance.
- **Core/storage/performance:** broader checked conversion/narrowing and module
  semantics, KV async/watch/queues, sustained cancellation/shutdown, collector
  peak-memory measurements, JSON string scanning and remaining linked numeric
  cost regressions. Budget and serial changes do not themselves prove those
  workloads or full compatibility.
- **Targets/release:** Linux/Windows/Pi5 and current CI/target evidence, wider
  browser/pilot parity, production performance, fuzzing and independent security
  review. GPU color fields are integrated; direct opacity groups and guest
  graphics/3D are separate unfinished work.

The four reproduced review defects are closed by `abea3ac`. Structural thenables
are explicitly unsupported with static/runtime rejection; full adoption remains
future compatibility work. The normal-stack compiler fix removes the former
console-check crash without raising the process stack limit.

### Platform targets and application model

**Checkpoint: 2026-11-01.** The design was accepted on 2026-10-29
([decisions](#accepted-decisions-2026-10-29)); parts are implemented under v38.
The [platform implementation checkpoint](#platform-implementation-checkpoint)
distinguishes delivered foundations from outstanding target acceptance.
[embedding.md](embedding.md) is the normative third-party host contract. The
following text retains the accepted target design; examples of desired commands
or proposed entry forms are not evidence that they are shipped. The owner's goal:

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

The target "no JS engine on the device" is to be met on desktop and Raspberry
Pi 5 by the planned [native renderer](#native-renderer-for-desktop-and-raspberry-pi-5):
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
| Browser, Worker (recommended) | Generated JS host in a dedicated Worker; generated applier on the main thread | `dist/web/`: `app.wasm`, `app.private.wasm`, `app.manifest.json`, self-contained `varta-worker.js`/`varta-host.js` (applier embedded), `index.html` with CSP and SRI | Partial: R1 plus v38 view sessions, packaged PNG/font resources and local `varta dev --target browser` with rebuild/reload through CLI/library; remaining host effects and full multi-browser and pilot acceptance remain open |
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
- **Proposed embedding level A1 (owner decision open).** The
  [embedding contract](embedding.md#conformance-levels) has no level for the
  official application/view host. The proposal: **A1 — application/view host**,
  R2 plus the v38 application instance (long-lived turns, host event delivery,
  close and permission revocation),
  `view_commit` with the L0 view protocol and an applier, the event-source imports,
  per-turn renewal with metered fuel and the browser effect mapping, specified in
  its own embedding section. No host may claim it until the instance and event
  interfaces leave "unstable" and full multi-browser acceptance passes; until the
  owner accepts it, embedding.md keeps its current levels.

#### Application lifecycle and event turns

Current native v38 application mode keeps one long-lived instance and delivers
events as budgeted turns. Bounded `main`/tool executions and the initial HTTP
request model retain their own store lifetimes. The detailed implemented contract
is [turn budgets](architecture.md#turn-budgets-and-application-instances-turn_budgets).
The following bullets describe the accepted design, including proposed entry
syntax; they do not replace the current source API:

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
output and the clock. Before v38, much of the import surface was pure computation
bound to value layouts. V38 now links those pure intrinsics into the guest and
imports effects only. The table preserves the design alternatives; target cold
start and final artifact-size acceptance remain open:

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

- **Positioning status.** Native layout, paint, hit testing and scrolling admit
  the whole positioning row on the wave-2 branch
  ([contract](architecture.md#native-positioned-painting-and-stacking)); a
  positioned box inside an inline formatting context is still refused.
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
  the accessibility tree; names follow the accname 1.2 steps the subset admits
  ([contract](architecture.md#v38-view-protocol-and-tsx-view_protocol-tsx)).
- **Open L0 v0/v1 choices (owner decision).** Raised by the first UARP console
  chat screen (2026-10-30) and not part of the subset until decided:
  `role="separator"` (a divider between message groups; ARIA gives it an
  orientation and, when focusable, a value) and `role="group"` (a set of
  related controls such as a message's action buttons, named by its author).
  Either needs a registry entry in `view::ROLES`, its accname step, the
  AccessKit mapping and a browser/native conformance page; until then the
  application uses `div` without a role.
- **Headless scripts and network requests.** `varta dev --view-script`
  runs on virtual time ([contract](architecture.md#v38-view-protocol-and-tsx-view_protocol-tsx)):
  inputs wait for every runnable task, `{"input":"wait","ms":N}` fires delayed
  timers, and network requests in flight are not waited for implicitly. The
  explicit `{"input":"wait_network","ms":N}` step now drives native I/O under
  a validated wall-clock bound while virtual timer time stays fixed. Timeout
  terminates the session before later inputs. This is bounded real I/O, so a
  real network session stays nondeterministic. `--network-record LOG` writes
  the session's completions to a bounded log and `--network-replay LOG`
  serves them instead of the network, in log order and per script step, with
  every request still authorized; the browser loader takes the same log as
  `networkReplay` ([contract](architecture.md#recorded-network-replay-v38)).
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
reactivity rely on `Proxy` and are out of scope. CSS-only design systems were
assumed usable as stylesheet assets; the 2026-11-06 inventory showed the L0
grammar rejects Tailwind output at its first byte, and full support is the
[K8 track](#k8--tailwind-at-compile-level-agreed-design-2026-11-06).

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
| GPU rendering | Vello family on `wgpu`; evaluate `vello_gpu` first | Apache-2.0 OR MIT; verify selected crates at adoption | Planned GPU path; renderer choice and actual desktop/Pi support require P13a measurements; the original compute-centric renderer is not mandatory |
| CPU fallback | `vello_cpu`, or `tiny-skia` with `softbuffer` | Apache-2.0 OR MIT; `tiny-skia` BSD-3-Clause; `softbuffer` MIT OR Apache-2.0 | For weak or missing GPUs |
| Accessibility | `accesskit` (with its `winit` adapter) | MIT OR Apache-2.0 | Maps to the native accessibility APIs on all three desktop OSes |
| SVG images | `resvg` | Apache-2.0 OR MIT | Artifact SVG only |
| Clipboard | `arboard` | MIT OR Apache-2.0 | v1: in-field copy/paste only |
| Prior art | Dioxus Blitz (HTML/CSS renderer on Stylo, Taffy, Parley, Vello) | MIT OR Apache-2.0; Stylo is MPL-2.0 | A reference and possible source of parts; Stylo implements far more CSS than the subset needs, and the subset's selector language is small enough for our own cascade |

Accepted stack direction: winit, taffy, parley/fontique, Vello-family GPU rendering with a CPU fallback,
AccessKit and resvg; each dependency is justified and pinned exactly (`=`) when it
is adopted. `varta-view` now pins Taffy 0.15.1 (MIT) for an experimental,
bounded box-only layout API and Parley 0.11.1 (Apache-2.0 OR MIT) for explicit-font
text measurement. The CPU raster experiment also pins `tiny-skia = 0.11.4`
(BSD-3-Clause), with default features disabled;
it supplies a portable software fallback without a system GPU or PNG codec.
The experimental desktop preview additionally pins `winit = 0.30.12`
(Apache-2.0) and `softbuffer = 0.4.6` (MIT OR Apache-2.0) for native windows
and CPU frame presentation. The remaining native renderer dependencies still
need adoption and license checks.

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
contained only by keeping the subset small. The later GPU plan below refines
renderer selection and records application graphics separately from UI painting.

**First renderer increment.** Box, text, flex, button, text input with IME and
overflow scrolling, measured on macOS and on a Pi 5: startup time, resident
memory, frame time for the 50-row list and keystroke-to-frame latency.

#### GPU and game graphics: future direction

**Recorded 2026-11-03; P13a optional prototype committed in `be85c981`,
P13b/P13c planned.** Separate three increments:
GPU acceleration of the existing UI, a typed graphics API for Wasm applications,
and library compatibility. The [architecture contract](architecture.md#planned-gpu-rendering-and-application-graphics)
records ownership, limits, fallback and acceptance requirements.

| Increment | Planned work | Exit criterion |
| --- | --- | --- |
| P13a — GPU UI | Optional shared-scene wgpu/Vello GPU renderer implemented; CPU fallback, input revisions and bounded caches retained; acceptance in progress | Same Showcase runs on CPU/GPU in Debug/Release; geometry/text/input checks and startup, input-to-present latency, median/p95 frame cost, memory and package-size measurements; device-loss/fallback checks on declared targets |
| P13b — Application graphics | Separately versioned typed GPU API, buffers/textures/shaders/pipelines, batched commands, composable graphics surfaces, budgeted frame callbacks and game input | Animated 3D scene alongside working TSX UI; validated resource ownership, capability and memory/queue limits, cleanup and device-loss recovery; shader/GPU execution limits specified independently of Wasm fuel |
| P13c — 3D library reach | Inventory and pin a typed 3D library or Three.js port; specify source/language and host API gaps before implementation | Pinned port passes declared game scenes and target/resource tests; no unmodified Three.js/npm compatibility claim from wgpu or type declarations alone |

P13a starts with a bounded prototype and measured backend selection. P13b depends
on that device/presentation foundation and an approved graphics contract; it does
not reuse the UI patch protocol as an unrestricted GPU command channel. P13c
depends on P13b and source admission. P13a desktop-only dependencies are pinned
in Cargo; the runtime/compiler dependency boundary is unchanged.
These increments do not replace the pending P10/M7 and release acceptance work.

Unmodified Three.js in a browser/WebView is a separate future compatibility
option, not an accepted native fallback. Adopting it would require an explicit
decision about the JS engine, Web APIs, distribution costs and execution/resource
guarantees. The current native no-WebView direction remains unchanged. Audio and
other game services also need separate contracts; GPU support alone does not
constitute a complete game runtime.

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
action, never a prompt triggered by guest code. *Superseded in part by the
accepted [permission model](#permission-model) and the P10b shell
status below:* user decisions are stored per artifact hash as stored decisions
(not as the host ceiling, which stays separate), and an optional grant may be
asked when guest code calls `request()` in a user-gesture turn.

**Updates.** Later, and only with C3 provenance: a signed update manifest names the
new payload hash and shell version; the shell verifies the signature and runs the
normal artifact validation before replacing itself.

**Shell status (wave 1, 2026-11-04).** The `varta-shell` binary exists in
`varta-desktop` beside `varta-preview`; both share one window/input/render core
(`src/shell/`). It runs a packaged artifact read and validated exactly as the
companion reads it, with the manifest as the request, an optional `--policy`
host ceiling, exact-operation matching, denied optional grants (no prompt yet)
and KV roots pinned to host-owned storage per artifact hash
([contract](architecture.md#native-shell-varta-shell)). Headless tests cover
tampered/truncated/foreign artifacts, request-only, narrower, empty and
exact-operation ceilings, denial before effect for `net.request` and KV, a
loopback request and a KV round-trip across instances, and a rendered first
frame. On the development Mac `varta-shell` opened a window and presented the
first frame of a packaged view example (log evidence; no screen capture was
possible). Fonts and guest effects followed in wave 2 (below).

**Shell status (wave 2, 2026-11-04).** Artifacts ship their fonts: a v38
configuration's `fonts` list is packaged into a versioned, payload-bound
[resource section](architecture.md#resource-section-artifact-fonts)
(`app.resources` beside `app.wasm`, or container format 2), and the shell
loads every face through `TextMeasurer::new_with_fonts` with real weight and
style selection; synthesis stays refused. Granted `copy_text` and external
links of trusted gestures reach the system clipboard and browser through a
`PlatformEffects` sink, with the existing `command_result` statuses
([native shell](architecture.md#native-shell-varta-shell)). Tests cover the
section round trip, tampered/foreign/oversized/malformed fonts before guest
start, face selection, and positive, refused and denied effects through a
recording sink. The packaged-font Chrome oracle (`native_font_probe` with
`scripts/test-native-inline.mjs`) passes at 1, 1.25 and 2 under the 2/255
bound. On the development Mac `varta-shell` presented the view probe's
regular, bold, italic and monospace lines from its artifact fonts and logged
`invalid_context` for the copy in main; trusted clicks on the copy target and
link were not performed manually (no UI automation was available), so the
gesture paths rest on the headless trusted-input tests; a human still has to
click the copy target and the link once in a live `varta-shell` window. After
review, browser builds carry the same section (every face with its weight,
style and generic families, registered before the first view commit, the
first family as the default), `fonts` and `browser.fonts` are mutually
exclusive, the oracle adds the `monospace` alias and default-family cases
through the applier's own font namespace, `varta build --single` with fonts
refuses a companion without container format 2 (launcher protocol 3), and the
browser launcher is started detached (Linux not exercised). After the second
review the two renderers agree on every admitted way to reach a packaged
generic: the browser base sheet maps the user-agent `monospace` of `pre` and
`code` to the declaring family, controls inherit their family and synthesis
is off, and the shell's packaged measurer
(`TextMeasurer::new_with_packaged_fonts`) sizes a mapped `monospace` as that
named family, without the 13/16 fixed-font ratio Chrome no longer applies;
the bundle and native oracles add `code`/`pre` at absolute, medium and
inherited sizes, and the shell keeps at most `MAX_RUNNING_LAUNCHERS` browser
launchers instead of one reaper thread per opening. The prototype
`varta-web-host` refuses artifacts with a resource section (sidecar or
container format 2) instead of loading fonts. Still open: the manual
trusted-gesture run above, the first-run grant list and prompts with stored
decisions (so `https://*` links are denied), real-font typography acceptance
beyond the synthetic fixtures, manifest `app` metadata, OS bundles, prebuilt
per-target shells, the code cache and UARP console acceptance in the shell.

**Shell status (wave 3, P10b native consent, 2026-11-05).** `varta-shell` is a
host with a prompt seam for the permission broker
([native permissions](architecture.md#native-permissions-p10b)). Decisions
persist per artifact hash in the host-owned
`<data-dir>/sha256-<digest>/permissions.json`, read before the start and
written between turns; required grants without a decision are asked once
before any guest code (Allow and Deny stored even when the start is refused,
Dismiss not); optional grants are asked only for `request()` in a
user-gesture turn. The question appears in a separate always-on-top host
window drawn from the broker's prompt with the shell's own bitmap font; while
it is open the application's window sends no user input to the guest, and an
input delay keeps a click or key meant for the application from answering it.
Rate limits are the browser's decided values (`permission::MAX_OPEN_PROMPTS`
open prompts, and `permission::MAX_DISMISSALS` dismissals before a stored
denial); `--revoke CAPABILITY` stores a denial that a running shell applies to
new operations once it rereads the file; every broker decision is appended to
`permission-audit.jsonl`. The first turn's deadline
now starts after first-run approval. Headless tests with a scripted decision
provider and the `consent-probe` fixture cover first-run approval, refusal,
gesture-only prompts, persisted allow and deny, revocation, rate limits,
audit rows and the deadline; window unit tests show that a covering guest
frame cannot reach an open prompt. On the development Mac the first-run
prompt window appeared over the blank application window with no guest
output, and a start with the required grant stored ran main without any
prompt (`start prompt`, `start kv NotAllowedError`). The manual acceptance
of a gesture-triggered optional prompt in a live window is **not met**: it
was never clicked (no UI automation was available), so that path rests only
on the headless trusted-click tests. Review fixes: the prompt shows every
requested pattern in full on pages and enables Allow only after every page
was shown; a request that does not fit `consent::MAX_RESOURCE_LINES` is
denied without a prompt and audited; decision-file writers hold a lock; the
worker syncs consent on every exit; a guest cannot title its window like the
prompt. `PROMPT_INPUT_DELAY` = 500 ms is an owner decision (2026-11-05).
Second review fixes: a refused start no longer stores a first-run answer
over a revocation made while it asked; a request too long for the native
prompt is reported on stderr, and its native denial (the browser shows it)
is documented as a cross-host difference, still without a build-time
warning; escapes are never split across lines and a backslash is shown
doubled; a page counts as shown only once presented whole. The application
first-turn deadline starting after first-run approval was accepted by the
lead (waiting for a human spends no guest time).
Still open (owner decisions): "allow for this session", a cool-down and its
value, the presentation of a refused required permission beyond the stderr
message and the identity key before C3; also an accessible and dark-mode
prompt, non-ASCII text in the prompt (escaped today) and non-macOS prompt
windows.

#### Browser security model and honest guarantees

| Guarantee | Browser (guest in a Worker) | Notes |
| --- | --- | --- |
| Memory isolation | **Strong** | Wasm linear memory; module-declared maximum checked before instantiation |
| Capabilities and host calls | **Strong only while the generated host is intact** | Any other script in the page's realm (XSS, extensions, third-party scripts) shares trust and could call imports or read guest memory. Mitigate: the guest runs in a dedicated Worker created from the bundle, a strict CSP, Subresource Integrity on the loader, Trusted Types, no other scripts. The origin's own permissions are an outer ceiling the host cannot widen |
| Deadline | **Coarse** | `Worker.terminate()` kills the whole instance; no per-turn graceful interruption. On the main thread there is none (development only) |
| Fuel | **Partial; prototype not conforming** | The initial console bundle meters guest and logging work, with native/Node/Chrome observations and real Worker limit tests. Complete host/CLI integration and effect-wide fair charging remain open; the prototype remains unmetered. Native keeps Wasmtime fuel; broader cross-host equivalence and overhead measurement remain open |
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

V38 implements native HTTP contract v2 with checked UTF-16/binary64 adapters,
fresh request stores and synchronous or asynchronous handlers; see the
[native server contract](architecture.md#http-contract-v2--native-server).
Earlier HTTP v1 and tool v1 retain their profile meanings. Typed binary64 tool
adapters and a common adapter ABI across tools, application/view events remain
planned. The prototype browser host rejects HTTP serving; official browser
admission/effect coverage remains part of the platform batch.

#### Storage, binary data and crypto

Later increments, each its own capability or capability-free contract:

- KV storage: implemented natively under v38 as Deno KV file compatibility
  (owner decision, wave 3; [contract](architecture.md#kv-storage-kv)); a
  browser backend with the same guest API runs the
  [agreed OPFS design](#browser-kv-storage-agreed-design-wave-3) (wave 3).
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
the artifact hash, the prompt seam, audit log). Browser prompt UI and the async
answer path now have focused acceptance below; the native shell's first-run
approval, prompt window, stored decisions and audit log have focused headless
acceptance ([shell status, wave 3](#desktop-packaging)); broader P9 effects and
"allow for this session" remain. V1 narrows two points: a
permission in `prompt` state is not asked at its first use (only through
`request()`), and persistence is supplied by the host. Native hosts use decision
files; the generated browser host now offers an explicit origin-local IndexedDB
store, acknowledged live revocation and host-owned consent (focused evidence below).
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

#### Platform implementation checkpoint

Working tree after `abea3ac`, updated 2026-11-02 after the consolidated 2946-test
gate: “implemented” below means the stated subset;
full acceptance in the following design table remains required.

| Increment | Delivered | Still needed for acceptance |
| --- | --- | --- |
| P0 | Published embedding contract, generated registry tables and drift checks | Maintain parity as boundaries change |
| P1 | Prototype host: 117 shared Node cases; generated registry and official checked browser Worker/loader, metering parity observations | Official Node conformance selector, remaining entry-point surfaces and full browser matrix |
| P2 | V38 effects-only host boundary: pure intrinsics, JSON and formatted values linked into guest Wasm | Browser/Pi cold start and module-size measurements on the final artifact |
| P3 | Current-target source-free packaging | Cross-target launcher selection, Windows CI and real Pi 5 delivery |
| P4 | Tool ABI v1 and v38 sync/async HTTP v2, with source-free acceptance | Structured tool entry-point v2 and Node parity |
| P5 | Native instances/budgets/broker/profiling; checked browser instances, consent/revocation and named host turns | Wider cross-host ordering, target UI budget measurements and sustained shutdown/resource acceptance |
| P6 | Native async HTTP(S), streaming/bytes/abort and bounded scripted waits; checked v38 browser Fetch adapter; recorded network replay natively and in the browser bundle (Chrome acceptance `scripts/test-browser-replay.mjs`) | Final native/browser pilot parity, replay of the console's own sessions and Firefox/WebKit replay runs |
| P7 | L0 validation, headless tree/cascade/accessibility, opt-in headless geometry with explicit fonts for the admitted layout subset (local Chrome oracle within ±1 px) and official transactional browser DOM/event adapters | Geometry beyond that subset (the UARP console views are refused), patch-stream replay through the official applier and cross-renderer layout/accessibility acceptance |
| P8 | Compiler TSX/templates and example-local reactive L1/keyed lists | Reusable package delivery and complete cross-host example acceptance |
| P9 | Official bundles/CSP/SRI/metering, views/resources/consent/events and local dev/reload covered by the consolidated gate; scoped Chrome/Firefox/WebKit matrix passes | Portable matrix driver, wider controls/effects/layout, three-browser pilot acceptance and measured target costs |
| P10–P10d | Experimental CPU controls/layout, retained renderer and pointer feedback with scoped macOS Showcase evidence; `varta-shell` runs packaged artifacts with manifest grants, a host ceiling, per-artifact KV storage and host-owned consent (first-run approval, gesture-only prompts in a separate host window, stored decisions, revocation, audit log) ([status](#desktop-packaging)) | Full admitted painting, clipboard/IME/accessibility, current UARP session, trusted delivery policy, macOS/Pi/other-target acceptance and end-to-end measurements |
| P11 | Accepted later hooks-subset direction | Specification and implementation after L1 acceptance |
| P12 | Native KV plus selected binary/encoding/Date/crypto APIs | KV async/watch/queues, wider values, remaining binary/crypto/Intl providers and cross-host parity |
| P13a–P13c | Optional GPU UI prototype implemented with Metal recovery; application graphics and 3D-library work remain planned | Prototype and measurements, versioned GPU contract, device/resource acceptance and separately proven library reach; [scope](#gpu-and-game-graphics-future-direction) |
| Svelte | Deferred design direction | Only after P8 is stable |

#### Increments and acceptance

The following table preserves the accepted increment scope and exit criteria;
its ordering is not a list of wholly unimplemented tasks. Current delivery is
tracked in the checkpoint above and [next batches](#next-delivery-batches). Every
increment keeps positive, negative and resource tests, Wasmtime validation and,
where a JS host exists, differential cases under Node:

| # | Increment | Acceptance | CORE | Pilot |
| --- | --- | --- | --- | --- |
| P0 | Embedding contract ([embedding.md](embedding.md)) with drift tests | Doc-only Node T1 host matches native (done in the probe) | 13, 14 | — |
| P1 | Generated JS host (T1, R1, R2 subsets) + conformance host selector for Node | Every conformance case whose imports the host supports matches native; probes become fixtures | 13, 15 | — |
| P2 | Thin host: Rust intrinsics linked into the guest under a new profile | Only the intrinsics an application uses are linked; JSON/Headers/value programs run identically in Node and Wasmtime; `app.wasm` growth, fuel deltas and cold start in the browser and on the Pi 5 recorded | 01, 10, 13, 16 | M1–M4 programs |
| P3 | Cross-target native packaging (`--target`, offline launcher metadata, musl aarch64) | Same artifact runs on a real Pi 5 (16 KiB pages) and on macOS/Linux; Windows CI | 14 | — |
| P4 | Structured entry points v2 (binary64/UTF-16 tools and HTTP) | A uarp-sdk-shaped typed tool with records/unions runs in native and Node hosts | 10, 13 | M2–M3 |
| P5 | Application lifecycle: long-lived instance, event turns, per-turn budgets, timers delivered as tasks; [permission broker](#permission-model), states and `query` | Timer and event ordering tests against Node logs; trap kills the instance without automatic restart; per-turn budget values set from measurements. **Runtime half implemented under v38 (`turn_budgets()`)**; pending: cross-host ordering cases for host events and per-turn UI values from target measurements; per-function fuel profiling is implemented | 07, 08, 09, 17 | M4–M5 (timers, backoff) |
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

Current interleaving: M1–M6 source admission and the native async/application
foundations are integrated. Finish their runtime/package acceptance alongside
P1/P9 browser delivery and P3/P10 native targets. M7 remains open until the SDK
own tests and the same-artifact cross-target console session pass; it is not
closed by a per-file compile probe.

##### UARP console (cross-target acceptance application)

**Status: removed (owner decision, 2026-11-06).** The console, the TSX Showcase and
the web-host prototype were experiments and were deleted with all of `examples/`,
including the SDK acceptance drivers below. uarp-sdk stays the
[pilot package](#pilot-acceptance-package-uarp-sdk); the cross-target acceptance
application and the SDK-own-test harness (M7) must be rebuilt in a new form. The
text below is the historical record.

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

**Status: product entry (wave 2, `w2-console-real-client`, 2026-11-04).** The
console has its product entry: `examples/uarp-console/varta.json` builds
`src/app/main.tsx` into one `app.wasm` and manifest that reads the API key from
KV, constructs the real `UarpClient`, creates a session with the tenant's Head
Agent and follows its `event_schema=1` stream through the chat model, reconnecting
with the model's `Last-Event-ID` by the console's stream rules. Its manifest
requests `net.request` on the API base URL (`GET`, `POST`, four declared response
headers), `kv.read`/`kv.write` on the key database and, beyond the manifest
described above, `time.timer` and `time.now`, which the unmodified SDK needs on
every request and stream connection (console README). Owner decision
(2026-11-05): these two `time` grants are accepted for the product manifest, with
every grant kept exact (no wildcard beyond the API base URL). `scripts/test.sh` runs the
product's start, send, approve, answer, drop and reconnect end to end against the
loopback mock replaying the recorded scenarios, including a stream that outlives
one per-turn deadline; the settings screen, session stitching, live-service and
cross-target acceptance remain open, and the new pilot findings F4–F7 are in the
console README.

**Status: session stitching and the remaining screens (wave 3,
`w3-console-stitching-screens`, 2026-11-05).** The console starts from the Head
Agent's newest page of sessions: the newest session, when active, is loaded from
its transcript and followed without duplicates, older sessions load as dated
history segments on scroll (one per request, across pages by the list's cursor) in
creation order, and a message sent with no live session (also after
`session_closed`) creates one (`sessions.create`) before it is sent. The settings
screen (an https base URL, API key) writes through the KV database only, with a
confirm dialog before forgetting the key; the empty chat, the context panel
of the runs and agents the conversation mentions, the loading and error banners and
the toasts use the shared stylesheet. The manifest's grants are unchanged.
`scripts/test.sh` replays recorded tenant and transcript responses and a session
list composed from recorded bodies (no list page was recorded) through the loopback
mock, checks that the test key appears in no settings-group output and that no
string of the API key form appears in the product's Wasm or manifest, and compares eight new headless screen snapshots;
the run, the measurement of the follower over 1000 reconnects and the new pilot
findings F8–F9 are in the console README. Live-service validation and cross-target
acceptance remain open.

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
JS/TS compatibility. The core remains the priority; the UARP console is a separate
acceptance example and no engine crate depends on its domain types.
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

Current checkpoint: implemented value/ABI foundations extend through v38's
layout 18 and linked intrinsics; remaining work is summarized in CORE-01/02/05/07.
The following increment records describe each older profile at introduction;
words such as “current limit” are scoped to that profile, not the newest v38.

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

Subsequent profiles implemented broader shape unions, sparse collections and
further array operations. General prototype values/coercion and newer structured
Tool/HTTP adapters remain open; no legacy field/schema/handler is reinterpreted.
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

The v32–v38 language and async streams described below are integrated in `main`.
Per-profile descriptions retain their historical admission limits; later
profiles may extend them. Use the CORE ledger and current integration backlog
for outstanding work rather than treating the old increment order as a queue.

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
<a id="k44--suspension-scheduling-and-async-planned"></a>

#### K4.4 — Suspension, scheduling and async (implemented foundation; expansion open)

Current status (2026-11-01): all six foundation steps below are implemented
within their checked subsets and integrated in `main`: v33 frames/generators,
v34 jobs/async functions, v35 liveness/tagged promises, v36 timers/abort/async
generators, v37 async HTTP(S)/streams and v38 application turns. Their contracts
remain profile-specific. The design and initial per-step notes below preserve
the original decisions; historical “Open” lists must be read against later
profiles and the [current ledger](#core-completion-ledger).

Remaining expansion includes structural thenable adoption (explicitly rejected
since the review fix), top-level await and general iterable/protocol forms,
async inbound HTTP integration, deterministic replay and complete cross-host
shutdown/cancellation acceptance. Outbound async I/O, abort listeners, Promise
combinators/withResolvers and liveness frames are already implemented; they are
not future foundation tasks. [Next batches](#next-delivery-batches) and the
[integration backlog](#current-integration-backlog) own the current queue.

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
on a development host. The selected UARP console is the separate product example
used for acceptance; it does not add domain models to the engine.
### K7 merged, values approved (2026-11-06)

The K7 stages H1–H4 are merged into `main` after the owner reviewed the
values set during implementation and accepted them as registry maxima that a
host may only lower: `MAX_CONCURRENT_REQUESTS` 1024 within a 1 GiB server
memory ceiling (`SERVER_MEMORY_BYTES`), 16 MiB request bodies, 60 s body
idle, 300 s request lifetime, 60 s write idle, and HTTP/2 bounds of 100
streams per connection, a 16 KiB header list and closing a connection after
100 abandoned requests. Contract v2 request heads were raised from 32
fields / 4 KiB to `http::V2_MAX_REQUEST_HEADERS` /
`V2_MAX_REQUEST_HEADER_BYTES` (owner decision, Node's 16 KiB default), so
bearer tokens and cookies fit; contract v1 and response headers keep the
earlier bounds. Open: a per-stream write-idle bound for HTTP/2 streams whose
client never opens its window, h2c, HTTP/3, certificate reload, and Linux
and sustained-load benchmarks.

### K9 C — expressions and functions (2026-11-06)

Owner decision (2026-11-06, K9): the first release covers the whole
TypeScript language except `any`, with Node 24 as the runtime oracle and tsc
5.x as the type-system reference. Track C admits the expression and function
forms under the v38 selector `expression_forms()`
([contract](architecture.md#expression-forms-expression_forms)); v37 and
earlier keep every old diagnostic. No host ABI, value layout, frame catalog
or scheduler change, so the browser host runs every form unchanged.

| Construct | Status | Evidence (`expression_form_cases`) |
| --- | --- | --- |
| `await` in call/`new` arguments, operands, templates, conditions, object/array literals, assignments, optional chains | Admitted (value blocks split around suspensions) | `await_positions`, `await_arguments`, `await_operands` |
| Top-level `await` (native and browser schedulers) | Refused: module code runs only pure constant initializers; needs module-level evaluation (calls and statements at module scope) first | refusal test |
| Object-literal methods with `this`, shorthand methods, fluent chains | Admitted (methods bound to the object; copies of such objects refused) | `object_methods` |
| Object-literal getters and setters | Refused: closed records have data fields only | refusal test |
| Constant computed keys (string/number literals, templates, `const` bindings, module constants) | Admitted; symbol keys wait for track D | `computed_keys` |
| Tagged templates, `String.raw` | Admitted; `TemplateStringsArray.raw` refused (cooked strings only), strings array fresh per evaluation | `tagged_templates` |
| Explicit `this` parameters (`f.call(o)`, `o.m()`) | Admitted for binding/`this` bases | `receivers` |
| Contextually typed class-field arrows | Admitted (annotated parameters; return annotation or evident body type) | `receivers` |
| Destructuring: parameters of arrows, functions, methods and callbacks (array methods, iterator helpers), tuples with rest, nested, defaults, assignment patterns, `for...of` heads, iterables | Admitted (iterables taken before targets run) | `destructuring`, `callback_patterns`, `iterators` |
| Spread in calls and `new` | Admitted for static spreads (literals, tuples); `Math.max(...xs)` is K9 D's | `spreads` |
| Hand-written iterators (`{ next() }` behind `[Symbol.iterator]()`) | Admitted for classes (body runs at the first `next()`) | `iterators` |
| `using` / `await using` with `Symbol.dispose`/`asyncDispose` | Admitted; `SuppressedError` (track D) not built | `using` |
| `new.target` | Admitted: `new.target.name` (v32) and comparisons | `operators` |
| Structural thenables in `await`/`Promise.resolve` | Admitted for statically known `then` shapes, job order as Node | `thenables` |
| Comma, `void`, `delete`, `in`, `instanceof`, optional chains, `&&=`/`\|\|=`/`??=`, `**=`, labelled blocks, `switch` fallthrough | Admitted (comma new; the rest pinned) | `operators` |
| Generators driven by `next`/`return`/`throw` | Admitted; results now `{ value, done }` in creation order | `generators` |

Every track-C probe of the K9 brief (`a29`–`a33`, `a41`, `a46`–`a49`,
`a57`–`a61`, `a63`, `a64`, `a67`, `a68`) now compiles and prints Node's
output; of the brief's 89 programs 50 run (43 compiled before). Still
refused: top-level `await`, object-literal accessors and `raw`. Fuel: the v38 iterator
result layout is `{ value, done }`; no fuel golden changed. Conformance:
eleven feature rows and fifteen cases (after the K9 D, K9 A step 2 and K9 B merges:
190 passed, 5 known unsupported of 195).
### K9 — TypeScript completeness and the first release (agreed, 2026-11-06)

**Owner decisions.** The first release targets **macOS (arm64) and the web**;
Linux and Windows follow. Varta covers **the whole TypeScript language** (the
TypeScript Handbook and ECMAScript 2025) except untyped code: `any`, implicit
`any`, untyped JavaScript and `.d.ts`-only packages stay rejected (research
thesis). Node 24 is the runtime oracle for expected output and tsc 5.x the
type-system reference; everything follows the published standards, with spec
sections cited in the contract.

**Baseline.** 89 probe programs of common TypeScript constructs against v38:
43 compile, `any` is rejected by design, 45 are not supported (type-level
operators, enums and namespaces, overloads, setters and static fields,
decorators, `await` inside expressions, object-literal `this`, tagged
templates, `Symbol()`, weak collections, Proxy, local-time `Date`, `Intl`,
RegExp backreferences, `sort` with a comparator and others).

**Tracks**, in parallel on separate branches, merged one by one through the
standard process (each branch's full gate, conflicts resolved, a combined
gate):

- **A — type level:** mapped and conditional types, `infer`, template literal
  types, every built-in utility type, precise `keyof`/`T[K]`, `typeof` and
  `keyof typeof` type queries, intersections, type predicates and assertion
  functions, `satisfies`, polymorphic `this`, call and construct signatures,
  variadic and optional tuple elements, `never` exhaustiveness.
- **B — declarations and classes:** all `enum` forms, `namespace` and
  declaration merging, abstract classes, function and method overloads,
  setters, static fields and blocks, class expressions, standard decorators
  and `accessor`.
- **C — expressions and functions:** `await` anywhere and top-level `await`,
  object-literal methods and accessors with `this`, computed keys, tagged
  templates, destructuring everywhere, spread in calls, hand-written
  iterators, `using`, structural thenables and the remaining operators.
- **D — ECMAScript 2025 library:** complete Array/Object/String/JSON/Promise
  APIs, iterator helpers and Set methods, `Symbol()` values (a new value tag
  under the CLAUDE.md versioning rules), weak collections integrated with the
  collector, local-time `Date` with the time zone as a recorded host input,
  full RegExp with matching bounded by fuel, Proxy and Reflect where sound,
  and `Intl` through ICU4X with deterministic data.

**Release definition (owner, 2026-11-06).** The release ships a full
implementation: every gap found by the tracks is closed by the standard, and
no deviation from Node/tsc stays documented unless the owner approves it
(approved so far: a lying type predicate throws `TypeError` at the narrowed
read). Follow-up tracks after the first merges: A3 (shared covariant arrays
with checked reads, the remaining refused type forms), B2 (standard
decorators on class values, run-time TDZ, index-signature JSON), C2
(object-literal accessors, `TemplateStringsArray.raw`, dynamic `this`,
top-level `await`, `SuppressedError` in `using`), D1 (Node console with a
standard-error host operation, JSON reviver/replacer, `structuredClone`,
`normalize`, `Promise.any`), D2 (Symbol values and keys, integrity levels,
weak collections), D3 (full RegExp), D4 (local-time `Date`, `Intl`), then
Proxy/Reflect. Release work after the language tracks settle: profile
freeze, macOS and web distributions, Chrome acceptance reruns, security
review and release notes.

**K9 A record (2026-11-06, type level).** Admitted in v38 under the
selector `type_level` ([contract](architecture.md#type-level-type_level),
[rules](profile-rules.md#type-level-type_level)), all evaluated at compile
time over resolved types: conditional types (distribution, `infer` with
constraints, recursive generic aliases), mapped types (`as`, `+/-readonly`,
`+/-?`, homomorphic over records, tuples and arrays), template literal and
intrinsic string types, every built-in utility type, `keyof`/precise
`T[K]` (also for computed record reads), `typeof`/`keyof typeof` queries of
module values, record intersections, call and construct signatures and
constructor types (class values become constructing closures), named,
optional, rest and variadic tuple elements and `[]`, type predicates and
assertion functions (predicate-narrowed reference reads are checked at run
time), `satisfies`, polymorphic `this`, `never` exhaustiveness over literal
sets, and tsc's literal inference (`const` literal types, `let` widening,
`as const` literal tuples and records), which also lets the Tailwind scanner
enumerate literal-typed class compositions. Of the A probe programs, every
one compiles except `a50_symbol` (track D: `Symbol()` values). Refused by
name (sound static layout missing, or owned by another track): `unique
symbol` (needs track D's Symbol values), pattern template literal types
outside `extends`, intersections of function types and branded primitives,
overloaded call/construct signatures and callable objects with properties,
mapped types and `keyof` over class instances, `typeof` of function-local
bindings, `infer` over declared generic types and optional tuple elements,
`this is T` method predicates and predicate narrowing of `unknown` to object
types. Evidence: `varta-runtime` `type_level_tests` (ten Node 24 logs,
v37 rejection of each), the compiler scanner test and nine new conformance
rows under the new `typescript_handbook` origin (conformance 135 → 144
passing of 140 → 149 cases). Owner decisions open: whether a lying type
predicate should keep its `TypeError` (Node continues with the wrong type)
and whether branded primitives (`string & { __brand: … }`) may be admitted
as their primitive.

**K9 A step 2 (2026-11-06).** Owner decisions: a lying predicate keeps its
`TypeError` (an intentional deviation, like checked `as`); branded
primitives are their primitive; `typeof` of locals is supported. Admitted
in the same selector: branded primitives, `typeof` of locals, overloads and
function intersections, generic call signatures, `this` in object types,
`keyof`/mapped/utility types over classes, arrays, tuples and primitives,
pattern template types (as `string`), `infer` through declared generics,
union patterns and optional elements, elements after a rest element,
subclass/`unknown`/`this is T` predicate narrowing, and class subtyping in
containers (track D's finding; `B[]` to `A[]` copies, a second intentional
deviation). Still refused: callable objects with properties, overloads with
type parameters or rest parameters, generic construct signatures,
`unique symbol` (track D's Symbol values). Evidence: five more Node-pinned
`type_level_cases`, the deviation test, a second language-guide example and
conformance 155 → 160 passing of 160 → 165 cases.

**Release readiness** additionally needs, after K9 and K8 merge: the release
profile frozen (v38 becomes a released profile; later admissions go to a new
one), the macOS arm64 and web distributions built and verified from a clean
checkout, the open Chrome acceptance runs (streaming, inline probes) repeated,
an independent security review of the host surface, and release notes. Each is
recorded here when it starts.

### K8 — Tailwind at compile level (agreed design, 2026-11-06)

**Goal (owner).** The developer writes ordinary TSX with Tailwind classes of the
latest version and no hand-written CSS, and gets the result modern web
technology gives, with Varta's determinism and typing. Varta does everything
under the hood, optimization and browser compatibility included, through Wasm.
Tailwind plugins and other extensibility come later, once Tailwind compiles
straight into the artifact.

**Owner decisions (2026-11-06).**

- Varta compiles Tailwind itself in Rust; no Node in its toolchain. The
  official `tailwindcss` package is only a test oracle.
- Varta supports web technologies without JavaScript: native applications run
  with no JS; on the web JS is only the bridge between Wasm and browser APIs
  (recorded in `CLAUDE.md`).
- Web stays web, native stays native: the web target keeps the real DOM, with
  Varta's cascade and style resolution in Wasm and the bridge setting computed
  styles; no canvas renderer replacing the DOM. Native keeps Varta's renderer.
- Class names follow the HTML class model (any non-whitespace characters
  without controls, bounded length), with strict escapes in selectors.
- TSX accepts `className` as an alias of `class`; both on one element is a
  compile error.
- `@plugin` and `@config` (JavaScript) are out of the first scope; CSS-first
  `@theme`, `@utility` and `@custom-variant` are in.

**Evidence.** Tailwind 4.3.5 output was run through the L0 grammar
(scratch inventory, not in the tree): every sheet fails at byte 0 (its
`/*!` header), then on `@layer`, `@property`, escaped classes and `>`. A
five-component application accepts 25% of its 588 class uses; the full
surface (39,135 candidates, 12 MB) accepts 5%. Theme colours in `oklch()`
pass the grammar through `var()` and render in Chrome but not natively: an
existing parity gap.

**Tracks.**

- **T1 compiler** (new crate, used by `varta build`): candidate parser,
  variants, utilities, theme, Preflight, `@apply`, `@utility`,
  `@custom-variant`, arbitrary values and modifiers, property order and
  canonical output (no comments, flattened nesting, normalized media, static
  `@supports`). Classes come from the module graph's string, template and JSX
  literals, never a filesystem walk; a `class` whose type is a finite union of
  literals is enumerated exactly, a template hole that builds a class is a
  diagnostic. Oracle tests compare candidate CSS with the official compiler.
- **T2 engine**, one Rust implementation for native and the web: S1 class
  tokens and escapes; S2 sheet structure (`@layer`, `@property`, media
  features as recorded host inputs); S3 values (`calc`, `oklch`/`oklab`/
  `color-mix`, units, keywords); S4 layout and text properties (logical
  properties, `inset`, `translate`/`scale`/`rotate`, `aspect-ratio`, …); S5
  selectors (combinators, `:where`/`:is`/`:not`/`:has` with work bounds,
  structural and form pseudo-classes, `data-*`/`aria-*` attributes,
  pseudo-elements, type selectors); S6 paint (shadows and rings, gradients,
  filters, masks, `clip-path`); S7 animation (`@keyframes`,
  `@starting-style`); S8 `@container`, `!important`, remaining media.
  Measured acceptance of the application's class uses after S1–S6: 27%, 38%,
  72%, 86%, 96%, 100%.
- **T3 web bridge**: move the browser host's view model, stylesheet grammar,
  cascade and custom-property resolution (about 1,700 lines of JS) into a
  wasm32 shim of the Rust engine, as the KV codec moved; the JS keeps DOM
  calls only.

**Security.** `url()` stays refused (or resolves at build time to artifact
resources only), `@import` resolves only at build time, `image-set()`,
`expression()`, `attr()` and `element()` stay refused, `:visited` never
matches, environment media features are recorded host inputs, and `:has()`,
descendant matching and infinite animations are charged against work and
frame bounds.

**T1 record (2026-11-06).** Implemented in the new tooling crate
`varta-tailwind` and wired into `check`/`dev`/`build`
([contract](architecture.md#build-time-tailwind-stylesheets-k8-t1)):

- Ported from Tailwind CSS v4.3.5: candidate and variant parsing, every
  built-in utility and variant (and the legacy utilities of its
  compatibility layer), theme, Preflight, `@import` of the embedded pinned
  package, `@theme`, `@apply`, `@utility`, `@custom-variant`, `@variant`,
  `@source inline()`, `--theme()`/`--spacing()`/`--alpha()`/`theme(--var)`,
  `@reference`, `important`/`prefix()`/`theme()` import parameters, the
  property sort and `optimizeAst` without polyfills; data tables stay data.
- Varta's canonical form on top: no comments, flattened nesting (CSS
  Nesting 1), `@supports` resolved against the L0 grammar (Conditional 3),
  Media Queries 4 ranges as `min-`/`max-` features with `not`.
- Classes come from the module graph (`varta_compiler::
  scan_classes_with_profile`): literals, template text, literal types and
  TSX values; a class composed at run time is a located error. `className`
  is an alias of `class`.
- Rejected with typed errors: `@plugin`, `@config`, file `@source`/`source()`,
  non-embedded `@import`, dot-path `theme()`.
- Oracle conformance: committed subset of 2,986 candidates and eight whole
  stylesheets, all matching after canonicalization; the full corpus
  (39,152 candidates, manual run) matches 39,152/39,152.
- Waiting for T2: the generated sheet is stored and checked, but the L0
  grammar refuses a full build at Preflight's `display: none !important`
  (S8) and static tokens such as `w-1/2` at the TSX attribute (S1). The CLI
  tests pin these first violations; the end-to-end test is ignored until T2.
  (Since the T2 standards items and T3 merged: static tokens apply, and the
  first violation of a full build is the theme's `:root, :host`, stage S5.)
- Not in T1: JavaScript plugins/config, `@source` file globs, source maps,
  IntelliSense suggestions and candidate canonicalization.
- Follow-up (owner decisions 2026-11-06): composed class names enumerate
  through the checker's finite string-literal types (bounded by
  `MAX_CLASS_COMBINATIONS`; other holes stay errors), and the entry may
  `@import` project stylesheets by relative path through the project
  loader under the module path policy (Cascade 5 placement and
  conditions, cycle, depth, count and byte bounds). Closed by K9 A's
  literal inference: an unannotated `const x = "a"` and `as const` tuple
  elements have literal types and are enumerated without annotations.

#### K8 T2 S1–S3 — engine tokens, sheet structure and values (2026-11-06)

Owner directive of the same day: everything by the published standards,
Chrome only as the oracle. The engine contract with its specification
sections is [view CSS engine](architecture.md#view-css-engine-k8-t2-and-t3).

- **S1.** CSS Syntax 3 tokenizer (`view::syntax`: preprocessing, comments,
  escapes, numbers, url tokens, §9 serialization); stylesheets parsed on
  tokens, forbidden constructs found after decoding; HTML class model in
  `set_class`, TSX and selectors (`css::class_list`, 256-byte names).
- **S2.** `@layer` (statements, blocks, nesting, anonymous; Cascade 5 order
  across sheets, unlayered last), `@property` (syntax, inherits,
  computationally independent initial values; reset per element when not
  inherited; syntax mismatch computes as `unset`), comments, Media Queries
  4/5 (`stylesheet::media`: lists, `not`/`only`/`and`/`or`, range syntax,
  `height`, `orientation`, `hover`, `pointer`, `any-*`,
  `prefers-reduced-motion`, `prefers-contrast`, `forced-colors`) with
  `Environment.height` and `Environment.preferences` as recorded host inputs
  (runtime and shell pass the viewport height).
- **S3.** `css::math` (Values 4 §10), `css::color` (Color 4/5, CSS gamut
  mapping), every length unit, `opacity` percentages, numeric
  `font-weight`, `white-space: pre`, `text-align: start | end`, any
  `transition-property` names, `cubic-bezier()`/`steps()` in transitions.
  Values are lowered at computed-value time (`css::lower`) to the host
  grammar the renderer reads, so `oklch()` theme colours through `var()` now
  render natively (the recorded parity gap). Values valid in CSS that native
  layout cannot render are refused as `Unrenderable` with their own
  diagnostic.
- **Browser host.** Unchanged and still rejecting exactly what it rejected:
  its registry (`FORBIDDEN_CSS`, `MEDIA_FEATURES`, grammars) is untouched and
  no CSS logic was added in JavaScript. Routing its stylesheet check,
  cascade and value validation through a wasm32 shim of this engine is the
  next step (T3): the shim needs `varta-common`'s view modules on `no_std` +
  `alloc` (or a std wasm32 build) and the JS view model's per-rule work and
  expansion data from Rust. A finding for it: Chrome ignores `@property` in
  shadow trees (Properties and Values API 1 §2.8 says registrations are not
  tree-scoped), so the bridge must register them at document level.
- **Measured acceptance** of the five-component application's 588 class
  uses, end to end through the native engine (each candidate's Tailwind CSS
  flattened as T1's canonical output will be, with the referenced theme
  variables; class token, sheet commit and every computed property): 25.3%
  before (149 uses; 17.3% of 312 distinct candidates), 70.4% after (414
  uses; 64.1% distinct). The design projected 72% for S1–S3; the difference
  is the values S3 refuses because native layout lacks them (`inline-flex`,
  `inline-block`, `sticky`, `fit-content`: 10 uses). The rest is S4–S6
  (logical properties, selectors, shadows).
- **Chrome oracle** (`native_css_engine_probe`, `test-native-css-engine.mjs`;
  ten scenes: escaped classes, layers, registered properties, theme
  colours, colour functions, out-of-gamut colours, math and units, media
  features, reduced motion, comments and escapes): 224 checks pass at 1x,
  1.25x and 2x; box geometry matches within 0.02 px, in-gamut colours within
  1/255 (RGB mean ≤ 0.033, the 1.25x maxima are antialiased edges), and all
  286 Tailwind 4 palette colours match Chrome within 1/255. Recorded
  deviations: Chrome clips out-of-gamut colours (mean 0.58, max 81 for
  far-out colours), ignores `@property` in shadow trees, and snaps lengths to
  1/64 px.
- **Open, for owner decisions:** (1) CSS-wide keywords on standard
  properties need a native cascade that knows initial values and user-agent
  rows; (2) `em` outside `font-size`, `ex`/`ch`/`lh`, and percentage-plus-
  length math need layout-time resolution (taffy's `calc` handles); (3)
  literal `opacity` above 1 and `font-weight` outside [1, 1000] stay refused
  although CSS clamps `<alpha-value>`; (4) `@layer` inside `@media`,
  `!important`, more than two `color-mix()` colours (Chrome refuses those
  too) and named colours stay outside; (5) registered computed values are not
  absolutized before inheritance; (6) the desktop shell passes default
  preferences, not the OS settings (reduced motion, contrast); (7) gamut
  mapping per the specification differs from Chrome's clipping for colours
  outside sRGB.
- Gate: `python3 scripts/verify.py full` PASS (3979 Rust tests passed, 0
  failed, 21 ignored; 218 browser Node tests; conformance baseline), report
  `target/verification/20261106-130906-pnr2i3f3/report.json` (macOS only).

#### K8 T2 standards items and T3 browser bridge (2026-11-06)

- **T3, browser bridge.** `varta-common`'s view modules build `no_std` with
  `alloc` and `libm` behind the `view` feature (the KV codec and intrinsic
  library bytes are unchanged); the cascade, variables and computed values
  moved into `varta-style`, shared by the native renderer and
  `varta-style-wasm`, which `scripts/build-view-engine.py` builds into the
  checked-in `view-engine.wasm` (`view-engine.json` fingerprint), as the KV
  codec is built. The browser view model mirrors each batch into the engine
  in a transaction and takes its verdicts; the DOM bridge applies the
  engine's computed values inline and records media features, viewport and
  interaction states as inputs. `view-css.mjs`, `view-variables.mjs`,
  `view-custom-properties.mjs` and the CSS parts of `view-model.mjs` are
  removed; no CSS decision remains in JavaScript. `@property` reaches Chrome
  no more (the engine substitutes), so its shadow-tree gap does not apply.
  Cross-host equality: the corpus `view-engine-corpus.json` (the Tailwind
  application's 312 candidates plus synthetic and inline cases) is decided
  identically by the Wasm module, the native session and the native
  renderer (Rust test), and by the browser view model (Node test).
- **Standards items (1)–(7) of the S1–S3 record:** (1) CSS-wide keywords
  on every property (Cascade 5 §7.3, `revert` and `revert-layer` included;
  `font-family: initial` stays refused, its initial value being the user
  agent's font); (3) `opacity` takes any number, clamped; literal
  `font-weight` outside [1, 1000] is invalid and math results are clamped
  to the property range; (4) `!important` with reversed layer order,
  `@layer` inside `@media`/`@supports` with conditional order, `@supports`,
  the 148 named colours and `color-mix()` of one to `MAX_MIX_COLORS` colours
  (Color 5 §3; Chrome mixes exactly two, a recorded deviation); (6) the desktop shell reads macOS
  "Reduce motion" and "Increase contrast" as recorded inputs, again on focus
  and theme changes; (7) gamut mapping stays per Color 4 §14.2, Chrome's
  clipping recorded as its deviation. Not done: (2) `em` outside
  `font-size`, `ex`/`ch`/`lh` and percentage-plus-length math (they need the
  element's computed font in the engine and `calc()` lengths in native
  layout; no use of the measured application needs them) and (5) the
  absolutization of registered values before inheritance (same
  prerequisite: the engine computing `font-size`).
- **Chrome oracle,** 15 scenes (new: CSS-wide keywords, importance,
  conditional rules, named colours and two-colour mixes, multi-colour
  mixes), now also requiring every box's centre pixel to match within
  1/255 besides the scene mean: `bridge` 451 checks and `platform` 439
  checks pass at 1x, 1.25x and 2x. The bridge matches the native pixels
  exactly at 1x and, apart from 1/64 px length snapping in one scene, at
  2x, including out-of-gamut colours and multi-colour mixes; the 1.25x
  differences are antialiased fractional edges (scene means at most
  0.018). Platform-mode deviations: gamut clipping (mean 0.58) and Chrome
  dropping `color-mix()` with other than two colours.
- **Measured acceptance** of the five-component application: unchanged at
  70.4% of 588 uses (64.1% of 312 distinct candidates). The new admissions
  are not what the application is missing; its refusals are selectors (S5,
  59 distinct), logical properties (S4), shadows (S6) and the values native
  layout lacks (`inline-flex`, `inline-block`, `sticky`, `fit-content`,
  `-webkit-box`).

#### K8 T2 items (2) and (5): font-relative units and layout-time math (2026-11-06)

Owner decision of the same day: (2) and (5) are required by the standards.

- The style engine computes font sizes and line heights (`varta_style::font`:
  the user-agent font rows, Blink's monospace and minimum-size rules, moved
  from the native renderer), so `em`, `lh` and `rlh` resolve in every
  property on both hosts, and registered length values compute at the
  declaring element before they inherit (Properties and Values API 1 §2.4).
  The browser host passes its `medium` size (the `all: initial` shadow host)
  and whether a packaged face names `monospace`; the engine receives each
  element's tag.
- Sums of lengths and percentages resolve at layout (Values 4 §10.10) for
  sizes, margins, padding, gaps, the flex basis, insets and
  `vertical-align`, through a recorded Taffy 0.15.1 patch that lets the
  high-level tree resolve calc handles (an index, never a pointer).
- Chrome oracle: two new scenes (font-relative units with a registered
  length, percentage-and-length math), `bridge` 503 and `platform` 491
  checks pass at 1x, 1.25x and 2x with no new deviation.
- **Dependency record: Taffy 0.15.1 vendored with a patch** (the dependency
  rule of CLAUDE.md §5, as for the Parley patch). Why: upstream's `calc`
  feature stores a `calc()` length as an opaque handle and asks the layout
  tree to resolve it, but the high-level `TaffyTree` that native layout uses
  hard-codes the answer zero, so no Taffy user above the low-level traits
  can resolve `calc()`. Alternatives rejected: reimplementing Taffy's tree
  over its low-level layout traits (a second copy of `TaffyTree` and its
  cache, view and rounding code), and refusing sums of lengths and
  percentages (they are required by CSS Values 4 §10.10 and the owner
  decision). Change: one upstream file, `src/tree/taffy_tree.rs`, gains a
  `calc_resolver` field and `TaffyTree::set_calc_resolver`; without a
  resolver the tree answers zero as upstream. No dependency version changes;
  the lockfile changes only Taffy's source selection (registry to
  `vendor/taffy`), and the `calc` feature is enabled for `varta-view`.
  Provenance: the checksum-pinned crates.io archive (SHA-256 in
  `vendor/taffy/VARTA-PATCH.md`); a recursive comparison with the unpacked
  release finds only the patched file, the two patch records and the
  dropped `Cargo.lock`/`.cargo_vcs_info.json`. Safety: the handle Varta
  passes is a table index made with `ptr::without_provenance`, never a
  pointer, and neither Taffy nor Varta dereferences it, so no `unsafe` is
  added to workspace code. Removal criterion: upstream exposing a resolver
  for `TaffyTree` (or Varta moving to the low-level API), after which the
  vendored copy is dropped and the lockfile returns to the registry source.
- Remaining for (2): `ex`, `ch`, `cap`, `ic` (and root forms), `lh` with
  `line-height: normal` and `rlh` need the metrics of the first available
  font. Owner decision (2026-11-06), "fonts plus host": the shared engine
  reads the first available font's metrics from packaged font bytes with
  read-fonts (one implementation for both hosts); for system and generic
  families the host supplies them (natively through CoreText, as the emoji
  platform metrics are read; on the web the bridge only measures through
  browser APIs and passes the numbers into Wasm, with no metric logic in
  JavaScript); the Values 4 fallbacks apply only when no metrics are
  available at all; the goal is Chrome's values with no recorded deviation
  for system fonts. Sums with percentages in `border-radius`, `transform`,
  `object-position` and grid tracks wait for those consumers.

### K7 — Modern HTTP server (agreed design, 2026-11-05)

Goal: the native HTTP server runs at current standards, secure and fast enough
for an internet-facing service such as the UARP API itself, while each request
keeps its own isolated, bounded instance. It builds on the v2 streaming
responses and bounded pool ([HTTP streaming](#status)). Owner decisions:

- **TLS** in the server with rustls, **TLS 1.3 only** (Mozilla "modern"). The
  certificate and key come from the host (its roots and policy), never from
  the package, and guest code never sees them.
- **HTTP/1.1 with keep-alive and HTTP/2** (ALPN over TLS). HTTP/3 over QUIC is
  a later, separate step.
- **Async Wasmtime**: guest code runs as asynchronous Wasmtime calls with
  fuel/epoch yielding, asynchronous host waits and a pooling allocator with
  pre-instantiated modules, so thousands of connections and event streams
  share a few OS threads.
- **Limits**: the registry fixes safe maxima; a host policy may only lower
  them; an application can change none.

Stages, each with its own full gate and record:

1. **H1 — Transport and Web requests.** HTTP/1.1 keep-alive (done
   2026-11-05: idle 5 s, 1000 requests and 600 s per connection, 1024 open
   connections, 8 workers with a 10 s wait and `Retry-After`); connections
   decoupled from workers (many cheap idle connections, bounded workers, with
   per-connection idle, header and request deadlines against slow clients); a
   handler may take a Web `Request` (method, URL, `Headers`, a streaming body)
   as well as `VartaHttpRequest`; request bodies stream with a registry bound;
   host policy may lower the server limits. Only the keep-alive/connection-pool
   part is implemented; Web Request ingress, streaming request bodies and the
   remaining policy exposure are still open.
2. **H2 — Async execution.** `async_support`, `call_async`, fuel and epoch
   yielding, the scheduler's waits and the network completions as futures,
   pooling allocator and `InstancePre`; workers become tasks, admitted while
   the sum of their memory budgets stays within the host's memory ceiling.
   Determinism, budgets and cancellation keep their current contracts.
3. **H3 — TLS 1.3 and HTTP/2.** rustls server configuration from host-supplied
   files, ALPN `h2`/`http/1.1`, HTTP/2 stream and header-list bounds,
   rapid-reset and flow-control protections; `net.listen` grants name whether
   an endpoint serves TLS.
4. **H4 — Hardening and performance acceptance.** Fuzzing of the envelopes
   and adapters, flood and slow-client tests, and recorded load benchmarks
   (throughput, p50/p99 latency, memory per connection and per stream) as
   acceptance targets.

Later: HTTP/3, response compression (a dependency decision), WebSocket.

#### H1 — Web requests, request bodies, host limits, slow clients (done 2026-11-05)

- **Web `Request` handlers.** A v38 handler may take a Web `Request` with any
  of the four result forms; the contract's `webRequest` flag (v2, with
  `asynchronous`) selects the adapter, whose envelope (`WebRequestHead`)
  carries the method, the absolute URL, the headers and the id of the body's
  transfer. The fetch prelude's `httpRequest` builds the `Request`; `Request`
  gained the body readers of `Response`, a `Request` made of a received one
  takes its stream over and `fetch` of it sends the body as bytes.
- **Streaming request bodies.** The body is a transfer of the request's own
  Store (`Net::inbound`), read like a response body: one bounded chunk per
  guest read, `http::MAX_REQUEST_BODY_BYTES` in all (413 before dispatch
  when the declared length is larger), `http::REQUEST_BODY_IDLE_SECONDS` per
  wait. Such a request is a sequence of renewing turns within
  `http::REQUEST_LIFETIME_SECONDS`; its waits use no turn deadline
  (`RuntimeContext::idle_bound`). `VartaHttpRequest` handlers are unchanged.
- **Host limits.** A host policy's `http.limits` lowers the v2 server limits
  (`HttpLimits::lowered`); a raised, zero or unknown limit, or any limit of a
  v1 service, fails before bind. Packages cannot carry the member.
- **Slow clients.** Keep-alive idle stays the head deadline; request bodies
  have their idle bound; a response write that makes no progress for
  `http::WRITE_IDLE_SECONDS` closes the connection (`server::io::WriteIdle`).
- Tests: runtime `server_tests::web_request` (URL/headers/body, a body
  slower than three turn deadlines, size and idle bounds, a client that stops
  reading, lowered and raised limits, every result form), the HTTP ABI flag
  and v37 rejection, contract/policy unit tests and the CLI smoke
  `http_cli_host_policy_lowers_server_limits_and_cannot_raise_them`.
- Gate: `python3 scripts/verify.py full` PASS (3872 Rust and 218 browser
  Node tests, conformance baseline), report
  `target/verification/20261105-233337-p4fj2lfr/report.json` (macOS only).

#### H2 — Async execution (done 2026-11-06)

- **Tasks, not threads.** The native server is a multi-threaded Tokio
  runtime with one worker thread per available CPU; every v2 request is a
  task. Its engine has the runtime's compilation settings (shared
  `engine_config`, so fuel is identical), a pooling allocator with
  `http::MAX_CONCURRENT_REQUESTS` slots of `http::REQUEST_MEMORY_BYTES`, and
  the module linked once into an `InstancePre`; each request still
  instantiates its own Store.
- **Yielding.** Guest calls are `call_async`; a guest yields at every epoch
  tick (one ticker thread per server, `server::engine::TICK`, which also
  meets per-turn deadlines without a deadline thread per Store). The
  scheduler's timer waits, network and body completions and cancellation are
  awaited (`checkpoint_until_async`, `Net::wait_async`,
  `Cancellation::cancelled`); a request's transfers run on the server
  runtime (`Net::share_runtime`). Blocking host operations (legacy
  `net.get`/`net.fetch`, KV) go through `crate::blocking` (`block_in_place`).
- **Fuel stays exact.** Wasmtime runs the call hook around libcalls too, so
  the v38 fair call charge counted every epoch check: with a ticker the
  fuel of identical requests differed by up to 15%, and even before, any
  epoch another Store's deadline or cancellation advanced on a shared engine
  could add 50 fuel. The epoch callback now refunds that charge; fuel that
  cannot cover a charge fails a counted host call before its effect and any
  other call when it returns. The server does not yield on fuel intervals,
  whose libcalls would count; the deterministic epoch yield suffices.
- **Admission by memory.** A v2 request runs once a slot is free and its
  memory budget fits `http::SERVER_MEMORY_BYTES` less the running requests'
  budgets, within the worker wait (else 503 with `Retry-After`); a host may
  lower slots and ceiling (`workers`, `memoryBytes`), and a ceiling below
  one request fails before bind.
- **Contract v1** keeps one blocking worker on the same engine and linked
  module: its checked JSON parse calls back into the guest from a host
  import, which an asynchronous call cannot.
- Values set in the registry (open to an owner review):
  `MAX_CONCURRENT_REQUESTS` 8 → 1024 slots and `SERVER_MEMORY_BYTES` 1 GiB
  (128 requests of the full 8 MiB budget, more with smaller package
  budgets). Lock: the `async` feature names an optional component-macro
  dependency weakly, so `Cargo.lock` gains six lock-only entries that are
  never built.
- Tests: `server_tests::tasks` (a CPU-bound guest yields so a quick request
  is answered while twice the worker threads spin, with identical fuel per
  spin; 64 streams open at once; memory admission and a too-low ceiling);
  the busy-503 test lowers the slots by host policy instead of opening 8
  streams. Every other server test is unchanged.
- Gate: `python3 scripts/verify.py full` PASS (3875 Rust and 218 browser
  Node tests, conformance baseline), report
  `target/verification/20261106-001950-ffibq692/report.json` (macOS only).

#### H3 — TLS 1.3 and HTTP/2 (done 2026-11-06)

- **TLS from the host.** A host policy's `http.tls` names absolute PEM
  paths of the certificate chain and key; the host reads them (bounded)
  before bind and serves TLS 1.3 only (rustls, ring, no 0-RTT) with ALPN
  `h2` then `http/1.1`. Bad files, relative paths and TLS for a v1 service
  fail before bind; nothing reaches a package or guest.
- **Grant form.** `net.listen` patterns name TLS endpoints as
  `https://IP:port` (canonical); a host that serves TLS needs that form in
  the package and the ceiling, a cleartext host the bare form, and
  `http_stop` checks the same. Denial tests: runtime
  `server_tests::tls::listen_grants_name_whether_an_endpoint_serves_tls`
  and CLI smoke
  `http_cli_tls_listeners_need_the_tls_grant_form_in_package_and_host`
  (positive path: the CLI announces `varta listening: https://…`).
- **HTTP/2.** hyper's `http2` feature: at most
  `http::MAX_CONCURRENT_STREAMS` streams, a `http::MAX_HEADER_LIST_BYTES`
  header list, bounded windows and send buffer, keep-alive pings, the
  rapid-reset bounds of `h2`, and the server's own: a connection whose
  clients abandon more than `http::MAX_ABANDONED_REQUESTS` requests is
  retired with `GOAWAY` and closed within a second. Keep-alive request
  count, idle bound and lifetime retire HTTP/2 connections with `GOAWAY`;
  the write idle bound covers TLS. Connection serving moved to
  `server::connection`, TLS configuration to `server::tls`.
- Dependencies: hyper's `http2` feature (`h2` 0.4.20 and `tokio-util`
  0.7.20 join the lock); rustls server parts and PEM parsing come with the
  locked rustls and pki-types; test-only: `rcgen` in the CLI smoke and
  tokio's `io-util` in the runtime tests.
- Tests: `server_tests::tls` (HTTP/2 and HTTP/1.1 by ALPN over TLS 1.3 with
  a Web `Request` body and `https` URL, TLS 1.2 and cleartext refused,
  HTTP/2 SETTINGS bounds, a 1000-stream rapid-reset flood ended with
  `GOAWAY` while other clients are served, grant forms and file failures),
  listen-grant validation unit tests.
- Gate: `python3 scripts/verify.py full` PASS (3879 Rust and 218 browser
  Node tests, conformance baseline), report
  `target/verification/20261106-004103-yowuttms/report.json` (macOS only).

#### H4 — Hardening and performance acceptance (done 2026-11-06)

- **Fuzzing (seeded, reproducible).** Mutated contract metadata never
  panics, and what parses is valid, keeps its version's registry limits and
  round-trips; a mutated host policy lowers within the registry or is
  refused (`varta-common` http tests). Random response envelopes are framed
  with a valid status and safe headers or refused (transport tests). Random
  requests (methods, percent-escaped targets, headers, Unicode bodies)
  reach the v1, v2 sync, v2 async and Web `Request` adapters exactly as
  sent, and mutated wire requests are refused before dispatch, never with a
  500 (`server_tests::fuzz`).
- **Floods and slow clients** (`server_tests::flood`, with the TLS
  rapid-reset flood of H3 and the write-idle test of H1): 64 slowloris
  clients are cut at the lowered idle bound while the service answers; a
  connection flood waits in the backlog; a request flood beyond the slots
  gets 503 with `Retry-After`; 32 stalled request bodies end at the body
  idle bound; every slot is free afterwards.
- **Load acceptance numbers** (`server_tests::bench`, ignored; run with
  `cargo test -p varta-runtime --release --lib -- --ignored --nocapture
  --test-threads=1 server_tests::bench`): Apple M1 Max, 10 cores, macOS 27,
  release build, clients in the same process over loopback, the machine
  shared with other builds (load average about 11); three runs:

  | Measure | Result |
  | --- | --- |
  | Web `Request` handler, 32 keep-alive HTTP/1.1 clients, 16,000 requests | 43,500–45,500 requests/s |
  | Latency p50 / p99 / max | 0.67–0.71 ms / 1.41–1.46 ms / 2.6–3.7 ms |
  | Resident memory per idle keep-alive connection (1000 open, client buffers included) | about 36 KB |
  | Resident memory per open event stream (100 open, each its own instance holding a timer) | 174–187 KB |

  Each request instantiates its own Store from the pooled `InstancePre`.
  These are acceptance baselines for this machine, not cross-platform
  claims; Linux, HTTP/2 over TLS and sustained-load runs remain to record.
- Gate: `python3 scripts/verify.py full` PASS (3888 Rust and 218 browser
  Node tests, conformance baseline), report
  `target/verification/20261106-010141-7bk6f7yq/report.json` (macOS only).

### Realistic turn budget and application memory (owner decisions, 2026-11-06)

The UARP console against the live API was terminated first by a network turn
that exhausted 8 million instructions (checked parsing of a 276 KB session
transcript) and, after that limit was raised, by `memory budget exhausted`
with a long history in its 8 MiB instance. The owner asked for budgets at
the level of current platforms rather than per-application workarounds and
decided:

- **Per-turn fuel**: the v38 default `budget::TURN_FUEL` is the engine
  maximum `budget::MAX_FUEL_PER_TURN` (about a second of guest CPU);
  maintenance keeps its own smaller `budget::MAINTENANCE_FUEL`. The
  deadline per turn still bounds wall time; a manifest can only lower, a host
  ceiling only raise.
- **Application memory**: a v38 module declares `DataLimits::memory_bytes`
  (`limits::V38_MAX_MEMORY_BYTES`) as its memory maximum and the default
  `memoryBytes` budget is the same; earlier profiles keep 8 MiB byte for
  byte. Each HTTP request instance is capped at `http::REQUEST_MEMORY_BYTES`
  whatever its package or ceiling asks, so concurrent workers stay a
  predictable sum, and its resource traps now report the typed resource
  failure like an application's.
- **Rule**: production budgets change only by an explicit, recorded owner
  decision, never to admit one feature or a test (CLAUDE.md §11). Tests that
  relied on the earlier values declare them as test budgets.

**Promise serials** (owner decision, same day): the console in the browser
then ended with `execution exceeds 1048576 promise serials` after 413,060
turns. V38 reaction serials, rejection-tracking ids and timer ids now count to
`limits::V38_MAX_PROMISE_SERIALS` over the instance's lifetime and are never
reused; only live state is bounded (held tokens, pending timers,
`limits::V38_MAX_OUTSTANDING_REJECTIONS` outstanding reports). The native
scheduler and the browser host (`schedulerLimits` per profile) change
together; v34–v37 keep their per-execution budget. A v38 test issues more
serials than the earlier bound and fails with the console's diagnostic on the
earlier scheduler.

**Network inactivity window** (owner decision, same day): the turns came
from a reconnect loop. A v38 network read waited for bytes at most one
per-turn deadline (5 s), so the console proxy's upstream session stream and
the console's own read of the proxy broke whenever the live session was quiet
for five seconds (`TypeError: terminated` in the proxy log); the follower
reconnected and replayed frames, for hours. V38 transfers now have their own
window, `net::READ_INACTIVITY_SECONDS` (Node's default `bodyTimeout`),
separate from the turn deadline, still bounded by the instance lifetime;
a host may lower it (`RunOptions::network_idle`, which the transfer tests
use) and the browser host reads `network.readInactivityMs`. The HTTP v2
stream idle bound equals it, so a proxy never cuts what its upstream allows.
A v38 test keeps a stream silent for three turn deadlines and completes.

**Idle collection on large heaps** (same day): with the reconnect loop gone
the console loaded its history in about 2,200 turns, then ended with
`turn 2241 (maintenance) exhausted its maintenance fuel budget`. A new cycle
zeroed its two mark bitmaps (heap/64 bytes) in one `memory.fill`, and bulk
memory work is charged per byte, so on a heap near 200 MB one idle step cost
more than the whole maintenance budget; 8 MiB instances never saw it. The
collector now clears the bitmaps incrementally
(`collector::CLEAR_BYTES_SHIFT` bytes per work unit) before its root
snapshot, with a headroom so blocks allocated meanwhile are ordinary blocks
below the limit. A test with a heap past 100 MB and a maintenance budget of
one and a half step reserves fails with the console's diagnostic on the
earlier collector and passes now; the collector stress corpora pass.

**Map and Set hashing** (same batch): a console handler that filled a
`Map<string, string>` with 300,000 `` `k${i}` `` → `` `v${i}` `` entries
exhausted its 1,000,000,000-fuel turn, and so did `Set.add`. The v37-layout
prelude was already a chained hash, so the cost was a large constant rather
than a lookup growing with the size: a profile of 20,000 insertions put
about 1,100 fuel per key into hashing the key in guest TypeScript (the
32-bit FNV-1a arithmetic over binary64 Numbers), and most of the rest into
five parallel vectors per entry (chains, hashes, serials, keys, values),
each written through the generic element path. BigInt keys shared one
chain (linear), and deleted entries were compacted only at the v37
ceiling, so under v38 they accumulated without bound. The
collection-hashing stream ([contract](architecture.md#collection-hashing-collection_hashing))
gives v38 the dictionary table encoding with allocation-free guest probes
selected by the key type (strings and BigInts share the dictionary probes),
one `insert` probe that finds a key or places the new position, three
vectors per map entry (two per set entry), rebuilds at half load and after
deletions outnumber the live entries, and a `first` bound on the deleted
prefix for queue-like use. Behaviour is pinned against Node 24; v37 keeps
its prelude and bytes.

Fuel of `n` entries (`varta-runtime` `collection_fuel_measurements`,
`--ignored --nocapture`; v38 turn budgets with test-only fuel of 20 turns
and the production v38 memory; deterministic, debug test build), before
(the v37-layout prelude lifted to the v38 ceiling, as integrated) and after:

| Workload | 1,000 | 10,000 | 100,000 | 300,000 | Per entry at 300,000 |
| --- | --- | --- | --- | --- | --- |
| `Map<string, string>` `set` of `` `k${i}` `` → `` `v${i}` `` | 4.6M → 1.7M | 58.4M → 24.1M | 585.0M → 234.6M | 1,922.0M → 873.7M | 6,407 → 2,912 |
| `Set<string>` `add` | 4.0M → 1.3M | 50.5M → 15.6M | 503.1M → 159.9M | 1,670.5M → 606.4M | 5,568 → 2,021 |
| `Map<number, number>` `set`, `get`, every other `delete` | 4.4M → 2.1M | 49.0M → 20.5M | 470.1M → 236.9M | 1,518.0M → 640.1M | 5,060 → 2,134 |
| `Set<bigint>` `add` (with `b += 1n`) | 3,741.0M → 5.8M | over 20,000M → 60.8M | over 20,000M → 605.3M | over 20,000M → 1,938.6M | — → 6,462 |
| `Map<object, number>` `set` | 3.8M → 1.1M | 50.3M → 14.8M | 433.5M → 171.2M | 1,429.4M → 562.5M | 4,765 → 1,875 |
| `Set<number>` queue (take the first, delete it, add one) | 34.8M → 3.5M | 3,053.3M → 37.5M | over 20,000M → 381.5M | over 20,000M → 1,146.6M | — → 3,822 |
| Two string arrays (comparison, unchanged) | 0.9M | 12.2M | 169.1M | 498.8M | 1,663 |
| Dictionary of the same entries (comparison, unchanged) | 2.4M | 31.6M | 318.8M | 996.6M | 3,322 |

- **The reported turn.** A `varta.events.listen` handler that fills a
  300,000-entry `Map<string, string>` now takes 873.7M fuel and a
  `Set<string>` 552.3M, each within the default 1,000,000,000-fuel turn
  (`three_hundred_thousand_entries_fit_one_turn_under_default_budgets`);
  before, they needed 1,922M and 1,671M.
- **Quadratic cases gone.** BigInt keys (one shared chain, a host comparison
  per entry) and a queue (deleted entries never compacted below the v38
  ceiling, so every first-key search crossed all of them) were quadratic;
  both are now linear in the entry count.
- **What remains.** About 1,250 fuel per map entry above the two arrays at
  300,000 entries: three vector appends, one probe with the key's FNV-1a
  hash, amortized rebuilds that rehash every key, and the roots of the
  prelude calls. The cost per entry of every workload, the arrays included,
  still grows with the live heap (map entries 1,700 to 2,900 fuel, array
  pairs 900 to 1,660 between 1,000 and 300,000 entries); the fuel profile
  puts that growth in the collector's marking work over the larger heap.
  Open: the collector's pacing on heaps of hundreds of megabytes; the
  per-entry constant (the three generic vector appends are about a quarter
  of it, by the profile, and could be done inside the `insert` probe).

Application side, same batch: the console reads a session transcript in pages
of 10 through the API's `limit`/`cursor` pagination (canonical `items`), and
the server no longer reports an idle keep-alive connection closing after it
answered requests. SIMD (`simd128`) string scanning in the compiled JSON
decoders, which then spent about 21 fuel per input byte on a transcript:
implemented, see the [SIMD scanning measurement](#simd-json-scanning-measurement-v38).

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
- Preserve the implemented async/event cancellation, task/handle lifetimes, retained
  roots and queue/turn budgets as new long-running operations are admitted; expand
  sustained lifetime and cross-host validation.
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

- Outbound HTTPS and async execution are implemented in their checked profiles.
  Production inbound HTTPS deployment and async server integration remain later
  work; the initial synchronous HTTP handler profile does not supply them.
- Authenticated AOT: define who trusts the signer, which bytes are authenticated,
  and how engine version, target and CPU requirements are pinned. Never restore
  arbitrary native deserialization behind a filename or format marker.
- Package compatibility: only after module/type/memory semantics are stable;
  evaluate packages against a documented subset rather than promising Node parity.
- Production: fuzz schemas/containers/Wasm boundaries, expand platform coverage,
  isolate compilation and blocking host operations where hard OS-level resource
  isolation is required, and obtain an independent security review.

### Browser timer increment — focused verification 2026-11-02

The generated Worker now supports native-compatible timer queues, cancellation,
intervals, unreferenced timers, V2 recoverable denial and v38 turn budget renewal.
A separate runtime boolean permission ceiling intersects exact package requests;
empty/false/unrelated grants deny. Optional permissions without a decision deny
as in native headless operation. No host policy is embedded by build. Stored
permission decisions, prompts, network/view adapters, browser dev and other
browser engines remain open. The historical full gate does not cover this work.

Focused verification passes: 13 Node scheduler/reader/timer tests, five generated
registry/layout tests, 18 native timer tests, 11 browser project tests and seven
CLI distribution tests. The expanded real-Chrome probe matches v36/v37/v38
native timer observations, long callback errors and runtime permission denial;
local evidence: `/tmp/varta-browser-timer-chrome-v3.log`, generated fixtures:
`target/browser-timers-probe-20261102-v3`. Common/project/CLI all-target Clippy is
checked at this boundary. Rebuilding linked intrinsics after the std-only common
registry addition leaves Wasm bytes identical; only its source fingerprint changes.

### Browser network increment — focused verification 2026-11-02

The generated v38 Worker now integrates the bounded async network broker and
Fetch transport with the Promise/timer loop. Exact package/runtime URL and method
scopes precede deferred I/O; response-header visibility is intersected, empty
ceilings deny, and optional requests deny headlessly. Network completion turns
renew v38 fuel/call/job budgets, result buffers pay byte fuel, and cancellation
replaces pending results and drops late completions. Deferred work/events are
bounded by request slots. Original portable Wasm remains unchanged.

Real Chrome cross-origin loopback acceptance matches native logs and exact usage
for GET/POST, byte bodies, custom-header preflight, absent grants, pre-abort and
active timer-driven cancellation. Runtime ceilings cover empty, method-mismatched,
valid and malformed scopes. Browser cookies are omitted; a denied-target counter
stays zero for refused requests and opaque redirects. Local evidence:
`/tmp/varta-browser-network-chrome-v5.log`; fixtures:
`target/browser-network-probe-20261102-v5`. Node scheduler/reader/timer/network/
transport tests pass 26 cases; common registry tests pass five, project browser
checks pass 13 and CLI browser distribution checks pass eight. Affected
common/project/CLI all-target Clippy passes. These are focused results outside the
historical native-server full gate.

Transport boundaries remain explicit: CORS/exposed headers, manual opaque
redirect failures, and rejection of exposed non-identity response encodings.
Hidden wire headers/counts and compressed wire-byte equivalence cannot be proved
through Fetch; v37 arrival-order network heads remain a build gap. A trusted
transport bridge needs a separate design if that full wire equivalence is
required. Browser view delivery, stored permission decisions/prompts, browser dev,
Firefox/WebKit, native GUI targets and release verification remain open.

CSP generation also rejects URL-valid origins that are not representable host
source tokens (quotes, directive punctuation, underscore and IPv6 literals)
before output. Dedicated regressions exercise the actual package/bundle boundary;
no raw origin may terminate the generated HTML attribute or add a CSP directive.

### Browser view foundation — focused verification 2026-11-02

Added an engine-owned structured view registry, strict batch codec, transactional
model and private DOM adapter. Native and generated hosts now share tag-content
rules and CSS composite grammars. The model covers every opcode, final id
uniqueness, fields/radio state, listeners/filters, command arguments and atomic
rollback. The DOM adapter preserves node identities, respects local edit counters,
uses a closed shadow root/CSSOM, encodes events and rejects ambient asset URLs.
This replaces reliance on the prototype's weaker validation for the official path;
the prototype is not an engine dependency and remains independently unchanged.

The public native oracle emits 36 batch sequences, 2294 CSS value observations and
all 11 event kinds. Browser model acceptance, structural state/rollback, property
validation and event bytes agree for that corpus. These are finite observations,
not a claim of complete layout or all-input equivalence. The Node browser suite
passes 34 tests with the oracle enabled. Real Chrome verifies atomic rejection,
UTF-16, CSSOM under CSP, event encoding, stale/fresh input updates, stable DOM
identities and rejection of unpackaged resources; screenshot inspected locally:
`target/browser-view-20261102-v2.png`. Evidence:
`/tmp/varta-browser-view-node-final-v3.log`, `/tmp/varta-browser-view-chrome-v2.log`.
Native view tests pass 18 cases. Common registry drift checks and affected Clippy
are checked at this boundary; the historical full gate still does not cover the
new browser code.

**At the foundation checkpoint, before session integration below:** connect the shared model
to `view_commit` in the Worker, preserve one commit per turn, bind validated bytes
and events over bounded transport, support long-lived session/turn deadlines and
cancellation, verify trusted gestures and clipboard/link decisions, complete
resource packaging, and extend control/accessibility/layout acceptance. Keep
`view_commit` unsupported until the connection is verified (now covered by the
session increment below); resource packaging and broader acceptance remain open. Static/private DOM success does not close P9 or native GUI work.

### Browser view sessions — focused verification 2026-11-02

The generated bundle now admits v38 `view_commit`. Worker validation precedes the
next guest effect, one turn commits once, and one bounded batch awaits parent
acknowledgement before the next task. A parent queue bounds/coalesces pending view
records with one event in transport. Event bytes reach only the synthesized closed
export; no DOM callback reenters Wasm. Native due/arrival ordering and v38 turn
renewal extend across UI, Promise jobs, timers and network work.

Public `mount` supplies `ready`/`finished`/`close`; idle waits have no active-turn
watchdog, new tasks renew it, transfers retain their operation deadline, and close
cancels the Worker/I/O and freezes the checked UI. Container ownership is released
for a fresh remount. Bounded `run` still renders without event delivery. Clipboard
and links intersect exact package/runtime ceilings and trusted gestures; prompts
without a seam deny. Anchors remain inert, speculative external targets never
enter hrefs, and frozen views retain navigation guards. API imports are passive;
only the generated matching bootstrap marker starts a package automatically. Parent
event dispatch waits until the DOM callback completes, including brokered defaults,
before sending a queued record to the Worker.

Chrome acceptance matches native logs and exact usage for initial rendering,
click/input state, Promise/timer updates and denied clipboard/link results.
Additional checks cover valid platform calls through test-only spies (no OS
clipboard write or external page), invalid/second commits before later logging,
close/remount, idle versus callback deadlines, passive imports and frozen-view
navigation prevention. Native exact-URL tests also pin empty query/fragment
delimiters; browser policy suffix matching now preserves them.

Evidence: `/tmp/varta-browser-view-session-chrome-final-v2.log`, fixtures
`target/browser-view-session-20261102-v7`. The shared console/timer/network/integrity
Chrome regression passes separately (`/tmp/varta-browser-view-session-regression.log`),
as does the private DOM probe (screenshot inspected). Oracle-enabled Node tests pass 43;
a subsequent deterministic transport test also covers deadlines before headers and
during body reads, plus timer cleanup on completion/cancel. Common registry
checks five, project browser checks 13, CLI distribution checks nine, and the new
native scope regression one. Affected all-target Clippy passes. These are scoped
results; no new full-workspace, Firefox/WebKit, Linux/Windows/Pi or release pass is
claimed. At that checkpoint, packaged image/fonts remained open; PNG delivery is
implemented in the following increment. Remaining: fonts, stored permission decisions/prompt/audit
integration, broader control/accessibility/layout acceptance, browser dev and
cross-browser/native targets. The complete goal remains open.


### Browser packaged PNG inventory — focused verification 2026-11-02

V38 browser builds accept an explicit `browser.resources` mapping from logical
view names to project-relative files. The shared project loader now exposes bounded
binary reads through the same pinned no-follow descriptors as module/text inputs.
Names follow the common view grammar. Package count, individual/aggregate bytes,
name lengths, PNG dimensions and chunks are bounded; animated PNG and duplicate
headers fail admission. All validation precedes output-directory creation.

`BrowserResource::png` and `bundle_with_resources` also expose this path to trusted
embedders. The bundle retains the original Wasm/manifest bytes, emits generated
flat resource file names and pins each resource's length/SHA-256 in the loader.
Every resource is verified and completely decoded before a renderer or Worker is
created. The Worker permits only package inventory names, while the DOM receives
owned Blob URLs under Blob-only image CSP. Temporary bitmaps close immediately;
resource URLs revoke on every terminal path. Logical `src` names never reach Fetch.

Chrome evidence: `/tmp/varta-browser-image-chrome-v2.log`, fixtures
`target/browser-image-resources-20261102-v2`. It checks real decoded PNG dimensions,
native logs and exact accounting, resource/Worker URL cleanup, digest/length
substitutions, invalid compressed contents before the first guest log, and an
unpackaged resource failing before later guest effects. Session regression passes
(`/tmp/varta-browser-image-view-regression.log`); the shared console/timer/network/
CORS/permission/integrity regression passes separately
(`/tmp/varta-browser-image-network-regression.log`). Project tests pass 28, CLI
browser distribution tests 11, and existing text-resource regressions three. The
follow-up limit check passes (`/tmp/varta-browser-image-resource-limits.log`), as
does the legacy-profile CLI regression (`/tmp/varta-browser-image-cli-v2.log`). Affected
all-target Clippy passes (`/tmp/varta-browser-image-clippy.log`). These remain scoped
results; no new full-workspace or cross-platform release gate is claimed.

Next: packaged fonts with per-session family isolation/cleanup, permission
prompt/decision/audit integration and broader controls/layout/accessibility. Other
image formats, browser dev and Firefox/WebKit remain open. Native GUI and platform
acceptance/release work still belong to the full goal.


### Browser packaged font sessions — focused verification 2026-11-02

`browser.fonts` maps logical families to pinned project-relative font files;
`BrowserResource::font` supplies the equivalent checked-byte API. The existing
resource inventory now carries PNG and TrueType/OpenType/WOFF/WOFF2 descriptors,
with shared raw/decoded-size limits. Font containers reject invalid table bounds,
duplicates/overlap, collections and oversized decoded estimates. FontFace fully
sanitizes verified binary bytes before registration, DOM creation or guest code.
PNG dimensions also contribute their RGBA bytes to the decoded inventory bound.

Each session owns private registered families. Checked inline font-family values,
stylesheet/media declarations and quoted/case-folded names resolve to that session.
Font-context copies of custom properties preserve nested variables/fallbacks while
leaving original color/layout values unchanged. Unknown named fonts fall back;
they cannot reach ambient host/session families. Namespace memory does not grow
with changing unknown family or custom-property names across turns. All terminal
paths unregister faces, including a load that completes after abort/deadline.
Retained inert DOM may fall back after fonts are released. Normal-face registration
is implemented; richer face collections and other image formats remain open.

Evidence: `/tmp/varta-browser-fonts-chrome-v3.log`, fixtures
`target/browser-fonts-20261102-v2`. All four font formats decode and render original
known-advance glyphs; native logs/accounting match. Tests compare inline, quoted,
variable and media paths, two sessions with different font bytes under `Brand`,
and an ambient host `Brand` face. They verify isolation, unchanged color variables,
terminal/late-load cleanup, content tampering and malformed fonts before the first
guest log, and font-as-image rejection. Fixture outlines are repository-original,
with a deterministic optional fontTools generator; tests need no font installation.

Oracle-enabled Node tests pass 46, focused project browser tests 17, and CLI browser
checks 12, including font-family validation and no-follow paths. PNG and view
session Chrome regressions also pass (`/tmp/varta-browser-fonts-images-regression.log`
and `/tmp/varta-browser-fonts-view-regression.log`). These are focused results.
The consolidated browser-core gate now passes **2919 tests, 0 failures, 16 ignored**,
all eight commands successful, source unchanged during verification. Evidence:
`target/verification/20261102-031538-rh756ebf/report.json`; source SHA-256
`17809511c875a150a6695cf3c6c1c0765174bae173c931b935308ba2ee632dbe`.
Conformance remains 123/127, four known unsupported, zero regressions and eight
untested feature rows. Immediately afterward only the checkpoint documentation changed; the permission-
broker work below is a later increment outside that full result.
This supersedes the earlier native-server gate for the current Rust implementation;
Chrome acceptance still has the separate evidence above, and other browser/OS
engines remain unverified.
Remaining full-goal work includes permission decisions/prompts/audits, browser dev
and broader controls/layout/accessibility, Firefox/WebKit, native platforms and
release acceptance.


### Browser permission broker — headless answer and audit increment 2026-11-02

Runtime `decisions` use the native DecisionFile schema, are copied/validated before
artifact fetch and selected only by original Wasm identity. Required stored denials
fail before Worker creation; optional grants stay denied without a decision; a
stored allow remains inside package/host resource scope. Query/request use existing
v38 imports/preludes and bounded one-shot sources, with exact fair fuel and no
ordinary host-call counts. Unsupported resource kinds and named event delivery
remain explicit admission gaps. No decisions are embedded by builds.

Parent/Worker broker states include additional exposed clipboard restrictions and
live updates; startup updates coalesce before guest entry and listeners clean up.
Owned bounded audit/decision reports and synchronous observers cover initial,
operation and platform-state decisions. Every turn now has a checked parent
acknowledgement, including R1 without renewing its root deadline, bounding queued
turn/audit traffic. Invalid startup/ack messages stop before later guest effects.
Active transfers are driven before overdue timers. Event/network watchers share
the timer-serial ownership guard. Native source ids now stop at the shared exact-f64
limit rather than rounding/reusing ids.

Focused evidence: oracle-enabled Node tests 59; common browser registry tests five;
project browser tests 17; CLI permission distribution check one; native source-id
exhaustion one; generated embedding limits one. Real Chrome permission/native
accounting/audit/platform probes and the view/font/network regressions pass against
`target/browser-permissions-final-20261102`. Logs:
`/tmp/varta-browser-permissions-final.log`,
`/tmp/varta-browser-permissions-view-final.log`,
`/tmp/varta-browser-permissions-fonts-final.log`,
`/tmp/varta-browser-permissions-network-regression.log`.
The lifecycle probes independently check pre-entry faults, early platform updates,
checkpoint backpressure and transfer progress before overdue intervals. Intrinsic
fingerprint remains current; no intrinsic binary rebuild is needed. The prototype
contract receives the new source-id bound from the published embedding table.
These are scoped results; consolidated Clippy/full-workspace verification remains
at the end of this open permission batch. The prior full gate (2919/0, 16 ignored) covers the preceding
browser-core batch, not these new changes. Next in this open permission batch:
async prompt answers between turns and host-owned prompt UI. Durable per-identity
choices and revocation have since landed in the following increment. The full
browser/native/release goal remains open.

### Browser permissions — durable choices and revocation increment 2026-11-02

The generated loader exports `createDecisionStore`; an explicit runtime
`decisionStore` replaces `decisions`, never the host ceiling. Real IndexedDB
read/write transactions preserve other artifacts/capabilities under concurrent
updates. Immutable snapshots use the native file schema and bounds. Corrupt,
unavailable, aborted or full storage fails closed without evicting choices;
connections close, and explicit whole-store clearing can recover corruption.
Nothing opens storage automatically or embeds decisions at build time.

`await app.revoke(id)` withholds new page/Worker operations and waits for a checked
Worker acknowledgement plus persistence, when configured. Requested required
grants can be revoked; their stored denial prevents the next startup. In-flight
authorized operations continue. Concurrent same-capability controls coalesce,
unknown requests return false, malformed/replayed controls fail, and terminal
cleanup rejects pending controls. Other live sessions do not auto-synchronize.
Worker audit observations preserve the original authorization decision even if
page revocation/platform changes precede receipt; observations never grant page
authority.

Focused evidence: 66 oracle-enabled Node tests and 17 project browser tests pass.
Real Chrome storage/revocation, native headless permissions/accounting, view session
and console/network regressions pass against
`target/browser-permission-storage-final-20261102`. Logs:
`/tmp/varta-browser-permission-storage-node-final.log`,
`/tmp/varta-browser-permission-storage-project-final.log`,
`/tmp/varta-browser-permission-storage-chrome-final.log`,
`/tmp/varta-browser-permission-storage-headless-final.log`,
`/tmp/varta-browser-permission-storage-view-final.log`,
`/tmp/varta-browser-permission-storage-network-final.log`.
Tests cover atomic concurrent updates, capacity/abort/corruption recovery and
connection cleanup; reload persistence, live clipboard denial, coalesced controls,
unknown/replayed/malformed controls, original transfer continuation and fatal
storage/observer failures. Formatting, diff whitespace and documentation links
also pass. These are focused results, part of the open permission batch outside
the preceding 2919-test full gate. The next consent/task work at this checkpoint
has since landed in the following increment. Broader browser/native/release
acceptance remains open.

The layout path now prunes `display:none` subtrees before interpreting their
contents, while still emitting zero rectangles and enforcing tree limits. Both
box-only and text-enabled paths accept a hidden button containing otherwise
unsupported text; revealing it fails closed. The hidden fixture matches Firefox,
WebKit and Chrome (`/tmp/varta-hidden-content-matrix.log`).

The box layout also supports automatic and negative pixel/percentage margins,
explicit `auto` sizing and `none` maximums. Centering, a negative percentage
margin, and constrained auto sizing fixtures match Firefox, WebKit and Chrome
(`/tmp/varta-auto-sizing-margin-matrix.log`); broader margin-collapsing and
intrinsic-sizing acceptance remains open.

The experimental box path now maps static, relative, absolute and fixed
positioning and signed pixel/percentage insets. Geometry extraction tracks the
positioned containing ancestor for out-of-flow boxes and the viewport for fixed
boxes. Nested containing-block, fixed viewport, relative-flow and percentage
offset fixtures match Firefox, WebKit and Chrome
(`/tmp/varta-positioned-layout-final.log`). Transform-created containing blocks,
scrolling and broader positioning acceptance remain open.

The experimental box path now maps CSS grid columns/rows and numeric line/span
placement through Taffy. It bounds explicit tracks, children, numeric values and
aggregate grid work. Fixtures for `1fr 128px`, explicit placement and
`repeat(2, minmax(0, 1fr))` agree with Firefox, WebKit and Chrome
(`/tmp/varta-grid-layout-matrix.log`). This is not full Grid or UARP geometry
acceptance; direct grid text, other controls and broad combinations remain open.

Isolated text can now be measured directly as a grid item. The layout path also
inherits supported `font-size` and `line-height` values: unitless line height
scales with the descendant font size, while percentages resolve before
inheritance. Grid text and font inheritance fixtures match Firefox, WebKit and
Chrome (`/tmp/varta-font-inheritance-matrix.log`). Full inline formatting, font
selection/fallback and native rendering remain open.

The experimental layout no longer rejects paint-only colors, outlines, opacity,
border radius, text decoration, cursor or stacking order, while it still rejects
transforms. An inherited single named `font-family` is accepted when it matches
the explicitly supplied font and rejected otherwise. Paint-only geometry and the
matching-family fixture pass in Firefox, WebKit and Chrome
(`/tmp/varta-paint-font-family-matrix.log`). Generic families, font lists, visual
rendering and full UI acceptance remain open.

The explicit-font path now accepts a CSS family list, skipping unavailable named
families and requiring an explicit host alias for a generic family that precedes
the supplied font. A named fallback fixture matches Firefox, WebKit and Chrome
(`/tmp/varta-font-family-list-matrix.log`); an explicit `system-ui` alias is
covered natively, but parity depends on the host mapping it to the same font on
each target. Multiple fonts, glyph-level fallback and real platform font selection
remain open.

Text layout now inherits `white-space: normal`, `nowrap`, and a subset of
`pre-wrap`. Tests cover collapsed spaces, preserved internal spaces/newlines,
and nowrap's flex intrinsic width against Firefox, WebKit and Chrome
(`/tmp/varta-white-space-preserved-matrix.log`). `pre-wrap` tab stops and
leading/trailing line spaces remain unsupported; general text layout remains open.

Text layout now supports inherited `overflow-wrap: normal/anywhere`, including
its effect on flex minimum-content sizing. The normal, anywhere and nowrap
interaction fixture matches Firefox, WebKit and Chrome
(`/tmp/varta-overflow-wrap-final.log`). Full Unicode line breaking, mixed inline
content and general text layout acceptance remain open.

### Browser consent — owned UI and asynchronous answers 2026-11-02

`mount({onPrompt})` asks required missing choices sequentially before Worker
creation and permits optional `request()` only in a trusted gesture turn. The
callback runs after the completed turn checkpoint. One prompt is allowed; three
dismissals become a stored denial. A checked answer queues a one-shot host task
ordered with view/network/due timer work, so guest resumption starts a new turn.
Waiting spends no fuel or turn deadline, including first-run approval. Headless
and bounded runs keep their existing policies and never prompt.

`createPermissionPrompt()` provides the host modal outside the guest's closed
view: operation, artifact identity and exact scope are host-owned descriptions,
package purpose is quoted plain text, synthetic clicks cannot grant, Escape
dismisses and abort cleans up. Initial Deny focus preserves the heading/scroll.
Generated v38 view pages with requested grants install the modal and default
IndexedDB store; capability-free views avoid storage. API imports remain passive.
User choices and revocations serialize through persistence. Revocation wins over
stale Allow answers, including approval delayed in a storage callback; audit
observation precedes the save, and guest acknowledgement waits for it. New page
effects and view delivery are withheld while consent remains pending.

Focused checks: 71 oracle-enabled Node tests, five common registry tests, 17 project
browser tests and 13 CLI browser checks pass. Native scripted-view oracles cover
required/optional/parallel requests with Allow/Deny/Dismiss; successful sessions
match real Chrome logs, exact fuel/calls/jobs/turns and audit rows. Required refusal
is checked before guest entry. Chrome tests wait beyond a narrowed 700ms deadline
before first-run approval and between turns, verify no guest progress, one open
prompt, bounded dismissals, abort/revocation/storage races, quoted markup, synthetic
input rejection, trusted approval/Escape, generated bootstrap and persisted choices
across reload. The owned modal is visually inspected from
`target/browser-consent-verified-20261102/permission-dialog.png`.

Current fixtures: `target/browser-consent-verified-20261102`. Evidence:
`/tmp/varta-browser-consent-node.log`, `/tmp/varta-browser-consent-common.log`,
`/tmp/varta-browser-consent-project.log`, `/tmp/varta-browser-consent-cli.log`,
`/tmp/varta-browser-consent-chrome-verified.log`,
`/tmp/varta-browser-consent-network-verified.log`,
`/tmp/varta-browser-consent-storage-regression.log`,
`/tmp/varta-browser-consent-headless-regression.log`,
`/tmp/varta-browser-consent-view-regression.log`,
`/tmp/varta-browser-consent-font-regression.log`,
`/tmp/varta-browser-consent-image-regression.log`.
The packaged font/image regressions pass; formatting, diff whitespace,
documentation links and the existing intrinsic fingerprint also pass.
These remain focused checks outside the prior 2919-test full gate; the broader
browser host batch remains open. Named event delivery has since landed below; next: per-activation
any-HTTPS link consent, browser dev/controls and cross-engine acceptance, then
native platform/release acceptance. Implicit first-use prompting, session-only
choices and automatic synchronization of other live sessions remain explicit gaps.

### Browser named host events — generic instances 2026-11-02

V38 event-source open/close imports now have browser adapters. A trusted embedder
can `mount()` a non-view application and call `app.deliver({source,record,gesture,
coalesce})`. Inputs snapshot immediately; UTF-8 record bounds/validity and source
generation are checked before delivery. Bounded checkpoint catalogs avoid per-import
open/close message floods. Reopened names cannot receive stale messages. The Worker
queues one task per record, coalesces only marked pending records preserving first
arrival/gesture, drops coalescible overflow and fails other queue overflow. Closing
the last source can finish a non-view instance; source waiting also keeps unref
timers eligible. Trusted host gestures integrate with consent, while ordinary
records cannot prompt. Queuing never calls guest code directly.

Browser acknowledgements are asynchronous and separately bounded to the generated
task count; their backpressure is documented independently from native-equivalent
Worker queue behavior. Admission promises report queued/coalesced/dropped rather
than callback completion. Missing/stale listeners and invalid records reject
recoverably; listener faults, malformed controls and non-coalescible overflow end
the instance and release pending promises.

Focused evidence: 77 oracle-enabled Node tests, five common registry tests, 17
project browser tests and 13 CLI browser checks pass. Real Chrome/native fixtures
match outputs and exact fuel/calls/jobs/turns for FIFO/coalescing, reopen/discard,
listener faults and host gesture consent. Browser probes also cover ordinary
background consent, pre-ready ownership, UTF-8 preservation, recoverable rejection,
transport saturation and terminal delivery cleanup. Fixtures:
`target/browser-host-events-verified-20261102`. Logs:
`/tmp/varta-browser-host-events-node-final.log`,
`/tmp/varta-browser-host-events-common.log`,
`/tmp/varta-browser-host-events-project.log`,
`/tmp/varta-browser-host-events-cli.log`,
`/tmp/varta-browser-host-events-chrome-final.log`,
`/tmp/varta-browser-host-events-network-verified.log`,
`/tmp/varta-browser-host-events-consent-regression.log`,
`/tmp/varta-browser-host-events-view-regression.log`.
Formatting, diff whitespace, documentation links and the existing intrinsic
fingerprint also pass.
These are focused results outside the prior 2919-test full gate. The broad host
batch's per-activation link consent has since landed below; consolidated
verification follows that boundary. Browser dev/controls, cross-engine/native
platform and release acceptance still belong to the unchanged full goal.

### Browser navigation — per-address consent 2026-11-02

Every trusted any-HTTPS activation now obtains fresh full-URL consent when either
the request or runtime ceiling uses the broad form. Explicit patterns on both
sides retain their direct policy path. A matching pause/resume handshake parks the
Worker after the current turn/checkpoint and before selecting another task; the
prompt never runs alongside guest code or spends a turn budget waiting. Inputs and
new page effects remain withheld during this request. The dialog says “Open once”
and explains that the choice is not saved. The captured original URL is rechecked
against current grant state and opened with `noopener,noreferrer`; no href reread
can replace the approved address.

Each per-address answer is audited and produces a nongesture command-result task.
Allow/Deny/Dismiss never change stored grant decisions or optional-grant dismissal
counts. Revocation defeats stale Allow; overlap/headless/synthetic activation deny,
abort/observer/protocol failures suppress late effects, and platform exceptions
report refusal recoverably. Positive opening tests use spies, never OS windows;
popup-blocker behavior/null handles and other browser engines remain separate
platform acceptance.

Focused evidence: 81 oracle-enabled Node tests pass. Chrome matches 15 native
scripted-view scenarios for outputs, exact fuel/calls/jobs/turns and audit rows
(wide/explicit requests, wide/explicit/empty ceilings, Allow/Deny/Dismiss), each
with three activations. Extra checks cover uncharged human waits, owned full-URL
display, stale approval, abort, observer/platform failure, headless/synthetic/overlap
behavior and checked pause ordering without an extra turn. Fixtures:
`target/browser-navigation-final-20261102`; logs include
`/tmp/varta-browser-navigation-node.log`,
`/tmp/varta-browser-navigation-chrome-final.log`,
`/tmp/varta-browser-navigation-network-final.log`.
The completed consent/event/navigation package now passes the consolidated full
gate: **2921 passed, 0 failed, 16 ignored**, all eight commands successful and
source unchanged during verification. Report:
`target/verification/20261102-072941-39498ymp/report.json`; source SHA-256
`52bcc1b91035af6a65a2d8688f43e22435df8e40c167ed0dff00618393dd19df`.
Conformance remains 123/127 with four known unsupported, zero regressions and eight
untested feature rows. This supersedes the earlier 2919-test browser-core checkpoint
for current code. Chrome and 81 Node checks remain independent evidence. Only
checkpoint documentation changed afterward; no code/build input changed. Remaining work is
browser dev/controls/effects, cross-engine acceptance, native targets and release
acceptance under the original goal.

### Browser pilot effects and idle collection — wave 1, 2026-11-04

Status: implemented on branch `wave1/w1-browser-pilot-effects`, scoped
verification only (the lead's full gate runs after the wave merge). The
official browser host now admits and implements `date_now`, `number_random`,
`env_snapshot` and `process_exit_f64` with native results, statuses, output,
fuel and host-call charges ([mapping](embedding.md#browser-effect-mapping-official-host),
[contract](architecture.md#browser-fuel-transform-foundation-in-development)),
and runs `varta.collector.idle` between the turns of `mount` sessions under the
maintenance budget. This removes the uarp-sdk blockers for `Date.now`/
`Math.random` backoff, `crypto.getRandomValues`/`randomUUID` and
`globalThis.process?.env` in the browser. Other unimplemented effects are still
refused before any bundle file exists.

Evidence: Rust registry/admission/template tests; 11 Worker handler tests in
`worker-effects.test.mjs` (denials: missing `time.now`, empty or disabling
ceiling, `env.get` filtering, exit denial and invalid codes; maintenance fuel,
deadline and invalid-status failures); `scripts/test-browser-effects.mjs` in
local Chrome against native `varta dev` for six cases (granted effects with
exit code 3, empty ceiling, no grant, `randomUUID`, a 13-turn timer session
with idle collection, and the same session under a maintenance ceiling below
one step). Logs, exit codes, host calls, microtasks, output, per-turn fuel and
the listed idle periods are equal. Firefox 157 also passes the six cases and
the 13 generated fixtures through the Playwright 1.64.1 matrix; WebKit was not
run (its 1.64.1 build is not installed).

Open:

- The maintenance-exhaustion failure is covered by Worker tests with the
  native text, not by a cross-host golden: with production step units a
  well-formed module cannot exhaust `maintenanceFuel` (`maintenance.rs`), and
  the native exhaustion test needs a test-only step size.
- `crypto.randomUUID` fuel is value-dependent in the guest prelude natively
  (25 native runs of one program: 24 × 22441, 1 × 22421), so its case compares
  everything except fuel. Making the prelude's cost value-independent is an
  owner decision.
- The browser maintenance deadline cannot interrupt a step inside guest code;
  it is checked between steps and enforced by terminating the Worker.

### Local browser dev — immutable snapshot host 2026-11-02

`varta dev --target browser` now prepares the normal checked artifact/resources,
then serves an immutable memory-only bundle on loopback. The CLI prints the URL;
`--listen` can choose another loopback port, Ctrl-C closes active connections,
`--app` enables generic v38 application mode and `--budget-report` displays page
usage. Native/default dev behavior remains. Native fuel profiling/view scripts
are rejected in browser mode. No guest runs natively, execution roots are not
bound, and no production output/source/policy files are served or written.

Runtime `--policy` ceilings derive the private memory maximum and trusted bootstrap
inputs while preserving original Wasm/manifest. `--decisions` validates the full
file but exposes only the current artifact's records. The loader gets a new SRI/
CSP hash and separate dev opt-in; a format mismatch fails closed. Opt-in rewriting
cannot change a requested origin that contains the marker text. Connections,
headers/buffers and lifetimes are bounded; foreign hosts/origins, non-origin-form
targets, body-bearing requests, unsupported methods and non-bundle paths reject.
The host reuses locked Hyper/http-body/Tokio dependencies; signal support is local
tooling, with no version changes or core→tooling dependency reversal.

Focused evidence: 25 project browser tests (including eight new dev tests), 16 CLI
browser checks (three new dev checks), two dependency-boundary checks and 81
oracle-enabled Node tests pass. Project all-target checking and targeted project/
CLI Clippy pass. Real Chrome exercises actual CLI console/view/application startup,
CSP/SRI, policy denial, a lower private memory ceiling, scoped decisions, trusted
input and graceful stop. Fresh distribution console/network, view and consent
regressions also pass after the bootstrap refactor. Logs:
`/tmp/varta-browser-dev-project-verified-v2.log`,
`/tmp/varta-browser-dev-cli-final.log`, `/tmp/varta-browser-dev-cli-latest.log`,
`/tmp/varta-browser-dev-boundaries.log`, `/tmp/varta-browser-dev-node.log`,
`/tmp/varta-browser-dev-all-targets.log`, `/tmp/varta-browser-dev-clippy.log`,
`/tmp/varta-browser-dev-clippy-final.log`,
`/tmp/varta-browser-dev-chrome-final.log`,
`/tmp/varta-browser-dev-network-regression.log`,
`/tmp/varta-browser-dev-view-regression.log`,
`/tmp/varta-browser-dev-consent-regression.log`.

Both distribution and dev reject HTTP service contracts before output/listening;
those contracts require the native target rather than a main-only browser run.
This first increment serves a fixed snapshot; source edits require restart.
Live rebuild/reload, diagnostics and their session lifecycle remain next in the
open dev batch. The 2921-test full gate above covers the preceding consent/event/
navigation package and does not cover this newer code. Consolidated verification
remains at the broader dev/reload boundary. Firefox/WebKit, wider effects/layout,
native platforms and release acceptance remain in the unchanged full objective.

### Browser dev rebuild foundation — bounded input observations 2026-11-02

The shared source loader now has opt-in content/probe observations. Browser-dev
preparation enables them across configuration, entry/module/text reads and the
packaged resource loader, then rejects observed drift before listening. Ordinary
compilation keeps observation disabled. Hashes come from the actual bounded bytes;
rechecks reuse pinned no-follow dependency descriptors and each original read
limit. Missing `.ts`/`.tsx` candidates and failed reads participate. The 4096-key
journal never evicts; contradictory repeated reads invalidate the attempt even
if bytes are restored. Unrelated files do not trigger a change. This is drift
detection rather than a claim of an atomic project snapshot.

Six focused tests cover equal-length changes, repeated-read conflicts, unrelated
files, missing candidates/parents, failure recovery, oversized reads, explicit
configuration/external entry inputs, no-follow replacements, retained directory
anchors, overflow and the actual compile/text/resource pipeline. Live rebuild,
revision-pinned file delivery, error diagnostics and page/session reload remain
the next implementation; this foundation does not claim those are delivered.
All 44 project library tests, all 25 CLI module integration tests and targeted
project/CLI all-target strict Clippy pass. Formatting and whitespace checks pass.
Evidence: `/tmp/varta-input-watch-focused-final.log`,
`/tmp/varta-input-watch-project-final.log`,
`/tmp/varta-input-watch-cli-modules.log`,
`/tmp/varta-input-watch-clippy-final.log`. These are focused checks after the
historical 2921-test gate; no full-goal or browser-dev acceptance is closed.

### Browser dev revision-pinned delivery — 2026-11-02

Development pages now pin their loader and every transitive verified fetch to a
content-derived `/__varta/<revision>/` directory. The revision covers length-framed
sorted file names/contents including runtime bootstrap inputs; rewriting the page
preserves loader CSP/SRI and rejects opt-in format drift. Mutable root asset
aliases are unavailable. The shared delivery catalog retains at most two distinct
revisions, treats identical publication as a no-op, deduplicates reverts and
never substitutes current bytes for expired/unknown paths. Already owned response
bytes survive eviction. The root page selects the current catalog entry.

Four new focused tests cover framing, every file's contribution, pinned page/
asset routes, root alias/path rejection, retention, reverts, immutable owned
responses, no fallback and format drift. Existing HTTP/runtime-input checks now
exercise revision URLs; the CLI smoke and real Chrome probe discover the pinned
directory from the page rather than relying on root aliases.
All 12 browser-dev tests (four new catalog cases), three CLI dev checks and
targeted all-target strict Clippy pass. Real Chrome passes console/view/application
startup, CSP/SRI, policy/memory ceilings, scoped decisions and graceful stop
through the revision routes. Formatting and whitespace checks pass.

At that checkpoint the server published only the startup snapshot; the live
rebuild increment below adds polling, publication and page/session reload.
This increment supplied the delivery boundary needed for them. Evidence:
`/tmp/varta-browser-revisions-tests.log`, `/tmp/varta-browser-revisions-cli.log`,
`/tmp/varta-browser-revisions-clippy.log`,
`/tmp/varta-browser-revisions-chrome.log`. These focused checks do not replace the
historical full gate or close cross-browser/native/release acceptance.

### Browser dev live rebuild/reload — 2026-11-02

After an initially valid startup, one serialized off-thread worker checks actual
input observations every 500ms and recompiles only changed inputs. Missing-file
and failed-read observations survive compile failures; creating a missing module
recovers without editing its importer. Invalid configuration retains the prior
watch set. Changes are baselined before attempts, avoiding repeated compilation
of unchanged errors without swallowing changes during failure. Runtime policy
and decision snapshots remain captured at startup.

Only stable admitted bundles publish into the bounded revision catalog. Errors
retain the preceding working version and at most 16KiB of UTF-8-safe diagnostic
text. `/__varta/status` uses the same host/origin/method/body admission and no-store
delivery, with 304 disabled. The dev-only SRI-covered client polls serially with
deadlines, a bounded response and validated revision/error fields. It renders
diagnostics as text outside the guest view; a new revision aborts the old owned
session before root navigation. Pagehide or unavailable/invalid status also stops
the session. Unchanged rebuilt bytes clear errors without reload.

Focused evidence: all 52 project library tests, 25 CLI module tests, three CLI dev
checks and project/CLI strict all-target Clippy pass. All 85 oracle-enabled Node
tests pass, including four dev-client tests. Actual Chrome proves missing-dependency/config
recovery, real realm replacement, old-generation retention/expiry, error clearing
without redundant reload and exactly one old live view Worker termination before
replacement, alongside previous mode/policy/decision/CSP/SRI/graceful-stop probes.
Logs: `/tmp/varta-browser-reload-project.log`,
`/tmp/varta-browser-reload-modules.log`, `/tmp/varta-browser-reload-cli.log`,
`/tmp/varta-browser-reload-clippy.log`, `/tmp/varta-browser-reload-node.log`,
`/tmp/varta-browser-reload-chrome-live-view-v2.log`.

Initial-invalid projects still fail before listening; no initial error page,
automatic reconnect or cross-browser acceptance is claimed. An in-flight
synchronous compiler call can finish after shutdown, with future checks and
publication cancelled; the CLI runtime does not wait for it. This does not claim
interruptible compilation. The consolidated dev/reload full gate passes:
**2946 passed, 0 failed, 16 ignored**, all eight commands, source unchanged.
Report: `target/verification/20261102-093345-4pi5qr3n/report.json`;
SHA-256 `ababac54e03c6070e9a05140d1d02fdfb6b1d312d607ec600067ef3b1d691a7b`.
Only result documentation changed after the gate. P9, native platforms and
release acceptance remain open.

### Scoped Chrome/Firefox/WebKit acceptance — 2026-11-02

An isolated Playwright 1.64.1 tool installation outside the repository runs the
same 13 generated fixed fixtures on Chrome 156.0.8078.74, Firefox 157.0 and
Playwright WebKit 27.2. Every case compares status, exact native logs and all
resource counters: console/UTF-16, Promise FIFO/rejections, v36/v37/v38 timers,
unreferenced/cancelled timers, live view startup, PNG and WOFF/WOFF2 fonts.
Five tamper boundaries per engine reject before guest or Worker start: original
Wasm, private image, manifest, Worker and loader SRI. Three live dev sequences per
engine cover missing-dependency recovery, trusted view clicks/old Worker teardown
on replacement, and server-loss teardown. The final matrix is **63 passed,
0 failed**. Installed Chrome runs with its process sandbox enabled; no browser
security settings or personal profiles are changed.

Report, fixture SHA-256 inventory and local drivers are archived at
`target/browser-matrix-20261102/report.json`. Firefox's initial launch attempts
failed before Varta execution because the default app-data directory was not
usable. Canonical paths alone did not fix it; explicitly isolated `MOZ_APP_DATA`
and `MOZ_LOCAL_APP_DATA` roots did. The startup behavior is tracked in the
[upstream Playwright issue](https://github.com/microsoft/playwright/issues/42768).
Final Firefox cases run unchanged artifacts with owned data roots. Initial
driver-selection mistakes and failed startup attempts retain their own logs;
they are not counted as executed guest cases.

This is a scoped local engine matrix, separate from Cargo's full gate and the
85 Node checks. Playwright WebKit is not installed-Safari product acceptance.
The 39 execution cases now have a portable repository driver,
`scripts/test-browser-matrix.mjs`, verified on all three engines. The integrity
driver now also runs from `scripts/test-browser-integrity-matrix.mjs` and passes
15 checks across the three engines. `scripts/test-browser-dev-matrix.mjs` runs
the nine live-dev scenarios across those engines; all pass locally.
permission/network/event/layout coverage, the actual UARP session and native
targets remain outstanding. These observations do not close P9 or M7.

### UARP view browser admission probe — 2026-11-02

The following paragraphs record sequential checkpoints; the most recent state is
the enabled UTF-16 read/has increment at the end of this section.

The real `examples/uarp-console/varta.view.json` project was submitted unchanged
to `varta dev --target browser`. Compilation reached browser admission, which
rejected the unsupported `env_get16` host effect before opening a listener:
`browser host effects not implemented: env_get16`. The view test entry reads
`TEST_VIEW` to select its scenario. This is evidence for the next host capability,
not a successful UARP browser session; no browser execution was attempted. The
existing browser admission tests cover explicit rejection of environment reads.
Browser environment input must preserve typed UTF-16 behavior, exact requested
names and host ceilings, deny-by-default policy, accounting, bounded snapshots,
and isolation from ambient browser/OS environment. It requires native/browser
differential and denied-access checks before rerunning the unchanged pilot.

The first host-input component now snapshots explicit name/value pairs into a
private bounded map and rejects duplicate names, invalid Unicode and oversized
inputs. Two Node tests cover ownership, missing names and invalid input. This
component is not yet connected to the loader, capability checks or Wasm imports;
`env_get16` and the unchanged UARP view remain unsupported in the browser.
An independent scope component now intersects requested and host patterns using
bounded Unicode-scalar wildcard matching and native-style exact-name enumeration;
an access adapter now asks the permission broker and audits before lookup,
including revocation, missing reads and `env.has`. Four Node tests pass. The
adapter is not yet wired into the Worker, loader or Wasm ABI.
An independent UTF-16 ABI adapter now exercises bounds, strict text decoding,
fuel hooks, result/status reset and denial/missing distinctions; six focused Node
tests pass. Worker/loader wiring and actual native/browser differential execution
remain required before enabling imports.
The environment ABI limits, result-global names and status codes are now
generated from the Rust browser registry; its drift tests and the six focused
environment Node tests pass. Worker-side integration remains outstanding.
This registry/code change is newer than the recorded 2946-test full gate.
The loader now validates and snapshots explicit `environmentInputs` before any
fetch and sends a copied input list in the Worker startup message. Chrome checks
that invalid input fails before fetching; 14 project browser tests and the Node
browser suite pass. Worker-side authorization and ABI imports are still disabled,
so this does not enable guest environment access or the UARP browser session.
The Worker now constructs its own validated snapshot, scopes and broker-bound
UTF-16 handlers, with lazy access to globals after instantiation. Import admission
remains closed pending end-to-end parity and host-ceiling delivery tests. The
browser project tests and all 91 Node tests pass with the updated Worker fixture.
Two additional isolated Worker tests now exercise allowed Unicode values, host
ceiling denial and auditing with a test-only admission override, and confirm the
production Worker still rejects the import before guest entry. These do not prove
real Wasm execution or native accounting parity.
The loader and local development bootstrap now preserve checked `env.get` and
`env.has` host-ceiling patterns, while excluding filesystem scopes. Focused Rust
and Chrome tests cover serialization and reject malformed ceilings. Wasm import
admission remains closed and real environment access is not yet enabled.

The `env_get16` and `env_has16` imports are now admitted and connected to the
Worker adapter. A generated real Wasm program run in Chrome matches native
Unicode values, denied/missing behavior, output, fuel, host calls and microtasks;
an empty host ceiling also denies access. Evidence:
`scripts/test-browser-environment.mjs`, `/tmp/varta-env-enabled-parity-final.log`,
14 project browser tests, 17 browser-dev tests and 18 focused Node tests. These
changes postdate the 2946-test full gate. The unchanged UARP view now passes
browser admission and starts a loopback server; a headless probe waits at the
required permission prompt, so no completed UARP session is claimed. Supplying
environment inputs from the CLI, full permission/prompt and cross-browser tests,
and broader result-buffer growth/failure parity remain open.

The environment result writer now converts a failed memory growth into the
native-style I/O status and clears result length instead of terminating the
Worker. An isolated one-page Worker test covers this path; a native Wasmtime
probe confirms the one-page growth diagnostic. The 20 focused Node cases and
the real Chrome/native environment comparison pass. A full real-Wasm boundary
comparison remains open.

For local development, embedding `--env` values into the HTTP-delivered loader
would let a guest with an allowed request to that server fetch values without an
`env.get` grant. That approach was discarded; the bootstrap now explicitly rejects
embedded `environmentInputs`, with a focused regression test. The programmatic
host API can still pass a private copied snapshot directly to `mount`/`run`.
Using that API, explicit test decisions and `TEST_VIEW=stream`, the unchanged UARP
view reached ready in isolated Chrome with no logs after 14 turns; its rendered
text contained the recorded permission/result stream. This is a startup/content
observation, not a complete interactive or cross-browser acceptance. A secure CLI
value channel remains to be designed and implemented.

The application-owned `scripts/test-browser-chat.mjs` now replays the five
recorded chat interactions in Chrome using trusted pointer and keyboard events.
It checks the entered message and pending status against the native view
expectation. This focused interactive probe passes; it compares selected text,
not the full tree/accessibility snapshot or the cross-browser pilot session.
The same interaction checks now pass in isolated Firefox 157,
Playwright WebKit 27.2 and Chrome 156 through the application-owned
`scripts/test-browser-chat-matrix.mjs`. All ordered element/text nodes and
non-style attributes match the native final snapshot in each engine. This covers
the chat fixture only and does not establish computed-style, accessibility,
layout or complete pilot parity. Evidence: `/tmp/varta-uarp-attributes-matrix.log`.
Eight selected computed style properties also match across all three engines.
All three consistently report `display: block` for three nodes whose native
snapshot records `inline`; the harness checks and reports these exact differences
instead of claiming full style parity. Each affected `span` is a direct child of
a `display:flex` container in both snapshots. This is consistent with CSS flex
item blockification: browsers compute `block` while the headless snapshot retains
the cascaded `inline` value. See the [CSS Flexbox specification](https://www.w3.org/TR/css-flexbox-1/#flex-items).
The harness now verifies both parent displays as well as the exact differences.
Reconciling the headless snapshot contract and downstream layout remains open;
this observation alone does not establish geometry parity. Evidence:
`/tmp/varta-uarp-flex-blockification.log`.
The headless view now also offers an explicit `snapshot_flex_blockified()`
projection for declared `inline` children of declared `flex` parents. It keeps
the established cascade snapshot unchanged; absent defaults and all other layout
rules remain unmodeled. Focused unit and public-API tests pass. This is newer than
the last full gate and does not close layout or cross-target acceptance.
The public projection helper is now applied to the real UARP chat snapshot by
the application-owned three-engine harness. All nine selected style properties,
including the three flex displays, match Chrome, Firefox and WebKit after this
native projection; the original cascaded snapshot is unchanged. Evidence:
`/tmp/varta-uarp-projected-matrix.log`. This remains a selected-property comparison,
not complete computed style, accessibility or geometry acceptance.
The engine-owned browser contract probe now emits a flex batch with both native
cascade and the optional native projection. A real Chrome view applies the same
batch and confirms its computed `display` matches the projected `block`, while
the original native cascade remains `inline`. The shared native oracle tests pass
with the extended fixture. This is a focused direct-child case, not the complete
UARP projected snapshot or general layout parity.

An unchanged UARP view was then launched in an isolated Chrome profile and its
two required permission prompts were allowed by trusted input. This exposed a
browser-only rejection of `translateY(1px)`: grammar parsing had discarded the
registered function-name spelling. Preserving that spelling fixes the stylesheet
without broadening native CSS acceptance. The UARP page reached `ready` in the
corrected Chrome probe; `TEST_VIEW` was not supplied, so the host logged the
missing-variable I/O error and selected the default scenario. The final matcher
accepts the actual stylesheet and all 20 view tests, including the native CSS
oracle, pass. This verifies startup only, not the complete interactive pilot or
cross-browser parity. The fix is newer than the last full gate.

The UARP three-engine comparison now checks 195 explicitly declared values across
14 style properties against the flex-projected native snapshot. All match in
Firefox 157, WebKit 27.2 and Chrome 156. The harness verifies the number of
properties and declarations exercised; absent defaults, complete computed style,
geometry and accessibility remain outside this check. Evidence:
`/tmp/varta-uarp-styles-expanded-final.log`.

The UARP matrix driver now resolves its default native projection tool relative
to the repository and accepts explicit projection-tool and Chromium-channel
overrides. It passed all three engines when launched from `/tmp`, both with
defaults and with explicit tool/Chrome settings. The bundled Chromium option and
other operating systems still need their own runs. Evidence:
`/tmp/varta-uarp-portable-matrix.log` and `/tmp/varta-uarp-portable-override.log`.

The same pilot matrix now compares selected accessible region, button, heading
and textbox roles and names with the native snapshot, plus its recorded heading
levels, expanded/disabled states, input value and focus. These checks pass on all
three engines. The test initially opened shadow roots for Playwright's role selectors;
it now retains a test-only reference to the actual closed root and all three engines
pass again (`/tmp/varta-uarp-closed-accessibility.log`). This does not verify the platform accessibility tree or all roles,
states and assistive technology behavior. Evidence:
`/tmp/varta-uarp-accessibility-matrix.log`.

The headless role mapping now recognizes `section` landmarks labeled via
`aria-labelledby`, retaining explicit-role precedence and existing `aria-label`
behavior. The scoped view suite passes; the matching fixture passes Playwright
role selectors in Firefox, WebKit and Chrome (`/tmp/varta-section-landmark-matrix.log`).
This checks selector behavior, not platform accessibility APIs or screen readers.

The native geometry foundation now exposes an experimental headless box-only
API using Taffy 0.15.1. It supports div block/flex trees, pixel dimensions, gap
and flex direction; unsupported computed CSS, elements and text fail explicitly.
It bounds nodes, nesting and viewport dimensions, and does not change the existing
snapshot. One flex fixture matches Firefox, WebKit and Chrome
(`/tmp/varta-flex-box-matrix.log`); this does not establish general layout parity,
text shaping, native rendering or target acceptance.

Box-only layout additionally maps flex wrapping, alignment, growth, shrinkage,
pixel basis and individual gap properties. A shorthand combined with a gap
longhand is rejected until cascade precedence is modeled. Growth/alignment and
wrapping fixtures agree with Firefox, WebKit and Chrome
(`/tmp/varta-flex-expanded-matrix.log`); other combinations remain to be tested.

The box-only subset now supports pixel min/max dimensions and pixel margin/padding
shorthands and longhands. Mixed shorthand and longhand declarations of the same
kind fail closed until cascade precedence is implemented. A nested flex fixture
with padding, margin and min-width agrees with Firefox, WebKit and Chrome
(`/tmp/varta-flex-spacing-matrix.log`); general block margin behavior remains
outside that observation.

The experimental box layout now expands stylesheet `margin`, `padding` and `gap`
shorthands in cascade order, including two-value gaps, before inline properties.
The ordinary snapshot is unchanged. Mixed shorthand/longhand within the inline
map is still rejected because it does not preserve operation order. A fixture
covering stylesheet order and inline precedence, and an unresolved-variable
fixture, agree with Firefox, WebKit and Chrome
(`/tmp/varta-flex-cascade-variable-matrix.log`).

The geometry path now records the most recent inline writer for each margin,
padding and gap longhand, so shorthand and longhand updates and removals can be
applied in operation order without changing the established style snapshot.
A margin update/removal sequence agrees with Firefox, WebKit and Chrome
(`/tmp/varta-inline-cascade-matrix.log`). Further CSSOM combinations remain
subject to broader differential coverage.

Box geometry now accepts nonnegative percentages as well as pixels for dimensions,
min/max constraints, margin, padding, gaps and flex basis. Nested percentage sizing
and a percentage gap/basis fixture agree with Firefox, WebKit and Chrome
(`/tmp/varta-layout-percent-flex-matrix.log`). Intrinsic and indefinite sizing,
negative margins, text and broader combinations remain open.

Box geometry now handles uniform `border-width`, `border-style` (`solid` or
`none`) and the non-geometric `border-color` longhands; an unspecified width
uses the CSS medium default of 3px when the style is solid. A border toggle and
default-width fixture agrees with Firefox, WebKit and Chrome
(`/tmp/varta-layout-border-final.log`). Border shorthands and individual sides
remain unsupported. The current view registry does not expose `box-sizing`;
the experimental path uses the default content-box model.

The box-only layout also accepts the `border` shorthand and independent
`border-top/right/bottom/left` shorthands with supported pixel widths, `solid`
and `none`. Overlapping shorthand and width/style forms are rejected until their
cross-property cascade is implemented. The uniform and asymmetric examples
match Firefox, WebKit and Chrome (`/tmp/varta-border-shorthand-matrix.log`).

Border geometry now resolves shorthand, individual sides and uniform width/style
in stylesheet order and tracks their inline updates and removals per side and
component. Mixed cascade and inline update fixtures match Firefox, WebKit and
Chrome (`/tmp/varta-border-cascade-final.log`); broader CSS and native renderer
acceptance remain open.

The headless crate has an explicit-font text measurement foundation using Parley
0.11.1 with system font discovery disabled. Font bytes, family and input sizes are
checked; the original synthetic font fixture measures `AAAA` at 32px for a 20px
font size, matching Firefox, WebKit and Chrome (`/tmp/varta-explicit-font-matrix.log`).
This is a single text-advance observation, not inline layout, font fallback,
rendering, full text shaping acceptance or native target UI.

`layout_boxes_with_text` now connects explicit-font Parley measurement to the
Taffy tree for isolated ASCII alphanumeric text nodes in flex containers. It
requires a caller-supplied covering font and size, bounds text work, and rejects
adjacent text, whitespace, baseline alignment and general inline formatting.
One text-plus-box fixture agrees with Firefox, WebKit and Chrome
(`/tmp/varta-flex-text-matrix.log`); real paragraphs, font fallback and native
rendering remain open.

The experimental flex-text path now collapses ASCII whitespace and measures
min-content and wrapped text at the width requested by Taffy. Total text and
repeated shaping work are bounded. A two-line fixture agrees with Firefox,
WebKit and Chrome (`/tmp/varta-wrapped-flex-text-matrix.log`). Adjacent nodes,
non-ASCII layout text, rich inline content and baseline alignment remain open.

The experimental flex-text path now also accepts Cyrillic alphanumeric scalars.
A synthetic explicit font covering `А`, `а`, `і` and `ї` produces matching
two-line geometry in Firefox, WebKit and Chrome
(`/tmp/varta-ukrainian-flex-text-matrix.log`). This is not full Unicode, font
fallback, punctuation, adjacent-node or general text-layout acceptance.

Flex text additionally accepts ASCII graphic characters and selected Ukrainian
punctuation; CSS whitespace is collapsed. A synthetic-font Ukrainian punctuation
and wrapping case matches Firefox, WebKit and Chrome
(`/tmp/varta-ukrainian-punctuation-matrix.log`). This does not establish general
punctuation, font fallback or full Unicode acceptance.

The experimental explicit-font layout now supports a single text child of a
block as well as isolated flex text. Its anonymous text-box geometry gives the
same container height for the tested Ukrainian wrapping fixture in Firefox,
WebKit and Chrome (`/tmp/varta-block-text-matrix.log`). Mixed block children and
general inline formatting remain unsupported and are rejected.

The text-layout path now admits `p` with one text child, applying the default
one-em vertical margins before author styles. A flex-column fixture with wrapped
Ukrainian text, default margins and an override agrees with Firefox, WebKit and
Chrome (`/tmp/varta-paragraph-layout-matrix.log`). General paragraph content and
block-margin collapsing need broader acceptance.

The experimental box path now accepts `main`, `header`, `nav`, `section`,
`footer`, `aside` and `form` as block/flex containers, and models `display:none`
as a zero-rectangle subtree. A semantic screen fragment and a hidden-subtree
fixture match Firefox, WebKit and Chrome (`/tmp/varta-semantic-hidden-matrix.log`).
Unsupported content in hidden subtrees still fails closed; full UI/layout
acceptance remains open.

The experimental geometry path now handles vertical scroll offsets for
`overflow: auto` and `hidden`, including bounds and positioned descendants.
Fixtures agree with Firefox, WebKit and Chrome (`/tmp/varta-scroll-layout-matrix.log`).
Scrollbar gutters, clipping, horizontal scrolling and complete platform rendering
remain open; this calculation does not alter the existing stored scroll state.

Experimental geometry now exposes rectangular viewport and ancestor overflow
clips separately from border-box rectangles, including nested padding-box clips
and positioned descendants. Browser hit-testing fixtures agree across Firefox,
WebKit and Chrome (`/tmp/varta-clip-layout-matrix.log`). Actual native painting,
rounded clips, hit testing, platform scrollbar gutters and horizontal scrolling
remain to be implemented.

The experimental geometry path now admits explicitly styled, explicitly sized
text inputs and single-text-child buttons, with border-box sizing. Their test
fixture agrees with Firefox, WebKit and Chrome (`/tmp/varta-controls-layout-matrix.log`).
Platform-default and intrinsic control sizing, control painting, text editing,
IME and the native shell remain open.

The first bounded CPU paint path draws supported box backgrounds with rectangular
overflow clipping into premultiplied RGBA pixels. A compositing and clipping
fixture matches browser screenshots in Firefox, WebKit and Chrome
(`/tmp/varta-raster-layout-matrix.log`). The first increment rejected text, controls,
borders, positioning and unsupported paint effects; window presentation,
full painting and native platform acceptance remain open.

The CPU path now paints solid rectangular borders with one color across visible
sides, including alpha compositing and `currentcolor`/shorthand cascade. The
fixture matches Firefox, WebKit and Chrome (`/tmp/varta-border-paint-matrix.log`).
Differently colored border sides, rounded corners and full native painting remain
open.

The CPU renderer can now paint the existing bounded explicit-font text subset
using Parley layout and `skrifa = 0.44.0` outlines (Apache-2.0 OR MIT; already a
Parley dependency). A clipped glyph fixture matches sampled browser screenshots
in Firefox, WebKit and Chrome (`/tmp/varta-glyph-paint-matrix.log`). Glyph
hinting, synthetic styles, text decoration, control painting and native window
presentation remain open.

The application instance now exposes its current bounded CPU frame to an
embedding host. An application-level test exercises a guest view commit, a
trusted click, the resulting guest turn and the updated frame. This is an
integration seam; native input and production window hosting remain open.

An experimental `varta-preview` binary opens a native window and hosts an
unprivileged, precompiled view application on a worker thread. Its main thread
presents bounded CPU frames and the worker rerenders on commits and resize.
On the development macOS machine, the test application reached successful
first-frame presentation. This is a preview, not a packaged shell: keyboard input,
IME, accessibility, logical-pixel scaling, permissions, target bundles and
macOS/Pi session acceptance remain open. See `crates/varta-desktop/README.md`.

The preview now maps left-button presses and releases through the displayed
frame's clipped hit regions into guest view events, with bounded delivery and
stale-frame rejection. On macOS, clicking a test guest changed its visible
background and executed its Wasm handler; targeted runtime and renderer tests
also pass. Keyboard, pointer movement/dragging, IME and broader CSS
hit testing remain open.

The desktop preview now handles vertical wheel/trackpad scrolling, including
bounded offsets, nested ancestor chaining and guest `scroll` events. A macOS
test view moved down and back up and logged the guest handler; the focused
headless test covers scroll bounds and nested targets. Horizontal scrolling,
scrollbars, touch gestures, full CSS and Pi acceptance remain open.

The CPU path and preview can now paint and activate an explicitly styled,
enabled button. Its background, border and explicit-font text matched sampled
Firefox, WebKit and Chrome screenshots (`/tmp/varta-button-paint-matrix.log`);
in macOS the test guest handled a click and changed the button's visible color.
Default platform button styling, disabled-state rendering, text input and
accessibility remain open.

The preview now forwards supported keydown keys and modifiers, including Tab,
Shift+Tab, Enter and Space, using the displayed frame's focus and a bounded
queue. Tab skips `display: none` ancestors, and host viewport changes update
the cascade used by input. On macOS, Tab visibly focused a test button and both
Enter and Space invoked its Wasm handler. Full text editing, IME, native accessibility,
full focus semantics and Pi acceptance remain open.

Explicitly styled text inputs now paint their value and end caret and accept
bounded direct keyboard text, Space and Backspace through the existing view
event channel. A macOS test sent a Cyrillic character, a space and Backspace to
a guest `input` handler; sampled input pixels match Firefox, WebKit and Chrome
(`/tmp/varta-input-paint-matrix.log`). Missing glyphs use the supplied font's
`.notdef` outline. Full editing, paste, blinking, IME, default
control styling and native accessibility remain open.

The input model now retains UTF-8 selection endpoints, supports arrow/Home/End
movement, Shift selection, replacement and grapheme-aware Backspace/Delete, and
uses `unicode-segmentation = 1.13.3` (MIT OR Apache-2.0). A stale guest value
patch leaves the current selection intact. A macOS test replaced a selected
Cyrillic character; focused browser fixtures agree for Cyrillic navigation,
replacement and `maxlength` (`/tmp/varta-input-edit-matrix.log`). Firefox's
Backspace removed only the last component of a tested family ZWJ emoji, while
this implementation removes the complete grapheme; that parity issue, full pointer
selection, clipboard, bidi visual navigation and IME remain open.

The preview now resolves pointer clicks and same-window drag endpoints against
the displayed input's shaped clusters, with bounded shaping work and validated
UTF-8 offsets. The selection is applied on release; release outside the field
does not synthesize a click. A macOS test placed the caret and replaced a dragged
Cyrillic selection. Live drag feedback, cross-window dragging, double-click word
selection, clipboard, full bidi editing and IME remain open.

The experimental preview now receives bounded IME preedit and commit events,
renders transient composition and positions the candidate window at the caret.
Preedit leaves the guest value unchanged; commit replaces the selection and
emits an input event. Focus changes and guest view commits cancel composition;
composition events are not exposed to guests. A focused headless test covers
the state transition and validation. Real platform IME acceptance, including
CJK input, remains open, as do clipboard and full editing semantics.
The native event adapter also ignores late IME text while the window is
unfocused and cancels malformed preedit or commit data without leaving the
keyboard in composition mode. Focused adapter tests cover both cases.
At the shared view boundary, a preedit that would overflow the field or an invalid commit clears
the previous transient composition without changing the value or edit sequence;
a focused test covers the UTF-16 size boundary.
The shared view now applies the guest `select` command to the complete text of
a text, search or password input or textarea, including atomic rollback for invalid batches. A focused
test verifies subsequent replacement and the absence of a spurious input event.
Chrome and WebKit also focus an attached, enabled control on `select`; the shared
view now matches that behavior, including focus records, readonly fields and
number/checkable controls. Detached and disabled controls do not receive focus.
Firefox acceptance remains open.
Programmatic focus and selection, as well as native focus input, now skip nodes
under `display: none`; Chrome and WebKit agree for selection under a hidden
ancestor. Scroll-into-view still needs layout-aware application after a commit.
The experimental geometry API now plans nearest-edge vertical scrolling from
inner to outer scrollports, including `overflow: hidden`, without mutating the
view. A nested fixture matches Chrome and WebKit. Positioned paths, horizontal
scrolling and Firefox remain open.
The view API can also apply the plan atomically, generating bounded scroll
records without a user-gesture grant. Focused tests cover nested updates, event
metrics, no-op repetition and unchanged state after unsupported geometry.
An experimental application-host API now applies this scroll between turns and
queues its records for later guest turns.
The view now provides an opaque element identity for deferred scrolling, with
tests for failed batches, ID reuse, separate views and divergent clones. This
addresses target identity for host-deferred actions.
An opt-in layout-aware view commit now executes scroll commands in batch order
using the geometry at each operation, bounds the number of layout operations and
rolls back the batch on a layout failure. The native preview supplies viewport and
font resources before the first turn and updates geometry after resize. A runtime
test covers guest commands at startup and after a media-query-changing resize;
broader native platform acceptance remains open.
On the development macOS machine, the bundled guest-scroll fixture also ran in
the native window: the guest reported offset 100 and the scrolled target was
visible in the first presented frame. The fixture and reproduction commands are
in `crates/varta-desktop/examples/guest-scroll` and `crates/varta-desktop/README.md`.
Scroll records created by layout-aware batches now retain the originating node's
identity, so same-batch removal and ID reuse cannot deliver an old scroll event
to the replacement. A focused batch test covers this case.
Explicitly styled native text inputs now paint a plain, fixed-gray placeholder
when empty. A focused renderer test covers its pixels, unchanged value and
replacement by IME preedit; browser-specific placeholder styling remains open.
Explicitly styled disabled buttons and text inputs now render without accepting
activation or edits. Disabling a field or making it readonly clears transient IME
composition. Focused renderer tests cover both controls; platform-default disabled
appearance and broader accessibility acceptance remain open.
For a focused field disabled afterward, keydown can still be delivered but Enter
does not submit its form and Backspace does not edit. In the observed browser
fixture Chrome did not submit, while WebKit did; this difference remains open.
Character input now emits keydown before the edit and respects `preventDefault`;
Ctrl/Command+A selects all, including with a non-Latin keyboard layout, and
printable Ctrl+Alt text can pass through for AltGr. A focused model test covers
these cases; a macOS window test confirmed Ukrainian input and Command+A replacement.
Bounded undo/redo now restores text and selection, advances the edit sequence,
clears redo after a new edit and respects guest value changes as history barriers.
The shared view history has both byte and entry limits. Focused tests cover
branching, barriers and eviction; a macOS window test confirmed Undo/Redo under
the Ukrainian keyboard layout. Typing-run grouping and cross-target editing
acceptance remain open.
The native editing batch has also passed the complete `varta-view` component
suite. Additional focused checks cover IME as a single undo step, readonly and
preventDefault, equal/stale guest patches, and immediate release of obsolete
history after successful commits while preserving it after rejected batches.
The CPU renderer now paints solid rectangular focus outlines with explicit width,
color and offset, respecting cascade and leaving layout/hit regions unchanged.
A focused test covers focus-visible and shorthand ordering; sampled outline
pixels match Chrome and WebKit. Existing raster/control tests pass; platform-default
focus styling, rounded outlines and native accessibility acceptance remain open.
The native preview now initializes CSS color-scheme conditions from the window's
system theme and follows theme changes, rejecting input and frames for an obsolete
scheme. A runtime test covers startup, resize preservation and light/dark media
updates; direct OS theme-switch acceptance remains open.
Guest view titles are now applied by the native preview with the accepted frame.
An application-level test covers initial and event-driven title changes; the
guest-scroll fixture also demonstrates this integration.
On the development macOS machine its visible window title became
`Varta scroll — 100` after the Wasm handler ran; other platforms remain unverified.
The renderer now has a device-scale-aware raster API: geometry and text remain
logical, paths paint at device resolution, and hit regions and IME cursor areas
use physical coordinates. Focused checks cover 2x and fractional scaling,
viewport media conditions, clipping and text hit positions. Native window DPI
events and cross-display acceptance are the next integration work.
The native preview now supplies its initial device scale to layout and painting,
handles DPI changes, converts pixel wheel movement to CSS pixels and guards
frames/input against stale scales. A runtime test covers scaled physical pixels
and hit positions. Cross-display and multi-platform DPI acceptance remain open.
On the development macOS Retina display, a 160-CSS-pixel input was observed at
320 physical pixels with scaled glyph/caret painting and working character input.
Changing displays or OS scale settings has not yet been accepted.
Fixed-size explicit textareas now join the CPU/native painting path, with newline
preservation, long-text wrapping, caret visibility and two-dimensional pointer
selection including device scaling. Focused checks cover rendered rows, Enter,
soft wrap and scaled hit positions. Vertical keyboard movement, internal scrolling,
auto-sizing, resize handles and platform textarea acceptance remain open.
Textarea wheel/trackpad scrolling now uses the rendered text height, clamps its
offset and participates in ancestor chaining. Manual scrolling is independent
of the caret until a new edit or selection change restores caret-following.
Focused checks cover top/bottom bounds, scaled wheel targets and a subsequent
edit; vertical keyboard movement and platform acceptance remain open.
Unmodified textarea Up/Down and Home/End now use shaped visual rows, preserve the
desired horizontal position through short lines and extend selection with Shift.
The host supplies a validated frame-derived destination; keydown prevention runs
before selection changes. Focused checks cover soft-wrap affinity, Undo, scaled
movement, Shift replacement and preventDefault. Modifier shortcuts, bidi parity
and native platform acceptance remain open.
The reusable native `text-editor` fixture has now been observed on macOS Retina:
Up/Up preserved the column across the short middle line, typing changed the
expected first-line position, and Home/Shift+End/Backspace removed the last row's
text with Undo restoring it and its selection. Guest JSON logs confirmed the
values. Pointer, IME and cross-platform textarea acceptance remain open.
Native wheel input now delivers measured CSS-pixel scroll/client heights, including
textarea padding. Focused checks cover complete record payloads and rejection of
invalid geometry without state changes. The textarea fixture's 108/56 heights
match Chrome and WebKit; an application test covers metric delivery to Wasm at 2x.
Explicit-width textareas now support `field-sizing: content` and `resize: none`.
Focused checks cover an empty line, newline growth, long-word wrapping, maximum
height, shrinking after replacement and minimum height. The CPU renderer uses
explicit-font shaping for intrinsic height; interactive resize and platform
auto-sizing acceptance remain open.

Explicitly styled native `search` and `password` fields now join the text-control
path. Password painting shapes only one mask per extended grapheme, including IME
preedit, while hit testing maps back to source UTF-8 offsets. Selection, caret,
replacement, grapheme deletion, Undo, placeholders and type changes retain the
original guest values. The shaping/work bounds are unchanged; numeric controls,
search-clear affordances and broader native controls remain open.

Explicitly styled checkbox/radio controls now render without fonts, with checked
marks, pointer/Space activation, radio-arrow movement and one Tab stop per group.
Both host models now scope named radio groups by tree and closest form, reconcile
checked membership during patches, and preserve rollback. Native click records
observe preactivation and cancellation restores the previous group without an edit.
The [verification record](history.md#native-form-controls--2026-11-02) covers
native/Wasm/browser and macOS observations. These additions after
`870344d` do not constitute P10 completion. Single-select controls now share live
selectedness, duplicate-value identity and transactional updates across native and
browser hosts. The native preview adds a scrolling popup, pointer/keyboard choice,
cancel and character cycling; TSX initializes values after options while preserving
getter evaluation and reactivity. A shared transaction allowance now bounds owner
walks, repeated option scans and final inventory in native/browser models, including
detached options and UTF-8 sizes; excessive work rolls back the batch. Consecutive
identical writes share a lookup, and admission is independent of deferred local
edits. Logical-key typeahead also handles absent committed text. Native popup
keyboard/pointer/scroll/cancel and character input were observed on macOS Retina.
Timed multi-character search, hover feedback, other-platform select acceptance,
broader focus/cascade work and host-buffer/memory audits,
numeric/progress controls, full styling and focus-modality parity, console screens,
Pi, performance, accessibility and production shell acceptance remain open.
The UARP console's live browser and mockup tooling now use official modules;
`a71d281` removes mockup dependence on the independent prototype. Chrome and
WebKit each pass five screens at three sizes and 41 regression checks. Firefox
could not start in that local run; the earlier scoped matrix is historical
and does not verify this newer package. Full pilot/cross-renderer acceptance
remains open.

### Radio transaction work — implementation 2026-11-02

Radio reconciliation now has an independent shared-registry transaction allowance
for owner walks, group node visits, compared UTF-8 name bytes and moved-subtree
visits. Native and browser models account for checked/type/name writes and
insertion independently of live checked state and edit counters, including stale
patches and unchecked radios. This preserves admission while local edits await
Worker delivery. Excessive work rejects the copied transaction before DOM or
other effects; accepted batches keep existing peer and sequence semantics.

Acceptance: repeated checked/type/name writes, stale patches, wide UTF-8 names
and subtree moves must agree between native and browser on acceptance, failure
position and rollback; a smaller subsequent transaction must still succeed.
A real Wasm handler must fail fatally without reaching catch/later logging and
retain its previous view. Component, registry/documentation, affected Clippy and
browser/Worker checks provide scoped evidence. Local-input radio navigation scans, aggregate retained text, cascade
work, complete platform/pilot acceptance and the release gate remain separate.

Scoped acceptance passed: 109 view tests, 43 runtime/view tests, 113 Node tests
with a fresh native oracle, 55 native/Chrome work batches and 1839 radio DOM/state
observations. Chrome and WebKit each pass the five-screen/15-projection pilot
mockup sample and 44 regressions. Registry/documentation, no-std, affected
all-target Clippy, formatting and whitespace checks pass. The detailed
[verification record](history.md#radio-transaction-work--scoped-verification-2026-11-02)
keeps this separate from the last full gate and remaining acceptance.

### Native radio navigation and retained view payload — implementation 2026-11-02

Tab now builds a transient document/form/name partition once and uses node-set
membership instead of repeated whole-tree group scans. Activation reuses the
captured group to uncheck peers. Navigation keeps document order, checked stops,
backwards movement, disabled/hidden filtering and event/cancellation behavior;
renaming/reparenting cannot leave a persistent index stale.

The common registry now bounds retained view text and field capacity. Source
slots (text, attributes, class, inline style, title, raw sheets) charge UTF-8 bytes,
including detached nodes. Every value-bearing field reserves its full existing
UTF-16 bound's worst-case UTF-8 size; that reservation survives type changes and
frees only on node removal. It therefore does not depend on undelivered local
values or stale counters. The explicit draft default admits at most 85 reserved
value fields without other payload. Final-state validation allows reclamation
within the same batch and rejects excess atomically. JS size caches are copied
with transactions and hidden from snapshots; synchronized field records cannot
bypass the per-value bound.

Acceptance: many distinct radio names and subsequent membership changes preserve
focus order; native/model/DOM agree on every retained source category, exact
capacity, UTF-8 sizes, detached state, replacement/deletion and sticky field
capacity. Pending Worker edits must preserve both accepted and rejected
admission. A Wasm resource failure must retain the prior view and stop before
catch/later effects. These are payload limits, not a full process-memory claim;
CSS/cascade work, other host buffers, sustained resource measurements, complete
controls/pilot and cross-platform/release acceptance remain open.

Scoped acceptance passed: 114 native view tests, 44 runtime/view checks and 119
oracle-enabled Node tests; Chrome matches 68 work/payload batches. Each selected
Chrome/WebKit engine passes 15 mockup projections and 47 regressions. Fresh
CLI/companion/projection builds, the normal UARP 107-test/28-group/two-view check,
and live chat tree/accessibility/style/interaction probes pass in Chrome/WebKit.
Registry/documentation, no-std, affected strict Clippy and the intrinsic fingerprint
pass. The [verification record](history.md#native-radio-navigation-and-retained-text--scoped-verification-2026-11-02)
keeps these scoped results separate from the last full gate and remaining target
acceptance. Next resource work: cascade/display costs and other host buffers;
numeric/progress controls, full L0 styling, actual Pi/native shells, metrics and
release acceptance continue in the platform queue.

### Stylesheet expansion and indexed cascade — implementation 2026-11-02

A small media header was copied into every parsed rule without an expansion
ceiling. The common parser and browser checker now share parsed-work units and
limits for one transaction and the installed sheet inventory. Checks precede
condition copying and DOM CSSOM preparation. Replacements/removals update cached
inventory totals atomically. The renderer incrementally indexes root/class
anchors across sheets, so unrelated and empty sheets do not require a full scan
for each node. Class-set lookup and deduplicated anchors preserve the common
matcher, comma specificity, media and source order. Raw declarations are borrowed;
only winning values are cloned. Custom maps are lazy, and Tab/display work skips
hidden subtrees and unrelated properties.

The package also corrects two grammar differences: whole `var()` components now
work in keyword-only properties as the contract already specifies; JS no longer
admits leading/repeated empty declarations rejected by native. A bounded child
reproduced exponential JS matching of repeated ambiguous variable tracks with an
invalid tail. Per-validation memoized sets of possible end positions remove
repeated equivalent paths while retaining positive admission.

Acceptance: exact parsed-work limit and one-extra-unit rejection; condition
amplification; aggregate installed state; repeated replacement within one batch;
rollback of counters, indices and effects; later reclamation; native/browser work
units and original failing operation; rejection before CSSOM parsing and before
Wasm catch/later effects. Sparse/duplicate selectors, sheet replacement/removal,
media changes and inherited variables must retain snapshots and focus order.
Complete cascade/derived-output/accessibility work bounds, grammar edge-case
inventory, host-buffer lifetime, native controls/platforms, metrics and release
acceptance remain open; this closes neither full CSS nor overall host budgeting.


Scoped acceptance passed: 119 native view tests, 45 filtered runtime/view checks,
127 oracle-enabled Node tests and 88 native/Chrome work/payload batches. Selected
Chrome/WebKit engines each pass 15 mockup projections and 51 regressions, plus
compiled live chat interactions/tree/accessibility/style. The normal UARP check
passes 107 tests in 28 groups and two views. A source-free generated browser
artifact verifies the typed stylesheet failure across Worker/DOM bundle bindings.
The [verification record](history.md#stylesheet-expansion-and-indexed-cascade--scoped-verification-2026-11-02)
records the repaired packaging failure and the final successful retries. Full
cascade/derived-output/accessibility budgeting remains the next resource package.


### View component depth and native traversal — implementation 2026-11-02

After `6830dab`, a native regression reproduced stack exhaustion while snapshotting
an accepted 512-edge branch on a normal 2 MiB test-thread stack. The shared draft
registry now caps component depth at 512 edges, including detached roots and text
leaves. Each insertion checks all moved descendants in the existing metered
subtree walk; a later repair in the same transaction does not admit the excessive
intermediate state. Failure reports the insertion and rolls back the complete
batch. The rule is independent of local field state and the smaller experimental
layout ceiling. Native tree/accessibility projection, descendant accessible-name
computation and subtree text collection use iterative work lists. Already-built
JSON values are inserted directly instead of recursively reserialized.

Acceptance: exact boundary snapshots, nested accessibility roles, accessible names,
serialization/clone/drop on the normal test stack; multi-batch detached growth,
attachment/reparenting, text-leaf depth, original insertion position, whole-batch
rollback and successful later recovery; native/model/real DOM parity and Worker
rejection before sending a batch. A compiled Wasm resource failure must stop
before catch/logging and retain its prior view. Scoped checks cover this package;
full cascade/derived-output/accessibility work budgets, other host buffers,
complete controls/platforms, metrics and release acceptance remain open.


Scoped acceptance passed: 119 existing view checks plus four corrected depth
regressions, 46 runtime/view checks and 131 oracle-enabled Node checks. Chrome
matches 102 work/payload/depth batches; Chrome/WebKit each pass 15 mockup
projections and 55 regressions. Generated browser depth/CSS failure probes,
registry/documentation/no-std/strict Clippy and the normal UARP
107-test/28-group/two-view check pass. The
[verification record](history.md#component-depth-and-iterative-native-projection--scoped-verification-2026-11-02)
distinguishes the repaired stack abort, corrected test assertion and final scoped
passes from the previous full gate. Next resource work remains complete cascade,
derived output and accessibility evaluation budgets.


### Potential cascade reads — implementation 2026-11-02

The working tree after `6830dab` and the depth package now charges potential
candidate/source and inherited variable discovery work before style-reading
commands and final inventory admission. Each candidate is considered once through
root/first-class anchors, regardless of duplicate classes, extra selector tests,
media, pseudo-state, local edits or winning declarations. Nodes and inline/class
source bytes are charged even in detached components. Raw `var(` candidates with
installed custom properties add their ancestor scope costs, calculated by an
iterative prefix walk rather than repeated ancestry scans during admission.
Native rule costs are cached with parsed indices; browser anchor sets and rule
records are private, incrementally updated and copied with the transaction.

One allowance spans command reads and the final inventory. Equal/disabled focus
commands do not discount it. Reading an excessive intermediate state fails at the
command, before style computation; final failure has no operation index. Later
replacement/removal can reclaim transient capacity if no command first reads it.
This package targets potential matching and inherited-map discovery. Derived-value
expansion/output, repeated accessibility evaluation, browser-private invalidation
costs and other host-buffer/lifetime limits remain open.

Acceptance: exact budget and one-byte overflow; duplicate/compound anchors;
rejected stylesheet replacement without poisoning indices; inactive media and
unmatched selector tails; detached inventory, ancestor scope changes/reparenting,
reclamation, repeated equal command reads and original command/final failure
positions. Native/model/DOM/Worker must agree and retain their previous state.
Compiled Wasm and source-free browser artifacts must stop before catch/logging
and preserve their last successful view. Broader platform/release criteria remain
separate from these scoped checks.


Scoped acceptance passed: 129 native view checks, 47 runtime/view checks, 138
oracle-enabled Node checks and 127 real Chrome work/payload/depth/cascade batches.
Each selected Chrome/WebKit engine passes 15 mockup projections and 59 regressions.
Fresh source-free artifact probes cover cascade, depth and stylesheet fatal
failure; normal UARP passes 107 tests in 28 groups plus two views, and live chat
passes tree/accessibility/style/interaction probes on Chrome/WebKit. Registry,
documentation, no-std, affected strict Clippy and intrinsic fingerprints pass.
The [verification record](history.md#potential-cascade-reads--scoped-verification-2026-11-02)
keeps these results separate from the last full gate. Next work remains derived
CSS value resolution/output and repeated accessibility evaluation, followed by
other host-buffer/lifetime and platform/release acceptance.


### Computed custom-property scopes — implementation 2026-11-03

Native substitution previously stopped after one alias and flattened inherited
raw declarations, which rebound parent aliases in child scopes. The renderer now
computes declarations at each element before inheritance. An iterative work stack
tracks active dependency cycles and evaluates fallback branches only when needed;
known invalid cycles cannot capture outside fallback consumers. This follows the
current working-draft substitution contexts and the Chrome comparison; WebKit
has a recorded outside-cycle fallback discrepancy. For unused self-fallbacks, the initial 2022 eager fallback graph was contradicted by both browsers.

Resolved fragments use arena indices and length summaries, with direct alias
reuse. Dependency traversal and ownership/drop do not recurse with alias depth;
UTF-16 size is checked before expansion. Empty/nested/function fallbacks and
case-insensitive variable functions are admitted by both validators. Malformed
custom references/functions/hashes fail early. With a valid variable reference,
the whole property grammar and numeric notes defer until substitution; property
whitelists and forbidden-source checks still apply. Native snapshot, geometry and paint revalidate
resolved values, preserve quoted strings and token boundaries, and do not revive
an earlier declaration after computed invalidity. Display-none consumers handle
keyword casing. The browser font namespace handles variable function casing.

Acceptance: alias chains, scope freezing and invalid local masks; actual cycles,
unused fallback self-references, outside consumers, invalid-prefix cycles and
cross-ancestor aliases; empty and wide-keyword fallbacks, quoted text, component
boundaries, selected numeric notes, viewport changes and geometry/paint; normal
2 MiB stack traversal/drop at dependency depth 5000, exact UTF-16 value limit and
bounded expansion. Independent browser CSS must agree with native resolved
values in the sampled supported property subset. Compiled Wasm must execute new
syntax and update aliases across turns. Native per-value computed invalidity is
renderer policy; common aggregate output/work limits and broader computed
property/default/inheritance coverage remain separate requirements.

Source-free generated-browser acceptance now passes in Chrome 156.0.8078.80 and
Playwright WebKit 26.5. A compiled v38 application runs through the packaged
loader/Worker after source and configuration removal. Initial styles and two
trusted clicks verify alias freezing, root updates and stylesheet removal, empty
component splices, chosen numeric fallbacks, ordinary cycles, unused self-fallbacks
and the private font namespace. The complex outside-cycle comparison remains
failing on WebKit; this positive artifact check does not establish full variable
parity. The [verification record](history.md#source-free-css-variable-acceptance--2026-11-03)
records that distinction. Next work remains cross-engine cycle semantics and
shared derived-output/expansion budgets, followed by accessibility and other host
buffer/lifetime bounds and platform/release acceptance.

### Empty CSS-reference fan-out — implementation 2026-11-03

During preparation of shared browser variable evaluation, a native materialization
gap was reproduced: doubling a valid empty reference built an exponentially
traversed graph with no output, bypassing the per-value size ceiling. The native
arena now discards zero-length fragments while retaining valid empty bindings.
Missing values, selected fallbacks, quoted empty strings, token boundaries and
CSS-wide keywords keep their semantics. No parser, source, execution or memory
allowance is increased.

Acceptance: a legal 2048-definition forward fan-out must complete snapshot,
style-reading focus, layout, paint and stylesheet replacement within a bounded
subprocess; empty remains distinct from missing. Compiled Wasm and generated
source-free browser artifacts must preserve empty-fallback semantics across turns
for a 32-level doubling fixture. Independent browser style observations must
retain the known complex-cycle discrepancy separately. Nonempty aggregate output,
shared browser variable evaluation, accessibility work and the remaining platform
and release requirements stay open.

Scoped acceptance passes: 145 native view tests, 48 runtime/view tests and 142
oracle-enabled Node checks; strict affected Clippy and fresh CLI/companion builds.
Chrome passes 21 variable fixtures with 231 observations. WebKit agrees on the new
empty fixture while retaining its two known complex-cycle mismatches. The updated
source-free 32-level fan-out artifact passes in Chrome/WebKit, and UARP passes 107
tests in 28 groups plus two views. The
[verification record](history.md#empty-css-reference-fan-out--scoped-verification-2026-11-03)
retains the reproduced timeout and corrected geometry fixture. Shared browser
variable evaluation and nonempty aggregate expansion/output remain the next
resource work; this package does not complete the platform or release plan.

### Browser computed custom-property scopes — implementation 2026-11-03

The official DOM adapter now owns custom-property computation through a JavaScript
port of the native iterative scope algorithm. Browser-native resolution is no
longer the authority for custom cycles: the previously failing WebKit outside-cycle
consumer now matches native output. The shared per-value UTF-16 ceiling, valid
empty values, selected fallbacks, inherited alias freezing and invalid local masks
apply before platform ordinary-property computation.

Prepared CSSOM custom declarations become private compiled records before sheet
adoption. Cached root/first-class anchors select candidates in specificity/sheet/
source order. Iterative component checkpoints avoid inherited-map copies and
restore sibling scopes. Non-ASCII private literal names cannot be declared by the
guest; aliases refer to inherited literals and font values use parallel namespace
bindings. Missing references, including pending shorthands, cannot inherit ambient
page variables. Root-selector rewriting preserves quoted component text.

Commits and focus effects refresh bindings; pseudo-state changes coalesce on an
animation frame, and media conditions have owned listeners. Replacement/removal
release old media listeners, rejected preparations install none, and close/freeze
cancel pending refreshes and release listeners. Renderer-private state remains
outside model snapshots and guest authority.

Scoped acceptance passes: **149 oracle-enabled Node checks**, **35 project browser
checks** and strict project all-target Clippy. Chrome/WebKit each match **23 native
fixtures, 253 computed-style observations and 34 live observations**, including
media/pseudo/input state, focus commands, reparenting/detached nodes, rollback,
removal, ambient isolation, quoted packaged-font names and media-listener lifecycle.
Each engine also passes **15 UARP mockup projections and 59 regressions**, the
source-free generated variable bundle (now including the outside-cycle consumer)
and live chat tree/accessibility/195-declaration checks. The fresh generated
CSS/depth/cascade fatal-failure probes retain prior DOM/title before catch/logging.
The broader Chrome DOM probe still passes 127 work batches, 1839 radio observations,
six activation scenarios and eleven select identity/event scenarios.

The [verification record](history.md#browser-computed-custom-property-scopes--scoped-verification-2026-11-03)
distinguishes browser-native controls from the normalized host results and records
the fixture/server corrections. This closes the recorded selected-engine custom
cycle discrepancy, not complete CSS or platform acceptance. Next work is aggregate
computed output/expansion admission and complete late-value typed-subset parity,
followed by accessibility work, other host buffers/lifetimes, full browser/native
platform evidence, metrics and release acceptance.

### Potential expanded component values — implementation 2026-11-03

Native and browser admission now share an 8 MiB UTF-16 allowance for potential
expanded component content. Cached declaration programs, iterative dependency
components and scoped inherited bounds cover the entire forest without expanding
strings. Custom candidates include parent alternatives and all fallback branches;
standard declarations use a maximum per property. Pure aliases reuse literal
content, while standard literals without references retain their full source
length. Inactive media, remaining selector classes, pseudo-state and winners do
not discount the estimate. Style-reading commands and final validation share the
allowance; excess rolls back before effects with the reading operation index.

Acceptance passes exact-boundary, one-unit excess, repeated command reads,
transient repair, alias reuse, inactive candidates, detached components and plain
oversized standard literals. Four common estimator tests, six native integration
tests and six Node expansion tests include 1458 candidate-world comparisons with
the actual resolver. The current scoped suites pass 151 native view tests, 49
runtime/view tests and 156 oracle-enabled Node checks; the Chrome boundary probe
passes 144 batches. Chrome/WebKit each retain 23 native variable fixtures, 253
style and 34 live observations. A source-free compiled Worker excess retains the
previous DOM/title and terminates before guest catch/logging.

This is conservative component-content admission, not an aggregate host-memory
quota. Serialized JSON, retained frames, namespace/cache/backend overhead,
repeated accessibility/backend work, RSS, late-value typed-subset parity and
platform/release acceptance remain open. The full commit-boundary gate is separate
from these scoped results.

### Potential accessible-name work — implementation 2026-11-03

Native and browser admission now check a shared 8 MiB potential naming-work
inventory before final publication. Numeric descendant and relation summaries
cover the entire forest, repeated `aria-labelledby` tokens, native labels and
future live-field capacity. Referenced traversals disable further labelledby
relations, keeping cycles finite. Visibility and author precedence do not discount
potential work. Root content is charged only for roles that admit name from
content, using the common role/label tables; descendants retain their content
semantics. Excess rejects the copied transaction before effects, with no operation
index. Transient excess can be repaired before the final inventory check.

Native snapshots build attached id/label and visibility indices once per snapshot.
Label order follows document order and hidden-ancestor relation behavior is
preserved. Descendant text remains an ordered fragment range until its traversal
ends, avoiding concatenation at every ancestor. Indices are snapshot-local, so
renaming, reparenting, removal and environment changes do not retain stale names.

Acceptance criteria: exact UTF-8 boundary and one-byte overflow; rollback and
reclamation; detached/hidden/author alternatives; host-label relations and cycles;
field type changes and deferred local input; cache order/lifetime; a depth-512
author-only tree with a full-size text leaf, rejection when all roles become
content-naming and acceptance when only one does. Compare computed element-name
sizes with the bound across 144 relation/visibility/role/control worlds. Compiled
Wasm and source-free Worker failure must retain the previous view and terminate
before guest catch/logging. Selected browser DOM adapters must retain node identity
and accept a smaller retry.

Scoped acceptance passes: **158 native view checks**, including the 144-world
unit matrix and six naming integration checks; **50 runtime/view checks**;
**162 oracle-enabled Node checks**; strict affected all-target Clippy; registry,
embedding-documentation, no-default-features and formatting checks. The final six
naming checks were rerun after extending the role-transition assertions. The
Chrome boundary probe passes **169 resource batches**, including **25 naming
batches**. Chrome/WebKit each pass **15 mockup projections and 63 regressions**,
the positive source-free variable bundle and live compiled chat (tree, selected
accessibility and 195 style declarations). The fresh source-free naming-failure
probe retains prior DOM/title before guest catch/logging; normal UARP retains
107 tests in 28 groups and two view snapshots. The
[verification record](history.md#potential-accessible-name-work--scoped-verification-2026-11-03)
keeps these scoped results separate from the preceding full gate.

This closes one repeated naming source/visit-work gap. Raw text leaves retain
their source bounds; whole serialized output, retained frames, repeated backend
getters, browser accessibility-engine CPU, complete platform accessibility APIs,
RSS, late-value CSS parity and platform/release acceptance remain open.

### Bounded view diagnostic export and effects — implementation 2026-11-03

Native view/application APIs now provide typed compact/pretty JSON export under
a separate 16 MiB encoded-byte allowance. The official CLI scripted-view path uses
it. Keys, escaping and indentation count before output allocation/copy, and no
partial document is published on refusal. Export failure leaves application state
and pending events intact, so compact retry or a later simplifying turn can
succeed. Requested budget diagnostics survive CLI export refusal. Structured
inspection remains available; caller-owned copies and custom serialization are
outside this buffer allowance.

The simulated platform's successful-open history now retains the newest 64
complete URLs within 1 MiB of raw UTF-8. Oldest entries are evicted before retaining
a new entry; `openedDropped` is added to snapshots only after a drop. Permission
and platform refusals do not enter that history, and retention never prevents a
later action. Prompt/drop counters saturate instead of overflowing.

Acceptance criteria: exact encoded boundary, one-byte excess, Unicode/escape/key
accounting in both formats, refusal before oversized-fragment allocation, bounded
capacity during incremental writes, long-URL byte eviction and action continuity.
A deep/wide admitted view must refuse pretty export while accepting compact
export; a compiled application must remain running and handle a later clear
click. The CLI must print no partial JSON or completion marker on refusal, retain
budget diagnostics, and print a normal snapshot after scripted cleanup. Fixtures
build in bounded chunks/turns under the existing select and execution allowances.

Scoped acceptance passes: **166 native view tests**, **51 runtime/view tests**,
**7 CLI view scenarios** and **162 oracle-enabled Node checks**. Strict all-target
Clippy passes for common/view/runtime/project/CLI; fresh CLI/companion, registry
(**5**), embedding documentation (**11**), no-default-features and formatting
checks pass. Normal UARP retains **107 tests in 28 groups and two view snapshots**.
The [verification record](history.md#bounded-view-diagnostic-export-and-effects--scoped-verification-2026-11-03)
keeps this evidence separate from the last full gate at `01acc09`.

Frame-retention inspection found an existing count bound in the desktop worker:
only one frame can await presentation acknowledgement, alongside the currently
held frame; there is no archived frame vector. CPU image dimensions are already
capped at 4096×4096 and 16,777,216 pixels. This is source evidence for those count/
dimension rules, not a new slow-consumer GUI/platform acceptance result or an
aggregate frame/geometry/font/cache/RSS quota. Those broader requirements and
late-value CSS, full native/browser matrices and release acceptance remain open.

### Native GUI input backlog — implementation 2026-11-03

The preview's keyboard/IME backlog now has a centralized 64-record limit and
1 MiB aggregate owned-text allowance. Its existing worker channel remains eight
messages and per-key/IME input validation is unchanged. The new controller checks
prospective count/bytes before compacting text or changing the queue. Spare input
buffer capacity is released on admission; short valid input is preserved even
when a platform buffer had large unused capacity. No committed text is truncated.

Only the last preedit with the same node/commit binding may coalesce; ordinary
keys, characters and IME commits keep FIFO order. Refused replacement preserves
its predecessor. Pop, stale-binding retention and clear release byte accounting.
Every producer, including focus-loss select/IME cleanup, uses one checked enqueue
path; overflow retains the existing explicit GUI error/exit policy.

Acceptance passes **11 desktop tests**: six queue quota/order/coalescing/release
checks, three real-preview-method checks without an OS window, and two existing
IME validation/focus checks. They cover exact bytes and one-byte excess, the
64-record boundary, rejected/successful replacement, full-size Unicode commits,
spare-buffer compaction, first-frame deferral, one-input-per-frame backpressure
and stale-binding capacity release. Strict desktop all-target Clippy, a fresh
`varta-preview` binary, formatting and whitespace checks pass. These are targeted
native-host checks, not a new GUI/OS IME or cross-platform acceptance matrix.

Text-cache inspection found existing bounds in pinned Parley 0.11.1: shaping
data/instance/plan LRUs each retain at most 16 entries; layout construction clears
its resolution/style/analysis scratch and prunes font sources. The engine uses
explicit bounded fonts with system-font discovery disabled. No cache reset or
allowance increase was added. This source audit establishes entry/lifecycle
behavior, not total cache bytes, glyph expansion or RSS. Broader input/rendering,
backend allocation, platform, late-value CSS and release acceptance remain open.


### Native field shaping reuse — implementation 2026-11-03

The preceding naming, bounded diagnostic-export and GUI-input packages were
committed together as `ab045dc`. Its full gate passed all eight commands with
**3133 tests passed, 16 ignored**, **123 conformance cases passed and four known
unsupported**, matching the baseline with no regressions or infrastructure
failures. The source fingerprint stayed unchanged during verification; a separate
19-test browser naming/session selection also passed. These results precede the
renderer change below.

A nonempty native field now retains one shaped layout for caret/selection/hit
geometry and glyph painting. Painting no longer builds a second layout while
the first is still live or clones the display into a temporary `TextShape`.
Ordinary values are borrowed, composition uses copy-on-write and password
masking replaces only the required display buffer; placeholder painting also
borrows its source. Paragraph shaping retains its existing path. Glyph, outline,
aggregate painting, image-dimension and shaping allowances are unchanged.

Scoped acceptance passes **166 native view tests**, including field/password/
IME/navigation/textarea rendering, strict all-target view Clippy, a fresh preview
binary and formatting/whitespace checks. The [verification record](history.md#native-field-layout-reuse--scoped-verification-2026-11-03)
separates these results from the preceding full gate. This is a reduction in
redundant shaping and buffers, not a process-RSS or slow-consumer acceptance
measurement.

At this checkpoint the UTF-16/UTF-8 boundary remained open (addressed by the
following package): a field may hold 32768 UTF-16 units, which become up to 98304 UTF-8 bytes; the public shaper accepts 65536 UTF-8
bytes. Password masks can reach the same 98304-byte size even for ASCII input,
while an IME replacement remains bounded by the existing combined-value check.
The then-existing maximum-password rendering refusal remained a recorded limitation,
not evidence of complete admitted-value rendering. A faithful bounded rendering
path must preserve shaping context, bidi, source offsets, wrapping, selection
and IME without truncating values or increasing guest/work budgets merely to
pass tests. Complete backend-buffer/cache accounting, native/browser platform
acceptance, late-value CSS and release acceptance also remain open.


### Maximum Unicode native field display — implementation 2026-11-03

The byte/UTF-16 mismatch identified above is addressed by a private field source
classification in shaping options. General/public text still accepts at most
65536 UTF-8 bytes. Field display independently verifies the existing semantic
32768-UTF-16-unit bound and its derived 98304-byte UTF-8 reservation, including
password masks and placeholders. Both checks run before building a layout, and
the source classification follows content-sized textarea remeasurement. CSS
cannot select it. No source, guest, glyph, aggregate shaping/paint, image or
font allowance is increased. Each complete field is shaped in one context;
there is no truncation, chunk boundary or persistent enlarged shaper state.

The first focused regressions exposed an independent Parley 0.11.1 issue: its
`ClusterData::text_offset` casts a run-local byte position to `u16`. A valid
98304-byte field run wrapped offsets and panicked in line/source mapping.
A documented local crates.io patch widens that field and its one construction
cast to `u32`; all other copied source/manifests/licenses match the pinned
release. The lockfile changes only Parley's source selection, with no dependency
version changes. Upstream licenses and patch provenance/removal criteria are
retained in `vendor/parley/VARTA-PATCH.md`. Splitting runs was rejected because
it could alter contextual shaping, bidi and caret mapping.

Acceptance covers successful maximum-size BMP/astral/combining field values,
password masks, start/end source hits, caret visibility, grapheme deletion,
one-unit excess refusal with rollback, complete IME replacement/commit and
combined-value overflow cancellation. Full-size Unicode placeholders and
content-sized textarea wrapping/remeasurement must succeed. A large mixed-
direction field must match the pixels, caret and shifted source hits of its
visible suffix, and a subsequent public shaper call must retain its byte limit.
Acceptance passes **181 native tests** (170 view, 11 desktop), the initial
**11-test** field/text selection and an **eight-test** field repeat after a
Clippy-only test-iterator correction. Strict all-target view/desktop Clippy, a
fresh preview binary, formatting and whitespace checks pass. Release archive
checksum/source comparison confirms that the 45 copied vendor files match the
pinned package except for the two documented substitutions. The
[verification record](history.md#maximum-unicode-native-field-display--scoped-verification-2026-11-03)
records the initial failures and unchanged upstream deprecation warning.

Correction to the initial boundary note: inspection of `Input::ImePreedit`
confirms the pre-existing prefix + replacement + suffix UTF-16 check. A valid
composition does not exceed the combined field bound; no larger transient
allowance was introduced. Complete cache/graph/backend/RSS measurement, complex-
font expansion, full bidi navigation, platform/browser matrices, late-value CSS
and release acceptance remain open.


### Large native glyph clusters — implementation 2026-11-03

The next complex-font probes found additional Parley 0.11.1 cluster assumptions:
255 stored glyphs collided with the inline-single-glyph sentinel, larger counts
wrapped, a contributing-character `u8` overflowed on a long grapheme, and font
coverage inspected only a 32-character scratch prefix. The local dependency
patch now uses `u32` glyph counts with a distinct `u32::MAX` inline marker and
`usize` coverage counts. Complete nominal coverage is evaluated without scratch
allocation; a second pure mapping pass publishes IDs only for a better candidate.
No source, font, glyph/outline or aggregate work budget is increased.

Two original, reproducible synthetic fonts exercise plain combining glyphs,
three-glyph OpenType `ccmp` expansion and an incomplete leading font. Acceptance
requires exact output counts around 254/255/256/257 and above; complete fallback
coverage after the old 32-character prefix; a paintable 65536-glyph result;
refusal at 65539 glyphs and a subsequent successful smaller render with the
unchanged field value. The first pre-fix count test observed **1 instead of 255**
glyphs; long-cluster painting also reproduced the contributing-counter panic.
Two focused shaping checks and the focused paint-boundary check now pass.

The expanded patch is limited to four upstream source files and is recorded in
`vendor/parley/VARTA-PATCH.diff`; all other copied source, manifest and licenses
remain identical to the checksum-pinned release. Fixture regeneration produces
identical bytes. The former two-line description belongs to the preceding offset
checkpoint; the current patch additionally covers these cluster counters/mapping.
Acceptance passes **184 native tests** (173 view, 11 desktop), strict all-target
view/desktop Clippy, a fresh preview binary and formatting/whitespace checks. The
[verification record](history.md#large-native-glyph-clusters--scoped-verification-2026-11-03)
records the focused failures, exact-count/coverage checks and provenance checks. Pre-paint glyph expansion allocation/CPU,
complete cache/backend/RSS quotas, full bidi/OS/platform matrices, late-value CSS
and release acceptance remain open.


### Bounded native shaping requests — implementation 2026-11-03

Pre-paint inspection found HarfRust's built-in safety bounds were not configurable
through Parley and incomplete results were not checked before layout copying.
The host now configures draft per-layout limits: 65536 completed glyphs,
262144 intermediate slots per glyph-info/position array, an initial one-million upstream-
charged-operation allowance across all runs, and 65536 retained slots per array.
The completed-output allowance matches the pre-existing paint glyph budget;
excess now refuses earlier with `TextMeasureError::Limit`/layout-text failure.
Intermediate/output limits are separate so normal temporary expansion is not
mistaken for completed output. Source, font, image and guest budgets are unchanged.

A pinned local HarfRust 0.12.0 patch adds optional request limits, explicit
completion status, charged-operation/capacity statistics and bounded fallible
array growth. Checks precede resize, even when previously cached storage could
hold the request; actual spare capacity is reused without unnecessary growth.
Parley reserves scalar slots rather than UTF-8 bytes, applies the limits, shares
output/operation accounting across font/script runs and refuses before copying
incomplete/excessive results into layout. Refused work discards reusable glyph
arrays; successful work discards arrays exceeding the retained allowance. Context
cloning preserves configured policy. Varta checks status before measuring or
painting, so partial shaping cannot appear as a successful frame.

Acceptance includes exact 64-slot intermediate boundary and overflow, aggregate
129/128-glyph script-run boundary, measured operation exhaustion and later retry,
refusal/recovery after strong 16× OpenType expansion, unchanged maximum 65536-byte
ASCII input and full Unicode/password/IME/textarea regressions. The focused
selection passes **42 tests**. Observed capacity totals: the 16× refusal reached
**10485760 bytes** and retained zero slots; a valid 65536-glyph result used
**21846 charged operations**, **3495360 capacity bytes** and retained zero slots;
a later small request used **800 capacity bytes** and retained 20 slots per array.
These are measured array capacities, not allocator/RSS or total CPU measurements.

Source/archive checks confirm unchanged dependency versions and local changes
limited to two HarfRust and six Parley source files, with licenses/provenance and
complete diffs retained. Acceptance passes **188 native tests** (177 view, 11 desktop), the **42-test**
focused selection and **three policy tests** repeated after the context-clone
policy correction. Final strict all-target view/desktop Clippy, a fresh preview,
workspace/modified-vendor formatting, whitespace, archive/source comparison and
three-fixture deterministic regeneration checks pass. The
[verification record](history.md#bounded-native-shaping-requests--scoped-verification-2026-11-03)
keeps these scoped results distinct from the last full gate at `ab045dc`. Full
backend/cache/RSS accounting, reallocation overlap, parser/analysis/plan CPU,
real-window/platform matrices, late-value CSS and release acceptance remain open.


At the initial bounded-request checkpoint, the operation allowance was an
exhaustion threshold: batch debits could overshoot it before failure (44 charged
units versus an allowance of 43 in the three-run probe). The following package
addresses strict pre-charge accounting. Coverage of uncharged analysis/plan/
parser work remains separate from any claimed deadline.


### Strict native shaping pre-charge — implementation 2026-11-03

All existing HarfRust charged-work sites now reserve a complete batch through one
checked helper before its corresponding mutation, allocation, recursion or AAT
transition. Failed conversion/insufficient allowance marks the request failed
without debiting the unfunded batch; refusal is sticky. The sole counter
subtraction follows the checked affordability guard. Buffer masks/cluster merges
and forward shifts no longer perform their charged work after rejection; forward
shifts check before capacity growth. Contextual recursion charges before cursor
movement. Kern/kerx/morx drivers charge transitions before flags/actions and no
longer force advancement after exhaustion; current/marked insertion counts use
the same helper. Exact zero remaining permits successful completion when no
further charged work is requested. Configured limits are unchanged.

Acceptance requires the multi-script baseline to succeed with its exact observed
44-unit allowance and refuse at 43 without overspending. Five original AAT fonts
exercise current insertion (4 output glyphs / 6 charged units), marked+current
insertion (7 / 8), legacy kern and kerx (1 / 3 each) and a nonadvancing morx loop.
For each finite fixture every smaller allowance must refuse without exceeding
it, while the exact allowance reproduces the unrestricted metrics. The loop must
stop at budgets 1/2/8/64, release refused glyph arrays and permit a later ordinary
space render. Focused OpenType/resource (**6**) and AAT (**2**) checks pass; all five new fonts
regenerate identically. Acceptance passes **190 native tests** (179 view,
11 desktop), strict all-target view/desktop Clippy, a fresh preview, workspace/
modified-vendor formatting and whitespace checks. The
[verification record](history.md#strict-native-shaping-pre-charge--scoped-verification-2026-11-03)
records scopes and remaining resource limits separately from `ab045dc`'s full gate.

Source/archive comparison must show six changed HarfRust files and the existing
six Parley files only, with all other copied source/manifests/licenses matching
the pinned releases. This closes overspending of the existing charged meter;
uncharged parser/plan/Unicode/outline work, frame-wide accounting, allocator/RSS,
real-window/native/browser matrices, late-value CSS and release acceptance remain
open. It does not establish a complete CPU/deadline guarantee.


### Shared native view shaping scope — implementation 2026-11-03

A new native-only draft allowance shares **16777216 charged operations** across
one outer text view request. Rasterization includes intrinsic text, Taffy
remeasurement, fields/placeholders/selects and painting; scaled rasterization
uses the same entry. Geometry/box queries and a `commit_with_layout` batch also
establish an outer scope. Repeated layout commands in a batch reuse it instead
of resetting accounting per geometry pass. Existing per-layout, source, buffer,
glyph/outline, image and guest limits are unchanged.

The text adapter sets a temporary additional Parley operation ceiling from the
remaining request allowance and deducts actual charged work, including failed
layouts. Nested scopes do not replenish it. An owned guard clears scope state
on success, ordinary error and unwinding; pointer-sized atomics retain the prior
thread traits without requiring an additional 64-bit atomic capability. The new
`last_view_shaping_operations()` diagnostic reports the last completed outer
scope using that measurer. Standalone measurement clears the temporary ceiling
and keeps its own profile; it does not overwrite the view-request diagnostic.

Acceptance requires exact cumulative frame and geometry boundaries, geometry
success followed by paint refusal under the same allowance, scaled-frame parity,
multiple layout-command accounting and full model/deferred-effect rollback.
Nested calls and caught refusals cannot reset the allowance; a later independent
request must succeed. Scope state must also clear during a deliberate unwind
after completed shaping. This tests counter cleanup, not all dependency-cache
unwind safety. Four focused checks pass: the fixture uses **16 operations** for
geometry, **24** for the frame and **32** for two layout commands; a refused batch
preserves its prior snapshot and performs no deferred clipboard action, while
the funded retry performs it once. Acceptance passes **194 native tests**
(183 view, 11 desktop), strict all-target view/desktop Clippy, a fresh preview,
workspace/modified-vendor formatting, whitespace and archive/source checks. The
[verification record](history.md#shared-native-view-shaping-scope--scoped-verification-2026-11-03)
records the final scoped results separately from the consolidated commit gate.

All scope probes use actual view APIs, AAT shaping and Taffy remeasurement with
smaller injected allowances to verify boundaries without an expensive default-
limit stress loop. The default outer allowance is exercised by normal requests.
The meter still omits some parser/plan/Unicode/coverage/outline work and does not
bound total frame CPU, allocator overlap, other retained structures or RSS.
Complete resource/platform matrices, late-value CSS and release acceptance remain
open.

### Native layout measurement failure — verified 2026-11-03

After a text measurement fails, Taffy's callback cannot return the error and its
traversal continues. The callback now retains the first diagnostic and uses only
intrinsic fallback sizes for the remaining measurements in that pass. It builds
no further text requests. The entire geometry/image is rejected, and failed
layout-command transactions retain their prior model and deferred effects.
Independent funded requests still render normally. Source, shaping, glyph,
measurement-work and guest allowances are unchanged; no dependency patch or
public ABI changed.

An eight-textarea AAT regression reproduced **16 shaping builds instead of the
necessary 9** before the fix. Geometry, boxes, raster and scaled-raster requests
now stop at nine: eight successful intrinsic builds and one refused width-bound
build. They preserve the text error and snapshot, and independent retries reproduce
the prior pixels. A two-command batch fails at its first layout command without
changing its title, commit count or clipboard; its funded retry writes once.

The focused **6-test** shaping-scope selection and **196 native tests** (185 view,
11 desktop) pass, along with strict all-target view/desktop Clippy, a fresh preview
build, formatting and whitespace checks. This is scoped evidence for the working
tree after `1111baa`; the last full gate remains that commit's checkpoint. See the
[verification record](history.md#native-layout-measurement-failure--scoped-verification-2026-11-03).
Taffy's remaining traversal, unmetered parser/plan/Unicode/coverage/outline work,
total CPU/RSS, real-window/platform matrices, full L0 styling/accessibility,
production distribution and release acceptance remain open.

### Native Unicode text nodes — verified 2026-11-03

The unchanged console view artifact reproduced native layout refusal on node
44's send-button text `↑`. Native text-node admission now accepts Unicode through
the existing explicit-font shaper instead of an ASCII/Cyrillic alphabet list.
Symbols, supplementary scalars, combining marks, mixed bidirectional runs and
Unicode line breaking keep their complete shaping context. The general 64 KiB
UTF-8 shaping allowance, aggregate work/output limits, control-character checks
and existing `pre-wrap` restrictions remain unchanged. No language profile,
host ABI or dependency version changed.

Three focused tests cover original-font glyph aliases and visual run order,
combining ink, CJK wrapping, normal/nowrap/pre-wrap/nonbreaking whitespace,
control refusal, unchanged source and UTF-8-limit refusal/recovery. The old
isolated-emoji rejection test now checks explicit-font `.notdef` geometry and
retains a control-character negative case. An independent real Chrome probe
passes **81 checks across 33 block/flex/grid cases**, including browser shaping
of the glyph-order oracle. This does not compare rasterizer pixels across engines
or establish complete natural-script typography/inline formatting.

**199 native tests (188 view, 11 desktop)** pass across valid reused scopes and
the corrected continuation, with strict all-target view/desktop Clippy, fresh
preview/headless-example builds, formatting and whitespace checks. A test-only
iterator lint correction was followed by the three focused tests. The original
font regenerates byte-for-byte. The generic `headless_frame` example runs an
actual Wasm/manifest pair and exports a bounded initial snapshot and CPU image;
its successful Unicode artifact, argument/font refusal paths and console failure
were checked. It does not drive later interactive turns. See the
[verification record](history.md#native-unicode-text-nodes--scoped-verification-2026-11-03).

With the same console artifact and identical initial snapshots, rendering now
reaches node 2's `max-width: 48rem` limitation. **Next native delivery work is
relative-length resolution**, with an explicit reference-font contract and
browser/native geometry checks across affected layout/text/paint properties;
converting just that one console declaration is not the acceptance criterion.
The console's complete native frame/session, other L0 styling and inline layout,
actual macOS/Pi target acceptance, accessibility, performance/resource measurements
and release work remain open. The last full gate remains `1111baa`.

### Native rem reference lengths — verified 2026-11-03

The shared L0 grammar already admits rem lengths; native geometry/text/paint now
resolve them against an explicit host document-root font reference. It defaults
to the initial font size and is independent of the view root's authored font,
the shaper's default text size and the media-query initial-font reference. A real
Chrome shadow-tree probe confirmed that a document root at 20px and view root at
40px still produce a 40px-wide `2rem` box. `Environment::root_font_size` and its
checked builder expose that reference to native hosts; `ViewLayout` can apply it
before the first guest turn. Existing Rust struct literals retain defaults with
`..Default::default()`. No wire field, guest operation or host ABI was added.

One bounded converter handles dimensions/min/max, padding/margins, gaps, flex
basis, signed positioned edges, nested grid tracks, font size/absolute line
height, borders and outlines. Both coefficients and resolved values retain the
existing numeric ceilings; text/shaping/paint budgets are unchanged. Internal
viewport and DPI contexts preserve the reference. Raw styles remain authored
units. Invalid host metrics reject without updating the environment; zero is
valid for geometry, while the existing text-size minimum still applies. Layout
failure retains the transaction and makes no deferred clipboard write.

The first three regressions failed before implementation. All **6 focused length
tests**, **3 runtime native-layout tests**, and **205 native tests (194 view,
11 desktop)** pass. The six-case selection was repeated after strengthening the
inherited-font fixture so rem line height cannot be mistaken for a multiplier.
Real Chrome passes **168 checks across 18 cases** at reference sizes 16/20/24px,
including distinct view/media font meanings. Strict all-target view/desktop/
runtime Clippy, fresh preview/headless-example builds, formatting and whitespace
checks pass. See the [verification record](history.md#native-rem-reference-lengths--scoped-verification-2026-11-03).
These are scoped results for newer code; the last full gate remains `1111baa`.

The unchanged console artifact and identical initial snapshots now reach node
11, `span.author-name`, beyond the previous node-2 `48rem` refusal. **Next native
work is semantic/inline container layout**, including proper inline flow rather
than globally replacing inline elements with blocks. Full console frame/session,
remaining styling/typography, OS accessibility, macOS/Pi/other-target acceptance,
resource/performance measurements and release validation remain open.

### Native inline formatting — verified 2026-11-03

Inline-only contexts now share one paragraph across adjacent text and nested
span/strong/em/code/link/br descendants. Flex/grid item blockification remains a
parent layout rule. Text normalization preserves cross-node whitespace and
shaping context; parent/ancestor struts, own-font fragment heights and empty
anchors determine line geometry. Original ligature/combining/Arabic fixtures and
real Chrome establish cluster ownership: the first source node owns its advance
and ink, while continuation nodes retain zero-width boxes. Painting reuses the
final shaped layout; backgrounds and hits use clipped fragments, including bare
text overflow and vertical scrolling. Snapshot/model state remains unchanged by
rendering. Explicit unsupported styles still fail closed.

An independent native request allowance bounds inline collection/path copying,
style-span work, line/cluster/glyph projection and fragment assignment. Existing
node/depth/text/shaping/paint ceilings are retained; nested layout calls cannot
refill the allowance. Failed layout commands roll back model changes and deferred
clipboard effects; independent retries succeed. No guest ABI, wire format,
dependency or old production budget was changed.

**11 inline tests** and the real layout-command work-scope regression pass.
The standards-mode real Chrome oracle passes **2724 checks over 13 fixtures**:
geometry, interior hit targets and selected screenshot pixels, with identical
original font bytes. It covers wrapping/nowrap, glyph context, nested metrics,
empty anchors, trailing breaks and clipping. Native tests separately cover
scroll movement, DPI, semantic font faces, fragment gaps and unsupported-style
refusal. Original inline/Unicode fonts regenerate byte-for-byte. Fresh native
preview/headless builds and browser/generator syntax checks pass.

The unchanged console Wasm has the same initial snapshots and advances from
`Layout(Unsupported(11))` at the author span to `Layout(Unsupported(16))` at the
styled thinking button. Full button/text alignment, remaining native styling,
mixed block/inline formatting, nonzero inline edges/decoration, platform fonts,
accessibility, real target sessions, CPU/RSS and release acceptance remain open.
This is a bounded native increment, not full browser or production readiness.

The consolidated full gate covers this increment and the preceding measurement,
Unicode and rem work: **3174 passed, 0 failed, 16 ignored**, all eight commands,
strict workspace Clippy/all-target build and unchanged source fingerprint.
Conformance: **123 passed, 4 known unsupported / 127**, baseline matched, no
regressions. See the current status and
[verification record](history.md#native-text-batch--full-verification-2026-11-03).
